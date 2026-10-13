//! Off-map support on canonical Conquest wallets, cooldowns and terrain.
//! Plane interception, paratrooper spawning and recon visibility are separate actuators.

use crate::{
    catalog::{Catalog, Value},
    economy::{self, Mode},
    environment,
    game::{CELL, Point, Smoke},
    match_combat::{DamageWorld, ExplosionSource},
    match_runtime::Match,
    match_story::{StoryChange, StoryKey, prepare_story_updates},
    math,
    random::StreamId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportKind {
    Artillery,
    Smoke,
    Cover,
}
impl SupportKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Artillery => "artillery",
            Self::Smoke => "smoke",
            Self::Cover => "cover",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportDenial {
    Blocked,
    Cooldown,
    Manpower,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupportError {
    Unsupported(&'static str),
    Invalid(&'static str),
    Actuation(String),
}
impl std::fmt::Display for SupportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(s) | Self::Invalid(s) => f.write_str(s),
            Self::Actuation(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for SupportError {}
#[derive(Clone, Debug, PartialEq)]
pub struct SupportStrike {
    pub kind: SupportKind,
    pub owner: usize,
    pub at: Point,
    pub direction: f64,
    pub delay: f64,
    pub left: f64,
    pub next: f64,
    pub live: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FighterCover {
    pub team: i32,
    pub owner: usize,
    pub at: Point,
    pub radius: f64,
    pub remaining: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SupportEvent {
    Shell(Point),
    SmokeShells(Point),
    Environment(environment::Effect),
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SupportState {
    pub strikes: Vec<SupportStrike>,
    pub covers: Vec<FighterCover>,
    pub events: Vec<SupportEvent>,
}
impl SupportState {
    pub fn take_events(&mut self) -> Vec<SupportEvent> {
        std::mem::take(&mut self.events)
    }
}
fn number(v: &Value, key: &str) -> Result<f64, SupportError> {
    v.get(key)
        .and_then(Value::number)
        .filter(|n| n.is_finite())
        .ok_or(SupportError::Invalid("invalid compiled support rule"))
}
fn catalog() -> Result<&'static Catalog, SupportError> {
    Catalog::compiled().map_err(|e| SupportError::Actuation(e.to_string()))
}
fn definition(kind: SupportKind) -> Result<&'static Value, SupportError> {
    catalog()?
        .support(kind.name())
        .map_err(|e| SupportError::Actuation(e.to_string()))
}
fn cfg() -> Result<&'static Value, SupportError> {
    catalog()?
        .table("CFG")
        .map_err(|e| SupportError::Actuation(e.to_string()))
}
fn strip(s: &SupportStrike, along: f64, side: f64) -> Point {
    Point {
        x: s.at.x + math::cos(s.direction) * along - math::sin(s.direction) * side,
        z: s.at.z + math::sin(s.direction) * along + math::cos(s.direction) * side,
    }
}
impl Match {
    pub(crate) fn validate_support(&self) -> Result<(), SupportError> {
        if !matches!(self.options.mode, Mode::Default | Mode::Conquest)
            || self.world.game.mode.is_some()
        {
            return Err(SupportError::Unsupported(
                "support requires ordinary Conquest mode",
            ));
        }
        let n = self.world.game.players.len();
        if self.start.players.len() != n
            || self.start.economy.players.len() != n
            || self.story.len() != n
        {
            return Err(SupportError::Invalid("support player ownership mismatch"));
        }
        if self.support.strikes.len() > 4096
            || self.support.covers.len() > 4096
            || self
                .start
                .players
                .iter()
                .any(|p| p.support.values().any(|v| !v.is_finite()))
            || self.start.economy.players.iter().any(|p| !p.mp.is_finite())
            || self.support.strikes.iter().any(|s| {
                s.owner >= n
                    || ![s.at.x, s.at.z, s.direction, s.delay, s.left, s.next]
                        .iter()
                        .all(|v| v.is_finite())
                    || s.left < 0.0
            })
            || self.support.covers.iter().any(|c| {
                c.owner >= n
                    || ![c.at.x, c.at.z, c.radius, c.remaining]
                        .iter()
                        .all(|v| v.is_finite())
                    || c.radius < 0.0
            })
        {
            return Err(SupportError::Invalid("invalid canonical support state"));
        }
        Ok(())
    }
    /// Admit one recorded command. The caller's existing command queue owns ordering.
    /// Unsupported plane kinds return before any payment or cooldown change.
    pub fn admit_support(
        &mut self,
        owner: usize,
        name: &str,
        at: Point,
        direction: Option<f64>,
    ) -> Result<Option<SupportDenial>, SupportError> {
        self.validate_support()?;
        let Some(player) = self.start.economy.players.get(owner) else {
            return Ok(Some(SupportDenial::Blocked));
        };
        if self.finish.is_some()
            || player.out
            || player.away
            || !at.x.is_finite()
            || !at.z.is_finite()
        {
            return Ok(Some(SupportDenial::Blocked));
        }
        let kind = match name {
            "artillery" => SupportKind::Artillery,
            "smoke" => SupportKind::Smoke,
            "cover" => SupportKind::Cover,
            "recon" | "strafe" | "bombing" | "dive" | "para" => {
                return Err(SupportError::Unsupported(
                    "support plane actuator is unavailable",
                ));
            }
            _ => return Ok(Some(SupportDenial::Blocked)),
        };
        let def = definition(kind)?;
        let cooldown = *self.start.players[owner]
            .support
            .get(name)
            .ok_or(SupportError::Invalid("missing canonical support cooldown"))?;
        if cooldown > 0.0 {
            return Ok(Some(SupportDenial::Cooldown));
        }
        let cost = economy::sup_cost(Mode::Conquest, name)
            .map_err(|e| SupportError::Actuation(e.to_string()))?
            .cost;
        if player.mp < cost {
            return Ok(Some(SupportDenial::Manpower));
        }
        if self.support.strikes.len() == 4096 {
            return Err(SupportError::Invalid("support strike capacity exhausted"));
        }
        let at = Point {
            x: math::min(self.world.game.w as f64 * CELL, math::max(0.0, at.x)),
            z: math::min(self.world.game.h as f64 * CELL, math::max(0.0, at.z)),
        };
        let direction = direction.filter(|d| d.is_finite()).unwrap_or_else(|| {
            math::atan2(
                at.z - self.start.players[owner].spawn.z,
                at.x - self.start.players[owner].spawn.x,
            )
        });
        let shells = if kind == SupportKind::Artillery {
            let doctrine = catalog()?
                .table("DOCTRINE")
                .map_err(|e| SupportError::Actuation(e.to_string()))?;
            let multiplier = doctrine
                .get(&(self.start.players[owner].config.faction as u8).to_string())
                .and_then(|v| v.get("shells"))
                .and_then(|v| v.get(name))
                .and_then(Value::number)
                .unwrap_or(1.0);
            math::round(number(def, "shells")? * multiplier)
        } else {
            def.get("dur").and_then(Value::number).unwrap_or(0.0)
        };
        let strike = SupportStrike {
            kind,
            owner,
            at,
            direction,
            delay: number(def, "delay")?,
            left: shells,
            next: 0.0,
            live: false,
        };
        let new_mp = player.mp - cost;
        let cd = number(def, "cd")?;
        let story = prepare_story_updates(
            &mut self.story,
            self.finish.is_some(),
            &[
                StoryChange {
                    slot: Some(owner),
                    key: StoryKey::SupportCalls,
                    amount: 1.0,
                },
                StoryChange {
                    slot: Some(owner),
                    key: StoryKey::MpSpent,
                    amount: cost,
                },
            ],
        )
        .map_err(SupportError::Actuation)?;
        self.start.economy.players[owner].mp = new_mp;
        self.start.players[owner].support.insert(name.into(), cd);
        self.support.strikes.push(strike);
        story.commit();
        Ok(None)
    }
    /// Run the source strike phase using the tick's captured late unit membership.
    /// Phase failures can follow committed earlier effects; the tick owner poisons failed ticks.
    pub fn step_support_strikes(
        &mut self,
        unit_order: &[u32],
        dt: f64,
    ) -> Result<(), SupportError> {
        self.validate_support()?;
        if !dt.is_finite() || dt < 0.0 || unit_order.windows(2).any(|v| v[0] >= v[1]) {
            return Err(SupportError::Invalid("invalid support step"));
        }
        if unit_order
            .iter()
            .any(|id| !self.world.game.units.contains_key(id))
        {
            return Err(SupportError::Unsupported(
                "captured support actor has been removed from canonical state",
            ));
        }
        let smoke_count = self
            .support
            .strikes
            .iter()
            .filter(|s| s.kind == SupportKind::Smoke && s.delay - dt <= 0.0)
            .count()
            * 5;
        self.next_unit_id
            .checked_add(
                u32::try_from(smoke_count)
                    .map_err(|_| SupportError::Invalid("smoke identity exhausted"))?,
            )
            .ok_or(SupportError::Invalid("smoke identity exhausted"))?;
        let mut strikes = std::mem::take(&mut self.support.strikes);
        let result = (|| {
            for s in &mut strikes {
                let def = definition(s.kind)?;
                s.delay -= dt;
                if s.delay > 0.0 {
                    continue;
                }
                if !s.live {
                    s.live = true;
                    if s.kind == SupportKind::Cover {
                        if self.support.covers.len() == 4096 {
                            return Err(SupportError::Invalid("cover capacity exhausted"));
                        }
                        self.support.covers.push(FighterCover {
                            team: self.world.game.players[s.owner].team,
                            owner: s.owner,
                            at: s.at,
                            radius: number(def, "radius")?,
                            remaining: number(def, "dur")?,
                        });
                        s.left = 0.0;
                    }
                }
                if s.kind == SupportKind::Smoke {
                    let clouds = number(def, "clouds")? as u32;
                    for i in 0..clouds {
                        let at = strip(
                            s,
                            (f64::from(i) / f64::from(clouds - 1) - 0.5)
                                * (number(def, "len")? - number(def, "cloud")?),
                            (self.start.random.next_fraction(StreamId::OffMap) - 0.5) * 2.0,
                        );
                        self.world.game.smokes.push(Smoke {
                            id: self.next_unit_id,
                            remaining: Some(number(def, "dur")?),
                            x: at.x,
                            z: at.z,
                            r: number(def, "cloud")?,
                        });
                        self.next_unit_id += 1;
                    }
                    self.support.events.push(SupportEvent::SmokeShells(s.at));
                    s.left = 0.0;
                } else if s.kind == SupportKind::Artillery {
                    s.next -= dt;
                    if s.next > 0.0 {
                        continue;
                    }
                    let along = (self.start.random.next_fraction(StreamId::OffMap) - 0.5)
                        * number(def, "len")?;
                    let side = (self.start.random.next_fraction(StreamId::OffMap) - 0.5)
                        * number(def, "width")?;
                    let at = strip(s, along, side);
                    self.support.events.push(SupportEvent::Shell(at));
                    let (wind_angle, wind_strength) = self.start.wind;
                    let current_order: Vec<_> = self.world.game.units.keys().copied().collect();
                    let mut damage = DamageWorld {
                        world: &mut self.world,
                        combat: &mut self.combat,
                        grid: &mut self.grid,
                        rules: &self.rules,
                        logistics: &self.logistics,
                        production: &self.production,
                        mode: self.options.mode,
                        tech_enabled: self.options.tech,
                        environment: &mut self.environment,
                        environment_seed: &mut self.start.environment_seed,
                        next_id: &mut self.next_unit_id,
                        unit_order: &current_order,
                        wind_angle,
                        wind_strength,
                    };
                    let effects = damage
                        .explosion_membership(
                            at,
                            number(def, "blast")?,
                            ExplosionSource::support(s.kind.name())
                                .map_err(|e| SupportError::Actuation(e.to_string()))?,
                            s.owner as i32,
                            None,
                            None,
                            None,
                            unit_order,
                            &mut self.firing,
                        )
                        .map_err(|e| SupportError::Actuation(e.to_string()))?;
                    self.support
                        .events
                        .extend(effects.into_iter().map(SupportEvent::Environment));
                    // Artillery dig=0 still dents its landing cell and marks open ground as cratered.
                    if let Some(c) = self.world.game.cell_of(at.x, at.z) {
                        let g = &self.world.game;
                        let old = g.height.as_ref().map_or(0, |h| h[c]);
                        let lower = i16::from(old) - 1;
                        let x = c % g.w;
                        let neighbors = [
                            c.checked_sub(g.w),
                            c.checked_add(g.w).filter(|n| *n < g.w * g.h),
                            (x > 0).then(|| c - 1),
                            (x + 1 < g.w).then(|| c + 1),
                        ];
                        if f64::from(lower) >= number(cfg()?, "minLevel")?
                            && !neighbors.into_iter().flatten().any(|n| {
                                i16::from(g.height.as_ref().map_or(0, |h| h[n])) - lower > 1
                            })
                        {
                            self.world
                                .set_level(c, lower as i8)
                                .map_err(|e| SupportError::Actuation(e.to_string()))?;
                        }
                        if self.world.game.chars[c] == b'.' {
                            self.world
                                .set_cell(c, b'+')
                                .map_err(|e| SupportError::Actuation(e.to_string()))?;
                        }
                    }
                    s.left -= 1.0;
                    s.next = number(def, "every")?;
                }
            }
            Ok(())
        })();
        if result.is_ok() {
            strikes.retain(|s| !s.live || s.left > 0.0);
        }
        self.support.strikes = strikes;
        result
    }
    /// Covers and cooldowns decay after strike activation and anti-air, in source order.
    pub fn advance_support_timers(&mut self, dt: f64) -> Result<(), SupportError> {
        self.validate_support()?;
        if !dt.is_finite() || dt < 0.0 {
            return Err(SupportError::Invalid("invalid support timer step"));
        }
        let radio = number(cfg()?, "radioCd")?;
        let mut values = Vec::new();
        for (slot, p) in self.start.players.iter().enumerate() {
            let accelerated = self.start.points.iter().any(|q| {
                q.kind.as_deref() == Some("radio")
                    && !q.cut
                    && q.owner
                        .is_some_and(|owner| self.world.game.allied(owner, slot))
            });
            let factor = if accelerated { radio } else { 1.0 };
            for (kind, current) in &p.support {
                let next = current - dt * factor;
                if !next.is_finite() {
                    return Err(SupportError::Invalid("support timer overflow"));
                }
                values.push((slot, kind.clone(), next));
            }
        }
        for c in &mut self.support.covers {
            c.remaining -= dt;
        }
        self.support.covers.retain(|c| c.remaining > 0.0);
        for (slot, kind, next) in values {
            self.start.players[slot].support.insert(kind, next);
        }
        Ok(())
    }
}
