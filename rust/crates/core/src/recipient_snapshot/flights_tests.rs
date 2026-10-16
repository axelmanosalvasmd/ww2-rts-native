use super::*;
use crate::{
    match_firing::LiveFlight,
    projectiles::{Flight, Point3},
};

fn scene() -> Match {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/recipient-snapshot.json")).unwrap();
    let mut state = super::super::tests::initialize(&fixture["cases"][0]);
    state.world.game.tick = 20;
    state.world.game.reveal = false;
    state.world.game.chars.fill(b'.');
    state.world.game.height = None;
    let kind = state.world.game.units[&1].kind;
    let solution = Flight {
        origin: Point3::new(5.0, 10.0, 25.0),
        velocity: Point3::new(20.0, 0.0, 0.0),
        gravity: 0.0,
        stamp: 0.0,
    };
    state.firing.flights.insert(
        7,
        LiveFlight {
            id: 7,
            owner: 0,
            shooter: 1,
            source_kind: kind,
            solution,
            launch_origin: solution.origin,
            launched: 0.0,
            last: 1.0,
            expires: 3.0,
            sent: true,
            energy: 1.0,
            sequence: 2,
            max_contacts: 3,
            near: 0.0,
            start_cell: state
                .world
                .game
                .cell_of(solution.origin.x, solution.origin.z)
                .unwrap(),
            intended: Some(3),
            damage_scale: 1.0,
            armor_piercing: false,
            ap_multiplier: 1.0,
            ap_naval: false,
            suppression: 0.0,
            suppression_scale: 1.0,
            volley_group: 7,
            explosive: false,
            terrain: 0.0,
            scar: 0.0,
            blast_radius: None,
            blast_source: None,
            effect: Some("shell"),
        },
    );
    state.firing.next_flight = 8;
    for team in [0, 1] {
        state
            .routing_fog
            .team(&state.world.game, &state.rules, team);
        state.routing_fog.teams.get_mut(&team).unwrap().vis.fill(1);
    }
    state
}

#[test]
fn live_flight_exact_prototype_shape_and_field_privacy() {
    let mut state = scene();
    // shared/sim.js:5985-5995 constructs precisely these keys. No target,
    // launch-origin, damage, private weapon or contact metadata is serialized.
    let expected = json!([{"flight":7,"kind":"rifle","effect":"shell","seq":2,
        "x":25.0,"y":10.0,"z":25.0,"v":[20.0,0,0],"gravity":0,
        "time":1.0,"launched":0,"expires":3.0,"until":1.15,"f":1,"fo":0}]);
    let original: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/live-flight-wire.json")).unwrap();
    for slot in 0..3 {
        if slot == 2 {
            state.world.game.players[slot].visible.clear();
        }
        super::super::tests::equal(
            &project(&state, slot).unwrap(),
            &original["rows"][slot],
            "original live flight wire",
        );
    }
    assert_eq!(project(&state, 0).unwrap(), expected);
    assert_eq!(project(&state, 1).unwrap(), expected);
    state.world.game.players[2].visible.clear();
    let mut hidden = expected.clone();
    hidden[0].as_object_mut().unwrap().remove("f");
    hidden[0].as_object_mut().unwrap().remove("fo");
    assert_eq!(project(&state, 2).unwrap(), hidden);
    state.world.game.players[2].visible = vec![1];
    assert_eq!(project(&state, 2).unwrap(), expected);
    // Seeing a shooter never reveals a projectile in a hidden ground cell.
    let cell = state.world.game.cell_of(25.0, 25.0).unwrap();
    state.routing_fog.teams.get_mut(&1).unwrap().vis[cell] = 0;
    assert_eq!(project(&state, 2).unwrap(), json!([]));
    state.world.game.reveal = true;
    assert_eq!(project(&state, 2).unwrap(), expected);
    state.firing.flights.get_mut(&7).unwrap().launched = 1.01;
    assert_eq!(project(&state, 0).unwrap(), json!([]));
}

