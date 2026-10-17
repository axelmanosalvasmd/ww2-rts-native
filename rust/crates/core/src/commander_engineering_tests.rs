use super::*;
use crate::{
    commander_ai::{Input, Level, PlayerObservation, PointObservation},
    commander_tactics::{Context, UnitFacts},
};
fn unit(id: u32, kind: &str, x: f64) -> UnitObservation {
    let d = Catalog::compiled().unwrap().unit(kind).unwrap();
    UnitObservation {
        id,
        kind: kind.into(),
        owner: 0,
        at: Point { x, z: 40.0 },
        hp: number(d, "models") * number(d, "hpPer"),
        supp: 0.0,
        target_id: 0,
        cover: 0,
        path: vec![],
        attack_id: false,
        retreating: false,
        riding: false,
        build: false,
        dig: false,
        entrench: false,
        garrison: false,
        enter: false,
        fire_at: false,
        nade: false,
    }
}
fn fixture() -> Input {
    let units = vec![unit(1, "rifle", 40.0)];
    Input {
        width: 80,
        height: 80,
        tick: 200,
        slot: 0,
        players: vec![
            PlayerObservation {
                team: 0,
                out: false,
                spawn: Point { x: 8.0, z: 8.0 },
            },
            PlayerObservation {
                team: 1,
                out: false,
                spawn: Point { x: 140.0, z: 40.0 },
            },
        ],
        points: vec![PointObservation {
            at: Point { x: 40.0, z: 40.0 },
            vp: 1.0,
            mp: 1.0,
            owner: Some(0),
            cut: false,
            locked: false,
        }],
        flags: vec![0; 6400],
        chars: vec![b'.'; 6400],
        manpower: 0.0,
        research: None,
        runtime: Some(Context {
            initial_chars: Some(vec![b'.'; 6400]),
            height: Some(vec![0; 6400]),
            initial_height: Some(vec![0; 6400]),
            units: BTreeMap::from([(
                1,
                UnitFacts {
                    built: 1.0,
                    ..UnitFacts::default()
                },
            )]),
            ..Context::default()
        }),
        units,
    }
}
fn run(i: Input, state: &mut crate::commander_tactics::State) -> Vec<Value> {
    crate::commander_tactics::plan(
        &Observation::new(i).unwrap(),
        &[],
        state,
        Level::Normal,
        &mut crate::random::MatchRandom::new(817),
    )
    .unwrap()
}
#[test]
fn ai13_point_holder_emits_mines_arc_strongpoint_wire_traps_and_garrison() {
    for (wallet, expected) in [(95.0, "entrench"), (600.0, "dig"), (0.0, "garrison")] {
        let mut i = fixture();
        i.manpower = wallet;
        if wallet == 0.0 {
            i.chars[20 * 80 + 21] = b'B';
        }
        let observation = Observation::new(i).unwrap();
        let command = holder(&observation, &observation.input().units[0], 0, false)
            .unwrap()
            .unwrap();
        assert_eq!(command["t"], expected, "wallet {wallet}: {command:?}");
        if wallet == 95.0 {
            assert_eq!(command["pattern"], "arc");
        }
    }
    for (pattern, armor) in [("strongpoint", false), ("wire", false), ("traps", true)] {
        let mut i = fixture();
        i.manpower = 600.0;
        i.runtime.as_mut().unwrap().mines =
            vec![Point { x: 31.0, z: 31.0 }, Point { x: 33.0, z: 31.0 }];
        if pattern != "strongpoint" {
            for cell in [
                20 * 80 + 20,
                20 * 80 + 21,
                20 * 80 + 22,
                21 * 80 + 20,
                21 * 80 + 21,
                21 * 80 + 22,
            ] {
                i.flags[cell] |= TRENCH;
            }
        }
        if pattern == "traps" {
            for cell in [20 * 80 + 29, 20 * 80 + 30, 21 * 80 + 29, 21 * 80 + 30] {
                i.chars[cell] = b'X';
            }
        }
        let observation = Observation::new(i).unwrap();
        let command = holder(&observation, &observation.input().units[0], 0, armor)
            .unwrap()
            .unwrap();
        assert_eq!(command["t"], "entrench");
        assert!(
            command["pattern"] == pattern || command["fort"] == pattern,
            "{command:?}"
        );
        crate::match_commands::Command::from_json(&command).unwrap();
    }
}
#[test]
fn ai13_repairs_emit_bridge_fill_demine_and_wait_for_observed_completion() {
    for name in ["bridge", "fill", "demine"] {
        let mut i = fixture();
        i.manpower = 1000.0;
        let cell = 20 * 80 + 23;
        match name {
            "bridge" => {
                i.runtime.as_mut().unwrap().initial_chars.as_mut().unwrap()[cell] = b'=';
                i.chars[cell] = b'W';
            }
            "fill" => {
                i.chars[cell] = b'F';
                i.runtime.as_mut().unwrap().height.as_mut().unwrap()[cell] = -1;
            }
            _ => i.runtime.as_mut().unwrap().found_mines = vec![at(cell, 80)],
        }
        let mut state = State::default();
        let first = repairs(
            &Observation::new(i.clone()).unwrap(),
            &mut state,
            &BTreeSet::new(),
            |_| true,
        )
        .unwrap();
        let command = first
            .commands
            .iter()
            .find(|c| c["t"] == "dig" && c["kind"] == name)
            .unwrap_or_else(|| panic!("{name}: {:?}", first.commands));
        crate::match_commands::Command::from_json(command).unwrap();
        assert_eq!(first.busy, BTreeSet::from([1]));
        assert!(!state.crew.is_empty());
        let expected = state.clone();
        state =
            crate::save_codec::from_value(crate::save_codec::to_value(&state).unwrap()).unwrap();
        assert_eq!(state, expected);
        assert!(
            repairs(
                &Observation::new(i.clone()).unwrap(),
                &mut state,
                &BTreeSet::new(),
                |_| true
            )
            .unwrap()
            .commands
            .is_empty()
        );
        i.units[0].dig = true;
        i.tick += 40;
        repairs(
            &Observation::new(i.clone()).unwrap(),
            &mut state,
            &BTreeSet::new(),
            |_| true,
        )
        .unwrap();
        i.units[0].dig = false;
        i.tick += 40;
        assert!(
            repairs(
                &Observation::new(i).unwrap(),
                &mut state,
                &BTreeSet::new(),
                |_| true
            )
            .unwrap()
            .commands
            .iter()
            .any(|c| c["kind"] == name)
        );
    }
}
#[test]
fn ai13_engineering_uses_only_delivered_damage_mines_and_safe_available_builders() {
    let base = fixture();
    assert!(
        repairs(
            &Observation::new(base.clone()).unwrap(),
            &mut Default::default(),
            &BTreeSet::new(),
            |_| true
        )
        .unwrap()
        .commands
        .is_empty()
    );
    for guard in ["enemy", "busy", "far", "wallet", "dig"] {
        let mut i = base.clone();
        i.manpower = 1000.0;
        i.runtime.as_mut().unwrap().found_mines = vec![Point { x: 45.0, z: 40.0 }];
        let mut busy = BTreeSet::new();
        match guard {
            "enemy" => {
                let mut enemy = unit(10, "rifle", 50.0);
                enemy.owner = 1;
                i.units.push(enemy);
            }
            "busy" => {
                busy.insert(1);
            }
            "far" => i.runtime.as_mut().unwrap().found_mines[0].x = 100.0,
            "wallet" => i.manpower = -1.0,
            _ => i.units[0].dig = true,
        }
        assert!(
            repairs(
                &Observation::new(i).unwrap(),
                &mut Default::default(),
                &busy,
                |_| true
            )
            .unwrap()
            .commands
            .is_empty(),
            "{guard}"
        );
    }
}

