//! Live weather over caller-owned state. Run the plan transition early in the tick,
//! then `advance` late, fire/flood work next, and expired-smoke removal last.
use crate::{
    catalog::{Catalog, CatalogError, Value},
    math,
    weather::WeatherKind,
};
use std::sync::OnceLock;

/// Shared environment Mulberry32. Startup, wind, showers and fire borrow one seed.
/// The recorded weather-plan seed is separate.
pub fn environment_random(seed: &mut u32) -> f64 {
    *seed = seed.wrapping_add(0x6d2b79f5);
    let mut t = (*seed ^ (*seed >> 15)).wrapping_mul(*seed | 1);
    t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
    f64::from(t ^ (t >> 14)) / 4_294_967_296.0
}
/// Existing live shower and terrain-wetness values, borrowed without copies.
pub struct Showers<'a> {
    pub raining: &'a mut bool,
    pub next: &'a mut f64,
    pub rain: &'a mut f64,
    pub wet: &'a mut f64,
}
/// The environment seed exists even when showers are disabled.
pub struct Environment<'a> {
    pub seed: &'a mut u32,
    pub wind_angle: &'a mut f64,
    pub wind_strength: &'a mut f64,
    pub showers: Option<Showers<'a>>,
}
/// Borrows an existing cloud without replacing its identity or other fields.
pub struct Smoke<'a> {
    pub x: &'a mut f64,
    pub z: &'a mut f64,
    pub remaining: &'a mut f64,
}
/// Timing gates only. The world still checks for fires or flood candidates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhaseDue {
    pub burn_dt: Option<f64>,
    pub flood: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WeatherRuntimeError {
    Catalog(CatalogError),
    InvalidInput(&'static str),
}
impl From<CatalogError> for WeatherRuntimeError {
    fn from(value: CatalogError) -> Self {
        Self::Catalog(value)
    }
}
struct Rules {
    wind_speed: f64,
    rain: [f64; 2],
    dry: [f64; 2],
    soak: f64,
    dry_out: f64,
}
impl Rules {
    fn compiled() -> Result<&'static Self, WeatherRuntimeError> {
        static RULES: OnceLock<Result<Rules, WeatherRuntimeError>> = OnceLock::new();
        RULES
            .get_or_init(|| {
                let cfg = Catalog::compiled()?.table("CFG")?;
                let get = |value: &Value, key| {
                    value
                        .get(key)
                        .and_then(Value::number)
                        .filter(|n| n.is_finite() && *n > 0.0)
                        .ok_or_else(|| CatalogError(format!("invalid live weather rule {key}")))
                };
                let weather = cfg
                    .get("weather")
                    .ok_or_else(|| CatalogError("missing weather rules".into()))?;
                let pair = |key| -> Result<[f64; 2], CatalogError> {
                    let row = weather
                        .get(key)
                        .and_then(Value::array)
                        .filter(|v| v.len() == 2)
                        .ok_or_else(|| CatalogError(format!("invalid live weather range {key}")))?;
                    let a = row[0].number().filter(|n| n.is_finite() && *n >= 0.0);
                    let b = row[1].number().filter(|n| n.is_finite() && *n >= 0.0);
                    match (a, b) {
                        (Some(a), Some(b)) if a <= b => Ok([a, b]),
                        _ => Err(CatalogError(format!("invalid live weather range {key}"))),
                    }
                };
                Ok(Self {
                    wind_speed: get(cfg, "windSpeed")?,
                    rain: pair("rain")?,
                    dry: pair("dry")?,
                    soak: get(weather, "soak")?,
                    dry_out: get(weather, "dryOut")?,
                })
            })
            .as_ref()
            .map_err(Clone::clone)
    }
}
fn finite(value: f64, field: &'static str) -> Result<(), WeatherRuntimeError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(WeatherRuntimeError::InvalidInput(field))
    }
}
/// Smoke lifetime decay followed by the original weather body. Checks finish
/// before any caller-owned state changes. Zero dt still consumes wind draws and
/// can toggle an expired shower. Long dt toggles at most one shower.
pub fn advance(
    tick: u64,
    dt: f64,
    kind: Option<WeatherKind>,
    mut environment: Environment<'_>,
    smokes: &mut [Smoke<'_>],
) -> Result<PhaseDue, WeatherRuntimeError> {
    finite(dt, "dt")?;
    if dt < 0.0 {
        return Err(WeatherRuntimeError::InvalidInput("dt"));
    }
    finite(*environment.wind_angle, "wind angle")?;
    finite(*environment.wind_strength, "wind strength")?;
    let mut shower = environment
        .showers
        .as_ref()
        .map(|w| (*w.raining, *w.next, *w.rain, *w.wet));
    if let Some((_, next, rain, wet)) = shower {
        finite(next, "next shower")?;
        finite(rain, "rain")?;
        finite(wet, "wetness")?;
    }
    for s in smokes.iter() {
        finite(*s.x, "smoke x")?;
        finite(*s.z, "smoke z")?;
        finite(*s.remaining, "smoke time")?;
    }
    let rules = Rules::compiled()?;
    let decay = dt * (1.0 + 0.5 * shower.map_or(0.0, |w| w.2));
    finite(decay, "smoke decay")?;
    let mut seed = *environment.seed;
    let angle = *environment.wind_angle + (environment_random(&mut seed) - 0.5) * 0.6 * dt;
    let strength = math::min(
        1.0,
        math::max(
            0.15,
            *environment.wind_strength + (environment_random(&mut seed) - 0.5) * 0.3 * dt,
        ),
    );
    finite(angle, "wind angle result")?;
    finite(strength, "wind strength result")?;
    let dx = math::cos(angle) * strength * rules.wind_speed * dt;
    let dz = math::sin(angle) * strength * rules.wind_speed * dt;
    let mut clouds = Vec::with_capacity(smokes.len());
    for s in smokes.iter() {
        let next = (*s.x + dx, *s.z + dz, *s.remaining - decay);
        finite(next.0, "smoke x result")?;
        finite(next.1, "smoke z result")?;
        finite(next.2, "smoke time result")?;
        clouds.push(next);
    }
    if let Some((raining, next, rain, wet)) = shower.as_mut() {
        if kind == Some(WeatherKind::Rain) {
            *raining = true;
            *next = 0.0;
        } else if kind == Some(WeatherKind::Snow) {
            *raining = false;
        } else {
            *next -= dt;
            if *next <= 0.0 {
                *raining = !*raining;
                let [lo, hi] = if *raining { rules.rain } else { rules.dry };
                *next = lo + environment_random(&mut seed) * (hi - lo);
            }
        }
        *rain = math::min(
            1.0,
            math::max(0.0, *rain + (if *raining { dt } else { -dt }) / 20.0),
        );
        *wet = if kind == Some(WeatherKind::Mud) {
            1.0
        } else {
            math::min(
                1.0,
                math::max(
                    0.0,
                    *wet + if *rain > 0.5 {
                        dt / rules.soak
                    } else {
                        -dt / rules.dry_out
                    },
                ),
            )
        };
        finite(*next, "next shower result")?;
        finite(*rain, "rain result")?;
        finite(*wet, "wetness result")?;
    }
    let due = tick.is_multiple_of(10);
    let burn_dt = if due {
        let t = dt * 10.0;
        finite(t, "burn dt")?;
        Some(t)
    } else {
        None
    };
    *environment.seed = seed;
    *environment.wind_angle = angle;
    *environment.wind_strength = strength;
    if let Some(out) = environment.showers.as_mut() {
        let (raining, next, rain, wet) = shower.expect("borrowed shower state");
        *out.raining = raining;
        *out.next = next;
        *out.rain = rain;
        *out.wet = wet;
    }
    for (s, (x, z, time)) in smokes.iter_mut().zip(clouds) {
        *s.x = x;
        *s.z = z;
        *s.remaining = time;
    }
    Ok(PhaseDue {
        burn_dt,
        flood: due,
    })
}
