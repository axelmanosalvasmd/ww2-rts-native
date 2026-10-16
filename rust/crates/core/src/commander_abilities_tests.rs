use super::*;
use crate::commander_ai::{Input, PlayerObservation, UnitObservation};

fn unit(id: u32, kind: &str, owner: usize, x: f64) -> UnitObservation {
    let catalog = Catalog::compiled().unwrap();
    let def = catalog.unit(kind).unwrap();
    UnitObservation {
        id,
        kind: kind.into(),
        owner,
        at: Point { x, z: 100.0 },
        hp: number(def, "models").unwrap() * number(def, "hpPer").unwrap(),
        supp: 0.0,
        target_id: 0,
        cover: 0,
        path: Vec::new(),
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
fn observation(mut units: Vec<UnitObservation>) -> Observation {
    units.sort_by_key(|u| u.id);
    Observation::new(Input {
        width: 128,
        height: 128,
        tick: 100,
        slot: 0,
        players: vec![
            PlayerObservation {
                team: 0,
                out: false,
                spawn: Point { x: 4.0, z: 4.0 },
            },
            PlayerObservation {
                team: 1,
                out: false,
                spawn: Point { x: 240.0, z: 240.0 },
            },
        ],
        units,
        points: Vec::new(),
        flags: vec![0; 16384],
        chars: vec![b'.'; 16384],
        manpower: 1000.0,
        research: None,
    })
    .unwrap()
}
fn check(
    obs: &Observation,
    cooldowns: &BTreeMap<u32, f64>,
    watched: &BTreeSet<u32>,
    moving: &BTreeSet<u32>,
    level: Level,
) -> Result<Vec<Command>, String> {
    plan(
        obs,
        &AbilityFacts {
            cooldowns: cooldowns.clone(),
            watched: watched.clone(),
            attack_moving: moving.clone(),
            ..AbilityFacts::default()
        },
        level,
    )
}
fn run(units: Vec<UnitObservation>, watched: &[u32], moving: &[u32], level: Level) -> Vec<Command> {
    let obs = observation(units);
    let cooldowns = obs
        .input()
        .units
        .iter()
        .filter(|u| u.owner == 0 && u.kind != "hq")
        .map(|u| (u.id, 0.0))
        .collect();
    let at_base = obs
        .input()
        .units
        .iter()
        .filter(|u| {
            u.owner == 0
                && u.kind != "hq"
                && (distance(u.at, obs.input().players[0].spawn) <= 30.0
                    || obs.input().units.iter().any(|base| {
                        base.kind == "hq" && base.owner == 0 && distance(u.at, base.at) <= 30.0
                    }))
        })
        .map(|u| u.id)
        .collect();
    plan(
        &obs,
        &AbilityFacts {
            cooldowns,
            watched: watched.iter().copied().collect(),
            attack_moving: moving.iter().copied().collect(),
            at_base,
            unavailable: BTreeSet::new(),
        },
        level,
    )
    .unwrap()
}

#[test]
fn each_ability_uses_its_combat_trigger() {
    for (kind, enemy_kind, aim) in [
        ("rifle", "mg", true),
        ("mg", "rifle", false),
        ("at", "tank", false),
        ("tank", "rifle", false),
        ("commando", "tank", true),
        ("conscript", "rifle", false),
        ("mortar", "rifle", true),
    ] {
        let mut own = unit(1, kind, 0, 100.0);
        own.target_id = 2;
        if kind == "tank" {
            own.hp *= 0.4;
        }
        if kind == "conscript" {
            own.supp = 50.0;
        }
        let mut enemy = unit(2, enemy_kind, 1, 110.0);
        enemy.garrison = kind == "mortar";
        let commands = run(vec![own, enemy], &[2], &[], Level::Normal);
        assert_eq!(commands.len(), 1, "{kind}");
        assert!(
            matches!(&commands[0], Command::Ability { ids, at, queue: false, automatic: false }
            if ids == &[1] && at.is_some() == aim),
            "{kind}"
        );
    }
}

#[test]
fn watched_current_targets_and_exact_cooldown_facts_are_required() {
    let mut own = unit(1, "mg", 0, 100.0);
    own.target_id = 2;
    let obs = observation(vec![own.clone(), unit(2, "rifle", 1, 110.0)]);
    assert!(
        check(
            &obs,
            &BTreeMap::from([(1, 0.0)]),
            &BTreeSet::new(),
            &BTreeSet::new(),
            Level::Normal
        )
        .unwrap()
        .is_empty()
    );
    assert!(
        check(
            &obs,
            &BTreeMap::from([(1, 0.1)]),
            &BTreeSet::from([2]),
            &BTreeSet::new(),
            Level::Normal
        )
        .unwrap()
        .is_empty()
    );
    for cooldowns in [
        BTreeMap::new(),
        BTreeMap::from([(1, f64::NAN)]),
        BTreeMap::from([(1, 0.0), (2, 0.0)]),
    ] {
        assert!(
            check(
                &obs,
                &cooldowns,
                &BTreeSet::from([2]),
                &BTreeSet::new(),
                Level::Normal
            )
            .is_err()
        );
    }
    assert!(
        check(
            &obs,
            &BTreeMap::from([(1, 0.0)]),
            &BTreeSet::from([3]),
            &BTreeSet::new(),
            Level::Normal
        )
        .is_err()
    );
    assert!(
        check(
            &obs,
            &BTreeMap::from([(1, 0.0)]),
            &BTreeSet::new(),
            &BTreeSet::from([2]),
            Level::Normal
        )
        .is_err()
    );
    // A remembered target ID supplies neither a delivered unit nor an aim point.
    assert!(run(vec![own], &[], &[], Level::Normal).is_empty());
}

#[test]
fn level_cast_budgets_follow_unit_id_order() {
    let mut units = (1..=4)
        .map(|id| {
            let mut u = unit(id, "mg", 0, 100.0);
            u.target_id = 9;
            u
        })
        .collect::<Vec<_>>();
    units.push(unit(9, "rifle", 1, 110.0));
    for (level, count) in [(Level::Easy, 1), (Level::Normal, 2), (Level::Hard, 3)] {
        let commands = run(units.clone(), &[9], &[], level);
        assert_eq!(commands.len(), count);
        for (index, command) in commands.iter().enumerate() {
            assert!(matches!(command, Command::Ability {ids,..} if ids == &[index as i64 + 1]));
        }
    }
}

#[test]
fn grenade_cover_and_range_satchel_range_and_ura_motion_are_distinct() {
    let own = unit(1, "rifle", 0, 100.0);
    let mut enemy = unit(2, "rifle", 1, 122.0);
    assert!(run(vec![own.clone(), enemy.clone()], &[2], &[], Level::Normal).is_empty());
    enemy.cover = 1;
    assert_eq!(
        run(vec![own.clone(), enemy.clone()], &[2], &[], Level::Normal).len(),
        1
    );
    enemy.at.x += 0.01;
    assert!(run(vec![own, enemy], &[2], &[], Level::Normal).is_empty());
    let own = unit(1, "commando", 0, 100.0);
    assert_eq!(
        run(
            vec![own.clone(), unit(2, "tank", 1, 120.0)],
            &[2],
            &[],
            Level::Normal
        )
        .len(),
        1
    );
    assert!(
        run(
            vec![own, unit(2, "tank", 1, 120.01)],
            &[2],
            &[],
            Level::Normal
        )
        .is_empty()
    );
    let own = unit(1, "conscript", 0, 100.0);
    let enemy = unit(2, "rifle", 1, 129.0);
    assert!(run(vec![own.clone(), enemy.clone()], &[2], &[], Level::Normal).is_empty());
    assert_eq!(
        run(vec![own.clone(), enemy], &[2], &[1], Level::Normal).len(),
        1
    );
    assert!(
        run(
            vec![own, unit(2, "rifle", 1, 130.0)],
            &[2],
            &[1],
            Level::Normal
        )
        .is_empty()
    );
}

#[test]
fn hard_barrage_chooses_largest_safe_cluster_and_safe_garrisons() {
    let own = unit(1, "mortar", 0, 100.0);
    let enemies = vec![
        unit(3, "rifle", 1, 130.0),
        unit(4, "rifle", 1, 132.0),
        unit(5, "rifle", 1, 134.0),
    ];
    let mut units = vec![own.clone()];
    units.extend(enemies.clone());
    let commands = run(units.clone(), &[3, 4, 5], &[], Level::Hard);
    assert!(matches!(&commands[0], Command::Ability {at:Some(at),..} if at.x == 132.0));
    let mut larger = units.clone();
    larger.extend([
        unit(6, "rifle", 1, 145.0),
        unit(7, "rifle", 1, 147.0),
        unit(8, "rifle", 1, 149.0),
        unit(9, "rifle", 1, 150.0),
    ]);
    let commands = run(larger, &[3, 4, 5, 6, 7, 8, 9], &[], Level::Hard);
    assert!(matches!(&commands[0], Command::Ability {at:Some(at),..} if at.x == 147.75));
    assert!(run(units.clone(), &[3, 4], &[], Level::Hard).is_empty());
    assert!(run(units.clone(), &[3, 4, 5], &[], Level::Normal).is_empty());
    units.push(unit(2, "rifle", 0, 132.0));
    assert!(run(units, &[3, 4, 5], &[], Level::Hard).is_empty());
    let mut enemy = unit(3, "rifle", 1, 130.0);
    enemy.garrison = true;
    assert!(
        run(
            vec![own.clone(), unit(2, "rifle", 0, 132.0), enemy.clone()],
            &[3],
            &[],
            Level::Hard
        )
        .is_empty()
    );
    enemy.at.x = 150.01;
    assert!(run(vec![own, enemy], &[3], &[], Level::Hard).is_empty());
}

#[test]
fn smoke_stays_home_and_retreating_riding_building_or_dead_units_do_not_cast() {
    let mut own = unit(1, "tank", 0, 100.0);
    own.hp *= 0.4;
    assert!(
        run(
            vec![own.clone(), unit(2, "hq", 0, 100.0)],
            &[],
            &[],
            Level::Normal
        )
        .is_empty()
    );
    own.at = Point { x: 4.0, z: 4.0 };
    assert!(run(vec![own], &[], &[], Level::Normal).is_empty());
    for excluded in 0..4 {
        let mut own = unit(1, "mg", 0, 100.0);
        own.target_id = 2;
        match excluded {
            0 => own.retreating = true,
            1 => own.riding = true,
            2 => own.build = true,
            _ => own.hp = 0.0,
        }
        assert!(
            run(
                vec![own, unit(2, "rifle", 1, 110.0)],
                &[2],
                &[],
                Level::Normal
            )
            .is_empty()
        );
    }
}

#[test]
fn strategic_unavailability_is_explicit_and_working_units_can_cast() {
    let mut own = unit(1, "mg", 0, 100.0);
    own.target_id = 2;
    own.path.push(Point { x: 120.0, z: 100.0 });
    own.dig = true;
    own.entrench = true;
    own.enter = true;
    let obs = observation(vec![own, unit(2, "rifle", 1, 110.0)]);
    let mut facts = AbilityFacts {
        cooldowns: BTreeMap::from([(1, 0.0)]),
        watched: BTreeSet::from([2]),
        ..AbilityFacts::default()
    };
    assert_eq!(plan(&obs, &facts, Level::Normal).unwrap().len(), 1);
    facts.unavailable.insert(1);
    assert!(plan(&obs, &facts, Level::Normal).unwrap().is_empty());
    facts.unavailable.insert(2);
    assert!(plan(&obs, &facts, Level::Normal).is_err());
    facts.unavailable.clear();
    facts.at_base.insert(2);
    assert!(plan(&obs, &facts, Level::Normal).is_err());
}

#[test]
fn ap_rejects_infantry_and_structures_and_smoke_requires_less_than_half_health() {
    for enemy_kind in ["rifle", "hq"] {
        let mut own = unit(1, "at", 0, 100.0);
        own.target_id = 2;
        assert!(
            run(
                vec![own, unit(2, enemy_kind, 1, 110.0)],
                &[2],
                &[],
                Level::Normal
            )
            .is_empty()
        );
    }
    let mut own = unit(1, "tank", 0, 100.0);
    own.hp *= 0.5;
    assert!(run(vec![own], &[], &[], Level::Normal).is_empty());
}
