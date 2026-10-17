//! Recipient projection of one caller-owned chronological ground shot batch.
use super::*;
use crate::{
    environment::Effect,
    game::Point,
    grid::SpatialGrid,
    legacy_scenario::{SayAction, validate_public_say},
    match_firing::LaunchEvent,
    match_support::SupportEvent,
    match_tick::TickEvent,
    projectiles::{self, BodyPosition, Box3, Flight, Point3},
};

/// Borrow existing producer payloads in their actual emission order. This view
/// owns no events and does not drain, acknowledge or reorder a producer.
#[derive(Clone, Copy, Debug)]
pub enum ShotRef<'a> {
    Firing(&'a LaunchEvent),
    Support(&'a SupportEvent),
    Environment(&'a Effect),
    Scenario(&'a SayAction),
    AuthoredMission(&'a Value),
}

impl<'a> TryFrom<&'a TickEvent> for ShotRef<'a> {
    type Error = SnapshotError;

    fn try_from(value: &'a TickEvent) -> Result<Self, Self::Error> {
        match value {
            TickEvent::Firing(value) => Ok(Self::Firing(value)),
            TickEvent::Support(value) => Ok(Self::Support(value)),
            TickEvent::Environment(value) => Ok(Self::Environment(value)),
            TickEvent::Scenario(value) => Ok(Self::Scenario(value)),
            TickEvent::AuthoredMission(value) => Ok(Self::AuthoredMission(value)),
            TickEvent::Aviation(_) => Err(SnapshotError::Unsupported {
                unit: 0,
                capability: SnapshotCapability::Air,
            }),
        }
    }
}

pub struct SelectedGroundShotFrame<'a> {
    frame: SelectedGroundListFrame<'a>,
    shots: Vec<(usize, Value)>,
}

impl DeliveryState {
    /// Borrow the complete caller-owned tick batch in its actual order. The host
    /// owns accumulation and scheduled resets, including ticks with no recipients.
    /// An unsupported row refuses before any selected delivery owner publishes.
    pub fn capture_selected_with_tick_events<'a>(
        &mut self,
        state: &'a mut Match,
        selected: &[SelectedSeat],
        events: &[TickEvent],
    ) -> Result<Option<SelectedGroundShotFrame<'a>>, SnapshotError> {
        if selected.is_empty() {
            return Ok(None);
        }
        let shots = events
            .iter()
            .map(ShotRef::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        self.capture_selected_with_shots(state, selected, &shots)
    }

    /// Project the complete supplied batch before any selected memory owner
    /// publishes. The host retains the scheduled global batch-reset policy.
    /// Nonsandbox retained flights, unsupported AIR/debris actors and undrained
    /// producer buffers refuse through complete-frame admission.
    pub fn capture_selected_with_shots<'a>(
        &mut self,
        state: &'a mut Match,
        selected: &[SelectedSeat],
        shots: &[ShotRef<'_>],
    ) -> Result<Option<SelectedGroundShotFrame<'a>>, SnapshotError> {
        if selected.is_empty() {
            return Ok(None);
        }
        let rows = self.preflight_selected(state, selected)?;
        composition::admit_actors(state, &[])?;
        let projected = project_shots(state, selected, shots)?;
        let frame = self
            .capture_selected_with_lists_admitted(state, selected, &[], rows)?
            .ok_or(SnapshotError::InvalidRecipient)?;
        Ok(Some(SelectedGroundShotFrame {
            frame,
            shots: projected,
        }))
    }
}

impl SelectedGroundShotFrame<'_> {
    pub fn slots(&self) -> impl Iterator<Item = usize> + '_ {
        self.frame.slots()
    }

    pub fn recipient(&self, slot: usize) -> Result<SupportedSeatSnapshot, SnapshotError> {
        let shots = self
            .shots
            .iter()
            .find(|(owner, _)| *owner == slot)
            .ok_or(SnapshotError::InvalidRecipient)?;
        Ok(self.frame.recipient(slot)?.with_shots(shots.1.clone()))
    }
}

fn refused() -> SnapshotError {
    SnapshotError::Unsupported {
        unit: 0,
        capability: SnapshotCapability::ShotsAndFlights,
    }
}

fn numbers(values: &[f64]) -> Result<(), SnapshotError> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(SnapshotError::InvalidNumber)
    }
}

