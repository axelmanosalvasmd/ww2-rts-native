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
pub(super) fn units<'a>(
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
pub(super) fn context<'a>(
    options: &super::MatchOptions,
    naval: bool,
    units: &'a [UnitView<'a>],
    regions: Option<&'a [Region]>,
    sudden_death: bool,
) -> Context<'a> {
    Context {
        mode: ModeContext {
            kind: options.mode,
            ..Default::default()
        },
        army: options.army,
        tech_enabled: options.tech,
        naval,
        sudden_death,
        units,
        drops: &[],
        world: regions.map(|regions| WorldContext { regions }),
    }
}
impl Match {
    pub(super) fn production_step_context(&self) -> Result<usize, ProductionStepError> {
        let count = self.start.players.len();
        let supported = match self.options.mode {
            Mode::Default | Mode::Conquest => self.world.game.mode.is_none(),
            Mode::Classic => {
                self.world.game.mode == Some(ModeKind::Classic) && self.classic_runtime.is_some()
            }
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
    pub(super) fn production_regions(&self) -> Result<Option<Vec<Region>>, ProductionStepError> {
        if self.options.mode == Mode::World {
            Ok(Some(
                self.world_economy_regions()
                    .map_err(ProductionStepError::Routing)?,
            ))
        } else {
            Ok(None)
        }
    }
    /// Admit research and live-building progression before the caller advances its clock.
    /// Publication still validates the actual placement at each building's later turn.
    pub fn validate_production_phases(&self, dt: f64) -> Result<(), ProductionStepError> {
        let count = self.production_step_context()?;
        let regions = self.production_regions()?;
        let units = units(&self.world.game, &self.rules, count)?;
        let context = context(
            &self.options,
            self.world.game.naval,
            &units,
            regions.as_deref(),
            self.classic_sudden_death(),
        );
        self.production
            .validate_phases(&context, &self.start.economy, dt)?;
        for (id, row) in self.production.buildings() {
            let unit = self
                .world
                .game
                .units
                .get(&id)
                .ok_or(ProductionStepError::InvalidState(
                    "production building missing",
                ))?;
            if !self.rules.kinds[unit.kind].building {
                return Err(ProductionStepError::InvalidState(
                    "production row is not a building",
                ));
            }
            if !row.jobs().is_empty() {
                if !self.options.mode.is_construction() {
                    return Err(ProductionStepError::InvalidState(
                        "nonempty Conquest training queue",
                    ));
                }
                if self.options.logistics
                    || unit.owner >= count
                    || !self.building_rallies.contains_key(&id)
                {
                    return Err(ProductionStepError::InvalidState(
                        "training publication context",
                    ));
                }
                if self.building_rallies[&id]
                    .into_iter()
                    .chain(self.player_rallies[unit.owner])
                    .any(|at| !at.x.is_finite() || !at.z.is_finite())
                {
                    return Err(ProductionStepError::InvalidState("training rally position"));
                }
                for job in row.jobs() {
                    crate::match_world_tick::validate_world_future_birth(&self.rules, &job.kind)
                        .map_err(ProductionStepError::Control)?;
                }
            }
        }
        Ok(())
    }
    /// Recorded World Buy queues training, without an immediate birth. Canonical
    /// production admission precedes publication checks and atomic MP-spent payment.
    pub(crate) fn recorded_world_buy(
        &mut self,
        slot: usize,
        kind: &str,
        from: Option<u32>,
    ) -> Result<u64, ProductionStepError> {
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
        let phase = self
            .validate_world_phase_context(0.0)
            .map_err(ProductionStepError::Control)?;
        self.validate_world_path_service()
            .map_err(ProductionStepError::Routing)?;
        self.validate_production_phases(0.0)?;
        let count = self.production_step_context()?;
        let units = units(&self.world.game, &self.rules, count)?;
        let context = context(
            &self.options,
            self.world.game.naval,
            &units,
            Some(&phase.regions),
            false,
        );
        let registered = self
            .production
            .buildings()
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        let prepared = self.production.prepare_queue_unit(
            &context,
            &mut self.start.economy,
            slot,
            self.start.players[slot].config.faction,
            kind,
            from,
        )?;
        crate::match_world_tick::validate_world_future_birth(&self.rules, kind)
            .map_err(ProductionStepError::Control)?;
        let building = prepared.building();
        if !registered.contains(&building) {
            return Err(ProductionStepError::InvalidState(
                "canonical producer state missing",
            ));
        }
        if !self.building_rallies.contains_key(&building) {
            return Err(ProductionStepError::InvalidState(
                "canonical training rally missing",
            ));
        }
        if self.building_rallies[&building]
            .into_iter()
            .chain(self.player_rallies[slot])
            .any(|at| !at.x.is_finite() || !at.z.is_finite())
        {
            return Err(ProductionStepError::InvalidState("training rally position"));
        }
        let story = crate::match_story::prepare_story_updates(
            &mut self.story,
            false,
            &[crate::match_story::StoryChange {
                slot: Some(slot),
                key: crate::match_story::StoryKey::MpSpent,
                amount: prepared.charge().mp,
            }],
        )
        .map_err(ProductionStepError::Control)?;
        let job = prepared.commit();
        story.commit();
        Ok(job)
    }
    /// Actual Classic paid queue caller. No World context or admission is reused.
    pub(crate) fn recorded_classic_buy(
        &mut self, slot: usize, kind: &str, from: Option<u32>,
    ) -> Result<u64, ProductionStepError> {
        self.validate_classic_base_state().map_err(ProductionStepError::Control)?;
        if self.finish.is_some() || self.start.economy.players.get(slot).is_none_or(|p| p.away || p.out) {
            return Err(production::Error::Denied(production::Denial::Blocked).into());
        }
        let count = self.production_step_context()?;
        let units = units(&self.world.game, &self.rules, count)?;
        let context = context(&self.options, self.world.game.naval, &units, None, self.classic_sudden_death());
        let registered = self.production.buildings().map(|(id, _)| id).collect::<Vec<_>>();
        let prepared = self.production.prepare_queue_unit(&context, &mut self.start.economy,
            slot, self.start.players[slot].config.faction, kind, from)?;
        // The connected birth provider currently represents ordinary ground
        // units. Preserve the unrepresented air/rail/naval boundary before pay.
        crate::match_world_tick::validate_world_future_birth(&self.rules, kind)
            .map_err(ProductionStepError::Control)?;
        let building = prepared.building();
        if !registered.contains(&building) || !self.building_rallies.contains_key(&building) {
            return Err(ProductionStepError::InvalidState("Classic canonical producer metadata"));
        }
        let story = crate::match_story::prepare_story_updates(&mut self.story, false,
            &[crate::match_story::StoryChange { slot: Some(slot),
                key: crate::match_story::StoryKey::MpSpent, amount: prepared.charge().mp }])
            .map_err(ProductionStepError::Control)?;
        let job = prepared.commit();
        story.commit();
        Ok(job)
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
            self.classic_sudden_death(),
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
            self.classic_sudden_death(),
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
        if !self.options.mode.is_construction()
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
            for job in self.production.building(building).unwrap().jobs() {
                crate::match_world_tick::validate_world_future_birth(&self.rules, &job.kind)
                    .map_err(ProductionStepError::Control)?;
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
            self.classic_sudden_death(),
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
            || self.aviation.planes.contains_key(&id)
            || self.aviation.anti_air.contains_key(&id)
            || self.logistics.get(id).is_some()
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
        let mut placed = self
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
        if placed.rally.is_some() {
            if self.options.mode == Mode::World {
                self.refresh_world_routing(owner)
                    .map_err(ProductionStepError::Routing)?;
            } else {
                self.refresh_recipient_routing(owner)
                    .map_err(ProductionStepError::Routing)?;
            }
        }
        let mut status = None;
        let mut route_end = None;
        if let Some(rally) = placed.rally {
            let (query, live) = if self.options.mode == Mode::World {
                let memory = &self
                    .world_runtime
                    .as_ref()
                    .ok_or(ProductionStepError::InvalidState("training World runtime"))?
                    .memory;
                let team = self.world.game.players[owner].team;
                let remembered = memory
                    .team(team)
                    .ok_or(ProductionStepError::InvalidState("training World memory"))?;
                let live = memory
                    .live_for(owner)
                    .ok_or(ProductionStepError::InvalidState(
                        "training World live routing",
                    ))?;
                (remembered.query(), live.live())
            } else {
                let recipient =
                    self.recipients
                        .get(owner)
                        .ok_or(ProductionStepError::InvalidState(
                            "training routing recipient",
                        ))?;
                (recipient.query(), recipient.live())
            };
            let path = self
                .commands
                .paths
                .find_path(
                    query
                        .path_terrain(live)
                        .map_err(ProductionStepError::Knowledge)?,
                    PathRules::compiled().map_err(|e| ProductionStepError::Production(e.into()))?,
                    &self.rules.kinds[kind],
                    placed.unit.at(),
                    rally.destination,
                    None,
                )
                .map_err(ProductionStepError::Path)?;
            placed.unit.path = path.waypoints;
            placed.unit.path_gen = 1;
            route_end = Some(path.route_end);
            status = Some(path.status);
        }
        let mut control = UnitControl::new_for_mode(&placed.unit, &self.rules, self.options.mode)
            .map_err(ProductionStepError::Control)?;
        control.aim = placed.aim;
        control.rally_drift = placed.rally.is_some();
        control.route_end = route_end;
        let units = units(&self.world.game, &self.rules, count)?;
        let context = context(
            &self.options,
            self.world.game.naval,
            &units,
            regions.as_deref(),
            self.classic_sudden_death(),
        );
        let prepared =
            self.production
                .prepare_building_step(&context, &self.start.economy, building, dt)?;
        let parts = PublicationParts {
            units: &mut self.world.game.units,
            grid: &mut self.grid,
            controls: &mut self.commands.units,
            recruitment: &mut self.recruit_metadata,
            next_id: &mut self.next_unit_id,
        };
        let staged = vec![StagedUnit {
            unit: placed.unit,
            control,
            recruitment: None,
        }];
        let publication = if self.options.mode == Mode::World {
            prepare_world_publication(parts, &self.rules, count, self.world.game.mode, staged)
        } else {
            prepare_publication(parts, &self.rules, count, staged)
        }
        .map_err(ProductionStepError::Publication)?;
        let completion = prepared.commit().expect("staged training completion");
        publication.publish_with(|unit, _| {
            self.combat.initialize(unit.id);
            self.firing.initialize(unit.id);
            self.abilities.initialize(unit.id);
            self.recovery.initialize(unit.id);
        });
        Ok(Some(CompletedTrainingUnit {
            unit: id,
            completion,
            route_status: status,
        }))
    }
}
