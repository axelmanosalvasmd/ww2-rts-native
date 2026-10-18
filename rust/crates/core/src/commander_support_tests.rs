use super::{
    tests::{fixture, unit},
    *,
};
use crate::commander_ai::{Input, PointObservation};

const KINDS: [&str; 7] = [
    "cover",
    "dive",
    "para",
    "bombing",
    "artillery",
    "strafe",
    "recon",
];

fn enemy(id: u32, kind: &str, x: f64) -> UnitObservation {
    let mut u = unit(id, kind, x);
    u.owner = 1;
    u
}

fn point(x: f64) -> PointObservation {
    PointObservation {
        at: Point { x, z: 20.0 },
        owner: Some(1),
        vp: 1.0,
        mp: 0.0,
        cut: false,
        locked: false,
    }
}

fn scene(enemies: Vec<UnitObservation>) -> Input {
    let mut units: Vec<_> = (1..=6)
        .map(|id| unit(id, "rifle", 30.0 + f64::from(id)))
        .collect();
    units.extend(enemies);
    let mut input = fixture(units);
    input.manpower = 5000.0;
    input.points = vec![point(80.0)];
    let context = input.runtime.as_mut().unwrap();
    context.munitions = 5000.0;
    context.support = KINDS.into_iter().map(|kind| (kind.into(), 0.0)).collect();
    input
}

fn decide(
    input: &Input,
    state: &mut State,
    level: Level,
    goal: Option<Point>,
    sees: bool,
) -> Option<Value> {
    let observation = Observation::new(input.clone()).unwrap();
    let input = observation.input();
    let context = input.runtime.as_ref().unwrap();
    let team = input.players[input.slot].team;
    let now = input.tick as f64 / 20.0;
    let enemies: Vec<_> = input
        .units
        .iter()
        .filter(|u| input.players[u.owner].team != team)
        .collect();
    let own: Vec<_> = input
        .units
        .iter()
        .filter(|u| u.owner == input.slot)
        .collect();
    let mine: Vec<_> = own
        .iter()
        .copied()
        .filter(|u| u.kind != "engineer")
        .collect();
    state.mind.update(&observation, &[], true);
    let mut budget = SupportBudget {
        mp: input.manpower,
        mun: context.munitions,
        last: state.called,
        spent: false,
    };
    let situation = Situation {
        kind: SituationKind::Push,
        until: now + 10.0,
        point: None,
        threat: None,
        at: goal,
        counter: None,
        safe: None,
        failed_at: 0,
    };
    let result = support_planner::plan(
        &observation,
        &own,
        &mine,
        &enemies,
        state,
        &situation,
        Skill::for_level(level),
        &mut budget,
        |_| sees,
    )
    .unwrap();
    state.called = budget.last;
    result
}

fn primed(input: &Input, level: Level) -> State {
    let mut early = input.clone();
    early.tick -= 60;
    early.runtime.as_mut().unwrap().support.clear();
    let mut state = State::default();
    assert_eq!(decide(&early, &mut state, level, None, true), None);
    state
}

fn kind(result: &Option<Value>) -> Option<&str> {
    result.as_ref().and_then(|value| value["kind"].as_str())
}

#[test]
fn intent_ai34_support_priority_and_one_call_per_look() {
    let mut input = scene(vec![
        enemy(20, "tank", 80.0),
        enemy(21, "rifle", 82.0),
        enemy(22, "rifle", 84.0),
    ]);
    input
        .runtime
        .as_mut()
        .unwrap()
        .strikes
        .push(AnnouncedStrike {
            owner: 1,
            kind: "bombing".into(),
            at: Point { x: 32.0, z: 20.0 },
            delay: 4.0,
        });
    for expected in ["cover", "dive", "para", "bombing", "artillery", "strafe"] {
        let mut state = primed(&input, Level::Normal);
        let command = decide(&input, &mut state, Level::Normal, None, true);
        assert_eq!(kind(&command), Some(expected), "{command:?}");
        assert_eq!(state.called, Some(10.0));
        input
            .runtime
            .as_mut()
            .unwrap()
            .support
            .insert(expected.into(), 1.0);
    }
    // The ordinary planner uses the same priority and does not add a second adaptive or smoke call.
    input.runtime.as_mut().unwrap().support = KINDS.into_iter().map(|k| (k.into(), 0.0)).collect();
    let mut state = primed(&input, Level::Normal);
    let commands = plan(
        &Observation::new(input).unwrap(),
        &[],
        &mut state,
        Level::Normal,
        &mut MatchRandom::new(34),
    )
    .unwrap();
    let support: Vec<_> = commands.iter().filter(|c| c["t"] == "support").collect();
    assert_eq!(support.len(), 1, "{commands:?}");
    assert_eq!(support[0]["kind"], "cover");
}

