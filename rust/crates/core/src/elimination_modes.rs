//! Bounded Annihilation and Classic deployment, elimination and late victory phases.
use std::collections::BTreeSet;

use crate::{
    catalog::{Catalog, Value},
    economy::Mode,
    environment::{Context, Effect},
    game::{CELL, ModeKind, Point, Unit},
    match_end::EndReason,
    match_runtime::{
        Match, MatchOptions, StartPlan, find_site, foot_center, footprint, terrain_view,
    },
    match_story::DeathRecord,
    match_world::ResourceNode,
    math,
};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EliminationState {
    pub teams: usize,
    pub bunkers: Vec<usize>,
    pub time_left: Option<f64>,
    pub sudden_death: bool,
    pub nodes: Vec<ResourceNode>,
}

#[derive(Clone, Debug, Default)]
pub struct EliminationEffects {
    pub removed: Vec<Unit>,
    pub effects: Vec<Effect>,
}

fn number(value: &Value, key: &str) -> Result<f64, String> {
    value
        .get(key)
        .and_then(Value::number)
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("invalid elimination rule {key}"))
}
fn produces(kind: usize, rules: &crate::rules::Rules) -> Result<bool, String> {
    let name = &rules
        .kinds
        .get(kind)
        .ok_or("invalid elimination kind")?
        .name;
    Ok(Catalog::compiled()
        .map_err(|e| e.to_string())?
        .unit(name)
        .map_err(|e| e.to_string())?
        .get("produces")
        .and_then(Value::boolean)
        .unwrap_or(false))
}

impl Match {
    pub fn new_annihilation(
        map: &crate::map::AuthoritativeMap,
        options: MatchOptions,
    ) -> Result<Self, String> {
        if options.mode != Mode::Annihilation {
            return Err("Annihilation constructor requires Annihilation options".into());
        }
        let mut result = Self::new_elimination_shell(map, options)?;
        for owner in 0..result.start.players.len() {
            result.fortify_assault(owner)?;
        }
        let mut bunkers = vec![
            0;
            result
                .start
                .players
                .iter()
                .map(|p| p.config.team)
                .max()
                .unwrap_or(0) as usize
                + 1
        ];
        for unit in result.world.game.units.values() {
            if result.rules.kinds[unit.kind].name == "bunker" {
                bunkers[result.start.players[unit.owner].config.team as usize] += 1;
            }
        }
        result.elimination.as_mut().unwrap().bunkers = bunkers;
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
        let scale = match result.options.army {
            crate::economy_state::ArmySize::Standard => "standard",
            crate::economy_state::ArmySize::Large => "large",
            crate::economy_state::ArmySize::Massive => "massive",
            crate::economy_state::ArmySize::Endless => "endless",
        };
        let initial = number(
            cfg.get("assault").ok_or("missing Assault rules")?,
            "annihilationMp",
        )? * number(
            cfg.get("armies")
                .and_then(|v| v.get(scale))
                .ok_or("missing army rules")?,
            "income",
        )?;
        for player in &mut result.start.economy.players {
            player.mp = initial;
        }
        result.complete_skirmish_start()?;
        Ok(result)
    }

    pub fn new_classic(
        map: &crate::map::AuthoritativeMap,
        options: MatchOptions,
    ) -> Result<Self, String> {
        if options.mode != Mode::Classic {
            return Err("Classic constructor requires Classic options".into());
        }
        let mut result = Self::new_elimination_shell(map, options)?;
        for owner in 0..result.start.players.len() {
            let spawn = result.start.players[owner].spawn;
            let center = result
                .world
                .game
                .cell_of(spawn.x, spawn.z)
                .ok_or("Classic spawn outside map")?;
            if let Some(cell) = center.checked_sub(result.world.game.w + 1)
                && let Some(cells) = footprint(&result.world.game, cell, 3)
            {
                for c in cells {
                    result.world.set_cell(c, b'.').map_err(|e| e.to_string())?;
                }
                result.place_starting_building(owner, "hq", cell)?;
            }
        }
        result.setup_classic_nodes()?;
        let mut placer = crate::unit_spawn::SpawnPlacer::new();
        let blocked = result
            .world
            .game
            .units
            .values()
            .filter(|u| {
                !result.rules.kinds[u.kind].structure
                    && result.world.game.flags_at(u.x, u.z) & crate::game::MOVE != 0
            })
            .map(|u| u.id)
            .collect::<Vec<_>>();
        for id in blocked {
            let unit = &result.world.game.units[&id];
            let cell = placer
                .nearest_free(
                    terrain_view(&result.world.game),
                    unit.at(),
                    result.rules.kinds[unit.kind].block(),
                    None,
                )
                .map_err(|e| format!("Classic force clearance: {e:?}"))?;
            let unit = result.world.game.units.get_mut(&id).unwrap();
            unit.x = (cell % result.world.game.w) as f64 * CELL + CELL / 2.0;
            unit.z = (cell / result.world.game.w) as f64 * CELL + CELL / 2.0;
        }
        result.complete_skirmish_start()?;
        Ok(result)
    }

