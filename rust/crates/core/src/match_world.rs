//! Authoritative World startup, with regions separate from immutable geography.
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    catalog::{Catalog, Value},
    economy::{Faction, Mode, TechState},
    game::{CELL, Game, LOCAL_DEFENDER_OWNER, MOVE, ModeKind, Player, Point},
    grid::SpatialGrid,
    match_runtime::{
        BuildingCell, Match, MatchOptions, StartPlan, foot_center, footprint, terrain_view,
    },
    math,
    random::MatchRandom,
    unit_publication::{PublicationParts, SpawnActor, StagedUnit, prepare_world_publication},
    unit_spawn::SpawnPlacer,
    vision,
    world::World,
};

#[derive(Clone, Debug, PartialEq)]
pub struct RegionState {
    pub team: i32,
    pub progress: f64,
    pub capper: Option<usize>,
    pub contested: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceNode {
    pub cell: usize,
    pub at: Point,
    pub depot: Option<u32>,
    pub rate: f64,
    pub fuel: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct LocalDefender {
    pub faction: Faction,
    pub spawn: Point,
    pub visible: Vec<u32>,
}
/// Intentional remembered building rows. Live units and health remain in Game.
#[derive(Clone, Debug, PartialEq)]
pub struct KnownBuilding {
    pub id: u32,
    pub kind: usize,
    pub owner: usize,
    pub at: Point,
    pub built: Option<f64>,
}
pub struct WorldRuntime {
    pub memory: crate::world_memory::WorldMemory,
    pub regions: Vec<RegionState>,
    pub nodes: Vec<ResourceNode>,
    pub local_defender: LocalDefender,
    pub rails: crate::match_world_rails::WorldRails,
    pub known_buildings: Vec<(i32, Vec<KnownBuilding>)>,
    /// Source worldbase.region metadata, resolved to immutable region indices.
    pub guard_bases: BTreeMap<u32, usize>,
}
fn numeric(v: &Value, name: &str) -> Result<f64, String> {
    v.get(name)
        .and_then(Value::number)
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("missing World startup number {name}"))
}

/// Borrowed actor facts. Commander slots and the World local actor stay distinct.
pub struct WorldActorView<'a> {
    pub owner: usize,
    pub team: i32,
    pub faction: Faction,
    pub spawn: Point,
    pub visible: &'a [u32],
}
impl Match {
    pub fn world_actor_view(&self, owner: usize) -> Result<WorldActorView<'_>, String> {
        let runtime = self.world_runtime.as_ref().ok_or("missing World runtime")?;
        if self.world.game.mode != Some(ModeKind::World) {
            return Err("World actor lookup requires World mode".into());
        }
        if owner == LOCAL_DEFENDER_OWNER {
            let actor = &runtime.local_defender;
            Ok(WorldActorView {
                owner,
                team: -1,
                faction: actor.faction,
                spawn: actor.spawn,
                visible: &actor.visible,
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
            Ok(WorldActorView {
                owner,
                team: player.team,
                faction: start.config.faction,
                spawn: start.spawn,
                visible: &player.visible,
            })
        }
    }
    fn checked_world_regions(
        &self,
    ) -> Result<(&crate::world_map::WorldDefinition, &WorldRuntime), String> {
        if self.world.game.mode != Some(ModeKind::World) {
            return Err("World region lookup requires World mode".into());
        }
        let geography = self.world.geography().ok_or("missing World geography")?;
        let runtime = self.world_runtime.as_ref().ok_or("missing World runtime")?;
        let game = &self.world.game;
        if game.w == 0
            || game.h == 0
            || game.w.checked_mul(game.h) != Some(game.chars.len())
            || self.start.dimensions != (game.w, game.h)
            || runtime.regions.len() != geography.regions.len()
            || geography
                .region_map
                .as_ref()
                .is_some_and(|m| m.len() != game.w * game.h)
        {
            return Err("mismatched World region shape".into());
        }
        Ok((geography, runtime))
    }
    /// Source membership values are ordered vector indices, distinct from region IDs.
    pub fn world_region_index(&self, at: Point) -> Result<Option<usize>, String> {
        if !at.x.is_finite() || !at.z.is_finite() {
            return Err("nonfinite World region point".into());
        }
        let (geography, runtime) = self.checked_world_regions()?;
        let game = &self.world.game;
        if let Some(membership) = &geography.region_map {
            let Some(cell) = game.cell_of(at.x, at.z) else {
                return Ok(None);
            };
            let index = membership[cell];
            return Ok((index >= 0)
                .then_some(index as usize)
                .filter(|&i| i < runtime.regions.len()));
        }
        let span = geography.region_cells as f64 * CELL;
        let (x, y) = ((at.x / span).floor(), (at.z / span).floor());
        let columns = game.w as f64 / geography.region_cells as f64;
        if x < 0.0 || y < 0.0 || x >= columns || y >= game.h as f64 / geography.region_cells as f64
        {
            return Ok(None);
        }
        let index = y * columns + x;
        // A fractional source array key has no region. Province bounds also retain
        // the source's partial last-column behavior on nonmultiple dimensions.
        Ok(
            (index.fract() == 0.0 && index < runtime.regions.len() as f64)
                .then_some(index as usize),
        )
    }
    pub fn world_region_at(&self, at: Point) -> Result<Option<&RegionState>, String> {
        let index = self.world_region_index(at)?;
        Ok(index.map(|index| {
            &self
                .world_runtime
                .as_ref()
                .expect("checked World runtime")
                .regions[index]
        }))
    }
    /// Economy consumes actual runtime ownership and exact source kind labels.
    pub fn world_economy_regions(&self) -> Result<Vec<crate::economy_state::Region>, String> {
        use crate::economy_state::{Region, RegionKind};
        let (geography, runtime) = self.checked_world_regions()?;
        Ok(geography
            .regions
            .iter()
            .zip(&runtime.regions)
            .map(|(definition, state)| Region {
                team: state.team,
                kind: match definition.kind.as_deref() {
                    Some("city") => RegionKind::City,
                    Some("industrial") => RegionKind::Industrial,
                    Some("resource") => RegionKind::Resource,
                    _ => RegionKind::Other,
                },
            })
            .collect())
    }
    /// Construct the bounded native startup state. World tick phases, recipient
    /// terrain collection and logistics-enabled matches remain separate adapters.
    /// `random` retains the generator's used streams rather than restarting them.
    pub fn new_world(
        map: &crate::map::AuthoritativeMap,
        options: MatchOptions,
        random: MatchRandom,
    ) -> Result<Self, String> {
        if options.logistics {
            return Err("World startup does not yet initialize logistics".into());
        }
        if options.mode != Mode::World || map.geography().is_none() {
            return Err("World startup requires explicit World geography and mode".into());
        }
        if random.snapshot().seed != options.seed {
            return Err("World startup random seed does not match options".into());
        }
        if !map.points().is_empty() {
            return Err("World startup does not yet adapt legacy objective lists".into());
        }
        if !map.triggers().is_empty() {
            return Err("World startup does not yet apply legacy scenarios".into());
        }
        let rules = crate::rules::Rules::compiled().map_err(|e| e.to_string())?;
        let start = StartPlan::prepare_world(map, &options, &rules, random)?;
        let mut world = World::from_map(map).map_err(|e| e.to_string())?;
        world.game.mode = Some(ModeKind::World);
        world.game.players = start
            .players
            .iter()
            .enumerate()
            .map(|(slot, p)| Player {
                slot,
                team: p.config.team,
                visible: Vec::new(),
            })
            .collect();
        world.game.weather = rules
            .weather
            .iter()
            .position(|w| w.name == start.weather.now.name());
        world.game.rain = start.initial_rain;
        world.game.wet = start.initial_wet;
        // Static structures precede the source World journal's zero position.
        world.reset_journal();
        let geography = map.geography().expect("checked geography");
        let regions = geography
            .regions
            .iter()
            .map(|r| {
                let team = match r.home {
                    Some(home) => {
                        options
                            .players
                            .get(home)
                            .ok_or("World home has no commander")?
                            .team
                    }
                    None => -1,
                };
                Ok(RegionState {
                    team,
                    progress: if r.home.is_some() { 1.0 } else { 0.0 },
                    capper: None,
                    contested: false,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let rails = crate::match_world_rails::WorldRails::from_metadata(
            geography.metadata.get("rails"),
            &world.game.chars,
            world.game.w,
            world.game.h,
            geography.regions.len(),
        )?;
        let runtime = WorldRuntime {
            memory: crate::world_memory::WorldMemory::default(),
            regions,
            nodes: Vec::new(),
            rails,
            local_defender: LocalDefender {
                faction: Faction::Usa,
                spawn: Point { x: 0.0, z: 0.0 },
                visible: Vec::new(),
            },
            known_buildings: Vec::new(),
            guard_bases: BTreeMap::new(),
        };
        let count = start.players.len();
        let initial_tech =
            TechState::new(1, 0, 0, 0).map_err(|e| format!("World research: {e:?}"))?;
        let mut result = Self {
            field_works: crate::field_works::FieldWorksRuntime::default(),
            convoys: None,
            supply_net: crate::supply_network::GroundSupplyNetwork::default(),
            supply_routes: crate::supply_routes::SupplyRouting::default(),
            support: crate::match_support::SupportState::default(),
            companions: crate::companions::Companions::default(),
            abilities: crate::match_abilities::AbilityState::default(),
            recovery: crate::match_recovery::RecoveryState::default(),
            combat: crate::match_combat::CombatState::default(),
            environment: crate::environment::EnvironmentRuntime::default(),
            firing: crate::match_firing::FiringState::default(),
            recipients: Vec::new(),
            routing_fog: crate::fog::Fog::default(),
            tick_state: crate::match_tick::TickState::default(),
            fallen: crate::match_story::FallenLocations::default(),
            world_runtime: Some(runtime),
            commands: crate::match_commands::MatchCommands::new(4096),
            grid: SpatialGrid::new(&world.game.units, 16.0),
            world,
            start,
            options,
            building_cells: BTreeMap::new(),
            production: crate::production::ProductionState::new(vec![initial_tech; count])
                .map_err(|e| e.to_string())?,
            building_rallies: BTreeMap::new(),
            player_rallies: vec![None; count],
            recruit_metadata: BTreeMap::new(),
            produced_placer: crate::produced_units::ProducedPlacer::new(),
            scenario: None,
            logistics: crate::logistics::LogisticsState::default(),
            capture_ticks: Vec::new(),
            story: vec![crate::match_story::PlayerStory::default(); count],
            finish: None,
            timeline: Vec::new(),
            next_unit_id: 1,
            rules,
        };
        let row_start = result.world.new_cells().len();
        result.initialize_world()?;
        let rails = &result.world_runtime.as_ref().expect("World runtime").rails;
        let rail_at: BTreeMap<_, _> = rails.rail_at.iter().copied().collect();
        let station_at: BTreeMap<_, _> = rails.station_at.iter().copied().collect();
        let fields: Vec<_> = result.world.new_cells()[row_start..]
            .iter()
            .map(|row| {
                (
                    row.cell,
                    rail_at.get(&row.cell).copied(),
                    station_at.get(&row.cell).copied(),
                )
            })
            .collect();
        result
            .world
            .annotate_recent_rows(row_start, &fields)
            .map_err(|e| e.to_string())?;
        Ok(result)
    }

    fn world_actor(&self, owner: usize) -> Result<SpawnActor, String> {
        if owner == LOCAL_DEFENDER_OWNER {
            Ok(SpawnActor::WorldLocalDefender {
                spawn: self
                    .world_runtime
                    .as_ref()
                    .ok_or("missing World runtime")?
                    .local_defender
                    .spawn,
            })
        } else {
            Ok(SpawnActor::Commander {
                owner,
                spawn: self
                    .start
                    .players
                    .get(owner)
                    .ok_or("invalid World commander")?
                    .spawn,
            })
        }
    }
    fn publish_world_unit(&mut self, draft: StagedUnit) -> Result<u32, String> {
        let id = draft.unit.id;
        if self.combat.get(id).is_some()
            || self.firing.get(id).is_some()
            || self.abilities.get(id).is_some()
            || self.recovery.get(id).is_some()
        {
            return Err("World birth metadata identity occupied".into());
        }
        let mode = self.world.game.mode;
        prepare_world_publication(
            PublicationParts {
                units: &mut self.world.game.units,
                grid: &mut self.grid,
                controls: &mut self.commands.units,
                recruitment: &mut self.recruit_metadata,
                next_id: &mut self.next_unit_id,
            },
            &self.rules,
            self.start.players.len(),
            mode,
            vec![draft],
        )
        .map_err(|e| format!("World publication: {e:?}"))?
        .publish_with(|unit, _| {
            self.combat.initialize(unit.id);
            self.firing.initialize(unit.id);
            self.abilities.initialize(unit.id);
            self.recovery.initialize(unit.id);
        });
        Ok(id)
    }
    fn stage_world_unit(
        &self,
        owner: usize,
        name: &str,
        index: u32,
        pose: Option<Point>,
    ) -> Result<StagedUnit, String> {
        let kind = self.rules.kind(name).ok_or("unknown World unit kind")?;
        let draft = StagedUnit::ordinary_with_actor(
            &self.world.game,
            &self.rules,
            self.world_actor(owner)?,
            kind,
            self.next_unit_id,
            index,
            pose,
        )
        .map_err(|e| format!("World spawn: {e:?}"))?;
        Ok(draft)
    }
    fn world_building(&mut self, owner: usize, name: &str, cell: usize) -> Result<u32, String> {
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let definition = catalog.unit(name).map_err(|e| e.to_string())?;
        let size = numeric(definition, "size")? as usize;
        let hp = numeric(definition, "hpPer")?;
        let game = &self.world.game;
        let cells = footprint(game, cell, size).ok_or("World building outside map")?;
        let top = cells
            .iter()
            .map(|&c| game.height.as_ref().map_or(0, |h| h[c]))
            .max()
            .ok_or("empty World footprint")?;
        for &c in &cells {
            if self.world.game.height.as_ref().map_or(0, |h| h[c]) < top {
                self.world.set_level(c, top).map_err(|e| e.to_string())?;
            }
            self.world.set_cell(c, b'K').map_err(|e| e.to_string())?;
        }
        let at = foot_center(&self.world.game, cell, size);
        let mut draft =
            self.stage_world_unit(owner, name, self.world.game.units.len() as u32, Some(at))?;
        draft.unit.cells = Some(cells.iter().map(|&c| c as u32).collect());
        draft.unit.built = Some(1.0);
        draft.unit.hp = hp;
        let id = self.publish_world_unit(draft)?;
        self.production
            .initialize_building(id)
            .map_err(|e| e.to_string())?;
        self.building_rallies.insert(id, None);
        for &c in &cells {
            self.building_cells.insert(
                c,
                BuildingCell {
                    unit: id,
                    owner,
                    at,
                },
            );
        }
        self.world
            .register_building_structure(id, &cells, hp, hp)
            .map_err(|e| e.to_string())?;
        Ok(id)
    }
    fn initialize_world(&mut self) -> Result<(), String> {
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let classic = catalog
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("classic")
            .ok_or("missing classic rules")?;
        // Geography is immutable. Only the small source startup projection is copied.
        let region_inputs: Vec<_> = self
            .world
            .geography()
            .ok_or("missing geography")?
            .regions
            .iter()
            .map(|r| {
                (
                    Point {
                        x: (r.x as f64 + 0.5) * CELL,
                        z: (r.y as f64 + 0.5) * CELL,
                    },
                    r.kind.clone(),
                )
            })
            .collect();
        for owner in 0..self.start.players.len() {
            let at = self.start.players[owner].spawn;
            let cell = self
                .world
                .game
                .cell_of(at.x, at.z)
                .ok_or("invalid World spawn")?
                .checked_sub(self.world.game.w + 1)
                .ok_or("World HQ footprint outside map")?;
            self.world_building(owner, "hq", cell)?;
        }
        let mut placer = SpawnPlacer::new();
        for (index, (at, kind)) in region_inputs.iter().enumerate() {
            let cell = self
                .world
                .game
                .cell_of(at.x + 8.0, at.z + 8.0)
                .ok_or("World node outside map")?;
            let fuel = kind.as_deref() == Some("industrial");
            let node = ResourceNode {
                cell,
                at: foot_center(&self.world.game, cell, 2),
                depot: None,
                rate: numeric(classic, if fuel { "contestedFuel" } else { "homeRate" })?,
                fuel,
            };
            self.world_runtime
                .as_mut()
                .expect("World runtime")
                .nodes
                .push(node);
            if self.world_runtime.as_ref().expect("World runtime").regions[index].team >= 0 {
                continue;
            }
            self.world_runtime
                .as_mut()
                .expect("World runtime")
                .local_defender
                .spawn = Point {
                x: at.x - 7.0,
                z: at.z - 7.0,
            };
            let base = self
                .world
                .game
                .cell_of(at.x - 8.0, at.z - 8.0)
                .ok_or("World guard base outside map")?;
            let cells = footprint(&self.world.game, base, 3)
                .ok_or("World guard base footprint outside map")?;
            for c in cells {
                self.world.set_cell(c, b'.').map_err(|e| e.to_string())?;
                self.world.set_level(c, 0).map_err(|e| e.to_string())?;
            }
            let id = self.world_building(LOCAL_DEFENDER_OWNER, "worldbase", base)?;
            self.world_runtime
                .as_mut()
                .expect("World runtime")
                .guard_bases
                .insert(id, index);
            let guards: &[&str] = match kind.as_deref() {
                Some("city") => &["rifle", "rifle", "mg", "at", "mortar"],
                Some("industrial") => &["rifle", "at", "armoredcar"],
                Some("resource") => &["rifle", "mg", "mortar"],
                _ => &["rifle", "rifle", "mg"],
            };
            for (n, name) in guards.iter().enumerate() {
                let angle = n as f64 / guards.len() as f64 * std::f64::consts::TAU;
                let mut draft =
                    self.stage_world_unit(LOCAL_DEFENDER_OWNER, name, n as u32, None)?;
                // The source's final guard clearance is MOVE-only, even for armour.
                let c = placer
                    .nearest_free(
                        terrain_view(&self.world.game),
                        Point {
                            x: at.x + math::cos(angle) * 6.0,
                            z: at.z + 3.0 + math::sin(angle) * 6.0,
                        },
                        MOVE,
                        None,
                    )
                    .map_err(|e| format!("guard placement: {e:?}"))?;
                let pose = foot_center(&self.world.game, c, 1);
                draft.unit.x = pose.x;
                draft.unit.z = pose.z;
                draft.unit.hold_pos = true;
                draft.control.auto_cast = false;
                draft.control.auto_retreat = false;
                draft.control.guard_home = Some(pose);
                self.publish_world_unit(draft)?;
            }
        }
        self.fortify_world_guards(&region_inputs)?;
        for owner in 0..self.start.players.len() {
            for (index, kind) in self.start.starting_kinds.clone().into_iter().enumerate() {
                let name = self.rules.kinds[kind].name.clone();
                let draft = self.stage_world_unit(owner, &name, index as u32, None)?;
                self.publish_world_unit(draft)?;
            }
        }
        self.grid.rebuild(&self.world.game.units);
        vision::update_vision(&mut self.world.game, &self.rules, &self.grid);
        self.initialize_world_visibility()?;
        Ok(())
    }

    fn fortify_world_guards(&mut self, inputs: &[(Point, Option<String>)]) -> Result<(), String> {
        let nearest = |at: Point| {
            let mut home = self.start.players[0].spawn;
            let mut distance = math::hypot(home.x - at.x, home.z - at.z);
            for p in self.start.players.iter().skip(1) {
                let d = math::hypot(p.spawn.x - at.x, p.spawn.z - at.z);
                if d < distance {
                    home = p.spawn;
                    distance = d;
                }
            }
            (home, distance)
        };
        let runtime = self.world_runtime.as_ref().expect("World runtime");
        let mut guarded: Vec<_> = inputs
            .iter()
            .enumerate()
            .filter(|(i, _)| runtime.regions[*i].team < 0)
            .map(|(_, (at, _))| {
                let (home, distance) = nearest(*at);
                (*at, home, distance)
            })
            .collect();
        guarded.sort_by(|a, b| b.2.total_cmp(&a.2));
        let count = guarded.len();
        for (index, (at, home, _)) in guarded.into_iter().enumerate() {
            if index as f64 >= count as f64 * 0.4 {
                continue;
            }
            let face = math::atan2(home.z - at.z, home.x - at.x);
            let (fx, fz) = (math::cos(face), math::sin(face));
            let Some(cell) = world_find_site(
                &self.world.game,
                &self.rules,
                Point {
                    x: at.x + fx * 14.0,
                    z: at.z + fz * 14.0,
                },
                2,
                &[(at, 8.0)],
            ) else {
                continue;
            };
            let id = self.world_building(LOCAL_DEFENDER_OWNER, "pillbox", cell)?;
            self.world
                .game
                .units
                .get_mut(&id)
                .expect("published box")
                .rot = face;
            if index as f64 >= count as f64 * 0.15 {
                continue;
            }
            if let Some(cell) = world_find_site(
                &self.world.game,
                &self.rules,
                Point {
                    x: at.x - fx * 6.0 + fz * 8.0,
                    z: at.z - fz * 6.0 - fx * 8.0,
                },
                1,
                &[(at, 6.0)],
            ) {
                self.world_building(LOCAL_DEFENDER_OWNER, "tower", cell)?;
            }
            let box_at = self.world.game.units[&id].at();
            for side in [-1.0, 1.0] {
                for i in 2..6 {
                    let Some(cell) = self.world.game.cell_of(
                        box_at.x - fz * side * f64::from(i) * CELL,
                        box_at.z + fx * side * f64::from(i) * CELL,
                    ) else {
                        continue;
                    };
                    if world_can_stamp(&self.world.game, &self.rules, &[cell]) {
                        let wall = self.world_building(LOCAL_DEFENDER_OWNER, "wall", cell)?;
                        self.world
                            .game
                            .units
                            .get_mut(&wall)
                            .expect("published wall")
                            .rot = face + std::f64::consts::FRAC_PI_2;
                    }
                }
            }
        }
        Ok(())
    }
    fn initialize_world_visibility(&mut self) -> Result<(), String> {
        let game = &self.world.game;
        if game
            .units
            .values()
            .any(|u| u.owner >= game.players.len() && u.owner != LOCAL_DEFENDER_OWNER)
        {
            return Err("invalid World visibility actor".into());
        }
        let runtime = self.world_runtime.as_mut().expect("World runtime");
        for player in &game.players {
            // Source shared teams reuse their visibility and do not revisit ghosts.
            if runtime
                .known_buildings
                .iter()
                .any(|(team, _)| *team == player.team)
            {
                continue;
            }
            let rows = player
                .visible
                .iter()
                .filter_map(|id| game.units.get(id))
                .filter(|u| self.rules.kinds[u.kind].building)
                .map(|u| KnownBuilding {
                    id: u.id,
                    kind: u.kind,
                    owner: u.owner,
                    at: u.at(),
                    built: u.built,
                })
                .collect();
            runtime.known_buildings.push((player.team, rows));
        }
        let mut seen = BTreeSet::new();
        for guard in game.units.values() {
            let kind = &self.rules.kinds[guard.kind];
            if guard.owner != LOCAL_DEFENDER_OWNER
                || (kind.building && !kind.weapon && !kind.lookout)
                || guard.built.is_some_and(|b| b < 1.0)
            {
                continue;
            }
            for id in self.grid.in_order(
                &game.units,
                (guard.x, guard.z),
                vision::vision_of(game, &self.rules, guard),
                |_, u| u.owner != LOCAL_DEFENDER_OWNER && u.hp > 0.0,
            ) {
                let enemy = &game.units[&id];
                if (kind.building
                    || vision::los(game, &self.rules, (guard.x, guard.z), (enemy.x, enemy.z)))
                    && seen.insert(id)
                {
                    runtime.local_defender.visible.push(id);
                }
            }
        }
        Ok(())
    }
}
fn world_can_stamp(game: &Game, rules: &crate::rules::Rules, cells: &[usize]) -> bool {
    if cells.is_empty()
        || cells.iter().any(|&c| {
            c >= game.flags.len()
                || game.flags[c] & MOVE != 0
                || b"WF=KOAQ".contains(&game.chars[c])
        })
    {
        return false;
    }
    let level = |c: usize| game.height.as_ref().map_or(0, |h| h[c]);
    let low = cells.iter().map(|&c| level(c)).min().expect("nonempty");
    let high = cells.iter().map(|&c| level(c)).max().expect("nonempty");
    i16::from(high) - i16::from(low) <= 1
        && !game.units.values().any(|u| {
            let kind = &rules.kinds[u.kind];
            u.hp > 0.0
                && !u.riding
                && !kind.air
                && !kind.structure
                && game.cell_of(u.x, u.z).is_some_and(|c| cells.contains(&c))
        })
}
fn world_find_site(
    game: &Game,
    rules: &crate::rules::Rules,
    at: Point,
    size: usize,
    avoid: &[(Point, f64)],
) -> Option<usize> {
    let start = game.cell_of(
        at.x.max(0.0).min(game.w as f64 * CELL - 1.0),
        at.z.max(0.0).min(game.h as f64 * CELL - 1.0),
    )?;
    let mut seen = vec![false; game.w * game.h];
    seen[start] = true;
    let mut queue = vec![start];
    let mut index = 0;
    while index < queue.len() && index < 4000 {
        let cell = queue[index];
        let pose = foot_center(game, cell, size);
        if footprint(game, cell, size).is_some_and(|cells| world_can_stamp(game, rules, &cells))
            && avoid
                .iter()
                .all(|(p, r)| math::hypot(p.x - pose.x, p.z - pose.z) >= *r)
        {
            return Some(cell);
        }
        let (x, y) = (cell % game.w, cell / game.w);
        for (nx, ny) in [
            (x as isize + 1, y as isize),
            (x as isize - 1, y as isize),
            (x as isize, y as isize + 1),
            (x as isize, y as isize - 1),
        ] {
            if nx < 0 || ny < 0 || nx as usize >= game.w || ny as usize >= game.h {
                continue;
            }
            let c = ny as usize * game.w + nx as usize;
            if !seen[c] {
                seen[c] = true;
                queue.push(c);
            }
        }
        index += 1;
    }
    None
}
