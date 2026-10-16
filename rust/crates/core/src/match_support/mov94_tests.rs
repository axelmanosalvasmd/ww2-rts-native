use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    match_tick::TickEvent,
    recipient_snapshot::{DeliveryState, SelectedSeat},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Support interception", "w":128,"h":128,
            "rows":vec![".".repeat(128);128],
            "spawns":[{"x":8,"y":8},{"x":8,"y":120},{"x":120,"y":120}],
            "points":[{"x":64,"y":64,"mp":0,"vp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 701,
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
    for player in &mut state.start.economy.players {
        player.mp = 10000.;
    }
    state
}

fn publish(state: &mut Match, name: &str, owner: usize, at: Point) -> u32 {
    let id = state.next_unit_id;
    let mut staged = StagedUnit::ordinary(
        &state.world.game,
        &state.rules,
        owner,
        state.rules.kind(name).unwrap(),
        id,
        state.world.game.units.len() as u32,
        state.start.players[owner].spawn,
        Some(at),
    )
    .unwrap();
    staged.unit.hold_pos = true;
    staged.unit.still = Some(10.);
    prepare_publication(
        PublicationParts {
            units: &mut state.world.game.units,
            grid: &mut state.grid,
            controls: &mut state.commands.units,
            recruitment: &mut state.recruit_metadata,
            next_id: &mut state.next_unit_id,
        },
        &state.rules,
        state.start.players.len(),
        vec![staged],
    )
    .unwrap()
    .publish_with(|unit, _| {
        state.combat.initialize(unit.id);
        state.firing.initialize(unit.id);
        state.abilities.initialize(unit.id);
        state.recovery.initialize(unit.id);
    });
    let control = state.commands.units.get_mut(&id).unwrap();
    control.hold_fire = name != "flak";
    control.auto_cast = false;
    control.auto_retreat = false;
    id
}

fn delivered(
    state: &mut Match,
    delivery: &mut DeliveryState,
    report: &[TickEvent],
) -> Vec<serde_json::Value> {
    let selected = (0..state.start.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(state, &selected, report)
        .unwrap()
        .unwrap();
    let snapshots = selected
        .iter()
        .map(|seat| frame.recipient(seat.slot).unwrap().into_json())
        .collect();
    assert!(state.tick_state.fault().is_none());
    snapshots
}

#[test]
fn mov94_every_plane_flak_interception_delivers_only_remaining_payload_and_private_shooter() {
    let at = Point { x: 128., z: 128. };
    let direction = std::f64::consts::FRAC_PI_2;
    for kind in [
        SupportKind::Recon,
        SupportKind::Strafe,
        SupportKind::Bombing,
        SupportKind::Dive,
        SupportKind::Para,
    ] {
        let mut state = scene();
        publish(&mut state, "rifle", 0, Point { x: 110., z: 128. });
        let victim = publish(&mut state, "rifle", 2, at);
        let flak = publish(&mut state, "flak", 2, Point { x: 172., z: 128. });
        crate::vision::update_vision(&mut state.world.game, &state.rules, &state.grid);
        let hp = state.world.game.units[&victim].hp;
        let owned = state
            .world
            .game
            .units
            .values()
            .filter(|u| u.owner == 0)
            .count();
        // Native streams need no shared draw order. Choose a reproducible hit with a nonempty partial payload.
        let (random, fraction) = (1..1024)
            .find_map(|seed| {
                let random = crate::random::MatchRandom::new(seed);
                let mut expected = random.clone();
                let hit = expected.next_fraction(StreamId::OffMap) < 0.35;
                let fraction = expected.next_fraction(StreamId::OffMap);
                (hit && fraction > 0.2 && fraction < 0.8).then_some((random, fraction))
            })
            .unwrap();
        state.start.random = random;
        let remaining = match kind {
            SupportKind::Recon => fraction * 15.,
            SupportKind::Bombing => (fraction * 6.).floor(),
            _ => 0.,
        };
        assert_eq!(
            state
                .admit_support(0, kind.name(), at, Some(direction))
                .unwrap(),
            None
        );
        let mut delivery = DeliveryState::new(&state).unwrap();
        let mut arrivals = 0;
        let mut downs = 0;
        let mut bombs = 0;
        let mut chutes = 0;
        let mut intercepted_tick = None;
        let mut retired_tick = None;
        for tick in 0..500 {
            let report = state.advance_tick().unwrap();
            for event in &report.events {
                match event {
                    TickEvent::Support(SupportEvent::Plane { kind: actual, .. }) => {
                        assert_eq!(*actual, kind);
                        arrivals += 1;
                    }
                    TickEvent::Support(SupportEvent::ShotDown {
                        kind: actual,
                        fighters,
                        ..
                    }) => {
                        assert_eq!(*actual, kind);
                        assert!(!fighters);
                        downs += 1;
                        intercepted_tick = Some(tick);
                        if remaining > 0. {
                            assert_eq!(
                                state.world.game.strikes[0].runtime.as_ref().unwrap().left,
                                remaining
                            );
                        }
                    }
                    TickEvent::Support(SupportEvent::Bomb(_)) => bombs += 1,
                    TickEvent::Support(SupportEvent::Chutes(_)) => chutes += 1,
                    _ => {}
                }
            }
            let snapshots = delivered(&mut state, &mut delivery, &report.events);
            for (slot, snapshot) in snapshots.iter().enumerate() {
                let shots = snapshot["shots"].as_array().unwrap();
                for (name, expected) in [
                    (
                        kind.name(),
                        report
                            .events
                            .iter()
                            .filter(|event| {
                                matches!(event, TickEvent::Support(SupportEvent::Plane { .. }))
                            })
                            .count(),
                    ),
                    (
                        "shotdown",
                        report
                            .events
                            .iter()
                            .filter(|event| {
                                matches!(event, TickEvent::Support(SupportEvent::ShotDown { .. }))
                            })
                            .count(),
                    ),
                    (
                        "flak",
                        report
                            .events
                            .iter()
                            .filter(|event| {
                                matches!(event, TickEvent::Support(SupportEvent::Flak { .. }))
                            })
                            .count(),
                    ),
                ] {
                    assert_eq!(
                        shots.iter().filter(|shot| shot["k"] == name).count(),
                        expected
                    );
                }
                for shot in shots {
                    if shot["k"] == kind.name() {
                        assert_eq!(
                            *shot,
                            json!({"k":kind.name(),"x":at.x,"z":at.z,"dir":direction,"fo":0,"pub":true})
                        );
                    } else if shot["k"] == "shotdown" {
                        assert_eq!(
                            *shot,
                            json!({"k":"shotdown","x":at.x,"z":at.z,"dir":direction,"kind":kind.name(),"by":"flak","pub":true})
                        );
                    } else if shot["k"] == "flak" {
                        assert_eq!(shot["fo"], 2);
                        assert_eq!(shot["pub"], true);
                        if slot == 2 {
                            assert_eq!(shot["f"], flak);
                        } else {
                            assert!(
                                shot.get("f").is_none(),
                                "a public arrival must not reveal the hidden flak identity"
                            );
                        }
                    }
                }
            }
            if state.world.game.strikes.is_empty() {
                retired_tick.get_or_insert(tick);
                if tick >= retired_tick.unwrap() + 4 {
                    break;
                }
            }
        }
        assert_eq!(arrivals, 1);
        assert_eq!(downs, 1);
        assert_eq!(state.story[2].planes_downed, 1.);
        assert_eq!(chutes, 0);
        assert_eq!(
            bombs,
            if kind == SupportKind::Bombing {
                remaining as usize
            } else {
                0
            }
        );
        if kind == SupportKind::Recon {
            let lifetime = retired_tick.unwrap() - intercepted_tick.unwrap();
            assert!(
                lifetime > 0 && lifetime < 300,
                "downed recon must expire before its full 15 seconds"
            );
            assert!((lifetime as f64 * 0.05 - remaining).abs() < 0.051);
        }
        if matches!(
            kind,
            SupportKind::Strafe | SupportKind::Dive | SupportKind::Para
        ) {
            assert_eq!(state.world.game.units[&victim].hp, hp);
            assert_eq!(
                state
                    .world
                    .game
                    .units
                    .values()
                    .filter(|u| u.owner == 0)
                    .count(),
                owned
            );
        }
        assert!(retired_tick.is_some());
        assert!(state.world.game.strikes.is_empty());
        assert!(state.tick_state.fault().is_none());
    }
}

#[test]
fn mov94_cover_exempts_recon_then_intercepts_only_one_same_tick_plane() {
    let mut state = scene();
    let at = Point { x: 128., z: 128. };
    let victim = publish(&mut state, "rifle", 2, at);
    let hp = state.world.game.units[&victim].hp;
    assert_eq!(state.admit_support(2, "cover", at, None).unwrap(), None);
    assert_eq!(state.admit_support(0, "recon", at, Some(0.)).unwrap(), None);
    let mut delivery = DeliveryState::new(&state).unwrap();
    let mut saw_recon = false;
    for _ in 0..65 {
        let report = state.advance_tick().unwrap();
        saw_recon |= report.events.iter().any(|event| {
            matches!(
                event,
                TickEvent::Support(SupportEvent::Plane {
                    kind: SupportKind::Recon,
                    ..
                })
            )
        });
        assert!(
            !report
                .events
                .iter()
                .any(|event| matches!(event, TickEvent::Support(SupportEvent::ShotDown { .. })))
        );
        delivered(&mut state, &mut delivery, &report.events);
    }
    assert!(saw_recon);
    assert!(
        state
            .world
            .game
            .strikes
            .iter()
            .any(|s| s.live && s.kind == StrikeKind::Recon)
    );
    assert_eq!(state.support.covers.len(), 1);
    assert!(state.support.covers[0].remaining > 0.);
    assert_eq!(state.admit_support(0, "dive", at, Some(0.)).unwrap(), None);
    assert_eq!(
        state.admit_support(0, "strafe", at, Some(0.)).unwrap(),
        None
    );
    let mut downs = 0;
    let mut arrivals = 0;
    for _ in 0..400 {
        let report = state.advance_tick().unwrap();
        let mut simultaneous = 0;
        for event in &report.events {
            match event {
                TickEvent::Support(SupportEvent::Plane {
                    kind: SupportKind::Dive | SupportKind::Strafe,
                    ..
                }) => {
                    arrivals += 1;
                    simultaneous += 1;
                }
                TickEvent::Support(SupportEvent::ShotDown { kind, fighters, .. }) => {
                    assert_eq!(*kind, SupportKind::Dive);
                    assert!(*fighters);
                    downs += 1;
                }
                TickEvent::Support(SupportEvent::Bomb(_)) => {
                    panic!("the intercepted dive must not deliver a bomb")
                }
                _ => {}
            }
        }
        if simultaneous > 0 {
            assert_eq!(simultaneous, 2);
        }
        let snapshots = delivered(&mut state, &mut delivery, &report.events);
        if simultaneous > 0 {
            for snapshot in snapshots {
                let down = snapshot["shots"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|shot| shot["k"] == "shotdown")
                    .collect::<Vec<_>>();
                assert_eq!(
                    down,
                    vec![
                        &json!({"k":"shotdown","x":at.x,"z":at.z,"dir":0.,"kind":"dive","by":"fighters","pub":true})
                    ]
                );
            }
        }
    }
    assert_eq!(arrivals, 2);
    assert_eq!(downs, 1);
    assert!(
        state.world.game.units[&victim].hp < hp,
        "the second same-tick arrival must deliver its strafe"
    );
    assert!(state.support.covers.is_empty());
    assert!(state.world.game.strikes.is_empty());
    assert!(state.tick_state.fault().is_none());
}