    fn new_elimination_shell(
        map: &crate::map::AuthoritativeMap,
        mut options: MatchOptions,
    ) -> Result<Self, String> {
        if map.source().get("scenario").is_some() && options.mode != Mode::Classic {
            return Err("elimination startup does not apply authored scenarios".into());
        }
        // These bounded modes have no reserves/convoy publication adapter yet.
        if options.logistics {
            return Err("elimination startup does not support logistics".into());
        }
        options.logistics = false;
        let rules = crate::rules::Rules::compiled().map_err(|e| e.to_string())?;
        let start = StartPlan::prepare_elimination(map, &options, &rules)?;
        let teams = options
            .players
            .iter()
            .map(|p| p.team)
            .collect::<BTreeSet<_>>()
            .len();
        let classic = options.mode == Mode::Classic;
        let mut result = Self::skirmish_shell(map, options, rules, start)?;
        result.world.game.mode = Some(if classic {
            ModeKind::Classic
        } else {
            ModeKind::Annihilation
        });
        let time_left = if classic {
            Some(number(
                Catalog::compiled()
                    .map_err(|e| e.to_string())?
                    .table("CFG")
                    .map_err(|e| e.to_string())?
                    .get("classic")
                    .ok_or("missing Classic rules")?,
                "time",
            )?)
        } else {
            None
        };
        result.elimination = Some(EliminationState {
            teams,
            bunkers: Vec::new(),
            time_left,
            sudden_death: false,
            nodes: Vec::new(),
        });
        Ok(result)
    }

