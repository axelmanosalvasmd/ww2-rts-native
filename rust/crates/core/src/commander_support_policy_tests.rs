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
        hp: def.get("models").and_then(Value::number).unwrap()
            * def.get("hpPer").and_then(Value::number).unwrap(),
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
        tick: 2000,
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
        points: vec![crate::commander_ai::PointObservation {
            at: Point { x: 130.0, z: 100.0 },
            vp: 1.0,
            mp: 0.0,
            owner: Some(1),
            cut: false,
            locked: false,
        }],
        flags: vec![0; 16384],
        chars: vec![b'.'; 16384],
        manpower: 1000.0,
        research: None,
        runtime: None,
    })
    .unwrap()
}
fn facts(obs: &Observation, enabled: &[&str]) -> SupportFacts {
    let catalog = Catalog::compiled().unwrap();
    SupportFacts {
        mode: Mode::Conquest,
        munitions: 1000.0,
        cooldowns: catalog
            .table("SUPPORT")
            .unwrap()
            .object()
            .unwrap()
            .keys()
            .map(|key| {
                (
                    key.clone(),
                    if enabled.contains(&key.as_str()) {
                        0.0
                    } else {
                        100.0
                    },
                )
            })
            .collect(),
        watched: obs
            .input()
            .units
            .iter()
            .filter(|u| u.owner == 1)
            .map(|u| u.id)
            .collect(),
        stationary_since: BTreeMap::new(),
        visible_points: BTreeSet::from([0]),
        ready_points: BTreeSet::from([0]),
        strikes: Vec::new(),
        situation: None,
        horde_seat: false,
        unavailable: BTreeSet::new(),
    }
}
fn chosen(obs: &Observation, facts: &SupportFacts, level: Level, called: f64) -> Option<String> {
    plan(obs, facts, level, called).unwrap().map(|c| match c {
        Command::Support { kind, .. } => kind,
        _ => panic!("support only"),
    })
}

#[test]
fn cover_ignores_the_situation_leash_while_dive_requires_a_watched_fight() {
    let obs = observation(vec![
        unit(1, "engineer", 0, 100.0),
        unit(2, "tank", 1, 120.0),
    ]);
    let mut f = facts(&obs, &["cover", "dive", "bombing"]);
    f.situation = Some(Point { x: 230.0, z: 230.0 });
    f.strikes.push(AnnouncedStrike {
        kind: "bombing".into(),
        owner: 1,
        at: Point { x: 100.0, z: 100.0 },
        delay: 3.0,
    });
    assert_eq!(
        chosen(&obs, &f, Level::Normal, 0.0).as_deref(),
        Some("cover")
    );
    f.strikes[0].owner = 0;
    assert_eq!(chosen(&obs, &f, Level::Normal, 0.0), None);
    f.situation = None;
    assert_eq!(
        chosen(&obs, &f, Level::Normal, 0.0).as_deref(),
        Some("dive")
    );
    f.watched.clear();
    assert_eq!(chosen(&obs, &f, Level::Normal, 0.0), None);
    f.strikes[0].owner = 1;
    f.strikes[0].delay = 0.0;
    assert_eq!(chosen(&obs, &f, Level::Normal, 0.0), None);
}

#[test]
fn admitted_air_support_requires_its_target_readiness_and_cooldown() {
    let mut units: Vec<_> = (1..=6).map(|id| unit(id, "rifle", 0, 100.0)).collect();
    units.extend([
        unit(7, "tank", 1, 130.0),
        unit(8, "rifle", 1, 132.0),
        unit(9, "rifle", 1, 134.0),
    ]);
    let targets = observation(units);
    let no_enemies = observation(vec![unit(1, "rifle", 0, 100.0)]);
    for kind in ["dive", "para", "bombing", "strafe", "recon"] {
        let obs = if kind == "recon" { &no_enemies } else { &targets };
        let mut f = facts(obs, &[kind]);
        assert_eq!(
            chosen(obs, &f, Level::Normal, 0.0).as_deref(),
            Some(kind),
            "ready {kind} must be selected for its qualifying target"
        );
        if kind != "recon" {
            assert_eq!(
                chosen(&no_enemies, &facts(&no_enemies, &[kind]), Level::Normal, 0.0),
                None
            );
        }
        f.cooldowns.insert(kind.into(), 0.01);
        assert_eq!(chosen(obs, &f, Level::Normal, 0.0), None);
        f.cooldowns.insert(kind.into(), 0.0);
        match kind {
            "para" => f.ready_points.clear(),
            "recon" => {
                assert_eq!(
                    chosen(&targets, &facts(&targets, &[kind]), Level::Normal, 0.0),
                    None
                );
                continue;
            }
            _ => f.watched.clear(),
        }
        assert_eq!(chosen(obs, &f, Level::Normal, 0.0), None);
    }
}

#[test]
fn air_support_priority_falls_back_through_bombing_artillery_and_strafe() {
    let obs = observation(vec![
        unit(1, "rifle", 0, 100.0),
        unit(2, "tank", 1, 130.0),
        unit(3, "rifle", 1, 132.0),
        unit(4, "rifle", 1, 134.0),
    ]);
    let mut f = facts(&obs, &["dive", "bombing", "artillery", "strafe"]);
    for kind in ["dive", "bombing", "artillery", "strafe"] {
        assert_eq!(
            chosen(&obs, &f, Level::Normal, 0.0).as_deref(),
            Some(kind)
        );
        f.cooldowns.insert(kind.into(), 1.0);
    }
    assert_eq!(chosen(&obs, &f, Level::Normal, 0.0), None);
}