#[test]
fn live_flight_until_stops_at_fog_or_visible_static_contact_only() {
    let mut state = scene();
    let hidden_cell = state.world.game.cell_of(28.0, 25.0).unwrap();
    state.routing_fog.teams.get_mut(&1).unwrap().vis[hidden_cell] = 0;
    assert_eq!(project(&state, 2).unwrap()[0]["until"], json!(1.15));
    let hidden_cell = state.world.game.cell_of(26.0, 25.0).unwrap();
    state.routing_fog.teams.get_mut(&1).unwrap().vis[hidden_cell] = 0;
    assert_eq!(project(&state, 2).unwrap()[0]["until"], json!(1.05));
    state.routing_fog.teams.get_mut(&1).unwrap().vis.fill(1);
    state.firing.flights.get_mut(&7).unwrap().solution.origin.y = 1.0;
    let target = state.world.game.units.get_mut(&3).unwrap();
    target.x = 27.0;
    target.z = 25.0;
    target.hp = 100.0;
    state.grid = crate::grid::SpatialGrid::new(&state.world.game.units, 16.0);
    state.world.game.players[0].visible.clear();
    let hidden = project(&state, 0).unwrap()[0]["until"].as_f64().unwrap();
    assert_eq!(hidden, 1.15);
    state.world.game.players[0].visible = vec![3];
    let visible = project(&state, 0).unwrap()[0]["until"].as_f64().unwrap();
    assert_eq!(visible, 1.04);
    // Event launch extrapolation carries no weapon, so it must stay fog-only.
    let row = project(&state, 0).unwrap();
    assert_eq!(
        shots::until(&state, &state.routing_fog.teams[&0].vis, &row[0]).unwrap(),
        hidden
    );
}

#[test]
fn malformed_live_flight_refuses_before_delivery_memory_changes() {
    let mut state = scene();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let before = delivery.clone();
    let world = format!("{:?}", state.world);
    state
        .firing
        .flights
        .get_mut(&7)
        .unwrap()
        .solution
        .velocity
        .x = f64::NAN;
    let selected = [SelectedSeat {
        slot: 0,
        full: false,
        omit: false,
    }];
    assert!(
        delivery
            .capture_selected_with_tick_events(&mut state, &selected, &[])
            .is_err()
    );
    assert_eq!(delivery, before);
    assert_eq!(format!("{:?}", state.world), world);
}

#[test]
fn live_flight_fog_clipping_keeps_original_stamp_and_ballistic_trajectory() {
    let mut state = scene();
    state.world.game.tick = 10;
    let p = state.firing.flights.get_mut(&7).unwrap();
    p.solution = Flight {
        origin: Point3::new(5.3, 10.0, 25.0),
        velocity: Point3::new(19.7, 3.0, 0.0),
        gravity: 9.81,
        stamp: 0.05,
    };
    p.launched = 0.05;
    let hidden = state.world.game.cell_of(16.0, 25.0).unwrap();
    state.routing_fog.teams.get_mut(&0).unwrap().vis[hidden] = 0;
    let row = project(&state, 0).unwrap();
    let until = row[0]["until"].as_f64().unwrap();
    // Original flightAt(p, 0.65) yields x=17.119999999999997.
    // Rebuilding the flight at 0.5 yields x=17.12 and changes the clipping bit.
    assert_eq!(until.to_bits(), 0.5931472081218275_f64.to_bits());
    let event_until = shots::until(&state, &state.routing_fog.teams[&0].vis, &row[0]).unwrap();
    assert_eq!(event_until.to_bits(), 0.5931472081218274_f64.to_bits());
    assert_ne!(until.to_bits(), event_until.to_bits());
}

#[test]
fn watcher_retains_hidden_live_flights_and_bounds_them_by_full_map_contacts() {
    let mut state = scene();
    for team in state.routing_fog.teams.values_mut() {
        team.vis.fill(0);
    }
    for player in &mut state.world.game.players {
        player.visible.clear();
    }
    assert_eq!(project(&state, 2).unwrap(), json!([]));
    let before = format!("{:?}{:?}", state.world, state.routing_fog);
    let mut watcher = super::super::SpectatorDelivery::new(&state).unwrap();
    let view = watcher.capture(&state, &[], true).unwrap();
    assert_eq!(view["flights"][0]["flight"], 7);
    assert_eq!(view["flights"][0]["f"], 1);
    assert_eq!(view["flights"][0]["until"], 1.15);
    assert_eq!(format!("{:?}{:?}", state.world, state.routing_fog), before);
    // EVERYTHING.has preserves the source identity after the actor retires.
    state.world.game.units.remove(&1).unwrap();
    let retired = watcher.capture(&state, &[], false).unwrap();
    assert_eq!(retired["flights"][0]["f"], 1);
    assert_eq!(retired["flights"][0]["fo"], 0);
    state.firing.flights.get_mut(&7).unwrap().solution.origin.y = 1.0;
    let target = state.world.game.units.get_mut(&3).unwrap();
    target.x = 27.0;
    target.z = 25.0;
    state.grid = crate::grid::SpatialGrid::new(&state.world.game.units, 16.0);
    let view = watcher.capture(&state, &[], false).unwrap();
    assert_eq!(view["flights"][0]["until"], 1.04);
    assert_eq!(project(&state, 2).unwrap(), json!([]));
    state.firing.flights.clear();
    assert_eq!(
        watcher.capture(&state, &[], false).unwrap()["flights"],
        json!([])
    );
}