    fn setup_classic_nodes(&mut self) -> Result<(), String> {
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
        let classic = cfg.get("classic").ok_or("missing Classic rules")?;
        let center = Point {
            x: self.world.game.w as f64 * CELL / 2.0,
            z: self.world.game.h as f64 * CELL / 2.0,
        };
        let mut avoid = self
            .start
            .players
            .iter()
            .map(|p| (p.spawn, 10.0))
            .collect::<Vec<_>>();
        let point_radius = number(cfg, "pointRadius")?;
        avoid.extend(self.start.points.iter().map(|p| (p.at, point_radius + 3.0)));
        let mut wanted = Vec::new();
        let mut pairs = Vec::new();
        let mut paired = BTreeSet::new();
        for (slot, player) in self.start.players.iter().enumerate() {
            let a = math::atan2(center.z - player.spawn.z, center.x - player.spawn.x);
            for side in [-1.0, 1.0] {
                wanted.push((
                    Point {
                        x: player.spawn.x + math::cos(a + side * 1.2) * 24.0,
                        z: player.spawn.z + math::sin(a + side * 1.2) * 24.0,
                    },
                    number(classic, "homeRate")?,
                    false,
                    None,
                ));
            }
            let mut enemies = self
                .start
                .players
                .iter()
                .enumerate()
                .filter(|(_, p)| p.config.team != player.config.team)
                .collect::<Vec<_>>();
            crate::sort::sort_by(&mut enemies, |(_, p), (_, q)| {
                math::hypot(p.spawn.x - player.spawn.x, p.spawn.z - player.spawn.z)
                    - math::hypot(q.spawn.x - player.spawn.x, q.spawn.z - player.spawn.z)
            });
            for (other, _) in enemies.into_iter().take(2) {
                let pair = (slot.min(other), slot.max(other));
                if paired.insert(pair) {
                    pairs.push(pair);
                }
            }
        }
        for &(a, b) in &pairs {
            let a = self.start.players[a].spawn;
            let b = self.start.players[b].spawn;
            let length = math::hypot(b.x - a.x, b.z - a.z);
            let length = if length == 0.0 { 1.0 } else { length };
            let sides: &[f64] = if pairs.len() == 1 {
                &[-1.0, 1.0]
            } else {
                &[0.0]
            };
            for side in sides {
                wanted.push((
                    Point {
                        x: (a.x + b.x) / 2.0 - (b.z - a.z) / length * side * 22.0,
                        z: (a.z + b.z) / 2.0 + (b.x - a.x) / length * side * 22.0,
                    },
                    number(classic, "contestedFuel")?,
                    true,
                    Some((a, b)),
                ));
            }
        }
        // Node footprints reject live ground troops. Only the search copy treats
        // those cells as blocked; walking distances retain the original terrain.
        let mut sites = self.world.game.clone();
        for unit in self.world.game.units.values() {
            if unit.hp > 0.0
                && !self.rules.kinds[unit.kind].air
                && !unit.riding
                && !self.rules.kinds[unit.kind].structure
                && let Some(cell) = self.world.game.cell_of(unit.x, unit.z)
            {
                sites.flags[cell] |= crate::game::MOVE;
            }
        }
        let mut nodes = Vec::<ResourceNode>::new();
        let mut walking = ClassicWalking::new(&self.world.game, &self.rules)?;
        for (at, rate, fuel, from) in wanted {
            let mut away = avoid.clone();
            away.extend(nodes.iter().map(|node| (node.at, 10.0)));
            let mut selected = find_site(&sites, at, 2, &away);
            if let Some((a, b)) = from
                && selected.is_some()
            {
                let mut best = f64::INFINITY;
                for radius in [0.0, 8.0, 16.0, 24.0] {
                    for k in 0..if radius == 0.0 { 1 } else { 8 } {
                        let angle = k as f64 * std::f64::consts::FRAC_PI_4;
                        let wanted = Point {
                            x: at.x + math::cos(angle) * radius,
                            z: at.z + math::sin(angle) * radius,
                        };
                        if let Some(cell) = find_site(&sites, wanted, 2, &away) {
                            let center = foot_center(&self.world.game, cell, 2);
                            let da = walking.distance(&self.world.game, &self.rules, a, center)?;
                            let db = walking.distance(&self.world.game, &self.rules, b, center)?;
                            let score = (da - db).abs() + 0.1 * (da + db);
                            if score < best {
                                best = score;
                                selected = Some(cell);
                            }
                        }
                    }
                }
            }
            if let Some(cell) = selected {
                nodes.push(ResourceNode {
                    cell,
                    at: foot_center(&self.world.game, cell, 2),
                    depot: None,
                    rate,
                    fuel,
                });
            }
        }
        self.elimination.as_mut().unwrap().nodes = nodes;
        Ok(())
    }

    pub fn elimination_sudden_death(&self) -> bool {
        self.elimination
            .as_ref()
            .is_some_and(|state| state.sudden_death)
    }

    pub fn validate_elimination_state(&self) -> Result<(), String> {
        self.validate_match_finish_state()?;
        self.validate_sample_units()?;
        let classic = match (self.options.mode, self.world.game.mode) {
            (Mode::Annihilation, Some(ModeKind::Annihilation)) => false,
            (Mode::Classic, Some(ModeKind::Classic)) => true,
            _ => return Err("elimination phase requires Annihilation or Classic".into()),
        };
        let state = self
            .elimination
            .as_ref()
            .ok_or("missing elimination state")?;
        let teams = self
            .start
            .players
            .iter()
            .map(|p| p.config.team)
            .collect::<BTreeSet<_>>()
            .len();
        if state.teams != teams
            || state.time_left.is_some() != classic
            || state.time_left.is_some_and(|v| !v.is_finite())
            || (!classic && (state.sudden_death || !state.nodes.is_empty()))
            || state.nodes.iter().any(|n| {
                !n.at.x.is_finite()
                    || !n.at.z.is_finite()
                    || !n.rate.is_finite()
                    || n.cell >= self.world.game.w * self.world.game.h
            })
        {
            return Err("invalid canonical elimination state".into());
        }
        for (&id, unit) in &self.world.game.units {
            if id != unit.id
                || unit.owner >= self.start.players.len()
                || !unit.x.is_finite()
                || !unit.z.is_finite()
            {
                return Err("invalid elimination actor".into());
            }
        }
        Ok(())
    }

