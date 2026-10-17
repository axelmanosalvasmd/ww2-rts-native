use super::*;
use crate::{
    commander_ai::{Input, PlayerObservation, PointObservation},
    commander_tactics::Context,
};

fn unit(id: u32, x: f64, z: f64) -> UnitObservation {
    UnitObservation {
        id,
        kind: "rifle".into(),
        owner: 0,
        at: Point { x, z },
        hp: 100.0,
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

fn input(units: Vec<UnitObservation>) -> Input {
    let chars = vec![b'.'; 128 * 128];
    Input {
        width: 128,
        height: 128,
        tick: 200,
        slot: 0,
        players: [0, 1]
            .map(|team| PlayerObservation {
                team,
                out: false,
                spawn: Point { x: 101.0, z: 101.0 },
            })
            .into(),
        units,
        points: vec![],
        flags: vec![0; chars.len()],
        chars: chars.clone(),
        manpower: 1000.0,
        research: None,
        runtime: Some(Context {
            field: Some(Facts {
                initial_chars: chars.clone(),
                ground: chars,
                ..Facts::default()
            }),
            ..Context::default()
        }),
    }
}

fn run(input: Input, state: &mut State) -> Vec<Value> {
    let observation = Observation::new(input).unwrap();
    let mut commands = Vec::new();
    field_work(&observation, state, &mut BTreeSet::new(), |raw| {
        commands.push(raw);
        true
    })
    .unwrap();
    commands
}

#[test]
fn if25_active_field_bridge_ingestion_and_gap_selection_keep_remembered_ties() {
    let left = 50 * 128 + 40;
    let right = 50 * 128 + 60;
    let mut i = input(vec![unit(1, 101.0, 91.0)]);
    i.chars[left] = b'W';
    i.chars[right] = b'W';
    let facts = i.runtime.as_mut().unwrap().field.as_mut().unwrap();
    facts.bridges.insert(right);
    facts.bridges.insert(left);
    let mut fresh = State::default();
    assert_eq!(run(i.clone(), &mut fresh)[0]["x"], 81.0);
    assert_eq!(fresh.bridges, [left, right]);
    let mut prior = State::default();
    let mut first = i.clone();
    first
        .runtime
        .as_mut()
        .unwrap()
        .field
        .as_mut()
        .unwrap()
        .bridges
        .remove(&left);
    first.manpower = 0.0;
    assert!(run(first, &mut prior).is_empty());
    assert_eq!(prior.bridges, [right]);
    assert_eq!(run(i, &mut prior)[0]["x"], 121.0);
    assert_eq!(prior.bridges, [right, left]);
}

#[test]
fn if25_active_field_bridge_crew_ties_keep_delivered_unit_order() {
    for swapped in [false, true] {
        let positions = if swapped {
            [(81.0, 111.0), (81.0, 91.0)]
        } else {
            [(81.0, 91.0), (81.0, 111.0)]
        };
        let mut i = input(vec![
            unit(1, positions[0].0, positions[0].1),
            unit(2, positions[1].0, positions[1].1),
        ]);
        let cell = 50 * 128 + 40;
        i.chars[cell] = b'W';
        i.runtime
            .as_mut()
            .unwrap()
            .field
            .as_mut()
            .unwrap()
            .bridges
            .insert(cell);
        let commands = run(i, &mut State::default());
        assert_eq!(commands[0]["kind"], "bridge");
        assert_eq!(commands[0]["ids"][0], 1);
    }
}

#[test]
fn if25_active_field_fill_reuses_earlier_far_cell_sort_for_later_equal_distance() {
    for swapped in [false, true] {
        let positions = if swapped {
            [(151.0, 163.0), (161.0, 153.0)]
        } else {
            [(161.0, 153.0), (151.0, 163.0)]
        };
        let expected = if swapped { 2 } else { 1 };
        let mut i = input(vec![
            unit(1, positions[0].0, positions[0].1),
            unit(2, positions[1].0, positions[1].1),
        ]);
        i.points.push(PointObservation {
            at: Point { x: 111.0, z: 113.0 },
            vp: 1.0,
            mp: 1.0,
            owner: Some(0),
            cut: false,
            locked: false,
        });
        for cell in [50 * 128 + 50, 61 * 128 + 60] {
            i.chars[cell] = b'F';
        }
        let commands = run(i, &mut State::default());
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0]["kind"], "fill");
        assert_eq!(commands[0]["x"], 121.0);
        assert_eq!(commands[0]["z"], 123.0);
        assert_eq!(commands[0]["ids"][0], expected);
    }
}

#[test]
fn if25_active_field_demine_reuses_earlier_far_mine_sort_for_later_equal_distance() {
    for swapped in [false, true] {
        let positions = if swapped {
            [(121.0, 101.0), (101.0, 121.0)]
        } else {
            [(101.0, 121.0), (121.0, 101.0)]
        };
        let expected = if swapped { 2 } else { 1 };
        let mut i = input(vec![
            unit(1, positions[0].0, positions[0].1),
            unit(2, positions[1].0, positions[1].1),
        ]);
        i.runtime
            .as_mut()
            .unwrap()
            .field
            .as_mut()
            .unwrap()
            .found_mines = vec![Point { x: 21.0, z: 31.0 }, Point { x: 121.0, z: 121.0 }];
        let commands = run(i, &mut State::default());
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0]["kind"], "demine");
        assert_eq!(commands[0]["x"], 121.0);
        assert_eq!(commands[0]["ids"][0], expected);
    }
}

#[test]
fn if25_active_field_rejects_nonfinite_mine_facts_before_memory_mutation() {
    let mut i = input(vec![unit(1, 101.0, 121.0)]);
    i.runtime
        .as_mut()
        .unwrap()
        .field
        .as_mut()
        .unwrap()
        .found_mines = vec![Point {
        x: f64::NAN,
        z: 121.0,
    }];
    let mut state = State::default();
    let before = state.clone();
    if let Ok(observation) = Observation::new(i) {
        assert!(field_work(&observation, &mut state, &mut BTreeSet::new(), |_| true).is_err());
    }
    assert_eq!(state, before);
}
