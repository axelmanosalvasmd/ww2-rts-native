use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Event recipient rules", "w":80,"h":80,
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
            seed: 8701,
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
    }
    state
}

fn selected() -> Vec<SelectedSeat> {
    (0..3)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect()
}

fn unit_at(state: &Match, owner: usize) -> (u32, Point) {
    let unit = state
        .world
        .game
        .units
        .values()
        .find(|unit| unit.owner == owner && unit.air.is_none())
        .unwrap();
    (
        unit.id,
        Point {
            x: unit.x,
            z: unit.z,
        },
    )
}

#[test]
fn support_arrival_bomb_chutes_and_shotdown_match_prototype_exact_shapes() {
    // sim.js:5249-5262,5281: public arrivals carry their support kind as k.
    // sim.js:4709: shotdown names interception type, never a target identity.
    let state = scene();
    let at = Point {
        x: 143.25,
        z: 145.75,
    };
    for kind in [
        SupportKind::Recon,
        SupportKind::Strafe,
        SupportKind::Bombing,
        SupportKind::Dive,
        SupportKind::Para,
    ] {
        let arrival = SupportEvent::Plane {
            kind,
            at,
            direction: 1.25,
            owner: 2,
        };
        let bomb = SupportEvent::Bomb(at);
        let chutes = SupportEvent::Chutes(at);
        for fighters in [false, true] {
            let shotdown = SupportEvent::ShotDown {
                kind,
                at,
                direction: 1.25,
                fighters,
            };
            let rows = project_shots(
                &state,
                &selected(),
                &[
                    ShotRef::Support(&arrival),
                    ShotRef::Support(&bomb),
                    ShotRef::Support(&chutes),
                    ShotRef::Support(&shotdown),
                ],
            )
            .unwrap();
            let expected = json!([
                {"k":kind.name(),"x":at.x,"z":at.z,"dir":1.25,"fo":2,"pub":true},
                {"k":"bomb","x":at.x,"z":at.z,"pub":true},
                {"k":"chutes","x":at.x,"z":at.z,"pub":true},
                {"k":"shotdown","x":at.x,"z":at.z,"dir":1.25,"kind":kind.name(),
                 "by":if fighters {"fighters"} else {"flak"},"pub":true}
            ]);
            for (_, actual) in rows {
                assert_eq!(actual, expected);
            }
        }
    }
}

#[test]
fn flak_public_event_hides_shooter_id_but_retains_prototype_owner_field() {
    // sim.js:4704,6080-6089: public nonlocal events redact f, retaining fo.
    let mut state = scene();
    let (shooter, at) = unit_at(&state, 2);
    state.world.game.players[0].visible.clear();
    state.world.game.players[1].visible.clear();
    let event = SupportEvent::Flak {
        unit: shooter,
        owner: 2,
        at,
    };
    let rows = project_shots(&state, &selected(), &[ShotRef::Support(&event)]).unwrap();
    let hidden = json!([{"k":"flak","fo":2,"x":at.x,"z":at.z,"pub":true}]);
    let visible = json!([{"k":"flak","f":shooter,"fo":2,"x":at.x,"z":at.z,"pub":true}]);
    assert_eq!(rows[0].1, hidden);
    assert_eq!(rows[1].1, hidden);
    assert_eq!(rows[2].1, visible);
    state.world.game.players[0].visible = vec![shooter];
    let rows = project_shots(&state, &selected(), &[ShotRef::Support(&event)]).unwrap();
    assert_eq!(rows[0].1, visible);
    assert_eq!(rows[1].1, hidden);
    state.world.game.reveal = true;
    for (_, row) in project_shots(&state, &selected(), &[ShotRef::Support(&event)]).unwrap() {
        assert_eq!(row, visible);
    }
}

#[test]
fn throw_uses_ground_cell_fog_and_redacts_both_hidden_shooter_fields() {
    // sim.js:2104,5427,5998-6004: local throw uses fog even for allies.
    let mut state = scene();
    let (enemy, shooter_at) = unit_at(&state, 2);
    // Seeing a ground unit reveals its own cell. Keep the event on a distinct
    // hidden cell so shooter visibility cannot make this assertion vacuous.
    let hidden_at = Point {
        x: shooter_at.x + 6.0,
        z: shooter_at.z,
    };
    let (_, visible_at) = unit_at(&state, 0);
    state.world.game.players[0].visible.clear();
    let visible_throw = LaunchEvent::Throw {
        shooter: enemy,
        owner: 2,
        at: visible_at,
    };
    let hidden_throw = LaunchEvent::Throw {
        shooter: enemy,
        owner: 2,
        at: hidden_at,
    };
    let rows = project_shots(
        &state,
        &selected(),
        &[
            ShotRef::Firing(&visible_throw),
            ShotRef::Firing(&hidden_throw),
        ],
    )
    .unwrap();
    assert_eq!(
        rows[0].1,
        json!([
            {"k":"throw","x":visible_at.x,"z":visible_at.z,"physical":true,"local":true}
        ])
    );
    assert_eq!(
        rows[2].1,
        json!([
            {"k":"throw","f":enemy,"fo":2,"x":hidden_at.x,"z":hidden_at.z,"physical":true,"local":true}
        ])
    );
    state.world.game.players[0].visible = vec![enemy];
    let rows = project_shots(
        &state,
        &selected(),
        &[
            ShotRef::Firing(&visible_throw),
            ShotRef::Firing(&hidden_throw),
        ],
    )
    .unwrap();
    // Knowing the shooter does not reveal a local event outside the fog mask.
    assert_eq!(
        rows[0].1,
        json!([
            {"k":"throw","f":enemy,"fo":2,"x":visible_at.x,"z":visible_at.z,"physical":true,"local":true}
        ])
    );
}

