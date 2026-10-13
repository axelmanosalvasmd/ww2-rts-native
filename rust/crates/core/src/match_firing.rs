//! Canonical weapon state for the authoritative Match firing phase.
//! Unit ownership, health and position remain in Game; command aim and intent
//! remain in UnitControl, and hit history and experience remain in CombatState.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    catalog::{Catalog, Value},
    game::{CELL, COVER, Point, Unit},
    grid::UnitId,
    match_combat::RetargetSink,
    match_runtime::Match,
    math, projectiles, vision,
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FiringUnit {
    /// Chosen target, with zero meaning no target.
    pub target_id: UnitId,
    pub cooldown: f64,
    pub retarget: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FiringState {
    pub units: BTreeMap<UnitId, FiringUnit>,
    pub flights: BTreeMap<u64, LiveFlight>,
    pub next_flight: u64,
    pub suppressed: BTreeMap<u64, BTreeSet<UnitId>>,
    pub events: Vec<LaunchEvent>,
    pub salvos: Vec<LiveSalvo>,
    claims: BTreeMap<(UnitId, i32), f64>,
    prior: BTreeMap<UnitId, crate::game::Pose>,
}

impl Default for FiringState {
    fn default() -> Self {
        Self {
            units: BTreeMap::new(),
            flights: BTreeMap::new(),
            next_flight: 1,
            suppressed: BTreeMap::new(),
            claims: BTreeMap::new(),
            prior: BTreeMap::new(),
            events: Vec::new(),
            salvos: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveSalvo {
    pub at: Point,
    pub owner: usize,
    pub left: u32,
    pub next: f64,
    pub every: f64,
    pub spread: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LaunchEvent {
    Flight {
        id: u64,
        kind: usize,
        effect: Option<&'static str>,
        shooter: UnitId,
        owner: usize,
        at: projectiles::Point3,
        velocity: projectiles::Point3,
        gravity: f64,
        time: f64,
        expires: f64,
    },
    Salvo {
        shooter: UnitId,
        owner: usize,
        at: Point,
        count: u32,
        spread: f64,
    },
}
impl LaunchEvent {
    fn from_flight(p: &LiveFlight) -> Self {
        Self::Flight {
            id: p.id,
            kind: p.source_kind,
            effect: p.effect,
            shooter: p.shooter,
            owner: p.owner,
            at: p.solution.origin,
            velocity: p.solution.velocity,
            gravity: p.solution.gravity,
            time: p.launched,
            expires: p.expires,
        }
    }
}

/// A round retains its launch solution when its shooter or intended target moves.
/// Shooter XP and hit metadata stay in CombatState while this ID remains referenced.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveFlight {
    pub id: u64,
    pub owner: usize,
    pub shooter: UnitId,
    pub source_kind: usize,
    pub solution: projectiles::Flight,
    pub launch_origin: projectiles::Point3,
    pub launched: f64,
    pub last: f64,
    pub expires: f64,
    pub sent: bool,
    pub energy: f64,
    pub sequence: u32,
    pub max_contacts: u32,
    pub near: f64,
    pub start_cell: usize,
    pub intended: Option<UnitId>,
    pub damage_scale: f64,
    pub armor_piercing: bool,
    pub ap_multiplier: f64,
    pub ap_naval: bool,
    pub suppression: f64,
    pub suppression_scale: f64,
    pub volley_group: u64,
    pub explosive: bool,
    pub terrain: f64,
    pub scar: f64,
    pub blast_radius: Option<f64>,
    pub blast_source: bool,
    pub effect: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlightSurface {
    Unit(UnitId),
    Cell(usize),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlightContact {
    pub hit: projectiles::ContactHit,
    pub surface: FlightSurface,
}

/// Terrain uses world-layers MATERIALS. Unit contacts use the projectile steel profile.
fn terrain_contact_material(name: &str) -> projectiles::ContactMaterial {
    let (name, hardness, absorption, rebound) = match name {
        "road" => ("road", 0.45, 0.65, 0.12),
        "wood" => ("wood", 0.3, 0.8, 0.05),
        "stone" => ("stone", 0.85, 0.3, 0.55),
        "concrete" => ("concrete", 0.95, 0.25, 0.65),
        "steel" => ("steel", 1.0, 0.15, 0.75),
        "water" => ("water", 0.0, 1.0, 0.0),
        _ => ("soil", 0.12, 0.9, 0.02),
    };
    projectiles::ContactMaterial {
        name,
        hardness,
        absorption,
        rebound,
    }
}

impl RetargetSink for FiringState {
    fn reset_retarget(&mut self, unit: UnitId) {
        self.initialize(unit).retarget = 0.0;
    }
}

fn n(v: &Value, key: &str, fallback: f64) -> f64 {
    v.get(key).and_then(Value::number).unwrap_or(fallback)
}
fn flag(v: &Value, key: &str) -> bool {
    v.get(key).is_some_and(|v| match v {
        Value::Bool(b) => *b,
        Value::Number(n) => *n != 0.0 && !n.is_nan(),
        _ => false,
    })
}
fn dist(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}

impl Match {
    fn flight_contact(
        &self,
        p: &LiveFlight,
        a: projectiles::Point3,
        b: projectiles::Point3,
        start_fraction: f64,
        end_fraction: f64,
    ) -> Result<Option<FlightContact>, String> {
        let g = &self.world.game;
        let level_height = n(self.firing_config()?, "levelHeight", 2.5);
        let mut best: Option<FlightContact> = None;
        for c in projectiles::crossed_cells(
            projectiles::BodyPosition { x: a.x, z: a.z },
            projectiles::BodyPosition { x: b.x, z: b.z },
            CELL,
            g.w,
            g.h,
        )
        .map_err(|e| format!("flight cell traversal: {e:?}"))?
        {
            let ch = g.physical(c);
            if p.sequence == 0
                && (c == p.start_cell
                    || (ch == b'#'
                        && (c % g.w).abs_diff(p.start_cell % g.w) <= 1
                        && (c / g.w).abs_diff(p.start_cell / g.w) <= 1))
            {
                continue;
            }
            let x = (c % g.w) as f64 * CELL;
            let z = (c / g.w) as f64 * CELL;
            let ground = g.height.as_ref().map_or(0.0, |h| f64::from(h[c])) * level_height;
            let obstacle = match ch {
                b'B' | b'K' => 5.0,
                b'H' => 1.8,
                b'#' | b'Y' => 1.2,
                b'Q' => 1.6,
                b'R' | b'O' => 0.8,
                b'=' => 0.25,
                _ => 0.0,
            };
            if let Some(hit) = projectiles::sweep_box(
                a,
                b,
                projectiles::Box3 {
                    x: [x, x + CELL],
                    y: [-100.0, ground + obstacle],
                    z: [z, z + CELL],
                },
            )
            .map_err(|e| format!("flight terrain sweep: {e:?}"))?
                && best.is_none_or(|old| hit.fraction < old.hit.fraction)
            {
                best = Some(FlightContact {
                    hit,
                    surface: FlightSurface::Cell(c),
                });
            }
        }
        let center = ((a.x + b.x) / 2.0, (a.z + b.z) / 2.0);
        let reach =
            math::hypot(b.x - a.x, b.z - a.z) / 2.0 + 14.0 + if g.naval { 55.0 } else { 0.0 };
        // Exact unit-contact ties use canonical insertion/ID order instead of bucket update order.
        for id in self.grid.in_order(&g.units, center, reach, |_, t| {
            t.hp > 0.0
                && t.air.is_none()
                && !t.riding
                && t.id != p.shooter
                && (p.blast_source || !g.allied(p.owner, t.owner))
        }) {
            let t = &g.units[&id];
            let def = &self.rules.kinds[t.kind];
            if def.building && t.cells.as_ref().is_some_and(|c| !c.is_empty()) {
                continue;
            }
            let old = self
                .firing
                .prior
                .get(&id)
                .copied()
                .unwrap_or_else(|| t.pose());
            let before = projectiles::BodyPosition {
                x: old.x + (t.x - old.x) * start_fraction,
                z: old.z + (t.z - old.z) * start_fraction,
            };
            let after = projectiles::BodyPosition {
                x: old.x + (t.x - old.x) * end_fraction,
                z: old.z + (t.z - old.z) * end_fraction,
            };
            let ground = f64::from(g.level_at(t.x, t.z)) * level_height;
            let radius = n(self.definition(t)?, "radius", 1.5) * 0.8;
            let top = ground
                + if def.structure || def.naval {
                    5.0
                } else if def.infantry {
                    1.8
                } else {
                    2.6
                };
            let hit = if def.hull != 0.0 {
                projectiles::sweep_hull(
                    a,
                    b,
                    projectiles::HullPosition {
                        x: before.x,
                        z: before.z,
                        rotation: Some(old.rot),
                    },
                    projectiles::HullPosition {
                        x: after.x,
                        z: after.z,
                        rotation: Some(t.rot),
                    },
                    radius,
                    def.hull,
                    ground,
                    top,
                )
            } else {
                projectiles::sweep_body(a, b, before, after, radius, ground, top)
            }
            .map_err(|e| format!("flight body sweep: {e:?}"))?;
            if let Some(hit) = hit
                && best.is_none_or(|old| hit.fraction < old.hit.fraction)
            {
                best = Some(FlightContact {
                    hit,
                    surface: FlightSurface::Unit(id),
                });
            }
        }
        Ok(best)
    }
    fn fire_weapon(&mut self, id: UnitId, target: UnitId, moving: bool) -> Result<(), String> {
        let g = &self.world.game;
        let u = &g.units[&id];
        let t = &g.units[&target];
        let w = self.weapon(u)?.ok_or("unarmed firing")?;
        let definition = self.definition(u)?;
        let target_definition = self.definition(t)?;
        let salvo = flag(w, "salvo");
        let per_model = flag(w, "perModel");
        let requested = if salvo {
            n(w, "rockets", f64::NAN)
        } else if per_model {
            (u.hp / n(definition, "hpPer", 1.0)).ceil()
        } else {
            1.0
        };
        if !requested.is_finite() || !(0.0..=4096.0).contains(&requested) {
            return Err("invalid volley size".into());
        }
        let requested = math::max(0.0, requested).floor();
        let count = self.logistics.get(id).map_or(requested, |r| {
            math::min(requested, math::max(0.0, r.ammo).floor())
        }) as u32;
        let supply_rate = self.logistics.shortage_rate(id);
        let end_id = self
            .firing
            .next_flight
            .checked_add(u64::from(count))
            .filter(|n| *n <= 9_007_199_254_740_991)
            .ok_or("flight ID exhausted")?;
        let cfg = self.firing_config()?;
        let level_height = n(cfg, "levelHeight", 2.5);
        let suppression_accuracy = if u.supp >= 90.0 {
            0.5
        } else if u.supp >= 50.0 {
            0.7
        } else {
            1.0
        };
        let rate = if u.supp >= 90.0 {
            3.0
        } else if u.supp >= 50.0 {
            1.5
        } else {
            1.0
        };
        let inf = self.rules.kinds[t.kind].infantry;
        let cover = if flag(w, "flame") {
            1.0
        } else {
            math::min(
                self.cover_mul(t, u.at())?,
                if inf {
                    1.0 - (1.0 - n(cfg, "coverMul", 1.0)) * self.cover_behind(t, u.at())?
                } else {
                    1.0
                },
            )
        };
        let hg = math::min(
            1.45,
            math::max(
                0.7,
                1.0 + n(cfg, "highGroundAcc", 0.15)
                    * f64::from(g.level_at(u.x, u.z) - g.level_at(t.x, t.z)),
            ),
        );
        let xp = self.combat.get(id).map_or(0.0, |m| m.xp);
        let veterancy = crate::combat::veterancy(&self.rules.kinds[u.kind].name, xp)
            .map_err(|e| format!("{e:?}"))?;
        let accuracy = n(w, if inf { "accInf" } else { "accVeh" }, 0.0)
            * suppression_accuracy
            * if moving {
                n(w, "moveFire", f64::NAN)
            } else {
                1.0
            }
            * cover
            * hg
            * (1.0 + n(cfg, "vetAcc", 0.1) * f64::from(veterancy));
        let spread = if let Some(far) = w.get("spreadFar").and_then(Value::number) {
            let a = n(w, "spread", 0.0);
            a + (far - a)
                * math::min(
                    1.0,
                    math::max(
                        0.0,
                        (dist(u.at(), t.at()) - n(w, "minRange", 0.0))
                            / (n(w, "range", 0.0) - n(w, "minRange", 0.0)),
                    ),
                )
        } else {
            n(w, "spread", 0.0)
        };
        let aim = self.aim_point(u, t);
        let bearing = math::atan2(aim.z - u.z, aim.x - u.x);
        let from = projectiles::Point3::new(
            u.x,
            f64::from(g.level_at(u.x, u.z)) * level_height
                + if u.garrison.is_some() { 3.5 } else { 1.2 },
            u.z,
        );
        let profile = if salvo {
            projectiles::ARTILLERY
        } else {
            projectiles::projectile_profile(
                &self.rules.kinds[u.kind].name,
                flag(w, "flame"),
                n(w, "veh", 0.0),
                flag(w, "shellTerrain"),
            )
            .map_err(|e| format!("projectile profile: {e:?}"))?
        };
        let duration = salvo.then(|| n(w, "flight", f64::NAN));
        let mut random = self.start.random.clone();
        let mut flights = Vec::with_capacity(count as usize);
        let first_id = self.firing.next_flight;
        for i in 0..count {
            let (x, z, height) = if salvo {
                let angle =
                    random.next_fraction(crate::random::StreamId::Combat) * std::f64::consts::TAU;
                let radius = random.next_fraction(crate::random::StreamId::Combat).sqrt() * spread;
                (
                    t.x + math::cos(angle) * radius,
                    t.z + math::sin(angle) * radius,
                    0.02,
                )
            } else {
                let accurate = random.next_fraction(crate::random::StreamId::Combat) < accuracy;
                let off = if accurate {
                    0.0
                } else {
                    (n(target_definition, "radius", 1.5) * 0.95
                        + 0.2
                        + random.next_fraction(crate::random::StreamId::Combat) * 0.7)
                        * if random.next_fraction(crate::random::StreamId::Combat) < 0.5 {
                            -1.0
                        } else {
                            1.0
                        }
                };
                (
                    aim.x - math::sin(bearing) * off,
                    aim.z + math::cos(bearing) * off,
                    if accurate {
                        if inf { 0.8 } else { 1.2 }
                    } else {
                        0.08
                    },
                )
            };
            let at =
                projectiles::Point3::new(x, f64::from(g.level_at(x, z)) * level_height + height, z);
            let solution = projectiles::launch_solution(
                from,
                at,
                profile.speed.unwrap_or(1.0),
                profile.gravity,
                duration,
            )
            .map_err(|e| format!("launch solution: {e:?}"))?;
            let delay = if salvo {
                f64::from(i) * n(w, "every", 0.0)
            } else {
                0.0
            };
            let born = g.tick as f64 * 0.05 + delay;
            let expires = born
                + duration.unwrap_or_else(|| {
                    math::max(
                        solution.duration + 0.15,
                        n(w, "range", 20.0) / profile.speed.unwrap_or(1.0),
                    )
                });
            if !born.is_finite() || !expires.is_finite() {
                return Err("invalid flight lifetime".into());
            }
            let flight_id = first_id + u64::from(i);
            flights.push(LiveFlight {
                id: flight_id,
                owner: u.owner,
                shooter: id,
                source_kind: u.kind,
                solution: projectiles::Flight {
                    origin: solution.origin,
                    velocity: solution.velocity,
                    gravity: solution.gravity,
                    stamp: born,
                },
                launch_origin: from,
                launched: born,
                last: born,
                expires,
                sent: delay == 0.0,
                energy: 1.0,
                sequence: 0,
                max_contacts: profile.max_contacts,
                near: profile.near,
                start_cell: g.cell_of(u.x, u.z).ok_or("shooter outside battlefield")?,
                intended: (!salvo).then_some(target),
                damage_scale: 1.0,
                armor_piercing: false,
                ap_multiplier: n(
                    definition.get("ab").ok_or("missing ability definition")?,
                    "mult",
                    1.5,
                ),
                ap_naval: definition.get("ab").is_some_and(|a| flag(a, "naval")),
                suppression: if salvo { 0.0 } else { n(w, "supp", 0.0) },
                suppression_scale: if per_model {
                    f64::from(count) / n(definition, "models", 1.0)
                } else {
                    1.0
                },
                volley_group: if salvo { flight_id } else { first_id },
                explosive: salvo || flag(w, "shellTerrain"),
                terrain: if salvo {
                    0.0
                } else {
                    n(w, "shellTerrain", 0.0)
                },
                scar: if !salvo && flag(w, "shellTerrain") {
                    n(cfg.get("scar").ok_or("missing scar rules")?, "tank", 0.25)
                } else {
                    0.0
                },
                blast_radius: salvo.then(|| n(w, "blast", 0.0)),
                blast_source: salvo,
                effect: salvo.then_some("rocket"),
            });
        }
        self.logistics.consume_ammo(id, requested);
        self.start.random = random;
        self.firing.next_flight = end_id;
        for flight in flights {
            self.firing
                .suppressed
                .entry(flight.volley_group)
                .or_default();
            if flight.sent {
                self.firing.events.push(LaunchEvent::from_flight(&flight));
            }
            self.firing.flights.insert(flight.id, flight);
        }
        if salvo && count > 0 {
            self.firing.salvos.push(LiveSalvo {
                at: t.at(),
                owner: u.owner,
                left: count,
                next: n(w, "flight", 0.0),
                every: n(w, "every", 0.0),
                spread,
            });
            self.firing.events.push(LaunchEvent::Salvo {
                shooter: id,
                owner: u.owner,
                at: t.at(),
                count,
                spread,
            });
        }
        self.world.game.units.get_mut(&id).unwrap().shot_at = Some(self.world.game.tick as f64);
        self.firing.initialize(id).cooldown =
            n(w, "interval", 0.0) * if salvo { 1.0 } else { rate } * supply_rate;
        Ok(())
    }

    /// Physical salvos retain the reference timer's two draws for each emitted segment.
    /// The projectile already carries the physical payload, so this timer emits no second blast.
    pub fn advance_firing_salvos(&mut self, dt: f64) -> Result<(), String> {
        if !dt.is_finite() || dt < 0.0 {
            return Err("invalid salvo timer step".into());
        }
        for s in &mut self.firing.salvos {
            s.next -= dt;
            if s.next > 0.0 {
                continue;
            }
            self.start
                .random
                .next_fraction(crate::random::StreamId::Combat);
            self.start
                .random
                .next_fraction(crate::random::StreamId::Combat);
            s.left -= 1;
            s.next = s.every;
        }
        self.firing.salvos.retain(|s| s.left > 0);
        Ok(())
    }
    fn turn_firing_hull(&mut self, id: UnitId, to: f64, dt: f64) -> Result<(), String> {
        let g = &self.world.game;
        let u = &g.units[&id];
        let kind = &self.rules.kinds[u.kind];
        let d = math::atan2(math::sin(to - u.rot), math::cos(to - u.rot));
        let max = kind.vehicle.as_ref().map_or(
            n(
                self.firing_config()?
                    .get("behavior")
                    .ok_or("missing behavior rules")?,
                "hullTurn",
                1.5,
            ),
            |p| p.hull_turn,
        ) * dt;
        let r = u.rot + math::max(-max, math::min(max, d));
        let next = math::atan2(math::sin(r), math::cos(r));
        if !kind.ground_vehicle()
            || crate::vehicle::swept_vehicle_clear(
                g,
                u.pose(),
                crate::game::Pose {
                    rot: next,
                    ..u.pose()
                },
                kind,
                kind.block(),
            )
        {
            self.world.game.units.get_mut(&id).unwrap().rot = next;
        }
        Ok(())
    }
    /// Run one unit's source targeting/firing block after its movement and grid update.
    /// Timer decay is caller-owned through advance_firing_timers before movement.
    pub fn apply_unit_firing(&mut self, id: UnitId, moving: bool, dt: f64) -> Result<(), String> {
        if !dt.is_finite() || dt < 0.0 {
            return Err("invalid firing step".into());
        }
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(());
        };
        if u.hp <= 0.0 {
            return Ok(());
        }
        let kind = &self.rules.kinds[u.kind];
        let control = self
            .commands
            .units
            .get(&id)
            .ok_or("missing canonical unit control")?;
        if kind.air
            || kind.rail
            || control.operative
            || control.fire_at.is_some()
            || control.nade.is_some()
        {
            return Err("air, rail, operative and ability/terrain firing are unsupported".into());
        }
        if u.riding {
            return Ok(());
        }
        if kind.building && (!kind.weapon || u.built.is_none_or(|v| v < 1.0)) {
            return Ok(());
        }
        let attack = control.attack_id;
        let hold_fire = control.hold_fire;
        let Some(w) = self.weapon(u)? else {
            self.firing.initialize(id).target_id = 0;
            self.commands.units.get_mut(&id).unwrap().aim = u.rot;
            return Ok(());
        };
        if flag(w, "beam") {
            return Err("beam firing is unsupported".into());
        }
        let hull = !kind.infantry && !kind.structure && !moving;
        if hull {
            let target = self
                .firing
                .get(id)
                .and_then(|s| self.world.game.units.get(&s.target_id))
                .filter(|t| t.hp > 0.0 && self.world.game.players[u.owner].sees(t.id));
            let seen = target.map(Unit::at);
            let danger = target.is_some_and(|t| {
                self.weapon(t)
                    .ok()
                    .flatten()
                    .is_some_and(|w| n(w, "veh", 0.0) >= 20.0)
            });
            let recent = self
                .combat
                .get(id)
                .is_some_and(|m| self.world.game.tick as f64 - m.hit_at <= 100.0);
            let at = if danger {
                seen
            } else if recent && u.hit_from.is_some() {
                u.hit_from
            } else {
                seen
            };
            if let Some(at) = at {
                self.turn_firing_hull(id, math::atan2(at.z - u.z, at.x - u.x), dt)?;
            }
        }
        let u = self.world.game.units[&id].clone();
        if n(w, "range", 0.0) == 0.0 || !self.logistics.can_fire(id, 1.0) || u.retreating {
            self.firing.initialize(id).target_id = 0;
            self.commands.units.get_mut(&id).unwrap().aim = u.rot;
            return Ok(());
        }
        let current = self.firing.get(id).copied().unwrap_or_default();
        if hold_fire && attack == 0 {
            self.set_firing_target(id, 0)?;
        } else if attack == 0
            && (current.retarget <= 0.0
                || (current.target_id != 0 && !self.can_shoot_target(id, current.target_id)?))
        {
            let target = self.pick_firing_target(id)?;
            self.set_firing_target(id, target)?;
            self.firing.initialize(id).retarget = 0.5;
        }
        let target = self.firing.get(id).unwrap().target_id;
        if target != 0 && self.can_shoot_target(id, target)? {
            let t = &self.world.game.units[&target];
            let aim = math::atan2(t.z - u.z, t.x - u.x);
            self.commands.units.get_mut(&id).unwrap().aim = aim;
            if self.rules.kinds[u.kind].infantry && !moving {
                self.world.game.units.get_mut(&id).unwrap().rot = aim;
            }
            let ready = if moving {
                w.get("moveFire").is_some()
            } else {
                u.still.is_some_and(|s| s >= n(w, "setup", 0.0))
            };
            if ready
                && self.firing.get(id).unwrap().cooldown <= 0.0
                && (!flag(w, "area") || attack == target)
            {
                self.fire_weapon(id, target, moving)?;
            }
        } else {
            self.set_firing_target(id, 0)?;
            if let Some(face) = self.commands.units[&id].face
                && !moving
                && u.amove.is_none()
                && (!hull
                    || u.hit_from.is_none()
                    || self
                        .combat
                        .get(id)
                        .is_none_or(|m| self.world.game.tick as f64 - m.hit_at > 100.0))
            {
                if hull {
                    self.turn_firing_hull(id, face.angle, dt)?;
                } else if self.rules.kinds[u.kind].infantry {
                    self.world.game.units.get_mut(&id).unwrap().rot = face.angle;
                }
            }
            self.commands.units.get_mut(&id).unwrap().aim = self.world.game.units[&id].rot;
        }
        Ok(())
    }
    fn firing_config(&self) -> Result<&'static Value, String> {
        Catalog::compiled()
            .map_err(|e| format!("{e:?}"))?
            .table("CFG")
            .map_err(|e| e.to_string())
    }
    fn cell_max_hp(&self, cell: usize) -> f64 {
        let g = &self.world.game;
        if let Some(s) = g.sections.get(&(cell as u32))
            && !s.failed
            && let Some(hp) = s.max_hp
        {
            return hp;
        }
        let ch = g.physical(cell);
        if ch == b'B' {
            self.rules.movement.house_hp[usize::from(g.house[cell])]
        } else {
            self.rules.movement.terrain_hp(ch).unwrap_or(0.0)
                * g.tough.as_ref().map_or(1.0, |t| t[cell])
        }
    }
    fn cover_quality(&self, cell: usize) -> Result<f64, String> {
        let g = &self.world.game;
        let cfg = self.firing_config()?;
        let ch = g.physical(cell);
        let max = self.cell_max_hp(cell);
        Ok(if ch == b'O' {
            n(cfg.get("wood").ok_or("missing wood rules")?, "cover", 0.6)
        } else if ch == b'+' {
            math::min(
                1.4,
                0.55 + g.wear.as_ref().map_or(0.45, |w| f64::from(w[cell])),
            )
        } else if max > 1.0 {
            0.5 + 0.5 * math::max(0.0, f64::from(g.cell_hp[cell])) / max
        } else {
            1.0
        })
    }
    pub(crate) fn cover_mul(&self, t: &Unit, from: Point) -> Result<f64, String> {
        let g = &self.world.game;
        let cfg = self.firing_config()?;
        if let Some(c) = t.garrison {
            let houses = cfg
                .get("houses")
                .and_then(Value::array)
                .ok_or("missing house rules")?;
            let house = houses
                .get(usize::from(g.house[c]))
                .ok_or("invalid house kind")?;
            let worn = n(house, "worn", 1.0);
            return Ok(worn
                + (n(house, "mul", 1.0) - worn) * math::max(0.0, f64::from(g.cell_hp[c]))
                    / n(house, "hp", 1.0));
        }
        let c = g.cell_of(t.x, t.z).ok_or("unit outside battlefield")?;
        let trench = cfg.get("trench").ok_or("missing trench rules")?;
        let front = g.trench_front.as_ref().map(|f| f64::from(f[c]));
        let faces = front.is_none_or(|f| {
            f.is_nan()
                || math::cos(math::atan2(from.z - t.z, from.x - t.x) - f) > n(trench, "arc", 0.0)
        });
        if vision::in_trench(g, &self.rules, t) && faces {
            let fresh = n(trench, "fresh", 1.0);
            let dug = n(trench, "dug", 1.0);
            return Ok(fresh
                + (dug - fresh)
                    * math::min(1.0, t.still.unwrap_or(f64::NAN) / n(trench, "settle", 1.0)));
        }
        Ok(
            if self.rules.kinds[t.kind].infantry && g.flags[c] & COVER != 0 {
                1.0 - (1.0 - n(cfg, "coverMul", 1.0)) * self.cover_quality(c)?
            } else {
                1.0
            },
        )
    }
    pub(crate) fn cover_behind(&self, t: &Unit, from: Point) -> Result<f64, String> {
        let g = &self.world.game;
        let a = math::atan2(from.z - t.z, from.x - t.x);
        let ca = math::cos(a);
        let sa = math::sin(a);
        let mut best = 0.0;
        let mut d = 0.8;
        while d < 3.7 {
            if let Some(c) = g.cell_of(t.x + ca * d, t.z + sa * d)
                && b"B#RHKQ".contains(&g.physical(c))
            {
                best = math::min(1.0, (3.8 - d) / 1.6) * math::min(1.0, self.cover_quality(c)?);
                break;
            }
            d += 0.7;
        }
        let x = (t.x / CELL).floor() as isize;
        let z = (t.z / CELL).floor() as isize;
        for (dx, dz) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let nx = x + dx;
            let nz = z + dz;
            if nx < 0 || nz < 0 || nx >= g.w as isize || nz >= g.h as isize {
                continue;
            }
            if b"BK".contains(&g.physical(nz as usize * g.w + nx as usize))
                && (dx as f64 * ca + dz as f64 * sa) / math::hypot(dx as f64, dz as f64) > 0.5
            {
                best = 1.0;
            }
        }
        let mut hull = |at: Point| {
            let dx = at.x - t.x;
            let dz = at.z - t.z;
            let d = math::hypot(dx, dz);
            if d > 0.0 && d < 4.0 && (dx * ca + dz * sa) / d > 0.7 {
                best = math::max(best, math::min(1.0, (4.0 - d) / 1.5));
            }
        };
        for v in g.units.values() {
            if v.id != t.id && v.air.is_none() && !self.rules.kinds[v.kind].infantry && v.hp > 0.0 {
                hull(v.at());
            }
        }
        for v in &g.wrecks {
            if !v.moving {
                hull(Point { x: v.x, z: v.z });
            }
        }
        Ok(best)
    }
    fn volley(&self, u: &Unit, t: &Unit) -> Result<f64, String> {
        let Some(w) = self.weapon(u)? else {
            return Ok(0.0);
        };
        if flag(w, "salvo") {
            return Ok(0.0);
        }
        let inf = self.rules.kinds[t.kind].infantry;
        let count = if flag(w, "perModel") {
            (u.hp / n(self.definition(u)?, "hpPer", 1.0)).ceil()
        } else {
            1.0
        };
        Ok(n(w, if inf { "inf" } else { "veh" }, 0.0)
            * n(w, if inf { "accInf" } else { "accVeh" }, 0.0)
            * count)
    }
    pub(crate) fn set_firing_target(&mut self, id: UnitId, next: UnitId) -> Result<(), String> {
        let u = self.world.game.units.get(&id).ok_or("missing shooter")?;
        let previous = self.firing.get(id).map_or(0, |s| s.target_id);
        if previous != next {
            let team = self.world.game.players[u.owner].team;
            for (target, sign) in [(previous, -1.0), (next, 1.0)] {
                if let Some(t) = self.world.game.units.get(&target) {
                    let amount = self.volley(u, t)?;
                    *self.firing.claims.entry((target, team)).or_default() += sign * amount;
                }
            }
        }
        self.firing.initialize(id).target_id = next;
        Ok(())
    }
    /// Build shared target claims before the ordered per-unit timer/movement/firing loop.
    pub fn begin_firing_tick(&mut self, unit_order: &[UnitId]) -> Result<(), String> {
        if unit_order.windows(2).any(|w| w[0] >= w[1]) {
            return Err("firing unit IDs must be strictly increasing".into());
        }
        if self.world.game.tick < 0 || self.world.game.tick > 9_007_199_254_740_351 {
            return Err("invalid firing tick".into());
        }
        for u in self.world.game.units.values() {
            if u.kind >= self.rules.kinds.len()
                || u.owner >= self.world.game.players.len()
                || ![u.x, u.z, u.hp, u.rot, u.supp]
                    .iter()
                    .all(|n| n.is_finite())
                || self.world.game.cell_of(u.x, u.z).is_none()
                || u.cells
                    .as_ref()
                    .is_some_and(|c| c.iter().any(|&c| c as usize >= self.world.game.flags.len()))
                || u.garrison.is_some_and(|c| c >= self.world.game.flags.len())
                || self
                    .firing
                    .get(u.id)
                    .is_some_and(|s| !s.cooldown.is_finite() || !s.retarget.is_finite())
            {
                return Err("invalid canonical firing unit".into());
            }
        }
        let mut claims = BTreeMap::new();
        for u in self.world.game.units.values() {
            if u.hp <= 0.0 {
                continue;
            }
            let target = self.firing.get(u.id).map_or(0, |s| s.target_id);
            if let Some(t) = self.world.game.units.get(&target) {
                *claims
                    .entry((target, self.world.game.players[u.owner].team))
                    .or_default() += self.volley(u, t)?;
            }
        }
        self.firing.claims = claims;
        self.firing.prior = self
            .world
            .game
            .units
            .iter()
            .map(|(&id, u)| (id, u.pose()))
            .collect();
        Ok(())
    }
    /// The tick integrator calls this once before this unit's movement, not at firing time.
    pub fn advance_firing_timers(&mut self, id: UnitId, dt: f64) -> Result<(), String> {
        if !dt.is_finite() || dt < 0.0 {
            return Err("invalid weapon timer step".into());
        }
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(());
        };
        if u.hp <= 0.0 || u.riding {
            return Ok(());
        }
        let kind = &self.rules.kinds[u.kind];
        if kind.air || kind.rail {
            return Err("air and rail firing phases are unsupported".into());
        }
        if kind.building && (!kind.weapon || u.built.is_none_or(|n| n < 1.0)) {
            return Ok(());
        }
        let row = self.firing.initialize(id);
        row.cooldown -= dt;
        row.retarget -= dt;
        Ok(())
    }
    fn pick_firing_target(&self, id: UnitId) -> Result<UnitId, String> {
        let g = &self.world.game;
        let u = &g.units[&id];
        let w = self.weapon(u)?.ok_or("unarmed target selection")?;
        let current = self.firing.get(id).map_or(0, |s| s.target_id);
        let team = g.players[u.owner].team;
        let salvo = flag(w, "salvo");
        let pool = if salvo {
            self.grid
                .unordered(&g.units, (u.x, u.z), n(w, "range", 0.0) + 6.0, |_, t| {
                    !t.riding && !g.allied(t.owner, u.owner) && g.players[u.owner].sees(t.id)
                })
        } else {
            vec![]
        };
        let targets = self.grid.in_order(
            &g.units,
            (u.x, u.z),
            n(w, "range", 0.0) + if g.naval { 55.0 } else { 0.0 },
            |_, t| {
                t.hp > 0.0
                    && t.air.is_none()
                    && !g.allied(t.owner, u.owner)
                    && g.players[u.owner].sees(t.id)
            },
        );
        let mut best = 0;
        let mut best_score = f64::INFINITY;
        let mut current_score = f64::INFINITY;
        for target in targets {
            if !self.can_shoot_target(id, target)? {
                continue;
            }
            let t = &g.units[&target];
            let definition = self.definition(t)?;
            let infantry = self.rules.kinds[t.kind].infantry;
            let trench = vision::in_trench(g, &self.rules, t);
            let covered = infantry && g.flags_at(t.x, t.z) & COVER != 0;
            let dug = if t.garrison.is_some() {
                3.0
            } else if trench {
                2.0
            } else if covered {
                1.5
            } else {
                1.0
            };
            let value = if salvo {
                dug * (1 + pool
                    .iter()
                    .filter(|&&other| {
                        let o = &g.units[&other];
                        o.owner == t.owner && dist(o.at(), t.at()) < 6.0
                    })
                    .count()) as f64
            } else {
                let factor = if infantry {
                    if flag(w, "flame") {
                        if t.garrison.is_some() || trench {
                            n(w, "flame", 1.0)
                        } else {
                            1.0
                        }
                    } else {
                        self.cover_mul(t, u.at())?
                    }
                } else if self.rules.kinds[t.kind].structure {
                    1.0
                } else {
                    crate::combat::armor_mul(&self.rules.kinds[t.kind].name, t.at(), t.rot, u.at())
                        .map_err(|e| format!("{e:?}"))?
                };
                n(w, if infantry { "inf" } else { "veh" }, 0.0)
                    * n(w, if infantry { "accInf" } else { "accVeh" }, 0.0)
                    * factor
                    * crate::combat::role_mul(
                        &self.rules.kinds[u.kind].name,
                        &self.rules.kinds[t.kind].name,
                    )
                    .map_err(|e| format!("{e:?}"))?
            };
            let meta = self.combat.get(id);
            let target_state = self.firing.get(t.id);
            let incoming = if meta.is_some_and(|m| m.hit_by == Some(t.id)) {
                meta.unwrap().hit_at
            } else if target_state.is_some_and(|s| s.target_id == id) {
                t.shot_at.unwrap_or(-1e9)
            } else {
                -1e9
            };
            let cfg = self.firing_config()?;
            let threat_time = n(
                cfg.get("behavior").ok_or("missing behavior rules")?,
                "threatTime",
                5.0,
            ) / 0.05;
            let threat = if g.tick as f64 - incoming <= threat_time {
                2.5
            } else if target_state.is_some_and(|s| s.target_id == id) {
                1.5
            } else {
                1.0
            };
            let others = if salvo {
                0.0
            } else {
                self.firing
                    .claims
                    .get(&(t.id, team))
                    .copied()
                    .unwrap_or(0.0)
                    - if t.id == current {
                        self.volley(u, t)?
                    } else {
                        0.0
                    }
            };
            let score = dist(u.at(), t.at()) / (value * threat)
                * if flag(definition, "structure") {
                    5.0
                } else {
                    1.0
                }
                * math::max(1.0, others / t.hp);
            if t.id == current {
                current_score = score;
            }
            if score < best_score {
                best_score = score;
                best = t.id;
            }
        }
        let gain = n(
            self.firing_config()?
                .get("behavior")
                .ok_or("missing behavior rules")?,
            "switchGain",
            0.7,
        );
        Ok(
            if current_score < f64::INFINITY && best_score > current_score * gain {
                current
            } else {
                best
            },
        )
    }
    fn definition(&self, u: &Unit) -> Result<&'static Value, String> {
        let kind = self.rules.kinds.get(u.kind).ok_or("unknown firing kind")?;
        Catalog::compiled()
            .map_err(|e| format!("{e:?}"))?
            .unit(&kind.name)
            .map_err(|e| e.to_string())
    }
    fn weapon(&self, u: &Unit) -> Result<Option<&'static Value>, String> {
        Ok(self.definition(u)?.get("w"))
    }
    fn hull_point(&self, from: Point, target: &Unit) -> Point {
        let h = self.rules.kinds[target.kind].hull;
        if h == 0.0 {
            return target.at();
        }
        let cx = math::cos(target.rot);
        let cz = math::sin(target.rot);
        let k = math::max(
            -h,
            math::min(h, (from.x - target.x) * cx + (from.z - target.z) * cz),
        );
        Point {
            x: target.x + cx * k,
            z: target.z + cz * k,
        }
    }
    fn aim_point(&self, shooter: &Unit, target: &Unit) -> Point {
        let Some(cells) = &target.cells else {
            return self.hull_point(shooter.at(), target);
        };
        let mut best = target.at();
        let mut distance = f64::INFINITY;
        for &cell in cells {
            if self
                .world
                .game
                .sections
                .get(&cell)
                .is_some_and(|s| s.failed || s.hp <= 0.0)
            {
                continue;
            }
            let at = Point {
                x: (cell as usize % self.world.game.w) as f64 * CELL + CELL / 2.0,
                z: (cell as usize / self.world.game.w) as f64 * CELL + CELL / 2.0,
            };
            let d = dist(shooter.at(), at);
            if d < distance {
                distance = d;
                best = at;
            }
        }
        best
    }
    /// Current authoritative visibility, alliance, hull range and line-of-sight query.
    /// Salvos require spotting and minimum range instead of a direct sight line.
    pub fn can_shoot_target(&self, shooter: UnitId, target: UnitId) -> Result<bool, String> {
        let g = &self.world.game;
        let Some(u) = g.units.get(&shooter) else {
            return Ok(false);
        };
        let Some(t) = g.units.get(&target) else {
            return Ok(false);
        };
        if g.w == 0
            || g.h == 0
            || u.kind >= self.rules.kinds.len()
            || t.kind >= self.rules.kinds.len()
            || u.owner >= g.players.len()
            || t.owner >= g.players.len()
            || ![u.x, u.z, u.rot, t.x, t.z, t.rot, t.hp]
                .iter()
                .all(|v| v.is_finite())
            || t.cells
                .as_ref()
                .is_some_and(|cells| cells.iter().any(|&c| c as usize >= g.flags.len()))
        {
            return Err("invalid canonical targeting input".into());
        }
        let Some(w) = self.weapon(u)? else {
            return Ok(false);
        };
        let range = n(w, "range", 0.0);
        if range == 0.0
            || !self.logistics.can_fire(u.id, 1.0)
            || t.hp <= 0.0
            || t.air.is_some()
            || t.riding
            || g.allied(t.owner, u.owner)
            || dist(u.at(), self.hull_point(u.at(), t)) > range
            || !g.players.get(u.owner).is_some_and(|p| p.sees(t.id))
        {
            return Ok(false);
        }
        if u.air.is_some() {
            return Err("air weapon firing is unsupported".into());
        }
        Ok(if flag(w, "salvo") {
            dist(u.at(), self.hull_point(u.at(), t)) >= n(w, "minRange", f64::NAN)
        } else {
            let at = self.aim_point(u, t);
            vision::los(g, &self.rules, (u.x, u.z), (at.x, at.z))
        })
    }
}

impl FiringState {
    pub fn target_claims(&self) -> impl Iterator<Item = ((UnitId, i32), f64)> + '_ {
        self.claims.iter().map(|(key, value)| (*key, *value))
    }

    pub fn take_events(&mut self) -> Vec<LaunchEvent> {
        std::mem::take(&mut self.events)
    }
    /// Initialize once. Repeated calls preserve the current target and timers.
    pub fn initialize(&mut self, id: UnitId) -> &mut FiringUnit {
        self.units.entry(id).or_default()
    }

    pub fn get(&self, id: UnitId) -> Option<&FiringUnit> {
        self.units.get(&id)
    }

    pub fn get_mut(&mut self, id: UnitId) -> Option<&mut FiringUnit> {
        self.units.get_mut(&id)
    }

    pub fn remove(&mut self, id: UnitId) -> Option<FiringUnit> {
        self.units.remove(&id)
    }
}