#[test]
fn ai13_emitted_holder_project_survives_canonical_queue_every_seat_delivery_and_finish() {
    use crate::{
        match_commands::Command,
        match_human_observation::HumanObservationMemory,
        orders::OrderKey,
        recipient_snapshot::{DeliveryState, SelectedSeat},
    };
    let mut state = crate::mov_audit_test_support::scene();
    state.start.win_vp = 3.0;
    state.start.economy.players[0].mp = 95.0;
    let point = state.start.points[0].at;
    state.start.points[0].owner = Some(0);
    let worker = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    state.world.game.units.get_mut(&worker).unwrap().x = point.x;
    state.world.game.units.get_mut(&worker).unwrap().z = point.z;
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
    }
    for unit in state.world.game.units.values_mut() {
        unit.hold_pos = true;
    }
    state.grid.rebuild(&state.world.game.units);
    let view = state
        .human_commander_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    let unit = view
        .packet
        .input()
        .units
        .iter()
        .find(|u| u.id == worker)
        .unwrap();
    let command = holder(&view.packet, unit, 0, false).unwrap().unwrap();
    assert_eq!(command["t"], "entrench");
    assert_eq!(command["pattern"], "arc");
    state
        .enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 1,
            },
            Command::from_json(&command).unwrap(),
        )
        .unwrap();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let selected: Vec<_> = (0..state.world.game.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect();
    let mut started = false;
    let mut terminal = 0;
    for _ in 0..1500 {
        let report = state.advance_tick().unwrap();
        if report.tick == 1 {
            assert!(
                report.commands.iter().all(|r| r.denial.is_none()),
                "{:?}",
                report.commands
            );
        }
        started |= state.field_works.projects().next().is_some();
        let frame = delivery
            .capture_selected_with_tick_events(&mut state, &selected, &report.events)
            .unwrap()
            .unwrap();
        for seat in &selected {
            frame.recipient(seat.slot).unwrap();
        }
        assert!(state.tick_state.fault().is_none());
        if state.finish.is_some() {
            terminal += 1;
            if terminal == 4 {
                break;
            }
        }
    }
    assert!(started);
    assert!(state.finish.is_some());
    assert_eq!(terminal, 4);
}

