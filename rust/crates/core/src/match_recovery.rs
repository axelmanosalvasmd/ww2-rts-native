//! Recovery metadata only. Game owns live health, and LogisticsState owns physical provisions.
use crate::grid::UnitId;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RecoveryUnit {
    pub hp_was: Option<f64>,
    pub wound_at: Option<i64>,
    pub heal_at: Option<i64>,
    pub reinforcement: f64,
}
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RecoveryState {
    units: BTreeMap<UnitId, RecoveryUnit>,
}
impl RecoveryState {
    pub fn initialize(&mut self, id: UnitId) -> &mut RecoveryUnit {
        self.units.entry(id).or_default()
    }
    pub fn get(&self, id: UnitId) -> Option<&RecoveryUnit> {
        self.units.get(&id)
    }
    pub fn get_mut(&mut self, id: UnitId) -> Option<&mut RecoveryUnit> {
        self.units.get_mut(&id)
    }
    pub fn remove(&mut self, id: UnitId) -> Option<RecoveryUnit> {
        self.units.remove(&id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (UnitId, &RecoveryUnit)> {
        self.units.iter().map(|(id, state)| (*id, state))
    }
}

use crate::{
    catalog::{Catalog, Value},
    game::{CELL, Point},
    match_runtime::Match,
    match_story::{StoryChange, StoryKey, prepare_story_updates},
    math,
    world::{Appearance, CellChange, WorldStructure},
};
use std::collections::BTreeSet;

fn def(m: &Match, id: UnitId) -> Result<&'static Value, String> {
    let unit = m.world.game.units.get(&id).ok_or("missing recovery unit")?;
    let kind = m
        .rules
        .kinds
        .get(unit.kind)
        .ok_or("unknown recovery kind")?;
    Catalog::compiled()
        .map_err(|e| e.to_string())?
        .unit(&kind.name)
        .map_err(|e| e.to_string())
}
fn number(value: &Value, key: &str) -> Result<f64, String> {
    value
        .get(key)
        .and_then(Value::number)
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("missing recovery rule {key}"))
}
fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::boolean).unwrap_or(false)
}
fn dist(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}

fn supported(m: &Match, structure: &WorldStructure) -> Result<BTreeSet<String>, String> {
    let mut live = BTreeSet::new();
    let mut links: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut supported = BTreeSet::new();
    let mut queue = Vec::new();
    for id in &structure.section_ids {
        let (meta, hp) = m.world.section_by_id(id).ok_or("missing repair section")?;
        let max_hp = hp.max_hp.ok_or("missing authoritative section maximum")?;
        if !hp.hp.is_finite() || !max_hp.is_finite() || max_hp <= 0. || hp.hp < 0. || hp.hp > max_hp
        {
            return Err("invalid repair section health".into());
        }
        if !hp.failed && hp.hp > 0. {
            live.insert(id.clone());
            links.insert(id.clone(), Vec::new());
            if meta.anchor {
                supported.insert(id.clone());
                queue.push(id.clone());
            }
        }
    }
    for id in &structure.section_ids {
        if !live.contains(id) {
            continue;
        }
        let (meta, _) = m.world.section_by_id(id).unwrap();
        for other in &meta.supports {
            if live.contains(other) {
                if !links[id].contains(other) {
                    links.get_mut(id).unwrap().push(other.clone());
                }
                if !links[other].contains(id) {
                    links.get_mut(other).unwrap().push(id.clone());
                }
            }
        }
    }
    let mut i = 0;
    while i < queue.len() {
        for next in &links[&queue[i]] {
            if supported.insert(next.clone()) {
                queue.push(next.clone());
            }
        }
        i += 1;
    }
    Ok(supported)
}

