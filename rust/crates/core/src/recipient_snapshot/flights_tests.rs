use super::*;
use crate::{
    game::Point,
    grid::SpatialGrid,
    match_firing::{GrenadeTicket, LaunchEvent, LiveSalvo},
    projectiles::{Flight, Point3},
};

fn scene() -> Match {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/recipient-selected.json")).unwrap();
    super::super::tests::initialize(&fixture["initial"])
}

fn flight(state: &Match) -> LiveFlight {
    LiveFlight {
        id: 7,
        owner: 0,
        shooter: 1,
        source_kind: state.rules.kind("rifle").unwrap(),
        solution: Flight {
            origin: Point3::new(5.0, 100.0, 5.0),
            velocity: Point3::new(20.0, 4.0, 0.0),
            gravity: 8.0,
            stamp: 0.0,
        },
        launch_origin: Point3::new(5.0, 100.0, 5.0),
        launched: 0.0,
        last: 0.0,
        expires: 5.0,
        sent: true,
        energy: 1.0,
        sequence: 2,
        max_contacts: 4,
        near: 0.0,
        start_cell: 2 * state.world.game.w + 2,
        intended: None,
        damage_scale: 1.0,
        armor_piercing: false,
        ap_multiplier: 1.5,
        ap_naval: false,
        suppression: 0.0,
        suppression_scale: 1.0,
        volley_group: 7,
        explosive: false,
        terrain: 0.0,
        scar: 0.0,
        blast_radius: None,
        blast_source: None,
        effect: Some("rocket"),
    }
}

#[test]
fn retained_rows_match_prototype_shape_current_trajectory_and_shooter_privacy() {
    let state = scene();
    let p = flight(&state);
    let visible = vec![1; state.world.game.chars.len()];
    let expected = json!({
        "flight":7,"kind":"rifle","effect":"rocket","seq":2,
        "x":15,"y":101,"z":5,"v":[20,0,0],"gravity":8,
        "time":0.5,"launched":0,"expires":5,"until":0.65,"f":1,"fo":0
    });
    super::super::tests::equal(
        &row(&state, 0, &visible, &p, 0.5).unwrap().unwrap(),
        &expected,
        "flight",
    );
    let hidden = row(&state, 2, &visible, &p, 0.5).unwrap().unwrap();
    let mut hidden_expected = expected;
    hidden_expected.as_object_mut().unwrap().remove("f");
    hidden_expected.as_object_mut().unwrap().remove("fo");
    super::super::tests::equal(&hidden, &hidden_expected, "hidden shooter");
    let mut p = p;
    p.effect = None;
    assert!(
        row(&state, 0, &visible, &p, 0.5)
            .unwrap()
            .unwrap()
            .get("effect")
            .is_none()
    );
}

#[test]
fn retained_flights_skip_future_launches_and_clip_at_ground_fog_boundary() {
    let state = scene();
    let mut p = flight(&state);
    p.solution.velocity = Point3::new(20.0, 0.0, 0.0);
    p.solution.gravity = 0.0;
    let mut visible = vec![1; state.world.game.chars.len()];
    // CELL is 2 m. The high projectile crosses x=6 into the hidden ground cell.
    visible[2 * state.world.game.w + 3] = 0;
    let projected = row(&state, 0, &visible, &p, 0.0).unwrap().unwrap();
    assert_eq!(projected["until"].as_f64(), Some(0.15 * (1.0 / 3.0)));
    visible[2 * state.world.game.w + 2] = 0;
    assert!(row(&state, 0, &visible, &p, 0.0).unwrap().is_none());
    p.launched = 1.0;
    visible.fill(1);
    assert!(row(&state, 0, &visible, &p, 0.0).unwrap().is_none());
    p.launched = 0.0;
    p.expires = 0.1;
    assert_eq!(
        row(&state, 0, &visible, &p, 0.0).unwrap().unwrap()["until"],
        json!(0.1)
    );
}

#[test]
fn retained_contact_clip_uses_current_visible_bodies_and_terrain() {
    let mut state = scene();
    let mut p = flight(&state);
    p.sequence = 0;
    // The selected-delivery fixture retains starting-base K cells near z=5.
    // Use a clear corridor so only the controlled body can clip this segment.
    for x in 2..=5 {
        assert_eq!(state.world.game.physical(10 * state.world.game.w + x), b'.');
    }
    p.start_cell = 10 * state.world.game.w + 2;
    p.solution.origin = Point3::new(5.0, 1.2, 21.0);
    p.solution.velocity = Point3::new(40.0, 0.0, 0.0);
    p.solution.gravity = 0.0;
    let visible = vec![1; state.world.game.chars.len()];
    state.world.game.players[0].visible.clear();
    let target = state.world.game.units.get_mut(&3).unwrap();
    target.x = 9.0;
    target.z = 21.0;
    state.grid = SpatialGrid::new(&state.world.game.units, 16.0);
    assert_eq!(
        row(&state, 0, &visible, &p, 0.0).unwrap().unwrap()["until"],
        json!(0.15)
    );
    state.world.game.players[0].visible.push(3);
    let projected = row(&state, 0, &visible, &p, 0.0).unwrap().unwrap();
    assert!(projected["until"].as_f64().unwrap() < 0.1);
    state.world.game.players[0].visible.clear();
    state
        .world
        .set_cell(10 * state.world.game.w + 4, b'K')
        .unwrap();
    assert_eq!(
        row(&state, 0, &visible, &p, 0.0).unwrap().unwrap()["until"],
        json!(0.075)
    );
}