#[test]
fn intent_ai34_cover_uses_announced_enemy_planes_near_own_builders() {
    for plane in ["strafe", "bombing", "dive", "para"] {
        let mut input = scene(vec![]);
        input.units = vec![unit(1, "engineer", 30.0)];
        input.points.clear();
        input.runtime.as_mut().unwrap().strikes = vec![AnnouncedStrike {
            owner: 1,
            kind: plane.into(),
            at: Point { x: 54.9, z: 20.0 },
            delay: 0.1,
        }];
        let result = decide(
            &input,
            &mut State::default(),
            Level::Normal,
            Some(Point { x: 140.0, z: 20.0 }),
            true,
        );
        assert_eq!(kind(&result), Some("cover"));
        for (owner, delay, x, kind_name) in [
            (2, 1.0, 30.0, plane),
            (1, 0.0, 30.0, plane),
            (1, 1.0, 55.0, plane),
            (1, 1.0, 30.0, "artillery"),
        ] {
            let strike = &mut input.runtime.as_mut().unwrap().strikes[0];
            strike.owner = owner;
            strike.delay = delay;
            strike.at.x = x;
            strike.kind = kind_name.into();
            assert_eq!(
                decide(&input, &mut State::default(), Level::Normal, None, true),
                None
            );
        }
    }
}

#[test]
fn intent_ai34_dive_and_bombing_wait_for_current_watched_targets() {
    for excluded in ["tankdestroyer", "rocket", "at"] {
        let mut input = scene(vec![enemy(20, excluded, 80.0)]);
        input.points.clear();
        let mut state = primed(&input, Level::Normal);
        assert_eq!(decide(&input, &mut state, Level::Normal, None, true), None);
    }
    for heavy in ["tank", "medium", "tiger", "churchill", "flaktrack"] {
        let mut input = scene(vec![enemy(20, heavy, 80.0)]);
        input.points.clear();
        assert_eq!(
            decide(&input, &mut State::default(), Level::Normal, None, true),
            None
        );
        let mut state = primed(&input, Level::Normal);
        assert_eq!(
            kind(&decide(&input, &mut state, Level::Normal, None, true)),
            Some("dive")
        );
        input
            .runtime
            .as_mut()
            .unwrap()
            .support
            .insert("dive".into(), 1.0);
        let mut state = primed(&input, Level::Normal);
        assert_eq!(
            kind(&decide(&input, &mut state, Level::Normal, None, true)),
            if heavy == "tank" {
                Some("bombing")
            } else {
                None
            }
        );
    }
    let mut garrison = enemy(20, "rifle", 80.0);
    garrison.garrison = true;
    let mut input = scene(vec![garrison]);
    input.points.clear();
    let mut state = primed(&input, Level::Normal);
    assert_eq!(
        kind(&decide(&input, &mut state, Level::Normal, None, true)),
        Some("bombing")
    );
    // Remembering the contact cannot aim a new strike after it leaves the delivered view.
    input.units.retain(|u| u.owner == 0);
    input.tick += 40;
    assert_eq!(decide(&input, &mut state, Level::Normal, None, true), None);
}

#[test]
fn intent_ai34_paradrop_requires_seen_enemy_point_notice_six_units_and_non_horde() {
    let mut input = scene(vec![]);
    // Isolate paradrop notice from the independent no-enemy recon fallback.
    input
        .runtime
        .as_mut()
        .unwrap()
        .support
        .insert("recon".into(), 1.0);
    assert_eq!(
        decide(&input, &mut State::default(), Level::Normal, None, true),
        None
    );
    let mut state = primed(&input, Level::Normal);
    assert_eq!(
        kind(&decide(&input, &mut state, Level::Normal, None, true)),
        Some("para")
    );
    for case in 0..5 {
        let mut changed = input.clone();
        match case {
            0 => {
                changed.units.pop();
            }
            1 => {
                changed.runtime.as_mut().unwrap().horde = true;
            }
            2 => {
                changed.points[0].owner = Some(2);
            }
            3 => {
                changed.points[0].owner = None;
            }
            _ => {}
        }
        // Recon is independent of seeing the point and must not conceal the paradrop assertion.
        changed
            .runtime
            .as_mut()
            .unwrap()
            .support
            .insert("recon".into(), 1.0);
        let mut state = primed(&changed, Level::Normal);
        assert_eq!(
            decide(&changed, &mut state, Level::Normal, None, case != 4),
            None,
            "case {case}"
        );
    }
}

