//! AI-13's ordinary command union, emitted by the seat planner from delivered facts.
//! World-only rail, board and unload remain deferred (shared/ai.js:630-710).
//! Recover is also World-only (shared/ai.js:434).
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::{
        Input, Level, Observation, PlayerObservation, Point, PointObservation, ResearchObservation,
        UnitObservation,
    },
    commander_tactics::{self, Context, State, UnitFacts},
    economy::{Faction, TechState},
    game::TRENCH,
    match_commands::Command,
    random::MatchRandom,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn unit(id: u32, kind: &str, owner: usize, x: f64, z: f64) -> UnitObservation {
    let d = Catalog::compiled().unwrap().unit(kind).unwrap();
    UnitObservation {
        id,
        kind: kind.into(),
        owner,
        at: Point { x, z },
        hp: d.get("models").and_then(Rule::number).unwrap()
            * d.get("hpPer").and_then(Rule::number).unwrap(),
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
fn fixture(mut units: Vec<UnitObservation>) -> Input {
    units.sort_by_key(|u| u.id);
    let facts = units
        .iter()
        .map(|u| {
            (
                u.id,
                UnitFacts {
                    built: 1.0,
                    ..UnitFacts::default()
                },
            )
        })
        .collect();
    Input {
        width: 80,
        height: 80,
        tick: 200,
        slot: 0,
        players: [Point { x: 8.0, z: 8.0 }, Point { x: 140.0, z: 40.0 }]
            .into_iter()
            .zip([0, 1])
            .map(|(spawn, team)| PlayerObservation {
                team,
                out: false,
                spawn,
            })
            .collect(),
        units,
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
            ground_flights: true,
            initial_chars: Some(vec![b'.'; 6400]),
            height: Some(vec![0; 6400]),
            initial_height: Some(vec![0; 6400]),
            units: facts,
            ..Context::default()
        }),
    }
}
fn look(mut input: Input, state: &mut State, level: Level) -> Vec<Value> {
    input.units.sort_by_key(|u| u.id);
    commander_tactics::plan(
        &Observation::new(input).unwrap(),
        &[],
        state,
        level,
        &mut MatchRandom::new(813),
    )
    .unwrap()
}
fn emitted<'a>(commands: &'a [Value], kind: &str) -> &'a Value {
    let command = commands
        .iter()
        .find(|c| c["t"] == kind)
        .unwrap_or_else(|| panic!("missing {kind}: {commands:?}"));
    Command::from_json(command).unwrap();
    command
}
fn complete_base(input: &mut Input) {
    for (id, kind) in [
        (60000, "hq"),
        (60001, "barracks"),
        (60002, "motorpool"),
        (60003, "airfield"),
    ] {
        input.units.push(unit(id, kind, 0, 8.0, 8.0));
        input.runtime.as_mut().unwrap().units.insert(
            id,
            UnitFacts {
                built: 1.0,
                ..UnitFacts::default()
            },
        );
    }
}
fn protect_wallet(input: &mut Input) {
    for id in [59000, 59001] {
        input.units.push(unit(id, "medic", 0, 8.0, 8.0));
        input.runtime.as_mut().unwrap().units.insert(
            id,
            UnitFacts {
                built: 1.0,
                ..UnitFacts::default()
            },
        );
    }
    input.runtime.as_mut().unwrap().build_reserve = 1_000_000.0;
}
fn shopping_outputs() -> Vec<Value> {
    let mut input = fixture(vec![unit(1, "rifle", 0, 40.0, 40.0)]);
    complete_base(&mut input);
    input.manpower = 1000.0;
    let commands = look(input, &mut State::default(), Level::Normal);
    emitted(&commands, "buy");
    commands
}
fn support_outputs() -> Vec<Value> {
    let mut input = fixture(
        (1..=6)
            .map(|id| unit(id, "rifle", 0, 25.0 + f64::from(id), 40.0))
            .collect(),
    );
    complete_base(&mut input);
    input.points[0].at.x = 60.0;
    input.points[0].owner = Some(1);
    input.manpower = 1000.0;
    input.runtime.as_mut().unwrap().build_reserve = 1_000_000.0;
    input
        .runtime
        .as_mut()
        .unwrap()
        .support
        .insert("para".into(), 0.0);
    let mut state = State::default();
    look(input.clone(), &mut state, Level::Normal);
    input.tick += 25;
    let commands = look(input, &mut state, Level::Normal);
    assert_eq!(emitted(&commands, "support")["kind"], "para");
    commands
}
fn construction_outputs(assist: bool) -> Vec<Value> {
    let mut input = fixture(vec![
        unit(1, "rifle", 0, 40.0, 40.0),
        unit(10, "hq", 0, 35.0, 40.0),
    ]);
    input.points.clear();
    input.manpower = 1000.0;
    if assist {
        input
            .runtime
            .as_mut()
            .unwrap()
            .units
            .get_mut(&10)
            .unwrap()
            .built = 0.5;
    }
    let commands = look(input, &mut State::default(), Level::Normal);
    emitted(&commands, if assist { "assist" } else { "build" });
    commands
}
fn tech_outputs() -> Vec<Value> {
    let mut input = fixture(vec![]);
    input.tick = 2400;
    input.manpower = 1000.0;
    input.research = Some(ResearchObservation {
        faction: Faction::Usa,
        tech: TechState::new(1, 1, 1, 1).unwrap(),
        busy: vec![],
        stock_base: true,
        armory_ready: true,
        standard_army: true,
    });
    let commands = look(input, &mut State::default(), Level::Normal);
    assert_eq!(emitted(&commands, "tech")["kind"], "tier");
    commands
}
fn repair_outputs(kind: &str) -> Vec<Value> {
    let mut input = fixture(vec![unit(1, "rifle", 0, 40.0, 40.0)]);
    complete_base(&mut input);
    input.manpower = 1000.0;
    protect_wallet(&mut input);
    // No facility needs construction, and no finished HQ fortification is due.
    let cell = 20 * 80 + 23;
    match kind {
        "bridge" => {
            input
                .runtime
                .as_mut()
                .unwrap()
                .initial_chars
                .as_mut()
                .unwrap()[cell] = b'=';
            input.chars[cell] = b'W';
        }
        "fill" => {
            input.chars[cell] = b'F';
            input.runtime.as_mut().unwrap().height.as_mut().unwrap()[cell] = -1;
        }
        "demine" => input.runtime.as_mut().unwrap().found_mines = vec![Point { x: 47.0, z: 41.0 }],
        _ => unreachable!(),
    }
    let commands = look(input, &mut State::default(), Level::Normal);
    assert_eq!(emitted(&commands, "dig")["kind"], kind);
    commands
}
fn holder_outputs(kind: &str) -> Vec<Value> {
    let mut input = fixture(vec![unit(1, "rifle", 0, 40.0, 40.0)]);
    complete_base(&mut input);
    protect_wallet(&mut input);
    input.manpower = match kind {
        "arc" => 95.0,
        "cover" | "garrison" => 0.0,
        _ => 600.0,
    };
    if matches!(kind, "strongpoint" | "wire" | "traps") {
        input.runtime.as_mut().unwrap().mines =
            vec![Point { x: 31.0, z: 31.0 }, Point { x: 33.0, z: 31.0 }];
    }
    if matches!(kind, "wire" | "traps") {
        for cell in [
            20 * 80 + 20,
            20 * 80 + 21,
            20 * 80 + 22,
            21 * 80 + 20,
            21 * 80 + 21,
            21 * 80 + 22,
        ] {
            input.flags[cell] |= TRENCH;
        }
    }
    if kind == "traps" {
        for cell in [20 * 80 + 29, 20 * 80 + 30, 21 * 80 + 29, 21 * 80 + 30] {
            input.chars[cell] = b'X';
        }
    }
    if kind == "garrison" {
        input.chars[20 * 80 + 21] = b'B';
    }
    let mut state = State::default();
    // A delivered vehicle teaches the trap preference, then leaves the quiet point.
    if kind == "traps" {
        let mut sight = input.clone();
        sight.units.push(unit(500, "tank", 1, 130.0, 130.0));
        look(sight, &mut state, Level::Normal);
        input.tick += 40;
    }
    let commands = look(input, &mut state, Level::Normal);
    let command = emitted(
        &commands,
        match kind {
            "mines" => "dig",
            "cover" | "garrison" => kind,
            _ => "entrench",
        },
    );
    match kind {
        "mines" => assert_eq!(command["kind"], kind),
        "arc" | "strongpoint" => assert_eq!(command["pattern"], kind),
        "wire" | "traps" => {
            assert_eq!(command["pattern"], "line");
            assert_eq!(command["fort"], kind);
        }
        _ => {}
    }
    commands
}
fn fort_outputs(gate: bool) -> Vec<Value> {
    let mut input = fixture(vec![
        unit(1, "rifle", 0, 40.0, 40.0),
        unit(2, "mg", 0, 42.0, 40.0),
        unit(3, "mg", 0, 44.0, 40.0),
    ]);
    complete_base(&mut input);
    // Put the HQ and crew in the interior so every wall stamp is delivered.
    input.units.iter_mut().find(|u| u.kind == "hq").unwrap().at = Point { x: 40.0, z: 40.0 };
    input.points.clear();
    input.tick = 3000;
    input.manpower = 300.0;
    let mut state = State::default();
    input.runtime.as_mut().unwrap().build_reserve = 1_000_000.0;
    state.fort_steps.insert(60000, if gate { 3 } else { 2 });
    let commands = look(input, &mut state, Level::Normal);
    if gate {
        assert_eq!(emitted(&commands, "dig")["kind"], "gate");
    } else {
        let c = emitted(&commands, "entrench");
        assert_eq!(c["pattern"], "line");
        assert_eq!(c["fort"], "wall");
    }
    commands
}
fn motion_outputs(kind: &str) -> Vec<Value> {
    let mut own = unit(
        1,
        if kind == "move" { "medic" } else { "rifle" },
        0,
        40.0,
        40.0,
    );
    if kind == "retreat" {
        own.hp *= 0.1;
    }
    let mut input = fixture(vec![own, unit(2, "rifle", 0, 100.0, 40.0)]);
    input.points[0].at.x = 120.0;
    input.points[0].owner = if kind == "amove" { Some(1) } else { None };
    let commands = look(input, &mut State::default(), Level::Normal);
    emitted(&commands, kind);
    commands
}
fn focus_outputs() -> Vec<Value> {
    let mut input = fixture(vec![
        unit(1, "rifle", 0, 40.0, 40.0),
        unit(2, "rifle", 0, 42.0, 40.0),
        unit(10, "rifle", 1, 50.0, 40.0),
        unit(11, "rifle", 1, 54.0, 40.0),
    ]);
    input.points.clear();
    for own in &mut input.units[..2] {
        own.target_id = 11;
    }
    input.units[2].hp *= 0.1;
    for id in [1, 2] {
        input
            .runtime
            .as_mut()
            .unwrap()
            .units
            .get_mut(&id)
            .unwrap()
            .cooldown = 1000.0;
    }
    let mut state = State::default();
    look(input.clone(), &mut state, Level::Hard);
    input.tick += 11;
    let commands = look(input, &mut state, Level::Hard);
    assert_eq!(emitted(&commands, "attack")["target"], 10);
    assert_eq!(emitted(&commands, "stance")["key"], "autoRetreat");
    commands
}