#[test]
fn ai13_unseen_repair_ground_gets_an_approach_without_a_phantom_job() {
    for name in ["bridge", "fill", "demine"] {
        let mut i = fixture();
        i.manpower = 1000.0;
        let cell = 20 * 80 + 28;
        match name {
            "bridge" => {
                i.runtime.as_mut().unwrap().initial_chars.as_mut().unwrap()[cell] = b'=';
                i.chars[cell] = b'W';
            }
            "fill" => {
                i.chars[cell] = b'F';
                i.runtime.as_mut().unwrap().height.as_mut().unwrap()[cell] = -1;
            }
            _ => i.runtime.as_mut().unwrap().found_mines = vec![at(cell, 80)],
        }
        let observation = Observation::new(i).unwrap();
        let mut state = State::default();
        let approach = repairs(&observation, &mut state, &BTreeSet::new(), |_| false).unwrap();
        assert_eq!(approach.commands.len(), 1, "{name}");
        assert_eq!(approach.commands[0]["t"], "move");
        assert!(state.crew.is_empty());
        let visible = repairs(&observation, &mut state, &BTreeSet::new(), |_| true).unwrap();
        assert!(visible.commands.iter().any(|c| c["kind"] == name));
    }
}

#[test]
fn ai13_research_and_supply_exclusions_precede_repair_and_combat_attempts() {
    use crate::{
        commander_ai::ResearchObservation,
        economy::{Faction, TechState},
    };
    let mut i = fixture();
    i.tick = 2400;
    i.manpower = 250.0;
    i.chars[20 * 80 + 23] = b'W';
    i.runtime.as_mut().unwrap().initial_chars.as_mut().unwrap()[20 * 80 + 23] = b'=';
    i.research = Some(ResearchObservation {
        faction: Faction::Usa,
        tech: TechState::new(1, 0, 0, 0).unwrap(),
        busy: vec![],
        stock_base: true,
        armory_ready: true,
        standard_army: true,
    });
    let commands = run(i, &mut Default::default());
    assert!(
        commands
            .iter()
            .any(|c| c["t"] == "tech" && c["kind"] == "tier"),
        "{commands:?}"
    );
    assert!(!commands.iter().any(|c| c["t"] == "dig"));
    for forced in [false, true] {
        let mut i = fixture();
        i.manpower = 1000.0;
        i.units[0].target_id = 2;
        let mut enemy = unit(2, "mg", 45.0);
        enemy.owner = 1;
        i.units.push(enemy);
        i.runtime.as_mut().unwrap().found_mines = vec![Point { x: 45.0, z: 40.0 }];
        i.runtime.as_mut().unwrap().units.get_mut(&1).unwrap().stock =
            Some(crate::commander_tactics::Stock {
                ammo: Some(0.0),
                provisions: 0.0,
                fuel: Some(0.0),
                forced,
            });
        let mut state = Default::default();
        run(i.clone(), &mut state);
        i.tick += 80;
        let commands = run(i, &mut state);
        assert!(
            !commands.iter().any(|c| matches!(
                c["t"].as_str(),
                Some("dig" | "ability" | "attack" | "fireat")
            )),
            "{commands:?}"
        );
    }
}
#[test]
fn ai13_traps_keep_the_first_visible_vehicle_lesson_after_contacts_expire() {
    let mut i = fixture();
    let mut enemy = unit(2, "halftrack", 100.0);
    enemy.owner = 1;
    i.units.push(enemy);
    let mut state = Default::default();
    run(i.clone(), &mut state);
    assert!(state.armor_ever_seen);
    i.units.pop();
    i.tick += 2000;
    run(i, &mut state);
    assert!(state.armor_ever_seen);
}

