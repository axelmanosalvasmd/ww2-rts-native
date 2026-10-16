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
fn run(input: Input, budget: f64, sees: impl Fn(Point) -> bool) -> BasePlan {
    let ground = terrain(&input);
    let observation = Observation::new(input).unwrap();
    let squads: Vec<_> = observation
        .input()
        .units
        .iter()
        .filter(|u| u.owner == 0 && u.id < 10)
        .collect();
    plan(&observation, &squads, budget, &ground, sees).unwrap()
}
fn kind(plan: &BasePlan) -> Option<&str> {
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
        assert_eq!(result.assigned, Some(1));
        assert_eq!(result.reserve, 0.0);
        assert!(result.spent > 0.0);
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
    assert_eq!(run(input.clone(), 2000.0, |_| true).assigned, Some(2));
    input.units[0].build = true;
    let working = run(input, 2000.0, |_| true);
    assert!(working.commands.is_empty());
    assert!(working.reserve > 0.0);
    let poor = run(fixture(&[]), 0.0, |_| true);
    assert!(poor.commands.is_empty());
    assert_eq!(poor.reserve, 200.0);
    assert_eq!(poor.spent, 0.0);
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
        assert_eq!(run(input, 2000.0, |_| true), BasePlan::default());
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
