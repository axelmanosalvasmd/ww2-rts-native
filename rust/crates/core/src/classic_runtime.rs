//! Canonical Classic startup. No room, FFI or ordinary constructor admission.
use super::{Match, MatchOptions, StartPlan, find_site, foot_center, footprint, terrain_view};
use crate::{
    catalog::{Catalog, Value},
    economy::Mode,
    game::{CELL, MOVE, ModeKind, Point, ROAD, SIGHT},
    match_world::ResourceNode,
    math,
    pathfinding::{PathRules, PathTerrain, Pathfinding},
    rules::Rules,
    unit_publication::StagedUnit,
    unit_spawn::SpawnPlacer,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassicRuntime {
    pub(crate) time_left: f64,
    pub(crate) initial_teams: usize,
    pub(crate) sudden_death: bool,
    pub(crate) nodes: Vec<ResourceNode>,
    fault: Option<String>,
}
fn number(v: &Value, key: &str) -> Result<f64, String> {
    v.get(key)
        .and_then(Value::number)
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("invalid Classic rule {key}"))
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
impl Match {
    /// Explicit Rust component entry. Public StartPlan/ordinary Match admission
    /// and every external transport remain unchanged.
    pub fn new_classic(
        map: &crate::map::AuthoritativeMap,
        options: MatchOptions,
    ) -> Result<Self, String> {
        if options.mode != Mode::Classic || map.geography().is_some() {
            return Err("Classic constructor requires an ordinary map and Classic options".into());
        }
        let rules = Rules::compiled().map_err(|e| e.to_string())?;
        let start = StartPlan::prepare_classic(map, &options, &rules)?;
        let mut state = Self::match_shell(map, options, rules, start, false)?;
        let cfg = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .table("CFG")
            .map_err(|e| e.to_string())?;
        let classic = cfg.get("classic").ok_or("missing Classic rules")?;
        let mut teams = std::collections::BTreeSet::new();
        for p in &state.start.players {
            teams.insert(p.config.team);
        }
        state.classic_runtime = Some(ClassicRuntime {
            time_left: number(classic, "time")?,
            initial_teams: teams.len(),
            sudden_death: false,
            nodes: Vec::new(),
            fault: None,
        });
        state.world.game.mode = Some(ModeKind::Classic);
        // Source clears each actual HQ footprint before placing the HQ. No
        // Conquest bases or mobile force ever existed in this match.
        for owner in 0..state.start.players.len() {
            let spawn = state.start.players[owner].spawn;
            let x = (spawn.x / CELL) as isize - 1;
            let y = (spawn.z / CELL) as isize - 1;
            let cell = y
                .checked_mul(state.world.game.w as isize)
                .and_then(|c| c.checked_add(x))
                .and_then(|c| usize::try_from(c).ok());
            let Some(cell) = cell else {
                continue;
            };
            let Some(cells) = footprint(&state.world.game, cell, 3) else {
                continue;
            };
            for cell in cells {
                state
                    .world
                    .set_cell(cell, b'.')
                    .map_err(|e| e.to_string())?;
            }
            state.place_starting_building(owner, "hq", cell)?;
        }
        state.initialize_classic_nodes(cfg)?;
        for owner in 0..state.start.players.len() {
            for index in 0..state.start.starting_kinds.len() {
                let kind = state.start.starting_kinds[index];
                let draft = StagedUnit::ordinary(
                    &state.world.game,
                    &state.rules,
                    owner,
                    kind,
                    state.next_unit_id,
                    index as u32,
                    state.start.players[owner].spawn,
                    None,
                )
                .map_err(|e| format!("Classic starting force: {e:?}"))?;
                state.publish_starting_unit(draft)?;
            }
        }
        state.grid.rebuild(&state.world.game.units);
        state.initialize_recipient_routing()?;
        Ok(state)
    }

    fn initialize_classic_nodes(&mut self, cfg: &Value) -> Result<(), String> {
        let classic = cfg.get("classic").ok_or("missing Classic rules")?;
        let center = Point {
            x: self.world.game.w as f64 * CELL / 2.,
            z: self.world.game.h as f64 * CELL / 2.,
        };
        let spawns = self
            .start
            .players
            .iter()
            .map(|p| p.spawn)
            .collect::<Vec<_>>();
        let mut requests: Vec<(Point, f64, Option<[Point; 2]>)> = Vec::new();
        for spawn in &spawns {
            let a = math::atan2(center.z - spawn.z, center.x - spawn.x);
            for side in [-1., 1.] {
                requests.push((
                    Point {
                        x: spawn.x + math::cos(a + side * 1.2) * 24.,
                        z: spawn.z + math::sin(a + side * 1.2) * 24.,
                    },
                    number(classic, "homeRate")?,
                    None,
                ));
            }
        }
        let mut pairs = Vec::new();
        for (slot, p) in self.start.players.iter().enumerate() {
            let mut foes = self
                .start
                .players
                .iter()
                .enumerate()
                .filter(|(_, q)| q.config.team != p.config.team)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            foes.sort_by(|a, b| {
                distance(spawns[*a], p.spawn).total_cmp(&distance(spawns[*b], p.spawn))
            });
            for foe in foes.into_iter().take(2) {
                let pair = (slot.min(foe), slot.max(foe));
                if !pairs.contains(&pair) {
                    pairs.push(pair);
                }
            }
        }
        for &(a, b) in &pairs {
            let from = [spawns[a], spawns[b]];
            let l = distance(from[0], from[1]);
            let l = if l == 0. { 1. } else { l };
            let px = -(from[1].z - from[0].z) / l;
            let pz = (from[1].x - from[0].x) / l;
            for side in if pairs.len() == 1 {
                &[-1., 1.][..]
            } else {
                &[0.][..]
            } {
                requests.push((
                    Point {
                        x: (from[0].x + from[1].x) / 2. + px * side * 22.,
                        z: (from[0].z + from[1].z) / 2. + pz * side * 22.,
                    },
                    number(classic, "contestedFuel")?,
                    Some(from),
                ));
            }
        }
        let mut avoid = spawns.iter().map(|p| (*p, 10.)).collect::<Vec<_>>();
        let radius = number(cfg, "pointRadius")? + 3.;
        avoid.extend(self.start.points.iter().map(|p| (p.at, radius)));
        let mut nodes: Vec<ResourceNode> = Vec::new();
        // One path service retains Source search history throughout node setup.
        let mut paths = Pathfinding::new();
        for (wish, rate, from) in requests {
            let mut away = avoid.clone();
            away.extend(nodes.iter().map(|n| (n.at, 10.)));
            let mut cell = find_site(&self.world.game, wish, 2, &away);
            if let (Some(from), Some(_)) = (from, cell) {
                let mut best = f64::INFINITY;
                for radius in [0., 8., 16., 24.] {
                    for k in 0..if radius == 0. { 1 } else { 8 } {
                        let a = k as f64 * std::f64::consts::PI / 4.;
                        let candidate = Point {
                            x: wish.x + math::cos(a) * radius,
                            z: wish.z + math::sin(a) * radius,
                        };
                        let Some(c) = find_site(&self.world.game, candidate, 2, &away) else {
                            continue;
                        };
                        let at = foot_center(&self.world.game, c, 2);
                        let da = self.classic_walk(&mut paths, from[0], at)?;
                        let db = self.classic_walk(&mut paths, from[1], at)?;
                        let score = (da - db).abs() + 0.1 * (da + db);
                        if score < best {
                            best = score;
                            cell = Some(c);
                        }
                    }
                }
            }
            if let Some(cell) = cell {
                nodes.push(ResourceNode {
                    cell,
                    at: foot_center(&self.world.game, cell, 2),
                    depot: None,
                    rate,
                    fuel: from.is_some(),
                });
            }
        }
        self.classic_runtime
            .as_mut()
            .ok_or("missing Classic runtime")?
            .nodes = nodes;
        Ok(())
    }

    fn classic_walk(&self, paths: &mut Pathfinding, from: Point, to: Point) -> Result<f64, String> {
        let game = &self.world.game;
        let profile = &self.rules.kinds[self
            .rules
            .kind("rifle")
            .ok_or("missing Classic walking profile")?];
        let cell = SpawnPlacer::new()
            .nearest_free(terrain_view(game), from, MOVE, None)
            .map_err(|e| format!("Classic HQ doorstep: {e:?}"))?;
        let start = Point {
            x: (cell % game.w) as f64 * CELL + CELL / 2.,
            z: (cell / game.w) as f64 * CELL + CELL / 2.,
        };
        let n = game.flags.len();
        let mut near = vec![0; n];
        for (c, near) in near.iter_mut().enumerate() {
            let (x, y) = (c % game.w, c / game.w);
            *near = u8::from((y.saturating_sub(1)..=(y + 1).min(game.h - 1)).any(|ny| {
                (x.saturating_sub(1)..=(x + 1).min(game.w - 1))
                    .any(|nx| game.flags[ny * game.w + nx] & (MOVE | SIGHT) == (MOVE | SIGHT))
            }));
        }
        let empty = vec![false; n];
        let path = paths
            .find_path(
                PathTerrain {
                    navigation: terrain_view(game),
                    near_walls: &near,
                    known_mines: &empty,
                    fires: &empty,
                    bunkers: &[],
                    wear: game.wear.as_deref(),
                    wetness: game.wet.unwrap_or(0.),
                    roads: game.flags.iter().any(|f| f & ROAD != 0),
                },
                PathRules::compiled().map_err(|e| e.to_string())?,
                profile,
                start,
                to,
                None,
            )
            .map_err(|e| format!("Classic resource walking balance: {e:?}"))?;
        if path.waypoints.is_empty() {
            return Ok(f64::INFINITY);
        }
        let (mut d, mut at) = (0., from);
        for p in path.waypoints {
            d += distance(at, p);
            at = p;
        }
        Ok(d)
    }
}

impl Match {
    pub(crate) fn classic_sudden_death(&self) -> bool {
        self.classic_runtime
            .as_ref()
            .is_some_and(|s| s.sudden_death)
    }
    pub(crate) fn validate_classic_base_state(&self) -> Result<(), String> {
        let state = self
            .classic_runtime
            .as_ref()
            .ok_or("missing Classic runtime")?;
        if self.options.mode != Mode::Classic
            || self.world.game.mode != Some(ModeKind::Classic)
            || self.world_runtime.is_some()
            || self.assault.is_some()
            || self.world.geography().is_some()
            || !state.time_left.is_finite()
            || state.initial_teams == 0
            || state.initial_teams > 6
            || self.start.players.len() != self.world.game.players.len()
            || self.start.players.len() != self.start.economy.players.len()
            || self.world.game.tick < 0
        {
            return Err("invalid canonical Classic base state".into());
        }
        if let Some(fault) = &state.fault {
            return Err(format!("Classic base caller faulted: {fault}"));
        }
        for node in &state.nodes {
            if node.cell >= self.world.game.flags.len()
                || !node.at.x.is_finite()
                || !node.at.z.is_finite()
                || !node.rate.is_finite()
                || node.rate < 0.
            {
                return Err("invalid Classic resource node".into());
            }
        }
        Ok(())
    }

    /// One actual base-component boundary from canonical Classic startup.
    /// This explicitly omits movement/combat/construction and is not the full
    /// Match runner. Its clock always advances by one catalog tick, never a
    /// caller-supplied interval or a synthetic near-deadline start.
    pub fn advance_classic_base_tick(
        &mut self,
    ) -> Result<Vec<crate::match_commands::CommandResult>, String> {
        self.advance_classic_components(false)
    }

    /// Fixed-tick Depot and Barracks components over canonical Classic startup.
    /// This is not the ordinary runner and does not execute combat or elimination.
    pub fn advance_classic_depot_tick(
        &mut self,
    ) -> Result<Vec<crate::match_commands::CommandResult>, String> {
        self.validate_classic_depot_components()?;
        self.advance_classic_components(true)
    }

    fn advance_classic_components(
        &mut self,
        construction: bool,
    ) -> Result<Vec<crate::match_commands::CommandResult>, String> {
        self.validate_classic_base_state()?;
        let dt = self.rules.movement.tick;
        if !dt.is_finite() || dt <= 0. || self.options.logistics || self.convoys.is_some() {
            return Err("unrepresented Classic base tick context".into());
        }
        let tick = u64::try_from(self.world.game.tick)
            .map_err(|_| "invalid Classic clock")?
            .checked_add(1)
            .filter(|t| *t < 9_007_199_254_740_351)
            .ok_or("exhausted Classic clock")?;
        self.validate_command_tick(tick)?;
        use crate::match_commands::Command;
        for (_, command) in self.commands.queue.pending_tick(tick) {
            if !matches!(
                command,
                Command::Buy { .. }
                    | Command::Technology { .. }
                    | Command::CancelProduction { .. }
                    | Command::Rally { .. }
                    | Command::CancelConstruction { id: None }
            ) && !(construction
                && matches!(command, Command::Build { kind, .. }
                    if matches!(kind.as_str(), "depot" | "barracks")))
                && !(construction
                    && matches!(
                        command,
                        Command::CancelConstruction { .. }
                            | Command::Assist { .. }
                            | Command::Move {
                                attack_move: false,
                                ..
                            }
                            | Command::Stop { .. }
                    ))
            {
                return Err("Classic base caller has an unrepresented command".into());
            }
        }
        self.validate_production_phases(dt)
            .map_err(|e| format!("Classic production: {e:?}"))?;
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
        let classic = cfg.get("classic").ok_or("missing Classic rules")?;
        let decay = number(classic, "decay")?;
        let state = self
            .classic_runtime
            .as_ref()
            .ok_or("missing Classic runtime")?;
        let decaying = state.sudden_death || state.time_left - dt <= 0.;
        for unit in self.world.game.units.values() {
            if !unit.hp.is_finite() || unit.owner >= self.start.players.len() {
                return Err("invalid Classic base actor".into());
            }
            let kind = self
                .rules
                .kinds
                .get(unit.kind)
                .ok_or("invalid Classic base kind")?;
            let definition = catalog.unit(&kind.name).map_err(|e| e.to_string())?;
            if !construction && kind.building && unit.hp > 0. && unit.built.is_none_or(|b| b < 1.) {
                return Err("Classic base caller does not execute unfinished construction".into());
            }
            if decaying
                && definition.get("produces").is_some()
                && unit.hp > 0.
                && unit.hp - number(definition, "hpPer")? * decay * dt <= 0.
            {
                return Err("Classic producer death requires the full retirement caller".into());
            }
        }
        let result = self.run_classic_base_tick(tick, dt, cfg, construction);
        if let Err(error) = &result {
            self.classic_runtime
                .as_mut()
                .ok_or("missing Classic runtime")?
                .fault = Some(error.clone());
        }
        result
    }

    fn run_classic_base_tick(
        &mut self,
        tick: u64,
        dt: f64,
        cfg: &Value,
        construction: bool,
    ) -> Result<Vec<crate::match_commands::CommandResult>, String> {
        let commands = self.apply_commands(tick, self.finish.is_some())?;
        self.world.game.tick += 1;
        self.step_research(dt)
            .map_err(|e| format!("Classic research: {e:?}"))?;
        // Source's live Map admits a newborn later in this loop. Newborn mobile
        // units have no producer queue, so only captured real makers train here.
        let makers = self
            .production
            .buildings()
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        for id in makers {
            if self
                .world
                .game
                .units
                .get(&id)
                .is_some_and(|u| u.hp > 0. && u.built.is_some_and(|b| b >= 1.))
                && !self.classic_sudden_death()
            {
                self.step_training(id, dt)
                    .map_err(|e| format!("Classic training: {e:?}"))?;
            }
        }
        if construction {
            self.step_classic_depot_construction()?;
        }
        self.classic_income(dt)?;
        let classic = cfg.get("classic").ok_or("missing Classic rules")?;
        let decay = number(classic, "decay")?;
        let state = self
            .classic_runtime
            .as_mut()
            .ok_or("missing Classic runtime")?;
        if self.finish.is_none() {
            state.time_left -= dt;
        }
        if state.time_left <= 0. && !state.sudden_death {
            state.sudden_death = true;
            self.production.clear_queues();
        }
        if state.sudden_death {
            let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
            for unit in self.world.game.units.values_mut() {
                let definition = catalog
                    .unit(&self.rules.kinds[unit.kind].name)
                    .map_err(|e| e.to_string())?;
                if definition.get("produces").is_some() && unit.hp > 0. {
                    unit.hp -= number(definition, "hpPer")? * decay * dt;
                }
            }
        }
        Ok(commands)
    }

    fn classic_income(&mut self, dt: f64) -> Result<(), String> {
        use crate::economy_state::{Context, ModeContext, Node, Point as IncomePoint, Unit};
        let state = self
            .classic_runtime
            .as_ref()
            .ok_or("missing Classic runtime")?;
        let queues = self
            .world
            .game
            .units
            .keys()
            .map(|id| {
                self.production
                    .building(*id)
                    .map(|b| b.queue().collect::<Vec<_>>())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        let units = self
            .world
            .game
            .units
            .values()
            .zip(&queues)
            .map(|(u, queue)| Unit {
                id: u64::from(u.id),
                owner: Some(u.owner),
                kind: &self.rules.kinds[u.kind].name,
                hp: u.hp,
                built: u.built.unwrap_or(1.),
                queue,
            })
            .collect::<Vec<_>>();
        let nodes = state
            .nodes
            .iter()
            .map(|n| Node {
                depot: n.depot.map(u64::from),
                rate: n.rate,
                fuel: n.fuel,
                region: None,
            })
            .collect::<Vec<_>>();
        let points = self
            .start
            .points
            .iter()
            .map(|p| IncomePoint {
                owner: p.owner,
                cut: p.cut,
                mp: p.mp,
                vp: p.vp,
            })
            .collect::<Vec<_>>();
        self.start
            .economy
            .advance_income(
                &Context {
                    mode: ModeContext {
                        kind: Mode::Classic,
                        ..Default::default()
                    },
                    army: self.options.army,
                    units: &units,
                    drops: &[],
                    points: &points,
                    nodes: &nodes,
                    world: None,
                },
                dt,
                self.finish.is_some(),
            )
            .map_err(|e| format!("Classic income: {e}"))
    }
}

#[cfg(test)]
#[path = "classic_runtime_tests.rs"]
mod tests;
