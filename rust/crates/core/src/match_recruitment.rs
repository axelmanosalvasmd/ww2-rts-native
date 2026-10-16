//! Atomic instant recruitment, produced-unit placement and owner-observed rally routing.
use super::{Match, terrain_view};
use crate::{
    catalog::Catalog,
    economy_state::{ModeContext, PendingDrop},
    game::StrikeKind,
    navigation::Versions,
    observed_memory::ObservedMemory,
    pathfinding::{PathRules, PathStatus},
    produced_units::{ProduceInput, ProductionBuilding, RallyKind},
    production::{self, Context, Denial, UnitView},
    route_knowledge::{KnowledgeError, LiveRouting, RouteQuery},
};

#[derive(Debug, PartialEq)]
pub enum RecruitError {
    Admission(production::Error),
    InvalidState(&'static str),
    Placement(crate::produced_units::Error),
    Knowledge(KnowledgeError),
    Path(crate::pathfinding::PathError),
    Publication(crate::unit_publication::PublicationError),
    Routing(String),
    Story(String),
}
impl From<production::Error> for RecruitError {
    fn from(e: production::Error) -> Self {
        Self::Admission(e)
    }
}
/// Immutable facts from the recruitment transaction, separate from live control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecruitMetadata {
    pub building: u32,
    pub initial_rally: Option<RallyKind>,
    pub initial_route_status: Option<PathStatus>,
}
#[derive(Debug)]
pub struct CompletedRecruitUnit {
    pub unit: u32,
    pub purchase: production::CompletedRecruit,
    pub route_status: Option<PathStatus>,
}
/// The remembered query is bound to its actual ObservedMemory producer. Live
/// hazards and weather must already have the source's recipient visibility policy.
pub struct RecruitRouting<'a> {
    slot: usize,
    team: i32,
    query: &'a RouteQuery<'a>,
    live: LiveRouting<'a>,
}
impl<'a> RecruitRouting<'a> {
    pub fn new(
        memory: &ObservedMemory,
        team: i32,
        query: &'a RouteQuery<'a>,
        live: LiveRouting<'a>,
    ) -> Result<Self, RecruitError> {
        let view = query.navigation();
        if view.knowledge_id != memory.identity() || view.world_known {
            return Err(RecruitError::InvalidState("recruit routing recipient"));
        }
        Ok(Self {
            slot: memory.slot(),
            team,
            query,
            live,
        })
    }
}
struct RecruitParts<'a> {
    game: &'a mut crate::game::Game,
    versions: Versions,
    start: &'a mut super::StartPlan,
    options: &'a super::MatchOptions,
    rules: &'a crate::rules::Rules,
    production: &'a production::ProductionState,
    player_rallies: &'a [Option<crate::game::Point>],
    building_rallies: &'a std::collections::BTreeMap<u32, Option<crate::game::Point>>,
    produced_placer: &'a mut crate::produced_units::ProducedPlacer,
    commands: &'a mut crate::match_commands::MatchCommands,
    combat: &'a mut crate::match_combat::CombatState,
    firing: &'a mut crate::match_firing::FiringState,
    abilities: &'a mut crate::match_abilities::AbilityState,
    recovery: &'a mut crate::match_recovery::RecoveryState,
    story: &'a mut [crate::match_story::PlayerStory],
    grid: &'a mut crate::grid::SpatialGrid,
    recruit_metadata: &'a mut std::collections::BTreeMap<u32, RecruitMetadata>,
    next_unit_id: &'a mut u32,
}
struct RecruitFacts<'a> {
    count: usize,
    horde_slot: Option<usize>,
    units: Vec<UnitView<'a>>,
    drops: Vec<PendingDrop<'static>>,
}
impl RecruitFacts<'_> {
    fn context<'a>(&'a self, options: &super::MatchOptions, naval: bool) -> Context<'a> {
        Context {
            mode: ModeContext {
                kind: options.mode,
                defender_team: self.horde_slot.map(|_| 0),
                income_excluded_slot: self.horde_slot,
            },
            army: options.army,
            tech_enabled: options.tech,
            naval,
            sudden_death: false,
            units: &self.units,
            drops: &self.drops,
            world: None,
        }
    }
}
fn admission_facts<'a>(
    game: &crate::game::Game,
    start: &super::StartPlan,
    options: &super::MatchOptions,
    rules: &'a crate::rules::Rules,
    player_rallies: &[Option<crate::game::Point>],
) -> Result<RecruitFacts<'a>, RecruitError> {
    let count = start.players.len();
    let horde = options.mode == crate::economy::Mode::Horde
        && game.mode == Some(crate::game::ModeKind::Horde)
        && (2..=6).contains(&count)
        && start.shared_defenders == Some(count - 1);
    if count != game.players.len()
        || count != start.economy.players.len()
        || player_rallies.len() != count
        || game.tick < 0
        || (!crate::match_actor::ordinary_ground_mode(game, options.mode) && !horde)
        || options.logistics
    {
        return Err(RecruitError::InvalidState("Conquest recruitment state"));
    }
    for (i, (player, configured)) in game.players.iter().zip(&start.players).enumerate() {
        if player.slot != i
            || player.team != configured.config.team
            || player.team != start.economy.players[i].team
        {
            return Err(RecruitError::InvalidState("recruitment player identity"));
        }
    }
    if game
        .strikes
        .iter()
        .any(|s| s.kind == StrikeKind::Other && !s.live && s.runtime.is_none())
    {
        return Err(RecruitError::InvalidState(
            "pending support needs exact kind metadata",
        ));
    }
    let mut units = Vec::with_capacity(game.units.len());
    for (id, u) in &game.units {
        if *id != u.id {
            return Err(RecruitError::InvalidState("canonical unit identity"));
        }
        let profile = rules
            .kinds
            .get(u.kind)
            .ok_or(RecruitError::InvalidState("unit kind"))?;
        units.push(UnitView {
            id: u.id,
            owner: Some(u.owner),
            kind: &profile.name,
            hp: u.hp,
            built: u.built.unwrap_or(0.0),
        });
    }
    let drops: Vec<_> = game
        .strikes
        .iter()
        .map(|s| PendingDrop {
            owner: s.owner,
            kind: s.runtime.as_ref().map_or("recon", |t| t.kind.name()),
            live: s.live,
        })
        .collect();
    Ok(RecruitFacts {
        count,
        horde_slot: horde.then(|| count - 1),
        units,
        drops,
    })
}
/// Owns only a prepared unpublished unit and its admission result. Routing refresh
/// cannot charge or publish it and cannot alter the authoritative purchase facts.
struct StagedRecruit {
    slot: usize,
    kind: String,
    from: Option<u32>,
    winner_decided: bool,
    request: production::RecruitRequest,
    kind_index: usize,
    placed: crate::produced_units::ProducedPlacement,
    route_end: Option<crate::game::Point>,
    route_status: Option<PathStatus>,
}
macro_rules! recruit_parts {
    ($m:expr) => {
        recruit_parts!(
            $m,
            &mut $m.world.game,
            Versions {
                infantry: $m.world.versions.infantry,
                vehicle: $m.world.versions.vehicle,
                naval: $m.world.versions.naval,
                obstruction: $m.world.versions.obstruction,
                knowledge: 0,
            }
        )
    };
    ($m:expr, $game:expr, $versions:expr) => {
        RecruitParts {
            game: $game,
            versions: $versions,
            start: &mut $m.start,
            options: &$m.options,
            rules: &$m.rules,
            production: &$m.production,
            player_rallies: &$m.player_rallies,
            building_rallies: &$m.building_rallies,
            produced_placer: &mut $m.produced_placer,
            commands: &mut $m.commands,
            combat: &mut $m.combat,
            firing: &mut $m.firing,
            abilities: &mut $m.abilities,
            recovery: &mut $m.recovery,
            story: &mut $m.story,
            grid: &mut $m.grid,
            recruit_metadata: &mut $m.recruit_metadata,
            next_unit_id: &mut $m.next_unit_id,
        }
    };
}
impl Match {
    /// Execute recorded Conquest purchases against the canonical finish and seat state.
    /// Routing borrows the actual refreshed recipient projection, never a copied view.
    pub fn recorded_buy(
        &mut self,
        slot: usize,
        kind: &str,
        from: Option<u32>,
    ) -> Result<CompletedRecruitUnit, RecruitError> {
        let wallet = self
            .start
            .economy
            .players
            .get(slot)
            .ok_or(production::Error::Denied(Denial::Blocked))?;
        if self.finish.is_some() || wallet.away || wallet.out {
            return Err(production::Error::Denied(Denial::Blocked).into());
        }
        self.validate_ordinary_finish_state()
            .map_err(RecruitError::Story)?;
        let staged = recruit_parts!(self).stage(slot, kind, from, false)?;
        self.finish_recorded_buy(staged)
    }
    fn finish_recorded_buy(
        &mut self,
        mut staged: StagedRecruit,
    ) -> Result<CompletedRecruitUnit, RecruitError> {
        let slot = staged.slot;
        if staged.placed.rally.is_none() {
            return recruit_parts!(self).finish(staged);
        }
        let result = (|| {
            let versions = Versions {
                infantry: self.world.versions.infantry,
                vehicle: self.world.versions.vehicle,
                naval: self.world.versions.naval,
                obstruction: self.world.versions.obstruction,
                knowledge: 0,
            };
            let observer = crate::staged_observer::StagedObserver::new(
                &self.world.game,
                &self.rules,
                staged.placed.unit.id,
                slot,
                &staged.placed.unit,
            )
            .map_err(RecruitError::Routing)?;
            let preview = crate::match_routing::prepare_recipient_routing(
                crate::match_routing::RecipientContext {
                    world: &self.world,
                    environment: &self.environment,
                    rules: &self.rules,
                    building_cells: &self.building_cells,
                    start: &self.start,
                    options: &self.options,
                },
                self.recipients
                    .get_mut(slot)
                    .ok_or_else(|| RecruitError::Routing("invalid routing recipient".into()))?,
                &mut self.routing_fog,
                slot,
                Some(observer),
            )
            .map_err(RecruitError::Routing)?;
            let mut paths = preview.fork_paths(&self.commands.paths);
            let query = preview.query();
            let routing = RecruitRouting {
                slot,
                team: self.world.game.players[slot].team,
                query: &query,
                live: preview.live(),
            };
            route_recruit(
                &self.world.game,
                &self.rules,
                &mut paths,
                &mut staged,
                Some(routing),
            )?;
            let (game, routing_commit) = preview
                .prepare_publication(&mut self.world, &self.environment)
                .map_err(RecruitError::Routing)?;
            let done = recruit_parts!(self, game, versions).finish(staged)?;
            routing_commit.commit();
            Ok((done, paths))
        })();
        result.map(|(done, paths)| {
            self.commands.paths = paths;
            done
        })
    }
    /// Public component actuator retaining explicit winner and recipient inputs.
    pub fn recruit(
        &mut self,
        slot: usize,
        kind: &str,
        from: Option<u32>,
        winner_decided: bool,
        routing: Option<RecruitRouting<'_>>,
    ) -> Result<CompletedRecruitUnit, RecruitError> {
        if self.options.mode == crate::economy::Mode::Tutorial {
            self.validate_tutorial_state()
                .map_err(RecruitError::Story)?;
        }
        let mut parts = recruit_parts!(self);
        let mut staged = parts.stage(slot, kind, from, winner_decided)?;
        let mut paths = parts.commands.paths.clone();
        route_recruit(parts.game, parts.rules, &mut paths, &mut staged, routing)?;
        let done = parts.finish(staged)?;
        parts.commands.paths = paths;
        Ok(done)
    }
}
impl RecruitParts<'_> {
    /// The typed buy actuator. A winner, away or eliminated seat is blocked.
    /// Routing is required only when produceUnit emits a rally request. No wallet,
    /// ID, unit or grid changes occur until placement and route input checks pass.
    /// Unreachable or deferred routes still create the purchased unit with an empty
    /// path, goal and rally drift retained, matching the source's immediate search.
    fn stage(
        &mut self,
        slot: usize,
        kind: &str,
        from: Option<u32>,
        winner_decided: bool,
    ) -> Result<StagedRecruit, RecruitError> {
        let Some(wallet) = self.start.economy.players.get(slot) else {
            return Err(production::Error::Denied(Denial::Blocked).into());
        };
        if winner_decided || wallet.away || wallet.out {
            return Err(production::Error::Denied(Denial::Blocked).into());
        }
        let game = &*self.game;
        let facts = admission_facts(
            self.game,
            self.start,
            self.options,
            self.rules,
            self.player_rallies,
        )?;
        let count = facts.count;
        let context = facts.context(self.options, game.naval);
        let prepared = if self.options.mode == crate::economy::Mode::Tutorial {
            self.production.prepare_tutorial_recruit(
                &context,
                &mut self.start.economy,
                slot,
                self.start.players[slot].config.faction,
                kind,
            )?
        } else {
            self.production.prepare_recruit(
                &context,
                &mut self.start.economy,
                slot,
                self.start.players[slot].config.faction,
                kind,
                from,
            )?
        };
        let request = prepared.request();
        let maker = if self.options.mode == crate::economy::Mode::Tutorial {
            None
        } else {
            let maker = game
                .units
                .get(&request.building)
                .ok_or(RecruitError::InvalidState("admitted maker missing"))?;
            let rally = *self
                .building_rallies
                .get(&maker.id)
                .ok_or(RecruitError::InvalidState(
                    "canonical producer rally missing",
                ))?;
            if self.production.building(maker.id).is_none() {
                return Err(RecruitError::InvalidState(
                    "canonical producer state missing",
                ));
            }
            Some((maker, rally))
        };
        let kind_index = self
            .rules
            .kind(kind)
            .ok_or(RecruitError::InvalidState("recruit kind"))?;
        let _next = self
            .next_unit_id
            .checked_add(1)
            .ok_or(RecruitError::InvalidState("unit ID capacity exhausted"))?;
        if self.combat.get(*self.next_unit_id).is_some()
            || self.recruit_metadata.contains_key(&*self.next_unit_id)
            || self.commands.units.contains_key(&*self.next_unit_id)
            || self.firing.get(*self.next_unit_id).is_some()
            || self.abilities.get(*self.next_unit_id).is_some()
            || self.recovery.get(*self.next_unit_id).is_some()
        {
            return Err(RecruitError::InvalidState(
                "recruit metadata ID already exists",
            ));
        }
        let catalog = Catalog::compiled().map_err(|e| RecruitError::Admission(e.into()))?;
        let mut view = terrain_view(game);
        view.versions = self.versions;
        let placed = if let Some((maker, building_rally)) = maker {
            self.produced_placer
                .produce(
                    self.rules,
                    catalog,
                    view,
                    &game.units,
                    ProduceInput {
                        id: *self.next_unit_id,
                        owner: slot,
                        kind: kind_index,
                        scatter_index: u32::try_from(game.units.len())
                            .map_err(|_| RecruitError::InvalidState("unit count"))?,
                        player_spawn: self.start.players[slot].spawn,
                        player_rally: self.player_rallies[slot],
                        building: ProductionBuilding {
                            at: maker.at(),
                            kind: maker.kind,
                            rally: building_rally,
                        },
                        mode: self.options.mode,
                        tick: game.tick as u64,
                    },
                )
                .map_err(RecruitError::Placement)?
        } else {
            self.produced_placer
                .produce_tutorial(
                    self.rules,
                    catalog,
                    view,
                    &game.units,
                    crate::produced_units::TutorialProduceInput {
                        id: *self.next_unit_id,
                        owner: slot,
                        kind: kind_index,
                        scatter_index: u32::try_from(game.units.len())
                            .map_err(|_| RecruitError::InvalidState("unit count"))?,
                        home: self.start.players[slot].spawn,
                        rally: self.player_rallies[slot],
                        tick: game.tick as u64,
                    },
                )
                .map_err(RecruitError::Placement)?
        };
        let request = prepared.request().clone();
        drop(prepared);
        // Preflight canonical unit/allocator/control publication before reading routing.
        let control = crate::match_commands::UnitControl::new(&placed.unit, self.rules)
            .map_err(|_| RecruitError::InvalidState("unit control metadata"))?;
        drop(
            crate::unit_publication::prepare_publication(
                crate::unit_publication::PublicationParts {
                    units: &mut self.game.units,
                    grid: self.grid,
                    controls: &mut self.commands.units,
                    recruitment: self.recruit_metadata,
                    next_id: self.next_unit_id,
                },
                self.rules,
                count,
                vec![crate::unit_publication::StagedUnit {
                    unit: placed.unit.clone(),
                    control,
                    recruitment: None,
                }],
            )
            .map_err(RecruitError::Publication)?,
        );
        Ok(StagedRecruit {
            slot,
            kind: kind.into(),
            from,
            winner_decided,
            request,
            kind_index,
            placed,
            route_end: None,
            route_status: None,
        })
    }
    fn finish(&mut self, staged: StagedRecruit) -> Result<CompletedRecruitUnit, RecruitError> {
        let StagedRecruit {
            slot,
            kind,
            from,
            winner_decided,
            request: admitted,
            placed,
            route_end,
            route_status,
            ..
        } = staged;
        let game = &*self.game;
        let facts = admission_facts(
            self.game,
            self.start,
            self.options,
            self.rules,
            self.player_rallies,
        )?;
        let count = facts.count;
        let context = facts.context(self.options, game.naval);
        let prepared = if self.options.mode == crate::economy::Mode::Tutorial {
            self.production.prepare_tutorial_recruit(
                &context,
                &mut self.start.economy,
                slot,
                self.start.players[slot].config.faction,
                &kind,
            )?
        } else {
            self.production.prepare_recruit(
                &context,
                &mut self.start.economy,
                slot,
                self.start.players[slot].config.faction,
                &kind,
                from,
            )?
        };
        let request = prepared.request();
        if request != &admitted {
            return Err(RecruitError::InvalidState(
                "purchase facts changed during routing preparation",
            ));
        }
        let mut control = crate::match_commands::UnitControl::new(&placed.unit, self.rules)
            .map_err(|_| RecruitError::InvalidState("unit control metadata"))?;
        control.aim = placed.aim;
        control.rally_drift = placed.rally.is_some();
        control.route_end = route_end;
        let metadata = RecruitMetadata {
            building: request.building,
            initial_rally: placed.rally.map(|r| r.kind),
            initial_route_status: route_status,
        };
        let id = placed.unit.id;
        let publication = crate::unit_publication::prepare_publication(
            crate::unit_publication::PublicationParts {
                units: &mut self.game.units,
                grid: self.grid,
                controls: &mut self.commands.units,
                recruitment: self.recruit_metadata,
                next_id: self.next_unit_id,
            },
            self.rules,
            count,
            vec![crate::unit_publication::StagedUnit {
                unit: placed.unit,
                control,
                recruitment: Some(metadata),
            }],
        )
        .map_err(RecruitError::Publication)?;
        let story = crate::match_story::prepare_story_updates(
            &mut *self.story,
            winner_decided,
            &[crate::match_story::StoryChange {
                slot: Some(slot),
                key: crate::match_story::StoryKey::MpSpent,
                amount: request.spend.mp,
            }],
        )
        .map_err(RecruitError::Story)?;
        let purchase = prepared.commit();
        let _published = publication.publish_with(|unit, _| {
            self.combat.initialize(unit.id);
            self.abilities.initialize(unit.id);
            self.recovery.initialize(unit.id);
            self.firing.initialize(unit.id);
        });
        story.commit();
        Ok(CompletedRecruitUnit {
            unit: id,
            purchase,
            route_status,
        })
    }
}

