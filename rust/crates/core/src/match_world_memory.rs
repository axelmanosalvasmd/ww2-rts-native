//! World recipient knowledge producers over canonical Match state.
use crate::{
    catalog::{Catalog, Value},
    game::{CELL, Game, LOCAL_DEFENDER_OWNER, Point, Unit, Wreck},
    grid::SpatialGrid,
    match_runtime::Match,
    math,
    route_knowledge::{LiveRouting, RouteKnowledge, RouteQuery},
    rules::Rules,
    vision::{airborne, los, vision_of},
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
fn sees(game: &Game, rules: &Rules, grid: &SpatialGrid, team: i32, at: Point) -> bool {
    let reach = game
        .units
        .values()
        .filter(|u| game.team_of(u) == team)
        .map(|u| vision_of(game, rules, u))
        .fold(1.0, math::max);
    grid.unordered(&game.units, (at.x, at.z), reach, |_, u| {
        game.team_of(u) == team
    })
    .into_iter()
    .any(|id| {
        let u = &game.units[&id];
        math::hypot(u.x - at.x, u.z - at.z) <= vision_of(game, rules, u)
            && if u.air.is_some() {
                airborne(u)
            } else {
                rules.kinds[u.kind].building || los(game, rules, (u.x, u.z), (at.x, at.z))
            }
    })
}
fn rounded(n: f64) -> f64 {
    math::round(n * 10.0) / 10.0
}
fn bound(u: &Unit) -> BoundUnit {
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
        let actor = self.world_actor_view(owner)?;
        world_memory::validate(&self.world, &self.rules, actor.team)?;
        let runtime = self.world_runtime.as_ref().unwrap();
        let geography = self.world.geography().ok_or("missing World geography")?;
        if runtime.regions.len() != geography.regions.len()
            || self.start.players.len() != self.world.game.players.len()
            || self
                .world
                .game
                .players
                .iter()
                .enumerate()
                .any(|(i, p)| p.slot != i || self.start.players[i].config.team != p.team)
            || runtime.regions.iter().any(|r| {
                !(-1..=5).contains(&r.team)
                    || !r.progress.is_finite()
                    || !(0.0..=1.0).contains(&r.progress)
                    || r.capper.is_some_and(|s| s >= self.start.players.len())
            })
            || runtime.known_buildings.iter().any(|(t, rows)| {
                !(-1..=5).contains(t)
                    || rows.iter().any(|b| {
                        b.kind >= self.rules.kinds.len()
                            || !b.at.x.is_finite()
                            || !b.at.z.is_finite()
                            || (b.owner >= self.start.players.len()
                                && b.owner != LOCAL_DEFENDER_OWNER)
                    })
            })
            || runtime.memory.fog.teams.values().any(|f| {
                f.vis.len() != self.world.game.chars.len()
                    || f.next.len() != self.world.game.chars.len()
                    || f.explored.len() != self.world.game.chars.len()
            })
        {
            return Err("invalid World recipient binding or region facts".into());
        }
        let mut ids = BTreeSet::new();
        if self
            .world
            .game
            .wrecks
            .iter()
            .any(|w| w.id.is_none() || !ids.insert(w.id) || !w.x.is_finite() || !w.z.is_finite())
        {
            return Err(
                "World wrecks require finite positions and unique source identities".into(),
            );
        }
        Ok(actor.team)
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
    fn refresh_world_wrecks(&mut self, owner: usize) -> Result<(), String> {
        let team = self.validate_world_memory_context(owner)?;
        let game = &self.world.game;
        let visible = |at| sees(game, &self.rules, &self.grid, team, at);
        let memory = &mut self
            .world_runtime
            .as_mut()
            .unwrap()
            .memory
            .team_mut(team)
            .ok_or("World memory has not been refreshed")?
            .wrecks;
        for current in &game.wrecks {
            if !game.reveal
                && !visible(Point {
                    x: current.x,
                    z: current.z,
                })
            {
                continue;
            }
            let row = Wreck {
                id: current.id,
                cell: None,
                x: rounded(current.x),
                z: rounded(current.z),
                moving: current.moving,
            };
            if let Some(old) = memory.iter_mut().find(|r| r.id == current.id) {
                *old = row;
            } else {
                memory.push(row);
            }
        }
        // World reveal discovers wrecks, but source removal still asks current teamSees.
        memory.retain(|row| {
            game.wrecks.iter().any(|w| w.id == row.id) || !visible(Point { x: row.x, z: row.z })
        });
        for row in memory {
            if row.moving && !(game.reveal || visible(Point { x: row.x, z: row.z })) {
                row.moving = false;
            }
        }
        Ok(())
    }
    pub fn world_wrecks(&mut self, owner: usize) -> Result<Vec<Wreck>, String> {
        self.refresh_world_memory(owner)?;
        self.refresh_world_wrecks(owner)?;
        let team = self.world_actor_view(owner)?.team;
        Ok(self
            .world_runtime
            .as_ref()
            .unwrap()
            .memory
            .team(team)
            .unwrap()
            .remembered_wrecks()
            .to_vec())
    }
    pub fn refresh_world_routing(&mut self, owner: usize) -> Result<(), String> {
        self.refresh_world_memory(owner)?;
        let actor = self.world_actor_view(owner)?;
        let team = actor.team;
        let visible = actor.visible.to_vec();
        let tick = self.world.game.tick;
        self.world_runtime
            .as_mut()
            .unwrap()
            .memory
            .apply_routes(team)?;
        let fresh = self
            .world_runtime
            .as_ref()
            .unwrap()
            .memory
            .live_for(owner)
            .is_none_or(|l| l.tick != Some(tick));
        if fresh {
            self.refresh_world_wrecks(owner)?;
            let game = &self.world.game;
            let memory = &mut self.world_runtime.as_mut().unwrap().memory;
            let vis = &memory.fog.team(game, &self.rules, team).vis;
            let mut fires = vec![false; game.chars.len()];
            for (&c, _) in self.world.fires.iter() {
                if vis[c] != 0 {
                    fires[c] = true;
                }
            }
            let mut live = WorldLive {
                tick: Some(tick),
                units: game
                    .units
                    .values()
                    .filter(|u| seen(game, owner, &visible, u))
                    .map(bound)
                    .collect(),
                fires,
                bunkers: Vec::new(),
                wrecks: memory.team(team).unwrap().remembered_wrecks().to_vec(),
                wetness: game.wet.unwrap_or(0.0),
                roads: self.world.roads,
            };
            update_live(&mut live, game, &self.rules);
            memory.lives.insert(owner, live);
        } else {
            update_live(
                self.world_runtime
                    .as_mut()
                    .unwrap()
                    .memory
                    .lives
                    .get_mut(&owner)
                    .unwrap(),
                &self.world.game,
                &self.rules,
            );
        }
        Ok(())
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
        let memory = runtime.memory.team(team).unwrap();
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
        let vis = &runtime.memory.fog.teams[&team].vis;
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
                    && region_at(geography, game.w, game.h, b) == Some(i)
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
        self.world_runtime
            .as_mut()
            .unwrap()
            .memory
            .team_mut(team)
            .unwrap()
            .regions = rows.clone();
        Ok(rows)
    }
}
fn region_at(
    w: &crate::world_map::WorldDefinition,
    width: usize,
    height: usize,
    at: Point,
) -> Option<usize> {
    let x = math::floor(at.x / CELL);
    let y = math::floor(at.z / CELL);
    if x < 0.0 || y < 0.0 || x >= width as f64 || y >= height as f64 {
        return None;
    }
    if let Some(map) = &w.region_map {
        let i = map[y as usize * width + x as usize];
        return (i >= 0).then_some(i as usize);
    }
    let cols = width / w.region_cells;
    let i = (y as usize / w.region_cells) * cols + x as usize / w.region_cells;
    (i < w.regions.len()).then_some(i)
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