#[test]
fn complete_seats_deliver_retained_ground_flights_and_physical_tickets_without_draining_events() {
    let mut state = scene();
    state.world.game.reveal = true;
    let p = flight(&state);
    state.firing.flights.insert(p.id, p);
    state.firing.salvos.push(LiveSalvo {
        at: Point { x: 5.0, z: 5.0 },
        owner: 0,
        left: 2,
        next: 1.0,
        every: 0.2,
        spread: 3.0,
    });
    state.firing.grenades.push(GrenadeTicket {
        at: Point { x: 5.0, z: 5.0 },
        remaining: 1.0,
        owner: 0,
        flight: 7,
    });
    let selected = (0..state.world.game.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut delivery = DeliveryState::new(&state).unwrap();
    for tick in 1..=3 {
        state.world.game.tick = tick;
        let frame = delivery
            .capture_selected_with_tick_events(&mut state, &selected, &[])
            .unwrap()
            .unwrap();
        for seat in &selected {
            let snapshot = frame.recipient(seat.slot).unwrap().into_json();
            assert_eq!(snapshot["flights"].as_array().unwrap().len(), 1);
            assert_eq!(snapshot["shots"], json!([]));
        }
    }
    let buffered = LaunchEvent::Throw {
        shooter: 1,
        owner: 0,
        at: Point { x: 5.0, z: 5.0 },
    };
    state.firing.events.push(buffered);
    let before = delivery.clone();
    assert!(matches!(
        delivery.capture_selected_with_tick_events(&mut state, &selected, &[]),
        Err(SnapshotError::Unsupported {
            capability: SnapshotCapability::ShotsAndFlights,
            ..
        })
    ));
    assert_eq!(delivery, before);
    assert_eq!(state.firing.events.len(), 1);
    let events = state
        .firing
        .take_events()
        .into_iter()
        .map(crate::match_tick::TickEvent::Firing)
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(&mut state, &selected, &events)
        .unwrap()
        .unwrap();
    for seat in &selected {
        super::super::tests::equal(
            &frame.recipient(seat.slot).unwrap().into_json()["shots"],
            &json!([{"f":1,"fo":0,"x":5,"z":5,"k":"throw","physical":true,"local":true}]),
            "throw",
        );
    }
    state.firing.flights.clear();
    state.firing.salvos.clear();
    state.firing.grenades.clear();
    let frame = delivery
        .capture_selected_with_tick_events(&mut state, &selected, &[])
        .unwrap()
        .unwrap();
    for seat in &selected {
        assert_eq!(
            frame.recipient(seat.slot).unwrap().into_json()["flights"],
            json!([])
        );
    }
}

#[test]
fn invalid_retained_flight_and_aircraft_refuse_before_publication() {
    let mut state = scene();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let mut p = flight(&state);
    p.solution.origin.x = f64::NAN;
    state.firing.flights.insert(p.id, p);
    let selected = [SelectedSeat {
        slot: 0,
        full: false,
        omit: false,
    }];
    let world = format!("{:?}", state.world);
    let recipients = format!("{:?}", state.recipients);
    let fog = state.routing_fog.clone();
    let before = delivery.clone();
    assert!(matches!(
        delivery.capture_selected_with_tick_events(&mut state, &selected, &[]),
        Err(SnapshotError::InvalidNumber)
    ));
    state.firing.flights.get_mut(&7).unwrap().solution.origin.x = 5.0;
    let aircraft = state.rules.kinds.iter().position(|kind| kind.air).unwrap();
    state.firing.flights.get_mut(&7).unwrap().source_kind = aircraft;
    assert!(matches!(
        delivery.capture_selected_with_tick_events(&mut state, &selected, &[]),
        Err(SnapshotError::Unsupported {
            capability: SnapshotCapability::Air,
            ..
        })
    ));
    assert_eq!(delivery, before);
    assert_eq!(format!("{:?}", state.world), world);
    assert_eq!(format!("{:?}", state.recipients), recipients);
    assert_eq!(state.routing_fog, fog);
}
