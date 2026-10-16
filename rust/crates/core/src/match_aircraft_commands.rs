//! Prepared aircraft branches for the single canonical recorded command dispatcher.
//! No ground destination, route or recipient-memory query belongs to these branches.
use crate::{
    game::{PlaneState, Point},
    grid::UnitId,
    match_aviation::{AviationError, MissionKind, PlaneMission},
    match_commands::MoveOutcome,
    match_runtime::Match,
};
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum AircraftOrder {
    Move(Point),
    Area(Point),
    Attack(UnitId),
    Stop,
    Escort(UnitId),
    Retreat,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct PreparedAircraftOrder {
    id: UnitId,
    order: AircraftOrder,
    mission: Option<PlaneMission>,
    send: bool,
    home: bool,
}
impl Match {
    pub(crate) fn aircraft_queue_ready(&self, id: UnitId) -> Result<bool, String> {
        let u = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing queued aircraft")?;
        let state = u.air.ok_or("queued unit is not aircraft")?;
        let sortie = self
            .aviation
            .planes
            .get(&id)
            .ok_or("missing queued sortie")?;
        if state == PlaneState::Rearm || sortie.fuel <= 0.0 || sortie.ammo <= 0.0 {
            return Ok(false);
        }
        Ok(!matches!(
            state,
            PlaneState::Out | PlaneState::Station | PlaneState::Home
        ) || sortie.mission.is_none())
    }
    pub(crate) fn is_aircraft_command_unit(&self, id: UnitId) -> bool {
        let u = &self.world.game.units[&id];
        u.air.is_some() || self.rules.kinds.get(u.kind).is_some_and(|k| k.air)
    }

    /// The dispatcher prepares every selected aircraft before selection or supply writes.
    pub(crate) fn prepare_aircraft_order(
        &self,
        owner: usize,
        id: UnitId,
        order: AircraftOrder,
    ) -> Result<PreparedAircraftOrder, AviationError> {
        self.validate_aviation()?;
        let u = self
            .world
            .game
            .units
            .get(&id)
            .ok_or(AviationError::Invalid("missing command aircraft"))?;
        if u.owner != owner || u.air.is_none() {
            return Err(AviationError::Invalid("invalid aircraft command selection"));
        }
        let active = matches!(
            u.air,
            Some(PlaneState::Out | PlaneState::Station | PlaneState::Home)
        );
        let mission = match order {
            AircraftOrder::Move(at) | AircraftOrder::Area(at) => Some(PlaneMission {
                kind: if matches!(order, AircraftOrder::Area(_)) {
                    MissionKind::Area
                } else {
                    MissionKind::Patrol
                },
                target: None,
                at,
            }),
            AircraftOrder::Attack(target) | AircraftOrder::Escort(target) => {
                let t = self
                    .world
                    .game
                    .units
                    .get(&target)
                    .ok_or(AviationError::Invalid("missing aircraft mission target"))?;
                Some(PlaneMission {
                    kind: if matches!(order, AircraftOrder::Attack(_)) {
                        MissionKind::Attack
                    } else {
                        MissionKind::Escort
                    },
                    target: Some(target),
                    at: t.at(),
                })
            }
            AircraftOrder::Stop if active => Some(PlaneMission {
                kind: MissionKind::Patrol,
                target: None,
                at: u.at(),
            }),
            AircraftOrder::Stop | AircraftOrder::Retreat => None,
        };
        if let Some(m) = mission
            && matches!(order, AircraftOrder::Attack(_))
        {
            let t = &self.world.game.units[&m.target.expect("attack target")];
            if t.hp <= 0.0
                || self.world.game.allied(owner, t.owner)
                || !self.world.game.players[owner].sees(t.id)
            {
                return Err(AviationError::Invalid("invalid aircraft attack target"));
            }
        }
        let send = mission
            .map(|m| self.validate_aircraft_mission(owner, id, m))
            .transpose()?
            .unwrap_or(false);
        Ok(PreparedAircraftOrder {
            id,
            order,
            mission,
            send,
            home: matches!(order, AircraftOrder::Retreat) && active,
        })
    }
    /// No fallible work remains after whole-selection preflight on the command boundary.
    pub(crate) fn apply_aircraft_order(&mut self, prepared: PreparedAircraftOrder) {
        let id = prepared.id;
        let u = self
            .world
            .game
            .units
            .get_mut(&id)
            .expect("prepared aircraft");
        let c = self.commands.units.get_mut(&id).expect("prepared control");
        c.follow = 0;
        c.orders.clear();
        if matches!(
            prepared.order,
            AircraftOrder::Move(_)
                | AircraftOrder::Area(_)
                | AircraftOrder::Attack(_)
                | AircraftOrder::Retreat
        ) {
            u.traffic = None;
            u.traffic_wait = 0.0;
            u.world_goal = None;
            u.entrench = false;
            c.face = None;
            c.outcome = Some(MoveOutcome::Interrupted);
            c.outcome_tick = Some(self.world.game.tick);
        }
        if matches!(prepared.order, AircraftOrder::Move(_))
            && let Some(r) = self.logistics.get_mut(id).filter(|r| r.forced)
        {
            c.supply_escape = true;
            r.stranded = true;
        }
        if prepared.send {
            self.aviation
                .planes
                .get_mut(&id)
                .expect("prepared sortie")
                .mission = prepared.mission;
            u.air = Some(PlaneState::Out);
        } else if prepared.home {
            u.air = Some(PlaneState::Home);
        }
    }
}
