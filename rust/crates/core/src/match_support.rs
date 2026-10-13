//! Off-map support on canonical Conquest wallets, cooldowns and terrain.
//! Plane interception, paratrooper spawning and recon visibility are separate actuators.

use crate::{
    catalog::{Catalog, Value},
    economy::{self, Mode},
    environment,
    game::{CELL, Point, Smoke, Strike, StrikeKind},
    match_combat::{DamageWorld, ExplosionSource},
    match_runtime::Match,
    match_story::{StoryChange, StoryKey, prepare_story_updates},
    math,
    random::StreamId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportKind {
    Recon,
    Artillery,
    Smoke,
    Cover,
}
impl SupportKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Recon => "recon",
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
pub struct SupportTiming {
    pub kind: SupportKind,
    pub delay: f64,
    pub left: f64,
    pub next: f64,
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
    Plane {
        kind: SupportKind,
        at: Point,
        direction: f64,
        owner: usize,
    },
    Flak {
        unit: u32,
        owner: usize,
        at: Point,
    },
    ShotDown {
        kind: SupportKind,
        at: Point,
        direction: f64,
        fighters: bool,
    },
    Environment(environment::Effect),
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SupportState {
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
fn strip(s: &Strike, along: f64, side: f64) -> Point {
    Point {
        x: s.x + math::cos(s.dir) * along - math::sin(s.dir) * side,
        z: s.z + math::sin(s.dir) * along + math::cos(s.dir) * side,
    }
}
impl Match {
    fn validate_support(&self) -> Result<(), SupportError> {
        if self.options.mode != Mode::Conquest {
            return Err(SupportError::Unsupported(
                "support currently requires Conquest",
            ));
        }
        let n = self.world.game.players.len();
        if self.start.players.len() != n
            || self.start.economy.players.len() != n
            || self.story.len() != n
        {
            return Err(SupportError::Invalid("support player ownership mismatch"));
        }
        if self.world.game.strikes.len() > 4096
            || self.support.covers.len() > 4096
            || self
                .start
                .players
                .iter()
                .any(|p| p.support.values().any(|v| !v.is_finite()))
            || self.start.economy.players.iter().any(|p| !p.mp.is_finite())
            || self.world.game.strikes.iter().any(|s| {
                s.owner >= n
                    || ![s.x, s.z, s.dir].iter().all(|v| v.is_finite())
                    || s.runtime.as_ref().is_none_or(|t| {
                        ![t.delay, t.left, t.next].iter().all(|v| v.is_finite())
                            || t.left < 0.0
                            || (s.kind == StrikeKind::Recon) != (t.kind == SupportKind::Recon)
                    })
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
            "recon" => SupportKind::Recon,
            "artillery" => SupportKind::Artillery,
            "smoke" => SupportKind::Smoke,
            "cover" => SupportKind::Cover,
            "strafe" | "bombing" | "dive" | "para" => {
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
        if self.world.game.strikes.len() == 4096 {
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
        let strike = Strike {
            kind: if kind == SupportKind::Recon {
                StrikeKind::Recon
            } else {
                StrikeKind::Other
            },
            owner,
            x: at.x,
            z: at.z,
            dir: direction,
            runtime: Some(SupportTiming {
                kind,
                delay: number(def, "delay")?,
                left: shells,
                next: 0.0,
            }),
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
        self.world.game.strikes.push(strike);
        story.commit();
        Ok(None)
    }
    /// A passing support is intercepted once, when its arrival delay expires.
    fn intercept_support(
        &mut self,
        strike: &Strike,
        timing: &mut SupportTiming,
        actors: &[u32],
    ) -> Result<bool, SupportError> {
        let team = self.world.game.players[strike.owner].team;
        let at = Point {
            x: strike.x,
            z: strike.z,
        };
        let cover = (timing.kind != SupportKind::Recon)
            .then(|| {
                self.support.covers.iter().position(|c| {
                    c.remaining > 0.0
                        && c.team != team
                        && math::hypot(c.at.x - strike.x, c.at.z - strike.z) <= c.radius
                })
            })
            .flatten();
        let mut down = cover.is_some();
        let mut by = cover.map(|index| self.support.covers[index].owner);
        if let Some(index) = cover {
            self.support.covers[index].remaining = 0.0;
        }
        for &id in actors {
            if down {
                continue;
            }
            let u = &self.world.game.units[&id];
            let def = catalog()?
                .unit(&self.rules.kinds[u.kind].name)
                .map_err(|e| SupportError::Actuation(e.to_string()))?;
            let Some(aa) = def.get("aa") else {
                continue;
            };
            let Some(chance) = aa.get("chance").and_then(Value::number) else {
                continue;
            };
            let setup = aa.get("setup").and_then(Value::boolean).unwrap_or(false);
            if self.commands.units[&id].hold_fire
                || u.hp <= 0.0
                || self.world.game.players[u.owner].team == team
                || math::hypot(u.x - strike.x, u.z - strike.z) > number(aa, "range")?
                || setup && u.still.is_some_and(|t| t < 2.0)
                || u.built.is_some_and(|built| built < 1.0)
                || self
                    .logistics
                    .get(id)
                    .is_some_and(|r| r.forced && !r.stranded)
            {
                continue;
            }
            let owner = u.owner;
            if self.logistics.consume_ammo(id, 1.0) == 0.0 {
                continue;
            }
            self.support.events.push(SupportEvent::Flak {
                unit: id,
                owner,
                at,
            });
            if self.start.random.next_fraction(StreamId::OffMap) < chance {
                down = true;
                by = Some(owner);
            }
        }
        if !down {
            return Ok(false);
        }
        self.record_plane_downed(by)
            .map_err(SupportError::Actuation)?;
        self.support.events.push(SupportEvent::ShotDown {
            kind: timing.kind,
            at,
            direction: strike.dir,
            fighters: cover.is_some(),
        });
        timing.left = if timing.kind == SupportKind::Recon {
            self.start.random.next_fraction(StreamId::OffMap) * timing.left
        } else {
            0.0
        };
        Ok(true)
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
        for &id in unit_order {
            let u = &self.world.game.units[&id];
            if u.owner >= self.world.game.players.len()
                || u.kind >= self.rules.kinds.len()
                || ![u.x, u.z, u.hp].iter().all(|v| v.is_finite())
                || u.still.is_some_and(|v| !v.is_finite())
                || u.built.is_some_and(|v| !v.is_finite())
                || !self.commands.units.contains_key(&id)
                || self
                    .logistics
                    .get(id)
                    .is_some_and(|r| !r.ammo.is_finite() || r.ammo < 0.0)
            {
                return Err(SupportError::Invalid("invalid support actor"));
            }
        }
        let smoke_count = self
            .world
            .game
            .strikes
            .iter()
            .filter(|s| {
                s.runtime
                    .as_ref()
                    .is_some_and(|t| t.kind == SupportKind::Smoke && t.delay - dt <= 0.0)
            })
            .count()
            * 5;
        self.next_unit_id
            .checked_add(
                u32::try_from(smoke_count)
                    .map_err(|_| SupportError::Invalid("smoke identity exhausted"))?,
            )
            .ok_or(SupportError::Invalid("smoke identity exhausted"))?;
        let mut strikes = std::mem::take(&mut self.world.game.strikes);
        let result = (|| {
            for s in &mut strikes {
                let mut timing = s
                    .runtime
                    .take()
                    .ok_or(SupportError::Invalid("missing support timer"))?;
                let kind = timing.kind;
                let def = definition(kind)?;
                timing.delay -= dt;
                // Keep metadata present even if this phase subsequently fails.
                s.runtime = Some(timing.clone());
                if timing.delay > 0.0 {
                    continue;
                }
                if !s.live {
                    s.live = true;
                    if kind == SupportKind::Recon {
                        self.support.events.push(SupportEvent::Plane {
                            kind,
                            at: Point { x: s.x, z: s.z },
                            direction: s.dir,
                            owner: s.owner,
                        });
                        if self.intercept_support(s, &mut timing, unit_order)? {
                            s.runtime = Some(timing);
                            continue;
                        }
                    }
                    if kind == SupportKind::Cover {
                        if self.support.covers.len() == 4096 {
                            return Err(SupportError::Invalid("cover capacity exhausted"));
                        }
                        self.support.covers.push(FighterCover {
                            team: self.world.game.players[s.owner].team,
                            owner: s.owner,
                            at: Point { x: s.x, z: s.z },
                            radius: number(def, "radius")?,
                            remaining: number(def, "dur")?,
                        });
                        timing.left = 0.0;
                    }
                }
                if kind == SupportKind::Recon {
                    timing.left -= dt;
                } else if kind == SupportKind::Smoke {
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
                    self.support
                        .events
                        .push(SupportEvent::SmokeShells(Point { x: s.x, z: s.z }));
                    timing.left = 0.0;
                } else if kind == SupportKind::Artillery {
                    timing.next -= dt;
                    if timing.next > 0.0 {
                        s.runtime = Some(timing);
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
                            ExplosionSource::support(kind.name())
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
                    timing.left -= 1.0;
                    timing.next = number(def, "every")?;
                }
                s.runtime = Some(timing);
            }
            Ok(())
        })();
        if result.is_ok() {
            strikes.retain(|s| !s.live || s.runtime.as_ref().is_some_and(|t| t.left > 0.0));
        }
        self.world.game.strikes = strikes;
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
