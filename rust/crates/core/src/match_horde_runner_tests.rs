use super::*;
use crate::{
    match_tick::{TickEvent, TickPhase},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Horde lifetime", "w":60,"h":60,"rows":vec![".".repeat(60);60],
            "spawns":[{"x":30,"y":5},{"x":10,"y":54},{"x":50,"y":54}],
            "defend":[0],"points":[{"x":30,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new_horde(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Horde,
            players: (0..3)
                .map(|slot| PlayerConfig {
                    name: format!("P{slot}"),
                    team: slot,
                    faction: Faction::Usa,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: true,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}

#[test]
fn mode52_mode54_boss_wave_runs_to_bunker_defeat_with_every_seat_snapshot() {
    let mut state = scene();
    let bunker = state.horde.as_ref().unwrap().bunker;
    let target = state.world.game.units[&bunker].at();
    // Declared start scene: an injured bunker and the next boss wave at a close gate.
    state.world.game.units.get_mut(&bunker).unwrap().hp = 1.0;
    for unit in state.world.game.units.values_mut() {
        if !state.rules.kinds[unit.kind].structure {
            unit.x = 11.0;
            unit.z = 101.0;
            unit.path.clear();
            unit.amove = None;
        }
    }
    let gate = Point {
        x: target.x,
        z: target.z + 14.0,
    };
    let horde = state.horde.as_mut().unwrap();
    horde.wave = 9;
    horde.time_left = 0.0;
    horde.gates = vec![gate];
    horde.profile_wave = 0;
    state.grid.rebuild(&state.world.game.units);
    let seats = (0..4)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let mut surfaced = false;
    let mut breath = false;
    let mut ending = 0;
    for _ in 0..200 {
        let report = state.advance_tick().unwrap();
        assert!(report.phases.contains(&TickPhase::Horde));
        for event in &report.events {
            surfaced |= matches!(event, TickEvent::Horde(HordeEvent::BossSurfaced { .. }));
            breath |= matches!(
                event,
                TickEvent::Firing(crate::match_firing::LaunchEvent::Breath { .. })
            );
        }
        let ended = state.match_result().unwrap().is_some();
        let frame = delivery
            .capture_selected_with_tick_events(&mut state, &seats, &report.events)
            .unwrap()
            .unwrap();
        for slot in 0..4 {
            let row = frame.recipient(slot).unwrap().into_json();
            assert_eq!(row["mode"]["kind"], "horde");
            assert_eq!(row["mode"]["wave"], 10);
            assert_eq!(row["mode"]["slot"], 3);
            assert_eq!(row["mode"]["suddenDeath"], false);
            assert!(row["mode"].get("nextProfile").is_none());
            if report.tick == 1 {
                assert_eq!(row["mode"]["boss"], json!([6000.0, 6000.0]));
                assert!(row["shots"].as_array().unwrap().contains(&json!({
                    "k":"say","text":"THE KAIJU HAS SURFACED","x":gate.x,"z":gate.z,"pub":true
                })));
            }
            if ended {
                assert_eq!(row["winner"], 1);
            }
        }
        if ended {
            ending += 1;
            if ending == 8 {
                break;
            }
        }
    }
    assert!(
        surfaced && breath,
        "boss spawning and beam must run through the tick owner"
    );
    assert_eq!(
        ending, 8,
        "natural bunker defeat must continue through ending ticks"
    );
    let result = state.horde_result().unwrap().unwrap();
    assert_eq!(result.wave, 10);
    assert_eq!(result.defender_outcomes, vec!["defeat"; 3]);
    assert_eq!(result.report.winner, 1);
    assert_eq!(
        result.report.reason,
        crate::match_end::EndReason::Structures
    );
    assert_eq!(result.report.at, target);
    assert!(state.tick_state.fault().is_none());
}

#[test]
fn horde_mode_wire_fields_and_private_purchase_omissions_match_prototype() {
    let mut state = scene();
    let expected = json!({
        "kind":"horde","defenderTeam":0,"attackerTeam":1,"timeLeft":45.0,
        "suddenDeath":false,"wave":0,"left":0.0,"active":false,"slot":3,
        "nextProfile":state.horde.as_ref().unwrap().next_profile.as_str()
    });
    let mut delivery = DeliveryState::new(&state).unwrap();
    let selected = (0..4)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    {
        let frame = delivery
            .capture_selected_with_tick_events(&mut state, &selected, &[])
            .unwrap()
            .unwrap();
        for slot in 0..4 {
            assert_eq!(frame.recipient(slot).unwrap().into_json()["mode"], expected);
        }
    }
    let h = state.horde.as_mut().unwrap();
    h.wave = 10;
    h.active = true;
    h.profile = HordeProfile::Armor;
    h.boss = Some([1235.0, 6000.0]);
    h.time_left = 0.0;
    let frame = delivery
        .capture_selected_with_tick_events(&mut state, &selected, &[])
        .unwrap()
        .unwrap();
    let expected = json!({
        "kind":"horde","defenderTeam":0,"attackerTeam":1,"timeLeft":0.0,
        "suddenDeath":false,"wave":10,"left":0.0,"active":true,"slot":3,
        "profile":"armor","boss":[1235.0,6000.0]
    });
    for slot in 0..4 {
        assert_eq!(frame.recipient(slot).unwrap().into_json()["mode"], expected);
    }
}

#[test]
fn horde_public_beam_keeps_prototype_shape_and_masks_unseen_actor_ids() {
    let mut state = scene();
    let target = state.horde.as_ref().unwrap().bunker;
    let boss = state
        .spawn_horde_ground(
            3,
            "kaiju",
            Point { x: 101.0, z: 101.0 },
            state.world.game.units[&target].at(),
            0,
            0.0,
        )
        .unwrap();
    for player in &mut state.world.game.players {
        player.visible.clear();
    }
    let at = Point { x: 93.0, z: 41.0 };
    let events = vec![
        TickEvent::Firing(crate::match_firing::LaunchEvent::Breath {
            shooter: boss,
            target,
            owner: 3,
            target_owner: 0,
            at,
        }),
        TickEvent::Firing(crate::match_firing::LaunchEvent::BreathShell { at }),
    ];
    let mut delivery = DeliveryState::new(&state).unwrap();
    let selected = (0..4)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(&mut state, &selected, &events)
        .unwrap()
        .unwrap();
    for slot in 0..4 {
        let mut expected =
            json!({"fo":3,"to":0,"x":93.0,"z":41.0,"hit":true,"k":"kaiju","pub":true});
        if slot == 3 {
            expected["f"] = json!(boss);
        } else {
            expected["t"] = json!(target);
        }
        assert_eq!(
            frame.recipient(slot).unwrap().into_json()["shots"],
            json!([
                expected,{"k":"shell","x":93.0,"z":41.0,"pub":true}
            ])
        );
    }
}