fn point(at: Point3) -> Result<(), SnapshotError> {
    numbers(&[at.x, at.y, at.z])
}

fn owner(state: &Match, value: usize) -> Result<(), SnapshotError> {
    if value < state.world.game.players.len() {
        Ok(())
    } else {
        Err(SnapshotError::Rules)
    }
}

fn identity(value: u64) -> Result<(), SnapshotError> {
    if (1..=9_007_199_254_740_991).contains(&value) {
        Ok(())
    } else {
        Err(SnapshotError::Rules)
    }
}

fn kind(state: &Match, value: usize) -> Result<(), SnapshotError> {
    let kind = state.rules.kinds.get(value).ok_or(SnapshotError::Rules)?;
    if kind.air {
        Err(SnapshotError::Unsupported {
            unit: 0,
            capability: SnapshotCapability::Air,
        })
    } else {
        Ok(())
    }
}

fn effect(state: &Match, value: &Effect) -> Result<Value, SnapshotError> {
    match value {
        Effect::Hurt(h) => {
            identity(u64::from(h.unit))?;
            owner(state, h.owner)?;
            numbers(&[h.at.x, h.at.z])?;
            if h.from.is_some_and(|from| from < -1) {
                return Err(SnapshotError::Rules);
            }
            if let Some(from) = h.from.and_then(|from| usize::try_from(from).ok()) {
                owner(state, from)?;
            }
        }
        Effect::Collapse(c) => {
            numbers(&[c.at, c.position.x, c.position.z])?;
            if let Some(impulse) = c.impulse {
                numbers(&[impulse])?;
            }
            if let Some(direction) = c.direction {
                numbers(&[direction])?;
            }
            if let Some(entity) = c.entity {
                identity(u64::from(entity))?;
            }
            if let Some(value) = c.owner {
                owner(state, value)?;
            }
        }
        Effect::BuildingCollapse {
            unit,
            owner: value,
            position,
        } => {
            identity(u64::from(*unit))?;
            owner(state, *value)?;
            numbers(&[position.x, position.z])?;
        }
        // Canonical damage consumers apply this metadata before returning shots.
        Effect::LastHit { .. } => return Err(refused()),
    }
    Ok(value.to_json())
}

fn event(state: &Match, shot: ShotRef<'_>) -> Result<Value, SnapshotError> {
    let mut row = raw_event(state, shot)?;
    crate::match_firing::normalize_json_zero(&mut row);
    Ok(row)
}

