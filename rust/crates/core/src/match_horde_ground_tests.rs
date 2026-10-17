use super::*;
use crate::{
    match_commands::{Command, Denial},
    recipient_snapshot::{DeliveryState, SelectedSeat, SnapshotCache},
    weather::WeatherChoice,
};
use serde_json::{Value as Json, json};

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Horde ground delivery", "w":60,"h":60,"rows":vec![".".repeat(60);60],
            "spawns":[{"x":30,"y":5},{"x":10,"y":54},{"x":50,"y":54}],
            "defend":[0], "points":[{"x":30,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new_horde(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Horde,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "B".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: true,
            supply: true,
            logistics: true,
            sandbox: false,
        },
    )
    .unwrap()
}

fn snapshots(
    delivery: &mut DeliveryState,
    state: &mut Match,
    events: &[crate::match_tick::TickEvent],
) -> Vec<Json> {
    let selected = (0..state.world.game.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: true,
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
fn base18_base59_horde_allied_producer_recruitment_and_exact_private_queue_rows() {
    let mut state = scene();
    let producer = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "barracks")
        .unwrap()
        .id;
    let at = state.world.game.units[&producer].at();
    let rally = Point {
        x: at.x + 16.0,
        z: at.z + 12.0,
    };
    assert_eq!(
        state
            .execute_command(
                1,
                &Command::Rally {
                    ids: vec![i64::from(producer)],
                    id: None,
                    at: Some(rally)
                },
                false
            )
            .unwrap(),
        None
    );
    let before = state.start.economy.players[1].mp;
    let buy = state.recorded_buy(1, "rifle", Some(producer)).unwrap();
    let price = crate::economy::price_of(Mode::Horde, "rifle").unwrap().mp;
    assert_eq!(state.start.economy.players[1].mp, before - price);
    assert_eq!(buy.purchase.request.building, producer);
    assert_eq!(state.world.game.units[&buy.unit].owner, 1);
    assert!(!state.world.game.units[&buy.unit].path.is_empty());
    assert!(
        state
            .production
            .building(producer)
            .unwrap()
            .jobs()
            .is_empty()
    );
    let cache = SnapshotCache::new(&state).unwrap();
    let mut expected = Vec::new();
    for u in state.world.game.units.values() {
        if state.production.building(u.id).is_some() && u.owner != 2 {
            let rally = state.building_rallies[&u.id];
            expected.push(json!([
                u.id,
                0.0,
                rally.map_or(-1.0, |p| math::round(p.x * 10.0) / 10.0),
                rally.map_or(-1.0, |p| math::round(p.z * 10.0) / 10.0)
            ]));
        }
    }
    for slot in 0..3 {
        let projection = cache.recipient(slot).unwrap();
        assert_eq!(
            projection.fields["queues"],
            if slot == 2 {
                json!([])
            } else {
                json!(expected)
            }
        );
        let jobs = state
            .world
            .game
            .units
            .values()
            .filter(|u| u.owner == slot && state.production.building(u.id).is_some())
            .map(|u| json!([u.id, []]))
            .collect::<Vec<_>>();
        assert_eq!(projection.fields["productionJobs"], json!(jobs));
        assert_eq!(
            projection.fields["mode"],
            crate::recipient_snapshot::horde_mode_row(&state).unwrap()
        );
    }
    drop(cache);
    state.start.economy.players[2].mp = 1_000.0;
    let before = state.start.economy.players[2].mp;
    assert_eq!(
        state
            .execute_command(
                2,
                &Command::Buy {
                    kind: "rifle".into(),
                    from: Some(producer)
                },
                false
            )
            .unwrap(),
        Some(Denial::Needs)
    );
    assert_eq!(state.start.economy.players[2].mp, before);
}

#[test]
fn base18_base59_horde_ground_ticks_wave_bunker_defeat_and_every_seat_delivery() {
    let mut state = scene();
    let maker = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    let at = state.world.game.units[&maker].at();
    state.player_rallies[1] = Some(Point {
        x: at.x + 20.0,
        z: at.z + 20.0,
    });
    state
        .enqueue_command(
            crate::orders::OrderKey {
                tick: 1,
                issuer: 1,
                sequence: 0,
            },
            Command::Buy {
                kind: "rifle".into(),
                from: Some(maker),
            },
        )
        .unwrap();
    let mut delivery = DeliveryState::new(&state).unwrap();
    snapshots(&mut delivery, &mut state, &[]);
    state.end_horde_break().unwrap();
    for _ in 0..25 {
        let report = state.advance_tick().unwrap();
        assert!(report.phases.contains(&crate::match_tick::TickPhase::Horde));
        let rows = snapshots(&mut delivery, &mut state, &report.events);
        assert!(rows.iter().all(|row| row["winner"].is_null()));
        assert!(state.tick_state.fault().is_none());
    }
    assert_eq!(state.horde.as_ref().unwrap().wave, 1);
    assert!(state.world.game.units.values().any(|u| u.owner == 2));
    assert!(state.recruit_metadata.values().any(|m| m.building == maker));
    let bunker = state.horde.as_ref().unwrap().bunker;
    let at = state.world.game.units[&bunker].at();
    state.world.game.units.get_mut(&bunker).unwrap().hp = 0.0;
    let report = state.advance_tick().unwrap();
    let rows = snapshots(&mut delivery, &mut state, &report.events);
    assert!(rows.iter().all(|row| row["winner"] == 1
        && row["end"] == json!({"reason":"structures", "x":at.x, "z":at.z})));
    let result = state.horde_result().unwrap().unwrap();
    for _ in 0..8 {
        let report = state.advance_tick().unwrap();
        snapshots(&mut delivery, &mut state, &report.events);
        assert_eq!(state.horde_result().unwrap().unwrap(), result);
        assert!(state.tick_state.fault().is_none());
    }
    state.restrict_to_ordinary_snapshots().unwrap();
}

#[test]
fn horde_boss_surface_event_matches_prototype_for_every_seat_and_saved_batch() {
    let mut state = scene();
    // Begin the boss-wave phase through its canonical owner. This fixture
    // skips earlier waves without changing the wave's spawn or event path.
    let horde = state.horde.as_mut().unwrap();
    horde.wave = 9;
    horde.time_left = 0.0;
    horde.profile_wave = 0;
    let gate = horde.gates[0];
    let events = state
        .step_horde(0.05)
        .unwrap()
        .into_iter()
        .map(crate::match_tick::TickEvent::Horde)
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    let saved = Match::save_events_json(&events).unwrap();
    let restored = Match::events_from_save_json(saved.clone()).unwrap();
    assert_eq!(Match::save_events_json(&restored).unwrap(), saved);
    assert!(
        matches!(&restored[0], crate::match_tick::TickEvent::Horde(HordeEvent::BossSurfaced { at }) if *at == gate)
    );
    let mut delivery = DeliveryState::new(&state).unwrap();
    let rows = snapshots(&mut delivery, &mut state, &restored);
    for row in rows {
        // Prototype sim.js:939. No scenario metadata or recipient-only keys.
        assert_eq!(
            row["shots"],
            json!([{
                "k":"say", "text":"THE KAIJU HAS SURFACED",
                "x":gate.x, "z":gate.z, "pub":true
            }])
        );
    }
}

#[test]
fn horde_ground_breath_ticks_publish_exact_public_rows_and_redact_retired_identities() {
    use crate::{match_firing::LaunchEvent, match_tick::TickEvent};
    let mut state = scene();
    let owner = state.horde.as_ref().unwrap().slot;
    let boss = state
        .spawn_horde_ground(
            owner,
            "kaiju",
            Point { x: 61.0, z: 61.0 },
            Point { x: 71.0, z: 61.0 },
            0,
            0.0,
        )
        .unwrap();
    let target = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    for (id, at) in [
        (boss, Point { x: 61.0, z: 61.0 }),
        (target, Point { x: 71.0, z: 61.0 }),
    ] {
        let unit = state.world.game.units.get_mut(&id).unwrap();
        unit.x = at.x;
        unit.z = at.z;
        unit.rot = 0.0;
        unit.path.clear();
        unit.amove = None;
        state.grid.update(id, unit);
    }
    // Keep other ground weapons quiet so this frame isolates the beam's
    // immediate damage and ephemeral shots, without projectile flights.
    for (&id, control) in &mut state.commands.units {
        control.hold_fire = id != boss;
    }
    state.commands.units.get_mut(&boss).unwrap().attack_id = target;
    state.firing.get_mut(boss).unwrap().target_id = target;
    let hp = state.world.game.units[&target].hp;
    let mut delivery = DeliveryState::new(&state).unwrap();
    let report = state.advance_tick().unwrap();
    assert!(state.world.game.units[&target].hp < hp);
    assert!(
        report
            .events
            .iter()
            .any(|event| matches!(event, TickEvent::Firing(LaunchEvent::Breath { .. })))
    );
    assert_eq!(
        report
            .events
            .iter()
            .filter(|event| matches!(event, TickEvent::Firing(LaunchEvent::BreathShell { .. })))
            .count(),
        4
    );
    // Prototype sim.js:4373-4374. Beam range is 34 metres; shell samples
    // lie every eight metres. No launch position or other fields are added.
    let expected = vec![
        json!({"f":boss,"t":target,"fo":owner,"to":0,"x":95.0,"z":61.0,"hit":true,"k":"kaiju","pub":true}),
        json!({"k":"shell","x":69.0,"z":61.0,"pub":true}),
        json!({"k":"shell","x":77.0,"z":61.0,"pub":true}),
        json!({"k":"shell","x":85.0,"z":61.0,"pub":true}),
        json!({"k":"shell","x":93.0,"z":61.0,"pub":true}),
    ];
    for row in snapshots(&mut delivery, &mut state, &report.events) {
        let breath = row["shots"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|shot| shot["k"] == "kaiju" || shot["k"] == "shell")
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(breath, expected);
    }
    // Delayed public effects survive ordinary retirement. With neither
    // actor visible, Source removes f/t and retains the original fo/to.
    state.world.game.units.get_mut(&boss).unwrap().hp = 0.0;
    state.world.game.units.get_mut(&target).unwrap().hp = 0.0;
    let retired = state.advance_tick().unwrap();
    assert!(!state.world.game.units.contains_key(&boss));
    assert!(!state.world.game.units.contains_key(&target));
    state.refresh_horde_visibility().unwrap();
    let mut delayed = report.events;
    delayed.extend(retired.events);
    let mut hidden = expected;
    hidden[0] = json!({"fo":owner,"to":0,"x":95.0,"z":61.0,"hit":true,"k":"kaiju","pub":true});
    for row in snapshots(&mut delivery, &mut state, &delayed) {
        let breath = row["shots"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|shot| shot["k"] == "kaiju" || shot["k"] == "shell")
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(breath, hidden);
    }
    for _ in 0..3 {
        let report = state.advance_tick().unwrap();
        snapshots(&mut delivery, &mut state, &report.events);
        assert!(state.tick_state.fault().is_none());
    }
    assert!(state.aviation.planes.is_empty());
    state.restrict_to_ordinary_snapshots().unwrap();
}
