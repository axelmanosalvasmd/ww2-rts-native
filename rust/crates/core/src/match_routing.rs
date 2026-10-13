//! Conquest recipient routing. Discovery queries current units, not fog masks.
use crate::{
    economy::Mode,
    game::{CELL, Game, Point, Unit, Wreck},
    grid::SpatialGrid,
    match_runtime::{BuildingCell, Match},
    math,
    observed_memory::{
        FootprintFact, ObservationFacts, ObservationFrame, ObservedMemory, ObservedRow,
    },
    route_knowledge::{InitialTerrain, LiveRouting, RouteKnowledge, RouteQuery},
    rules::Rules,
    vision::{airborne, unit_sees, vision_of},
    world::{Recipient, World},
};
use std::collections::BTreeMap;

/// One owner's remembered rows and routing projection. Refresh through Match;
/// consumers borrow this state separately from mutable units or path scratch.
#[derive(Clone)]
pub struct RecipientRouting {
    memory: ObservedMemory,
    routes: RouteKnowledge,
    key: Option<(i64, u32, i64)>,
    fires: Vec<bool>,
    bunkers: Vec<Point>,
    bound_units: Vec<BoundUnit>,
    wetness: f64,
    roads: bool,
    wreck_memory: Vec<crate::recipient_lists::RememberedWreck>,
    wreck_queries: Vec<Wreck>,
    wrecks: Vec<Wreck>,
}
impl RecipientRouting {
    pub fn knowledge(&self) -> &RouteKnowledge {
        &self.routes
    }
    pub fn query(&self) -> RouteQuery<'_> {
        self.routes.query()
    }
    pub fn live(&self) -> LiveRouting<'_> {
        LiveRouting {
            fires: &self.fires,
            bunkers: &self.bunkers,
            wetness: self.wetness,
            roads: self.roads,
        }
    }
    pub fn wrecks(&self) -> &[Wreck] {
        &self.wrecks
    }
    pub fn remembered_wrecks(&self) -> &[Wreck] {
        &self.wreck_queries
    }
    pub fn remembered_wreck_rows(&self) -> Result<Vec<crate::recipient_lists::WreckRow>, String> {
        self.wreck_memory
            .iter()
            .map(|row| {
                row.display.clone().ok_or_else(|| {
                    format!(
                        "missing observed wreck display facts: {}",
                        row.query.id.unwrap_or(0)
                    )
                })
            })
            .collect()
    }
    pub fn remembered(&self) -> &[ObservedRow] {
        self.memory.full_rows()
    }
}
#[derive(Clone, Copy)]
struct BoundUnit {
    id: u32,
    kind: usize,
    hp: f64,
    at: Point,
}
impl BoundUnit {
    fn from_unit(u: &Unit) -> Self {
        Self {
            id: u.id,
            kind: u.kind,
            hp: u.hp,
            at: u.at(),
        }
    }
}
impl RecipientRouting {
    fn refresh_bound_units(&mut self, game: &Game, rules: &Rules) {
        for bound in &mut self.bound_units {
            if let Some(unit) = game.units.get(&bound.id) {
                *bound = BoundUnit::from_unit(unit);
            }
        }
        self.bunkers = self
            .bound_units
            .iter()
            .filter(|u| rules.kinds[u.kind].name == "bunker" && u.hp > 0.0)
            .map(|u| u.at)
            .collect();
    }
}
#[derive(Default)]
struct Facts {
    visible: BTreeMap<usize, bool>,
    known: BTreeMap<usize, bool>,
    owned: BTreeMap<usize, bool>,
    footprints: BTreeMap<usize, FootprintFact>,
}
impl ObservationFacts for Facts {
    fn cell_visible(&mut self, c: usize, _: i32) -> bool {
        self.visible.get(&c).copied().unwrap_or(false)
    }
    fn mine_known(&mut self, c: usize, _: i32) -> bool {
        self.known.get(&c).copied().unwrap_or(false)
    }
    fn mine_owned(&mut self, c: usize, _: i32) -> bool {
        self.owned.get(&c).copied().unwrap_or(false)
    }
    fn footprint(&mut self, c: usize, _: usize, _: i32) -> Option<FootprintFact> {
        self.footprints.get(&c).copied()
    }
}
fn sees(
    game: &Game,
    rules: &Rules,
    grid: &SpatialGrid,
    team: i32,
    reach: f64,
    at: Point,
    observer: Option<crate::staged_observer::StagedObserver<'_>>,
) -> bool {
    grid.unordered(&game.units, (at.x, at.z), reach, |_, u| {
        game.team_of(u) == team
    })
    .into_iter()
    .any(|id| unit_sees(game, rules, &game.units[&id], at))
        || observer.is_some_and(|source| {
            game.team_of(source.unit()) == team && unit_sees(game, rules, source.unit(), at)
        })
}