#[test]
fn intent_ai34_artillery_and_strafe_cluster_selection_and_hard_friend_safety() {
    let input = scene(vec![
        enemy(20, "rifle", 60.0),
        enemy(21, "rifle", 62.0),
        enemy(22, "rifle", 90.0),
        enemy(23, "rifle", 92.0),
        enemy(24, "rifle", 94.0),
        enemy(25, "rifle", 96.0),
    ]);
    for (level, x) in [(Level::Normal, 61.0), (Level::Hard, 93.0)] {
        let mut changed = input.clone();
        changed.points.clear();
        let mut state = primed(&changed, level);
        let result = decide(&changed, &mut state, level, None, true).unwrap();
        assert_eq!(result["kind"], "artillery");
        assert_eq!(result["x"], x);
    }
    let mut changed = input.clone();
    changed.points.clear();
    changed.units[0].at.x = 93.0;
    let mut state = primed(&changed, Level::Hard);
    assert_eq!(decide(&changed, &mut state, Level::Hard, None, true), None);
    changed.units[0].at.x = 103.0;
    let mut state = primed(&changed, Level::Hard);
    assert_eq!(
        kind(&decide(&changed, &mut state, Level::Hard, None, true)),
        Some("artillery")
    );

    let mut spread = scene(vec![enemy(20, "rifle", 70.0), enemy(21, "rifle", 79.0)]);
    spread.points.clear();
    let mut state = primed(&spread, Level::Normal);
    assert_eq!(
        decide(&spread, &mut state, Level::Normal, None, true),
        None,
        "eligible artillery keeps precedence even without a target"
    );
    spread
        .runtime
        .as_mut()
        .unwrap()
        .support
        .insert("artillery".into(), 1.0);
    let mut state = primed(&spread, Level::Normal);
    assert_eq!(
        kind(&decide(&spread, &mut state, Level::Normal, None, true)),
        Some("strafe")
    );
    spread.units.last_mut().unwrap().kind = "armoredcar".into();
    let mut state = primed(&spread, Level::Normal);
    assert_eq!(decide(&spread, &mut state, Level::Normal, None, true), None);
}

#[test]
fn intent_ai34_artillery_requires_a_delivered_stationary_clock_older_than_three_seconds() {
    for gun in ["mg", "at"] {
        let mut input = scene(vec![enemy(20, gun, 80.0)]);
        input.points.clear();
        let mut state = primed(&input, Level::Normal);
        assert_eq!(decide(&input, &mut state, Level::Normal, None, true), None);
        // Repeated attention looks cannot invent or age a missing delivered clock.
        input.tick += 100;
        assert_eq!(decide(&input, &mut state, Level::Normal, None, true), None);
        input
            .runtime
            .as_mut()
            .unwrap()
            .stationary_since
            .insert(20, 12.0);
        assert_eq!(decide(&input, &mut state, Level::Normal, None, true), None);
        input.tick += 1;
        assert_eq!(
            kind(&decide(&input, &mut state, Level::Normal, None, true)),
            Some("artillery")
        );
        // A packet that saw movement or a reappearing contact supplies a fresh clock.
        input
            .runtime
            .as_mut()
            .unwrap()
            .stationary_since
            .insert(20, 15.05);
        assert_eq!(decide(&input, &mut state, Level::Normal, None, true), None);
        input.tick += 61;
        assert_eq!(
            kind(&decide(&input, &mut state, Level::Normal, None, true)),
            Some("artillery")
        );
        input.runtime.as_mut().unwrap().stationary_since.clear();
        assert_eq!(decide(&input, &mut state, Level::Normal, None, true), None);
    }
}

