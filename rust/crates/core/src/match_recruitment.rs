//! Atomic Conquest purchase, produced-unit placement and owner-observed rally routing.
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
impl Match {
    /// The typed buy actuator. A winner, away or eliminated seat is blocked.
    /// Routing is required only when produceUnit emits a rally request. No wallet,
    /// ID, unit or grid changes occur until placement and route input checks pass.
    /// Unreachable or deferred routes still create the purchased unit with an empty
    /// path, goal and rally drift retained, matching the source's immediate search.
    pub fn recruit(
        &mut self,
        slot: usize,
        kind: &str,
        from: Option<u32>,
        winner_decided: bool,
        routing: Option<RecruitRouting<'_>>,
    ) -> Result<CompletedRecruitUnit, RecruitError> {
        let Some(wallet) = self.start.economy.players.get(slot) else {
            return Err(production::Error::Denied(Denial::Blocked).into());
        };
        if winner_decided || wallet.away || wallet.out {
            return Err(production::Error::Denied(Denial::Blocked).into());
        }
        let game = &self.world.game;
        let count = self.start.players.len();
        if count != game.players.len()
            || count != self.start.economy.players.len()
            || self.player_rallies.len() != count
            || game.tick < 0
            || game.mode.is_some()
            || self.options.logistics
        {
            return Err(RecruitError::InvalidState("Conquest recruitment state"));
        }
        for (i, (player, start)) in game.players.iter().zip(&self.start.players).enumerate() {
            if player.slot != i
                || player.team != start.config.team
                || player.team != self.start.economy.players[i].team
            {
                return Err(RecruitError::InvalidState("recruitment player identity"));
            }
        }
        if game
            .strikes
            .iter()
            .any(|s| s.kind == StrikeKind::Other && !s.live)
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
            let profile = self
                .rules
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
            .filter(|s| s.kind == StrikeKind::Recon)
            .map(|s| PendingDrop {
                owner: s.owner,
                kind: "recon",
                live: s.live,
            })
            .collect();
        let context = Context {
            mode: ModeContext {
                kind: self.options.mode,
                ..Default::default()
            },
            army: self.options.army,
            tech_enabled: self.options.tech,
            naval: game.naval,
            sudden_death: false,
            units: &units,
            drops: &drops,
            world: None,
        };
        let prepared = self.production.prepare_recruit(
            &context,
            &mut self.start.economy,
            slot,
            self.start.players[slot].config.faction,
            kind,
            from,
        )?;
        let request = prepared.request();
        let maker = game
            .units
            .get(&request.building)
            .ok_or(RecruitError::InvalidState("admitted maker missing"))?;
        let building_rally =
            *self
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
        let kind_index = self
            .rules
            .kind(kind)
            .ok_or(RecruitError::InvalidState("recruit kind"))?;
        let next = self
            .next_unit_id
            .checked_add(1)
            .ok_or(RecruitError::InvalidState("unit ID capacity exhausted"))?;
        if self.recruit_metadata.contains_key(&self.next_unit_id)
            || self.commands.units.contains_key(&self.next_unit_id)
            || self.firing.get(self.next_unit_id).is_some()
        {
            return Err(RecruitError::InvalidState(
                "recruit metadata ID already exists",
            ));
        }
        let catalog = Catalog::compiled().map_err(|e| RecruitError::Admission(e.into()))?;
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
                catalog,
                view,
                &game.units,
                ProduceInput {
                    id: self.next_unit_id,
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
            .map_err(RecruitError::Placement)?;
        let mut route_end = None;
        let mut route_status = None;
        if let Some(rally) = placed.rally {
            let routing = routing.ok_or(RecruitError::InvalidState(
                "rally routing recipient missing",
            ))?;
            let nav = routing.query.navigation();
            if routing.slot != slot
                || routing.team != game.players[slot].team
                || nav.width != game.w
                || nav.height != game.h
                || nav.world_known
            {
                return Err(RecruitError::InvalidState(
                    "rally routing recipient mismatch",
                ));
            }
            let path = self
                .commands
                .paths
                .find_path(
                    routing
                        .query
                        .path_terrain(routing.live)
                        .map_err(RecruitError::Knowledge)?,
                    PathRules::compiled().map_err(|e| RecruitError::Admission(e.into()))?,
                    &self.rules.kinds[kind_index],
                    placed.unit.at(),
                    rally.destination,
                    None,
                )
                .map_err(RecruitError::Path)?;
            placed.unit.path = path.waypoints;
            placed.unit.path_gen = 1;
            route_end = Some(path.route_end);
            route_status = Some(path.status);
        }
        let mut control = crate::match_commands::UnitControl::new(&placed.unit, &self.rules)
            .map_err(|_| RecruitError::InvalidState("unit control metadata"))?;
        control.aim = placed.aim;
        control.rally_drift = placed.rally.is_some();
        control.route_end = route_end;
        let metadata = RecruitMetadata {
            building: request.building,
            initial_rally: placed.rally.map(|r| r.kind),
            initial_route_status: route_status,
        };
        let purchase = prepared.commit();
        let id = placed.unit.id;
        // Source spawnUnit inserts once and placeUnit then moves the same grid
        // record. Staged insertion directly at its final pose retains that order.
        self.world.game.units.insert(id, placed.unit);
        self.grid.update(id, &self.world.game.units[&id]);
        self.commands.insert(id, control);
        self.combat.initialize(id);
        self.firing.initialize(id);
        self.recruit_metadata.insert(id, metadata);
        self.next_unit_id = next;
        Ok(CompletedRecruitUnit {
            unit: id,
            purchase,
            route_status,
        })
    }
}
