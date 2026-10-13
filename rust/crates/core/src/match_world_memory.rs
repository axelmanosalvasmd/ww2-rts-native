//! World recipient knowledge producers over canonical Match state.
use crate::{
    catalog::{Catalog, Value},
    game::{CELL, Game, LOCAL_DEFENDER_OWNER, Point, Unit, Wreck},
    match_runtime::Match,
    math,
    route_knowledge::{LiveRouting, RouteKnowledge, RouteQuery},
    rules::Rules,
    vision::{airborne, los, team_sees_with_observer, vision_of},
    world_memory::{self, BoundUnit, KnownRegion, WorldLive, WorldRow},
};
use std::collections::{BTreeMap, BTreeSet};

pub struct WorldRouting<'a> {
    knowledge: &'a RouteKnowledge,
    live: &'a WorldLive,
}
impl WorldRouting<'_> {
    pub fn query(&self) -> RouteQuery<'_> {
        self.knowledge.query()
    }
    pub fn knowledge(&self) -> &RouteKnowledge {
        self.knowledge
    }
    pub fn live(&self) -> LiveRouting<'_> {
        self.live.live()
    }
    pub fn wrecks(&self) -> &[Wreck] {
        self.live.wrecks()
    }
}
fn seen(game: &Game, owner: usize, visible: &[u32], unit: &Unit) -> bool {
    game.reveal
        || game.allied(unit.owner, owner)
        || (visible.contains(&unit.id) && (unit.air.is_none() || airborne(unit)))
}
fn rounded(n: f64) -> f64 {
    math::round(n * 10.0) / 10.0
}
pub(crate) fn bound(u: &Unit) -> BoundUnit {
    BoundUnit {
        id: u.id,
        kind: u.kind,
        hp: u.hp,
        at: u.at(),
    }
}
fn update_live(live: &mut WorldLive, game: &Game, rules: &Rules) {
    for old in &mut live.units {
        if let Some(u) = game.units.get(&old.id) {
            *old = bound(u);
        }
    }
    live.bunkers = live
        .units
        .iter()
        .filter(|u| rules.kinds[u.kind].name == "bunker" && u.hp > 0.0)
        .map(|u| u.at)
        .collect();
    // Source wx is rebound every read, even when the per-tick live cache is reused.
    live.wetness = game.wet.unwrap_or(0.0);
}
impl Match {
    pub(crate) fn validate_world_memory_context(&self, owner: usize) -> Result<i32, String> {
        let runtime = self.world_runtime.as_ref().ok_or("missing World runtime")?;
        validate_world_context(
            WorldRoutingContext {
                world: &self.world,
                environment: &self.environment,
                rules: &self.rules,
                start: &self.start,
                rails: &runtime.rails,
                regions: &runtime.regions,
                known_buildings: &runtime.known_buildings,
                local_defender: &runtime.local_defender,
            },
            &runtime.memory,
            owner,
        )
    }
    /// Explicit vision phase. Routing and delivery reads never advance this pass.
    pub fn refresh_world_visibility(&mut self) -> Result<(), String> {
        self.validate_world_memory_context(LOCAL_DEFENDER_OWNER)?;
        crate::vision::update_vision(&mut self.world.game, &self.rules, &self.grid);
        let game = &self.world.game;
        let runtime = self.world_runtime.as_mut().unwrap();
        let mut teams = BTreeSet::new();
        for player in &game.players {
            if !teams.insert(player.team) {
                continue;
            }
            let at = runtime
                .known_buildings
                .iter()
                .position(|(t, _)| *t == player.team);
            let index = at.unwrap_or_else(|| {
                runtime.known_buildings.push((player.team, Vec::new()));
                runtime.known_buildings.len() - 1
            });
            let rows = &mut runtime.known_buildings[index].1;
            for id in &player.visible {
                let Some(u) = game
                    .units
                    .get(id)
                    .filter(|u| self.rules.kinds[u.kind].building)
                else {
                    continue;
                };
                let row = crate::match_world::KnownBuilding {
                    id: u.id,
                    kind: u.kind,
                    owner: u.owner,
                    at: u.at(),
                    built: u.built,
                };
                if let Some(old) = rows.iter_mut().find(|r| r.id == *id) {
                    *old = row;
                } else {
                    rows.push(row);
                }
            }
            rows.retain(|row| {
                player.visible.contains(&row.id)
                    || game.units.contains_key(&row.id)
                    || !game
                        .units
                        .values()
                        .filter(|u| game.team_of(u) == player.team)
                        .any(|u| {
                            let d = math::hypot(u.x - row.at.x, u.z - row.at.z);
                            (u.air.is_none() || airborne(u))
                                && d <= vision_of(game, &self.rules, u)
                                && (u.air.is_some()
                                    || self.rules.kinds[u.kind].building
                                    || los(game, &self.rules, (u.x, u.z), (row.at.x, row.at.z))
                                    || d < 8.0)
                        })
            });
        }
        let mut seen = BTreeSet::new();
        runtime.local_defender.visible.clear();
        for guard in game.units.values() {
            let k = &self.rules.kinds[guard.kind];
            if guard.owner != LOCAL_DEFENDER_OWNER
                || (k.building && !k.weapon && !k.lookout)
                || guard.built.is_some_and(|b| b < 1.0)
            {
                continue;
            }
            for id in self.grid.in_order(
                &game.units,
                (guard.x, guard.z),
                vision_of(game, &self.rules, guard),
                |_, u| u.owner != LOCAL_DEFENDER_OWNER && u.hp > 0.0,
            ) {
                let enemy = &game.units[&id];
                if (k.building || los(game, &self.rules, (guard.x, guard.z), (enemy.x, enemy.z)))
                    && seen.insert(id)
                {
                    runtime.local_defender.visible.push(id);
                }
            }
        }
        Ok(())
    }
    pub fn refresh_world_memory(&mut self, owner: usize) -> Result<(), String> {
        let team = self.validate_world_memory_context(owner)?;
        let runtime = self.world_runtime.as_mut().unwrap();
        runtime
            .memory
            .refresh(&mut self.world, &self.rules, team, &runtime.rails)?;
        Ok(())
    }
    /// Shared team memory, independent commander send cursor. The local actor has no seat.
    pub fn world_terrain(&mut self, owner: usize, full: bool) -> Result<Vec<WorldRow>, String> {
        if owner == LOCAL_DEFENDER_OWNER {
            return Err("local defender has no terrain delivery cursor".into());
        }
        self.refresh_world_memory(owner)?;
        let team = self.world.game.players[owner].team;
        self.world_runtime
            .as_mut()
            .unwrap()
            .memory
            .terrain_for(team, owner, full)
    }
    fn observe_world_wrecks(
        &mut self,
        owner: usize,
        full_facts: bool,
    ) -> Result<(Vec<Wreck>, Vec<crate::recipient_lists::WreckRow>), String> {
        let team = self.validate_world_memory_context(owner)?;
        let game = &self.world.game;
        let accepted = collect_world_wrecks(game, &self.rules, team, None, &self.environment)?;
        let prior = self
            .world_runtime
            .as_ref()
            .unwrap()
            .memory
            .team(team)
            .map_or(&[][..], |m| m.wrecks.as_slice());
        let rows = staged_world_wreck_memory(prior, game, &self.rules, team, None, accepted);
        let display = if full_facts {
            crate::recipient_lists::display_rows(&rows)?
        } else {
            Vec::new()
        };
        // All new fallible display work precedes terrain/Fog observation changes.
        self.refresh_world_memory(owner)?;
        let memory = self
            .world_runtime
            .as_mut()
            .unwrap()
            .memory
            .team_mut(team)
            .expect("successful refresh creates team memory");
        memory.wreck_queries = rows.iter().map(|row| row.query.clone()).collect();
        memory.wrecks = rows;
        Ok((memory.wreck_queries.clone(), display))
    }
    pub fn world_wrecks(&mut self, owner: usize) -> Result<Vec<Wreck>, String> {
        Ok(self.observe_world_wrecks(owner, false)?.0)
    }
    /// Exact team-observed display rows; imported missing facts reject before collection.
    pub fn world_wreck_rows(
        &mut self,
        owner: usize,
    ) -> Result<Vec<crate::recipient_lists::WreckRow>, String> {
        if owner == LOCAL_DEFENDER_OWNER {
            return Err("local defender has no snapshot seat".into());
        }
        Ok(self.observe_world_wrecks(owner, true)?.1)
    }
    pub fn refresh_world_routing(&mut self, owner: usize) -> Result<(), String> {
        let team = self.validate_world_memory_context(owner)?;
        let visible = self.world_actor_view(owner)?.visible.to_vec();
        let runtime = self.world_runtime.as_mut().unwrap();
        let receipt = refresh_world_parts(
            WorldRoutingContext {
                world: &self.world,
                environment: &self.environment,
                rules: &self.rules,
                start: &self.start,
                rails: &runtime.rails,
                regions: &runtime.regions,
                known_buildings: &runtime.known_buildings,
                local_defender: &runtime.local_defender,
            },
            &mut runtime.memory,
            owner,
            team,
            &visible,
            None,
            false,
        )?;
        receipt.commit(&mut self.world)
    }
    /// Borrow after refresh, so consumers can split Match's other mutable fields.
    pub fn world_routing(&self, owner: usize) -> Result<WorldRouting<'_>, String> {
        let actor = self.world_actor_view(owner)?;
        let memory = &self.world_runtime.as_ref().unwrap().memory;
        Ok(WorldRouting {
            knowledge: memory
                .team(actor.team)
                .ok_or("World memory has not been refreshed")?
                .knowledge(),
            live: memory
                .live_for(owner)
                .ok_or("World routing has not been refreshed")?,
        })
    }
    /// Preserve the final referenced object values after canonical removal.
    pub fn retain_removed_world_route_unit(&mut self, unit: &Unit) -> Result<(), String> {
        self.validate_world_memory_context(unit.owner)?;
        if self.world.game.units.contains_key(&unit.id)
            || unit.kind >= self.rules.kinds.len()
            || !unit.x.is_finite()
            || !unit.z.is_finite()
            || !unit.hp.is_finite()
        {
            return Err("invalid removed World route unit".into());
        }
        for live in self
            .world_runtime
            .as_mut()
            .unwrap()
            .memory
            .lives
            .values_mut()
        {
            if let Some(old) = live.units.iter_mut().find(|u| u.id == unit.id) {
                *old = bound(unit);
            }
        }
        Ok(())
    }
    /// Remembered region rows only. Full World HUD/economy/recovery is separate.
    pub fn world_regions(&mut self, owner: usize) -> Result<Vec<KnownRegion>, String> {
        if owner == LOCAL_DEFENDER_OWNER {
            return Err("local defender has no region snapshot seat".into());
        }
        self.refresh_world_memory(owner)?;
        let rows =
            self.project_world_regions(owner, &self.world_runtime.as_ref().unwrap().memory)?;
        let team = self.world.game.players[owner].team;
        self.world_runtime
            .as_mut()
            .unwrap()
            .memory
            .team_mut(team)
            .unwrap()
            .regions = rows.clone();
        Ok(rows)
    }
    /// Reuse the canonical source selector against an uncommitted memory candidate.
    pub(crate) fn project_world_regions(
        &self,
        owner: usize,
        candidate: &world_memory::WorldMemory,
    ) -> Result<Vec<KnownRegion>, String> {
        let game = &self.world.game;
        let team = game.players[owner].team;
        let visible = &game.players[owner].visible;
        let runtime = self.world_runtime.as_ref().unwrap();
        let geography = self.world.geography().unwrap();
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let radius = catalog
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("pointRadius")
            .and_then(Value::number)
            .ok_or("missing point radius")?;
        let produces = self
            .rules
            .kinds
            .iter()
            .map(|k| {
                catalog.unit(&k.name).map(|v| {
                    v.get("produces")
                        .is_some_and(|p| !matches!(p, Value::Bool(false) | Value::Null))
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let memory = candidate.team(team).ok_or("missing World region memory")?;
        let runs = geography
            .region_map
            .as_ref()
            .map(|r| territory_runs(r, memory.rows().iter().map(|r| r.observed.row.cell), game.w));
        let mut rows = memory.regions().to_vec();
        let base_rows = game
            .units
            .values()
            .filter(|u| seen(game, owner, visible, u))
            .map(|u| (u.kind, u.owner, u.at()))
            .chain(
                runtime
                    .known_buildings
                    .iter()
                    .find(|(t, _)| *t == team)
                    .into_iter()
                    .flat_map(|(_, r)| r)
                    .map(|b| (b.kind, b.owner, b.at)),
            )
            .collect::<Vec<_>>();
        let vis = &candidate.fog.teams[&team].vis;
        for (i, r) in geography.regions.iter().enumerate() {
            let live = &runtime.regions[i];
            let at = Point {
                x: (r.x as f64 + 0.5) * CELL,
                z: (r.y as f64 + 0.5) * CELL,
            };
            let current = game.reveal || vis[r.y * game.w + r.x] != 0;
            let old = rows.iter().find(|v| v.id == r.id);
            if !current && live.team != team && old.is_none_or(|v| v.team != team) {
                continue;
            }
            let locked = base_rows.iter().any(|&(kind, owner, b)| {
                produces[kind]
                    && self
                        .world_region_index(b)
                        .expect("validated immutable World region context")
                        == Some(i)
                    && (self.rules.kinds[kind].name == "worldbase"
                        || game.players.get(owner).is_some_and(|p| p.team == live.team))
            });
            let contested = if current {
                if game.reveal {
                    live.contested
                } else {
                    game.units
                        .values()
                        .filter(|u| {
                            u.hp > 0.0
                                && !u.retreating
                                && !u.riding
                                && !self.logistics.get(u.id).is_some_and(|l| l.forced)
                                && self.rules.kinds[u.kind].infantry
                                && seen(game, owner, visible, u)
                                && math::hypot(u.x - at.x, u.z - at.z) <= radius
                        })
                        .map(|u| game.team_of(u))
                        .collect::<BTreeSet<_>>()
                        .len()
                        > 1
                }
            } else {
                old.is_some_and(|r| r.contested)
            };
            let row = KnownRegion {
                id: r.id,
                name: r.name.clone(),
                kind: r.kind.clone(),
                team: live.team,
                at,
                bounds: if runs.is_none() {
                    Some(r.bounds.map(|n| n as f64 * CELL))
                } else {
                    None
                },
                runs: runs
                    .as_ref()
                    .and_then(|m| m.get(&r.id))
                    .cloned()
                    .unwrap_or_default(),
                progress: if current {
                    rounded(live.progress)
                } else {
                    old.map_or(if live.team == team { 1.0 } else { 0.0 }, |r| r.progress)
                },
                capper: if current {
                    live.capper
                } else {
                    old.and_then(|r| r.capper)
                },
                locked,
                contested,
            };
            if let Some(old) = rows.iter_mut().find(|v| v.id == r.id) {
                *old = row;
            } else {
                rows.push(row);
            }
        }
        if let Some(runs) = runs {
            for row in &mut rows {
                row.runs = runs.get(&row.id).cloned().unwrap_or_default();
            }
        }
        Ok(rows)
    }
}
/// Group by raw region-map labels, exactly as source. Labels are not remapped to r.id.
fn territory_runs(
    map: &[i16],
    cells: impl Iterator<Item = usize>,
    width: usize,
) -> BTreeMap<i32, Vec<[usize; 3]>> {
    let mut groups: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
    for c in cells {
        if map[c] >= 0 {
            groups.entry(i32::from(map[c])).or_default().push(c);
        }
    }
    groups
        .into_iter()
        .map(|(id, mut cells)| {
            cells.sort_unstable();
            let mut runs: Vec<[usize; 3]> = Vec::new();
            for c in cells {
                let y = c / width;
                let x = c % width;
                if let Some(last) = runs.last_mut().filter(|r| r[0] == y && r[2] == x) {
                    last[2] += 1;
                } else {
                    runs.push([y, x, x + 1]);
                }
            }
            (id, runs)
        })
        .collect()
}

/// Canonical immutable World fields, split from its one memory owner.
#[derive(Clone, Copy)]
pub struct WorldRoutingContext<'a> {
    pub world: &'a crate::world::World,
    pub environment: &'a crate::environment::EnvironmentRuntime,
    pub rules: &'a Rules,
    pub start: &'a crate::match_runtime::StartPlan,
    pub rails: &'a crate::match_world_rails::WorldRails,
    pub regions: &'a [crate::match_world::RegionState],
    pub known_buildings: &'a [(i32, Vec<crate::match_world::KnownBuilding>)],
    pub local_defender: &'a crate::match_world::LocalDefender,
}
impl<'a> WorldRoutingContext<'a> {
    fn actor(self, owner: usize) -> Result<crate::match_world::WorldActorView<'a>, String> {
        if self.world.game.mode != Some(crate::game::ModeKind::World) {
            return Err("World routing requires World mode".into());
        }
        if owner == LOCAL_DEFENDER_OWNER {
            Ok(crate::match_world::WorldActorView {
                owner,
                team: -1,
                faction: self.local_defender.faction,
                spawn: self.local_defender.spawn,
                visible: &self.local_defender.visible,
            })
        } else {
            let player = self
                .world
                .game
                .players
                .get(owner)
                .ok_or("invalid World actor")?;
            let start = self
                .start
                .players
                .get(owner)
                .ok_or("invalid World commander")?;
            Ok(crate::match_world::WorldActorView {
                owner,
                team: player.team,
                faction: start.config.faction,
                spawn: start.spawn,
                visible: &player.visible,
            })
        }
    }
}
pub(crate) fn validate_world_context(
    context: WorldRoutingContext<'_>,
    memory: &world_memory::WorldMemory,
    owner: usize,
) -> Result<i32, String> {
    let actor = context.actor(owner)?;
    world_memory::validate(context.world, context.rules, actor.team)?;
    memory.fog.validate_shape(&context.world.game)?;
    let geography = context.world.geography().ok_or("missing World geography")?;
    if context.start.dimensions != (context.world.game.w, context.world.game.h)
        || context.regions.len() != geography.regions.len()
        || context.start.players.len() != context.world.game.players.len()
        || context
            .world
            .game
            .players
            .iter()
            .enumerate()
            .any(|(i, p)| p.slot != i || context.start.players[i].config.team != p.team)
        || context.regions.iter().any(|r| {
            !(-1..=5).contains(&r.team)
                || !r.progress.is_finite()
                || !(0.0..=1.0).contains(&r.progress)
                || r.capper.is_some_and(|s| s >= context.start.players.len())
        })
        || context.known_buildings.iter().any(|(t, rows)| {
            !(-1..=5).contains(t)
                || rows.iter().any(|b| {
                    b.kind >= context.rules.kinds.len()
                        || !b.at.x.is_finite()
                        || !b.at.z.is_finite()
                        || (b.owner >= context.start.players.len()
                            && b.owner != LOCAL_DEFENDER_OWNER)
                })
        })
        || memory.fog.teams.values().any(|f| {
            f.vis.len() != context.world.game.chars.len()
                || f.next.len() != context.world.game.chars.len()
                || f.explored.len() != context.world.game.chars.len()
        })
    {
        return Err("invalid World recipient binding or region facts".into());
    }
    let mut ids = BTreeSet::new();
    if context
        .world
        .game
        .wrecks
        .iter()
        .any(|w| w.id.is_none() || !ids.insert(w.id) || !w.x.is_finite() || !w.z.is_finite())
    {
        return Err("World wrecks require finite positions and unique source identities".into());
    }
    Ok(actor.team)
}
pub(crate) fn collect_world_wrecks(
    game: &Game,
    rules: &Rules,
    team: i32,
    observer: Option<crate::staged_observer::StagedObserver<'_>>,
    environment: &crate::environment::EnvironmentRuntime,
) -> Result<Vec<crate::recipient_lists::RememberedWreck>, String> {
    let grid = crate::grid::SpatialGrid::new(&game.units, 16.0);
    game.wrecks
        .iter()
        .filter(|w| {
            game.reveal
                || team_sees_with_observer(
                    game,
                    rules,
                    &grid,
                    team,
                    Point { x: w.x, z: w.z },
                    observer,
                )
        })
        .map(|w| {
            crate::recipient_lists::RememberedWreck::observe(
                game,
                rules,
                w,
                w.id.and_then(|id| environment.wreck_metadata().get(&id)),
            )
        })
        .collect()
}
pub(crate) fn staged_world_wreck_memory(
    prior: &[crate::recipient_lists::RememberedWreck],
    game: &Game,
    rules: &Rules,
    team: i32,
    observer: Option<crate::staged_observer::StagedObserver<'_>>,
    accepted: Vec<crate::recipient_lists::RememberedWreck>,
) -> Vec<crate::recipient_lists::RememberedWreck> {
    let grid = crate::grid::SpatialGrid::new(&game.units, 16.0);
    let visible = |at| team_sees_with_observer(game, rules, &grid, team, at, observer);
    // Source World removal ignores reveal; known motion still respects it.
    crate::recipient_lists::updated_wreck_memory(prior, game, accepted, visible, |at| {
        game.reveal || visible(at)
    })
}

fn refresh_world_parts(
    context: WorldRoutingContext<'_>,
    memory: &mut world_memory::WorldMemory,
    owner: usize,
    team: i32,
    visible: &[u32],
    observer: Option<crate::staged_observer::StagedObserver<'_>>,
    capture_frame: bool,
) -> Result<crate::world::ObservationReceipt, String> {
    let WorldRoutingContext {
        world,
        rules,
        rails,
        ..
    } = context;
    let cold_live = memory
        .live_for(owner)
        .is_none_or(|l| l.tick != Some(world.game.tick));
    let accepted = if cold_live {
        Some(collect_world_wrecks(
            &world.game,
            rules,
            team,
            observer,
            context.environment,
        )?)
    } else {
        None
    };
    let (_, receipt) = memory.refresh_impl(world, rules, team, rails, observer, capture_frame)?;
    memory.apply_routes(team)?;
    let game = &world.game;
    let tick = game.tick;
    if memory.live_for(owner).is_none_or(|l| l.tick != Some(tick)) {
        let team_memory = memory
            .team_mut(team)
            .expect("successful refresh creates team memory");
        let rows = staged_world_wreck_memory(
            &team_memory.wrecks,
            game,
            rules,
            team,
            observer,
            accepted.unwrap(),
        );
        team_memory.wreck_queries = rows.iter().map(|row| row.query.clone()).collect();
        team_memory.wrecks = rows;
        let vis = &memory
            .fog
            .team_with_observer(game, rules, team, observer)
            .vis;
        let mut fires = vec![false; game.chars.len()];
        for (&c, _) in world.fires.iter() {
            if vis[c] != 0 {
                fires[c] = true;
            }
        }
        let mut live = WorldLive {
            tick: Some(tick),
            units: game
                .units
                .values()
                .chain(observer.map(|o| o.unit()))
                .filter(|u| seen(game, owner, visible, u))
                .map(bound)
                .collect(),
            fires,
            bunkers: Vec::new(),
            wrecks: memory.team(team).unwrap().remembered_wrecks().to_vec(),
            wetness: game.wet.unwrap_or(0.0),
            roads: world.roads,
        };
        update_live(&mut live, game, rules);
        memory.lives.insert(owner, live);
    } else {
        update_live(memory.lives.get_mut(&owner).unwrap(), game, rules);
    }
    Ok(receipt)
}
/// A private prepared copy, never a second live memory owner. On abort, discard
/// any search scratch that observed its temporary terrain-buffer addresses.
#[must_use = "commit after every fallible check, or drop without changing World memory"]
pub struct PreparedWorldRouting<'a> {
    target: &'a mut world_memory::WorldMemory,
    state: world_memory::WorldMemory,
    owner: usize,
    team: i32,
    receipt: crate::world::ObservationReceipt,
    wreck_frame: crate::recipient_lists::WreckSourceFrame,
}
impl PreparedWorldRouting<'_> {
    /// Commit moves the copied arrays for every existing team, so all matching
    /// histories follow those allocations, including teams not read by this guard.
    pub fn fork_paths(
        &self,
        paths: &crate::pathfinding::Pathfinding,
    ) -> crate::pathfinding::Pathfinding {
        let mut fork = paths.clone();
        for (team, memory) in &self.target.teams {
            let prepared = self
                .state
                .team(*team)
                .expect("preview retains existing teams");
            fork.rebind_terrain(memory.query().navigation(), prepared.query().navigation());
        }
        fork
    }

    pub fn query(&self) -> RouteQuery<'_> {
        self.state.team(self.team).unwrap().query()
    }
    pub fn knowledge(&self) -> &RouteKnowledge {
        self.state.team(self.team).unwrap().knowledge()
    }
    pub fn live(&self) -> LiveRouting<'_> {
        self.state.live_for(self.owner).unwrap().live()
    }
    pub fn wrecks(&self) -> &[Wreck] {
        self.state.live_for(self.owner).unwrap().wrecks()
    }
}
pub struct WorldRoutingCommit<'a, 'w, 'e> {
    _environment: &'e crate::environment::EnvironmentRuntime,
    target: &'a mut world_memory::WorldMemory,
    state: world_memory::WorldMemory,
    owner: usize,
    team: i32,
    ledger: crate::world::PreparedObservation<'w>,
}
impl WorldRoutingCommit<'_, '_, '_> {
    pub(crate) fn query(&self) -> RouteQuery<'_> {
        self.state.team(self.team).unwrap().query()
    }
    pub(crate) fn knowledge(&self) -> &RouteKnowledge {
        self.state.team(self.team).unwrap().knowledge()
    }
    pub(crate) fn live(&self) -> LiveRouting<'_> {
        self.state.live_for(self.owner).unwrap().live()
    }
    pub(crate) fn wrecks(&self) -> &[Wreck] {
        self.state.live_for(self.owner).unwrap().wrecks()
    }
    pub(crate) fn failure_version(&self) -> u64 {
        self.state.team(self.team).unwrap().version()
    }
    pub fn commit(self) {
        *self.target = self.state;
        self.ledger.commit();
    }
}
impl<'a> PreparedWorldRouting<'a> {
    pub fn prepare_publication<'w, 'e>(
        self,
        world: &'w mut crate::world::World,
        environment: &'e crate::environment::EnvironmentRuntime,
    ) -> Result<(&'w mut Game, WorldRoutingCommit<'a, 'w, 'e>), String> {
        let Self {
            target,
            state,
            receipt,
            wreck_frame,
            owner,
            team,
        } = self;
        wreck_frame.validate(environment)?;
        let (game, ledger) = receipt.prepare(world)?;
        Ok((
            game,
            WorldRoutingCommit {
                _environment: environment,
                target,
                state,
                owner,
                team,
                ledger,
            },
        ))
    }
}
pub fn prepare_world_routing<'a>(
    context: WorldRoutingContext<'_>,
    target: &'a mut world_memory::WorldMemory,
    owner: usize,
    observer: Option<crate::staged_observer::StagedObserver<'_>>,
) -> Result<PreparedWorldRouting<'a>, String> {
    let team = validate_world_context(context, target, owner)?;
    let actor = context.actor(owner)?;
    if let Some(source) = observer {
        crate::staged_observer::StagedObserver::new(
            &context.world.game,
            context.rules,
            source.unit().id,
            owner,
            source.unit(),
        )?;
    }
    let mut state = target.clone();
    let receipt = refresh_world_parts(
        context,
        &mut state,
        owner,
        team,
        actor.visible,
        observer,
        true,
    )?;
    Ok(PreparedWorldRouting {
        target,
        state,
        owner,
        team,
        receipt,
        wreck_frame: crate::recipient_lists::WreckSourceFrame::capture(
            &context.world.game,
            context.environment,
        ),
    })
}
