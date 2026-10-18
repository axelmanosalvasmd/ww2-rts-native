use super::*;
use crate::{
    commander_ai::{Input, PlayerObservation, ResearchObservation},
    commander_tactics::{Context, UnitFacts},
    economy::{Faction, TechState},
};
use std::collections::BTreeMap;

fn unit(id: u32, kind: &str, x: f64, z: f64) -> UnitObservation {
    let catalog = Catalog::compiled().unwrap();
    let d = catalog.unit(kind).unwrap();
    UnitObservation {
        id,
        kind: kind.into(),
        owner: 0,
        at: Point { x, z },
        hp: number(d, "models").unwrap() * number(d, "hpPer").unwrap(),
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
fn fixture(buildings: &[&str]) -> Input {
    let mut units = vec![unit(1, "rifle", 40.0, 40.0), unit(2, "rifle", 42.0, 40.0)];
    units.extend(
        buildings
            .iter()
            .enumerate()
            .map(|(i, kind)| unit(10 + i as u32, kind, 40.0 + i as f64 * 20.0, 60.0)),
    );
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
        width: 120,
        height: 100,
        tick: 4000,
        slot: 0,
        players: vec![
            PlayerObservation {
                team: 0,
                out: false,
                spawn: Point { x: 40.0, z: 40.0 },
            },
            PlayerObservation {
                team: 1,
                out: false,
                spawn: Point { x: 220.0, z: 180.0 },
            },
        ],
        units,
        points: vec![],
        flags: vec![LAND; 12000],
        chars: vec![b'.'; 12000],
        manpower: 2000.0,
        research: None,
        runtime: Some(Context {
            units: facts,
            ..Context::default()
        }),
    }
}
fn terrain(input: &Input) -> Game {
    Game {
        w: input.width,
        h: input.height,
        flags: input.flags.clone(),
        height: None,
        house: vec![0; input.flags.len()],
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
    }
}
fn run(input: Input, budget: f64, sees: impl Fn(Point) -> bool) -> Plan {
    let ground = terrain(&input);
    let observation = Observation::new(input).unwrap();
    let squads: Vec<_> = observation
        .input()
        .units
        .iter()
        .filter(|u| u.owner == 0 && u.id < 10)
        .collect();
    plan(
        &observation,
        &squads,
        budget,
        false,
        &ground,
        sees,
        &mut MatchRandom::new(77),
    )
    .unwrap()
}
fn kind(plan: &Plan) -> Option<&str> {
    plan.commands.first().and_then(|c| c["kind"].as_str())
}
fn tier(input: &mut Input, n: u8) {
    input.research = Some(ResearchObservation {
        faction: Faction::Usa,
        tech: TechState::new(n, 0, 0, 0).unwrap(),
        busy: vec![],
        stock_base: false,
        armory_ready: false,
        standard_army: true,
    });
}

#[test]
fn intent_ai52_rebuilds_hq_then_unlocks_production_and_tier_two_armory() {
    for (buildings, expected) in [
        (vec![], "hq"),
        (vec!["hq"], "barracks"),
        (vec!["hq", "barracks"], "motorpool"),
        (vec!["hq", "barracks", "motorpool"], "airfield"),
    ] {
        let result = run(fixture(&buildings), 2000.0, |_| true);
        assert_eq!(kind(&result), Some(expected));
        assert_eq!(result.builders, vec![1]);
        assert_eq!(result.reserve, 0.0);
        assert!(result.remaining_mp < 2000.0);
    }
    let mut input = fixture(&["hq", "barracks", "motorpool", "airfield"]);
    tier(&mut input, 2);
    assert_eq!(kind(&run(input, 2000.0, |_| true)), Some("armory"));
    let mut input = fixture(&["hq", "barracks"]);
    tier(&mut input, 1);
    assert!(
        run(input, 2000.0, |_| true).commands.is_empty(),
        "tier one cannot begin Motor Pool"
    );
}

#[test]
fn intent_ai52_finishes_before_repair_and_repairs_strictly_below_eighty_percent() {
    let mut input = fixture(&["hq", "barracks"]);
    input.units[2].hp *= 0.5;
    input
        .runtime
        .as_mut()
        .unwrap()
        .units
        .get_mut(&11)
        .unwrap()
        .built = 0.5;
    let result = run(input, 2000.0, |_| true);
    assert_eq!(
        result.commands[0],
        serde_json::json!({"t":"assist","ids":[1],"id":11})
    );
    for fraction in [0.799, 0.8] {
        let mut input = fixture(&["hq", "barracks"]);
        input.units[2].hp *= fraction;
        let result = run(input, 2000.0, |_| true);
        assert_eq!(
            result.commands[0]["t"],
            if fraction < 0.8 { "assist" } else { "build" }
        );
    }
}

#[test]
fn intent_ai52_nearest_free_builder_working_freeze_and_unaffordable_reserve() {
    let mut input = fixture(&["hq"]);
    input.units[0].target_id = 10;
    assert_eq!(run(input.clone(), 2000.0, |_| true).builders, vec![2]);
    input.units[0].build = true;
    let working = run(input, 2000.0, |_| true);
    assert!(working.commands.is_empty());
    assert!(working.reserve > 0.0);
    let poor = run(fixture(&[]), 0.0, |_| true);
    assert!(poor.commands.is_empty());
    assert_eq!(poor.reserve, 200.0);
    assert_eq!(poor.remaining_mp, 0.0);
}

#[test]
fn intent_ai52_seen_site_search_follows_radii_and_never_uses_unseen_site() {
    let mut input = fixture(&[]);
    input.units.clear();
    input.units.push(unit(1, "rifle", 40.0, 40.0));
    let first = run(input.clone(), 2000.0, |_| true);
    let at = Point {
        x: first.commands[0]["x"].as_f64().unwrap(),
        z: first.commands[0]["z"].as_f64().unwrap(),
    };
    assert_eq!(at, Point { x: 51.0, z: 39.0 });
    let later = run(input.clone(), 2000.0, |p| p.x >= 60.0);
    assert!(later.commands[0]["x"].as_f64().unwrap() >= 60.0);
    let hidden = run(input, 2000.0, |_| false);
    assert!(hidden.commands.is_empty());
    assert_eq!(hidden.reserve, 200.0);
}

#[test]
fn intent_ai52_naval_coast_horde_shared_bases_and_construction_mode_exclusion() {
    let buildings = ["hq", "barracks", "motorpool", "airfield"];
    let mut input = fixture(&buildings);
    input.runtime.as_mut().unwrap().naval = true;
    assert!(
        run(input.clone(), 2000.0, |_| true).commands.is_empty(),
        "inland naval map must retain shipyard reserve"
    );
    input.flags.fill(0);
    assert_eq!(kind(&run(input, 2000.0, |_| true)), Some("shipyard"));
    let mut input = fixture(&["hq"]);
    input.runtime.as_mut().unwrap().mode = Mode::Horde;
    input.players[1].team = 0;
    input.units[2].owner = 1;
    assert_eq!(
        kind(&run(input.clone(), 2000.0, |_| true)),
        Some("barracks"),
        "Horde defender uses allied HQ"
    );
    input.runtime.as_mut().unwrap().horde = true;
    assert!(
        run(input, 2000.0, |_| true).commands.is_empty(),
        "director does not construct a skirmish base"
    );
    for mode in [Mode::Classic, Mode::World] {
        let mut input = fixture(&[]);
        input.runtime.as_mut().unwrap().mode = mode;
        let plan = run(input, 2000.0, |_| true);
        assert!(plan.commands.is_empty());
        assert!(plan.builders.is_empty());
        assert_eq!(plan.remaining_mp, 2000.0);
    }
}

#[test]
fn intent_ai52_assault_defender_skips_forward_hq_but_builds_home_tech() {
    let mut input = fixture(&["hq"]);
    let context = input.runtime.as_mut().unwrap();
    context.mode = Mode::Assault;
    context.defender = true;
    assert_eq!(kind(&run(input, 2000.0, |_| true)), Some("barracks"));
}

#[test]
fn intent_ai52_site_search_avoids_delivered_supply_node_lanes() {
    let mut input = fixture(&[]);
    let aim = Point { x: 100.0, z: 100.0 };
    let ground = terrain(&input);
    let observation = Observation::new(input.clone()).unwrap();
    let clear = site(&observation, &ground, aim, 2, false, |_| true)
        .unwrap()
        .unwrap();
    assert_eq!(clear, aim);
    input.runtime.as_mut().unwrap().site_nodes = vec![aim];
    let observation = Observation::new(input).unwrap();
    let occupied = site(&observation, &ground, aim, 2, false, |_| true)
        .unwrap()
        .unwrap();
    assert_eq!(occupied, Point { x: 106.0, z: 100.0 });
    assert!(distance(occupied, aim) >= 5.0);
    assert_eq!(
        site(
            &observation,
            &ground,
            Point { x: 104.0, z: 104.0 },
            4,
            false,
            |_| true
        )
        .unwrap(),
        None
    );
}

#[test]
fn if25_construction_builder_ties_keep_available_order_and_repairs_keep_observation_order() {
    let mut input = fixture(&["hq", "barracks"]);
    input.units[1].at = input.units[0].at;
    input.units[3].at = input.units[2].at;
    for u in input.units.iter_mut().skip(2) {
        u.hp *= 0.5;
    }
    let ground = terrain(&input);
    let observation = Observation::new(input).unwrap();
    let units = &observation.input().units;
    let available = [&units[1], &units[0]];
    let result = plan(
        &observation,
        &available,
        2000.0,
        false,
        &ground,
        |_| true,
        &mut MatchRandom::new(77),
    )
    .unwrap();
    assert_eq!(
        result.commands[0],
        serde_json::json!({"t":"assist","ids":[2],"id":10})
    );
    let available = [&units[0], &units[1]];
    let result = plan(
        &observation,
        &available,
        2000.0,
        false,
        &ground,
        |_| true,
        &mut MatchRandom::new(77),
    )
    .unwrap();
    assert_eq!(
        result.commands[0],
        serde_json::json!({"t":"assist","ids":[1],"id":10})
    );
}

#[test]
fn intent_ai66_classic_site_draws_once_per_eligible_engineer_even_without_build() {
    use crate::random::StreamId;
    for (buildings, next) in [
        (vec!["hq"], "barracks"),
        (vec!["hq", "barracks"], "motorpool"),
        (
            vec!["hq", "barracks", "motorpool", "depot", "depot", "depot"],
            "airfield",
        ),
        (vec!["hq", "barracks", "motorpool"], "flakpos"),
    ] {
        for (budget, visible) in [(0.0, true), (2000.0, false), (2000.0, true)] {
            let mut input = fixture(&buildings);
            input.runtime.as_mut().unwrap().mode = Mode::Classic;
            for worker in &mut input.units[..2] {
                worker.kind = "engineer".into();
            }
            if next == "flakpos" {
                let mut plane = unit(40, "fighter", 60.0, 60.0);
                plane.owner = 1;
                input.units.push(plane);
                input.runtime.as_mut().unwrap().units.insert(
                    40,
                    UnitFacts {
                        built: 1.0,
                        airborne: true,
                        ..UnitFacts::default()
                    },
                );
            }
            let ground = terrain(&input);
            let observation = Observation::new(input).unwrap();
            let workers: Vec<_> = observation.input().units[..2].iter().collect();
            let mut random = MatchRandom::new(661);
            let mut expected = random.clone();
            for _ in 0..2 {
                expected.next_fraction(StreamId::Ai(0));
            }
            let result = plan(
                &observation,
                &workers,
                budget,
                false,
                &ground,
                |_| visible,
                &mut random,
            )
            .unwrap();
            assert_eq!(
                random, expected,
                "{next}, budget={budget}, visible={visible}"
            );
            if budget == 0.0 || !visible {
                assert!(result.commands.is_empty());
            } else {
                assert_eq!(result.commands.len(), 2);
                assert!(
                    result
                        .commands
                        .iter()
                        .all(|c| c["t"] == "build" && c["kind"] == next)
                );
                if next != "flakpos" {
                    assert_eq!(result.commands[0]["x"], result.commands[1]["x"]);
                    assert_eq!(result.commands[0]["z"], result.commands[1]["z"]);
                }
            }
        }
    }
    for reason in ["retreat", "building", "repair", "depot", "complete"] {
        let buildings = if reason == "complete" {
            vec!["hq", "barracks", "motorpool", "airfield"]
        } else {
            vec!["hq"]
        };
        let mut input = fixture(&buildings);
        input.runtime.as_mut().unwrap().mode = Mode::Classic;
        for worker in &mut input.units[..2] {
            worker.kind = "engineer".into();
            worker.retreating = reason == "retreat";
            worker.build = reason == "building";
        }
        if reason == "repair" {
            input.units[2].hp *= 0.5;
        }
        if reason == "depot" {
            input
                .runtime
                .as_mut()
                .unwrap()
                .nodes
                .push(crate::commander_tactics::NodeObservation {
                    at: Point { x: 100.0, z: 100.0 },
                    rate: 0.0,
                    fuel: false,
                    visible: false,
                });
        }
        let ground = terrain(&input);
        let observation = Observation::new(input).unwrap();
        let workers = observation.input().units[..2].iter().collect::<Vec<_>>();
        let mut random = MatchRandom::new(662);
        let before = random.clone();
        let result = plan(
            &observation,
            &workers,
            2000.0,
            false,
            &ground,
            |_| true,
            &mut random,
        )
        .unwrap();
        assert_eq!(random, before, "{reason}");
        if reason == "repair" {
            assert!(result.commands.iter().all(|c| c["t"] == "assist"));
        } else if reason == "depot" {
            assert_eq!(
                result.commands,
                [json!({"t":"build", "ids":[workers[0].id],
                "kind":"depot", "x":100.0, "z":100.0})]
            );
        } else {
            assert!(result.commands.is_empty(), "{reason}");
        }
    }
}

#[test]
fn if25_classic_repair_ties_keep_observation_order_for_each_engineer() {
    let mut input = fixture(&["hq", "barracks"]);
    input.runtime.as_mut().unwrap().mode = Mode::Classic;
    for worker in &mut input.units[..2] {
        worker.kind = "engineer".into();
    }
    input.units[3].at = input.units[2].at;
    for building in &mut input.units[2..] {
        building.hp *= 0.5;
    }
    let ground = terrain(&input);
    let observation = Observation::new(input).unwrap();
    let units = &observation.input().units;
    for workers in [[&units[1], &units[0]], [&units[0], &units[1]]] {
        let result = plan(
            &observation,
            &workers,
            2000.0,
            false,
            &ground,
            |_| true,
            &mut MatchRandom::new(663),
        )
        .unwrap();
        assert_eq!(result.commands.len(), 2);
        for (command, worker) in result.commands.iter().zip(workers) {
            assert_eq!(*command, json!({"t":"assist","ids":[worker.id],"id":10}));
        }
    }
}
