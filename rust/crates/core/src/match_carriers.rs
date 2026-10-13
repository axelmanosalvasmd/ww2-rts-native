//! Ordinary ground carrier commands and physical boarding using canonical Match owners.
//! Boat/rail, garrison exit and wrecked-carrier ejection remain explicit boundaries.

use crate::{
    catalog::Catalog,
    economy::Mode,
    game::{CELL, MOVE, Point},
    grid::UnitId,
    match_commands::{Command, Denial, MoveOutcome},
    match_runtime::Match,
    math,
    navigation::TerrainView,
    unit_spawn::SpawnPlacer,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BoardingOutcome {
    Idle,
    Cleared,
    Boarded {
        carrier: UnitId,
    },
    /// The command scheduler must enqueue the existing Board request with this live goal.
    NeedsPath {
        carrier: UnitId,
        at: Point,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RidingOutcome {
    OnFoot,
    /// Source early-unit-loop continuation: skip ordinary timers, movement and fire.
    Riding,
    /// A missing/dead carrier needs the unimplemented thrown exit, not ordinary unload.
    UnsupportedEjection,
}
fn view(m: &Match) -> TerrainView<'_> {
    let g = &m.world.game;
    TerrainView {
        width: g.w,
        height: g.h,
        flags: &g.flags,
        heights: g.height.as_deref(),
        world_known: false,
        knowledge_id: 0,
        versions: Default::default(),
        changed: None,
        changes: &[],
    }
}
impl Match {
    fn validate_carrier_world(&self) -> Result<(), String> {
        let g = &self.world.game;
        if !matches!(self.options.mode, Mode::Default | Mode::Conquest) || g.mode.is_some() {
            return Err("carrier execution requires ordinary Conquest".into());
        }
        if g.w == 0
            || g.h == 0
            || g.w.checked_mul(g.h) != Some(g.flags.len())
            || g.chars.len() != g.flags.len()
            || g.height.as_ref().is_some_and(|h| h.len() != g.flags.len())
            || !(0..=9_007_199_254_740_991).contains(&g.tick)
        {
            return Err("invalid carrier world shape/tick".into());
        }
        Ok(())
    }
    fn validate_carrier_unit(&self, id: UnitId) -> Result<(), String> {
        let g = &self.world.game;
        let u = g.units.get(&id).ok_or("missing carrier unit")?;
        let kind = self
            .rules
            .kinds
            .get(u.kind)
            .ok_or("unknown carrier unit kind")?;
        let meta = self
            .combat
            .get(id)
            .ok_or("missing carrier combat metadata")?;
        if id == 0
            || u.id != id
            || u.owner >= g.players.len()
            || ![u.x, u.z, u.rot, u.hp, u.supp, u.repath]
                .iter()
                .all(|v| v.is_finite())
            || (!kind.air && g.cell_of(u.x, u.z).is_none())
            || u.supp < 0.
            || meta.carrier.is_some() != u.riding
            || meta.carrier == Some(0)
            || !self.commands.units.contains_key(&id)
            || self.firing.get(id).is_none()
            || u.dig != self.field_works.is_active(id)
        {
            return Err("invalid canonical carrier unit".into());
        }
        if meta.cargo.len() > 4096 {
            return Err("carrier cargo resource bound exceeded".into());
        }
        let unique: BTreeSet<_> = meta.cargo.iter().copied().collect();
        if unique.len() != meta.cargo.len() || unique.contains(&0) || unique.contains(&id) {
            return Err("invalid carrier cargo identities".into());
        }
        Ok(())
    }
    fn validate_carriers(&self) -> Result<(), String> {
        self.validate_carrier_world()?;
        for &id in self.world.game.units.keys() {
            self.validate_carrier_unit(id)?;
        }
        Ok(())
    }
    fn carrier_seats(&self, id: UnitId) -> Result<usize, String> {
        let u = &self.world.game.units[&id];
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let n = catalog
            .unit(&self.rules.kinds[u.kind].name)
            .map_err(|e| e.to_string())?
            .get("carries")
            .and_then(|v| v.number())
            .unwrap_or(0.);
        if !n.is_finite() || n < 0. || n != f64::from(n as u32) || n > 4096. {
            return Err("invalid carrier seat definition".into());
        }
        Ok(n as usize)
    }
    fn carrier_boundary(&self, id: UnitId) -> bool {
        let u = &self.world.game.units[&id];
        let k = &self.rules.kinds[u.kind];
        k.naval || k.rail || k.air || u.rail || u.air.is_some() || u.garrison.is_some()
    }
    fn carrier_command_open(&self, owner: usize, winner_decided: bool) -> bool {
        self.start
            .economy
            .players
            .get(owner)
            .is_some_and(|p| !p.out && !p.away)
            && !winner_decided
            && self.finish.is_none()
    }
    /// Source board selection and seat reservations. The input order is recorded intent;
    /// stable equal-distance pairs retain squad/carrier selection order, not grid history.
    pub fn carrier_board_command(
        &mut self,
        owner: usize,
        ids: &[i64],
        target: Option<i64>,
        winner_decided: bool,
    ) -> Result<Option<Denial>, String> {
        self.validate_carriers()?;
        if !self.carrier_command_open(owner, winner_decided) {
            return Ok(Some(Denial::Blocked));
        }
        if ids
            .iter()
            .any(|id| id.unsigned_abs() > 9_007_199_254_740_991)
            || target.is_some_and(|id| id.unsigned_abs() > 9_007_199_254_740_991)
        {
            return Ok(Some(Denial::Blocked));
        }
        let rows = &ids[..ids.len().min(50.max(self.world.game.units.len()))];
        let carriers = if let Some(raw) = target {
            let Some(id) = u32::try_from(raw)
                .ok()
                .filter(|id| self.world.game.units.contains_key(id))
            else {
                return Ok(Some(Denial::Unseen));
            };
            let u = &self.world.game.units[&id];
            if u.owner != owner || u.hp <= 0. || self.carrier_seats(id)? == 0 {
                return Ok(Some(Denial::Unseen));
            }
            if self.carrier_boundary(id) {
                return Ok(Some(Denial::Unsupported));
            }
            vec![id]
        } else {
            let mut carriers = Vec::new();
            for &raw in rows {
                if let Ok(id) = u32::try_from(raw)
                    && let Some(u) = self.world.game.units.get(&id)
                    && u.owner == owner
                    && u.hp > 0.
                    && self.carrier_seats(id)? > 0
                    && !self.rules.kinds[u.kind].naval
                {
                    if self.carrier_boundary(id) {
                        return Ok(Some(Denial::Unsupported));
                    }
                    carriers.push(id);
                }
            }
            carriers
        };
        if carriers.is_empty() {
            return Ok(Some(Denial::Needs));
        }
        let eligible: Vec<_> = rows
            .iter()
            .filter_map(|&id| self.eligible_command_unit(id, owner, &Command::Unsupported))
            .collect();
        let squads: Vec<_> = eligible
            .iter()
            .copied()
            .filter(|id| {
                let u = &self.world.game.units[id];
                self.rules.kinds[u.kind].infantry && !u.retreating
            })
            .collect();
        if squads.iter().any(|id| self.carrier_boundary(*id)) {
            return Ok(Some(Denial::Unsupported));
        }
        if squads.iter().any(|id| {
            let u = &self.world.game.units[id];
            u.entrench || u.build
        }) {
            return Ok(Some(Denial::Unsupported));
        }
        // Guard every fallible mutation before the source mine() follow reset or job cancellation.
        let mut free = BTreeMap::new();
        for &id in &carriers {
            let reserved = self
                .world
                .game
                .units
                .values()
                .filter(|u| u.board == id)
                .count();
            let seats = self.carrier_seats(id)? as i64
                - self.combat.get(id).unwrap().cargo.len() as i64
                - reserved as i64;
            free.insert(id, seats);
        }
        let mut pairs = Vec::new();
        for &s in &squads {
            for &c in &carriers {
                let u = &self.world.game.units[&s];
                let carrier = &self.world.game.units[&c];
                pairs.push((s, c, math::hypot(u.x - carrier.x, u.z - carrier.z)));
            }
        }
        crate::sort::sort_by(&mut pairs, |a, b| a.2 - b.2);
        let mut placed = BTreeSet::new();
        let mut assignments = Vec::new();
        for (s, c, _) in pairs {
            if placed.contains(&s) || free[&c] <= 0 {
                continue;
            }
            placed.insert(s);
            *free.get_mut(&c).unwrap() -= 1;
            assignments.push((s, c));
        }
        for &(id, _) in &assignments {
            self.world.game.units[&id]
                .path_gen
                .checked_add(1)
                .ok_or("carrier path identity exhausted")?;
        }
        let mut routes = Vec::new();
        for &(id, c) in &assignments {
            let u = self.world.game.units[&id].clone();
            let at = self.world.game.units[&c].at();
            routes.push(self.command_route(owner, &u, at)?.waypoints);
        }
        for id in &eligible {
            self.commands.units.get_mut(id).unwrap().follow = 0;
        }
        if squads.is_empty() {
            return Ok(Some(
                if eligible
                    .iter()
                    .any(|id| self.world.game.units[id].retreating)
                {
                    Denial::Retreating
                } else {
                    Denial::Needs
                },
            ));
        }
        if !free.values().any(|n| *n > 0) && assignments.is_empty() {
            return Ok(Some(Denial::Maximum));
        }
        for ((id, carrier), path) in assignments.into_iter().zip(routes) {
            self.cancel_field_work(id)?;
            let u = self.world.game.units.get_mut(&id).unwrap();
            u.traffic = None;
            u.traffic_wait = 0.;
            u.world_goal = None;
            u.entrench = false;
            u.attack = false;
            u.dig = false;
            u.board = carrier;
            u.amove = None;
            u.build = false;
            u.repath = 0.;
            u.path = path;
            u.path_gen += 1;
            let c = self.commands.units.get_mut(&id).unwrap();
            c.orders.clear();
            c.face = None;
            c.attack_id = 0;
            c.nade = None;
            c.enter = None;
            c.fire_at = None;
            c.outcome = Some(MoveOutcome::Interrupted);
            c.outcome_tick = Some(self.world.game.tick);
            self.firing.get_mut(id).unwrap().target_id = 0;
        }
        Ok(None)
    }
    /// Source post-ability boarding block. A deferred path request remains explicit for
    /// the command scheduler; this method does not fabricate route completion.
    pub fn apply_unit_boarding(&mut self, id: UnitId) -> Result<BoardingOutcome, String> {
        self.validate_carrier_world()?;
        if self.world.game.units.contains_key(&id) {
            self.validate_carrier_unit(id)?;
        }
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(BoardingOutcome::Idle);
        };
        if u.hp <= 0. || u.board == 0 {
            return Ok(BoardingOutcome::Idle);
        }
        if u.riding
            || !self.rules.kinds[u.kind].infantry
            || self.commands.units[&id].operative
            || self.carrier_boundary(id)
        {
            return Err("unsupported boarding actor".into());
        }
        let carrier = u.board;
        let Some(c) = self.world.game.units.get(&carrier) else {
            self.world.game.units.get_mut(&id).unwrap().board = 0;
            return Ok(BoardingOutcome::Cleared);
        };
        self.validate_carrier_unit(carrier)?;
        if self.carrier_boundary(carrier) {
            return Err("unsupported boarding carrier".into());
        }
        if c.hp <= 0.
            || self.combat.get(carrier).unwrap().cargo.len() >= self.carrier_seats(carrier)?
        {
            self.world.game.units.get_mut(&id).unwrap().board = 0;
            return Ok(BoardingOutcome::Cleared);
        }
        let cfg = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .table("CFG")
            .map_err(|e| e.to_string())?;
        let reach = cfg
            .get("aid")
            .and_then(|v| v.get("board"))
            .and_then(|v| v.number())
            .ok_or("missing boarding reach")?;
        let at = c.at();
        let reachable = self.world.game.flags[self.world.game.cell_of(u.x, u.z).unwrap()] & MOVE
            == 0
            && crate::pathfinding::walkable(view(self), u.at(), at, MOVE, 0.15);
        if math::hypot(u.x - c.x, u.z - c.z) <= reach && reachable {
            u.path_gen
                .checked_add(1)
                .ok_or("carrier path identity exhausted")?;
            if self.combat.get(carrier).unwrap().cargo.contains(&id) {
                return Err("duplicate boarded cargo".into());
            }
            let u = self.world.game.units.get_mut(&id).unwrap();
            u.riding = true;
            u.board = 0;
            u.path.clear();
            u.path_gen += 1;
            u.attack = false;
            self.commands.units.get_mut(&id).unwrap().attack_id = 0;
            self.firing.get_mut(id).unwrap().target_id = 0;
            self.combat.get_mut(id).unwrap().carrier = Some(carrier);
            self.combat.get_mut(carrier).unwrap().cargo.push(id);
            return Ok(BoardingOutcome::Boarded { carrier });
        }
        if u.path.is_empty() && u.repath <= 0. {
            Ok(BoardingOutcome::NeedsPath { carrier, at })
        } else {
            Ok(BoardingOutcome::Idle)
        }
    }
    /// Source early riding branch. Caller skips the remaining unit turn on Riding;
    /// dead-carrier ejection must be handled by the future drop/effect actuator.
    pub fn apply_unit_riding(&mut self, id: UnitId, dt: f64) -> Result<RidingOutcome, String> {
        self.validate_carrier_world()?;
        if self.world.game.units.contains_key(&id) {
            self.validate_carrier_unit(id)?;
        }
        if !dt.is_finite() || !(0.0..=2.5).contains(&dt) {
            return Err("invalid riding step".into());
        }
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(RidingOutcome::OnFoot);
        };
        if u.hp <= 0. {
            return Ok(RidingOutcome::OnFoot);
        }
        let Some(carrier) = self.combat.get(id).unwrap().carrier else {
            if !self.combat.get(id).unwrap().cargo.is_empty() {
                for &rider in &self.combat.get(id).unwrap().cargo {
                    if self.world.game.units.contains_key(&rider) {
                        self.validate_carrier_unit(rider)?;
                    }
                }
                let retained = self
                    .combat
                    .get(id)
                    .unwrap()
                    .cargo
                    .iter()
                    .copied()
                    .filter(|rider| {
                        self.world.game.units.get(rider).is_some_and(|u| u.riding)
                            && self
                                .combat
                                .get(*rider)
                                .is_some_and(|m| m.carrier == Some(id))
                    })
                    .collect();
                self.combat.get_mut(id).unwrap().cargo = retained;
            }
            return Ok(RidingOutcome::OnFoot);
        };
        let Some(c) = self.world.game.units.get(&carrier) else {
            return Ok(RidingOutcome::UnsupportedEjection);
        };
        self.validate_carrier_unit(carrier)?;
        if c.hp <= 0. {
            return Ok(RidingOutcome::UnsupportedEjection);
        }
        if self.carrier_boundary(id) || self.carrier_boundary(carrier) {
            return Err("unsupported riding actor/carrier".into());
        }
        let (x, z, rot) = (c.x, c.z, c.rot);
        let ability = self
            .abilities
            .get(id)
            .ok_or("missing rider ability timer")?;
        if !ability.cooldown.is_finite() {
            return Err("invalid rider ability timer".into());
        }
        if !self.combat.get(carrier).unwrap().cargo.contains(&id) || !u.riding {
            return Err("inconsistent rider relation".into());
        }
        let u = self.world.game.units.get_mut(&id).unwrap();
        u.x = x;
        u.z = z;
        u.rot = rot;
        u.supp = 0.;
        u.still = Some(0.);
        self.firing.get_mut(id).unwrap().target_id = 0;
        self.abilities.get_mut(id).unwrap().cooldown -= dt;
        self.grid.update(id, u);
        Ok(RidingOutcome::Riding)
    }
    /// Manual ground unload. Full ordered placements are staged before relations change;
    /// no thrown damage, shore landing or rail drop is silently substituted.
    pub fn carrier_unload_command(
        &mut self,
        owner: usize,
        ids: &[i64],
        winner_decided: bool,
    ) -> Result<Option<Denial>, String> {
        self.validate_carriers()?;
        if !self.carrier_command_open(owner, winner_decided)
            || ids
                .iter()
                .any(|id| id.unsigned_abs() > 9_007_199_254_740_991)
        {
            return Ok(Some(Denial::Blocked));
        }
        let eligible: Vec<_> = ids
            .iter()
            .take(50.max(self.world.game.units.len()))
            .filter_map(|&id| self.eligible_command_unit(id, owner, &Command::Unsupported))
            .collect();
        let carriers: Vec<_> = eligible
            .iter()
            .copied()
            .filter(|id| !self.combat.get(*id).unwrap().cargo.is_empty())
            .collect();
        if carriers.iter().any(|id| self.carrier_boundary(*id)) {
            return Ok(Some(Denial::Unsupported));
        }
        let mut placer = SpawnPlacer::new();
        let mut drops = Vec::new();
        let mut staged = BTreeSet::new();
        for &carrier in &carriers {
            if !staged.insert(carrier) {
                continue;
            }
            for &id in &self.combat.get(carrier).unwrap().cargo {
                let Some(u) = self.world.game.units.get(&id) else {
                    drops.push((carrier, id, None));
                    continue;
                };
                if self.carrier_boundary(id)
                    || self.combat.get(id).unwrap().carrier != Some(carrier)
                    || !u.riding
                {
                    return Err("unsupported or inconsistent unload rider".into());
                }
                let cell = placer
                    .nearest_free(view(self), u.at(), self.rules.kinds[u.kind].block(), None)
                    .map_err(|e| format!("carrier exit: {e:?}"))?;
                drops.push((
                    carrier,
                    id,
                    Some(Point {
                        x: (cell % self.world.game.w) as f64 * CELL + CELL * 0.5,
                        z: (cell / self.world.game.w) as f64 * CELL + CELL * 0.5,
                    }),
                ));
            }
        }
        for id in &eligible {
            self.commands.units.get_mut(id).unwrap().follow = 0;
        }
        if carriers.is_empty() {
            return Ok(Some(Denial::Needs));
        }
        for (carrier, id, at) in drops {
            self.combat
                .get_mut(carrier)
                .unwrap()
                .cargo
                .retain(|other| *other != id);
            if let Some(at) = at {
                self.combat.get_mut(id).unwrap().carrier = None;
                let u = self.world.game.units.get_mut(&id).unwrap();
                u.riding = false;
                u.x = at.x;
                u.z = at.z;
                self.grid.update(id, u);
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests;