impl Match {
    /// Read-only whole-operation admission for callers that stage several crew repairs.
    /// Returns a conservative mutation reservation; no health or terrain is published.
    pub(crate) fn preflight_building_repair(&self, id: UnitId, amount: f64) -> Result<u64, String> {
        if self.options.mode == crate::economy::Mode::Horde {
            self.validate_horde_economy()?;
            self.validate_building_activity_rows()?;
            if !self.commands.units.iter().any(|(worker, control)| {
                control.build_target == Some(id)
                    && self
                        .world
                        .game
                        .units
                        .get(worker)
                        .is_some_and(|unit| unit.build)
            }) {
                return Err("invalid Conquest repair request".into());
            }
        } else if self.validate_actor_mode().is_err()
            || !matches!(
                self.options.mode,
                crate::economy::Mode::Default
                    | crate::economy::Mode::Conquest
                    | crate::economy::Mode::Assault
                    | crate::economy::Mode::Annihilation
                    | crate::economy::Mode::Classic
            )
        {
            return Err("invalid Conquest repair request".into());
        }
        if !amount.is_finite() || amount < 0.0 {
            return Err("invalid Conquest repair request".into());
        }
        let unit = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing repair unit")?;
        let definition = def(self, id)?;
        if !flag(definition, "building") || !unit.hp.is_finite() {
            return Err("invalid repair building".into());
        }
        number(definition, "hpPer")?;
        if unit.hp <= 0.0 {
            return Ok(0);
        }
        let Some(structure) = self
            .world
            .structures
            .iter()
            .find(|s| s.entity_id == Some(id))
        else {
            return Ok(0);
        };
        if structure.cells.len() != structure.section_ids.len()
            || self.world.game.units.values().any(|u| {
                u.kind >= self.rules.kinds.len() || ![u.x, u.z, u.hp].iter().all(|v| v.is_finite())
            })
        {
            return Err("invalid repair footprint/occupancy".into());
        }
        self.world
            .preflight_cell_rows(&structure.cells)
            .map_err(|e| e.to_string())?;
        supported(self, structure)?;
        let count = structure.cells.len() as u64 * 4 + 1;
        self.world
            .mutation_capacity(count)
            .map_err(|e| e.to_string())?;
        self.world
            .journal_end()
            .map_err(|e| e.to_string())?
            .checked_add(count)
            .ok_or("repair journal capacity exhausted")?;
        Ok(count)
    }

