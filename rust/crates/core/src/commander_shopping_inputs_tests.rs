use super::tests::{fixture, unit};
use super::*;
use crate::commander_ai::{Input, ResearchObservation};

fn cost(kind: &str) -> f64 {
    num(Catalog::compiled().unwrap().unit(kind).unwrap(), "cost")
}
fn shopping(
    input: Input,
    budget: f64,
    reserves: (f64, f64),
) -> crate::commander_shopping::Shopping {
    let observation = Observation::new(input).unwrap();
    let input = observation.input();
    let mine: Vec<_> = input
        .units
        .iter()
        .filter(|u| {
            u.owner == input.slot
                && u.kind != "engineer"
                && !boolean(
                    Catalog::compiled().unwrap().unit(&u.kind).unwrap(),
                    "structure",
                )
        })
        .collect();
    crate::commander_shopping::plan(
        &observation,
        &mine,
        &[],
        0,
        &Mind::default(),
        None,
        budget,
        input
            .units
            .iter()
            .filter(|u| u.owner == input.slot && u.kind == "engineer")
            .count(),
        reserves,
        Level::Normal,
    )
    .unwrap()
}
fn look(input: Input) -> Vec<Value> {
    plan(
        &Observation::new(input).unwrap(),
        &[],
        &mut State::default(),
        Level::Normal,
        &mut MatchRandom::new(41),
    )
    .unwrap()
}

#[test]
fn intent_ai33_delivered_depots_claim_nodes_without_hidden_authority_reads() {
    let mut input = fixture(vec![
        unit(1, "engineer", 20.0),
        unit(2, "hq", 20.0),
        unit(3, "barracks", 30.0),
    ]);
    let context = input.runtime.as_mut().unwrap();
    context.mode = Mode::Classic;
    context.nodes = vec![Point { x: 70.0, z: 20.0 }];
    assert_eq!(shopping(input.clone(), 1000.0, (0.0, 0.0)).buy, "engineer");
    // A nearby Barracks does not claim a resource node.
    input.units[2].at.x = 70.0;
    assert_eq!(shopping(input.clone(), 1000.0, (0.0, 0.0)).buy, "engineer");
    // Depot memories are already seat knowledge. An unknown hidden depot has no input here.
    input.runtime.as_mut().unwrap().known_depots = vec![Point { x: 62.0, z: 20.0 }];
    assert_eq!(shopping(input.clone(), 1000.0, (0.0, 0.0)).buy, "engineer");
    input.runtime.as_mut().unwrap().known_depots[0].x = 62.1;
    assert_ne!(shopping(input.clone(), 1000.0, (0.0, 0.0)).buy, "engineer");
    input.runtime.as_mut().unwrap().known_depots.clear();
    let mut depot = unit(4, "depot", 70.0);
    depot.owner = 2;
    input.units.push(depot);
    assert_ne!(shopping(input.clone(), 1000.0, (0.0, 0.0)).buy, "engineer");
    // World engineer demand uses all unclaimed nodes, not only currently owned ground.
    input.runtime.as_mut().unwrap().mode = Mode::World;
    input
        .runtime
        .as_mut()
        .unwrap()
        .nodes
        .push(Point { x: 100.0, z: 40.0 });
    assert_eq!(shopping(input, 1000.0, (0.0, 0.0)).buy, "engineer");
}

#[test]
fn intent_ai33_actual_build_reserve_protects_three_squads_but_not_two_and_caps_buys() {
    let build = cost("motorpool");
    let rifle = crate::economy::price_of(Mode::Conquest, "rifle")
        .unwrap()
        .mp;
    for (squads, expected) in [(2, 4), (3, 2)] {
        let mut units: Vec<_> = (1..=squads).map(|id| unit(id, "mg", 30.0)).collect();
        units.push(unit(10, "hq", 20.0));
        units.push(unit(11, "barracks", 24.0));
        let mut input = fixture(units);
        input.runtime.as_mut().unwrap().large_army = true;
        input.runtime.as_mut().unwrap().faction = economy::Faction::Germany;
        input.manpower = build + rifle * 2.0;
        let commands = look(input);
        assert_eq!(
            commands.iter().filter(|c| c["t"] == "buy").count(),
            expected,
            "{squads}"
        );
    }
    let mut input = fixture(vec![
        unit(1, "mg", 30.0),
        unit(2, "mg", 30.0),
        unit(3, "mg", 30.0),
        unit(10, "hq", 20.0),
        unit(11, "barracks", 24.0),
    ]);
    input.runtime.as_mut().unwrap().large_army = true;
    input.manpower = build + rifle - 1.0;
    assert!(!look(input.clone()).iter().any(|c| c["t"] == "buy"));
    input.manpower = build + rifle;
    assert_eq!(look(input).iter().filter(|c| c["t"] == "buy").count(), 1);
}