fn raw_event(state: &Match, shot: ShotRef<'_>) -> Result<Value, SnapshotError> {
    match shot {
        ShotRef::AuthoredMission(value) => {
            let mission = state
                .authored_mission
                .as_ref()
                .ok_or(SnapshotError::Rules)?;
            if !mission.accepts_shot(value)
                || value["atTick"]
                    .as_i64()
                    .is_none_or(|tick| tick > state.world.game.tick)
            {
                return Err(SnapshotError::Rules);
            }
            Ok(value.clone())
        }
        ShotRef::Scenario(value) => {
            validate_public_say(value).map_err(|_| SnapshotError::Rules)?;
            if state.scenario.is_none()
                || state.world.game.tick < 0
                || value.at_tick > state.world.game.tick as u64
            {
                return Err(SnapshotError::Rules);
            }
            Ok(value.shot_json())
        }
        ShotRef::Environment(value) => effect(state, value),
        ShotRef::Support(SupportEvent::Environment(value)) => effect(state, value),
        ShotRef::Support(
            SupportEvent::Bomb(_)
            | SupportEvent::Chutes(_)
            | SupportEvent::Plane { .. }
            | SupportEvent::Flak { .. }
            | SupportEvent::ShotDown { .. },
        ) => Err(SnapshotError::Unsupported {
            unit: 0,
            capability: SnapshotCapability::Air,
        }),
        ShotRef::Support(SupportEvent::Shell(at) | SupportEvent::SmokeShells(at)) => {
            numbers(&[at.x, at.z])?;
            let name = if matches!(shot, ShotRef::Support(SupportEvent::Shell(_))) {
                "shell"
            } else {
                "smokeshells"
            };
            Ok(json!({"k":name,"x":at.x,"z":at.z,"pub":true}))
        }
        ShotRef::Firing(value) => {
            match value {
                LaunchEvent::Environment(value) => return effect(state, value),
                LaunchEvent::Flight {
                    id,
                    kind: value,
                    shooter,
                    owner: seat,
                    at,
                    velocity,
                    gravity,
                    time,
                    expires,
                    ..
                } => {
                    identity(*id)?;
                    identity(u64::from(*shooter))?;
                    kind(state, *value)?;
                    owner(state, *seat)?;
                    point(*at)?;
                    point(*velocity)?;
                    numbers(&[*gravity, *time, *expires])?;
                    if *time < 0.0 || *expires < *time {
                        return Err(SnapshotError::Rules);
                    }
                }
                LaunchEvent::Salvo {
                    shooter,
                    owner: seat,
                    at,
                    spread,
                    ..
                } => {
                    identity(u64::from(*shooter))?;
                    owner(state, *seat)?;
                    numbers(&[at.x, at.z, *spread])?;
                }
                LaunchEvent::Breath {
                    shooter,
                    target,
                    owner: seat,
                    target_owner,
                    at,
                } => {
                    if !state.options.sandbox {
                        return Err(refused());
                    }
                    identity(u64::from(*shooter))?;
                    identity(u64::from(*target))?;
                    owner(state, *seat)?;
                    owner(state, *target_owner)?;
                    numbers(&[at.x, at.z])?;
                }
                LaunchEvent::BreathShell { at } => {
                    if !state.options.sandbox {
                        return Err(refused());
                    }
                    numbers(&[at.x, at.z])?;
                }
                LaunchEvent::Throw {
                    shooter,
                    owner: seat,
                    at,
                } => {
                    if !state.options.sandbox {
                        return Err(refused());
                    }
                    identity(u64::from(*shooter))?;
                    owner(state, *seat)?;
                    numbers(&[at.x, at.z])?;
                }
                LaunchEvent::SmallArms(_) => return Err(refused()),
                LaunchEvent::Contact(value) => {
                    kind(state, value.kind)?;
                    identity(value.flight)?;
                    identity(u64::from(value.shooter))?;
                    owner(state, value.owner)?;
                    point(value.at)?;
                    point(value.normal)?;
                    point(value.direction)?;
                    numbers(&[value.time, value.impulse])?;
                    if let Some(blast) = value.blast {
                        numbers(&[blast])?;
                    }
                    if let Some(target) = value.target {
                        identity(u64::from(target))?;
                    }
                    if let Some(value) = value.target_owner {
                        owner(state, value)?;
                    }
                }
                LaunchEvent::Expiry { id, time, at, .. } => {
                    identity(*id)?;
                    numbers(&[*time])?;
                    point(*at)?;
                }
            }
            Ok(value.to_json(&state.rules))
        }
    }
}

fn seen(state: &Match, slot: usize, row: &Value, key: &str) -> bool {
    row.get(key)
        .and_then(Value::as_u64)
        .and_then(|id| u32::try_from(id).ok())
        .is_some_and(|id| super::seen_by(state, slot, id))
}

fn allied(state: &Match, slot: usize, row: &Value, key: &str) -> bool {
    row.get(key)
        .and_then(Value::as_u64)
        .and_then(|owner| usize::try_from(owner).ok())
        .is_some_and(|owner| state.world.game.allied(owner, slot))
}

fn flight_visible(state: &Match, visible: &[u8], at: Point) -> bool {
    state.world.game.reveal
        || state
            .world
            .game
            .cell_of(at.x, at.z)
            .is_some_and(|cell| visible[cell] != 0)
}

