use super::*;
use crate::{
    commander_ai::{Input, PlayerObservation},
    commander_tactics::{Context, UnitFacts},
    game::LAND,
};

fn fixture() -> Input {
    let units = [1, 2]
        .into_iter()
        .map(|id| UnitObservation {
            id,
            kind: "engineer".into(),
            owner: 0,
            at: Point { x: 40.0, z: 40.0 },
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
        })
        .collect::<Vec<_>>();
    Input {
        width: 80,
        height: 80,
        tick: 4000,
        slot: 0,
        players: vec![PlayerObservation {
            team: 0,
            out: false,
            spawn: Point { x: 20.0, z: 20.0 },
        }],
        points: vec![],
        flags: vec![LAND; 6400],
        chars: vec![b'.'; 6400],
        manpower: 2000.0,
        research: None,
        runtime: Some(Context {
            mode: Mode::Classic,
            units: units.iter().map(|u| (u.id, UnitFacts::default())).collect(),
            nodes: vec![Point { x: 50.0, z: 40.0 }, Point { x: 70.0, z: 40.0 }]
                .into_iter()
                .map(|at| crate::commander_tactics::NodeObservation {
                    at,
                    rate: 0.0,
                    fuel: false,
                    visible: false,
                })
                .collect(),
            node_worth: vec![1.0, 2.0],
            ..Context::default()
        }),
        units,
    }
}
fn run(
    mut input: Input,
    budget: f64,
    sees: impl Fn(Point) -> bool,
    reservations: &mut BTreeMap<u32, usize>,
) -> Plan {
    let terrain = Game {
        w: input.width,
        h: input.height,
        flags: input.flags.clone(),
        height: None,
        house: vec![0; 6400],
        chars: input.chars.clone(),
        ground: None,
        objects: None,
        wear: None,
        cell_hp: vec![],
        trench_front: None,
        tough: None,
        wet: None,
        reveal: false,
        wrecks: vec![],
        tick: input.tick,
        vision_tick: None,
        terrain_version: 0,
        mode: None,
        naval: false,
        weather: None,
        rain: 0.0,
        players: vec![],
        units: BTreeMap::new(),
        smokes: vec![],
        strikes: vec![],
        sections: BTreeMap::new(),
    };
    let requested = input.units.iter().map(|u| u.id).collect::<Vec<_>>();
    input.units.sort_by_key(|u| u.id);
    let observation = Observation::new(input).unwrap();
    let available = requested
        .iter()
        .map(|id| {
            observation
                .input()
                .units
                .iter()
                .find(|u| u.id == *id)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let mut random = MatchRandom::new(77);
    let before = random.snapshot();
    let result = plan_with_nodes(
        &observation,
        &available,
        budget,
        false,
        &terrain,
        sees,
        &mut random,
        reservations,
    )
    .unwrap();
    assert_eq!(
        random.snapshot(),
        before,
        "Depot planning consumes no flak angle draw"
    );
    result
}

#[test]
fn if25_classic_depot_richness_ties_keep_node_and_engineer_order_with_reservations() {
    for reverse_nodes in [false, true] {
        for reverse_engineers in [false, true] {
            let mut input = fixture();
            if reverse_nodes {
                let context = input.runtime.as_mut().unwrap();
                context.nodes.reverse();
                context.node_worth.reverse();
            }
            if reverse_engineers {
                input.units.reverse();
            }
            let expected = input.runtime.as_ref().unwrap().nodes.clone();
            let ids = input.units.iter().map(|u| u.id).collect::<Vec<_>>();
            let mut reservations = BTreeMap::new();
            let first = run(input.clone(), 2000.0, |_| true, &mut reservations);
            for (i, &id) in ids.iter().enumerate() {
                assert_eq!(
                    first.commands[i],
                    json!({"t":"build","ids":[id],"kind":"depot","x":expected[i].at.x,"z":expected[i].at.z})
                );
                assert_eq!(reservations[&id], i);
            }
            assert_eq!(
                first.remaining_mp, 2000.0,
                "Classic proposals share the delivered budget"
            );
            input.units.reverse();
            let second = run(input.clone(), 2000.0, |_| true, &mut reservations);
            assert_eq!(second.commands[0], first.commands[1]);
            assert_eq!(second.commands[1], first.commands[0]);
            // Reservations use the existing AI state codec and survive the next look.
            let stored = crate::commander_tactics::State {
                node_assignments: reservations.clone(),
                ..crate::commander_tactics::State::default()
            };
            let restored: crate::commander_tactics::State =
                serde_json::from_value(serde_json::to_value(&stored).unwrap()).unwrap();
            assert_eq!(restored.node_assignments, reservations);
            reservations = restored.node_assignments;
            input.units.remove(0);
            run(input, 2000.0, |_| true, &mut reservations);
            assert_eq!(
                reservations.len(),
                1,
                "absent engineers release their reservation"
            );
        }
    }
}

#[test]
fn if25_classic_depot_values_richer_nodes_and_walks_only_without_a_path() {
    let mut input = fixture();
    input.units.truncate(1);
    // The farther node wins after its extra richness exceeds the distance cost.
    input.runtime.as_mut().unwrap().node_worth[1] = 2.5;
    let mut reservations = BTreeMap::new();
    let result = run(input.clone(), 0.0, |_| true, &mut reservations);
    assert_eq!(
        result.commands,
        vec![json!({"t":"move","orders":[[1,70.0,40.0]]})]
    );
    assert_eq!(reservations[&1], 1);
    assert_eq!(result.reserve, 0.0);
    let hidden = run(input.clone(), 2000.0, |_| false, &mut reservations);
    assert_eq!(hidden.commands, result.commands);
    input.units[0].path.push(Point { x: 60.0, z: 40.0 });
    assert!(
        run(input, 0.0, |_| true, &mut reservations)
            .commands
            .is_empty()
    );
}

#[test]
fn if25_classic_depot_skips_delivered_claims_before_scoring() {
    let mut input = fixture();
    input.units.truncate(1);
    input
        .runtime
        .as_mut()
        .unwrap()
        .known_depots
        .push(Point { x: 50.0, z: 40.0 });
    let result = run(input, 2000.0, |_| true, &mut BTreeMap::new());
    assert_eq!(
        result.commands,
        vec![json!({"t":"build","ids":[1],"kind":"depot","x":70.0,"z":40.0})]
    );
}