#[test]
fn intent_ai34_reserves_cooldown_gap_and_aim_boundaries() {
    for (mode, level) in [
        (Mode::Conquest, Level::Normal),
        (Mode::Conquest, Level::Easy),
        (Mode::Classic, Level::Easy),
    ] {
        let mut context = Context {
            mode,
            ..Context::default()
        };
        context.support.insert("dive".into(), 0.0);
        let skill = Skill::for_level(level);
        let price = economy::sup_cost(mode, "dive").unwrap();
        let reserve = if mode.is_construction() {
            skill.mun_reserve
        } else {
            skill.sup_reserve
        };
        let goal = Point { x: 80.0, z: 20.0 };
        let sit = Situation {
            kind: SituationKind::Push,
            until: 100.0,
            point: None,
            threat: None,
            at: Some(goal),
            counter: None,
            safe: None,
            failed_at: 0,
        };
        for (balance, cooldown, elapsed, distance, expected) in [
            (price.cost + reserve - 0.01, 0.0, 45.0, 0.0, false),
            (price.cost + reserve, 0.01, 45.0, 0.0, false),
            (price.cost + reserve, 0.0, 44.99, 0.0, level != Level::Easy),
            (price.cost + reserve, 0.0, 45.0, 40.0, true),
            (price.cost + reserve, 0.0, 45.0, 40.01, false),
        ] {
            context.support.insert("dive".into(), cooldown);
            let mut budget = SupportBudget {
                mp: if mode.is_construction() { 0.0 } else { balance },
                mun: if mode.is_construction() { balance } else { 0.0 },
                last: Some(10.0),
                spent: false,
            };
            let at = Point {
                x: goal.x + distance,
                z: goal.z,
            };
            let command = budget
                .call("dive", at, None, &sit, &context, skill, 10.0 + elapsed)
                .unwrap();
            assert_eq!(
                command.is_some(),
                expected,
                "{mode:?} {level:?} {balance} {cooldown} {elapsed} {distance}"
            );
            if expected {
                assert_eq!(
                    if mode.is_construction() {
                        budget.mun
                    } else {
                        budget.mp
                    },
                    reserve
                );
                assert_eq!(
                    budget
                        .call("dive", at, None, &sit, &context, skill, 10.0 + elapsed)
                        .unwrap(),
                    None
                );
            } else {
                assert_eq!(budget.last, Some(10.0));
            }
        }
    }
}

#[test]
fn intent_ai34_recon_requires_no_current_enemy_and_classic_or_more_than_250_mp() {
    for (mode, mp, expected) in [
        (Mode::Conquest, 250.0, false),
        (Mode::Conquest, 250.01, true),
        (Mode::Classic, 0.0, true),
    ] {
        let mut input = scene(vec![]);
        input.units.truncate(1);
        input.manpower = mp;
        let context = input.runtime.as_mut().unwrap();
        context.mode = mode;
        context.support.insert("para".into(), 1.0);
        let mut state = State::default();
        let result = decide(&input, &mut state, Level::Normal, None, false);
        assert_eq!(kind(&result), expected.then_some("recon"));
        input.units.push(enemy(20, "rifle", 80.0));
        assert_eq!(
            decide(&input, &mut State::default(), Level::Normal, None, false),
            None
        );
    }
}

#[test]
fn intent_ai34_malformed_strike_refuses_without_mutating_private_state_or_rng() {
    for case in 0..4 {
        let mut input = scene(vec![]);
        let mut strike = AnnouncedStrike {
            owner: 1,
            kind: "bombing".into(),
            at: Point { x: 32.0, z: 20.0 },
            delay: 4.0,
        };
        match case {
            0 => strike.owner = 3,
            1 => strike.kind = "unknown".into(),
            2 => strike.at.x = f64::NAN,
            _ => strike.delay = f64::INFINITY,
        }
        input.runtime.as_mut().unwrap().strikes.push(strike);
        let mut state = State::default();
        let mut random = MatchRandom::new(34);
        let before = (state.clone(), random.clone());
        assert_eq!(
            plan(
                &Observation::new(input).unwrap(),
                &[],
                &mut state,
                Level::Normal,
                &mut random
            ),
            Err("invalid delivered strike announcement".into())
        );
        assert_eq!((state, random), before);
    }
}

#[test]
fn intent_ai34_malformed_stationary_clock_refuses_without_mutating_private_state_or_rng() {
    for (id, since) in [(20, f64::NAN), (20, -1.0), (20, 10.01), (99, 5.0)] {
        let mut input = scene(vec![enemy(20, "mg", 80.0)]);
        input
            .runtime
            .as_mut()
            .unwrap()
            .stationary_since
            .insert(id, since);
        let mut state = State::default();
        let mut random = MatchRandom::new(34);
        let before = (state.clone(), random.clone());
        assert_eq!(
            plan(
                &Observation::new(input).unwrap(),
                &[],
                &mut state,
                Level::Normal,
                &mut random
            ),
            Err("invalid delivered stationary clock".into())
        );
        assert_eq!((state, random), before);
    }
}

