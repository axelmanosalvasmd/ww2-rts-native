use super::*;
use crate::{
    commander_ai::{Input, Level, PlayerObservation},
    commander_tactics::UnitFacts,
    game::SIGHT,
};

fn unit(id: u32, kind: &str, owner: usize, x: f64) -> UnitObservation {
    UnitObservation {
        id,
        kind: kind.into(),
        owner,
        at: Point { x, z: 40.0 },
        hp: health(Catalog::compiled().unwrap().unit(kind).unwrap()),
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
fn fixture(units: Vec<UnitObservation>) -> Input {
    Input {
        width: 100,
        height: 80,
        tick: 200,
        slot: 0,
        players: [0, 1, 0]
            .map(|team| PlayerObservation {
                team,
                out: false,
                spawn: Point { x: 5.0, z: 5.0 },
            })
            .into(),
        runtime: Some(Context {
            ground_flights: true,
            munitions: 100.0,
            units: units
                .iter()
                .filter(|u| u.owner == 0)
                .map(|u| {
                    (
                        u.id,
                        UnitFacts {
                            built: 1.0,
                            ..UnitFacts::default()
                        },
                    )
                })
                .collect(),
            ..Context::default()
        }),
        units,
        points: vec![],
        flags: vec![0; 8000],
        chars: vec![b'.'; 8000],
        manpower: 0.0,
        research: None,
    }
}
fn run(input: Input, watched: &[u32], level: Level) -> Plan {
    plan(
        &Observation::new(input).unwrap(),
        &watched.iter().copied().collect(),
        Skill::for_level(level),
        &BTreeSet::new(),
    )
    .unwrap()
}
fn casts(plan: &Plan) -> Vec<&Value> {
    plan.commands
        .iter()
        .filter(|c| c["t"] == "ability")
        .collect()
}

#[test]
fn ai13_ground_abilities_emit_for_each_family_from_visible_facts() {
    for kind in ["rifle", "mg", "at", "tank", "ranger", "conscript", "mortar"] {
        let mut own = unit(1, kind, 0, 40.0);
        let mut enemy = unit(
            10,
            if matches!(kind, "at" | "ranger") {
                "tank"
            } else {
                "mg"
            },
            1,
            50.0,
        );
        own.target_id = 10;
        if kind == "tank" {
            own.hp *= 0.4;
        }
        if kind == "conscript" {
            own.supp = 50.0;
        }
        if kind == "mortar" {
            enemy.garrison = true;
        }
        let input = fixture(vec![own, enemy]);
        let output = run(input.clone(), &[10], Level::Normal);
        let commands = casts(&output);
        assert_eq!(commands.len(), 1, "{kind}: {:?}", output.commands);
        assert_eq!(commands[0]["ids"], json!([1]));
        if matches!(kind, "rifle" | "ranger" | "mortar") {
            assert_eq!(
                (commands[0]["x"].as_f64(), commands[0]["z"].as_f64()),
                (Some(50.0), Some(40.0))
            );
        } else {
            assert!(commands[0].get("x").is_none());
        }
        let mut cooling = input;
        cooling
            .runtime
            .as_mut()
            .unwrap()
            .units
            .get_mut(&1)
            .unwrap()
            .cooldown = 1.0;
        assert!(
            casts(&run(cooling, &[10], Level::Normal)).is_empty(),
            "{kind}"
        );
    }
}

#[test]
fn ai13_ground_targeted_abilities_require_watched_delivered_enemies() {
    for (kind, target) in [
        ("rifle", "mg"),
        ("mg", "rifle"),
        ("at", "tank"),
        ("ranger", "tank"),
        ("mortar", "mg"),
    ] {
        let mut own = unit(1, kind, 0, 40.0);
        own.target_id = 10;
        let mut enemy = unit(10, target, 1, 50.0);
        enemy.garrison = kind == "mortar";
        let input = fixture(vec![own, enemy]);
        assert!(
            casts(&run(input.clone(), &[], Level::Normal)).is_empty(),
            "new contact {kind}"
        );
        let mut hidden = input.clone();
        hidden.units.pop();
        assert!(
            casts(&run(hidden, &[10], Level::Normal)).is_empty(),
            "remembered {kind}"
        );
        let mut ally = input;
        ally.units[1].owner = 2;
        assert!(
            casts(&run(ally, &[10], Level::Normal)).is_empty(),
            "allied {kind}"
        );
    }
    let input = fixture(vec![unit(1, "rifle", 0, 40.0), unit(10, "rifle", 1, 50.0)]);
    assert!(casts(&run(input.clone(), &[10], Level::Normal)).is_empty());
    let mut covered = input;
    covered.units[1].cover = 1;
    assert_eq!(casts(&run(covered, &[10], Level::Normal)).len(), 1);
    let mut input = fixture(vec![unit(1, "at", 0, 40.0), unit(10, "rifle", 1, 50.0)]);
    input.units[0].target_id = 10;
    assert!(casts(&run(input, &[10], Level::Normal)).is_empty());
}

#[test]
fn ai13_ground_ability_caps_wallet_and_local_mirror_prevent_overbooking() {
    let mut units: Vec<_> = (1..=4)
        .map(|id| {
            let mut u = unit(id, "mg", 0, 40.0);
            u.target_id = 10;
            u
        })
        .collect();
    units.push(unit(10, "rifle", 1, 50.0));
    let mut input = fixture(units);
    assert_eq!(casts(&run(input.clone(), &[10], Level::Easy)).len(), 1);
    assert_eq!(casts(&run(input.clone(), &[10], Level::Hard)).len(), 3);
    input.runtime.as_mut().unwrap().mode = Mode::Classic;
    input.runtime.as_mut().unwrap().munitions = 10.0;
    let saved = input.clone();
    assert_eq!(casts(&run(input.clone(), &[10], Level::Hard)).len(), 1);
    let observation = Observation::new(input.clone()).unwrap();
    let output = plan(
        &observation,
        &BTreeSet::from([10]),
        Skill::for_level(Level::Hard),
        &BTreeSet::from([1]),
    )
    .unwrap();
    assert_eq!(casts(&output)[0]["ids"], json!([2]));
    assert_eq!(observation.input(), &saved);
    input.runtime.as_mut().unwrap().munitions = 9.0;
    assert!(casts(&run(input, &[10], Level::Hard)).is_empty());
}

#[test]
fn ai13_ground_ura_uses_watched_attack_move_threat_and_barrage_avoids_friends() {
    let own = unit(1, "conscript", 0, 40.0);
    let mut input = fixture(vec![own, unit(10, "rifle", 1, 50.0)]);
    input
        .runtime
        .as_mut()
        .unwrap()
        .units
        .get_mut(&1)
        .unwrap()
        .amove = Some(Point { x: 80.0, z: 40.0 });
    assert_eq!(casts(&run(input.clone(), &[10], Level::Normal)).len(), 1);
    assert!(casts(&run(input, &[], Level::Normal)).is_empty());
    let mut input = fixture(vec![
        unit(1, "mortar", 0, 20.0),
        unit(10, "rifle", 1, 50.0),
        unit(11, "rifle", 1, 52.0),
        unit(12, "rifle", 1, 54.0),
    ]);
    let output = run(input.clone(), &[10, 11, 12], Level::Hard);
    assert_eq!(casts(&output)[0]["x"], json!(52.0));
    input.units.insert(1, unit(2, "rifle", 2, 52.0));
    assert!(casts(&run(input, &[10, 11, 12], Level::Hard)).is_empty());
}

#[test]
fn ai13_ground_fireat_shells_watched_occupied_house_and_reserves_shooter() {
    let mut enemy = unit(10, "rifle", 1, 50.0);
    enemy.garrison = true;
    let input = fixture(vec![unit(1, "tank", 0, 40.0), enemy]);
    let output = run(input.clone(), &[10], Level::Normal);
    assert_eq!(
        output.commands,
        vec![json!({"t":"fireat", "ids":[1], "x":50.0, "z":40.0})]
    );
    assert_eq!(output.busy, BTreeSet::from([1]));
    assert!(run(input.clone(), &[], Level::Normal).commands.is_empty());
    for change in 0..4 {
        let mut guarded = input.clone();
        match change {
            0 => guarded.units[0].fire_at = true,
            1 => guarded.units[0].target_id = 10,
            2 => guarded.units[1].garrison = false,
            _ => guarded.units[1].at.x = 100.0,
        }
        assert!(
            !run(guarded, &[10], Level::Normal)
                .commands
                .iter()
                .any(|c| c["t"] == "fireat")
        );
    }
}

#[test]
fn ai13_ground_focus_emits_attack_and_queued_resume_with_visibility_range_los_controls() {
    let mut one = unit(1, "rifle", 0, 40.0);
    one.target_id = 11;
    let mut two = unit(2, "rifle", 0, 42.0);
    two.target_id = 11;
    let mut victim = unit(10, "rifle", 1, 50.0);
    victim.hp *= 0.1;
    let mut input = fixture(vec![one, two, victim, unit(11, "rifle", 1, 54.0)]);
    let goal = Point { x: 100.0, z: 40.0 };
    for facts in input.runtime.as_mut().unwrap().units.values_mut() {
        facts.amove = Some(goal);
        facts.cooldown = 1.0;
    }
    let second = input.runtime.as_mut().unwrap().units.get_mut(&2).unwrap();
    second.amove = None;
    second.queued_amove = Some(goal);
    let output = run(input.clone(), &[10, 11], Level::Hard);
    assert_eq!(
        output.commands,
        vec![
            json!({"t":"attack", "ids":[1,2], "target":10}),
            json!({"t":"amove", "orders":[[1,100.0,40.0],[2,100.0,40.0]], "queue":true})
        ]
    );
    assert_eq!(output.busy, BTreeSet::from([1, 2]));
    assert!(
        run(input.clone(), &[10, 11], Level::Normal)
            .commands
            .is_empty()
    );
    assert!(run(input.clone(), &[], Level::Hard).commands.is_empty());
    for change in 0..5 {
        let mut guarded = input.clone();
        match change {
            0 => {
                guarded.units[2].retreating = true;
                guarded.units[3].retreating = true;
            }
            1 => {
                guarded.units[2].at.x = 120.0;
                guarded.units[3].at.x = 125.0;
            }
            2 => guarded.units[1].garrison = true,
            3 => guarded.units[1].nade = true,
            _ => {
                for z in 0..80 {
                    guarded.flags[z * 100 + 23] = SIGHT;
                }
            }
        }
        assert!(
            !run(guarded, &[10, 11], Level::Hard)
                .commands
                .iter()
                .any(|c| c["t"] == "attack"),
            "guard {change}"
        );
    }
    input.units.retain(|u| u.id != 10);
    assert!(
        !run(input, &[10, 11], Level::Hard)
            .commands
            .iter()
            .any(|c| c["t"] == "attack")
    );
}

#[test]
fn ai13_ground_focus_releases_visible_retreating_ranged_or_occluded_target() {
    let mut own = unit(1, "rifle", 0, 40.0);
    own.target_id = 10;
    own.attack_id = true;
    let mut input = fixture(vec![own, unit(10, "rifle", 1, 50.0)]);
    let facts = input.runtime.as_mut().unwrap().units.get_mut(&1).unwrap();
    facts.attack_target = 10;
    facts.cooldown = 1.0;
    for change in 0..3 {
        let mut guarded = input.clone();
        match change {
            0 => guarded.units[1].retreating = true,
            1 => guarded.units[1].at.x = 90.0,
            _ => {
                for z in 0..80 {
                    guarded.flags[z * 100 + 23] = SIGHT;
                }
            }
        }
        let output = run(guarded.clone(), &[], Level::Hard);
        assert_eq!(
            output.commands,
            vec![json!({"t":"amove", "orders":[[1,40.0,40.0]]})],
            "{change}"
        );
        guarded
            .runtime
            .as_mut()
            .unwrap()
            .units
            .get_mut(&1)
            .unwrap()
            .queued_amove = Some(Point { x: 100.0, z: 40.0 });
        assert_eq!(
            run(guarded, &[], Level::Hard).commands,
            vec![json!({"t":"amove", "orders":[[1,100.0,40.0]]})]
        );
    }
    input.units.pop();
    assert!(run(input, &[10], Level::Hard).commands.is_empty());
}

#[test]
fn ai13_ground_focus_assigns_weapons_to_suitable_visible_targets() {
    let mut gun = unit(1, "at", 0, 40.0);
    gun.target_id = 12;
    let mut second = unit(2, "at", 0, 42.0);
    second.target_id = 12;
    let mut victim = unit(10, "tank", 1, 50.0);
    victim.hp *= 0.2;
    let mut input = fixture(vec![gun, second, victim, unit(12, "rifle", 1, 54.0)]);
    for facts in input.runtime.as_mut().unwrap().units.values_mut() {
        facts.cooldown = 1.0;
    }
    assert_eq!(
        run(input.clone(), &[10, 12], Level::Hard).commands,
        vec![json!({"t":"attack", "ids":[1,2], "target":10})]
    );
    input.units[2].kind = "rifle".into();
    input.units[2].hp = health(Catalog::compiled().unwrap().unit("rifle").unwrap()) * 0.2;
    assert!(
        !run(input.clone(), &[10, 12], Level::Hard)
            .commands
            .iter()
            .any(|c| c["t"] == "attack")
    );
    input.runtime.as_mut().unwrap().mode = Mode::World;
    assert!(run(input, &[10, 12], Level::Hard).commands.is_empty());
}

#[test]
fn ai13_ground_emitted_ability_and_focus_orders_survive_ordinary_match_delivery_and_finish() {
    use crate::{
        match_commands::Command,
        match_human_observation::HumanObservationMemory,
        mov_audit_test_support::{point, scene, spawn},
        orders::OrderKey,
        recipient_snapshot::DeliveryState,
    };
    let mut state = scene();
    state.world.game.reveal = true;
    state.start.win_vp = 5.0;
    let mg = spawn(&mut state, "mg", 0, point(40.0, 40.0));
    let one = spawn(&mut state, "rifle", 0, point(40.0, 43.0));
    let two = spawn(&mut state, "rifle", 0, point(42.0, 43.0));
    let victim = spawn(&mut state, "rifle", 1, point(50.0, 43.0));
    let other = spawn(&mut state, "rifle", 1, point(54.0, 43.0));
    state.world.game.units.get_mut(&victim).unwrap().hp *= 0.1;
    let capture_at = state.start.points[0].at;
    spawn(&mut state, "rifle", 0, capture_at);
    for id in state.world.game.units.keys().copied().collect::<Vec<_>>() {
        state.recovery.initialize(id);
    }
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    for unit in state.world.game.units.values_mut() {
        unit.hold_pos = true;
    }
    // Preserve combat-order state without entering the separately held flight-delivery work.
    for id in [mg, one, two] {
        state.firing.get_mut(id).unwrap().target_id = other;
        state.firing.get_mut(id).unwrap().cooldown = 1000.0;
    }
    for id in [one, two] {
        state.abilities.get_mut(id).unwrap().cooldown = 1000.0;
    }
    let visible: Vec<_> = state.world.game.units.keys().copied().collect();
    for player in &mut state.world.game.players {
        player.visible = visible.clone();
    }
    let delivered = state
        .human_commander_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    let output = plan(
        &delivered.packet,
        &BTreeSet::from([victim, other]),
        Skill::for_level(Level::Hard),
        &BTreeSet::new(),
    )
    .unwrap();
    assert!(
        output
            .commands
            .iter()
            .any(|c| c["t"] == "ability" && c["ids"] == json!([mg]))
    );
    assert!(
        output
            .commands
            .iter()
            .any(|c| c["t"] == "attack" && c["target"] == json!(victim))
    );
    let next = state.world.game.tick as u64 + 1;
    for (sequence, raw) in output.commands.iter().enumerate() {
        state
            .enqueue_command(
                OrderKey {
                    tick: next,
                    issuer: 0,
                    sequence: sequence as u64,
                },
                Command::from_json(raw).unwrap(),
            )
            .unwrap();
    }
    let mut delivery = DeliveryState::new(&state).unwrap();
    let mut applied = false;
    let mut active = 0;
    for _ in 0..1600 {
        let report = state.advance_tick().unwrap();
        if !applied {
            assert_eq!(report.commands.len(), output.commands.len());
            assert!(
                report.commands.iter().all(|c| c.denial.is_none()),
                "{:?}",
                report.commands
            );
            assert!(state.abilities.get(mg).unwrap().buff > 0.0);
            assert_eq!(state.commands.units[&one].attack_id, victim);
            assert_eq!(state.commands.units[&two].attack_id, victim);
            applied = true;
        }
        let effects = report.environment().cloned().collect::<Vec<_>>();
        let frame = delivery
            .capture_with_lists(&mut state, &[true; 2], &[false; 2], &effects)
            .unwrap();
        for slot in 0..2 {
            assert_eq!(frame.recipient(slot).unwrap().into_json()["t"], "s");
        }
        assert!(state.tick_state.fault().is_none());
        active += 1;
        if state.finish.is_some() {
            break;
        }
    }
    assert!(applied && active >= 4);
    assert_eq!(
        state
            .finish
            .as_ref()
            .expect("objective income finishes ordinary match")
            .winner,
        0
    );
    for _ in 0..3 {
        let report = state.advance_tick().unwrap();
        let effects = report.environment().cloned().collect::<Vec<_>>();
        let frame = delivery
            .capture_with_lists(&mut state, &[true; 2], &[false; 2], &effects)
            .unwrap();
        for slot in 0..2 {
            assert_eq!(frame.recipient(slot).unwrap().into_json()["winner"], 0);
        }
    }
    assert!(state.tick_state.fault().is_none());
}

#[test]
fn ai13_human_delivery_capability_holds_projectile_abilities_and_fireat() {
    for kind in ["rifle", "ranger", "mortar", "tank"] {
        let own = unit(1, kind, 0, 40.0);
        let mut enemy = unit(10, "mg", 1, 50.0);
        enemy.garrison = true;
        let mut input = fixture(vec![own, enemy]);
        input.runtime.as_mut().unwrap().ground_flights = false;
        let output = run(input, &[10], Level::Normal);
        assert!(
            !output
                .commands
                .iter()
                .any(|c| c["t"] == "fireat" || c["t"] == "ability"),
            "{kind}: {:?}",
            output.commands
        );
    }
}