    /// Source pre-capture defeat phase. Terminal losers retain their armies for the reveal.
    pub fn apply_annihilation_elimination(&mut self) -> Result<EliminationEffects, String> {
        self.validate_elimination_state()?;
        if self.options.mode != Mode::Annihilation {
            return Err("expected Annihilation mode".into());
        }
        if self.finish.is_some() {
            return Ok(EliminationEffects::default());
        }
        let left = self.standing_bunker_teams();
        if self.elimination.as_ref().unwrap().teams > 1 && left.len() <= 1 {
            for player in &mut self.start.economy.players {
                if !left.contains(&player.team) {
                    player.out = true;
                }
            }
            return Ok(EliminationEffects::default());
        }
        self.stage_elimination(false, 0.0)
    }

    fn standing_bunker_teams(&self) -> BTreeSet<i32> {
        self.world
            .game
            .units
            .values()
            .filter(|u| self.rules.kinds[u.kind].name == "bunker" && u.hp > 0.0)
            .map(|u| self.start.economy.players[u.owner].team)
            .collect()
    }

    /// Classic runs after income. Annihilation only decides victory here, after its early defeat phase.
    pub fn apply_elimination_victory(&mut self, dt: f64) -> Result<EliminationEffects, String> {
        self.validate_elimination_state()?;
        if !dt.is_finite() || dt < 0.0 {
            return Err("invalid elimination interval".into());
        }
        if self.options.mode == Mode::Annihilation {
            if self.finish.is_none() {
                let left = self.standing_bunker_teams();
                self.finish_elimination(left, EndReason::Bunkers, self.fallen.bunker)?;
            }
            return Ok(EliminationEffects::default());
        }
        let state = self.elimination.as_ref().unwrap();
        let time = state.time_left.unwrap() - if self.finish.is_none() { dt } else { 0.0 };
        if !time.is_finite() {
            return Err("Classic timer overflow".into());
        }
        let sudden = state.sudden_death || time <= 0.0;
        let effects = self.stage_elimination(true, if sudden { dt } else { 0.0 })?;
        if sudden && !self.elimination_sudden_death() {
            self.production.clear_queues();
        }
        let state = self.elimination.as_mut().unwrap();
        state.time_left = Some(time);
        state.sudden_death = sudden;
        if self.finish.is_none() {
            let left = self
                .start
                .economy
                .players
                .iter()
                .filter(|p| !p.out)
                .map(|p| p.team)
                .collect();
            self.finish_elimination(left, EndReason::Headquarters, self.fallen.hq)?;
        }
        Ok(effects)
    }

    fn finish_elimination(
        &mut self,
        left: BTreeSet<i32>,
        reason: EndReason,
        at: Option<Point>,
    ) -> Result<(), String> {
        if self.elimination.as_ref().unwrap().teams > 1 && left.len() <= 1 {
            let winner = left.into_iter().next().unwrap_or(-1);
            let prepared = self.prepare_match_finish(winner, reason, at)?;
            self.publish_match_finish(prepared);
        }
        Ok(())
    }

