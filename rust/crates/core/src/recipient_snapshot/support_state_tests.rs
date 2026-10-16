use super::*;
use crate::{
    game::{Point, StrikeKind},
    match_support::SupportKind,
};

#[test]
fn every_support_kind_projects_canonical_strike_state_for_each_seat() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/recipient-snapshot.json")).unwrap();
    let mut state = super::super::tests::initialize(&fixture["cases"][0]);
    state.world.game.strikes.clear();
    for (index, kind) in [
        SupportKind::Recon,
        SupportKind::Strafe,
        SupportKind::Bombing,
        SupportKind::Dive,
        SupportKind::Para,
        SupportKind::Artillery,
        SupportKind::Smoke,
        SupportKind::Cover,
    ]
    .into_iter()
    .enumerate()
    {
        for player in &mut state.start.players {
            for cooldown in player.support.values_mut() {
                *cooldown = 0.;
            }
        }
        let owner = index % state.start.players.len();
        state.start.economy.players[owner].mp = 10000.;
        let at = Point { x: 14.15, z: 16.25 };
        assert_eq!(
            state
                .admit_support(owner, kind.name(), at, Some(-0.15))
                .unwrap(),
            None
        );
        let strike = state.world.game.strikes.last_mut().unwrap();
        // The public row clamps an expired arrival timer to zero seconds.
        strike.runtime.as_mut().unwrap().delay = -0.15;
    }
    let expected = json!([
        ["recon", 14.2, 16.3, -0.1, 0., 0],
        ["strafe", 14.2, 16.3, -0.1, 0., 1],
        ["bombing", 14.2, 16.3, -0.1, 0., 2],
        ["dive", 14.2, 16.3, -0.1, 0., 0],
        ["para", 14.2, 16.3, -0.1, 0., 1],
        ["artillery", 14.2, 16.3, -0.1, 0., 2],
        ["smoke", 14.2, 16.3, -0.1, 0., 0],
        ["cover", 14.2, 16.3, -0.1, 0., 1]
    ]);
    let cache = SnapshotCache::new(&state).unwrap();
    for slot in 0..state.start.players.len() {
        let fields = cache.recipient(slot).unwrap().fields;
        assert_eq!(fields["strikes"], expected);
        assert!(fields["sup"].is_object());
    }
    state.world.game.strikes[0].kind = StrikeKind::Other;
    assert!(matches!(
        SnapshotCache::new(&state),
        Err(SnapshotError::Unsupported {
            capability: SnapshotCapability::TeamEffects,
            ..
        })
    ));
}

