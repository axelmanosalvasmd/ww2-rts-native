use super::*;
use crate::horde_commander::{OwnPlan, OwnUnit, Wave};
use std::collections::VecDeque;

fn unit(id: u32, owner: usize, kind: &str, x: f64, z: f64) -> Unit {
    let rule = Catalog::compiled().unwrap().unit(kind).unwrap();
    Unit {
        id,
        owner,
        kind: kind.into(),
        at: Point { x, z },
        hp: number(rule, "models") * number(rule, "hpPer"),
        suppression: 0.0,
        plane: flag(rule, "air").then_some(PlaneState::Base),
        cooldown: 0.0,
        auto_retreat: false,
        garrisoned: false,
        in_cover: false,
        riding: false,
    }
}
fn fixture(rows: Vec<Unit>) -> (View, Facts) {
    let own = rows
        .iter()
        .filter(|u| u.owner == 0)
        .map(|u| {
            (
                u.id,
                OwnUnit {
                    id: u.id,
                    owner: 0,
                    hp: u.hp,
                    air: u.plane.is_some(),
                    structure: flag(
                        Catalog::compiled().unwrap().unit(&u.kind).unwrap(),
                        "structure",
                    ),
                    plan: OwnPlan {
                        delivered: None,
                        path_generation: 0,
                        goal: None,
                        route_end: None,
                        outcome: None,
                        outcome_tick: None,
                        path: vec![],
                        amove: None,
                        attack: 0,
                        target: 0,
                        retreating: false,
                        orders: VecDeque::new(),
                        dig: None,
                        entrench: false,
                        build: None,
                        enter: None,
                        fire_at: None,
                        nade: None,
                        hold: false,
                    },
                },
            )
        })
        .collect();
    let view = View {
        tick: 0,
        wave: Wave {
            slot: 0,
            number: 1,
            active: true,
        },
        units: own,
        bunker: None,
    };
    let facts = Facts {
        tick: 0,
        slot: 0,
        spawn: Point { x: 0.0, z: 0.0 },
        teams: vec![0, 1, 0],
        manpower: 1000.0,
        support: [
            "cover",
            "dive",
            "bombing",
            "artillery",
            "strafe",
            "recon",
            "para",
        ]
        .into_iter()
        .map(|s| (s.into(), 0.0))
        .collect(),
        units: rows.into_iter().map(|u| (u.id, u)).collect(),
        points: vec![],
        strikes: vec![],
    };
    (view, facts)
}
fn plan(view: &View, facts: &Facts) -> Vec<Command> {
    let mut policy = HordePolicy::default();
    policy.observe(view, facts).unwrap();
    policy.plan(view, facts).unwrap()
}
fn support(commands: &[Command]) -> Option<(&str, Point)> {
    commands.iter().find_map(|c| match c {
        Command::Support { kind, at, .. } => Some((kind.as_str(), *at)),
        _ => None,
    })
}
fn ability(commands: &[Command], id: u32) -> Option<Option<Point>> {
    commands.iter().find_map(|c| match c {
        Command::Ability { ids, at, .. } if ids == &[i64::from(id)] => Some(*at),
        _ => None,
    })
}