fn until(state: &Match, visible: &[u8], shot: &Value) -> Result<f64, SnapshotError> {
    let value = |key: &str| shot[key].as_f64().ok_or(SnapshotError::Rules);
    let time = value("time")?;
    let expires = value("expires")?;
    let velocity = shot["v"].as_array().ok_or(SnapshotError::Rules)?;
    let p = Flight {
        origin: Point3::new(value("x")?, value("y")?, value("z")?),
        velocity: Point3::new(
            velocity[0].as_f64().ok_or(SnapshotError::Rules)?,
            velocity[1].as_f64().ok_or(SnapshotError::Rules)?,
            velocity[2].as_f64().ok_or(SnapshotError::Rules)?,
        ),
        gravity: value("gravity")?,
        stamp: time,
    };
    let end = math::min(expires, time + 0.15);
    numbers(&[end])?;
    let a = projectiles::flight_at(p, time).map_err(|_| SnapshotError::Rules)?;
    let b = projectiles::flight_at(p, end).map_err(|_| SnapshotError::Rules)?;
    let g = &state.world.game;
    let crossed = projectiles::crossed_cells(
        BodyPosition { x: a.x, z: a.z },
        BodyPosition { x: b.x, z: b.z },
        crate::game::CELL,
        g.w,
        g.h,
    )
    .map_err(|_| SnapshotError::Rules)?;
    let mut until = end;
    for cell in crossed {
        let x = (cell % g.w) as f64 * crate::game::CELL;
        let z = (cell / g.w) as f64 * crate::game::CELL;
        if flight_visible(
            state,
            visible,
            Point {
                x: x + crate::game::CELL / 2.0,
                z: z + crate::game::CELL / 2.0,
            },
        ) {
            continue;
        }
        // Source's visibility box has an unbounded vertical interval. Flatten
        // only this query, keeping the actual flight trajectory unchanged.
        if let Some(hit) = projectiles::sweep_box(
            Point3::new(a.x, 0.0, a.z),
            Point3::new(b.x, 0.0, b.z),
            Box3 {
                x: [x, x + crate::game::CELL],
                y: [0.0, 0.0],
                z: [z, z + crate::game::CELL],
            },
        )
        .map_err(|_| SnapshotError::Rules)?
        {
            until = math::min(until, time + (end - time) * hit.fraction);
        }
    }
    // flightEventFor reconstructs no weapon/contact profile. Its extrapolation
    // uses only this fog clipping; live flights require their separate adapter.
    Ok(until)
}

/// Prototype flightsFor: live state is projected from current time, independent of shot history.
pub(super) fn live_flights(state: &Match, slot: usize) -> Result<Value, SnapshotError> {
    if !state.options.sandbox {
        return Err(refused());
    }
    state
        .validate_firing_tick_state()
        .map_err(|_| SnapshotError::Rules)?;
    let g = &state.world.game;
    let player = g.players.get(slot).ok_or(SnapshotError::InvalidRecipient)?;
    state
        .routing_fog
        .validate_shape(g)
        .map_err(|_| SnapshotError::Rules)?;
    let mut fog = state.routing_fog.clone();
    let visible = &fog.team(g, &state.rules, player.team).vis;
    let time = g.tick as f64 * 0.05;
    let mut rows = Vec::new();
    for p in state.firing.flights.values() {
        if p.launched > time {
            continue;
        }
        kind(state, p.source_kind)?;
        let at = projectiles::flight_at(p.solution, time).map_err(|_| SnapshotError::Rules)?;
        if !flight_visible(state, visible, Point { x: at.x, z: at.z }) {
            continue;
        }
        let v = projectiles::velocity_at(p.solution, time).map_err(|_| SnapshotError::Rules)?;
        let mut row = json!({
            "flight":p.id,"kind":state.rules.kinds[p.source_kind].name,"seq":p.sequence,
            "x":at.x,"y":at.y,"z":at.z,"v":[v.x,v.y,v.z],"gravity":p.solution.gravity,
            "time":time,"launched":p.launched,"expires":p.expires
        });
        if let Some(effect) = p.effect {
            row["effect"] = json!(effect);
        }
        let mut end = until(state, visible, &row)?;
        if state.rules.kinds[p.source_kind].weapon && end > time {
            let b = projectiles::flight_at(p.solution, end).map_err(|_| SnapshotError::Rules)?;
            if let Some(fraction) = state
                .recipient_flight_contact(p, at, b, |id| seen_by(state, slot, id))
                .map_err(|_| SnapshotError::Rules)?
            {
                end = time + (end - time) * fraction;
            }
        }
        row["until"] = json!(end);
        if seen_by(state, slot, p.shooter) {
            row["f"] = json!(p.shooter);
            row["fo"] = crate::game::source_owner_json(p.owner);
        }
        rows.push(row);
    }
    Ok(Value::Array(rows))
}