fn support_scene() -> Match {
    use crate::{
        economy::{Faction, Mode},
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_runtime::{MatchOptions, PlayerConfig},
        weather::WeatherChoice,
    };
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Plane support delivery", "w":80,"h":80,
            "rows":vec![".".repeat(80);80],
            "spawns":[{"x":8,"y":8},{"x":8,"y":72},{"x":72,"y":72}],
            "points":[{"x":40,"y":40}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 8721,
            mode: Mode::Conquest,
            players: [0, 0, 1]
                .into_iter()
                .enumerate()
                .map(|(slot, team)| PlayerConfig {
                    name: format!("P{slot}"),
                    team,
                    faction: Faction::Usa,
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
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    state.start.economy.players[0].mp = 10000.;
    state
}

fn support_snapshots(
    state: &mut Match,
    delivery: &mut DeliveryState,
    events: &[crate::match_tick::TickEvent],
) -> Vec<Value> {
    let selected = (0..state.start.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(state, &selected, events)
        .unwrap()
        .unwrap();
    selected
        .iter()
        .map(|seat| frame.recipient(seat.slot).unwrap().into_json())
        .collect()
}

#[test]
fn mov92_plane_support_announces_to_every_seat_and_delivers_real_payload_through_expiry() {
    use crate::{environment::Effect, match_support::SupportEvent, match_tick::TickEvent};
    // sim.js:5935,6052,5249: six public incoming fields, then the kind on arrival.
    for (kind, delay) in [
        (SupportKind::Recon, 3.),
        (SupportKind::Strafe, 5.),
        (SupportKind::Bombing, 6.),
        (SupportKind::Dive, 5.),
        (SupportKind::Para, 6.),
    ] {
        let mut state = support_scene();
        let at = if kind == SupportKind::Recon {
            Point { x: 80.15, z: 80.25 }
        } else {
            Point { x: 44.15, z: 16.25 }
        };
        let direction = 1.55;
        let victim = state
            .world
            .game
            .units
            .values()
            .find(|unit| unit.owner == 2 && state.rules.kinds[unit.kind].name == "rifle")
            .unwrap()
            .id;
        if kind == SupportKind::Strafe {
            crate::unit_placement::place_unit(
                &mut state.grid,
                state.world.game.units.get_mut(&victim).unwrap(),
                |unit| {
                    unit.x = at.x;
                    unit.z = at.z;
                    unit.hold_pos = true;
                },
            );
        }
        let initial_hp = state.world.game.units[&victim].hp;
        let initial_owned = state
            .world
            .game
            .units
            .values()
            .filter(|u| u.owner == 0)
            .count();
        let sees_recon_center = |state: &Match| {
            let mut fog = state.routing_fog.clone();
            let game = &state.world.game;
            let cell = game.cell_of(at.x, at.z).unwrap();
            fog.team(game, &state.rules, 0).vis[cell] != 0
        };
        if kind == SupportKind::Recon {
            assert!(!sees_recon_center(&state));
        }
        assert_eq!(
            state
                .admit_support(0, kind.name(), at, Some(direction))
                .unwrap(),
            None
        );
        assert!(SUPPORTED_SUPPORT_KINDS.contains(&kind));
        state.validate_tick_state().unwrap();
        state.preflight_ordinary_snapshot().unwrap();
        let mut delivery = DeliveryState::new(&state).unwrap();
        let initial = support_snapshots(&mut state, &mut delivery, &[]);
        let rounded_x = if kind == SupportKind::Recon {
            80.2
        } else {
            44.2
        };
        for snapshot in initial {
            assert_eq!(
                snapshot["strikes"],
                json!([[kind.name(), rounded_x, r(at.z), 1.6, delay, 0]])
            );
        }
        let mut arrivals = 0;
        let mut bombs = 0;
        let mut chutes = 0;
        let mut hurts = 0;
        let mut recon_live = false;
        let mut retired_ticks = 0;
        let mut previous_seconds = delay;
        for _ in 0..500 {
            let report = state.advance_tick().unwrap();
            let mut arrived = false;
            for event in &report.events {
                match event {
                    TickEvent::Support(SupportEvent::Plane { kind: actual, .. }) => {
                        assert_eq!(*actual, kind);
                        arrivals += 1;
                        arrived = true;
                    }
                    TickEvent::Support(SupportEvent::Bomb(_)) => bombs += 1,
                    TickEvent::Support(SupportEvent::Chutes(_)) => chutes += 1,
                    TickEvent::Support(SupportEvent::Environment(Effect::Hurt(hurt)))
                        if kind == SupportKind::Strafe =>
                    {
                        assert_eq!(hurt.unit, victim);
                        hurts += 1;
                    }
                    _ => {}
                }
            }
            if kind == SupportKind::Recon && state.world.game.strikes.iter().any(|s| s.live) {
                // Fog refreshes at the next vision pass, after support arrival.
                recon_live |= sees_recon_center(&state);
            }
            let active = !state.world.game.strikes.is_empty();
            let snapshots = support_snapshots(&mut state, &mut delivery, &report.events);
            let strikes = snapshots[0]["strikes"].clone();
            for snapshot in &snapshots {
                assert_eq!(snapshot["strikes"], strikes);
                if arrived {
                    let arrivals = snapshot["shots"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|shot| shot["k"] == kind.name())
                        .collect::<Vec<_>>();
                    assert_eq!(
                        arrivals,
                        vec![
                            &json!({"k":kind.name(),"x":at.x,"z":at.z,"dir":direction,"fo":0,"pub":true})
                        ]
                    );
                }
            }
            if active {
                let row = &strikes[0];
                assert_eq!(row.as_array().unwrap().len(), 6);
                assert_eq!(row[0], kind.name());
                assert_eq!(row[1], json!(rounded_x));
                assert_eq!(row[2], json!(r(at.z)));
                assert_eq!(row[3], json!(1.6));
                assert_eq!(row[5], 0);
                let seconds = row[4].as_f64().unwrap();
                assert!(seconds >= 0. && seconds <= previous_seconds);
                previous_seconds = seconds;
            } else {
                assert_eq!(strikes, json!([]));
                retired_ticks += 1;
                if retired_ticks >= 5 {
                    break;
                }
            }
            state.validate_tick_state().unwrap();
            assert!(state.tick_state.fault().is_none());
        }
        assert_eq!(arrivals, 1, "{kind:?} announces exactly once");
        assert_eq!(retired_ticks, 5, "{kind:?} retires and remains deliverable");
        match kind {
            SupportKind::Recon => {
                assert!(recon_live);
                assert!(!sees_recon_center(&state));
            }
            SupportKind::Strafe => {
                assert_eq!(hurts, 1);
                assert!(state.world.game.units[&victim].hp < initial_hp);
            }
            SupportKind::Bombing => assert_eq!(bombs, 6),
            SupportKind::Dive => assert_eq!(bombs, 1),
            SupportKind::Para => {
                assert_eq!(chutes, 3);
                assert_eq!(
                    state
                        .world
                        .game
                        .units
                        .values()
                        .filter(|u| u.owner == 0)
                        .count(),
                    initial_owned + 3
                );
            }
            _ => unreachable!(),
        }
        state.validate_tick_state().unwrap();
        assert!(state.tick_state.fault().is_none());
    }
}
