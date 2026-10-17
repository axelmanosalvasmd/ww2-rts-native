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
        flags: vec![0; 128 * 128],
        chars: vec![b'.'; 128 * 128],
        manpower: 1000.0,
        research: None,
        runtime: Some(Context::default()),
    }
}

fn run(i: Input, order: &[usize], state: &mut State) -> Plan {
    let observation = Observation::new(i).unwrap();
    let squads: Vec<_> = order
        .iter()
        .map(|index| &observation.input().units[*index])
        .collect();
    plan(&observation, &squads, &[], &BTreeSet::new(), state, |_| {
        true
    })
    .unwrap()
}

#[test]
fn if25_ai_bridge_ingestion_sorts_new_cells_and_preserves_prior_memory_ties() {
    let left = 50 * 128 + 40;
    let right = 50 * 128 + 60;
    for crew_order in [[0, 1], [1, 0]] {
        let mut i = input(vec![unit(1, 81.0, 91.0), unit(2, 81.0, 111.0)]);
        i.chars[left] = b'W';
        i.chars[right] = b'W';
        i.runtime.as_mut().unwrap().bridge_cells = vec![right, left, right];
        let mut state = State::default();
        let planned = run(i.clone(), &crew_order, &mut state);
        assert_eq!(state.bridges, [left, right]);
        assert_eq!(planned.commands[0]["kind"], "bridge");
        assert_eq!(planned.commands[0]["x"], 81.0);
        assert_eq!(planned.commands[0]["ids"][0], (crew_order[0] + 1) as u32);
        assert_eq!(planned.commands[0]["dir"], 0.0);

        let mut remembered = State {
            bridges: vec![right],
            ..State::default()
        };
        let planned = run(i, &crew_order, &mut remembered);
        assert_eq!(remembered.bridges, [right, left]);
        assert_eq!(planned.commands[0]["x"], 121.0);
    }
}

#[test]
fn if25_ai_bridge_safety_and_active_crew_filter_the_actual_gap() {
    let left = 50 * 128 + 40;
    let right = 50 * 128 + 60;
    let mut i = input(vec![unit(1, 101.0, 91.0)]);
    i.chars[left] = b'W';
    i.chars[right] = b'W';
    i.runtime.as_mut().unwrap().bridge_cells = vec![right, left];
    let observation = Observation::new(i.clone()).unwrap();
    let enemy = unit(9, 61.0, 101.0);
    let mut state = State::default();
    let planned = plan(
        &observation,
        &[&observation.input().units[0]],
        &[&enemy],
        &BTreeSet::new(),
        &mut state,
        |_| true,
    )
    .unwrap();
    assert_eq!(planned.commands[0]["x"], 121.0);
    i.tick += 40;
    i.units[0].dig = true;
    assert!(run(i, &[0], &mut state).commands.is_empty());
    assert!(state.bridge_crew[&1].dug);
}

#[test]
fn if25_ai_fill_repeated_sort_keeps_the_earlier_far_cell_order_on_a_later_tie() {
    for order in [[0, 1], [1, 0]] {
        let mut i = input(vec![unit(1, 161.0, 153.0), unit(2, 151.0, 163.0)]);
        i.points.push(PointObservation {
            at: Point { x: 111.0, z: 113.0 },
            vp: 1.0,
            mp: 1.0,
            owner: Some(0),
            cut: false,
            locked: false,
        });
        let early = 50 * 128 + 50;
        let later = 61 * 128 + 60;
        i.runtime.as_mut().unwrap().spoiled_cells = vec![later, early];
        let mut state = State::default();
        let planned = run(i, &order, &mut state);
        assert_eq!(planned.commands.len(), 1);
        assert_eq!(planned.commands[0]["kind"], "fill");
        assert_eq!(planned.commands[0]["x"], 121.0);
        assert_eq!(planned.commands[0]["z"], 123.0);
        assert_eq!(planned.commands[0]["ids"][0], 1);
    }
}

#[test]
fn if25_ai_demine_repeated_sort_keeps_earlier_mine_order_on_a_later_tie() {
    for order in [[0, 1], [1, 0]] {
        let mut i = input(vec![unit(1, 101.0, 121.0), unit(2, 121.0, 101.0)]);
        i.runtime.as_mut().unwrap().found_mines =
            vec![Point { x: 21.0, z: 31.0 }, Point { x: 121.0, z: 121.0 }];
        let mut state = State::default();
        let planned = run(i, &order, &mut state);
        assert_eq!(planned.commands.len(), 1);
        assert_eq!(planned.commands[0]["kind"], "demine");
        assert_eq!(planned.commands[0]["x"], 121.0);
        assert_eq!(planned.commands[0]["ids"][0], 1);
    }
}