#[test]
fn ai13_repair_stamp_requires_visible_neighbors_as_well_as_its_center() {
    for name in ["bridge", "fill", "demine"] {
        let mut i = fixture();
        i.manpower = 1000.0;
        let cell = 20 * 80 + 28;
        match name {
            "bridge" => {
                i.runtime.as_mut().unwrap().initial_chars.as_mut().unwrap()[cell] = b'=';
                i.chars[cell] = b'W';
            }
            "fill" => {
                i.chars[cell] = b'F';
                i.runtime.as_mut().unwrap().height.as_mut().unwrap()[cell] = -1;
            }
            _ => i.runtime.as_mut().unwrap().found_mines = vec![at(cell, 80)],
        }
        let observation = Observation::new(i).unwrap();
        let mut state = State::default();
        let center = at(cell, 80);
        let plan = repairs(&observation, &mut state, &BTreeSet::new(), |point| {
            distance(point, center) < 0.5
        })
        .unwrap();
        assert_eq!(plan.commands[0]["t"], "move", "{name}: {:?}", plan.commands);
        assert!(state.crew.is_empty());
    }
}

#[test]
fn ai13_bridge_worker_is_nearest_the_complete_repair_center() {
    let mut i = fixture();
    i.manpower = 1000.0;
    let gap = 20 * 80 + 28;
    i.runtime.as_mut().unwrap().initial_chars.as_mut().unwrap()[gap] = b'=';
    for x in 28..=32 {
        i.chars[20 * 80 + x] = b'W';
    }
    for z in 16..=24 {
        i.chars[z * 80 + 28] = b'W';
    }
    i.units[0].at = Point { x: 55.0, z: 41.0 };
    let mut second = unit(2, "rifle", 64.0);
    second.at.z = 41.0;
    i.units.push(second);
    let plan = repairs(
        &Observation::new(i).unwrap(),
        &mut Default::default(),
        &BTreeSet::new(),
        |_| true,
    )
    .unwrap();
    assert_eq!(plan.commands[0]["ids"], json!([2]));
    assert_eq!(plan.commands[0]["x"], 61.0);
}
