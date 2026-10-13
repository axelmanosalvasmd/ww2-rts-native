//! Source-positioned research and transactional single-building training.
use super::{Match, terrain_view};
use crate::{
    catalog::Catalog,
    economy::{Mode, Technology},
    economy_state::{ModeContext, Region, WorldContext},
    game::{Game, LOCAL_DEFENDER_OWNER, ModeKind},
    match_commands::UnitControl,
    navigation::Versions,
    pathfinding::{PathRules, PathStatus},
    produced_units::{ProduceInput, ProductionBuilding},
    production::{self, Context, UnitView},
    rules::Rules,
    unit_publication::{
        PublicationParts, StagedUnit, prepare_publication, prepare_world_publication,
    },
};
#[derive(Debug, PartialEq)]
pub enum ProductionStepError {
    InvalidState(&'static str),
    Production(production::Error),
    Placement(crate::produced_units::Error),
    Knowledge(crate::route_knowledge::KnowledgeError),
    Path(crate::pathfinding::PathError),
    Publication(crate::unit_publication::PublicationError),
    Control(String),
    Routing(String),
}
impl From<production::Error> for ProductionStepError {
    fn from(e: production::Error) -> Self {
        Self::Production(e)
    }
}
#[derive(Debug, PartialEq)]
pub struct CompletedTrainingUnit {
    pub unit: u32,
    pub completion: production::CompletedUnit,
    pub route_status: Option<PathStatus>,
}
fn units<'a>(
    game: &Game,
    rules: &'a Rules,
    count: usize,
) -> Result<Vec<UnitView<'a>>, ProductionStepError> {
    game.units
        .iter()
        .map(|(&id, u)| {
            if id != u.id
                || (u.owner >= count
                    && !(game.mode == Some(ModeKind::World) && u.owner == LOCAL_DEFENDER_OWNER))
            {
                return Err(ProductionStepError::InvalidState(
                    "production unit identity/owner",
                ));
            }
            let kind = rules
                .kinds
                .get(u.kind)
                .ok_or(ProductionStepError::InvalidState("production unit kind"))?;
            Ok(UnitView {
                id,
                owner: (u.owner != LOCAL_DEFENDER_OWNER).then_some(u.owner),
                kind: &kind.name,
                hp: u.hp,
                built: u.built.unwrap_or(1.0),
            })
        })
        .collect()
}
fn context<'a>(
    options: &super::MatchOptions,
    naval: bool,
    units: &'a [UnitView<'a>],
    regions: Option<&'a [Region]>,
) -> Context<'a> {
    Context {
        mode: ModeContext {
            kind: options.mode,
            ..Default::default()
        },
        army: options.army,
        tech_enabled: options.tech,
        naval,
        sudden_death: false,
        units,
        drops: &[],
        world: regions.map(|regions| WorldContext { regions }),
    }
}
macro_rules! training_parts {
    ($m:expr, $game:expr) => {
        TrainingParts {
            game: $game,
            rules: &$m.rules,
            options: &$m.options,
            production: &mut $m.production,
            economy: &$m.start.economy,
            commands: &mut $m.commands,
            grid: &mut $m.grid,
            recruitment: &mut $m.recruit_metadata,
            next_id: &mut $m.next_unit_id,
            combat: &mut $m.combat,
            firing: &mut $m.firing,
            abilities: &mut $m.abilities,
            recovery: &mut $m.recovery,
        }
    };
}
impl Match {
    fn production_step_context(&self) -> Result<usize, ProductionStepError> {
        let count = self.start.players.len();
        let supported = match self.options.mode {
            Mode::Default | Mode::Conquest => self.world.game.mode.is_none(),
            Mode::World => {
                self.world.game.mode == Some(ModeKind::World) && self.world_runtime.is_some()
            }
            _ => false,
        };
        if !supported
            || self.world.game.tick < 0
            || count != self.world.game.players.len()
            || count != self.start.economy.players.len()
            || count != self.player_rallies.len()
        {
            return Err(ProductionStepError::InvalidState("production mode/context"));
        }
        for (slot, (player, start)) in self
            .world
            .game
            .players
            .iter()
            .zip(&self.start.players)
            .enumerate()
        {
            if player.slot != slot
                || player.team != start.config.team
                || player.team != self.start.economy.players[slot].team
            {
                return Err(ProductionStepError::InvalidState("production player/team"));
            }
        }
        Ok(count)
    }
    fn production_regions(&self) -> Result<Option<Vec<Region>>, ProductionStepError> {
        if self.options.mode == Mode::World {
            Ok(Some(
                self.world_economy_regions()
                    .map_err(ProductionStepError::Routing)?,
            ))
        } else {
            Ok(None)
        }
    }
    /// Typed source `tech` admission. Global command gates precede the research
    /// denials. Wallet, lab and canonical MP-spent story publish together.
    pub fn request_research(
        &mut self,
        slot: usize,
        kind: Technology,
    ) -> Result<crate::economy::ResearchCost, ProductionStepError> {
        if self.finish.is_some()
            || self
                .start
                .economy
                .players
                .get(slot)
                .is_none_or(|p| p.away || p.out)
        {
            return Err(production::Error::Denied(production::Denial::Blocked).into());
        }
        let count = self.production_step_context()?;
        let regions = self.production_regions()?;
        let units = units(&self.world.game, &self.rules, count)?;
        let context = context(
            &self.options,
            self.world.game.naval,
            &units,
            regions.as_deref(),
        );
        let prepared =
            self.production
                .prepare_research(&context, &mut self.start.economy, slot, kind)?;
        let story = crate::match_story::prepare_story_updates(
            &mut self.story,
            false,
            &[crate::match_story::StoryChange {
                slot: Some(slot),
                key: crate::match_story::StoryKey::MpSpent,
                amount: prepared.cost().mp,
            }],
        )
        .map_err(ProductionStepError::Control)?;
        let cost = prepared.commit();
        story.commit();
        Ok(cost)
    }
    /// Called at the source early lab phase, before the per-unit loop. Existing
    /// research continues after a winner and for away seats; out seats pause.
    pub fn step_research(
        &mut self,
        dt: f64,
    ) -> Result<Vec<(usize, Technology)>, ProductionStepError> {
        let count = self.production_step_context()?;
        let regions = self.production_regions()?;
        let units = units(&self.world.game, &self.rules, count)?;
        let context = context(
            &self.options,
            self.world.game.naval,
            &units,
            regions.as_deref(),
        );
        Ok(self
            .production
            .advance_research(&context, &self.start.economy, dt)?)
    }
    /// Called at this building's live unit-loop position. A completed unit enters
    /// the same loop later by its fresh ascending ID. No wallet is charged again.
    pub fn step_training(
        &mut self,
        building: u32,
        dt: f64,
    ) -> Result<Option<CompletedTrainingUnit>, ProductionStepError> {
        let count = self.production_step_context()?;
        if !dt.is_finite() || dt < 0.0 {
            return Err(production::Error::InvalidInput("dt").into());
        }
        if self.options.mode != Mode::World
            && self
                .production
                .building(building)
                .is_some_and(|b| !b.jobs().is_empty())
        {
            return Err(ProductionStepError::InvalidState(
                "nonempty Conquest training queue",
            ));
        }
        if self
            .production
            .building(building)
            .is_some_and(|b| !b.jobs().is_empty())
        {
            if self.options.logistics {
                return Err(ProductionStepError::InvalidState(
                    "training reserves initialization unsupported",
                ));
            }
            if !self.building_rallies.contains_key(&building) {
                return Err(ProductionStepError::InvalidState(
                    "canonical training rally missing",
                ));
            }
        }
        let regions = self.production_regions()?;
        let completion = {
            let units = units(&self.world.game, &self.rules, count)?;
            let context = context(
                &self.options,
                self.world.game.naval,
                &units,
                regions.as_deref(),
            );
            let prepared = self.production.prepare_building_step(
                &context,
                &self.start.economy,
                building,
                dt,
            )?;
            if let Some(completed) = prepared.completed().cloned() {
                Some(completed)
            } else {
                prepared.commit();
                None
            }
        };
        let Some(completion) = completion else {
            return Ok(None);
        };
        let owner = completion.owner;
        let rally =
            *self
                .building_rallies
                .get(&building)
                .ok_or(ProductionStepError::InvalidState(
                    "canonical training rally missing",
                ))?;
        let game = &self.world.game;
        let id = self.next_unit_id;
        if self.combat.get(id).is_some()
            || self.firing.get(id).is_some()
            || self.abilities.get(id).is_some()
            || self.recovery.get(id).is_some()
        {
            return Err(ProductionStepError::InvalidState(
                "training metadata identity occupied",
            ));
        }
        let maker = &game.units[&building];
        let kind = self
            .rules
            .kind(&completion.kind)
            .ok_or(ProductionStepError::InvalidState("training unit kind"))?;
        let mut view = terrain_view(game);
        view.versions = Versions {
            infantry: self.world.versions.infantry,
            vehicle: self.world.versions.vehicle,
            naval: self.world.versions.naval,
            obstruction: self.world.versions.obstruction,
            knowledge: 0,
        };
        let placed = self
            .produced_placer
            .produce(
                &self.rules,
                Catalog::compiled().map_err(|e| ProductionStepError::Production(e.into()))?,
                view,
                &game.units,
                ProduceInput {
                    id,
                    owner,
                    kind,
                    scatter_index: u32::try_from(game.units.len())
                        .map_err(|_| ProductionStepError::InvalidState("training unit count"))?,
                    player_spawn: self.start.players[owner].spawn,
                    player_rally: self.player_rallies[owner],
                    building: ProductionBuilding {
                        at: maker.at(),
                        kind: maker.kind,
                        rally,
                    },
                    mode: self.options.mode,
                    tick: game.tick as u64,
                },
            )
            .map_err(ProductionStepError::Placement)?;
        let mut draft = TrainingDraft {
            building,
            dt,
            count,
            regions,
            completion,
            kind,
            placed,
            route_status: None,
            route_end: None,
        };
        if draft.placed.rally.is_none() {
            return training_parts!(self, &mut self.world.game)
                .finish(draft)
                .map(Some);
        }
        let mut paths = self.commands.paths.clone();
        let result = (|| {
            let observer = crate::staged_observer::StagedObserver::new(
                &self.world.game,
                &self.rules,
                id,
                owner,
                &draft.placed.unit,
            )
            .map_err(ProductionStepError::Routing)?;
            // Paid queues are admitted only by the represented World constructor.
            let runtime = self
                .world_runtime
                .as_mut()
                .ok_or(ProductionStepError::InvalidState("training World runtime"))?;
            let preview = crate::match_world_memory::prepare_world_routing(
                crate::match_world_memory::WorldRoutingContext {
                    world: &self.world,
                    rules: &self.rules,
                    start: &self.start,
                    rails: &runtime.rails,
                    regions: &runtime.regions,
                    known_buildings: &runtime.known_buildings,
                    local_defender: &runtime.local_defender,
                },
                &mut runtime.memory,
                owner,
                Some(observer),
            )
            .map_err(ProductionStepError::Routing)?;
            route_training(
                &self.rules,
                &mut paths,
                &mut draft,
                preview.query(),
                preview.live(),
            )?;
            let (game, routing_commit) = preview
                .prepare_publication(&mut self.world)
                .map_err(ProductionStepError::Routing)?;
            let done = training_parts!(self, game).finish(draft)?;
            routing_commit.commit();
            Ok(Some(done))
        })();
        if result.is_ok() {
            self.commands.paths = paths;
        }
        result
    }
}
struct TrainingDraft {
    building: u32,
    dt: f64,
    count: usize,
    regions: Option<Vec<Region>>,
    completion: production::CompletedUnit,
    kind: usize,
    placed: crate::produced_units::ProducedPlacement,
    route_status: Option<PathStatus>,
    route_end: Option<crate::game::Point>,
}
fn route_training(
    rules: &Rules,
    paths: &mut crate::pathfinding::Pathfinding,
    draft: &mut TrainingDraft,
    query: crate::route_knowledge::RouteQuery<'_>,
    live: crate::route_knowledge::LiveRouting<'_>,
) -> Result<(), ProductionStepError> {
    let rally = draft.placed.rally.expect("staged rally");
    let path = paths
        .find_path(
            query
                .path_terrain(live)
                .map_err(ProductionStepError::Knowledge)?,
            PathRules::compiled().map_err(|e| ProductionStepError::Production(e.into()))?,
            &rules.kinds[draft.kind],
            draft.placed.unit.at(),
            rally.destination,
            None,
        )
        .map_err(ProductionStepError::Path)?;
    draft.placed.unit.path = path.waypoints;
    draft.placed.unit.path_gen = 1;
    draft.route_end = Some(path.route_end);
    draft.route_status = Some(path.status);
    Ok(())
}
struct TrainingParts<'a> {
    game: &'a mut Game,
    rules: &'a Rules,
    options: &'a super::MatchOptions,
    production: &'a mut production::ProductionState,
    economy: &'a crate::economy_state::EconomyState,
    commands: &'a mut crate::match_commands::MatchCommands,
    grid: &'a mut crate::grid::SpatialGrid,
    recruitment: &'a mut std::collections::BTreeMap<u32, super::RecruitMetadata>,
    next_id: &'a mut u32,
    combat: &'a mut crate::match_combat::CombatState,
    firing: &'a mut crate::match_firing::FiringState,
    abilities: &'a mut crate::match_abilities::AbilityState,
    recovery: &'a mut crate::match_recovery::RecoveryState,
}