#[test]
fn ai13_union_buy_support_build_assist_tech_are_planner_outputs() {
    shopping_outputs();
    support_outputs();
    construction_outputs(false);
    construction_outputs(true);
    tech_outputs();
}
#[test]
fn ai13_union_dig_bridge_fill_demine_mines_gate_are_planner_outputs() {
    for kind in ["bridge", "fill", "demine"] {
        repair_outputs(kind);
    }
    holder_outputs("mines");
    fort_outputs(true);
}
#[test]
fn ai13_union_entrench_arc_strongpoint_wire_traps_wall_are_planner_outputs() {
    for kind in ["arc", "strongpoint", "wire", "traps"] {
        holder_outputs(kind);
    }
    fort_outputs(false);
}
#[test]
fn ai13_union_move_amove_attack_retreat_stance_garrison_cover_are_planner_outputs() {
    for kind in ["move", "amove", "retreat"] {
        motion_outputs(kind);
    }
    focus_outputs();
    holder_outputs("garrison");
    holder_outputs("cover");
}

#[test]
fn ai13_union_contains_every_ordinary_command_family_and_engineering_variant() {
    let mut commands = shopping_outputs();
    commands.extend(support_outputs());
    commands.extend(construction_outputs(false));
    commands.extend(construction_outputs(true));
    commands.extend(tech_outputs());
    for kind in ["bridge", "fill", "demine"] {
        commands.extend(repair_outputs(kind));
    }
    for kind in [
        "mines",
        "arc",
        "strongpoint",
        "wire",
        "traps",
        "garrison",
        "cover",
    ] {
        commands.extend(holder_outputs(kind));
    }
    commands.extend(fort_outputs(false));
    commands.extend(fort_outputs(true));
    for kind in ["move", "amove", "retreat"] {
        commands.extend(motion_outputs(kind));
    }
    commands.extend(focus_outputs());
    for kind in ["rifle", "ranger", "mortar", "tank"] {
        commands.extend(projectile_lifecycle(kind));
    }
    let families: BTreeSet<_> = commands.iter().map(|c| c["t"].as_str().unwrap()).collect();
    assert_eq!(
        families,
        BTreeSet::from([
            "buy", "support", "build", "assist", "tech", "dig", "entrench", "move", "amove",
            "attack", "retreat", "ability", "stance", "garrison", "cover", "fireat"
        ])
    );
    let dig: BTreeSet<_> = commands
        .iter()
        .filter(|c| c["t"] == "dig")
        .map(|c| c["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        dig,
        BTreeSet::from(["bridge", "fill", "demine", "mines", "gate"])
    );
    let entrench: BTreeSet<_> = commands
        .iter()
        .filter(|c| c["t"] == "entrench")
        .map(|c| c.get("fort").unwrap_or(&c["pattern"]).as_str().unwrap())
        .collect();
    assert_eq!(
        entrench,
        BTreeSet::from(["arc", "strongpoint", "wire", "traps", "wall"])
    );
}

// These four cases use real ordinary human packets, canonical queued commands,
// unrestricted live projectile clocks and every seat's actual recipient encoder.
fn projectile_lifecycle(kind: &str) -> Vec<Value> {
    use crate::{
        match_firing::LaunchEvent,
        match_human_observation::HumanObservationMemory,
        match_tick::TickEvent,
        mov_audit_test_support::{point, scene_rows, spawn},
        orders::OrderKey,
        recipient_snapshot::{DeliveryState, SelectedSeat},
    };
    let mut rows = vec![".".repeat(64); 64];
    rows[20].replace_range(25..29, "BBBB");
    rows[21].replace_range(25..29, "BBBB");
    let mut m = scene_rows(rows);
    m.world.game.reveal = true;
    m.start.win_vp = 3.0;
    let shooter = spawn(&mut m, kind, 0, point(40.0, 41.0));
    let enemy = spawn(&mut m, "mg", 1, point(51.0, 41.0));
    m.world.game.units.get_mut(&enemy).unwrap().garrison = Some(20 * 64 + 25);
    let capture = m.start.points[0].at;
    spawn(&mut m, "rifle", 0, capture);
    // Keep unrelated opening units idle. Projectile and ability clocks remain live.
    for id in m.world.game.units.keys().copied().collect::<Vec<_>>() {
        m.recovery.initialize(id);
    }
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    for u in m.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    m.grid.rebuild(&m.world.game.units);
    let visible = m.world.game.units.keys().copied().collect::<Vec<_>>();
    for player in &mut m.world.game.players {
        player.visible = visible.clone();
    }
    let mut memory = HumanObservationMemory::new(0);
    let mut planner = State::default();
    let first = m.human_commander_observation(&mut memory).unwrap();
    assert!(
        first
            .packet
            .input()
            .runtime
            .as_ref()
            .unwrap()
            .ground_flights
    );
    commander_tactics::plan(
        &first.packet,
        &[],
        &mut planner,
        Level::Normal,
        &mut MatchRandom::new(813),
    )
    .unwrap();
    let mut delivery = DeliveryState::new(&m).unwrap();
    let selected: Vec<_> = (0..m.world.game.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect();
    let mut emitted_commands = Vec::new();
    let mut launched = BTreeSet::new();
    let mut retired = BTreeSet::new();
    let mut retained = false;
    let mut delivered_flights = BTreeSet::new();
    let mut applied = false;
    let mut terminal = 0;
    for _ in 0..1600 {
        let report = m.advance_tick().unwrap();
        if !report.commands.is_empty() {
            assert!(
                report.commands.iter().all(|c| c.denial.is_none()),
                "{kind}: {:?}",
                report.commands
            );
            applied = true;
        }
        for event in &report.events {
            match event {
                TickEvent::Firing(LaunchEvent::Flight {
                    id,
                    shooter: source,
                    ..
                }) if *source == shooter => {
                    launched.insert(*id);
                }
                TickEvent::Firing(LaunchEvent::Contact(contact))
                    if contact.shooter == shooter && contact.terminal =>
                {
                    retired.insert(contact.flight);
                }
                TickEvent::Firing(LaunchEvent::Expiry { id, .. }) if launched.contains(id) => {
                    retired.insert(*id);
                }
                _ => {}
            }
        }
        retained |= m
            .firing
            .flights
            .values()
            .any(|flight| flight.shooter == shooter);
        let finished = m.finish.is_some();
        let frame = delivery
            .capture_selected_with_tick_events(&mut m, &selected, &report.events)
            .unwrap()
            .unwrap();
        for seat in &selected {
            let packet = frame.recipient(seat.slot).unwrap().into_json();
            assert_eq!(packet["t"], "s");
            if packet
                .get("flights")
                .and_then(Value::as_array)
                .is_some_and(|rows| {
                    rows.iter().any(|row| {
                        row["flight"]
                            .as_u64()
                            .is_some_and(|id| launched.contains(&id))
                    })
                })
            {
                delivered_flights.insert(seat.slot);
            }
            if finished {
                assert_eq!(packet["winner"], 0);
            }
        }
        drop(frame);
        assert!(m.tick_state.fault().is_none(), "{kind}");
        if emitted_commands.is_empty() && m.world.game.tick == 26 {
            let view = m.human_commander_observation(&mut memory).unwrap();
            let output = commander_tactics::plan(
                &view.packet,
                &[],
                &mut planner,
                Level::Normal,
                &mut MatchRandom::new(813),
            )
            .unwrap();
            let family = if kind == "tank" { "fireat" } else { "ability" };
            let command = output
                .into_iter()
                .find(|c| c["t"] == family && c["ids"] == json!([shooter]))
                .unwrap_or_else(|| panic!("{kind}: planner did not emit {family}"));
            assert!(command["x"].is_number() && command["z"].is_number());
            if kind == "tank" {
                m.commands.units.get_mut(&shooter).unwrap().hold_fire = false;
            }
            m.enqueue_command(
                OrderKey {
                    tick: m.world.game.tick as u64 + 1,
                    issuer: 0,
                    sequence: 0,
                },
                Command::from_json(&command).unwrap(),
            )
            .unwrap();
            emitted_commands.push(command);
        }
        if m.finish.is_some() {
            terminal += 1;
            if terminal == 61 {
                break;
            }
        }
    }
    assert!(applied && !emitted_commands.is_empty(), "{kind}");
    assert!(
        !launched.is_empty() && retained,
        "{kind}: live flights must launch and persist"
    );
    assert_eq!(
        delivered_flights,
        selected.iter().map(|seat| seat.slot).collect(),
        "{kind}: each recipient must receive an emitted live flight"
    );
    assert!(
        !launched.is_disjoint(&retired),
        "{kind}: an emitted flight must resolve on ordinary ticks"
    );
    assert_eq!(
        m.finish
            .as_ref()
            .expect("objective income finishes the ordinary match")
            .winner,
        0
    );
    assert_eq!(terminal, 61, "{kind}");
    let result = m.match_result().unwrap().expect("finished ordinary result");
    assert_eq!(result.winner, 0);
    assert_eq!(m.world.game.tick, result.end_tick + 60);
    assert_eq!(result.reason, crate::match_end::EndReason::VictoryPoints);
    emitted_commands
}

#[test]
fn ai13_union_ability_grenade_satchel_barrage_execute_live_flights_and_finish() {
    for kind in ["rifle", "ranger", "mortar"] {
        projectile_lifecycle(kind);
    }
}
#[test]
fn ai13_union_fireat_executes_live_flights_every_seat_delivery_and_finish() {
    projectile_lifecycle("tank");
}
