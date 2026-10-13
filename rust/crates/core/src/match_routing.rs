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
    vision::{airborne, los, vision_of},
    world::{Recipient, World},
};
use std::collections::BTreeMap;

/// One owner's remembered rows and routing projection. Refresh through Match;
/// consumers borrow this state separately from mutable units or path scratch.
#[derive(Clone, Debug)]
pub struct RecipientRouting {
    memory: ObservedMemory,
    routes: RouteKnowledge,
    key: Option<(i64, u32, i64)>,
    fires: Vec<bool>,
    bunkers: Vec<Point>,
    bound_units: Vec<BoundUnit>,
    wetness: f64,
    roads: bool,
    wreck_memory: Vec<Wreck>,
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
        &self.wreck_memory
    }
    pub fn remembered(&self) -> &[ObservedRow] {
        self.memory.full_rows()
    }
}
#[derive(Clone, Copy, Debug)]
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
fn sees(game: &Game, rules: &Rules, grid: &SpatialGrid, team: i32, reach: f64, at: Point) -> bool {
    // The reference's eyesFor builds a fresh team bucket index. Match rebuilds
    // its index first; the exact-distance check removes extra square candidates.
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
fn facts(
    world: &World,
    rules: &Rules,
    grid: &SpatialGrid,
    buildings: &BTreeMap<usize, BuildingCell>,
    slot: usize,
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
                    ruin_visible: sees(game, rules, grid, team, reach, at),
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
                ),
            );
        }
    }
    Ok(result)
}
fn refresh_wrecks(
    state: &mut RecipientRouting,
    game: &Game,
    rules: &Rules,
    grid: &SpatialGrid,
    owner: usize,
) {
    let team = game.players[owner].team;
    let reach = game
        .units
        .values()
        .filter(|u| game.team_of(u) == team)
        .map(|u| vision_of(game, rules, u))
        .fold(1.0, math::max);
    let visible = |at| game.reveal || sees(game, rules, grid, team, reach, at);
    for current in &game.wrecks {
        if !visible(Point {
            x: current.x,
            z: current.z,
        }) {
            continue;
        }
        let row = Wreck {
            id: current.id,
            cell: None,
            x: math::round(current.x * 10.0) / 10.0,
            z: math::round(current.z * 10.0) / 10.0,
            moving: current.moving,
        };
        if let Some(old) = state.wreck_memory.iter_mut().find(|w| w.id == current.id) {
            *old = row;
        } else {
            state.wreck_memory.push(row);
        }
    }
    state.wreck_memory.retain(|old| {
        game.wrecks.iter().any(|w| w.id == old.id) || !visible(Point { x: old.x, z: old.z })
    });
    for row in &mut state.wreck_memory {
        if row.moving && !visible(Point { x: row.x, z: row.z }) {
            row.moving = false;
        }
    }
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
                    wrecks: Vec::new(),
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(())
    }
    pub(crate) fn validate_routing_context(&self) -> Result<(), String> {
        crate::observed_memory::validate_world(&self.world)
            .map_err(|e| format!("routing terrain: {e:?}"))?;
        let g = &self.world.game;
        if self.recipients.iter().any(|state| {
            state
                .bound_units
                .iter()
                .any(|u| u.kind >= self.rules.kinds.len())
        }) || self.routing_fog.teams.values().any(|f| {
            f.vis.len() != g.w * g.h || f.next.len() != g.w * g.h || f.explored.len() != g.w * g.h
        }) || self.routing_fog.terrain.as_ref().is_some_and(|t| {
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
        if !matches!(self.options.mode, Mode::Default | Mode::Conquest) || g.mode.is_some() {
            return Err("owned routing currently supports ordinary Conquest".into());
        }
        if g.players.len() != self.start.players.len()
            || g.players.len() != self.options.players.len()
            || g.players.iter().enumerate().any(|(i, p)| {
                p.slot != i
                    || !(0..6).contains(&p.team)
                    || p.visible.windows(2).any(|v| v[0] >= v[1])
            })
            || g.units.iter().any(|(id, u)| {
                *id != u.id
                    || u.kind >= self.rules.kinds.len()
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
            || g.weather.is_some_and(|v| v >= self.rules.weather.len())
            || !g.rain.is_finite()
            || g.wet.is_some_and(|v| !v.is_finite())
            || g.smokes
                .iter()
                .any(|v| !v.x.is_finite() || !v.z.is_finite() || !v.r.is_finite() || v.r < 0.0)
            || g.strikes.iter().any(|v| {
                v.owner >= g.players.len()
                    || !v.x.is_finite()
                    || !v.z.is_finite()
                    || !v.dir.is_finite()
            })
            || self
                .world
                .mines
                .iter()
                .any(|(c, o)| *c >= g.w * g.h || *o < -1 || *o >= g.players.len() as i32)
            || self
                .world
                .mine_seen
                .keys()
                .chain(self.world.fires.iter().map(|(cell, _)| cell))
                .any(|&c| c >= g.w * g.h)
            || self.building_cells.iter().any(|(&c, b)| {
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
        refresh_recipient_memory_parts(
            &mut self.world,
            &self.rules,
            &mut self.grid,
            &self.building_cells,
            &mut self.recipients[owner],
            owner,
        )
    }
    /// Refresh before borrowing recipient query/live and mutable unit or search fields.
    pub fn refresh_recipient_routing(&mut self, owner: usize) -> Result<(), String> {
        self.validate_recipient(owner)?;
        refresh_recipient_parts(
            &mut self.world,
            &self.rules,
            &mut self.grid,
            &self.building_cells,
            &mut self.routing_fog,
            &mut self.recipients[owner],
            owner,
        )
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
        let grid = SpatialGrid::new(&self.world.game.units, 16.0);
        let g = &self.world.game;
        let team = g.players[owner].team;
        let reach = g
            .units
            .values()
            .filter(|u| g.team_of(u) == team)
            .map(|u| vision_of(g, &self.rules, u))
            .fold(1.0, math::max);
        Ok(sees(g, &self.rules, &grid, team, reach, at))
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
        let grid = SpatialGrid::new(&self.world.game.units, 16.0);
        refresh_wrecks(
            &mut self.recipients[owner],
            &self.world.game,
            &self.rules,
            &grid,
            owner,
        );
        Ok(self.recipients[owner].wreck_memory.clone())
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

fn refresh_recipient_memory_parts(
    world: &mut World,
    rules: &Rules,
    _grid: &mut SpatialGrid,
    buildings: &BTreeMap<usize, BuildingCell>,
    state: &mut RecipientRouting,
    owner: usize,
) -> Result<(), String> {
    if !state.memory.needs_replay(world) {
        let team = world.game.players[owner].team;
        state
            .memory
            .refresh(
                world,
                ObservationFrame {
                    team,
                    skip_fog: false,
                },
                &mut Facts::default(),
            )
            .map_err(|e| format!("observed refresh: {e:?}"))?;
        return Ok(());
    }
    let observation_grid = SpatialGrid::new(&world.game.units, 16.0);
    let mut facts = facts(world, rules, &observation_grid, buildings, owner)?;
    let team = world.game.players[owner].team;
    state
        .memory
        .refresh(
            world,
            ObservationFrame {
                team,
                skip_fog: false,
            },
            &mut facts,
        )
        .map_err(|e| format!("observed refresh: {e:?}"))?;
    Ok(())
}
/// Validated caller-owned parts, used lazily by convoy routing callbacks.
pub(crate) fn refresh_recipient_parts(
    world: &mut World,
    rules: &Rules,
    grid: &mut SpatialGrid,
    buildings: &BTreeMap<usize, BuildingCell>,
    fog: &mut crate::fog::Fog,
    state: &mut RecipientRouting,
    owner: usize,
) -> Result<(), String> {
    let g = &world.game;
    let key = (g.tick, g.terrain_version, g.vision_tick.unwrap_or(-1));
    if state.key == Some(key) {
        state.refresh_bound_units(&world.game, rules);
        state.wetness = world.game.wet.unwrap_or(0.0);
        return Ok(());
    }
    refresh_recipient_memory_parts(world, rules, grid, buildings, state, owner)?;
    let observation_grid = SpatialGrid::new(&world.game.units, 16.0);

    state
        .memory
        .apply_routes(&mut state.routes)
        .map_err(|e| format!("observed projection: {e:?}"))?;
    let g = &world.game;
    refresh_wrecks(state, g, rules, &observation_grid, owner);
    state.wrecks = state.wreck_memory.clone();
    state.fires.fill(false);
    if !world.fires.is_empty() {
        let fog = fog.team(g, rules, g.players[owner].team);
        for &c in world.fires.iter().map(|(cell, _)| cell) {
            state.fires[c] = fog.vis[c] != 0;
        }
    }
    state.bound_units = g
        .units
        .values()
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
    Ok(())
}