#[test]
fn anti_air_and_plane_down_match_shapes_and_redact_each_hidden_identity() {
    // sim.js:2250-2251,6080-6089: aa and planedown use nonlocal identity rules.
    let mut state = scene();
    let (shooter, at) = unit_at(&state, 0);
    let (target, _) = unit_at(&state, 2);
    for player in &mut state.world.game.players {
        player.visible.clear();
    }
    let aa = AviationEvent::AntiAir {
        shooter,
        target,
        owner: 0,
        target_owner: 2,
        at,
    };
    let down = AviationEvent::PlaneDown {
        target,
        owner: 2,
        at,
        direction: 0.75,
    };
    let shots = [ShotRef::Aviation(&aa), ShotRef::Aviation(&down)];
    let rows = project_shots(&state, &selected(), &shots).unwrap();
    assert_eq!(
        rows[0].1,
        json!([
            {"k":"aa","f":shooter,"fo":0,"to":2,"x":at.x,"z":at.z},
            {"k":"planedown","to":2,"x":at.x,"z":at.z,"dir":0.75,"kill":true}
        ])
    );
    assert_eq!(
        rows[2].1,
        json!([
            {"k":"aa","fo":0,"t":target,"to":2,"x":at.x,"z":at.z},
            {"k":"planedown","t":target,"to":2,"x":at.x,"z":at.z,"dir":0.75,"kill":true}
        ])
    );
    state.world.game.players[0].visible = vec![target];
    let rows = project_shots(&state, &selected(), &shots).unwrap();
    assert_eq!(
        rows[0].1,
        json!([
            {"k":"aa","f":shooter,"fo":0,"t":target,"to":2,"x":at.x,"z":at.z},
            {"k":"planedown","t":target,"to":2,"x":at.x,"z":at.z,"dir":0.75,"kill":true}
        ])
    );
}

#[test]
fn event_batch_keeps_chronology_through_ordinary_ticks_and_every_recipient_snapshot() {
    let mut state = scene();
    let (shooter, at) = unit_at(&state, 0);
    let (target, _) = unit_at(&state, 2);
    let mut delivery = DeliveryState::new(&state).unwrap();
    for _ in 0..4 {
        let report = state.advance_tick().unwrap();
        assert!(report.events.is_empty());
        // The same detached caller-owned batch crosses complete-frame admission.
        let events = vec![
            TickEvent::Support(SupportEvent::Plane {
                kind: SupportKind::Dive,
                at,
                direction: 0.5,
                owner: 0,
            }),
            TickEvent::Support(SupportEvent::Bomb(at)),
            TickEvent::Support(SupportEvent::Chutes(at)),
            TickEvent::Support(SupportEvent::Flak {
                unit: shooter,
                owner: 0,
                at,
            }),
            TickEvent::Support(SupportEvent::ShotDown {
                kind: SupportKind::Dive,
                at,
                direction: 0.5,
                fighters: false,
            }),
            TickEvent::Firing(LaunchEvent::Throw {
                shooter,
                owner: 0,
                at,
            }),
            TickEvent::Aviation(AviationEvent::AntiAir {
                shooter,
                target,
                owner: 0,
                target_owner: 2,
                at,
            }),
            TickEvent::Aviation(AviationEvent::PlaneDown {
                target,
                owner: 2,
                at,
                direction: 0.5,
            }),
        ];
        let expected = project_shots(
            &state,
            &selected(),
            &events
                .iter()
                .map(ShotRef::try_from)
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
        )
        .unwrap();
        let frame = delivery
            .capture_selected_with_tick_events(&mut state, &selected(), &events)
            .unwrap()
            .unwrap();
        for (slot, shots) in expected {
            assert_eq!(frame.recipient(slot).unwrap().into_json()["shots"], shots);
        }
        drop(frame);
        state.validate_tick_state().unwrap();
        assert!(state.tick_state.fault().is_none());
    }
}