    /// Stage all owner transfers and real wreck effects before publishing the canonical state.
    fn stage_elimination(
        &mut self,
        classic: bool,
        decay_dt: f64,
    ) -> Result<EliminationEffects, String> {
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        if decay_dt == 0.0 {
            let mut required = self
                .world
                .game
                .units
                .values()
                .any(|u| classic && self.rules.kinds[u.kind].building && u.hp <= 0.0);
            for (slot, _) in self
                .start
                .economy
                .players
                .iter()
                .enumerate()
                .filter(|(_, p)| !p.out)
            {
                let mut keeps = false;
                for unit in self.world.game.units.values().filter(|u| u.owner == slot) {
                    keeps |= if classic {
                        unit.hp > 0.0 && produces(unit.kind, &self.rules)?
                    } else {
                        unit.hp > 0.0 && self.rules.kinds[unit.kind].name == "bunker"
                    };
                }
                required |= !keeps;
            }
            if !required {
                return Ok(EliminationEffects::default());
            }
        }
        self.validate_routing_context()?;
        let mut draft = EliminationDraft {
            transaction: self.world.begin_transaction().map_err(|e| e.to_string())?,
            environment: self.environment.clone(),
            combat: self.combat.clone(),
            grid: self.grid.clone_for_transaction(),
            controls: self.commands.units.clone(),
            seed: self.start.environment_seed,
            next_id: self.next_unit_id,
            report: EliminationEffects::default(),
            deaths: Vec::new(),
            transfers: Vec::new(),
        };
        let mut wallets = self.start.economy.clone();
        if classic {
            let decay = number(
                catalog
                    .table("CFG")
                    .map_err(|e| e.to_string())?
                    .get("classic")
                    .ok_or("missing Classic rules")?,
                "decay",
            )?;
            for unit in draft.transaction.world.game.units.values_mut() {
                if unit.hp > 0.0 && produces(unit.kind, &self.rules)? {
                    unit.hp -= number(
                        catalog
                            .unit(&self.rules.kinds[unit.kind].name)
                            .map_err(|e| e.to_string())?,
                        "hpPer",
                    )? * decay
                        * decay_dt;
                    if !unit.hp.is_finite() {
                        return Err("Classic decay overflow".into());
                    }
                }
            }
            // A preceding building collapse may kill a later building in this same pass.
            let buildings = draft
                .transaction
                .world
                .game
                .units
                .values()
                .filter(|u| self.rules.kinds[u.kind].building)
                .map(|u| u.id)
                .collect::<Vec<_>>();
            for id in buildings {
                if draft
                    .transaction
                    .world
                    .game
                    .units
                    .get(&id)
                    .is_some_and(|u| u.hp <= 0.0)
                {
                    draft.remove(id, &self.rules, self.start.wind, true)?;
                }
            }
        }
        for slot in 0..wallets.players.len() {
            let mut keeps = false;
            for unit in draft
                .transaction
                .world
                .game
                .units
                .values()
                .filter(|u| u.owner == slot)
            {
                keeps |= if classic {
                    produces(unit.kind, &self.rules)?
                } else {
                    self.rules.kinds[unit.kind].name == "bunker" && unit.hp > 0.0
                };
            }
            if wallets.players[slot].out || keeps {
                continue;
            }
            wallets.players[slot].out = true;
            let team = wallets.players[slot].team;
            let mut mates = wallets
                .players
                .iter()
                .enumerate()
                .filter(|(_, p)| p.team == team && !p.out)
                .map(|(slot, _)| slot)
                .collect::<Vec<_>>();
            let owned = draft
                .transaction
                .world
                .game
                .units
                .values()
                .filter(|u| u.owner == slot)
                .map(|u| u.id)
                .collect::<Vec<_>>();
            for id in owned {
                let unit = &draft.transaction.world.game.units[&id];
                if self.rules.kinds[unit.kind].building || mates.is_empty() {
                    draft.remove(id, &self.rules, self.start.wind, false)?;
                } else {
                    let metadata = draft.combat.get(id).ok_or("missing transfer metadata")?;
                    if unit.riding
                        || metadata.carrier.is_some()
                        || !metadata.cargo.is_empty()
                        || unit.dig
                        || unit.entrench
                    {
                        return Err("transfer cargo, riders and field work are unsupported".into());
                    }
                    crate::sort::sort_by(&mut mates, |a, b| {
                        let a_at = self.start.players[a].spawn;
                        let b_at = self.start.players[b].spawn;
                        math::hypot(a_at.x - unit.x, a_at.z - unit.z)
                            - math::hypot(b_at.x - unit.x, b_at.z - unit.z)
                    });
                    let to = mates[0];
                    let unit = draft.transaction.world.game.units.get_mut(&id).unwrap();
                    if unit.path_gen == u64::MAX {
                        return Err("transferred route identity exhausted".into());
                    }
                    unit.owner = to;
                    unit.path.clear();
                    unit.path_gen += 1;
                    unit.build = false;
                    unit.amove = None;
                    unit.retreating = false;
                    let control = draft
                        .controls
                        .get_mut(&unit.id)
                        .ok_or("missing transfer control")?;
                    control.orders.clear();
                    control.build_target = None;
                    control.attack_id = 0;
                    draft.grid.update(unit.id, unit);
                    draft.transfers.push(unit.id);
                }
            }
        }
        if draft
            .transaction
            .world
            .game
            .units
            .values()
            .any(|u| !u.hp.is_finite())
        {
            return Err("invalid elimination wreck result".into());
        }
        let death_facts = draft
            .deaths
            .iter()
            .map(|u| {
                (
                    DeathRecord {
                        owner: Some(u.owner),
                        kind: self.rules.kinds[u.kind].name.as_str(),
                        at: u.at(),
                    },
                    None,
                )
            })
            .collect::<Vec<_>>();
        self.validate_story_for_deaths(&death_facts)?;
        draft
            .transaction
            .prepare_publication(&mut self.world)
            .map_err(|e| e.to_string())?
            .commit();
        self.start.economy = wallets;
        self.environment = draft.environment;
        self.grid = draft.grid;
        self.combat = draft.combat;
        self.commands.units = draft.controls;
        self.start.environment_seed = draft.seed;
        self.next_unit_id = draft.next_id;
        for unit in &draft.report.removed {
            self.retain_removed_route_unit(unit)?;
            self.field_works.cancel(unit.id);
            self.commands.units.remove(&unit.id);
            self.commands.requests.retire_unit(unit.id);
            self.firing.remove(unit.id);
            self.abilities.remove(unit.id);
            self.recovery.remove(unit.id);
            self.production.remove_building(unit.id);
            self.building_rallies.remove(&unit.id);
            self.recruit_metadata.remove(&unit.id);
            self.logistics.remove(unit.id);
            self.aviation.remove(unit.id);
            for node in &mut self.elimination.as_mut().unwrap().nodes {
                if node.depot == Some(unit.id) {
                    node.depot = None;
                }
            }
        }
        for id in draft.transfers {
            self.commands.requests.retire_unit(id);
        }
        for unit in draft.deaths {
            let name = self.rules.kinds[unit.kind].name.clone();
            self.record_death(
                DeathRecord {
                    owner: Some(unit.owner),
                    kind: &name,
                    at: unit.at(),
                },
                None,
            )?;
        }
        Ok(draft.report)
    }
}