    /// Repair existing sections in source order. Dead buildings cannot be restored.
    pub fn repair_building_sections(&mut self, id: UnitId, amount: f64) -> Result<f64, String> {
        self.preflight_building_repair(id, amount)?;
        let unit = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing repair unit")?;
        let definition = def(self, id)?;
        if !flag(definition, "building") || !unit.hp.is_finite() {
            return Err("invalid repair building".into());
        }
        if unit.hp <= 0. {
            return Ok(0.);
        }
        let full = number(definition, "hpPer")?;
        let Some(structure) = self
            .world
            .structures
            .iter()
            .find(|s| s.entity_id == Some(id))
            .cloned()
        else {
            let add = math::min(amount, full - unit.hp);
            self.world.game.units.get_mut(&id).unwrap().hp += add;
            return Ok(add);
        };
        if structure.cells.len() != structure.section_ids.len()
            || structure
                .cells
                .iter()
                .any(|c| *c >= self.world.game.chars.len())
        {
            return Err("invalid repair footprint".into());
        }
        if self
            .world
            .game
            .objects
            .as_ref()
            .is_none_or(|objects| objects.len() != self.world.game.chars.len())
            || self.world.game.units.values().any(|u| {
                u.kind >= self.rules.kinds.len() || ![u.x, u.z, u.hp].iter().all(|v| v.is_finite())
            })
        {
            return Err("invalid repair occupancy/layers".into());
        }
        let supported = supported(self, &structure)?;
        self.world
            .mutation_capacity(structure.cells.len() as u64 * 4 + 1)
            .map_err(|e| e.to_string())?;
        let max_radius = self
            .rules
            .kinds
            .iter()
            .map(|k| k.radius)
            .fold(0., math::max);
        let mut left = math::min(amount, full - unit.hp);
        let mut repaired = 0.;
        for (section_id, &cell) in structure.section_ids.iter().zip(&structure.cells) {
            if left <= 0. {
                break;
            }
            let (meta, hp) = self.world.section_by_id(section_id).unwrap();
            let failed = hp.failed;
            if failed {
                let at = Point {
                    x: (cell % self.world.game.w) as f64 * CELL + CELL / 2.,
                    z: (cell / self.world.game.w) as f64 * CELL + CELL / 2.,
                };
                let occupied = self.grid.has_heavy(
                    &self.world.game.units,
                    (at.x, at.z),
                    max_radius + CELL,
                    |_, unit| {
                        let kind = &self.rules.kinds[unit.kind];
                        unit.hp > 0.
                            && unit.air.is_none()
                            && !unit.riding
                            && !kind.infantry
                            && !kind.structure
                            && (unit.x - at.x).abs() < kind.radius + CELL / 2.
                            && (unit.z - at.z).abs() < kind.radius + CELL / 2.
                    },
                );
                if !meta.anchor && !meta.supports.iter().any(|s| supported.contains(s))
                    || occupied
                    || !b".R".contains(
                        &self
                            .world
                            .game
                            .objects
                            .as_ref()
                            .ok_or("missing object layer")?[cell],
                    )
                {
                    continue;
                }
            }
            let add = math::min(left, hp.max_hp.unwrap() - hp.hp);
            if add <= 0. {
                continue;
            }
            if failed {
                let (meta, hp) = self.world.section_by_id_mut(section_id).unwrap();
                hp.failed = false;
                meta.appearance = Appearance::Damaged;
                meta.rubble.clear();
                self.world
                    .mutate_cell(
                        cell,
                        CellChange {
                            object: Some(b'K'),
                            ..CellChange::default()
                        },
                    )
                    .map_err(|e| e.to_string())?;
            }
            let burning = self.world.fires.contains_key(&cell);
            let hp = {
                let (meta, hp) = self.world.section_by_id_mut(section_id).unwrap();
                hp.hp += add;
                meta.appearance = if burning {
                    Appearance::Burning
                } else if hp.hp >= hp.max_hp.unwrap() {
                    Appearance::Intact
                } else {
                    Appearance::Damaged
                };
                hp.hp
            };
            left -= add;
            repaired += add;
            self.world.game.cell_hp[cell] = hp as f32;
            self.world.log_cell(cell, true).map_err(|e| e.to_string())?;
        }
        self.world.game.units.get_mut(&id).unwrap().hp += repaired;
        if structure.section_ids.iter().all(|id| {
            let (_, hp) = self.world.section_by_id(id).unwrap();
            hp.hp >= hp.max_hp.unwrap()
        }) {
            self.world.game.units.get_mut(&id).unwrap().hp = full;
        }
        Ok(repaired)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryFrame {
    tick: i64,
    units: Vec<UnitId>,
    bases: Vec<UnitId>,
    depots: Vec<(usize, Point)>,
    tents: Vec<(usize, Point)>,
    tracks: Vec<UnitId>,
}
impl RecoveryFrame {
    pub fn units(&self) -> &[UnitId] {
        &self.units
    }
}
/// A medic route installed during this recovery pass, in source order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MedicRoute {
    pub unit: UnitId,
    pub goal: Point,
}
fn config() -> Result<&'static Value, String> {
    Catalog::compiled()
        .map_err(|e| e.to_string())?
        .table("CFG")
        .map_err(|e| e.to_string())
}
impl Match {
    fn validate_recovery_ids(&self, ids: &[UnitId]) -> Result<(), String> {
        self.validate_actor_mode()?;
        if !(0..9_007_199_254_740_351).contains(&self.world.game.tick) {
            return Err("invalid recovery clock".into());
        }
        let mut seen = BTreeSet::new();
        for &id in ids {
            if !seen.insert(id) {
                return Err("duplicate recovery unit".into());
            }
            let unit = self
                .world
                .game
                .units
                .get(&id)
                .ok_or("missing captured recovery unit")?;
            if unit.id != id
                || unit.kind >= self.rules.kinds.len()
                || ![
                    unit.x,
                    unit.z,
                    unit.hp,
                    unit.still.unwrap_or(0.),
                    unit.built.unwrap_or(0.),
                ]
                .iter()
                .all(|v| v.is_finite())
            {
                return Err("invalid captured recovery unit".into());
            }
            self.actor_policy(unit.owner)?;
            let row = self.recovery.get(id).ok_or("missing recovery metadata")?;
            if !row.reinforcement.is_finite() || row.hp_was.is_some_and(|hp| !hp.is_finite()) {
                return Err("invalid recovery metadata".into());
            }
            if let Some(reserves) = self.logistics.get(id)
                && (!reserves.provisions.is_finite() || reserves.provisions < 0.)
            {
                return Err("invalid recovery provisions".into());
            }
            if !self.commands.units.contains_key(&id) {
                return Err("missing recovery control".into());
            }
            let definition = def(self, id)?;
            if number(definition, "models")? <= 0. || number(definition, "hpPer")? <= 0. {
                return Err("invalid recovery definition".into());
            }
        }
        Ok(())
    }
    fn check_recovery_frame(&self, frame: &RecoveryFrame, dt: f64) -> Result<(), String> {
        if frame.tick != self.world.game.tick || !dt.is_finite() || !(0.0..=60.).contains(&dt) {
            return Err("invalid recovery frame/time".into());
        }
        self.validate_recovery_ids(&frame.units)?;
        // Nearby patients can include units born after the captured list. Validate their
        // canonical rows without adding them to this phase's outer iteration.
        let live = self.world.game.units.keys().copied().collect::<Vec<_>>();
        self.validate_recovery_ids(&live)?;
        if self
            .world
            .game
            .players
            .iter()
            .any(|p| !(0..=5).contains(&p.team))
            || self.world.mines.iter().any(|(c, owner)| {
                *c >= self.world.game.chars.len()
                    || *owner >= 0 && *owner as usize >= self.world.game.players.len()
            })
        {
            return Err("invalid recovery mine ownership".into());
        }
        if !self.world.mines.is_empty() {
            self.world
                .mutation_capacity((self.world.game.chars.len() * frame.units.len()) as u64 + 1)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    /// Capture membership before healing and the supply/network refresh. Later births are excluded.
    pub fn prepare_recovery(&self, ids: &[UnitId]) -> Result<RecoveryFrame, String> {
        self.validate_recovery_ids(ids)?;
        let mut frame = RecoveryFrame {
            tick: self.world.game.tick,
            units: ids.to_vec(),
            bases: Vec::new(),
            depots: Vec::new(),
            tents: Vec::new(),
            tracks: Vec::new(),
        };
        for &id in ids {
            let u = &self.world.game.units[&id];
            let base = if self.options.mode.is_construction() {
                flag(def(self, id)?, "produces")
            } else {
                self.rules.kinds[u.kind].name == "hq"
            };
            if base && u.built.unwrap_or(0.) >= 1. && u.hp > 0. {
                frame.bases.push(id);
            }
            if flag(def(self, id)?, "reinforces") && u.hp > 0. && u.still.unwrap_or(0.) >= 2. {
                frame.tracks.push(id);
            }
        }
        for point in &self.start.points {
            if point.kind.as_deref() == Some("depot")
                && !point.cut
                && let Some(owner) = point.owner
            {
                if owner >= self.world.game.players.len()
                    || !point.at.x.is_finite()
                    || !point.at.z.is_finite()
                {
                    return Err("invalid recovery depot".into());
                }
                frame.depots.push((owner, point.at));
            }
        }
        for (&cell, &owner) in self.world.aid.iter() {
            if cell >= self.world.game.chars.len() {
                return Err("invalid recovery aid".into());
            }
            self.actor_policy(owner)?;
            frame.tents.push((
                owner,
                Point {
                    x: (cell % self.world.game.w) as f64 * CELL + CELL / 2.,
                    z: (cell / self.world.game.w) as f64 * CELL + CELL / 2.,
                },
            ));
        }
        Ok(frame)
    }
    /// The source's cadence-ten healing and mine discovery, including immediate medic routes.
    pub fn heal_and_sweep(
        &mut self,
        frame: &RecoveryFrame,
        dt: f64,
    ) -> Result<Vec<MedicRoute>, String> {
        self.check_recovery_frame(frame, dt)?;
        if self.world.game.tick % 10 != 0 {
            return Ok(Vec::new());
        }
        let routes = self.heal_wounded(&frame.units, dt * 10.)?;
        self.sweep_prepared_mines(frame)?;
        Ok(routes)
    }
    /// The original medic pass, called with an explicit captured unit list and elapsed time.
    pub fn heal_wounded(&mut self, ids: &[UnitId], dt: f64) -> Result<Vec<MedicRoute>, String> {
        let frame = self.prepare_recovery(ids)?;
        self.check_recovery_frame(&frame, dt)?;
        let cfg = config()?;
        let aid = cfg.get("aid").ok_or("missing recovery aid rules")?;
        let medics = frame
            .units
            .iter()
            .copied()
            .filter(|id| {
                let u = &self.world.game.units[id];
                flag(def(self, *id).unwrap(), "medic") && u.hp > 0. && !u.retreating && !u.riding
            })
            .collect::<Vec<_>>();
        // Route identities must have headroom before any healing or wound metadata changes.
        if medics.iter().any(|id| {
            let u = &self.world.game.units[id];
            let c = &self.commands.units[id];
            u.path_gen == u64::MAX
                && u.path.is_empty()
                && u.amove.is_none()
                && u.garrison.is_none()
                && c.enter.is_none()
                && u.board == 0
        }) {
            return Err("medic path identity exhausted".into());
        }
        let seek = number(aid, "seek")?;
        let reach = number(aid, "medic")?;
        let calm = number(aid, "calm")?;
        let hot = number(aid, "hot")?;
        let heal = number(aid, "heal")?;
        let hurt = number(aid, "hurt")?;
        let mut routes = Vec::new();
        if !medics.is_empty() {
            for &id in &frame.units {
                let u = &self.world.game.units[&id];
                if self.rules.kinds[u.kind].infantry {
                    let row = self.recovery.get_mut(id).unwrap();
                    if u.hp < row.hp_was.unwrap_or(u.hp) {
                        row.wound_at = Some(self.world.game.tick);
                    }
                    row.hp_was = Some(u.hp);
                }
            }
            for id in medics {
                let m = &self.world.game.units[&id];
                let control = &self.commands.units[&id];
                let idle = m.path.is_empty()
                    && m.amove.is_none()
                    && m.garrison.is_none()
                    && control.enter.is_none()
                    && m.board == 0;
                let mut best = None;
                let mut worst = 1.;
                let mut far = None;
                let mut far_worst = hurt;
                for other in self.grid.unordered(
                    &self.world.game.units,
                    (m.x, m.z),
                    if idle { seek } else { reach },
                    |_, _| true,
                ) {
                    let u = &self.world.game.units[&other];
                    let definition = def(self, other)?;
                    if other == id
                        || self.commands.units[&other].operative
                        || !self.rules.kinds[u.kind].infantry
                        || u.hp <= 0.
                        || u.riding
                        || u.retreating
                        || !self.world.game.allied(u.owner, m.owner)
                    {
                        continue;
                    }
                    let fraction =
                        u.hp / (number(definition, "models")? * number(definition, "hpPer")?);
                    let d = dist(u.at(), m.at());
                    if d <= reach && fraction < worst {
                        best = Some(other);
                        worst = fraction;
                    } else if d <= seek && fraction < far_worst {
                        far = Some(other);
                        far_worst = fraction;
                    }
                }
                if let Some(best) = best {
                    let u = &self.world.game.units[&best];
                    let definition = def(self, best)?;
                    let rate = if self.world.game.tick as f64
                        - self
                            .recovery
                            .get(best)
                            .unwrap()
                            .wound_at
                            .map_or(-1e9, |t| t as f64)
                        >= calm / 0.05
                    {
                        1.
                    } else {
                        hot
                    };
                    let amount = math::min(
                        number(definition, "models")? * number(definition, "hpPer")? - u.hp,
                        math::min(
                            heal * rate
                                * dt
                                * self.logistics.recovery_rate(best)
                                * self.logistics.recovery_rate(id),
                            self.logistics
                                .get(id)
                                .map_or(f64::INFINITY, |r| r.provisions * 5.),
                        ),
                    );
                    if let Some(reserves) = self.logistics.get_mut(id) {
                        reserves.provisions = math::max(0., reserves.provisions - amount / 5.);
                    }
                    let u = self.world.game.units.get_mut(&best).unwrap();
                    u.hp += amount;
                    let row = self.recovery.get_mut(best).unwrap();
                    row.hp_was = Some(u.hp);
                    row.heal_at = Some(self.world.game.tick);
                } else if idle && let Some(far) = far {
                    let goal = self.world.game.units[&far].at();
                    let medic = self.world.game.units[&id].clone();
                    let result = self.command_route(medic.owner, &medic, goal)?;
                    let unit = self.world.game.units.get_mut(&id).unwrap();
                    unit.path = result.waypoints;
                    unit.path_gen += 1;
                    self.commands.units.get_mut(&id).unwrap().route_end = Some(result.route_end);
                    routes.push(MedicRoute { unit: id, goal });
                }
            }
        }
        Ok(routes)
    }
    pub fn sweep_mines(&mut self, ids: &[UnitId]) -> Result<(), String> {
        let frame = self.prepare_recovery(ids)?;
        self.check_recovery_frame(&frame, 0.)?;
        self.sweep_prepared_mines(&frame)
    }
    fn recovery_home(&self, frame: &RecoveryFrame, id: UnitId) -> Result<bool, String> {
        let u = &self.world.game.units[&id];
        let d = def(self, id)?;
        let full = number(d, "models")? * number(d, "hpPer")?;
        let home_radius = number(config()?, "reinforceRadius")?;
        let home = !flag(d, "structure")
            && !flag(d, "air")
            && u.hp > 0.
            && u.hp < full
            && ((self.options.mode == crate::economy::Mode::Tutorial
                && dist(u.at(), self.start.players[u.owner].spawn) <= home_radius)
                || frame.bases.iter().any(|base| {
                    let b = &self.world.game.units[base];
                    (if self.options.mode.is_construction() {
                        self.world.game.allied(b.owner, u.owner)
                    } else {
                        b.owner == u.owner
                    }) && dist(u.at(), b.at()) <= home_radius
                })
                || frame.depots.iter().any(|(owner, at)| {
                    self.world.game.allied(*owner, u.owner) && dist(u.at(), *at) <= home_radius
                }));
        Ok(home)
    }
    /// Run after point supply and network refresh, using the earlier captured base/aid membership.
    pub fn reinforce(&mut self, frame: &RecoveryFrame, dt: f64) -> Result<(), String> {
        self.check_recovery_frame(frame, dt)?;
        self.validate_match_finish_state()?;
        self.validate_support_provisions()?;
        if self.start.economy.players.iter().any(|p| !p.mp.is_finite()) {
            return Err("invalid recovery wallet".into());
        }
        let cfg = config()?;
        let every = number(cfg, "reinforceEvery")?;
        let aid = cfg.get("aid").ok_or("missing recovery aid rules")?;
        let slow = number(aid, "slow")?;
        let aid_radius = number(aid, "radius")?;
        let carrier_radius = number(aid, "carrier")?;
        // Provision hooks can decline a charge. Simulate the positive upper bounds on only
        // the small story vector, so later accepted spending cannot fail after stock changes.
        let mut changes = Vec::new();
        for &id in &frame.units {
            let u = &self.world.game.units[&id];
            let d = def(self, id)?;
            let models = number(d, "models")?;
            let full = models * number(d, "hpPer")?;
            let cost = if self.rules.kinds[u.kind].infantry {
                number(d, "cost")? / models * 0.5
            } else {
                math::max(0., full - u.hp) * 0.5
            };
            if !full.is_finite()
                || !cost.is_finite()
                || cost < 0.
                || !(self.recovery.get(id).unwrap().reinforcement
                    + dt * self.logistics.recovery_rate(id))
                .is_finite()
            {
                return Err("invalid recovery result".into());
            }
            changes.push(StoryChange {
                slot: self.actor_policy(u.owner)?.economy_slot,
                key: StoryKey::MpSpent,
                amount: cost,
            });
        }
        let mut story = self.story.clone();
        prepare_story_updates(&mut story, self.finish.is_some(), &changes)?.commit();
        for &id in &frame.units {
            let u = &self.world.game.units[&id];
            if self.commands.units[&id].operative {
                continue;
            }
            if self
                .tutorial_state()
                .is_some_and(|state| state.enemy() == u.owner)
            {
                self.recovery.get_mut(id).unwrap().reinforcement = 0.0;
                continue;
            }
            let d = def(self, id)?;
            let models = number(d, "models")?;
            let hp_per = number(d, "hpPer")?;
            let full = models * hp_per;
            let infantry = self.rules.kinds[u.kind].infantry;
            let home = self.recovery_home(frame, id)?;
            let forward = infantry
                && (frame.tents.iter().any(|(owner, at)| {
                    self.world.game.allied(*owner, u.owner) && dist(u.at(), *at) <= aid_radius
                }) || frame.tracks.iter().any(|track| {
                    let h = &self.world.game.units[track];
                    self.world.game.allied(h.owner, u.owner)
                        && dist(u.at(), h.at()) <= carrier_radius
                }));
            if !home && !(u.hp > 0. && u.hp < full && forward) {
                self.recovery.get_mut(id).unwrap().reinforcement = 0.;
                continue;
            }
            let owner = u.owner;
            let economy_slot = self.actor_policy(owner)?.economy_slot;
            let hp = u.hp;
            let rate = self.logistics.recovery_rate(id);
            if infantry {
                let cost = number(d, "cost")? / models * 0.5;
                let timer = &mut self.recovery.get_mut(id).unwrap().reinforcement;
                *timer += dt * rate;
                if *timer >= every * if home { 1. } else { slow }
                    && economy_slot.is_some_and(|slot| self.start.economy.players[slot].mp >= cost)
                    && self.spend_support_provisions(id, economy_slot.unwrap(), 5.)?
                {
                    self.recovery.get_mut(id).unwrap().reinforcement = 0.;
                    let slot = economy_slot.unwrap();
                    self.start.economy.players[slot].mp -= cost;
                    self.record_spending(slot, cost)?;
                    self.world.game.units.get_mut(&id).unwrap().hp =
                        math::min(full, ((hp / hp_per).ceil() + 1.) * hp_per);
                }
            } else {
                let add = math::min(full - hp, 18. * dt * rate);
                let cost = add * 0.5;
                if add > 0.
                    && economy_slot.is_some_and(|slot| self.start.economy.players[slot].mp >= cost)
                    && self.spend_support_provisions(id, economy_slot.unwrap(), add / 5.)?
                {
                    self.world.game.units.get_mut(&id).unwrap().hp += add;
                    let slot = economy_slot.unwrap();
                    self.start.economy.players[slot].mp -= cost;
                    self.recovery.get_mut(id).unwrap().reinforcement = 1.;
                    self.record_spending(slot, cost)?;
                }
            }
        }
        Ok(())
    }
    fn sweep_prepared_mines(&mut self, frame: &RecoveryFrame) -> Result<(), String> {
        if self.world.mines.is_empty() {
            return Ok(());
        }
        let cfg = config()?;
        let mines = cfg.get("mine").ok_or("missing mine rules")?;
        let detect = number(mines, "detect")?;
        let sweep = number(mines, "sweep")?;
        let builders = cfg
            .get("fortBuilders")
            .and_then(Value::array)
            .ok_or("missing mine builders")?;
        self.world
            .mutation_capacity((self.world.game.chars.len() * frame.units.len()) as u64 + 1)
            .map_err(|e| e.to_string())?;
        let radius = (detect / CELL).ceil() as i64;
        for &id in &frame.units {
            let unit = &self.world.game.units[&id];
            let kind = &self.rules.kinds[unit.kind].name;
            if unit.hp <= 0.
                || unit.riding
                || !builders.iter().any(|v| v.string() == Some(kind.as_str()))
                || kind != "engineer" && unit.still.unwrap_or(0.) < sweep
            {
                continue;
            }
            let team = self.actor_policy(unit.owner)?.team;
            if !(0..=5).contains(&team) && team != -1 {
                return Err("invalid mine discovery team".into());
            }
            // Source JavaScript masks the signed team shift to five bits.
            let bit = 1u32 << ((team as u32) & 31);
            let at = unit.at();
            let cx = (at.x / CELL).floor() as i64;
            let cz = (at.z / CELL).floor() as i64;
            for z in (cz - radius).max(0)..=(cz + radius).min(self.world.game.h as i64 - 1) {
                for x in (cx - radius).max(0)..=(cx + radius).min(self.world.game.w as i64 - 1) {
                    let c = z as usize * self.world.game.w + x as usize;
                    let known = self.world.mines.get(&c).is_some_and(|by| {
                        *by >= 0 && self.world.game.players[*by as usize].team == team
                    }) || self.world.mine_seen.get(&c).copied().unwrap_or(0) & bit != 0;
                    let center = Point {
                        x: x as f64 * CELL + CELL / 2.,
                        z: z as f64 * CELL + CELL / 2.,
                    };
                    if self.world.game.chars[c] == b'N' && !known && dist(at, center) <= detect {
                        let seen = self.world.mine_seen.get(&c).copied().unwrap_or(0) | bit;
                        self.world.mine_seen.insert(c, seen);
                        self.world.log_cell(c, false).map_err(|e| e.to_string())?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "interface_collection_keys.rs"]
mod interface_collection_keys;
