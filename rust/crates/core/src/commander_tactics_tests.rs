use super::*;
use crate::{
    commander_ai::{Input, PlayerObservation, PointObservation, ResearchObservation},
    economy::TechState,
};

pub(super) fn unit(id: u32, kind: &str, x: f64) -> UnitObservation {
    let catalog = Catalog::compiled().unwrap();
    let definition = catalog.unit(kind).unwrap();
    let health = definition.get("models").and_then(Rule::number).unwrap()
        * definition.get("hpPer").and_then(Rule::number).unwrap();
    UnitObservation {
        id,
        kind: kind.into(),
        owner: 0,
        at: Point { x, z: 20.0 },
        hp: health,
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

pub(super) fn fixture(units: Vec<UnitObservation>) -> Input {
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
        height: 40,
        tick: 200,
        slot: 0,
        players: [0, 1, 0]
            .map(|team| PlayerObservation {
                team,
                out: false,
                spawn: Point { x: 10.0, z: 20.0 },
            })
            .into(),
        units,
        points: vec![PointObservation {
            at: Point { x: 120.0, z: 20.0 },
            vp: 1.0,
            mp: 1.0,
            owner: None,
            cut: false,
            locked: false,
        }],
        flags: vec![0; 3200],
        chars: vec![b'.'; 3200],
        manpower: 0.0,
        research: None,
        runtime: Some(Context {
            units: facts,
            ..Context::default()
        }),
    }
}

pub(super) fn run(input: Input, state: &mut State, level: Level) -> Vec<Value> {
    let observation = Observation::new(input).unwrap();
    plan(
        &observation,
        &[],
        state,
        level,
        &mut MatchRandom::new(0x51_36_28),
    )
    .unwrap()
}

pub(super) fn has(commands: &[Value], kind: &str) -> bool {
    commands.iter().any(|c| c["t"] == kind)
}

pub(super) fn acts(commands: &[Value], kind: &str, id: u32) -> bool {
    commands.iter().any(|c| {
        c["t"] == kind
            && (c["ids"]
                .as_array()
                .is_some_and(|ids| ids.contains(&json!(id)))
                || c["orders"]
                    .as_array()
                    .is_some_and(|orders| orders.iter().any(|row| row[0] == id)))
    })
}

fn stock_input(ammo: Option<f64>, provisions: f64, fuel: Option<f64>) -> Input {
    let mut input = fixture(vec![unit(1, "rifle", 40.0)]);
    input
        .runtime
        .as_mut()
        .unwrap()
        .units
        .get_mut(&1)
        .unwrap()
        .stock = Some(Stock {
        ammo,
        provisions,
        fuel,
        forced: false,
    });
    input
}

#[test]
fn intent_ai28_one_supply_retreat_until_every_recovery_threshold_is_met() {
    let mut state = State::default();
    for (index, (ammo, provisions, fuel)) in [
        (0.0, 0.0, 36.0),
        (0.0, 0.0, 36.0),
        (0.499, 60.0, 90.0),
        (0.5, 59.6, 90.0),
        (0.5, 60.0, 89.999),
        (0.5, 60.0, 90.0),
    ]
    .into_iter()
    .enumerate()
    {
        let mut input = stock_input(Some(ammo), provisions, Some(fuel));
        input.tick += index as i64 * 40;
        let commands = run(input, &mut state, Level::Normal);
        assert_eq!(acts(&commands, "retreat", 1), index == 0, "look {index}");
        assert_eq!(acts(&commands, "move", 1), index == 5, "look {index}");
        assert_eq!(state.supply_waiting.contains(&1), index < 5, "look {index}");
    }
    let commands = run(
        stock_input(Some(0.0), 60.0, Some(90.0)),
        &mut state,
        Level::Normal,
    );
    assert!(acts(&commands, "retreat", 1));
}

#[test]
fn intent_ai28_fractional_stock_is_preserved_and_each_critical_field_can_retreat() {
    for (ammo, provisions, fuel, critical) in [
        (Some(1.0), 59.6, Some(36.4), false),
        (Some(1.0), 60.0, Some(36.0), true),
        (Some(0.0), 60.0, Some(90.0), true),
        (Some(1.0), 0.0, Some(90.0), true),
        (None, 60.0, None, false),
    ] {
        let mut state = State::default();
        let commands = run(
            stock_input(ammo, provisions, fuel),
            &mut state,
            Level::Normal,
        );
        assert_eq!(acts(&commands, "retreat", 1), critical);
        assert_eq!(acts(&commands, "move", 1), !critical);
        assert_eq!(state.supply_waiting.contains(&1), critical);
    }
}

#[test]
fn intent_ai28_forced_units_supply_trucks_and_convoys_receive_no_combat_orders() {
    let mut input = fixture(vec![
        unit(1, "rifle", 40.0),
        unit(2, "truck", 42.0),
        unit(3, "rifle", 44.0),
    ]);
    let context = input.runtime.as_mut().unwrap();
    for id in 1..=3 {
        context.units.get_mut(&id).unwrap().stock = Some(Stock {
            ammo: Some(0.0),
            provisions: 0.0,
            fuel: Some(0.0),
            forced: id == 1,
        });
    }
    context.units.get_mut(&3).unwrap().convoy = true;
    let mut state = State::default();
    state.supply_waiting.insert(2);
    let commands = run(input, &mut state, Level::Hard);
    for id in 1..=3 {
        for kind in ["retreat", "move", "amove", "stance"] {
            assert!(!acts(&commands, kind, id), "{kind} for excluded {id}");
        }
    }
    assert!(state.supply_waiting.is_empty());
}

#[test]
fn intent_ai36_hurt_conditions_and_skill_thresholds_control_retreat() {
    for (kind, fraction, supp, level, expected) in [
        ("tank", 0.349, 0.0, Level::Normal, true),
        ("tank", 0.35, 0.0, Level::Normal, false),
        ("medic", 0.5, 0.0, Level::Normal, true),
        ("medic", 0.501, 0.0, Level::Normal, false),
        ("tank", 0.599, 90.0, Level::Normal, true),
        ("tank", 0.6, 90.0, Level::Normal, false),
        ("tank", 0.599, 89.999, Level::Normal, false),
        ("tank", 0.49, 0.0, Level::Easy, false),
        ("tank", 0.49, 0.0, Level::Hard, true),
        ("tank", 0.5, 0.0, Level::Hard, false),
    ] {
        let mut u = unit(1, kind, 50.0);
        u.hp *= fraction;
        u.supp = supp;
        let commands = run(fixture(vec![u]), &mut State::default(), level);
        assert_eq!(
            acts(&commands, "retreat", 1),
            expected,
            "{kind} {fraction} {supp} {level:?}"
        );
    }
    let mut u = unit(1, "rifle", 50.0);
    u.hp *= 0.1;
    u.retreating = true;
    let commands = run(fixture(vec![u]), &mut State::default(), Level::Hard);
    for kind in ["retreat", "move", "amove", "stance"] {
        assert!(!acts(&commands, kind, 1));
    }
}

#[test]
fn intent_ai36_reinforcement_wait_uses_finished_hq_radius_and_available_budget() {
    for (distance, fraction, manpower, expected_retreat, expected_move) in [
        (15.0, 0.2, 0.0, false, false),
        (16.0, 0.2, 0.0, true, false),
        (15.0, 0.8, 20.0, false, false),
        (15.0, 0.8, 19.0, false, true),
        (15.0, 1.0, 20.0, false, true),
    ] {
        let mut u = unit(1, "rifle", 20.0 + distance);
        u.hp *= fraction;
        let mut input = fixture(vec![u, unit(2, "hq", 20.0)]);
        input.manpower = manpower;
        let commands = run(input, &mut State::default(), Level::Normal);
        assert_eq!(
            acts(&commands, "retreat", 1),
            expected_retreat,
            "distance {distance} hp {fraction}"
        );
        assert_eq!(
            acts(&commands, "move", 1),
            expected_move,
            "distance {distance} hp {fraction}"
        );
    }
    let mut u = unit(1, "rifle", 35.0);
    u.hp *= 0.2;
    let mut input = fixture(vec![u, unit(2, "hq", 20.0)]);
    input
        .runtime
        .as_mut()
        .unwrap()
        .units
        .get_mut(&2)
        .unwrap()
        .built = 0.999;
    let commands = run(input, &mut State::default(), Level::Normal);
    assert!(acts(&commands, "assist", 1));
    assert!(!acts(&commands, "retreat", 1));
}

#[test]
fn intent_ai36_classic_and_world_reinforce_at_finished_allied_production() {
    for mode in [Mode::Classic, Mode::World] {
        for (kind, owner, built, alive, home) in [
            ("barracks", 2, 1.0, true, true),
            ("barracks", 2, 0.999, true, false),
            ("barracks", 1, 1.0, true, false),
            ("armory", 2, 1.0, true, false),
            ("barracks", 2, 1.0, false, false),
        ] {
            let mut u = unit(1, "rifle", 75.0);
            u.hp *= 0.2;
            let mut building = unit(2, kind, 60.0);
            building.owner = owner;
            if !alive {
                building.hp = 0.0;
            }
            let mut input = fixture(vec![u, building]);
            let context = input.runtime.as_mut().unwrap();
            context.mode = mode;
            context.units.get_mut(&2).unwrap().built = built;
            let commands = run(input, &mut State::default(), Level::Normal);
            assert_eq!(
                acts(&commands, "retreat", 1),
                !home,
                "{mode:?} {kind} owner {owner} built {built} alive {alive}"
            );
            assert!(!acts(&commands, "move", 1));
        }
    }
}

fn research_state(
    tier: u8,
    infantry: u8,
    guns: u8,
    armor: u8,
    busy: Vec<Technology>,
) -> ResearchObservation {
    ResearchObservation {
        faction: Faction::Usa,
        tech: TechState::new(tier, infantry, guns, armor).unwrap(),
        busy,
        stock_base: true,
        armory_ready: true,
        standard_army: true,
    }
}

fn researched(commands: &[Value]) -> Vec<&str> {
    commands
        .iter()
        .filter(|c| c["t"] == "tech")
        .map(|c| c["kind"].as_str().unwrap())
        .collect()
}

#[test]
fn intent_ai51_tier_research_starts_at_120_and_390_seconds() {
    for (tier, boundary) in [(1, 2400), (2, 7800)] {
        for (tick, due) in [(boundary - 1, false), (boundary, true)] {
            let mut input = fixture(vec![]);
            input.tick = tick;
            input.manpower = 1000.0;
            input.research = Some(research_state(tier, tier, tier, tier, vec![]));
            let mut state = State::default();
            let commands = run(input, &mut state, Level::Normal);
            assert_eq!(
                researched(&commands),
                if due { vec!["tier"] } else { vec![] }
            );
            assert!(state.decision.is_some());
        }
    }
}

#[test]
fn intent_ai51_armory_selects_lowest_free_line_with_stable_ties() {
    for (infantry, guns, armor_level, busy, expected) in [
        (0, 0, 0, vec![], Some("inf")),
        (1, 0, 0, vec![], Some("guns")),
        (1, 1, 0, vec![], Some("armor")),
        (0, 0, 0, vec![Technology::Infantry], Some("guns")),
        (
            0,
            0,
            0,
            vec![Technology::Infantry, Technology::Guns],
            Some("armor"),
        ),
        (
            0,
            0,
            0,
            vec![Technology::Infantry, Technology::Guns, Technology::Armor],
            None,
        ),
        (2, 2, 2, vec![], None),
    ] {
        let mut input = fixture(vec![]);
        input.manpower = 1000.0;
        input.research = Some(research_state(2, infantry, guns, armor_level, busy));
        let commands = run(input, &mut State::default(), Level::Normal);
        assert_eq!(
            researched(&commands),
            expected.into_iter().collect::<Vec<_>>()
        );
    }
    let mut input = fixture(vec![]);
    input.tick = 2400;
    input.manpower = 1000.0;
    input.research = Some(research_state(1, 0, 0, 0, vec![Technology::Tier]));
    assert_eq!(
        researched(&run(input, &mut State::default(), Level::Normal)),
        vec!["inf"]
    );
}

#[test]
fn intent_ai51_classic_munitions_gate_research_and_horde_skips_it() {
    for kind in [Technology::Tier, Technology::Infantry] {
        let mut input = fixture(vec![]);
        input.tick = if kind == Technology::Tier { 2400 } else { 200 };
        let r = research_state(1, 0, 0, 0, vec![]);
        let cost = economy::tech_cost(r.tech, kind, Mode::Classic)
            .unwrap()
            .unwrap();
        input.manpower = cost.mp;
        input.research = Some(r);
        input.runtime.as_mut().unwrap().mode = Mode::Classic;
        input.runtime.as_mut().unwrap().munitions = cost.mun - 0.001;
        assert!(!has(
            &run(input.clone(), &mut State::default(), Level::Normal),
            "tech"
        ));
        input.runtime.as_mut().unwrap().munitions = cost.mun;
        assert!(has(
            &run(input, &mut State::default(), Level::Normal),
            "tech"
        ));
    }
    let mut input = fixture(vec![]);
    input.tick = 7800;
    input.manpower = 1000.0;
    input.research = Some(research_state(2, 0, 0, 0, vec![]));
    let context = input.runtime.as_mut().unwrap();
    context.mode = Mode::Horde;
    context.horde = true;
    let commands = run(input, &mut State::default(), Level::Hard);
    assert!(!has(&commands, "tech"));
    assert!(!has(&commands, "buy"));
}

#[test]
fn intent_ai51_tier_savings_protect_shopping_and_spare_budget_can_buy() {
    let mut input = fixture(vec![unit(1, "hq", 20.0)]);
    input.tick = 2400;
    let r = research_state(1, 1, 1, 1, vec![]);
    let cost = economy::tech_cost(r.tech, Technology::Tier, Mode::Conquest)
        .unwrap()
        .unwrap();
    input.manpower = cost.mp - 1.0;
    assert!(has(
        &run(input.clone(), &mut State::default(), Level::Normal),
        "buy"
    ));
    input.research = Some(r);
    let commands = run(input.clone(), &mut State::default(), Level::Normal);
    assert!(!has(&commands, "tech"));
    assert!(!has(&commands, "buy"));
    let price = economy::price_of(Mode::Conquest, "rifle").unwrap();
    input.manpower = cost.mp + price.mp;
    let commands = run(input, &mut State::default(), Level::Normal);
    assert_eq!(researched(&commands), vec!["tier"]);
    assert!(
        commands
            .iter()
            .any(|c| c["t"] == "buy" && c["unit"] == "rifle")
    );
}