fn project_shots(
    state: &Match,
    selected: &[SelectedSeat],
    shots: &[ShotRef<'_>],
) -> Result<Vec<(usize, Value)>, SnapshotError> {
    project_shots_perspective(state, selected, shots, false)
}

pub(super) fn project_shots_perspective(
    state: &Match,
    selected: &[SelectedSeat],
    shots: &[ShotRef<'_>],
    full_map: bool,
) -> Result<Vec<(usize, Value)>, SnapshotError> {
    let events = shots
        .iter()
        .copied()
        .map(|shot| {
            event(state, shot).map(|row| {
                (
                    matches!(shot, ShotRef::Scenario(_) | ShotRef::AuthoredMission(_)),
                    row,
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    state
        .routing_fog
        .validate_shape(&state.world.game)
        .map_err(|_| SnapshotError::Rules)?;
    // This temporary component cache prepares the exact current Source mask.
    // It never publishes fog versions, exploration or delivery acknowledgments.
    let mut fog = state.routing_fog.clone();
    let g = &state.world.game;
    let grid = SpatialGrid::new(&g.units, 16.0);
    let mut projections = Vec::with_capacity(selected.len());
    for seat in selected {
        let visible = &fog.team(g, &state.rules, g.players[seat.slot].team).vis;
        let mut rows = Vec::new();
        for (public_say, event) in &events {
            // Mission messages have recipient policies instead of ground geometry.
            if *public_say {
                let policy = event["scenarioRecipients"]
                    .as_str()
                    .ok_or(SnapshotError::Rules)?;
                let side = event["scenarioSide"].as_u64().ok_or(SnapshotError::Rules)? as usize;
                let allowed = policy == "public"
                    || (!full_map
                        && match policy {
                            "side" => side == seat.slot,
                            "team" => g
                                .players
                                .get(side)
                                .is_some_and(|p| p.team == g.players[seat.slot].team),
                            _ => false,
                        });
                if allowed {
                    rows.push(event.clone());
                }
                continue;
            }
            let at = Point {
                x: event["x"].as_f64().ok_or(SnapshotError::Rules)?,
                z: event["z"].as_f64().ok_or(SnapshotError::Rules)?,
            };
            let local = event["local"] == true;
            let admitted = if local {
                full_map || flight_visible(state, visible, at)
            } else {
                full_map
                    || g.reveal
                    || vision::team_sees(g, &state.rules, &grid, g.players[seat.slot].team, at)
                    || event["pub"] == true
                    || allied(state, seat.slot, event, "fo")
                    || allied(state, seat.slot, event, "to")
                    || seen(state, seat.slot, event, "f")
                    || seen(state, seat.slot, event, "t")
            };
            if !admitted {
                continue;
            }
            let mut row = event.clone();
            let object = row.as_object_mut().ok_or(SnapshotError::Rules)?;
            for (id, owner) in [("f", "fo"), ("t", "to")] {
                // watchGame supplies EVERYTHING.has, including IDs whose units died.
                let visible_identity = full_map || seen(state, seat.slot, event, id);
                if !visible_identity {
                    object.remove(id);
                    if local {
                        object.remove(owner);
                    }
                }
            }
            if local && event["k"] == "flight" {
                let until = if full_map {
                    math::min(
                        event["expires"].as_f64().ok_or(SnapshotError::Rules)?,
                        event["time"].as_f64().ok_or(SnapshotError::Rules)? + 0.15,
                    )
                } else {
                    until(state, visible, event)?
                };
                object.insert("until".into(), json!(until));
            }
            rows.push(row);
        }
        projections.push((seat.slot, Value::Array(rows)));
    }
    Ok(projections)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_live_flights_use_current_pose_exact_shape_and_recipient_visibility() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/recipient-delivery.json"))
                .unwrap();
        let mut state = super::super::tests::initialize(&fixture["initial"]);
        state.options.sandbox = true;
        let profile = crate::match_firing::AbilityBlast::from_unit(
            state.world.game.units[&1].kind,
            &state.rules,
        )
        .unwrap();
        let id = state
            .launch_ability_flight(1, Point { x: 20.0, z: 20.0 }, profile, Some(1.0), None)
            .unwrap();
        let flight = state.firing.flights.get_mut(&id).unwrap();
        flight.solution = Flight {
            origin: Point3::new(73.0, 20.0, 73.0),
            velocity: Point3::new(2.0, 0.0, 0.0),
            gravity: 0.0,
            stamp: 0.0,
        };
        flight.launched = 0.0;
        flight.expires = 1.0;
        flight.sequence = 1;
        flight.max_contacts = 4;
        let mut future = flight.clone();
        future.id = id + 1;
        future.launched = 10.0;
        future.expires = 11.0;
        future.solution.stamp = 10.0;
        state.firing.flights.insert(future.id, future);
        state.world.game.tick = 2;
        let rows = live_flights(&state, 2).unwrap();
        assert_eq!(
            rows,
            json!([{
                "flight":id,"kind":"rifle","effect":"boom","seq":1,
                "x":73.2,"y":20.0,"z":73.0,"v":[2.0,0.0,0.0],"gravity":0.0,
                "time":0.1,"launched":0.0,"expires":1.0,"until":0.25
            }])
        );
        assert_eq!(live_flights(&state, 0).unwrap(), json!([]));
        state.world.game.reveal = true;
        let mut expected = rows;
        expected[0]["f"] = json!(1);
        expected[0]["fo"] = json!(0);
        assert_eq!(live_flights(&state, 0).unwrap(), expected);
        let projection = SnapshotCache::new(&state).unwrap().recipient(0).unwrap();
        assert_eq!(
            composition::complete(projection, None).into_json()["flights"],
            expected
        );
    }

    #[test]
    fn sandbox_live_flight_until_clips_current_visible_body_contact() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/recipient-delivery.json"))
                .unwrap();
        let mut state = super::super::tests::initialize(&fixture["initial"]);
        state.options.sandbox = true;
        state.world.game.reveal = true;
        let profile = crate::match_firing::AbilityBlast::from_unit(
            state.world.game.units[&1].kind,
            &state.rules,
        )
        .unwrap();
        let id = state
            .launch_ability_flight(1, Point { x: 20.0, z: 20.0 }, profile, Some(1.0), None)
            .unwrap();
        let flight = state.firing.flights.get_mut(&id).unwrap();
        flight.solution = Flight {
            origin: Point3::new(50.0, 1.0, 50.0),
            velocity: Point3::new(20.0, 0.0, 0.0),
            gravity: 0.0,
            stamp: 0.0,
        };
        let target = state.world.game.units.get_mut(&7).unwrap();
        target.x = 52.0;
        target.z = 50.0;
        state.grid = SpatialGrid::new(&state.world.game.units, 16.0);
        let rows = live_flights(&state, 0).unwrap();
        let end = rows[0]["until"].as_f64().unwrap();
        assert!(
            end > 0.0 && end < 0.15,
            "visible body clips extrapolation: {end}"
        );
        // The same retained projectile without the body has the full horizon.
        state.world.game.units.get_mut(&7).unwrap().x = 60.0;
        state.grid = SpatialGrid::new(&state.world.game.units, 16.0);
        assert_eq!(live_flights(&state, 0).unwrap()[0]["until"], json!(0.15));
    }

    #[test]
    fn sandbox_throw_uses_local_fog_and_shooter_redaction() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/recipient-delivery.json"))
                .unwrap();
        let mut state = super::super::tests::initialize(&fixture["initial"]);
        state.options.sandbox = true;
        let event = LaunchEvent::Throw {
            shooter: 1,
            owner: 0,
            at: Point { x: 73.0, z: 73.0 },
        };
        let seats = [
            SelectedSeat {
                slot: 2,
                full: false,
                omit: false,
            },
            SelectedSeat {
                slot: 0,
                full: false,
                omit: false,
            },
        ];
        let rows = project_shots(&state, &seats, &[ShotRef::Firing(&event)]).unwrap();
        assert_eq!(
            rows[0].1,
            json!([{"x":73.0,"z":73.0,"k":"throw","physical":true,"local":true}])
        );
        assert_eq!(rows[1].1, json!([]));
    }

    #[test]
    fn sandbox_breath_is_public_but_hidden_actor_ids_are_removed() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/recipient-delivery.json"))
                .unwrap();
        let mut state = super::super::tests::initialize(&fixture["initial"]);
        state.options.sandbox = true;
        let breath = LaunchEvent::Breath {
            shooter: 900001,
            target: 900002,
            owner: 0,
            target_owner: 1,
            at: Point {
                x: -100.0,
                z: -100.0,
            },
        };
        let shell = LaunchEvent::BreathShell {
            at: Point {
                x: -100.0,
                z: -100.0,
            },
        };
        let selected = [
            SelectedSeat {
                slot: 0,
                full: false,
                omit: false,
            },
            SelectedSeat {
                slot: 1,
                full: false,
                omit: false,
            },
        ];
        let shots = [ShotRef::Firing(&breath), ShotRef::Firing(&shell)];
        for (_, rows) in project_shots(&state, &selected, &shots).unwrap() {
            assert_eq!(
                rows,
                json!([
                    {"fo":0,"to":1,"x":-100.0,"z":-100.0,"hit":true,"k":"kaiju","pub":true},
                    {"k":"shell","x":-100.0,"z":-100.0,"pub":true}
                ])
            );
        }
        state.options.sandbox = false;
        assert!(matches!(
            project_shots(&state, &selected, &shots),
            Err(SnapshotError::Unsupported {
                capability: SnapshotCapability::ShotsAndFlights,
                ..
            })
        ));
    }

    #[test]
    fn actual_legacy_say_keeps_public_order_and_rejects_a_later_bad_row() {
        use crate::{
            economy::{Faction, Mode},
            economy_state::ArmySize,
            legacy_scenario_runtime::{LegacyScenarioEmission, step_legacy_scenario_emitting},
            map::AuthoritativeMap,
            match_runtime::{MatchOptions, PlayerConfig},
            weather::WeatherChoice,
        };
        let map = AuthoritativeMap::from_json(include_bytes!(
            "../../../server/src/authoritative-maps/el-alamein.json"
        ))
        .unwrap();
        let mut state = Match::new(
            &map,
            MatchOptions {
                seed: 701,
                mode: Mode::Conquest,
                players: (0..2)
                    .map(|team| PlayerConfig {
                        name: format!("P{team}"),
                        team,
                        faction: Faction::try_from(team as u8).unwrap(),
                    })
                    .collect(),
                shuffle: false,
                army: ArmySize::Standard,
                weather: WeatherChoice::Map,
                weather_seed: Some(1),
                living_weather: false,
                map_key: None,
                tech: false,
                supply: false,
                logistics: false,
                sandbox: false,
            },
        )
        .unwrap();
        state.world.game.tick = 60;
        let mut actions = Vec::new();
        step_legacy_scenario_emitting(&mut state, 60, &mut |event| match event {
            LegacyScenarioEmission::Say(action) => actions.push(action),
            LegacyScenarioEmission::Environment(_) => panic!("the intro trigger has no Destroy"),
        })
        .unwrap();
        assert_eq!(actions.len(), 1);
        let say = &actions[0];
        // Buffered public messages can precede the current snapshot tick.
        state.world.game.tick = 61;
        let shell = SupportEvent::Shell(Point { x: 1.0, z: 1.0 });
        let selected: Vec<_> = (0..state.world.game.players.len())
            .map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect();
        let shots = [
            ShotRef::Scenario(say),
            ShotRef::Support(&shell),
            ShotRef::Scenario(say),
        ];
        let rows = project_shots(&state, &selected, &shots).unwrap();
        assert_eq!(rows.len(), selected.len());
        for (_, rows) in rows {
            assert_eq!(
                rows,
                json!([say.shot_json(), {"k":"shell","x":1.0,"z":1.0,"pub":true}, say.shot_json()])
            );
            assert!(rows[0].get("x").is_none());
        }
        let mut delivery = DeliveryState::new(&state).unwrap();
        let world = format!("{:?}", state.world);
        let recipients = format!("{:?}", state.recipients);
        let fog = state.routing_fog.clone();
        let acknowledgments = delivery.clone();
        for (side, at_tick) in [(1, 60), (0, 62)] {
            let mut invalid = say.clone();
            invalid.side = side;
            invalid.at_tick = at_tick;
            let shots = [ShotRef::Scenario(say), ShotRef::Scenario(&invalid)];
            assert!(matches!(
                delivery.capture_selected_with_shots(&mut state, &selected, &shots),
                Err(SnapshotError::Rules)
            ));
            assert_eq!(format!("{:?}", state.world), world);
            assert_eq!(format!("{:?}", state.recipients), recipients);
            assert_eq!(state.routing_fog, fog);
            assert_eq!(delivery, acknowledgments);
        }
        // Persisted point metadata can be structurally valid yet refer to a
        // nonexistent commander. Match-bound admission must refuse it globally.
        let mut invalid_state = state.scenario.as_ref().unwrap().state().clone();
        *invalid_state.owners.values_mut().next().unwrap() = Some(2);
        state
            .scenario
            .as_mut()
            .unwrap()
            .restore_state(invalid_state)
            .unwrap();
        let scenario = state.scenario.as_ref().unwrap().state().clone();
        assert!(matches!(
            delivery.capture_selected_with_shots(&mut state, &selected, &[ShotRef::Scenario(say)]),
            Err(SnapshotError::Unsupported {
                capability: SnapshotCapability::Scenario,
                ..
            })
        ));
        assert_eq!(state.scenario.as_ref().unwrap().state(), &scenario);
        assert_eq!(format!("{:?}", state.world), world);
        assert_eq!(format!("{:?}", state.recipients), recipients);
        assert_eq!(state.routing_fog, fog);
        assert_eq!(delivery, acknowledgments);
    }

    #[test]
    fn later_unrepresented_metadata_refuses_before_any_recipient_publication() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/recipient-delivery.json"))
                .unwrap();
        let mut state = super::super::tests::initialize(&fixture["initial"]);
        let mut delivery = DeliveryState::new(&state).unwrap();
        state
            .world
            .set_cell(6 * state.world.game.w + 6, b'T')
            .unwrap();
        let valid = Effect::Hurt(crate::environment::Hurt {
            unit: 1,
            owner: 0,
            at: Point { x: 13.0, z: 13.0 },
            killed: false,
            from: None,
            local: false,
        });
        let metadata = Effect::LastHit { unit: 1, owner: 0 };
        let shots = [
            ShotRef::Environment(&valid),
            ShotRef::Environment(&metadata),
        ];
        let world = format!("{:?}", state.world);
        let recipients = format!("{:?}", state.recipients);
        let fog = state.routing_fog.clone();
        let acknowledgments = delivery.clone();
        assert!(matches!(
            delivery.capture_selected_with_shots(
                &mut state,
                &[SelectedSeat {
                    slot: 0,
                    full: false,
                    omit: false
                }],
                &shots,
            ),
            Err(SnapshotError::Unsupported {
                capability: SnapshotCapability::ShotsAndFlights,
                ..
            })
        ));
        assert_eq!(format!("{:?}", state.world), world);
        assert_eq!(format!("{:?}", state.recipients), recipients);
        assert_eq!(state.routing_fog, fog);
        assert_eq!(delivery, acknowledgments);
        // A later unrepresented producer row must not publish the valid prefix.
        for unexpected in [
            TickEvent::Aviation(crate::match_aviation::AviationEvent::PlaneDown {
                target: 1,
                owner: 0,
                at: Point { x: 13.0, z: 13.0 },
                direction: 0.0,
            }),
            TickEvent::Firing(LaunchEvent::Throw {
                shooter: 1,
                owner: 0,
                at: Point { x: 13.0, z: 13.0 },
            }),
            TickEvent::Environment(metadata.clone()),
        ] {
            let batch = [TickEvent::Environment(valid.clone()), unexpected];
            assert!(
                delivery
                    .capture_selected_with_tick_events(
                        &mut state,
                        &[SelectedSeat {
                            slot: 0,
                            full: false,
                            omit: false
                        }],
                        &batch,
                    )
                    .is_err()
            );
            assert_eq!(format!("{:?}", state.world), world);
            assert_eq!(format!("{:?}", state.recipients), recipients);
            assert_eq!(state.routing_fog, fog);
            assert_eq!(delivery, acknowledgments);
            assert!(
                delivery
                    .capture_selected_with_tick_events(&mut state, &[], &batch)
                    .unwrap()
                    .is_none()
            );
        }
        assert!(
            delivery
                .capture_selected_with_shots(&mut state, &[], &shots)
                .unwrap()
                .is_none()
        );
        assert_eq!(format!("{:?}", state.world), world);
        assert_eq!(format!("{:?}", state.recipients), recipients);
        assert_eq!(state.routing_fog, fog);
        assert_eq!(delivery, acknowledgments);
    }
}