fn facts(
    world: &World,
    rules: &Rules,
    grid: &SpatialGrid,
    buildings: &BTreeMap<usize, BuildingCell>,
    slot: usize,
    observer: Option<crate::staged_observer::StagedObserver<'_>>,
) -> Result<Facts, String> {
    let game = &world.game;
    let team = game.players[slot].team;
    let reach = game
        .units
        .values()
        .filter(|u| game.team_of(u) == team)
        .map(|u| vision_of(game, rules, u))
        .fold(1.0, math::max);
    let mut result = Facts::default();
    let mut cached = BTreeMap::new();
    let pending: Vec<_> = world.pending(Recipient::Player(slot)).map_or_else(
        || (0..world.cell_log().len()).collect(),
        |v| v.iter().copied().collect(),
    );
    for index in pending {
        let c = world
            .cell_log()
            .get(index)
            .ok_or("invalid terrain pending index")?
            .cell;
        if c >= game.w * game.h {
            return Err("invalid pending terrain cell".into());
        }
        let owner = world.mines.get(&c).copied().unwrap_or(-1);
        let owned = owner >= 0 && game.players[owner as usize].team == team;
        result.owned.insert(c, owned);
        result.known.insert(
            c,
            owned || (world.mine_seen.get(&c).copied().unwrap_or(0) >> team & 1) == 1,
        );
        if let Some(b) = buildings.get(&c) {
            let fact = *cached.entry(b.unit).or_insert_with(|| {
                let live = game.units.get(&b.unit);
                let owner = live.map_or(b.owner, |u| u.owner);
                let at = live.map_or(b.at, |u| u.at());
                FootprintFact {
                    identity: u64::from(b.unit),
                    allied: game.allied(owner, slot),
                    live: live.is_some(),
                    visible: game.players[slot].sees(b.unit),
                    ruin_visible: sees(game, rules, grid, team, reach, at, observer),
                }
            });
            result.footprints.insert(c, fact);
        } else {
            result.visible.insert(
                c,
                sees(
                    game,
                    rules,
                    grid,
                    team,
                    reach,
                    Point {
                        x: (c % game.w) as f64 * CELL + CELL / 2.0,
                        z: (c / game.w) as f64 * CELL + CELL / 2.0,
                    },
                    observer,
                ),
            );
        }
    }
    Ok(result)
}
fn collect_wrecks(
    game: &Game,
    rules: &Rules,
    grid: &SpatialGrid,
    owner: usize,
    observer: Option<crate::staged_observer::StagedObserver<'_>>,
    environment: &crate::environment::EnvironmentRuntime,
) -> Result<Vec<crate::recipient_lists::RememberedWreck>, String> {
    let team = game.players[owner].team;
    let reach = game
        .units
        .values()
        .filter(|u| game.team_of(u) == team)
        .map(|u| vision_of(game, rules, u))
        .fold(1.0, math::max);
    game.wrecks
        .iter()
        .filter(|w| {
            game.reveal
                || sees(
                    game,
                    rules,
                    grid,
                    team,
                    reach,
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
fn staged_wreck_memory(
    prior: &[crate::recipient_lists::RememberedWreck],
    game: &Game,
    rules: &Rules,
    grid: &SpatialGrid,
    owner: usize,
    observer: Option<crate::staged_observer::StagedObserver<'_>>,
    accepted: Vec<crate::recipient_lists::RememberedWreck>,
) -> Vec<crate::recipient_lists::RememberedWreck> {
    let team = game.players[owner].team;
    let reach = game
        .units
        .values()
        .filter(|u| game.team_of(u) == team)
        .map(|u| vision_of(game, rules, u))
        .fold(1.0, math::max);
    let visible = |at| game.reveal || sees(game, rules, grid, team, reach, at, observer);
    crate::recipient_lists::updated_wreck_memory(prior, game, accepted, visible, visible)
}
fn publish_wreck_memory(
    state: &mut RecipientRouting,
    rows: Vec<crate::recipient_lists::RememberedWreck>,
) {
    state.wreck_queries = rows.iter().map(|row| row.query.clone()).collect();
    state.wreck_memory = rows;
}

impl Match {
    pub(crate) fn initialize_recipient_routing(&mut self) -> Result<(), String> {
        self.validate_routing_context()?;
        let w = &self.world;
        let i = w.initial();
        let shoals = w
            .shoal
            .as_ref()
            .map(|v| v.iter().map(|&v| v != 0).collect::<Vec<_>>());
        self.recipients = (0..w.game.players.len())
            .map(|slot| {
                let memory = ObservedMemory::new(w, slot, slot as u64)
                    .map_err(|e| format!("observed memory: {e:?}"))?;
                let routes = RouteKnowledge::observed(
                    InitialTerrain {
                        width: w.game.w,
                        height: w.game.h,
                        chars: &i.chars,
                        ground: Some(&i.ground),
                        objects: Some(&i.objects),
                        heights: i.height.as_deref(),
                        shoals: shoals.as_deref(),
                        house_kinds: Some(&w.game.house),
                        toughness: w.game.tough.as_deref(),
                        trench_front: i.trench_front.as_deref(),
                    },
                    slot as u64,
                    self.rules.clone(),
                )
                .map_err(|e| format!("observed routes: {e:?}"))?;
                Ok(RecipientRouting {
                    memory,
                    routes,
                    key: None,
                    fires: vec![false; w.game.w * w.game.h],
                    bunkers: Vec::new(),
                    bound_units: Vec::new(),
                    wetness: 0.0,
                    roads: w.roads,
                    wreck_memory: Vec::new(),
                    wreck_queries: Vec::new(),
                    wrecks: Vec::new(),
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(())
    }
    pub(crate) fn validate_routing_context(&self) -> Result<(), String> {
        validate_context(
            RecipientContext {
                world: &self.world,
                rules: &self.rules,
                building_cells: &self.building_cells,
                start: &self.start,
                options: &self.options,
                environment: &self.environment,
            },
            self.recipients.iter(),
            &self.routing_fog,
        )
    }
    pub(crate) fn validate_recipient(&self, owner: usize) -> Result<(), String> {
        self.validate_routing_context()?;
        if self.recipients.len() != self.world.game.players.len() {
            return Err("recipient routing table mismatch".into());
        }
        let state = self
            .recipients
            .get(owner)
            .ok_or("invalid routing recipient")?;
        if state.memory.slot() != owner || state.memory.identity() != owner as u64 {
            return Err("routing owner mismatch".into());
        }
        state
            .memory
            .validate_binding(&self.world)
            .map_err(|e| format!("routing binding: {e:?}"))
    }
    fn refresh_memory(&mut self, owner: usize) -> Result<(), String> {
        self.validate_recipient(owner)?;
        let receipt = refresh_memory_parts(
            &self.world,
            &self.rules,
            &self.building_cells,
            &mut self.recipients[owner],
            owner,
            None,
            false,
        )?;
        receipt.commit(&mut self.world)
    }
    /// Refresh before borrowing recipient query/live and mutable unit or search fields.
    pub fn refresh_recipient_routing(&mut self, owner: usize) -> Result<(), String> {
        self.validate_recipient(owner)?;
        let receipt = refresh_recipient_parts(
            RecipientObservation {
                world: &self.world,
                rules: &self.rules,
                building_cells: &self.building_cells,
                environment: &self.environment,
            },
            &mut self.routing_fog,
            &mut self.recipients[owner],
            owner,
        )?;
        receipt.commit(&mut self.world)
    }
    /// Preserve the fields the original cached view still reads from a removed
    /// unit object. The lifecycle owner calls this with the final removed unit.
    pub fn retain_removed_route_unit(&mut self, unit: &Unit) -> Result<(), String> {
        self.validate_routing_context()?;
        if unit.owner >= self.world.game.players.len()
            || unit.kind >= self.rules.kinds.len()
            || !unit.x.is_finite()
            || !unit.z.is_finite()
            || !unit.hp.is_finite()
        {
            return Err("invalid removed routing unit".into());
        }
        for state in &mut self.recipients {
            if let Some(bound) = state.bound_units.iter_mut().find(|u| u.id == unit.id) {
                *bound = BoundUnit::from_unit(unit);
            }
            state.refresh_bound_units(&self.world.game, &self.rules);
        }
        for footprint in self
            .building_cells
            .values_mut()
            .filter(|b| b.unit == unit.id)
        {
            footprint.owner = unit.owner;
            footprint.at = unit.at();
        }
        Ok(())
    }

    /// The source teamSees point predicate at current positions. This is not a
    /// fog-mask query or a remembered-wreck/smoke policy.
    pub fn recipient_sees(&mut self, owner: usize, at: Point) -> Result<bool, String> {
        self.validate_recipient(owner)?;
        if !at.x.is_finite() || !at.z.is_finite() {
            return Err("invalid visibility point".into());
        }
        let observation_grid = SpatialGrid::new(&self.world.game.units, 16.0);
        let g = &self.world.game;
        let team = g.players[owner].team;
        let reach = g
            .units
            .values()
            .filter(|u| g.team_of(u) == team)
            .map(|u| vision_of(g, &self.rules, u))
            .fold(1.0, math::max);
        Ok(sees(
            g,
            &self.rules,
            &observation_grid,
            team,
            reach,
            at,
            None,
        ))
    }

    /// Query an already refreshed owner state without borrowing all of Match mutably.
    pub fn recipient_routing(&self, owner: usize) -> Result<&RecipientRouting, String> {
        self.validate_recipient(owner)?;
        Ok(&self.recipients[owner])
    }
    /// Full remembered wreck projection for the fields used by core cover queries.
    /// Hidden moving wrecks become static remembered cover; removal waits until seen.
    pub fn observed_wrecks(&mut self, owner: usize) -> Result<Vec<Wreck>, String> {
        self.validate_recipient(owner)?;
        let observation_grid = SpatialGrid::new(&self.world.game.units, 16.0);
        let game = &self.world.game;
        let accepted = collect_wrecks(
            game,
            &self.rules,
            &observation_grid,
            owner,
            None,
            &self.environment,
        )?;
        let rows = staged_wreck_memory(
            &self.recipients[owner].wreck_memory,
            game,
            &self.rules,
            &observation_grid,
            owner,
            None,
            accepted,
        );
        publish_wreck_memory(&mut self.recipients[owner], rows);
        Ok(self.recipients[owner].wreck_queries.clone())
    }

    /// Full display list. Missing imported facts reject before memory publication.
    pub fn observed_wreck_rows(
        &mut self,
        owner: usize,
    ) -> Result<Vec<crate::recipient_lists::WreckRow>, String> {
        self.validate_recipient(owner)?;
        let g = &self.world.game;
        let grid = SpatialGrid::new(&g.units, 16.0);
        let accepted = collect_wrecks(g, &self.rules, &grid, owner, None, &self.environment)?;
        let rows = staged_wreck_memory(
            &self.recipients[owner].wreck_memory,
            g,
            &self.rules,
            &grid,
            owner,
            None,
            accepted,
        );
        let display = crate::recipient_lists::display_rows(&rows)?;
        publish_wreck_memory(&mut self.recipients[owner], rows);
        Ok(display)
    }

    /// Full/reconnect reads preserve memory order; delta reads drain only its outbox.
    pub fn observed_terrain(
        &mut self,
        owner: usize,
        full: bool,
    ) -> Result<Vec<ObservedRow>, String> {
        self.refresh_memory(owner)?;
        let memory = &mut self.recipients[owner].memory;
        Ok(if full {
            memory.full_rows().to_vec()
        } else {
            memory.take_changes()
        })
    }
}

/// Canonical immutable fields, split from the selected recipient and Fog owners.
#[derive(Clone, Copy)]
pub struct RecipientContext<'a> {
    pub world: &'a World,
    pub rules: &'a Rules,
    pub building_cells: &'a BTreeMap<usize, BuildingCell>,
    pub start: &'a crate::match_runtime::StartPlan,
    pub options: &'a crate::match_runtime::MatchOptions,
    pub environment: &'a crate::environment::EnvironmentRuntime,
}
fn validate_context<'a>(
    context: RecipientContext<'_>,
    states: impl IntoIterator<Item = &'a RecipientRouting>,
    fog: &crate::fog::Fog,
) -> Result<(), String> {
    crate::observed_memory::validate_world(context.world)
        .map_err(|e| format!("routing terrain: {e:?}"))?;
    let g = &context.world.game;
    fog.validate_shape(g)?;
    if states.into_iter().any(|state| {
        state
            .bound_units
            .iter()
            .any(|u| u.kind >= context.rules.kinds.len())
    }) || fog.teams.values().any(|f| {
        f.vis.len() != g.w * g.h || f.next.len() != g.w * g.h || f.explored.len() != g.w * g.h
    }) || fog.terrain.as_ref().is_some_and(|t| {
        t.sight.len() != g.w * g.h
            || t.sat_w != g.w + 1
            || t.sat.len() != (g.w + 1) * (g.h + 1)
            || t.height.as_ref().is_some_and(|h| h.len() != g.w * g.h)
    }) {
        return Err("invalid owned routing cache shape".into());
    }
    let mut wreck_ids = std::collections::BTreeSet::new();
    if g.wrecks.iter().any(|w| {
        w.id.is_none_or(|id| id == 0 || !wreck_ids.insert(id))
            || !w.x.is_finite()
            || !w.z.is_finite()
            || w.cell.is_some_and(|c| c >= g.w * g.h)
    }) {
        return Err("routing needs valid unique wreck identities and positions".into());
    }
    if !matches!(context.options.mode, Mode::Default | Mode::Conquest) || g.mode.is_some() {
        return Err("owned routing currently supports ordinary Conquest".into());
    }
    if g.players.len() != context.start.players.len()
        || g.players.len() != context.options.players.len()
        || g.players.iter().enumerate().any(|(i, p)| {
            p.slot != i || !(0..6).contains(&p.team) || p.visible.windows(2).any(|v| v[0] >= v[1])
        })
        || g.units.iter().any(|(id, u)| {
            *id != u.id
                || u.kind >= context.rules.kinds.len()
                || u.owner >= g.players.len()
                || !u.x.is_finite()
                || !u.z.is_finite()
                || !u.hp.is_finite()
                || u.garrison.is_some_and(|c| c >= g.w * g.h)
                || u.cells
                    .as_ref()
                    .is_some_and(|cells| cells.iter().any(|&c| c as usize >= g.w * g.h))
                || u.built.is_some_and(|v| !v.is_finite())
        })
        || g.weather.is_some_and(|v| v >= context.rules.weather.len())
        || !g.rain.is_finite()
        || g.wet.is_some_and(|v| !v.is_finite())
        || g.smokes
            .iter()
            .any(|v| !v.x.is_finite() || !v.z.is_finite() || !v.r.is_finite() || v.r < 0.0)
        || g.strikes.iter().any(|v| {
            v.owner >= g.players.len() || !v.x.is_finite() || !v.z.is_finite() || !v.dir.is_finite()
        })
        || context
            .world
            .mines
            .iter()
            .any(|(c, o)| *c >= g.w * g.h || *o < -1 || *o >= g.players.len() as i32)
        || context
            .world
            .mine_seen
            .keys()
            .chain(context.world.fires.iter().map(|(cell, _)| cell))
            .any(|&c| c >= g.w * g.h)
        || context.building_cells.iter().any(|(&c, b)| {
            c >= g.w * g.h
                || b.owner >= g.players.len()
                || !b.at.x.is_finite()
                || !b.at.z.is_finite()
        })
    {
        return Err("invalid recipient routing context".into());
    }
    Ok(())
}
fn refresh_memory_parts(
    world: &World,
    rules: &Rules,
    buildings: &BTreeMap<usize, BuildingCell>,
    state: &mut RecipientRouting,
    owner: usize,
    observer: Option<crate::staged_observer::StagedObserver<'_>>,
    capture_frame: bool,
) -> Result<crate::world::ObservationReceipt, String> {
    let mut facts = if state.memory.needs_replay(world) {
        let grid = SpatialGrid::new(&world.game.units, 16.0);
        facts(world, rules, &grid, buildings, owner, observer)?
    } else {
        Facts::default()
    };
    state
        .memory
        .refresh_impl(
            world,
            ObservationFrame {
                team: world.game.players[owner].team,
                skip_fog: false,
            },
            &mut facts,
            capture_frame,
        )
        .map(|(_, receipt)| receipt)
        .map_err(|e| format!("observed refresh: {e:?}"))
}
#[derive(Clone, Copy)]
pub struct RecipientObservation<'a> {
    pub world: &'a World,
    pub rules: &'a Rules,
    pub building_cells: &'a BTreeMap<usize, BuildingCell>,
    pub environment: &'a crate::environment::EnvironmentRuntime,
}
/// Existing immediate field-level consumer boundary. No gameplay grid mutation.
pub(crate) fn refresh_recipient_parts(
    context: RecipientObservation<'_>,
    fog: &mut crate::fog::Fog,
    state: &mut RecipientRouting,
    owner: usize,
) -> Result<crate::world::ObservationReceipt, String> {
    refresh_parts(
        RecipientObservation {
            world: context.world,
            rules: context.rules,
            building_cells: context.building_cells,
            environment: context.environment,
        },
        state,
        fog,
        owner,
        None,
        false,
    )
}
fn refresh_parts(
    inputs: RecipientObservation<'_>,
    state: &mut RecipientRouting,
    fog: &mut crate::fog::Fog,
    owner: usize,
    observer: Option<crate::staged_observer::StagedObserver<'_>>,
    capture_frame: bool,
) -> Result<crate::world::ObservationReceipt, String> {
    let RecipientObservation {
        world,
        rules,
        building_cells: buildings,
        environment,
    } = inputs;
    let g = &world.game;
    let key = (g.tick, g.terrain_version, g.vision_tick.unwrap_or(-1));
    if state.key == Some(key) {
        state.refresh_bound_units(g, rules);
        state.wetness = g.wet.unwrap_or(0.0);
        return Ok(world.defer_observation(None, capture_frame));
    }
    let grid = SpatialGrid::new(&g.units, 16.0);
    let accepted = collect_wrecks(g, rules, &grid, owner, observer, environment)?;
    let receipt = refresh_memory_parts(
        world,
        rules,
        buildings,
        state,
        owner,
        observer,
        capture_frame,
    )?;
    state
        .memory
        .apply_routes(&mut state.routes)
        .map_err(|e| format!("observed projection: {e:?}"))?;
    let rows = staged_wreck_memory(
        &state.wreck_memory,
        g,
        rules,
        &grid,
        owner,
        observer,
        accepted,
    );
    publish_wreck_memory(state, rows);
    state.wrecks = state.wreck_queries.clone();
    state.fires.fill(false);
    // Source teamFog is inside the fire filter callback. An empty fire map
    // leaves a cold fog cache available for the later birth sight source.
    if !world.fires.is_empty() {
        let vis = &fog
            .team_with_observer(g, rules, g.players[owner].team, observer)
            .vis;
        for (&c, _) in world.fires.iter() {
            state.fires[c] = vis[c] != 0;
        }
    }
    state.bound_units = g
        .units
        .values()
        .chain(observer.map(|o| o.unit()))
        .filter(|u| {
            g.reveal
                || g.allied(u.owner, owner)
                || (g.players[owner].sees(u.id) && (u.air.is_none() || airborne(u)))
        })
        .map(BoundUnit::from_unit)
        .collect();
    state.refresh_bound_units(g, rules);
    state.wetness = g.wet.unwrap_or(0.0);
    state.roads = world.roads;
    state.key = Some(key);
    Ok(receipt)
}
/// Staging owns component copies only. Query buffers survive a successful commit.
/// The consumer must discard path scratch derived from this preview if it aborts.
#[must_use = "publish after every fallible check, or drop without changing the owner"]
pub struct PreparedRecipientRouting<'a> {
    target: &'a mut RecipientRouting,
    fog_target: &'a mut crate::fog::Fog,
    state: RecipientRouting,
    fog: crate::fog::Fog,
    receipt: crate::world::ObservationReceipt,
    wreck_frame: crate::recipient_lists::WreckSourceFrame,
}
impl PreparedRecipientRouting<'_> {
    /// Preserve canonical search history while rebinding this recipient's
    /// external arrays to the preview. Publish the fork only with this receipt.
    pub fn fork_paths(
        &self,
        paths: &crate::pathfinding::Pathfinding,
    ) -> crate::pathfinding::Pathfinding {
        paths.fork_for_terrain(self.target.query().navigation(), self.query().navigation())
    }

    pub fn query(&self) -> RouteQuery<'_> {
        self.state.query()
    }
    pub fn live(&self) -> LiveRouting<'_> {
        self.state.live()
    }
    pub fn knowledge(&self) -> &RouteKnowledge {
        self.state.knowledge()
    }
    pub fn wrecks(&self) -> &[Wreck] {
        self.state.wrecks()
    }
}
pub struct RecipientRoutingCommit<'a, 'w, 'e> {
    _environment: &'e crate::environment::EnvironmentRuntime,
    target: &'a mut RecipientRouting,
    fog_target: &'a mut crate::fog::Fog,
    state: RecipientRouting,
    fog: crate::fog::Fog,
    ledger: crate::world::PreparedObservation<'w>,
}
impl RecipientRoutingCommit<'_, '_, '_> {
    pub fn commit(self) {
        *self.target = self.state;
        *self.fog_target = self.fog;
        self.ledger.commit();
    }
}
impl<'a> PreparedRecipientRouting<'a> {
    pub fn prepare_publication<'w, 'e>(
        self,
        world: &'w mut World,
        environment: &'e crate::environment::EnvironmentRuntime,
    ) -> Result<(&'w mut Game, RecipientRoutingCommit<'a, 'w, 'e>), String> {
        let Self {
            target,
            fog_target,
            state,
            fog,
            receipt,
            wreck_frame,
        } = self;
        wreck_frame.validate(environment)?;
        let (game, ledger) = receipt.prepare(world)?;
        Ok((
            game,
            RecipientRoutingCommit {
                _environment: environment,
                target,
                fog_target,
                state,
                fog,
                ledger,
            },
        ))
    }
}
pub fn prepare_recipient_routing<'a>(
    context: RecipientContext<'_>,
    target: &'a mut RecipientRouting,
    fog_target: &'a mut crate::fog::Fog,
    owner: usize,
    observer: Option<crate::staged_observer::StagedObserver<'_>>,
) -> Result<PreparedRecipientRouting<'a>, String> {
    validate_context(
        RecipientContext { ..context },
        std::iter::once(&*target),
        fog_target,
    )?;
    if target.memory.slot() != owner
        || target.memory.identity() != owner as u64
        || owner >= context.world.game.players.len()
    {
        return Err("routing owner mismatch".into());
    }
    target
        .memory
        .validate_binding(context.world)
        .map_err(|e| format!("routing binding: {e:?}"))?;
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
    let mut fog = fog_target.clone();
    let receipt = refresh_parts(
        RecipientObservation {
            world: context.world,
            rules: context.rules,
            building_cells: context.building_cells,
            environment: context.environment,
        },
        &mut state,
        &mut fog,
        owner,
        observer,
        true,
    )?;
    Ok(PreparedRecipientRouting {
        target,
        fog_target,
        state,
        fog,
        receipt,
        wreck_frame: crate::recipient_lists::WreckSourceFrame::capture(
            &context.world.game,
            context.environment,
        ),
    })
}