fn route_recruit(
    game: &crate::game::Game,
    rules: &crate::rules::Rules,
    paths: &mut crate::pathfinding::Pathfinding,
    staged: &mut StagedRecruit,
    routing: Option<RecruitRouting<'_>>,
) -> Result<(), RecruitError> {
    let mut route_end = None;
    let mut route_status = None;
    if let Some(rally) = staged.placed.rally {
        let routing = routing.ok_or(RecruitError::InvalidState(
            "rally routing recipient missing",
        ))?;
        let nav = routing.query.navigation();
        if routing.slot != staged.slot
            || routing.team != game.players[staged.slot].team
            || nav.width != game.w
            || nav.height != game.h
            || nav.world_known
        {
            return Err(RecruitError::InvalidState(
                "rally routing recipient mismatch",
            ));
        }
        let path = paths
            .find_path(
                routing
                    .query
                    .path_terrain(routing.live)
                    .map_err(RecruitError::Knowledge)?,
                PathRules::compiled().map_err(|e| RecruitError::Admission(e.into()))?,
                &rules.kinds[staged.kind_index],
                staged.placed.unit.at(),
                rally.destination,
                None,
            )
            .map_err(RecruitError::Path)?;
        staged.placed.unit.path = path.waypoints;
        staged.placed.unit.path_gen = 1;
        route_end = Some(path.route_end);
        route_status = Some(path.status);
    }
    staged.route_end = route_end;
    staged.route_status = route_status;
    Ok(())
}

#[cfg(test)]
#[path = "match_recruitment_tests.rs"]
mod tests;