#[test]
fn invalid_event_field_refuses_whole_batch_before_delivery_publication() {
    let mut state = scene();
    let (_, at) = unit_at(&state, 0);
    let valid = SupportEvent::Bomb(at);
    let invalid = [
        SupportEvent::Chutes(Point {
            x: f64::NAN,
            z: at.z,
        }),
        SupportEvent::Flak {
            unit: 0,
            owner: 0,
            at,
        },
        SupportEvent::Flak {
            unit: 1,
            owner: 3,
            at,
        },
        SupportEvent::Plane {
            kind: SupportKind::Smoke,
            at,
            direction: 0.5,
            owner: 0,
        },
        SupportEvent::ShotDown {
            kind: SupportKind::Dive,
            at,
            direction: f64::INFINITY,
            fighters: true,
        },
    ];
    let mut delivery = DeliveryState::new(&state).unwrap();
    for bad in &invalid {
        let acknowledgments = delivery.clone();
        let world = format!("{:?}", state.world);
        let memory = format!("{:?}", state.recipients);
        let fog = state.routing_fog.clone();
        assert!(
            delivery
                .capture_selected_with_shots(
                    &mut state,
                    &selected(),
                    &[ShotRef::Support(&valid), ShotRef::Support(bad)]
                )
                .is_err()
        );
        assert_eq!(delivery, acknowledgments);
        assert_eq!(format!("{:?}", state.world), world);
        assert_eq!(format!("{:?}", state.recipients), memory);
        assert_eq!(state.routing_fog, fog);
    }
}

#[test]
fn nonpublic_regular_shots_independently_use_position_allies_or_seen_participants() {
    // Prototype e4e579a shared/sim.js:6080-6088: nonlocal events retain owner fields.
    let mut state = scene();
    let (shooter, _) = unit_at(&state, 0);
    let (target, _) = unit_at(&state, 2);
    let (_, observer_at) = unit_at(&state, 1);
    let hidden_at = Point { x: 145.0, z: 17.0 };
    state.world.game.players[1].team = 2;
    state.start.players[1].config.team = 2;
    state.world.game.players[1].visible.clear();
    let selected = [SelectedSeat {
        slot: 1,
        full: false,
        omit: false,
    }];
    let hidden = AviationEvent::AntiAir {
        shooter,
        target,
        owner: 0,
        target_owner: 2,
        at: hidden_at,
    };
    let grid = crate::grid::SpatialGrid::new(&state.world.game.units, 16.0);
    assert!(!crate::vision::team_sees(
        &state.world.game,
        &state.rules,
        &grid,
        2,
        hidden_at
    ));
    let project = |state: &Match, event: &AviationEvent| {
        project_shots(state, &selected, &[ShotRef::Aviation(event)]).unwrap()[0]
            .1
            .clone()
    };
    let redacted = json!([{"k":"aa","fo":0,"to":2,"x":hidden_at.x,"z":hidden_at.z}]);
    assert_eq!(project(&state, &hidden), json!([]));

    // Each participant independently admits an otherwise unseen shot.
    for (seen, field, other) in [(shooter, "f", "t"), (target, "t", "f")] {
        state.world.game.players[1].visible = vec![seen];
        let actual = project(&state, &hidden);
        let mut expected = redacted.clone();
        expected[0][field] = json!(seen);
        assert_eq!(actual, expected);
        assert!(actual[0].get(other).is_none());
    }
    state.world.game.players[1].visible.clear();

    // An allied shooter or target independently admits the shot even at hidden ground.
    for (team, field, id, other) in [(0, "f", shooter, "t"), (1, "t", target, "f")] {
        state.world.game.players[1].team = team;
        state.start.players[1].config.team = team;
        assert!(!crate::vision::team_sees(
            &state.world.game,
            &state.rules,
            &grid,
            team,
            hidden_at
        ));
        let actual = project(&state, &hidden);
        let mut expected = redacted.clone();
        expected[0][field] = json!(id);
        assert_eq!(actual, expected);
        assert!(actual[0].get(other).is_none());
    }
    state.world.game.players[1].team = 2;
    state.start.players[1].config.team = 2;

    // Visible ground alone admits the shot while both participant IDs stay hidden.
    let visible = AviationEvent::AntiAir {
        shooter,
        target,
        owner: 0,
        target_owner: 2,
        at: observer_at,
    };
    assert!(crate::vision::team_sees(
        &state.world.game,
        &state.rules,
        &grid,
        2,
        observer_at
    ));
    assert_eq!(
        project(&state, &visible),
        json!([
            {"k":"aa","fo":0,"to":2,"x":observer_at.x,"z":observer_at.z}
        ])
    );
    state.world.game.reveal = true;
    assert_eq!(
        project(&state, &hidden),
        json!([
            {"k":"aa","f":shooter,"fo":0,"t":target,"to":2,"x":hidden_at.x,"z":hidden_at.z}
        ])
    );
}