#[test]
fn observed_stationary_timer_and_hard_friend_safety_gate_artillery() {
    let obs = observation(vec![unit(1, "rifle", 0, 100.0), unit(2, "mg", 1, 130.0)]);
    let mut f = facts(&obs, &["artillery"]);
    assert_eq!(chosen(&obs, &f, Level::Normal, 0.0), None);
    f.stationary_since.insert(2, 97.0);
    assert_eq!(chosen(&obs, &f, Level::Normal, 0.0), None);
    f.stationary_since.insert(2, 96.99);
    assert_eq!(
        chosen(&obs, &f, Level::Normal, 0.0).as_deref(),
        Some("artillery")
    );
    f.watched.clear();
    assert_eq!(chosen(&obs, &f, Level::Normal, 0.0), None);
    let close = observation(vec![unit(1, "rifle", 0, 132.0), unit(2, "mg", 1, 130.0)]);
    f.watched.insert(2);
    assert_eq!(chosen(&close, &f, Level::Hard, 0.0), None);
}

#[test]
fn clusters_and_situations_bound_strikes_with_hard_friendly_safety() {
    let obs = observation(vec![
        unit(1, "rifle", 0, 100.0),
        unit(2, "rifle", 1, 130.0),
        unit(3, "rifle", 1, 132.0),
        unit(4, "rifle", 1, 134.0),
    ]);
    let mut f = facts(&obs, &["artillery"]);
    assert_eq!(
        chosen(&obs, &f, Level::Hard, 0.0).as_deref(),
        Some("artillery")
    );
    let mut larger_units = obs.input().units.clone();
    larger_units.extend([
        unit(5, "rifle", 1, 145.0),
        unit(6, "rifle", 1, 147.0),
        unit(7, "rifle", 1, 149.0),
        unit(8, "rifle", 1, 150.0),
    ]);
    let larger = observation(larger_units);
    let larger_facts = facts(&larger, &["artillery"]);
    assert!(
        matches!(plan(&larger,&larger_facts,Level::Hard,0.0).unwrap(),Some(Command::Support {at,..}) if at.x ==147.75)
    );
    f.situation = Some(Point { x: 172.0, z: 100.0 });
    assert!(plan(&obs, &f, Level::Hard, 0.0).unwrap().is_some());
    f.situation = Some(Point {
        x: 172.01,
        z: 100.0,
    });
    assert_eq!(chosen(&obs, &f, Level::Hard, 0.0), None);
    f.mode = Mode::Horde;
    f.horde_seat = true;
    f.watched.clear();
    assert_eq!(
        chosen(&obs, &f, Level::Hard, 0.0).as_deref(),
        Some("artillery")
    );
    let close = observation(vec![
        unit(1, "rifle", 0, 132.0),
        unit(2, "rifle", 1, 130.0),
        unit(3, "rifle", 1, 132.0),
        unit(4, "rifle", 1, 134.0),
    ]);
    f.horde_seat = false;
    f.mode = Mode::Conquest;
    f.situation = None;
    f.watched = BTreeSet::from([2, 3, 4]);
    assert_eq!(chosen(&close, &f, Level::Hard, 0.0), None);
}

#[test]
fn reserves_cooldowns_easy_gap_and_classic_munitions_gate_calls() {
    let mut input = observation(vec![
        unit(1, "rifle", 0, 100.0),
        unit(2, "rifle", 1, 130.0),
        unit(3, "rifle", 1, 132.0),
    ])
    .input()
    .clone();
    input.manpower = 250.0;
    let obs = Observation::new(input.clone()).unwrap();
    let f = facts(&obs, &["artillery"]);
    assert_eq!(
        chosen(&obs, &f, Level::Normal, 100.0).as_deref(),
        Some("artillery")
    );
    input.manpower = 249.0;
    let low = Observation::new(input.clone()).unwrap();
    assert_eq!(chosen(&low, &f, Level::Normal, 0.0), None);
    input.manpower = 400.0;
    let obs = Observation::new(input).unwrap();
    assert_eq!(
        chosen(&obs, &f, Level::Easy, 55.0).as_deref(),
        Some("artillery")
    );
    assert_eq!(chosen(&obs, &f, Level::Easy, 55.01), None);
    let mut classic = f.clone();
    classic.mode = Mode::Classic;
    classic.munitions = 60.0;
    assert_eq!(
        chosen(&low, &classic, Level::Normal, 0.0).as_deref(),
        Some("artillery")
    );
    classic.munitions = 59.99;
    assert_eq!(chosen(&low, &classic, Level::Normal, 0.0), None);
    classic.munitions = 99.99;
    assert_eq!(chosen(&low, &classic, Level::Easy, 0.0), None);
    classic.munitions = 100.0;
    assert_eq!(
        chosen(&low, &classic, Level::Easy, 0.0).as_deref(),
        Some("artillery")
    );
    classic.cooldowns.insert("artillery".into(), 0.01);
    assert_eq!(chosen(&low, &classic, Level::Easy, 0.0), None);
}

#[test]
fn malformed_detached_facts_fail_before_emitting() {
    let obs = observation(vec![unit(1, "rifle", 0, 100.0)]);
    let base = facts(&obs, &["recon"]);
    for invalid in 0..6 {
        let mut f = base.clone();
        match invalid {
            0 => {
                f.cooldowns.remove("smoke");
            }
            1 => {
                f.watched.insert(999);
            }
            2 => {
                f.stationary_since.insert(1, 0.0);
            }
            3 => {
                f.ready_points.insert(9);
            }
            4 => {
                f.munitions = f64::NAN;
            }
            _ => {
                f.cooldowns.insert("dive".into(), f64::INFINITY);
            }
        }
        assert!(plan(&obs, &f, Level::Normal, 0.0).is_err());
    }
}