/// Startup-only path scorer. Fuel sites compare actual walk lengths from each HQ doorstep.
struct ClassicWalking {
    paths: crate::pathfinding::Pathfinding,
    placer: crate::unit_spawn::SpawnPlacer,
    walls: Vec<u8>,
    hazards: Vec<bool>,
    path_rules: crate::pathfinding::PathRules,
    infantry: usize,
}
impl ClassicWalking {
    fn new(game: &crate::game::Game, rules: &crate::rules::Rules) -> Result<Self, String> {
        let mut walls = vec![0; game.w * game.h];
        for (cell, wall) in walls.iter_mut().enumerate() {
            let (x, y) = (cell % game.w, cell / game.w);
            *wall = u8::from((y.saturating_sub(1)..=(y + 1).min(game.h - 1)).any(|ny| {
                (x.saturating_sub(1)..=(x + 1).min(game.w - 1)).any(|nx| {
                    game.flags[ny * game.w + nx] & (crate::game::MOVE | crate::game::SIGHT)
                        == (crate::game::MOVE | crate::game::SIGHT)
                })
            }));
        }
        Ok(Self {
            paths: crate::pathfinding::Pathfinding::new(),
            placer: crate::unit_spawn::SpawnPlacer::new(),
            walls,
            hazards: vec![false; game.w * game.h],
            path_rules: crate::pathfinding::PathRules::compiled().map_err(|e| e.to_string())?,
            infantry: rules
                .kind("rifle")
                .ok_or("missing Classic walking profile")?,
        })
    }
    fn distance(
        &mut self,
        game: &crate::game::Game,
        rules: &crate::rules::Rules,
        from: Point,
        to: Point,
    ) -> Result<f64, String> {
        let cell = self
            .placer
            .nearest_free(terrain_view(game), from, crate::game::MOVE, None)
            .map_err(|e| format!("Classic walk doorstep: {e:?}"))?;
        let start = Point {
            x: (cell % game.w) as f64 * CELL + CELL / 2.0,
            z: (cell / game.w) as f64 * CELL + CELL / 2.0,
        };
        let path = self
            .paths
            .find_path(
                crate::pathfinding::PathTerrain {
                    navigation: terrain_view(game),
                    near_walls: &self.walls,
                    known_mines: &self.hazards,
                    fires: &self.hazards,
                    bunkers: &[],
                    wear: game.wear.as_deref(),
                    wetness: game.wet.unwrap_or(0.0),
                    roads: false,
                },
                self.path_rules,
                &rules.kinds[self.infantry],
                start,
                to,
                None,
            )
            .map_err(|e| format!("Classic fuel walk: {e:?}"))?;
        if path.waypoints.is_empty() {
            return Ok(f64::INFINITY);
        }
        let mut distance = 0.0;
        let mut at = from;
        for point in path.waypoints {
            distance += math::hypot(point.x - at.x, point.z - at.z);
            at = point;
        }
        Ok(distance)
    }
}

