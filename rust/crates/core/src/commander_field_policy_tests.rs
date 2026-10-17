use super::tests::{fixture, run, unit};
use super::*;
use crate::{
    commander_ai::PointObservation,
    commander_field_policy::{self as field, Facts},
    game::TRENCH,
};

fn scene() -> crate::commander_ai::Input {
    let mut i = fixture(vec![
        unit(1, "rifle", 40.0),
        unit(2, "rifle", 44.0),
        unit(3, "rifle", 48.0),
    ]);
    i.players[1].spawn = Point { x: 150.0, z: 20.0 };
    i.manpower = 1000.0;
    i.points = vec![PointObservation {
        at: Point { x: 40.0, z: 20.0 },
        vp: 1.0,
        mp: 1.0,
        owner: Some(0),
        cut: false,
        locked: false,
    }];
    i.runtime.as_mut().unwrap().field = Some(Facts {
        initial_chars: i.chars.clone(),
        ground: i.chars.clone(),
        ..Facts::default()
    });
    i
}
fn fields(i: crate::commander_ai::Input, state: &mut field::State, admit: bool) -> Vec<Value> {
    let o = Observation::new(i).unwrap();
    let mut commands = vec![];
    field::field_work(&o, state, &mut BTreeSet::new(), |c| {
        commands.push(c);
        admit
    })
    .unwrap();
    commands
}
fn hold(i: crate::commander_ai::Input, armor: bool) -> Vec<Value> {
    let o = Observation::new(i).unwrap();
    let mut commands = vec![];
    field::holder(&o, &o.input().units[0], 0, armor, |c| {
        commands.push(c);
        true
    })
    .unwrap();
    commands
}
#[test]
fn intent_ai48_remembered_bridge_fill_demine_each_dispatch_once_and_approach_safely() {
    let mut i = scene();
    let w = i.width;
    let bridge = 12 * w + 25;
    for z in 0..i.height {
        i.chars[z * w + 25] = b'W';
        i.chars[z * w + 60] = b'W';
    }
    let flood = 12 * w + 20;
    i.chars[flood] = b'F';
    let f = i.runtime.as_mut().unwrap().field.as_mut().unwrap();
    f.bridges.insert(bridge);
    f.bridges.insert(30 * w + 60);
    f.found_mines = vec![Point { x: 61.0, z: 21.0 }];
    let mut state = field::State::default();
    let commands = fields(i.clone(), &mut state, true);
    assert_eq!(
        commands
            .iter()
            .map(|c| c["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["bridge", "fill", "demine"]
    );
    assert_eq!(commands[0]["ids"], json!([3]));
    assert_eq!(
        (commands[0]["x"].as_f64(), commands[0]["z"].as_f64()),
        (Some(51.0), Some(25.0)),
        "choose the crossing nearest spawn"
    );
    assert_eq!(
        commands[0]["dir"],
        json!(0.0),
        "span the shorter horizontal water run"
    );
    assert_ne!(commands[0]["ids"], commands[1]["ids"]);
    assert_ne!(commands[1]["ids"], commands[2]["ids"]);
    let denied = fields(i.clone(), &mut field::State::default(), false);
    assert_eq!(denied.len(), 6);
    assert_eq!(denied[1]["t"], "move");
    assert_eq!(denied[3]["t"], "move");
    let goal = Point {
        x: denied[5]["orders"][0][1].as_f64().unwrap(),
        z: denied[5]["orders"][0][2].as_f64().unwrap(),
    };
    assert!((dist(goal, Point { x: 61.0, z: 21.0 }) - 9.0).abs() < 1e-10);
    i.runtime
        .as_mut()
        .unwrap()
        .field
        .as_mut()
        .unwrap()
        .bridges
        .clear();
    i.tick += 20;
    let second = fields(i.clone(), &mut state, true);
    assert_eq!(
        second
            .iter()
            .filter(|c| c["kind"] == "fill" || c["kind"] == "demine")
            .count(),
        0
    );
    i.tick += 1801;
    let expired = fields(i, &mut state, true);
    assert!(expired.iter().any(|c| c["kind"] == "fill"));
    assert!(
        expired.iter().any(|c| c["kind"] == "bridge"),
        "remember a delivered crossing across looks"
    );
}
fn dist(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
#[test]
fn intent_ai48_job_limits_threats_spoiled_ground_and_completed_crew_release() {
    let mut i = scene();
    let c = 10 * i.width + 22;
    i.chars[c] = b'+';
    assert!(
        fields(i.clone(), &mut field::State::default(), true).is_empty(),
        "dry crater cover stays"
    );
    i.chars[c] = b'F';
    i.manpower = 0.0;
    assert!(fields(i.clone(), &mut field::State::default(), true).is_empty());
    i.manpower = 1000.0;
    let mut enemy = unit(9, "rifle", 50.0);
    enemy.owner = 1;
    i.units.push(enemy);
    assert!(fields(i.clone(), &mut field::State::default(), true).is_empty());
    i.units.pop();
    let mut state = field::State::default();
    let sent = fields(i.clone(), &mut state, true);
    let id = sent[0]["ids"][0].as_u64().unwrap() as u32;
    i.units.iter_mut().find(|u| u.id == id).unwrap().dig = true;
    i.tick += 1;
    assert!(fields(i.clone(), &mut state, true).is_empty());
    i.units.iter_mut().find(|u| u.id == id).unwrap().dig = false;
    i.tick += 1;
    assert_eq!(fields(i.clone(), &mut state, true)[0]["kind"], "fill");
    i.points.clear();
    i.runtime
        .as_mut()
        .unwrap()
        .field
        .as_mut()
        .unwrap()
        .free_nodes = vec![Point { x: 45.0, z: 21.0 }];
    assert_eq!(
        fields(i, &mut field::State::default(), true)[0]["kind"],
        "fill",
        "free delivered nodes also get repaired"
    );
}
#[test]
fn intent_ai49_holder_builds_mines_trenches_wire_and_seen_armor_traps_with_reserve() {
    let mut i = scene();
    let r = Observation::new(i.clone()).unwrap().point_radius();
    let m = hold(i.clone(), false);
    assert_eq!(m[0]["kind"], "mines");
    assert_eq!(m[0]["x"], 40.0 + r + 2.0);
    i.runtime.as_mut().unwrap().field.as_mut().unwrap().mines =
        vec![Point { x: 41.0, z: 20.0 }, Point { x: 42.0, z: 20.0 }];
    assert_eq!(hold(i.clone(), false)[0]["pattern"], "strongpoint");
    i.manpower = 599.0;
    assert_eq!(hold(i.clone(), false)[0]["pattern"], "arc");
    for x in 18..24 {
        i.flags[10 * i.width + x] |= TRENCH;
    }
    let wire = hold(i.clone(), false);
    assert_eq!(wire[0]["fort"], "wire");
    assert_eq!(
        (wire[0]["z2"].as_f64().unwrap() - wire[0]["z"].as_f64().unwrap()).abs(),
        14.0
    );
    let ahead = (40.0 + r + 7.0) / 2.0;
    for x in ahead as usize - 1..ahead as usize + 3 {
        i.chars[10 * i.width + x] = b'X';
    }
    assert_eq!(
        hold(i.clone(), false)[0]["t"],
        "cover",
        "unseen armor does not justify traps"
    );
    let traps = hold(i.clone(), true);
    assert_eq!(traps[0]["fort"], "traps");
    assert_eq!(
        (traps[0]["z2"].as_f64().unwrap() - traps[0]["z"].as_f64().unwrap()).abs(),
        16.0
    );
    i.manpower = 60.0;
    assert_eq!(hold(i, false)[0]["t"], "cover");
}
#[test]
fn intent_ai49_quiet_holder_leaves_house_for_work_else_garrisons_or_takes_cover() {
    let mut i = scene();
    i.units[0].garrison = true;
    assert_eq!(hold(i.clone(), false)[0]["t"], "move");
    i.units[0].garrison = false;
    let mut enemy = unit(9, "rifle", 60.0);
    enemy.owner = 1;
    i.units.push(enemy);
    i.chars[10 * i.width + 20] = b'B';
    assert_eq!(hold(i.clone(), false)[0]["t"], "garrison");
    i.chars[10 * i.width + 20] = b'.';
    assert_eq!(hold(i.clone(), false)[0]["t"], "cover");
    i.units[0].cover = 1;
    assert!(hold(i, false).is_empty());
}
#[test]
fn intent_ai49_only_first_holder_is_claimed_in_integrated_tactics() {
    let mut i = scene();
    i.units[0].cover = 0;
    i.runtime.as_mut().unwrap().mode = Mode::Classic;
    let commands = run(i, &mut State::default(), Level::Normal);
    assert!(
        commands
            .iter()
            .any(|c| c["t"] == "dig" && c["kind"] == "mines" && c["ids"] == json!([1]))
    );
    assert!(
        !commands
            .iter()
            .any(|c| matches!(c["t"].as_str(), Some("move" | "amove"))
                && c["orders"].as_array().unwrap().iter().any(|r| r[0] == 1))
    );
}

#[test]
fn intent_ai48_fill_and_demine_keep_their_distinct_squad_reach_limits() {
    let mut input = scene();
    input.units = vec![unit(1, "rifle", 41.0)];
    input.units[0].at.z = 21.0;
    input.points[0].at = Point { x: 101.0, z: 21.0 };
    input.chars[10 * input.width + 50] = b'F';
    assert_eq!(
        fields(input.clone(), &mut field::State::default(), true)[0]["kind"],
        "fill"
    );
    input.units[0].at.x = 40.99;
    assert!(fields(input.clone(), &mut field::State::default(), true).is_empty());
    input.points.clear();
    input.units[0].at.x = 41.0;
    input
        .runtime
        .as_mut()
        .unwrap()
        .field
        .as_mut()
        .unwrap()
        .found_mines = vec![Point { x: 81.0, z: 21.0 }];
    assert_eq!(
        fields(input.clone(), &mut field::State::default(), true)[0]["kind"],
        "demine"
    );
    input.units[0].at.x = 40.99;
    assert!(fields(input, &mut field::State::default(), true).is_empty());
}