#[test]
fn intent_ai57_no_notice_or_human_policy_and_nearest_visible_bunker() {
    let mut rows: Vec<_> = (1..=8)
        .map(|id| unit(id, 0, "rifle", 100.0, 100.0))
        .collect();
    rows.extend([
        unit(90, 1, "tank", 105.0, 100.0),
        unit(91, 1, "bunker", 80.0, 0.0),
        unit(92, 1, "bunker", 0.0, 80.0),
        unit(93, 1, "bunker", 90.0, 90.0),
    ]);
    let (mut view, mut facts) = fixture(rows);
    // The commander queue's older coarse bunker hint cannot override full facts.
    view.bunker = Some(Point { x: 999.0, z: 999.0 });
    facts.units.get_mut(&1).unwrap().hp = 1.0;
    view.units.get_mut(&1).unwrap().hp = 1.0;
    view.units.get_mut(&2).unwrap().plan.retreating = true;
    facts.points.push(PointFact {
        at: Point { x: 10.0, z: 10.0 },
        owner: Some(1),
        visible: true,
    });
    let commands = plan(&view, &facts);
    // Fresh tank is eligible immediately; human Normal waits 1.25 seconds.
    assert_eq!(
        support(&commands),
        Some(("dive", Point { x: 105.0, z: 100.0 }))
    );
    let armed = commands
        .iter()
        .find_map(|c| match c {
            Command::Stance {
                ids,
                stance: Stance::AutoRetreat,
                on: true,
            } => Some(ids),
            _ => None,
        })
        .unwrap();
    assert_eq!(armed, &(1..=8).collect::<Vec<i64>>());
    assert!(commands.iter().all(|c| !matches!(
        c,
        Command::Buy { .. } | Command::Technology { .. } | Command::Retreat { .. }
    )));
    assert!(
        commands
            .iter()
            .all(|c| !matches!(c, Command::Support { kind, .. } if kind == "para"))
    );
    let orders = commands
        .iter()
        .find_map(|c| match c {
            Command::Move {
                orders,
                attack_move: true,
                ..
            } => Some(orders),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        orders.len(),
        7,
        "no human march limit and no health retreat"
    );
    assert!(
        orders
            .iter()
            .all(|o| o.unit != 2 && o.at == Point { x: 80.0, z: 0.0 })
    );
    facts.units.remove(&91);
    assert!(plan(&view, &facts).iter().any(|c| matches!(c,
        Command::Move { orders, attack_move: true, .. } if orders.iter().all(|o| o.at == Point { x:0.0,z:80.0 }))));
}

#[test]
fn intent_ai57_support_priority_reserve_cooldown_and_cluster_fallbacks() {
    let (view, mut facts) = fixture(vec![
        unit(1, 0, "rifle", 50.0, 50.0),
        unit(10, 1, "tank", 60.0, 50.0),
        unit(11, 1, "mg", 61.0, 50.0),
    ]);
    facts.strikes.push(Strike {
        at: Point { x: 50.0, z: 50.0 },
        owner: 1,
        kind: "bombing".into(),
        delay: 2.0,
    });
    assert_eq!(support(&plan(&view, &facts)).unwrap().0, "cover");
    facts.support.insert("cover".into(), 1.0);
    assert_eq!(support(&plan(&view, &facts)).unwrap().0, "dive");
    facts.support.insert("dive".into(), 1.0);
    assert_eq!(support(&plan(&view, &facts)).unwrap().0, "bombing");
    facts.support.insert("bombing".into(), 1.0);
    assert_eq!(
        support(&plan(&view, &facts)),
        Some(("artillery", Point { x: 60.5, z: 50.0 }))
    );
    facts.support.insert("artillery".into(), 1.0);
    facts.units.get_mut(&10).unwrap().kind = "rifle".into();
    assert_eq!(support(&plan(&view, &facts)).unwrap().0, "strafe");
    facts.manpower = economy::sup_cost(Mode::Horde, "strafe").unwrap().cost + 99.0;
    assert_eq!(support(&plan(&view, &facts)), None);
    facts.manpower += 1.0;
    assert_eq!(support(&plan(&view, &facts)).unwrap().0, "strafe");
    // Source's else-if does not try strafe when affordable artillery had no target.
    facts.support.insert("artillery".into(), 0.0);
    facts.manpower = 1000.0;
    facts.units.get_mut(&11).unwrap().at.x = 69.0;
    assert_eq!(support(&plan(&view, &facts)), None);
    facts.units.remove(&10);
    facts.units.remove(&11);
    facts.points.push(PointFact {
        at: Point { x: 150.0, z: 150.0 },
        owner: Some(1),
        visible: false,
    });
    assert_eq!(
        support(&plan(&view, &facts)),
        Some(("recon", Point { x: 150.0, z: 150.0 }))
    );
    facts.manpower = 250.0;
    assert_eq!(support(&plan(&view, &facts)), None);
}

#[test]
fn intent_ai57_stationary_artillery_uses_observations_and_forgets_hidden_units() {
    let (mut view, mut facts) = fixture(vec![
        unit(1, 0, "rifle", 0.0, 0.0),
        unit(10, 1, "mg", 50.0, 50.0),
    ]);
    let mut state = HordePolicy::default();
    state.observe(&view, &facts).unwrap();
    assert_eq!(support(&state.plan(&view, &facts).unwrap()), None);
    view.tick = 60;
    facts.tick = 60;
    state.observe(&view, &facts).unwrap();
    assert_eq!(support(&state.plan(&view, &facts).unwrap()), None);
    view.tick = 61;
    facts.tick = 61;
    state.observe(&view, &facts).unwrap();
    assert_eq!(
        support(&state.plan(&view, &facts).unwrap()).unwrap().0,
        "artillery"
    );
    facts.units.remove(&10);
    view.tick = 80;
    facts.tick = 80;
    state.observe(&view, &facts).unwrap();
    facts.units.insert(10, unit(10, 1, "mg", 50.0, 50.0));
    view.tick = 120;
    facts.tick = 120;
    state.observe(&view, &facts).unwrap();
    assert_eq!(support(&state.plan(&view, &facts).unwrap()), None);
    let before = state.clone();
    facts.tick = 119;
    assert!(state.observe(&view, &facts).is_err());
    assert_eq!(state, before);
}

#[test]
fn intent_ai57_base_planes_choose_army_nearest_visible_targets_and_patrol() {
    let (view, mut facts) = fixture(vec![
        unit(1, 0, "rifle", 100.0, 100.0),
        unit(2, 0, "attacker", 0.0, 0.0),
        unit(3, 0, "fighter", 0.0, 0.0),
        unit(4, 0, "bomber", 0.0, 0.0),
        unit(10, 1, "tank", 110.0, 100.0),
        unit(11, 1, "tank", 90.0, 100.0),
        unit(12, 1, "rifle", 100.0, 100.0),
        unit(13, 1, "fighter", 105.0, 100.0),
    ]);
    let commands = plan(&view, &facts);
    assert!(
        commands
            .iter()
            .any(|c| matches!(c,Command::Attack{ids,target:10,..}if ids==&[2]))
    );
    for id in [3, 4] {
        assert!(
            commands
                .iter()
                .any(|c| matches!(c,Command::Move{orders,attack_move:false,..}
            if orders.len()==1 && orders[0].unit==id && orders[0].at==Point{x:105.0,z:100.0}))
        );
    }
    facts.units.remove(&13);
    assert!(
        plan(&view, &facts)
            .iter()
            .any(|c| matches!(c,Command::Move{orders,..}
        if orders[0].unit==3 && orders[0].at==Point{x:100.0,z:100.0}))
    );
    facts.units.get_mut(&2).unwrap().plane = Some(PlaneState::Rearm);
    assert!(
        !plan(&view, &facts)
            .iter()
            .any(|c| matches!(c,Command::Attack{ids,..}if ids==&[2]))
    );
    facts.units.get_mut(&2).unwrap().plane = Some(PlaneState::Base);
    facts.units.get_mut(&10).unwrap().at.x = 220.0;
    facts.units.get_mut(&11).unwrap().at.x = 221.0;
    assert!(
        !plan(&view, &facts)
            .iter()
            .any(|c| matches!(c,Command::Attack{ids,..}if ids==&[2]))
    );
}

#[test]
fn intent_ai57_all_ground_abilities_ignore_notice_and_human_cast_limit() {
    // Source shared/ai.js:653-668, Normal Horde bypasses the two-cast limit.
    let mut commando = unit(5, 0, "commando", 100.0, 100.0);
    commando.hp = 10.0;
    let mut conscript = unit(6, 0, "conscript", 100.0, 100.0);
    conscript.suppression = 50.0;
    let mut tank = unit(4, 0, "tank", 100.0, 100.0);
    tank.hp = 100.0;
    let mut target = unit(10, 1, "mg", 110.0, 100.0);
    target.garrisoned = true;
    let (mut view, mut facts) = fixture(vec![
        unit(1, 0, "rifle", 100.0, 100.0),
        unit(2, 0, "mg", 100.0, 100.0),
        unit(3, 0, "at", 100.0, 100.0),
        tank,
        commando,
        conscript,
        unit(7, 0, "mortar", 100.0, 100.0),
        target,
        unit(11, 1, "tank", 112.0, 100.0),
    ]);
    view.units.get_mut(&2).unwrap().plan.target = 10;
    view.units.get_mut(&3).unwrap().plan.target = 11;
    let commands = plan(&view, &facts);
    for id in 1..=7 {
        assert!(ability(&commands, id).is_some(), "ability for {id}");
    }
    assert_eq!(
        ability(&commands, 1),
        Some(Some(Point { x: 110.0, z: 100.0 }))
    );
    assert_eq!(ability(&commands, 2), Some(None));
    assert_eq!(ability(&commands, 3), Some(None));
    assert_eq!(ability(&commands, 4), Some(None));
    assert_eq!(
        ability(&commands, 5),
        Some(Some(Point { x: 110.0, z: 100.0 }))
    );
    assert_eq!(ability(&commands, 6), Some(None));
    assert_eq!(
        ability(&commands, 7),
        Some(Some(Point { x: 110.0, z: 100.0 }))
    );
    for id in 1..=7 {
        facts.units.get_mut(&id).unwrap().cooldown = 1.0;
    }
    assert!(
        !plan(&view, &facts)
            .iter()
            .any(|c| matches!(c, Command::Ability { .. }))
    );
    facts.units.get_mut(&1).unwrap().cooldown = 0.0;
    view.units.get_mut(&1).unwrap().plan.retreating = true;
    assert_eq!(ability(&plan(&view, &facts), 1), None);
    facts.units.get_mut(&1).unwrap().riding = true;
    view.units.get_mut(&1).unwrap().plan.retreating = false;
    assert_eq!(ability(&plan(&view, &facts), 1), None);
}

#[test]
fn intent_ai57_delivered_facts_validate_binding_and_hidden_state_is_not_an_input() {
    let (view, mut facts) = fixture(vec![
        unit(1, 0, "rifle", 100.0, 100.0),
        unit(10, 1, "bunker", 150.0, 150.0),
    ]);
    let first = plan(&view, &facts);
    // Enemy cooldown and stance are never read by the policy.
    facts.units.get_mut(&10).unwrap().cooldown = 99999.0;
    facts.units.get_mut(&10).unwrap().auto_retreat = true;
    assert_eq!(plan(&view, &facts), first);
    facts.slot = 1;
    assert!(facts.validate(&view).is_err());
    facts.slot = 0;
    facts.tick = 1;
    assert!(facts.validate(&view).is_err());
    facts.tick = 0;
    facts.units.get_mut(&1).unwrap().owner = 1;
    assert!(facts.validate(&view).is_err());
}

#[test]
fn intent_ai57_submission_results_keep_refused_abilities_idle_and_support_available() {
    // Original pinned ai.js with an intercepting submitter produces movement
    // after a refused grenade, and installs local nade intent only on acceptance.
    let (view, mut facts) = fixture(vec![
        unit(1, 0, "rifle", 100.0, 100.0),
        unit(10, 1, "mg", 110.0, 100.0),
        unit(11, 1, "bunker", 150.0, 150.0),
    ]);
    let mut policy = HordePolicy::default();
    policy.observe(&view, &facts).unwrap();
    let accepted = policy
        .plan_with_submission(&view, &facts, |_| true)
        .unwrap();
    let denied = policy
        .plan_with_submission(&view, &facts, |command| {
            !matches!(command, Command::Ability { .. })
        })
        .unwrap();
    assert!(ability(&accepted, 1).is_some());
    assert!(
        ability(&denied, 1).is_some(),
        "refused attempt remains in the record"
    );
    let moved = |commands: &[Command]| {
        commands.iter().any(|c| {
            matches!(c,
        Command::Move { orders, .. } if orders.iter().any(|o| o.unit == 1))
        })
    };
    assert!(!moved(&accepted));
    assert!(moved(&denied));
    facts.units.insert(12, unit(12, 1, "tank", 115.0, 100.0));
    facts.strikes.push(Strike {
        at: Point { x: 100.0, z: 100.0 },
        owner: 1,
        kind: "bombing".into(),
        delay: 2.0,
    });
    policy.observe(&view, &facts).unwrap();
    let commands = policy
        .plan_with_submission(&view, &facts, |command| {
            !matches!(command,
        Command::Support { kind, .. } if kind == "cover")
        })
        .unwrap();
    let calls: Vec<_> = commands
        .iter()
        .filter_map(|c| match c {
            Command::Support { kind, .. } => Some(kind.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        calls,
        ["cover", "dive"],
        "a refused cover does not consume the look's support call"
    );
}