impl TrainingParts<'_> {
    fn finish(self, draft: TrainingDraft) -> Result<CompletedTrainingUnit, ProductionStepError> {
        let TrainingDraft {
            building,
            dt,
            count,
            regions,
            completion: expected,
            placed,
            route_status,
            route_end,
            ..
        } = draft;
        let mut control = UnitControl::new_for_mode(&placed.unit, self.rules, self.options.mode)
            .map_err(ProductionStepError::Control)?;
        control.aim = placed.aim;
        control.rally_drift = placed.rally.is_some();
        control.route_end = route_end;
        let id = placed.unit.id;
        let units = units(self.game, self.rules, count)?;
        let context = context(self.options, self.game.naval, &units, regions.as_deref());
        let prepared =
            self.production
                .prepare_building_step(&context, self.economy, building, dt)?;
        if prepared.completed() != Some(&expected) {
            return Err(ProductionStepError::InvalidState(
                "training facts changed during routing preparation",
            ));
        }
        let mode = self.game.mode;
        let parts = PublicationParts {
            units: &mut self.game.units,
            grid: self.grid,
            controls: &mut self.commands.units,
            recruitment: self.recruitment,
            next_id: self.next_id,
        };
        let staged = vec![StagedUnit {
            unit: placed.unit,
            control,
            recruitment: None,
        }];
        let publication = if self.options.mode == Mode::World {
            prepare_world_publication(parts, self.rules, count, mode, staged)
        } else {
            prepare_publication(parts, self.rules, count, staged)
        }
        .map_err(ProductionStepError::Publication)?;
        let completion = prepared.commit().expect("checked training completion");
        publication.publish_with(|unit, _| {
            self.combat.initialize(unit.id);
            self.firing.initialize(unit.id);
            self.abilities.initialize(unit.id);
            self.recovery.initialize(unit.id);
        });
        Ok(CompletedTrainingUnit {
            unit: id,
            completion,
            route_status,
        })
    }
}