#[test]
fn intent_ai33_fortification_reserves_follow_the_next_step_and_tier_bank() {
    let input = fixture(vec![
        unit(1, "rifle", 30.0),
        unit(2, "rifle", 32.0),
        unit(3, "rifle", 34.0),
        unit(4, "hq", 20.0),
    ]);
    let mut input = input;
    input.tick = 3000;
    let observation = Observation::new(input).unwrap();
    let context = observation.input().runtime.as_ref().unwrap();
    let ground = sight_game(&observation, context);
    let available = observation.input().units[..3].iter().collect::<Vec<_>>();
    let wall = num(
        Catalog::compiled()
            .unwrap()
            .table("FORTS")
            .unwrap()
            .get("wall")
            .unwrap(),
        "cost",
    );
    for (step, expected) in [
        (0, cost("tower")),
        (1, cost("pillbox")),
        (2, wall * 2.0),
        (3, wall * 2.0),
        (4, wall * 2.0),
        (5, wall * 2.0),
        (6, 0.0),
    ] {
        let mut steps = BTreeMap::from([(4, step)]);
        let r = crate::commander_fortification::plan(
            &observation,
            &available,
            expected - 1.0,
            0.0,
            0.0,
            &ground,
            |_| false,
            &mut steps,
        )
        .unwrap();
        assert_eq!(r.reserve, expected, "step {step}");
        assert!(r.commands.is_empty());
    }
    // Once the post-research wallet plus savings exceeds the bank, savings can fund the step.
    let mut steps = BTreeMap::new();
    let r = crate::commander_fortification::plan(
        &observation,
        &available,
        0.0,
        0.0,
        300.0 + cost("tower"),
        &ground,
        |_| false,
        &mut steps,
    )
    .unwrap();
    assert_eq!(r.reserve, 0.0);
    let r = crate::commander_fortification::plan(
        &observation,
        &available,
        cost("tower"),
        1.0,
        0.0,
        &ground,
        |_| false,
        &mut BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(r.reserve, cost("tower"));
    let mut low_tier = observation.input().clone();
    low_tier.research = Some(ResearchObservation {
        faction: economy::Faction::Germany,
        tech: economy::TechState::new(1, 0, 0, 0).unwrap(),
        busy: vec![],
        stock_base: false,
        armory_ready: false,
        standard_army: true,
    });
    let low_tier = Observation::new(low_tier).unwrap();
    assert_eq!(
        crate::commander_fortification::plan(
            &low_tier,
            &available,
            0.0,
            0.0,
            0.0,
            &ground,
            |_| false,
            &mut BTreeMap::from([(4, 1)])
        )
        .unwrap()
        .reserve,
        0.0
    );
    let mut input = observation.input().clone();
    input.tick = 2999;
    let observation = Observation::new(input).unwrap();
    assert_eq!(
        crate::commander_fortification::plan(
            &observation,
            &available,
            0.0,
            0.0,
            0.0,
            &ground,
            |_| false,
            &mut BTreeMap::new()
        )
        .unwrap()
        .reserve,
        0.0
    );
}

#[test]
fn intent_ai33_classic_reserves_only_non_depot_builds_and_rush_releases_them() {
    let mut input = fixture(vec![
        unit(1, "mg", 30.0),
        unit(2, "mg", 32.0),
        unit(3, "mg", 34.0),
        unit(4, "engineer", 20.0),
        unit(5, "hq", 18.0),
        unit(6, "barracks", 24.0),
    ]);
    input.units[3].build = true;
    let c = input.runtime.as_mut().unwrap();
    c.mode = Mode::Classic;
    c.faction = economy::Faction::Germany;
    let rifle = economy::price_of(Mode::Classic, "rifle").unwrap().mp;
    input.manpower = cost("motorpool") + rifle - 1.0;
    assert!(!look(input.clone()).iter().any(|c| c["t"] == "buy"));
    input.runtime.as_mut().unwrap().nodes = vec![Point { x: 100.0, z: 40.0 }];
    // An available depot is not protected by a building reserve, and raises engineer demand.
    let cmds = look(input.clone());
    assert!(
        cmds.iter()
            .any(|c| c["t"] == "buy" && c["unit"] == "engineer")
    );
    input.runtime.as_mut().unwrap().nodes.clear();
    input.tick = 4000;
    let tank = Sighting {
        id: 90,
        kind: "tank".into(),
        owner: 1,
        at: Point { x: 25.0, z: 20.0 },
        time: 199.0,
        worth: 300.0,
    };
    let mut enemy = unit(90, "tank", 25.0);
    enemy.owner = 1;
    input.units.push(enemy);
    // Armor counter shopping can fall back to rifles without a Motor Pool.
    for (age, buys) in [(1.0, true), (5.0, false)] {
        let sight = Sighting {
            time: 200.0 - age,
            ..tank.clone()
        };
        let cmds = plan(
            &Observation::new(input.clone()).unwrap(),
            &[sight],
            &mut State::default(),
            Level::Normal,
            &mut MatchRandom::new(41),
        )
        .unwrap();
        assert_eq!(cmds.iter().any(|c| c["t"] == "buy"), buys, "age {age}");
    }
}

#[test]
fn intent_ai33_fort_jobs_charge_gate_and_leave_deferred_wall_segments_for_ticks() {
    let mut input = fixture(vec![
        unit(1, "rifle", 30.0),
        unit(2, "mg", 32.0),
        unit(3, "mg", 34.0),
        unit(4, "hq", 20.0),
    ]);
    input.tick = 3000;
    input.players[1].spawn = Point { x: 140.0, z: 20.0 };
    let observation = Observation::new(input).unwrap();
    let context = observation.input().runtime.as_ref().unwrap();
    let ground = sight_game(&observation, context);
    let available = observation.input().units[..3].iter().collect::<Vec<_>>();
    for step in 2..=5 {
        let r = crate::commander_fortification::plan(
            &observation,
            &available,
            300.0,
            0.0,
            0.0,
            &ground,
            |_| true,
            &mut BTreeMap::from([(4, step)]),
        )
        .unwrap();
        assert_eq!(r.remaining_mp, if step == 3 { 150.0 } else { 300.0 });
        assert_eq!(r.reserve, 0.0);
        assert!(!r.commands.is_empty());
    }
}

#[test]
fn intent_ai33_world_engineers_count_unclaimed_nodes_while_build_reserves_use_owned_ground() {
    let mut input = fixture(vec![
        unit(1, "engineer", 20.0),
        unit(2, "hq", 18.0),
        unit(3, "barracks", 24.0),
    ]);
    let c = input.runtime.as_mut().unwrap();
    c.mode = Mode::World;
    c.nodes = vec![Point { x: 100.0, z: 40.0 }];
    for (owned, reserve) in [(false, cost("motorpool")), (true, 0.0)] {
        if owned {
            input.runtime.as_mut().unwrap().owned_nodes.insert(0);
        }
        let observation = Observation::new(input.clone()).unwrap();
        let context = observation.input().runtime.as_ref().unwrap();
        let ground = sight_game(&observation, context);
        let r = crate::commander_construction::plan(
            &observation,
            &[&observation.input().units[0]],
            0.0,
            false,
            &ground,
            |_| false,
            &mut MatchRandom::new(77),
        )
        .unwrap();
        assert_eq!(r.reserve, reserve);
        assert!(r.commands.is_empty());
        assert_eq!(
            shopping(input.clone(), 1000.0, (reserve, 0.0)).buy,
            "engineer"
        );
    }
}

#[test]
fn intent_ai33_base_and_fortification_share_the_wallet_without_overspending() {
    let mut input = fixture(vec![
        unit(1, "rifle", 99.0),
        unit(2, "rifle", 101.0),
        unit(3, "rifle", 103.0),
        unit(10, "hq", 99.0),
        unit(11, "barracks", 79.0),
    ]);
    input.width = 128;
    input.height = 128;
    input.chars = vec![b'.'; 128 * 128];
    input.flags = vec![0; 128 * 128];
    for u in &mut input.units {
        u.at.z = 99.0;
    }
    input.players[0].spawn = Point { x: 99.0, z: 99.0 };
    input.players[1].spawn = Point { x: 199.0, z: 39.0 };
    input.players[2].spawn = input.players[0].spawn;
    input.tick = 3000;
    input.manpower = 400.0;
    input.runtime.as_mut().unwrap().faction = economy::Faction::Germany;
    let mut state = State::default();
    let commands = plan(
        &Observation::new(input).unwrap(),
        &[],
        &mut state,
        Level::Normal,
        &mut MatchRandom::new(41),
    )
    .unwrap();
    let builds: Vec<_> = commands.iter().filter(|c| c["t"] == "build").collect();
    assert_eq!(builds.len(), 2, "{commands:?}");
    assert_eq!(builds[0]["kind"], "motorpool");
    assert_eq!(builds[1]["kind"], "tower");
    assert_eq!(state.fort_steps.get(&10), Some(&1));
    let buys: f64 = commands
        .iter()
        .filter(|c| c["t"] == "buy")
        .map(|c| {
            economy::price_of(Mode::Conquest, c["unit"].as_str().unwrap())
                .unwrap()
                .mp
        })
        .sum();
    assert!(cost("motorpool") + cost("tower") + buys <= 400.0);
}

#[test]
fn intent_ai33_mounted_squads_cannot_spend_or_advance_construction() {
    let mut input = fixture(vec![
        unit(1, "rifle", 12.0),
        unit(2, "rifle", 30.0),
        unit(3, "hq", 18.0),
        unit(4, "barracks", 24.0),
    ]);
    input.units[0].riding = true;
    input.tick = 3000;
    let observation = Observation::new(input.clone()).unwrap();
    let context = observation.input().runtime.as_ref().unwrap();
    let ground = sight_game(&observation, context);
    let crew = observation.input().units[..2].iter().collect::<Vec<_>>();
    let base = crate::commander_construction::plan(
        &observation,
        &crew,
        1000.0,
        false,
        &ground,
        |_| true,
        &mut MatchRandom::new(77),
    )
    .unwrap();
    assert_eq!(base.builders, vec![2]);
    let fort = crate::commander_fortification::plan(
        &observation,
        &crew,
        1000.0,
        0.0,
        0.0,
        &ground,
        |_| true,
        &mut BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(fort.builders, vec![2]);
    input.units[1].riding = true;
    let observation = Observation::new(input).unwrap();
    let crew = observation.input().units[..2].iter().collect::<Vec<_>>();
    let base = crate::commander_construction::plan(
        &observation,
        &crew,
        1000.0,
        false,
        &ground,
        |_| true,
        &mut MatchRandom::new(77),
    )
    .unwrap();
    assert!(base.commands.is_empty());
    assert_eq!(base.remaining_mp, 1000.0);
    let mut steps = BTreeMap::new();
    let fort = crate::commander_fortification::plan(
        &observation,
        &crew,
        1000.0,
        0.0,
        0.0,
        &ground,
        |_| true,
        &mut steps,
    )
    .unwrap();
    assert!(fort.commands.is_empty());
    assert_eq!(fort.remaining_mp, 1000.0);
    assert!(steps.is_empty());
    // The earlier forward-HQ spender must apply the same canonical rider exclusion.
    let mut input = fixture(vec![
        unit(1, "rifle", 110.0),
        unit(2, "rifle", 125.0),
        unit(3, "rifle", 130.0),
        unit(4, "hq", 20.0),
    ]);
    input.units[0].riding = true;
    let observation = Observation::new(input.clone()).unwrap();
    let ground = sight_game(&observation, observation.input().runtime.as_ref().unwrap());
    let crew = observation.input().units[..3].iter().collect::<Vec<_>>();
    let forward = crate::commander_forward_hq::plan(&observation, &crew, 1000.0, &ground, |_| true)
        .unwrap()
        .unwrap();
    assert_eq!(forward.unit, 2);
    for u in &mut input.units[..3] {
        u.riding = true;
    }
    let observation = Observation::new(input).unwrap();
    let crew = observation.input().units[..3].iter().collect::<Vec<_>>();
    assert!(
        crate::commander_forward_hq::plan(&observation, &crew, 1000.0, &ground, |_| true)
            .unwrap()
            .is_none()
    );
}

#[test]
fn intent_ai33_assist_and_ordinary_dig_keep_other_builders_fortification_reserve() {
    let mut units: Vec<_> = (1..=4)
        .map(|id| unit(id, if id <= 2 { "rifle" } else { "mg" }, 30.0 + id as f64))
        .collect();
    units.extend([
        unit(10, "hq", 20.0),
        unit(11, "barracks", 24.0),
        unit(12, "motorpool", 26.0),
        unit(13, "airfield", 28.0),
    ]);
    let mut input = fixture(units);
    input.research = Some(ResearchObservation {
        faction: economy::Faction::Germany,
        tech: economy::TechState::new(1, 1, 1, 1).unwrap(),
        busy: vec![Technology::Tier],
        stock_base: false,
        armory_ready: false,
        standard_army: true,
    });
    input.tick = 3000;
    input.manpower = cost("tower") - 1.0;
    input.runtime.as_mut().unwrap().large_army = true;
    input.runtime.as_mut().unwrap().faction = economy::Faction::Germany;
    input.units.iter_mut().find(|u| u.id == 11).unwrap().hp = 1.0;
    let commands = look(input.clone());
    assert!(commands.iter().any(|c| c["t"] == "assist"));
    assert!(!commands.iter().any(|c| c["t"] == "buy"), "{commands:?}");
    // Ordinary Dig occupies its worker, but only a wall job blocks every fortifier.
    input.units.iter_mut().find(|u| u.id == 11).unwrap().hp = num(
        Catalog::compiled().unwrap().unit("barracks").unwrap(),
        "hpPer",
    );
    input.units[0].dig = true;
    assert!(!look(input.clone()).iter().any(|c| c["t"] == "buy"));
    input
        .runtime
        .as_mut()
        .unwrap()
        .units
        .get_mut(&1)
        .unwrap()
        .wall_work = true;
    assert!(look(input).iter().any(|c| c["t"] == "buy"));
}