#[test]
fn if25_ai_engineering_crew_memory_expires_after_completion_loss_or_timeout() {
    let mut i = input(vec![unit(1, 101.0, 121.0)]);
    i.runtime.as_mut().unwrap().found_mines = vec![Point { x: 121.0, z: 121.0 }];
    let mut state = State::default();
    assert_eq!(
        run(i.clone(), &[0], &mut state).commands[0]["kind"],
        "demine"
    );
    i.tick += 40;
    i.units[0].dig = true;
    assert!(run(i.clone(), &[0], &mut state).commands.is_empty());
    assert!(state.demine_crew[&1].dug);
    i.units[0].dig = false;
    i.runtime.as_mut().unwrap().found_mines.clear();
    i.tick += 40;
    assert!(run(i.clone(), &[0], &mut state).commands.is_empty());
    assert!(state.demine_crew.is_empty());
    state.demine_crew.insert(
        1,
        Job {
            time: 0.0,
            dug: false,
        },
    );
    i.tick = 1840;
    run(i.clone(), &[0], &mut state);
    assert!(state.demine_crew.is_empty());
    state.demine_crew.insert(
        9,
        Job {
            time: 90.0,
            dug: false,
        },
    );
    run(i, &[0], &mut state);
    assert!(state.demine_crew.is_empty());
}

#[test]
fn if25_ai_engineering_unseen_jobs_approach_the_bank_hole_and_mines_safely() {
    for kind in ["bridge", "fill", "demine"] {
        let mut i = input(vec![unit(1, 101.0, 101.0)]);
        let target = Point { x: 121.0, z: 101.0 };
        let context = i.runtime.as_mut().unwrap();
        match kind {
            "bridge" => {
                context.bridge_cells = vec![50 * 128 + 60];
                i.chars[50 * 128 + 60] = b'W';
            }
            "fill" => {
                context.spoiled_cells = vec![50 * 128 + 60];
                context.nodes = vec![target];
            }
            _ => context.found_mines = vec![target],
        }
        let observation = Observation::new(i).unwrap();
        let planned = plan(
            &observation,
            &[&observation.input().units[0]],
            &[],
            &BTreeSet::new(),
            &mut State::default(),
            |_| false,
        )
        .unwrap();
        assert_eq!(planned.commands[0]["t"], "move", "{kind}");
        let x = planned.commands[0]["orders"][0][1].as_f64().unwrap();
        assert_eq!(
            x,
            match kind {
                "bridge" => 114.0,
                "fill" => 121.0,
                _ => 112.0,
            }
        );
        assert_eq!(planned.builders, [1]);
    }
}

#[test]
fn if25_ai_engineering_rejects_nonfinite_delivered_mines_before_memory_changes() {
    let mut i = input(vec![unit(1, 101.0, 101.0)]);
    i.runtime.as_mut().unwrap().found_mines = vec![Point {
        x: f64::NAN,
        z: 101.0,
    }];
    assert!(Observation::new(i).is_err());
}

#[test]
fn if25_ai_engineering_pending_and_active_jobs_continue_after_serde_roundtrip() {
    let mut i = input(vec![unit(1, 101.0, 121.0)]);
    i.runtime.as_mut().unwrap().found_mines = vec![Point { x: 121.0, z: 121.0 }];
    let mut state = State::default();
    run(i.clone(), &[0], &mut state);
    state.bridges = vec![42, 17];
    let mut restored: State = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(state, restored);
    i.tick += 40;
    assert!(
        run(i.clone(), &[0], &mut restored).commands.is_empty(),
        "pending motor input must not be duplicated"
    );
    i.units[0].dig = true;
    run(i.clone(), &[0], &mut restored);
    assert!(restored.demine_crew[&1].dug);
    let mut active: State =
        serde_json::from_slice(&serde_json::to_vec(&restored).unwrap()).unwrap();
    assert_eq!(active, restored);
    i.tick += 40;
    i.units[0].dig = false;
    assert_eq!(run(i, &[0], &mut active).commands[0]["kind"], "demine");
}
