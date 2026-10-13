use serde_json::{Value, json};
use ww2_core::{
    weather::{WeatherKind, WeatherPlan},
    weather_runtime::{self, Environment, Showers, Smoke, WeatherRuntimeError},
};
fn number(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}
fn kind(v: &Value) -> Option<WeatherKind> {
    v.as_str().map(|s| WeatherKind::try_from(s).unwrap())
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/weather-runtime.json")).unwrap()
}
#[derive(Clone, Debug, PartialEq)]
struct Cloud {
    id: u32,
    x: f64,
    z: f64,
    time: f64,
}
#[derive(Clone, Debug, PartialEq)]
struct Rain {
    raining: bool,
    next: f64,
    rain: f64,
    wet: f64,
}
#[derive(Clone, Debug, PartialEq)]
struct State {
    seed: u32,
    angle: f64,
    strength: f64,
    rain: Option<Rain>,
    clouds: Vec<Cloud>,
}
impl State {
    fn from(v: &Value) -> Self {
        Self {
            seed: v["seed"].as_u64().unwrap() as u32,
            angle: number(&v["wind"]["a"]),
            strength: number(&v["wind"]["v"]),
            rain: (!v["wx"].is_null()).then(|| Rain {
                raining: v["wx"]["raining"].as_bool().unwrap(),
                next: number(&v["wx"]["next"]),
                rain: number(&v["wx"]["rain"]),
                wet: number(&v["wx"]["wet"]),
            }),
            clouds: v["smokes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| Cloud {
                    id: s["id"].as_u64().unwrap() as u32,
                    x: number(&s["x"]),
                    z: number(&s["z"]),
                    time: number(&s["t"]),
                })
                .collect(),
        }
    }
    fn step(
        &mut self,
        tick: u64,
        dt: f64,
        kind: Option<WeatherKind>,
    ) -> Result<weather_runtime::PhaseDue, WeatherRuntimeError> {
        let mut clouds: Vec<_> = self
            .clouds
            .iter_mut()
            .map(|c| Smoke {
                x: &mut c.x,
                z: &mut c.z,
                remaining: &mut c.time,
            })
            .collect();
        weather_runtime::advance(
            tick,
            dt,
            kind,
            Environment {
                seed: &mut self.seed,
                wind_angle: &mut self.angle,
                wind_strength: &mut self.strength,
                showers: self.rain.as_mut().map(|r| Showers {
                    raining: &mut r.raining,
                    next: &mut r.next,
                    rain: &mut r.rain,
                    wet: &mut r.wet,
                }),
            },
            &mut clouds,
        )
    }
    fn snapshot(&self, plan: Option<WeatherPlan>) -> Value {
        json!({"seed":self.seed,"wind":[bits(self.angle),bits(self.strength)],"wx":self.rain.as_ref().map(|r|json!([r.raining,bits(r.next),bits(r.rain),bits(r.wet)])),"plan":plan.map(|p|json!({"now":p.now.name(),"next":p.next.map(WeatherKind::name),"at":p.at})),"smoke":self.clouds.iter().map(|c|json!([c.id,bits(c.x),bits(c.z),bits(c.time)])).collect::<Vec<_>>()})
    }
}
#[test]
fn live_weather_matches_original_steps_and_order() {
    let f = fixture();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    let mut steps = 0;
    for case in f["cases"].as_array().unwrap() {
        let input = &case["input"];
        let mut state = State::from(input);
        let mut plan = kind(&input["weather"]["now"]).map(|now| WeatherPlan {
            now,
            next: kind(&input["weather"]["next"]),
            at: input["weather"]["at"].as_u64().unwrap() as u32,
        });
        for (action, expected) in case["steps"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["expected"].as_array().unwrap())
        {
            let tick = action[0].as_u64().unwrap();
            let dt = number(&action[1]);
            if let Some(p) = plan.as_mut() {
                p.step(tick);
            }
            let due = state.step(tick, dt, plan.map(|p| p.now)).unwrap();
            assert_eq!(
                state.snapshot(plan),
                *expected,
                "{} tick {tick}",
                case["name"]
            );
            assert_eq!(due.burn_dt, tick.is_multiple_of(10).then_some(dt * 10.0));
            assert_eq!(due.flood, tick.is_multiple_of(10));
            steps += 1;
        }
    }
    assert_eq!(steps, 386);
}
#[test]
fn environment_random_matches_original_and_resumes() {
    let f = fixture();
    let mut count = 0;
    for case in f["rng"].as_array().unwrap() {
        let mut seed = case["seed"].as_u64().unwrap() as u32;
        for row in case["draws"].as_array().unwrap() {
            assert_eq!(u64::from(seed), row[0].as_u64().unwrap());
            assert_eq!(bits(weather_runtime::environment_random(&mut seed)), row[1]);
            assert_eq!(u64::from(seed), row[2].as_u64().unwrap());
            count += 1;
        }
    }
    assert_eq!(count, 400);
}
#[test]
fn invalid_input_is_atomic_and_expired_smoke_still_drifts() {
    let mut state = State::from(&fixture()["cases"][0]["input"]);
    let before = state.clone();
    for dt in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(state.step(10, dt, Some(WeatherKind::Clear)).is_err());
        assert_eq!(state, before);
    }
    assert!(state.step(10, f64::MAX, Some(WeatherKind::Clear)).is_err());
    assert_eq!(state, before);
    state.clouds[1].time = f64::INFINITY;
    let saved_seed = state.seed;
    let saved_angle = state.angle;
    assert!(state.step(10, 0.05, Some(WeatherKind::Clear)).is_err());
    assert_eq!(state.seed, saved_seed);
    assert_eq!(state.angle, saved_angle);
    state = before;
    let old = state.clouds[0].clone();
    state.step(1, 0.05, Some(WeatherKind::Clear)).unwrap();
    assert!(state.clouds[0].time < 0.0);
    assert_ne!(state.clouds[0].x, old.x);
    assert_eq!(state.clouds[0].id, old.id);
}
