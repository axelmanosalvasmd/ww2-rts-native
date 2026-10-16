//! AI-64 uses real mode constructors and complete recipient deliveries.
use crate::{
    ai_view::{SeatView, commander::CommanderMemory},
    catalog::Catalog,
    commander_ai::Level,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    horde_policy::HordePolicy,
    human_commander::ScreenView,
    human_commander_policy::HumanPolicy,
    map::AuthoritativeMap,
    match_commands::{Command, MoveOrder, UnitControl},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    unit_spawn, vision,
    weather::WeatherChoice,
};
use serde_json::{Value, json};

struct Scene {
    state: Match,
    own: u32,
    visible: u32,
    hidden: u32,
    hidden_at: Point,
    foe: usize,
}
fn actor(state: &mut Match, owner: usize, kind: &str, at: Point) -> u32 {
    let id = state.next_unit_id;
    state.next_unit_id += 1;
    let unit = unit_spawn::initial_unit(
        &state.rules,
        Catalog::compiled().unwrap(),
        state.rules.kind(kind).unwrap(),
        id,
        owner,
        at,
    )
    .unwrap();
    state
        .commands
        .units
        .insert(id, UnitControl::new(&unit, &state.rules).unwrap());
    state.firing.initialize(id);
    state.combat.initialize(id);
    state.abilities.initialize(id);
    state.recovery.initialize(id);
    state.world.game.units.insert(id, unit);
    id
}
fn refresh(state: &mut Match, tick: i64) {
    state.grid.rebuild(&state.world.game.units);
    state.world.game.tick = tick - (tick - 1) % 4;
    vision::update_vision(&mut state.world.game, &state.rules, &state.grid);
    state.world.game.tick = tick;
}
fn scene(mode: Mode, houses: bool, seed: u64, slot: usize, tick: i64) -> Scene {
    let mut rows = vec![".".repeat(80); 80];
    if houses {
        rows[20] = format!("{}B{}", ".".repeat(26), ".".repeat(53));
    }
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"AI recipient mode privacy", "w":80,"h":80,"rows":rows,
            "spawns":[{"x":40,"y":5},{"x":10,"y":74},{"x":70,"y":74}],
            "defend":[0],"points":[{"x":40,"y":30},{"x":35,"y":40}]
        }))
        .unwrap(),
    )
    .unwrap();
    let options = MatchOptions {
        seed,
        mode,
        players: (0..if mode == Mode::Horde { 1 } else { 2 })
            .map(|slot| PlayerConfig {
                name: format!("Seat {slot}"),
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
        tech: false,
        supply: false,
        logistics: false,
        sandbox: false,
    };
    let mut state = match mode {
        Mode::Conquest => Match::new(&map, options),
        Mode::Classic => Match::new_classic(&map, options),
        Mode::Assault => Match::new_assault(&map, options, 0),
        Mode::Horde => Match::new_horde(&map, options),
        _ => unreachable!(),
    }
    .unwrap();
    if mode == Mode::Horde {
        assert!(state.end_horde_break().unwrap());
        for _ in 0..16 {
            state.advance_tick().unwrap();
        }
        assert!(state.horde.as_ref().unwrap().wave > 0);
    }
    let foe = 1 - slot;
    let own = actor(&mut state, slot, "rifle", Point { x: 30.0, z: 30.0 });
    actor(&mut state, slot, "engineer", Point { x: 32.0, z: 30.0 });
    let visible = actor(
        &mut state,
        foe,
        "rifle",
        Point {
            x: 45.011,
            z: 30.011,
        },
    );
    state.world.game.units.get_mut(&visible).unwrap().hp -= 0.1;
    let hidden_at = [
        Point { x: 10.0, z: 10.0 },
        Point { x: 148.0, z: 10.0 },
        Point { x: 10.0, z: 148.0 },
        Point { x: 148.0, z: 148.0 },
    ]
    .into_iter()
    .max_by(|a, b| {
        let distance = |p: Point| {
            state
                .world
                .game
                .units
                .values()
                .filter(|u| u.owner == slot)
                .map(|u| (u.x - p.x) * (u.x - p.x) + (u.z - p.z) * (u.z - p.z))
                .fold(f64::INFINITY, f64::min)
        };
        distance(*a).total_cmp(&distance(*b))
    })
    .unwrap();
    let hidden = actor(&mut state, foe, "rifle", hidden_at);
    state.start.weather.next = Some(crate::weather::WeatherKind::Fog);
    state.start.weather.at = 100_000;
    refresh(&mut state, tick);
    Scene {
        state,
        own,
        visible,
        hidden,
        hidden_at,
        foe,
    }
}
fn snapshot(state: &mut Match, slot: usize) -> Value {
    DeliveryState::new(state)
        .unwrap()
        .capture_selected_with_lists(
            state,
            &[SelectedSeat {
                slot,
                full: true,
                omit: true,
            }],
            &[],
        )
        .unwrap()
        .unwrap()
        .recipient(slot)
        .unwrap()
        .into_json()
}
fn perturb(scene: &mut Scene, name: &str) {
    let Scene {
        state,
        visible,
        hidden,
        hidden_at,
        foe,
        ..
    } = scene;
    match name {
        "enemy_economy" => {
            let p = &mut state.start.economy.players[*foe];
            p.mp += 12345.67;
            p.mun += 891.2;
            p.fuel += 765.4;
            for cd in state.start.players[*foe].support.values_mut() {
                *cd += 97.0;
            }
        }
        "hidden_army" => {
            let u = state.world.game.units.get_mut(hidden).unwrap();
            u.hp = 1.0;
            u.still = Some(1000.0);
            u.path = vec![Point { x: 90.0, z: 90.0 }];
            state.commands.units.get_mut(hidden).unwrap().attack_id = 999998;
            state.firing.units.get_mut(hidden).unwrap().target_id = 999999;
        }
        "added_tank" => {
            actor(state, *foe, "tank", *hidden_at);
        }
        "secret_depot" => {
            actor(state, *foe, "depot", *hidden_at);
        }
        "visible_private" => {
            let u = state.world.game.units.get_mut(visible).unwrap();
            u.path = vec![Point { x: 90.0, z: 90.0 }];
            u.still = Some(1000.0);
            u.hold_pos = !u.hold_pos;
            let c = state.commands.units.get_mut(visible).unwrap();
            c.hold_fire = !c.hold_fire;
            c.auto_retreat = !c.auto_retreat;
            c.orders.push_back(Command::Move {
                orders: vec![MoveOrder {
                    unit: i64::from(*visible),
                    at: *hidden_at,
                }],
                control_ids: None,
                attack_move: false,
                queue: false,
                together: false,
                face: None,
            });
            state.abilities.get_mut(*visible).unwrap().cooldown = 999.0;
        }
        "wire_precision" => {
            let u = state.world.game.units.get_mut(visible).unwrap();
            u.x = 45.039;
            u.z = 30.039;
            u.hp = u.hp.ceil() - 0.45;
        }
        "hidden_footprint" => {
            let c = (hidden_at.z / 2.0) as usize * 80 + (hidden_at.x / 2.0) as usize;
            state.world.set_cell(c, b'R').unwrap();
        }
        "future_weather" => {
            state.start.weather.next = Some(crate::weather::WeatherKind::Clear);
            state.start.weather.at = 200_000;
        }
        "hidden_mines" => {
            for (dx, ch) in [(0, b'.'), (1, b'M'), (2, b'D')] {
                let c = (hidden_at.z / 2.0) as usize * 80 + (hidden_at.x / 2.0) as usize + dx;
                state.world.set_cell(c, ch).unwrap();
                state.world.set_cell(c, b'N').unwrap();
                state.world.mines.insert(c, *foe as i32);
            }
        }
        _ => unreachable!(),
    }
    state.grid.rebuild(&state.world.game.units);
}
fn view(state: &Match, slot: usize, snapshot: &Value) -> SeatView {
    state
        .ordinary_ai_view_memory(slot)
        .unwrap()
        .decode(snapshot)
        .unwrap()
}
fn commands(view: &SeatView, slot: usize, level: Level, seed: u64) -> Vec<Value> {
    if view.mode["kind"] == "horde" && view.mode["slot"].as_u64() == Some(slot as u64) {
        let (wave, facts) = view.horde_packet(slot).unwrap();
        let mut policy = HordePolicy::default();
        policy.observe(&wave, &facts).unwrap();
        return policy
            .plan(&wave, &facts)
            .unwrap()
            .iter()
            .map(|c| serde_json::to_value(c).unwrap())
            .collect();
    }
    let packet = CommanderMemory::new(slot, 80, 80)
        .unwrap()
        .observe(view)
        .unwrap();
    let context = packet.packet.input().runtime.as_ref().unwrap();
    assert!(!context.horde);
    assert_eq!(
        context.defender,
        view.mode["defenderTeam"].as_i64() == Some(packet.packet.input().players[slot].team.into())
    );
    HumanPolicy::new(slot as u32, seed)
        .plan(
            ScreenView {
                tick: packet.tick,
                spawn: packet.spawn,
                units: packet.units.iter().collect(),
                visible: packet.visible.clone(),
                sightings: packet.sightings.iter().collect(),
                packet: &packet.packet,
            },
            level,
        )
        .unwrap()
}
#[test]
fn ai64_all_modes_all_seats_and_levels_ignore_hidden_state_in_complete_recipient_frames() {
    let perturbations = [
        "enemy_economy",
        "hidden_army",
        "added_tank",
        "secret_depot",
        "visible_private",
        "wire_precision",
        "hidden_footprint",
        "future_weather",
        "hidden_mines",
    ];
    let mut comparisons = 0;
    let mut emitted = 0;
    let mut directors = 0;
    for (mode, houses) in [
        (Mode::Conquest, true),
        (Mode::Conquest, false),
        (Mode::Classic, true),
        (Mode::Assault, true),
        (Mode::Horde, true),
    ] {
        for seed in [617, 902] {
            for tick in [40, 120] {
                for slot in [0, 1] {
                    let mut original = scene(mode, houses, seed, slot, tick);
                    let expected = snapshot(&mut original.state, slot);
                    assert_eq!(
                        Mode::from_kind(Some(
                            expected["mode"]["kind"].as_str().unwrap_or("conquest")
                        )),
                        mode
                    );
                    let ids = expected["units"].as_array().unwrap();
                    assert!(ids.iter().any(|r| r[0] == original.visible));
                    assert!(!ids.iter().any(|r| r[0] == original.hidden));
                    let seat = view(&original.state, slot, &expected);
                    let levels = [Level::Easy, Level::Normal, Level::Hard];
                    let baseline = levels.map(|level| commands(&seat, slot, level, seed));
                    assert!(
                        baseline.iter().all(|c| !c.is_empty()),
                        "{mode:?} {slot}: {baseline:?}"
                    );
                    emitted += baseline.iter().map(Vec::len).sum::<usize>();
                    directors += usize::from(mode == Mode::Horde && slot == 1);
                    for name in perturbations {
                        let mut changed = scene(mode, houses, seed, slot, tick);
                        perturb(&mut changed, name);
                        let actual = snapshot(&mut changed.state, slot);
                        assert_eq!(
                            actual, expected,
                            "snapshot {mode:?} slot={slot} seed={seed} tick={tick} {name}"
                        );
                        let altered = view(&changed.state, slot, &actual);
                        assert_eq!(altered, seat);
                        for (level, expected_commands) in levels.into_iter().zip(&baseline) {
                            assert_eq!(
                                &commands(&altered, slot, level, seed),
                                expected_commands,
                                "policy {mode:?} slot={slot} {name} {level:?}"
                            );
                            comparisons += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 1080);
    assert!(emitted >= 120);
    assert_eq!(directors, 4);
}

#[test]
fn ai64_visible_own_health_wallet_and_horde_stance_change_actual_commands() {
    for mode in [Mode::Conquest, Mode::Classic, Mode::Assault, Mode::Horde] {
        for slot in [0, 1] {
            for level in [Level::Easy, Level::Normal, Level::Hard] {
                let mut original = scene(mode, true, 617, slot, 120);
                let normal = snapshot(&mut original.state, slot);
                let healthy = commands(&view(&original.state, slot, &normal), slot, level, 617);
                if mode == Mode::Horde && slot == 1 {
                    original
                        .state
                        .commands
                        .units
                        .get_mut(&original.own)
                        .unwrap()
                        .auto_retreat = false;
                } else {
                    original.state.start.economy.players[slot].mp = 0.0;
                    let u = original
                        .state
                        .world
                        .game
                        .units
                        .get_mut(&original.own)
                        .unwrap();
                    u.hp = 1.0;
                }
                let changed = snapshot(&mut original.state, slot);
                assert_ne!(normal, changed, "visible control {mode:?} {slot}");
                let altered = commands(&view(&original.state, slot, &changed), slot, level, 617);
                assert_ne!(healthy, altered, "policy control {mode:?} {slot} {level:?}");
            }
        }
    }
}

#[test]
fn ai64_real_mode_policy_orders_and_all_recipient_adapters_survive_ticks_and_ending() {
    use crate::{orders::OrderKey, recipient_snapshot::ShotRef};
    for mode in [Mode::Conquest, Mode::Classic, Mode::Assault, Mode::Horde] {
        let mut fixture = scene(mode, true, 617, 0, if mode == Mode::Horde { 16 } else { 0 });
        let state = &mut fixture.state;
        let first_tick = state.world.game.tick as u64 + 1;
        let seats = state.world.game.players.len();
        let selected = (0..seats)
            .map(|slot| SelectedSeat {
                slot,
                full: true,
                omit: false,
            })
            .collect::<Vec<_>>();
        let mut delivery = DeliveryState::new(state).unwrap();
        let mut memories = (0..seats)
            .map(|slot| state.ordinary_ai_view_memory(slot).unwrap())
            .collect::<Vec<_>>();
        let mut commanders = (0..seats)
            .map(|slot| CommanderMemory::new(slot, 80, 80).unwrap())
            .collect::<Vec<_>>();
        let mut horde_policy = HordePolicy::default();
        let mut submitted = 0;
        for (slot, memory) in memories.iter_mut().enumerate() {
            let packet = state.ordinary_ai_view(memory, &mut delivery, true).unwrap();
            let director = mode == Mode::Horde && slot == seats - 1;
            let attempts = commands(&packet, slot, Level::Hard, 617);
            for (sequence, attempt) in attempts.iter().enumerate() {
                let command = if director {
                    serde_json::from_value(attempt.clone()).unwrap()
                } else {
                    Command::from_json(attempt).unwrap()
                };
                state
                    .enqueue_command(
                        OrderKey {
                            tick: first_tick,
                            issuer: slot as u32,
                            sequence: sequence as u64,
                        },
                        command,
                    )
                    .unwrap();
                submitted += 1;
            }
        }
        assert!(submitted > 0);
        let mut consumed = 0;
        let mut accepted = 0;
        let mut ending = 0;
        for index in 0..100 {
            if index == 12 {
                match mode {
                    Mode::Conquest => state.start.economy.players[0].vp = state.start.win_vp,
                    Mode::Assault => state.assault.as_mut().unwrap().time_left = 0.01,
                    Mode::Classic => {
                        state.elimination.as_mut().unwrap().time_left = Some(0.01);
                        for u in state.world.game.units.values_mut() {
                            if u.owner != 0 && state.rules.kinds[u.kind].building {
                                u.hp = 0.0;
                            }
                        }
                    }
                    Mode::Horde => {
                        let bunker = state.horde.as_ref().unwrap().bunker;
                        state.world.game.units.get_mut(&bunker).unwrap().hp = 0.0;
                    }
                    _ => unreachable!(),
                }
            }
            let report = state.advance_tick().unwrap();
            consumed += report.commands.len();
            accepted += report
                .commands
                .iter()
                .filter(|c| c.denial.is_none())
                .count();
            let shots = report
                .events
                .iter()
                .map(ShotRef::try_from)
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            let finished = state.finish.is_some();
            let frame = delivery
                .capture_selected_with_shots(state, &selected, &shots)
                .unwrap()
                .unwrap();
            for slot in 0..seats {
                let raw = frame.recipient(slot).unwrap().into_json();
                assert_eq!(raw["tick"], report.tick);
                let seat = memories[slot].decode(&raw).unwrap();
                if !finished {
                    if mode == Mode::Horde && slot == seats - 1 {
                        let (wave, facts) = seat.horde_packet(slot).unwrap();
                        horde_policy.observe(&wave, &facts).unwrap();
                        horde_policy.plan(&wave, &facts).unwrap();
                    } else {
                        commanders[slot].observe(&seat).unwrap();
                    }
                }
            }
            if state.finish.is_some() {
                ending += 1;
            }
            if ending >= 61 {
                break;
            }
        }
        assert_eq!(consumed, submitted, "{mode:?}");
        assert!(
            accepted > 0,
            "at least one policy command must actuate in {mode:?}"
        );
        assert!(ending >= 60, "{mode:?} ending={ending}");
        assert_eq!(
            state.world.game.tick,
            state.finish.as_ref().unwrap().end_tick + 60
        );
        assert!(state.tick_state.fault().is_none());
    }
}