#[test]
fn intent_ai34_delivered_enemy_strike_plans_cover_and_survives_real_command_ticks_and_all_seat_views()
 {
    use crate::{
        ai_view::commander::CommanderMemory,
        economy::Faction,
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_commands::Command,
        match_runtime::{Match, MatchOptions, PlayerConfig},
        match_support::SupportEvent,
        match_tick::TickEvent,
        orders::OrderKey,
        recipient_snapshot::{DeliveryState, SelectedSeat},
        weather::WeatherChoice,
    };
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"AI support intent", "w":80,"h":80,
            "rows":vec![".".repeat(80);80],
            "spawns":[{"x":8,"y":8},{"x":8,"y":72},{"x":72,"y":72}],
            "points":[{"x":40,"y":40}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 3401,
            mode: Mode::Conquest,
            players: [0, 0, 1]
                .into_iter()
                .enumerate()
                .map(|(slot, team)| PlayerConfig {
                    name: format!("P{slot}"),
                    team,
                    faction: Faction::Usa,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    for player in &mut state.start.economy.players {
        player.mp = 5000.0;
    }
    let own = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "rifle")
        .unwrap();
    let at = Point { x: own.x, z: own.z };
    assert_eq!(
        state.admit_support(2, "bombing", at, Some(0.0)).unwrap(),
        None
    );
    state.restrict_to_ordinary_snapshots().unwrap();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let mut memories = (0..3)
        .map(|slot| state.ordinary_ai_view_memory(slot).unwrap())
        .collect::<Vec<_>>();
    let selected = (0..3)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let initial = delivery
        .capture_selected_with_tick_events(&mut state, &selected, &[])
        .unwrap()
        .unwrap();
    let views = (0..3)
        .map(|slot| {
            memories[slot]
                .observe(initial.recipient(slot).unwrap())
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(views[0].strikes[0]["owner"], 2);
    let mut commander = CommanderMemory::new(0, 80, 80).unwrap();
    let observed = commander.observe(&views[0]).unwrap();
    let mut private = State::default();
    let mut random = MatchRandom::new(34);
    let commands = plan(
        &observed.packet,
        &[],
        &mut private,
        Level::Normal,
        &mut random,
    )
    .unwrap();
    let calls = commands
        .iter()
        .filter(|c| c["t"] == "support")
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 1, "{commands:?}");
    assert_eq!(calls[0]["kind"], "cover");
    state
        .enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 1,
            },
            Command::from_json(calls[0]).unwrap(),
        )
        .unwrap();
    let mut cover_seen = false;
    let mut intercepted = false;
    let selected = (0..3)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    for tick in 1..=150 {
        let report = state.advance_tick().unwrap();
        if tick == 1 {
            assert_eq!(report.commands.len(), 1);
            assert_eq!(report.commands[0].denial, None);
        }
        intercepted |= report.events.iter().any(|event| {
            matches!(
                event,
                TickEvent::Support(SupportEvent::ShotDown { fighters: true, .. })
            )
        });
        let frame = delivery
            .capture_selected_with_tick_events(&mut state, &selected, &report.events)
            .unwrap()
            .unwrap();
        let views = (0..3)
            .map(|slot| {
                memories[slot]
                    .observe(frame.recipient(slot).unwrap())
                    .unwrap()
            })
            .collect::<Vec<_>>();
        cover_seen |= !views[0].covers.is_empty();
        assert_eq!(views[0].covers, views[1].covers);
        assert!(views[2].covers.is_empty());
        assert_eq!(views[0].strikes, views[2].strikes);
        let observed = commander.observe(&views[0]).unwrap();
        assert_eq!(observed.packet.input().tick, tick);
        assert!(state.tick_state.fault().is_none());
    }
    assert!(
        cover_seen,
        "the real planned cover arrives and reaches allied views"
    );
    assert!(
        intercepted,
        "the planned cover intercepts the announced enemy strike"
    );
    assert!(state.support.covers.is_empty());
    assert_eq!(private.called, Some(0.0));
}
