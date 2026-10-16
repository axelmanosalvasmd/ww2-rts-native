//! Canonical ground ability state. Health, sprint and the aimed point retain their existing owners.
use crate::grid::UnitId;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AbilityUnit {
    /// Source `cd`, allowed to expire below zero.
    pub cooldown: f64,
    pub buff: f64,
    pub ap: bool,
    /// The aimed point itself is `UnitControl::nade`.
    pub aimed_auto: bool,
    pub aimed_walked: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AbilityState {
    units: BTreeMap<UnitId, AbilityUnit>,
}
impl AbilityState {
    pub fn initialize(&mut self, id: UnitId) -> &mut AbilityUnit {
        self.units.entry(id).or_default()
    }
    pub fn get(&self, id: UnitId) -> Option<&AbilityUnit> {
        self.units.get(&id)
    }
    pub fn get_mut(&mut self, id: UnitId) -> Option<&mut AbilityUnit> {
        self.units.get_mut(&id)
    }
    pub fn remove(&mut self, id: UnitId) -> Option<AbilityUnit> {
        self.units.remove(&id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (UnitId, &AbilityUnit)> {
        self.units.iter().map(|(id, state)| (*id, state))
    }
}

use crate::{
    catalog::{Catalog, Value},
    game::{CELL, COVER, MOVE, Point, Smoke, TRENCH},
    match_commands::{Command, Denial, MoveOutcome},
    match_firing::AbilityBlast,
    match_runtime::Match,
    math,
    navigation::{TerrainView, Versions},
    unit_spawn::SpawnPlacer,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbilityKind {
    None,
    Grenade,
    Suppress,
    Ap,
    Smoke,
    Barrage,
    Satchel,
    Ura,
}
impl AbilityKind {
    pub(crate) fn from_definition(definition: &Value) -> Result<Self, String> {
        Ok(
            match definition
                .get("ab")
                .ok_or("missing ability definition")?
                .get("id")
                .and_then(Value::string)
                .ok_or("missing ability identity")?
            {
                "none" => Self::None,
                "grenade" => Self::Grenade,
                "suppress" => Self::Suppress,
                "ap" => Self::Ap,
                "smoke" => Self::Smoke,
                "barrage" => Self::Barrage,
                "satchel" => Self::Satchel,
                "ura" => Self::Ura,
                _ => return Err("unsupported ground ability".into()),
            },
        )
    }
    fn aimed(self) -> bool {
        matches!(self, Self::Grenade | Self::Barrage | Self::Satchel)
    }
    fn ammo(self) -> bool {
        matches!(
            self,
            Self::Grenade | Self::Satchel | Self::Barrage | Self::Ap | Self::Suppress
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AutocastTarget {
    Instant,
    At(Point),
}
fn definition(m: &Match, id: UnitId) -> Result<&'static Value, String> {
    let unit = m.world.game.units.get(&id).ok_or("missing ability unit")?;
    let kind = m.rules.kinds.get(unit.kind).ok_or("unknown ability kind")?;
    Catalog::compiled()
        .map_err(|e| e.to_string())?
        .unit(&kind.name)
        .map_err(|e| e.to_string())
}
fn ability(m: &Match, id: UnitId) -> Result<&'static Value, String> {
    definition(m, id)?
        .get("ab")
        .ok_or_else(|| "missing ability definition".into())
}
fn n(value: &Value, key: &str, fallback: f64) -> f64 {
    value.get(key).and_then(Value::number).unwrap_or(fallback)
}
fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::boolean).unwrap_or(false)
}
fn dist(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
impl Match {
    fn ability_munitions_cost(&self, id: UnitId) -> Result<f64, String> {
        let ab = ability(self, id)?;
        let cost = crate::economy::ab_cost(
            self.options.mode,
            crate::economy::AbilityCost {
                id: ab
                    .get("id")
                    .and_then(Value::string)
                    .ok_or("missing ability ID")?,
                munitions: ab.get("mun").and_then(Value::number),
            },
        )
        .map_err(|e| e.to_string())?;
        if !cost.is_finite() || cost < 0. {
            return Err("invalid ability Munitions cost".into());
        }
        Ok(cost)
    }
    fn ability_munitions_available(&self, owner: usize) -> Result<f64, String> {
        let slot = self.actor_policy(owner)?.economy_slot;
        let balance = slot.map_or(0., |slot| self.start.economy.players[slot].mun);
        if !balance.is_finite() {
            return Err("invalid ability Munitions wallet".into());
        }
        Ok(balance)
    }
    // Both source resources and actor identity are preflighted before any actuator writes.
    fn charge_ability_munitions(&mut self, owner: usize, cost: f64) {
        if cost != 0. {
            self.start.economy.players[owner].mun -= cost;
        }
    }
    pub fn ability_kind(&self, id: UnitId) -> Result<AbilityKind, String> {
        AbilityKind::from_definition(definition(self, id)?)
    }
    pub(crate) fn validate_abilities(&self) -> Result<(), String> {
        if !matches!(
            self.options.mode,
            crate::economy::Mode::Default
                | crate::economy::Mode::Conquest
                | crate::economy::Mode::Tutorial
                | crate::economy::Mode::World
        ) {
            return Err("abilities currently require represented Conquest or World state".into());
        }
        self.validate_actor_mode()?;
        if self.world.game.tick < 0
            || self.world.game.tick as u64
                > 9_007_199_254_740_991u64
                    - u64::from(
                        self.world
                            .game
                            .units
                            .keys()
                            .next_back()
                            .copied()
                            .unwrap_or(0),
                    )
        {
            return Err("abilities require represented actor clock".into());
        }
        for (&id, unit) in &self.world.game.units {
            if id != unit.id
                || unit.kind >= self.rules.kinds.len()
                || self.actor_policy(unit.owner).is_err()
                || ![
                    unit.hp,
                    unit.x,
                    unit.z,
                    unit.rot,
                    unit.supp,
                    unit.sprint,
                    unit.repath,
                ]
                .iter()
                .all(|v| v.is_finite())
            {
                return Err("invalid ability unit".into());
            }
            let row = self.abilities.get(id).ok_or("missing ability row")?;
            if !row.cooldown.is_finite() || !row.buff.is_finite() {
                return Err("invalid ability timer".into());
            }
            let control = self
                .commands
                .units
                .get(&id)
                .ok_or("missing ability control")?;
            crate::match_construction::validate_build_activity(unit, control)?;
            if control
                .nade
                .is_some_and(|p| !p.x.is_finite() || !p.z.is_finite())
            {
                return Err("invalid aimed ability point".into());
            }
            if self.firing.get(id).is_none() {
                return Err("missing ability firing state".into());
            }
            if let Some(r) = self.logistics.get(id)
                && (!r.ammo.is_finite() || r.ammo < 0.)
            {
                return Err("invalid ability ammo".into());
            }
            self.ability_kind(id)?;
            self.ability_munitions_cost(id)?;
            if self.options.mode == crate::economy::Mode::World {
                self.ability_munitions_available(unit.owner)?;
            }
        }
        Ok(())
    }
    /// The ability timers decay beside the firing and Game timers. Sprint stays in Game.
    pub fn advance_ability_timers(&mut self, id: UnitId, dt: f64) -> Result<(), String> {
        self.validate_abilities()?;
        if !dt.is_finite() || !(0.0..=60.).contains(&dt) {
            return Err("invalid ability timestep".into());
        }
        let row = self.abilities.get_mut(id).ok_or("missing ability timer")?;
        row.cooldown -= dt;
        row.buff -= dt;
        Ok(())
    }
    fn ability_foes(&self, id: UnitId, at: Point, radius: f64) -> Result<Vec<UnitId>, String> {
        let u = &self.world.game.units[&id];
        let actor = self.actor_policy(u.owner)?;
        Ok(self
            .grid
            .radius(&self.world.game.units, (at.x, at.z), radius, |_, e| {
                e.hp > 0.
                    && e.air.is_none()
                    && !self.world.game.allied(e.owner, u.owner)
                    && actor.sees(e.id)
            }))
    }
    fn ability_clear(&self, id: UnitId, at: Point, radius: f64) -> bool {
        let owner = self.world.game.units[&id].owner;
        self.grid
            .radius(&self.world.game.units, (at.x, at.z), radius, |_, e| {
                e.hp > 0. && e.air.is_none() && self.world.game.allied(e.owner, owner)
            })
            .is_empty()
    }
    /// Exact ground autocast judgment, using current authoritative visibility and firing geometry.
    pub fn autocast_target(&self, id: UnitId) -> Result<Option<AutocastTarget>, String> {
        self.validate_abilities()?;
        let u = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing autocast unit")?;
        let ab = ability(self, id)?;
        let kind = self.ability_kind(id)?;
        let control = &self.commands.units[&id];
        let row = self.abilities.get(id).unwrap();
        let weapon = definition(self, id)?
            .get("w")
            .ok_or("missing ability weapon")?;
        if control.hold_fire
            && matches!(
                kind,
                AbilityKind::Grenade | AbilityKind::Suppress | AbilityKind::Barrage
            )
        {
            return Ok(None);
        }
        let at = match kind {
            AbilityKind::None => None,
            AbilityKind::Grenade => {
                if u.garrison.is_some() {
                    return Ok(None);
                }
                let mut foes = self.ability_foes(id, u.at(), n(ab, "range", 0.))?;
                foes.retain(|enemy| {
                    let e = &self.world.game.units[enemy];
                    self.rules.kinds[e.kind].infantry
                        && (e.garrison.is_some()
                            || self.world.game.flags_at(e.x, e.z) & (COVER | TRENCH) != 0)
                        && self.ability_clear(id, e.at(), n(ab, "radius", 0.) + 1.)
                });
                foes.sort_by(|a, b| {
                    dist(u.at(), self.world.game.units[a].at())
                        .total_cmp(&dist(u.at(), self.world.game.units[b].at()))
                });
                foes.first()
                    .map(|e| AutocastTarget::At(self.world.game.units[e].at()))
            }
            AbilityKind::Suppress => {
                let mut found = false;
                if u.path.is_empty() {
                    for enemy in self.ability_foes(id, u.at(), n(weapon, "range", 0.))? {
                        let e = &self.world.game.units[&enemy];
                        if self.rules.kinds[e.kind].infantry
                            && !e.path.is_empty()
                            && !e.retreating
                            && math::cos(e.rot - math::atan2(u.z - e.z, u.x - e.x)) > 0.5
                            && self.can_shoot_target(id, enemy)?
                        {
                            found = true;
                            break;
                        }
                    }
                }
                found.then_some(AutocastTarget::Instant)
            }
            AbilityKind::Ap => {
                let target = self.firing.get(id).unwrap().target_id;
                if !row.ap
                    && let Some(t) = self.world.game.units.get(&target)
                {
                    let k = &self.rules.kinds[t.kind];
                    (!k.infantry
                        && !k.structure
                        && (!flag(ab, "naval") || k.naval)
                        && self.can_shoot_target(id, target)?)
                    .then_some(AutocastTarget::Instant)
                } else {
                    None
                }
            }
            AbilityKind::Smoke => (u.hp
                < n(definition(self, id)?, "models", 0.)
                    * n(definition(self, id)?, "hpPer", 0.)
                    * 0.5
                && self.world.game.tick as f64
                    - self
                        .combat
                        .get(id)
                        .ok_or("missing ability hit history")?
                        .at_hit
                        .unwrap_or(-1e9)
                    <= 3. / 0.05)
                .then_some(AutocastTarget::Instant),
            AbilityKind::Ura => {
                (u.supp >= 50. && !u.path.is_empty()).then_some(AutocastTarget::Instant)
            }
            AbilityKind::Barrage => {
                let radius = n(weapon, "spread", 0.) + n(weapon, "blast", 0.);
                let mut best = None;
                let mut most = 0;
                for enemy in self.ability_foes(id, u.at(), n(ab, "range", 0.))? {
                    let e = &self.world.game.units[&enemy];
                    let k = &self.rules.kinds[e.kind];
                    if dist(u.at(), e.at()) < n(weapon, "minRange", 0.) || k.building {
                        continue;
                    }
                    let crowd = self.ability_foes(id, e.at(), radius)?.len();
                    let dug = e.garrison.is_some()
                        || self.rules.kinds[e.kind].infantry
                            && self.world.game.flags_at(e.x, e.z) & TRENCH != 0
                        || k.structure;
                    let score = crowd + if dug { 2 } else { 0 };
                    if (dug || crowd >= 3)
                        && score > most
                        && self.ability_clear(id, e.at(), radius + 1.)
                    {
                        best = Some(e.at());
                        most = score;
                    }
                }
                best.map(AutocastTarget::At)
            }
            AbilityKind::Satchel => {
                if let Some(t) = self.world.game.units.get(&control.attack_id) {
                    let at = self.aim_point(u, t);
                    (u.garrison.is_none()
                        && self.actor_policy(u.owner)?.sees(t.id)
                        && (t.garrison.is_some() || self.rules.kinds[t.kind].structure)
                        && dist(u.at(), at) <= 20.
                        && self.ability_clear(id, at, n(ab, "radius", 0.)))
                    .then_some(AutocastTarget::At(at))
                } else {
                    None
                }
            }
        };
        Ok(at)
    }
    /// Use the normal command actuator on the source's staggered ten-tick schedule.
    pub fn apply_autocast(&mut self, id: UnitId) -> Result<Option<Denial>, String> {
        self.validate_abilities()?;
        let u = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing autocast unit")?;
        let control = &self.commands.units[&id];
        if !control.auto_cast
            || self.abilities.get(id).unwrap().cooldown > 0.
            || control.nade.is_some()
            || u.retreating
            || (self.world.game.tick + i64::from(id)) % 10 != 0
        {
            return Ok(None);
        }
        let owner = u.owner;
        let Some(target) = self.autocast_target(id)? else {
            return Ok(None);
        };
        let at = match target {
            AutocastTarget::Instant => None,
            AutocastTarget::At(at) => Some(at),
        };
        if self.actor_policy(owner)?.economy_slot.is_none() {
            // Source local actors can judge autocast, but their zero Munitions
            // cannot pay for any currently represented nonzero-cost ability.
            self.commands.units.get_mut(&id).unwrap().follow = 0;
            return self.apply_ability_selection(owner, &[Some(id)], at, false, true);
        }
        self.execute_command(
            owner,
            &Command::Ability {
                ids: vec![i64::from(id)],
                at,
                queue: false,
                automatic: true,
            },
            self.finish.is_some(),
        )
    }
    /// Advance the queued ground ability after jobs and before entering/boarding.
    /// Firing retains the committed flights, grenade tickets and ordered launch events.
    pub fn step_aimed_ability(&mut self, id: UnitId) -> Result<(), String> {
        self.validate_abilities()?;
        let u = self.world.game.units.get(&id).ok_or("missing aimed unit")?;
        let control = &self.commands.units[&id];
        if u.hp <= 0. || u.riding || control.operative {
            return Ok(());
        }
        let Some(at) = control.nade else {
            return Ok(());
        };
        let row = *self.abilities.get(id).unwrap();
        let halt = !row.aimed_auto || row.aimed_walked;
        let kind = self.ability_kind(id)?;
        let ab = ability(self, id)?;
        let in_range = dist(u.at(), at) <= n(ab, "range", 0.);
        let w = definition(self, id)?
            .get("w")
            .ok_or("missing ability weapon")?;
        let count =
            if matches!(kind, AbilityKind::Grenade | AbilityKind::Satchel) && flag(w, "perModel") {
                (u.hp / n(definition(self, id)?, "hpPer", 1.)).ceil()
            } else {
                1.
            };
        if kind.aimed() && in_range && !self.logistics.can_fire(id, count) {
            self.clear_aimed_ability(id);
            return Ok(());
        }
        let cost = self.ability_munitions_cost(id)?;
        let owner = u.owner;
        if kind.aimed() && in_range && self.ability_munitions_available(owner)? < cost {
            self.clear_aimed_ability(id);
            return Ok(());
        }
        if kind.aimed() && in_range {
            if halt && u.path_gen == u64::MAX {
                return Err("aimed path identity exhausted".into());
            }
            if kind == AbilityKind::Barrage {
                let requested = ab.get("shells").and_then(Value::number);
                if requested
                    .is_some_and(|n| !n.is_finite() || n <= 0. || n > 4096. || n.floor() != n)
                {
                    return Err("invalid ability salvo count".into());
                }
                self.launch_weapon_salvo(id, at, requested.map(|n| n as u32))?;
                self.firing.get_mut(id).unwrap().cooldown = n(w, "interval", 0.);
            } else {
                if !count.is_finite() || count <= 0. || count > 4096. {
                    return Err("invalid ability ammunition count".into());
                }
                let blast = AbilityBlast::from_unit(u.kind, &self.rules)?;
                self.launch_ability_flight(id, at, blast, None, Some(count as u32))?;
            }
            self.charge_ability_munitions(owner, cost);
            self.clear_aimed_ability(id);
            self.abilities.get_mut(id).unwrap().cooldown = n(ab, "cd", 0.);
            if halt {
                let u = self.world.game.units.get_mut(&id).unwrap();
                u.path.clear();
                u.path_gen += 1;
            }
        } else if self.logistics.get(id).is_some_and(|r| r.forced) {
            self.clear_aimed_ability(id);
        } else if u.repath <= 0. {
            self.request_command_path(id, at, crate::path_requests::RequestKind::Nade)?;
            if self.commands.units[&id].nade.is_some() {
                self.abilities.get_mut(id).unwrap().aimed_walked = true;
            }
        }
        Ok(())
    }
    fn clear_aimed_ability(&mut self, id: UnitId) {
        self.commands.units.get_mut(&id).unwrap().nade = None;
        let row = self.abilities.get_mut(id).unwrap();
        row.aimed_auto = false;
        row.aimed_walked = false;
    }
    pub(crate) fn unrepresented_ability_work(
        &self,
        selected: &[UnitId],
        at: Option<Point>,
        queue: bool,
        automatic: bool,
    ) -> Result<bool, String> {
        if queue || automatic || !at.is_some_and(|p| p.x.is_finite() && p.z.is_finite()) {
            return Ok(false);
        }
        for &id in selected {
            let u = &self.world.game.units[&id];
            crate::match_construction::validate_build_activity(u, &self.commands.units[&id])?;
            let kind = self.ability_kind(id)?;
            if !kind.aimed() || u.retreating || self.abilities.get(id).unwrap().cooldown > 0. {
                continue;
            }
            let d = definition(self, id)?;
            let per_model = d.get("w").is_some_and(|w| flag(w, "perModel"));
            let count = if matches!(kind, AbilityKind::Grenade | AbilityKind::Satchel) && per_model
            {
                (u.hp / n(d, "hpPer", 1.)).ceil()
            } else {
                1.
            };
            if self.logistics.can_fire(id, count)
                && (u.entrench != self.field_works.membership(id).is_some()
                    || u.dig != self.field_works.is_active(id))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(crate) fn preflight_ability_selection(
        &self,
        selected: &[UnitId],
        at: Option<Point>,
        queue: bool,
        automatic: bool,
    ) -> Result<BTreeMap<UnitId, Point>, String> {
        let valid_at = at.is_some_and(|p| p.x.is_finite() && p.z.is_finite());
        let mut increments = BTreeMap::new();
        let mut smoke_ids = std::collections::BTreeSet::new();
        let mut exits = BTreeMap::new();
        for &id in selected {
            let kind = self.ability_kind(id)?;
            let u = &self.world.game.units[&id];
            let weapon = definition(self, id)?
                .get("w")
                .ok_or("missing ability weapon")?;
            let count = if matches!(kind, AbilityKind::Grenade | AbilityKind::Satchel)
                && flag(weapon, "perModel")
            {
                (u.hp / n(definition(self, id)?, "hpPer", 1.)).ceil()
            } else {
                1.
            };
            if self.abilities.get(id).unwrap().cooldown > 0.
                || u.retreating
                || kind.ammo() && !self.logistics.can_fire(id, count)
                || self.options.mode == crate::economy::Mode::World
                    && self.ability_munitions_available(u.owner)?
                        < self.ability_munitions_cost(id)?
            {
                continue;
            }
            if kind == AbilityKind::Smoke {
                smoke_ids.insert(id);
            }
            if kind.aimed() && valid_at && !queue && !automatic {
                *increments.entry(id).or_insert(0u64) += 1;
                if u.garrison.is_some() && !exits.contains_key(&id) {
                    let g = &self.world.game;
                    let mut placer = SpawnPlacer::new();
                    let cell = placer
                        .nearest_free(
                            TerrainView {
                                width: g.w,
                                height: g.h,
                                flags: &g.flags,
                                heights: g.height.as_deref(),
                                world_known: false,
                                knowledge_id: 0,
                                versions: Versions::default(),
                                changed: None,
                                changes: &[],
                            },
                            u.at(),
                            MOVE,
                            None,
                        )
                        .map_err(|e| format!("ability garrison exit: {e:?}"))?;
                    exits.insert(
                        id,
                        Point {
                            x: (cell % g.w) as f64 * CELL + CELL / 2.,
                            z: (cell / g.w) as f64 * CELL + CELL / 2.,
                        },
                    );
                }
            }
        }
        for (id, count) in increments {
            if self.world.game.units[&id]
                .path_gen
                .checked_add(count)
                .is_none()
            {
                return Err("unit path identity exhausted".into());
            }
        }
        let smoke_end = self
            .next_unit_id
            .checked_add(u32::try_from(smoke_ids.len()).map_err(|_| "smoke identity capacity")?)
            .ok_or("smoke identity capacity")?;
        if self
            .world
            .game
            .units
            .range(self.next_unit_id..smoke_end)
            .next()
            .is_some()
            || self
                .world
                .game
                .smokes
                .iter()
                .any(|s| (self.next_unit_id..smoke_end).contains(&s.id))
            || self.world.game.wrecks.iter().any(|w| {
                w.id.is_some_and(|id| (self.next_unit_id..smoke_end).contains(&id))
            })
        {
            return Err("smoke identity already held".into());
        }
        Ok(exits)
    }
    pub(crate) fn apply_ability_selection(
        &mut self,
        owner: usize,
        selected: &[Option<UnitId>],
        at: Option<Point>,
        queue: bool,
        automatic: bool,
    ) -> Result<Option<Denial>, String> {
        self.validate_abilities()?;
        let eligible: Vec<_> = selected.iter().flatten().copied().collect();
        if eligible
            .iter()
            .any(|id| self.world.game.units[id].owner != owner)
        {
            return Err("ability selection names another actor".into());
        }
        if self.unrepresented_ability_work(&eligible, at, queue, automatic)? {
            return Ok(Some(Denial::Unsupported));
        }
        let at = at
            .filter(|p| p.x.is_finite() && p.z.is_finite())
            .map(|p| Point {
                x: math::min(self.world.game.w as f64 * CELL, math::max(0., p.x)),
                z: math::min(self.world.game.h as f64 * CELL, math::max(0., p.z)),
            });
        let exits = self.preflight_ability_selection(&eligible, at, queue, automatic)?;
        let mut used = false;
        let mut reason = Denial::Needs;
        for &id in selected {
            let Some(id) = id else {
                reason = Denial::Needs;
                continue;
            };
            let kind = self.ability_kind(id)?;
            let ab = ability(self, id)?;
            let u = &self.world.game.units[&id];
            if queue && !u.retreating && at.is_some() && kind.aimed() {
                let orders = &mut self.commands.units.get_mut(&id).unwrap().orders;
                if orders.len() < 8 {
                    orders.push_back(Command::Ability {
                        ids: vec![i64::from(id)],
                        at,
                        queue: false,
                        automatic: false,
                    });
                    used = true;
                } else {
                    reason = Denial::QueueFull;
                }
                continue;
            }
            let weapon = definition(self, id)?
                .get("w")
                .ok_or("missing ability weapon")?;
            let count = if matches!(kind, AbilityKind::Grenade | AbilityKind::Satchel)
                && flag(weapon, "perModel")
            {
                (u.hp / n(definition(self, id)?, "hpPer", 1.)).ceil()
            } else {
                1.
            };
            if kind.ammo() && !self.logistics.can_fire(id, count) {
                reason = Denial::Ammo;
                continue;
            }
            if self.abilities.get(id).unwrap().cooldown > 0. {
                reason = Denial::Cooldown;
                continue;
            }
            if u.retreating {
                reason = Denial::Retreating;
                continue;
            }
            let cost = self.ability_munitions_cost(id)?;
            if self.options.mode == crate::economy::Mode::World
                && self.ability_munitions_available(u.owner)? < cost
            {
                reason = Denial::Munitions;
                continue;
            }
            if kind.aimed() {
                let Some(at) = at else {
                    reason = Denial::Blocked;
                    continue;
                };
                if !automatic {
                    if u.garrison.is_some() {
                        let at = exits[&id];
                        let unit = self.world.game.units.get_mut(&id).unwrap();
                        unit.x = at.x;
                        unit.z = at.z;
                        unit.garrison = None;
                        self.grid.update(id, unit);
                    }
                    self.cancel_field_work(id)?;
                    let u = self.world.game.units.get_mut(&id).unwrap();
                    u.traffic = None;
                    u.traffic_wait = 0.;
                    u.world_goal = None;
                    u.entrench = false;
                    u.dig = false;
                    u.build = false;
                    u.board = 0;
                    u.amove = None;
                    u.stuck = 0.;
                    u.path.clear();
                    u.path_gen += 1;
                    u.drift = false;
                    u.reverse = None;
                    u.attack = false;
                    u.repath = 0.;
                    let c = self.commands.units.get_mut(&id).unwrap();
                    c.orders.clear();
                    c.face = None;
                    c.enter = None;
                    c.fire_at = None;
                    c.attack_id = 0;
                    c.rally_drift = false;
                    c.outcome = Some(MoveOutcome::Interrupted);
                    c.outcome_tick = Some(self.world.game.tick);
                    self.firing.get_mut(id).unwrap().target_id = 0;
                }
                self.commands.units.get_mut(&id).unwrap().nade = Some(at);
                let row = self.abilities.get_mut(id).unwrap();
                row.aimed_auto = automatic;
                row.aimed_walked = false;
            } else {
                match kind {
                    AbilityKind::Suppress => {
                        self.abilities.get_mut(id).unwrap().buff = n(ab, "dur", 0.)
                    }
                    AbilityKind::Ura => {
                        let u = self.world.game.units.get_mut(&id).unwrap();
                        u.sprint = n(ab, "dur", 0.);
                        u.supp = 0.;
                    }
                    AbilityKind::Ap => self.abilities.get_mut(id).unwrap().ap = true,
                    AbilityKind::Smoke => {
                        let u = &self.world.game.units[&id];
                        self.world.game.smokes.push(Smoke {
                            id: self.next_unit_id,
                            remaining: Some(n(ab, "dur", 0.)),
                            x: u.x,
                            z: u.z,
                            r: n(ab, "radius", 0.),
                        });
                        self.next_unit_id += 1;
                    }
                    _ => continue,
                }
                self.abilities.get_mut(id).unwrap().cooldown = n(ab, "cd", 0.);
                self.charge_ability_munitions(owner, cost);
            }
            used = true;
        }
        Ok((!used).then_some(reason))
    }
}

#[cfg(test)]
mod tests;