struct EliminationDraft {
    transaction: crate::world::WorldTransaction,
    environment: crate::environment::EnvironmentRuntime,
    combat: crate::match_combat::CombatState,
    grid: crate::grid::SpatialGrid,
    controls: std::collections::BTreeMap<u32, crate::match_commands::UnitControl>,
    seed: u32,
    next_id: u32,
    report: EliminationEffects,
    deaths: Vec<Unit>,
    transfers: Vec<u32>,
}
impl EliminationDraft {
    fn remove(
        &mut self,
        id: u32,
        rules: &crate::rules::Rules,
        wind: (f64, f64),
        death: bool,
    ) -> Result<(), String> {
        let unit = self
            .transaction
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing elimination unit")?;
        let metadata = self
            .combat
            .get(id)
            .ok_or("missing elimination combat metadata")?;
        if unit.riding || metadata.carrier.is_some() || !metadata.cargo.is_empty() || unit.entrench
        {
            return Err("elimination cargo, riders and shared projects are unsupported".into());
        }
        if rules.kinds[unit.kind].building
            && self.transaction.world.game.units.values().any(|u| {
                u.riding
                    || self
                        .combat
                        .get(u.id)
                        .is_some_and(|m| m.carrier.is_some() || !m.cargo.is_empty())
            })
        {
            return Err("building elimination with linked carriers is unsupported".into());
        }
        let unit = self.transaction.world.game.units.remove(&id).unwrap();
        self.grid.remove_unit(id);
        if rules.kinds[unit.kind].building {
            let order = self
                .transaction
                .world
                .game
                .units
                .keys()
                .copied()
                .collect::<Vec<_>>();
            let world = &mut self.transaction.world;
            let mut context = Context {
                tick: world.game.tick as u64,
                seed: &mut self.seed,
                next_id: &mut self.next_id,
                unit_order: &order,
                grid: &self.grid,
                rain: world.game.rain,
                wet: world.game.wet.unwrap_or(0.0),
                wind_angle: wind.0,
                wind_strength: wind.1,
            };
            for effect in self
                .environment
                .wreck_building(world, &unit, &mut context)
                .map_err(|e| e.to_string())?
            {
                if let Effect::LastHit { unit, owner } = effect {
                    self.combat.initialize(unit).last_hit = Some(owner);
                } else {
                    self.report.effects.push(effect);
                }
            }
        }
        if death {
            self.deaths.push(unit.clone());
        }
        self.report.removed.push(unit);
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "elimination_modes/classic_nodes_tests.rs"]
mod classic_nodes_tests;
