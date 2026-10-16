//! Original exported thinker outputs for the bounded quiet-opening policy.
use serde_json::Value;
use ww2_core::{
    commander_ai::{
        self, Aim, Input, Level, Observation, PlayerObservation, Point, PointObservation,
        PolicyError, SourceState, UnitObservation,
    },
    match_commands::Command,
    random::{MatchRandom, StreamId},
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/commander-ai.json")).unwrap()
}
fn array(value: &Value) -> &[Value] {
    value.as_array().unwrap()
}
fn integer(value: &Value) -> usize {
    usize::try_from(value.as_u64().unwrap()).unwrap()
}
fn number(value: &Value) -> f64 {
    value.as_f64().unwrap()
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().unwrap() != 0.0,
        Value::String(v) => !v.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}
fn point(value: &Value) -> Point {
    Point {
        x: number(&value["x"]),
        z: number(&value["z"]),
    }
}
fn level(value: &Value) -> Level {
    match value.as_str().unwrap() {
        "easy" => Level::Easy,
        "normal" => Level::Normal,
        "hard" => Level::Hard,
        other => panic!("unrecorded level {other}"),
    }
}
fn input(row: &Value) -> Input {
    let frame = &row["frame"];
    Input {
        width: integer(&frame["w"]),
        height: integer(&frame["h"]),
        tick: frame["tick"].as_i64().unwrap(),
        slot: integer(&row["slot"]),
        players: array(&frame["players"])
            .iter()
            .map(|p| PlayerObservation {
                team: i32::try_from(p["team"].as_i64().unwrap()).unwrap(),
                out: truthy(&p["out"]),
                spawn: point(&p["spawn"]),
            })
            .collect(),
        units: array(&frame["units"])
            .iter()
            .map(|u| UnitObservation {
                id: u32::try_from(integer(&u["id"])).unwrap(),
                kind: u["type"].as_str().unwrap().into(),
                owner: integer(&u["owner"]),
                at: point(u),
                hp: number(&u["hp"]),
                supp: number(&u["supp"]),
                target_id: u32::try_from(integer(&u["targetId"])).unwrap(),
                cover: u8::try_from(integer(&u["cover"])).unwrap(),
                path: array(&u["path"]).iter().map(point).collect(),
                attack_id: truthy(&u["attackId"]),
                retreating: truthy(&u["retreating"]),
                // The ordinary source loop reads the delivered riding bit directly.
                riding: u["flags"].as_u64().unwrap() & 262_144 != 0,
                build: truthy(&u["build"]),
                dig: truthy(&u["dig"]),
                entrench: truthy(&u["entrench"]),
                garrison: u["garrison"].as_i64().is_some_and(|v| v >= 0),
                enter: u["enter"].as_i64().is_some_and(|v| v >= 0),
                fire_at: u["fireAt"].as_i64().is_some_and(|v| v >= 0),
                nade: truthy(&u["nade"]),
            })
            .collect(),
        points: array(&frame["points"])
            .iter()
            .map(|p| PointObservation {
                at: point(p),
                vp: number(&p["vp"]),
                mp: number(&p["mp"]),
                owner: p["owner"].as_u64().map(|v| usize::try_from(v).unwrap()),
                cut: truthy(&p["cut"]),
                locked: truthy(&p["locked"]),
            })
            .collect(),
        chars: array(&frame["chars"])
            .iter()
            .flat_map(|run| {
                std::iter::repeat_n(run[0].as_str().unwrap().as_bytes()[0], integer(&run[1]))
            })
            .collect(),
        manpower: number(&frame["me"]["mp"]),
        research: None,
        runtime: None,
        flags: array(&frame["flags"])
            .iter()
            .flat_map(|run| {
                std::iter::repeat_n(u16::try_from(integer(&run[0])).unwrap(), integer(&run[1]))
            })
            .collect(),
    }
}
fn state(row: &Value) -> SourceState {
    let memory = &row["memory"];
    SourceState {
        aim: aim(&memory["aim"]),
        point_failures: array(&memory["pointFailures"])
            .iter()
            .map(|v| u8::try_from(integer(v)).unwrap())
            .collect(),
    }
}
fn aim(value: &Value) -> Option<Aim> {
    (!value.is_null()).then(|| Aim {
        i: integer(&value["i"]),
        until: number(&value["until"]),
    })
}
fn aim_bits(value: Option<Aim>) -> Option<(usize, u64)> {
    value.map(|a| (a.i, a.until.to_bits()))
}
type CommandBits = (bool, Vec<(i64, u64, u64)>);
fn command_bits(commands: &[Command]) -> Vec<CommandBits> {
    commands
        .iter()
        .filter(|c| !matches!(c, Command::Cover { .. }))
        .map(|c| {
            let Command::Move {
                orders,
                attack_move,
                control_ids,
                queue,
                together,
                face,
            } = c
            else {
                panic!("opening emitted a non-movement command: {c:?}");
            };
            assert!(control_ids.is_none() && !queue && !together && face.is_none());
            (
                *attack_move,
                orders
                    .iter()
                    .map(|o| (o.unit, o.at.x.to_bits(), o.at.z.to_bits()))
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn original_thinker_orders_aim_and_waypoint_draws_match() {
    let records = fixture();
    assert_eq!(
        records["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(array(&records["cases"]).len(), 62);
    let mut draws = 0;
    let mut pending = 0;
    for row in array(&records["cases"]) {
        for command in array(&row["expected"]["pendingCommands"]) {
            assert!(
                matches!(command["t"].as_str(), Some("cover" | "garrison")),
                "unrecorded pending AI branch"
            );
            pending += 1;
        }
        let name = row["name"].as_str().unwrap();
        let observation = Observation::new(input(row)).unwrap();
        let before = observation.clone();
        let mut source = state(row);
        let failures = source.point_failures.clone();
        let fractions: Vec<_> = array(&row["random"]).iter().map(number).collect();
        if row["expectedNativeError"] == "UnsupportedHolder" {
            let before_state = source.clone();
            assert_eq!(
                commander_ai::plan_with_fractions(
                    &observation,
                    &mut source,
                    level(&row["level"]),
                    &fractions
                ),
                Err(PolicyError::UnsupportedHolder)
            );
            assert_eq!(
                source, before_state,
                "{name}: unsupported holder leaves private memory unchanged"
            );
            let mut random = MatchRandom::new(778);
            let before_random = random.clone();
            assert_eq!(
                commander_ai::plan(&observation, &mut source, level(&row["level"]), &mut random),
                Err(PolicyError::UnsupportedHolder)
            );
            assert_eq!(
                random, before_random,
                "{name}: unsupported holder leaves RNG unchanged"
            );
            assert_eq!(source, before_state);
            continue;
        }
        let result = commander_ai::plan_with_fractions(
            &observation,
            &mut source,
            level(&row["level"]),
            &fractions,
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let expected: Vec<_> = array(&row["expected"]["commands"])
            .iter()
            .map(|v| Command::from_json(v).unwrap())
            .collect();
        let actual_cover: Vec<_> = result
            .commands
            .iter()
            .filter_map(|c| match c {
                Command::Cover { ids, .. } => Some(serde_json::json!({"t":"cover","ids":ids})),
                _ => None,
            })
            .collect();
        let expected_cover: Vec<_> = array(&row["expected"]["pendingCommands"])
            .iter()
            .filter(|c| c["t"] == "cover")
            .cloned()
            .collect();
        assert_eq!(actual_cover, expected_cover, "holder {name}");

        assert_eq!(
            command_bits(&result.commands),
            command_bits(&expected),
            "{name}: commands"
        );
        assert_eq!(
            aim_bits(source.aim),
            aim_bits(aim(&row["expected"]["aim"])),
            "{name}: aim"
        );
        assert_eq!(
            result.draws,
            integer(&row["expected"]["draws"]),
            "{name}: RNG draws"
        );
        assert_eq!(
            result.draws,
            fractions.len(),
            "{name}: source tape consumed exactly"
        );
        assert_eq!(
            source.point_failures, failures,
            "{name}: loss memory retained"
        );
        assert_eq!(
            observation, before,
            "{name}: delivered input remains immutable"
        );
        draws += result.draws;
    }
    assert_eq!(draws, 270);
    assert_eq!(
        pending, 4,
        "actual quiet holder and unsupported garrison orders retained"
    );
}

#[test]
fn original_dispatch_schedule_and_large_ticks_are_safe() {
    let records = fixture();
    assert_eq!(array(&records["schedule"]).len(), 18);
    for row in array(&records["schedule"]) {
        let level = level(&row["level"]);
        let slot = integer(&row["slot"]);
        assert_eq!(level.every(), row["every"].as_i64().unwrap());
        let acts: Vec<_> = (0i64..=240).filter(|&tick| level.due(tick, slot)).collect();
        let views: Vec<_> = (0i64..=240)
            .filter(|&tick| level.wants_view(tick, slot))
            .collect();
        let expected_acts: Vec<_> = array(&row["acts"])
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        let expected_views: Vec<_> = array(&row["views"])
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        assert_eq!(acts, expected_acts, "{level:?} slot {slot}: acts");
        assert_eq!(views, expected_views, "{level:?} slot {slot}: views");
    }
    // Compute in a wider integer type so the scheduling API cannot hide signed overflow.
    for level in [Level::Easy, Level::Normal, Level::Hard] {
        for tick in [i64::MAX - 13, i64::MAX] {
            for slot in [0, 5, usize::MAX] {
                let every = i128::from(level.every());
                let phase = (i128::from(tick) + slot as i128 * 13) % every;
                assert_eq!(commander_ai::due(tick, slot, level), phase == 0);
                assert_eq!(
                    commander_ai::wants_view(tick, slot, level),
                    (every - phase) % every < 4
                );
            }
        }
        assert!(!level.due(-1, 0) && !level.wants_view(-1, 0));
    }
}

#[test]
fn invalid_observations_and_tapes_do_not_commit_private_state() {
    let records = fixture();
    let row = &records["cases"][0];
    let valid = input(row);
    let observation = Observation::new(valid.clone()).unwrap();
    let before = SourceState {
        aim: Some(Aim { i: 1, until: 0.0 }),
        point_failures: vec![3, 0, 9],
    };
    let mut source = before.clone();
    assert_eq!(
        commander_ai::plan_with_fractions(&observation, &mut source, Level::Normal, &[]),
        Err(PolicyError::ExhaustedFractions)
    );
    assert_eq!(
        source, before,
        "failed tape must not commit expired aim replacement"
    );
    for fraction in [f64::NAN, f64::INFINITY, -0.1, 1.0] {
        assert_eq!(
            commander_ai::plan_with_fractions(
                &observation,
                &mut source,
                Level::Normal,
                &[fraction]
            ),
            Err(PolicyError::InvalidFraction)
        );
        assert_eq!(source, before);
    }
    let mut duplicate = valid.clone();
    duplicate.units[1].id = duplicate.units[0].id;
    assert_eq!(Observation::new(duplicate), Err(PolicyError::UnitOrder));
    let mut unknown = valid.clone();
    unknown.units[0].kind = "unknown-kind".into();
    assert_eq!(Observation::new(unknown), Err(PolicyError::UnitKind));
    let mut bad_shape = valid.clone();
    bad_shape.flags.pop();
    assert_eq!(Observation::new(bad_shape), Err(PolicyError::Dimensions));
    let mut private = valid.clone();
    private.players.push(PlayerObservation {
        team: 1,
        out: false,
        spawn: Point { x: 87.0, z: 87.0 },
    });
    private.units[0].owner = 1;
    private.units[0].path.push(Point { x: 33.0, z: 33.0 });
    assert_eq!(
        Observation::new(private),
        Err(PolicyError::PrivateUnitFields)
    );
    let mut random = MatchRandom::new(93);
    let rng_before = random.snapshot();
    source.aim = Some(Aim {
        i: valid.points.len(),
        until: 20.0,
    });
    let invalid_source = source.clone();
    assert_eq!(
        commander_ai::plan(&observation, &mut source, Level::Normal, &mut random),
        Err(PolicyError::SourceState)
    );
    assert_eq!(source, invalid_source);
    assert_eq!(random.snapshot(), rng_before);
}

#[test]
fn production_randomness_is_confined_to_one_ai_seat() {
    let records = fixture();
    let row = &records["cases"][0];
    let observation = Observation::new(input(row)).unwrap();
    let mut left_state = state(row);
    let mut right_state = left_state.clone();
    let mut left = MatchRandom::new(0x3141_5926);
    let mut right = left.clone();
    for _ in 0..17 {
        right.next_u64(StreamId::Combat);
        right.next_u64(StreamId::Terrain);
        right.next_u64(StreamId::Ai(1));
    }
    let mut expected_left = left.clone();
    let mut expected_right = right.clone();
    let a = commander_ai::plan(&observation, &mut left_state, Level::Normal, &mut left).unwrap();
    let b = commander_ai::plan(&observation, &mut right_state, Level::Normal, &mut right).unwrap();
    assert_eq!(command_bits(&a.commands), command_bits(&b.commands));
    assert_eq!(a.draws, b.draws);
    assert_eq!(left_state, right_state);
    assert!(a.draws > 0);
    for _ in 0..a.draws {
        expected_left.next_fraction(StreamId::Ai(0));
        expected_right.next_fraction(StreamId::Ai(0));
    }
    assert_eq!(left.snapshot(), expected_left.snapshot());
    assert_eq!(right.snapshot(), expected_right.snapshot());
    let mut post_opening = input(row);
    post_opening.tick = 3600;
    let post_opening = Observation::new(post_opening).unwrap();
    let before = (left_state.clone(), left.snapshot());
    assert_eq!(
        commander_ai::plan(&post_opening, &mut left_state, Level::Normal, &mut left),
        Err(PolicyError::NotOpening)
    );
    assert_eq!((left_state, left.snapshot()), before);
}

fn technology(value: &Value) -> ww2_core::economy::Technology {
    use ww2_core::economy::Technology;
    match value.as_str().unwrap() {
        "tier" => Technology::Tier,
        "inf" => Technology::Infantry,
        "guns" => Technology::Guns,
        "armor" => Technology::Armor,
        _ => panic!("unknown source research kind"),
    }
}
fn research_input(case: &Value, frame: &Value) -> Input {
    use ww2_core::{
        commander_ai::ResearchObservation,
        economy::{Faction, TechState},
    };
    let mut delivered = frame["delivered"].clone();
    let slot = integer(&case["slot"]);
    let me = delivered["players"][slot].clone();
    delivered["me"] = me.clone();
    delivered["chars"] = Value::Array(
        array(&delivered["chars"])
            .iter()
            .map(|v| serde_json::json!([v, 1]))
            .collect(),
    );
    delivered["flags"] = Value::Array(
        array(&delivered["flags"])
            .iter()
            .map(|v| serde_json::json!([v, 1]))
            .collect(),
    );
    let row = serde_json::json!({"slot":slot,"frame":delivered});
    let mut input = input(&row);
    let own: Vec<_> = array(&frame["delivered"]["units"])
        .iter()
        .filter(|u| {
            integer(&u["owner"]) == slot
                && ["hq", "barracks", "armory"].contains(&u["type"].as_str().unwrap())
        })
        .collect();
    let ready = |kind: &str| {
        own.iter()
            .filter(|u| u["type"] == kind && number(&u["hp"]) > 0.0 && number(&u["built"]) >= 1.0)
            .count()
    };
    let tech = &me["tech"];
    input.research = Some(ResearchObservation {
        faction: Faction::try_from(integer(&me["faction"]) as u8).unwrap(),
        tech: TechState::new(
            integer(&tech["tier"]) as u8,
            integer(&tech["armory"]["inf"]) as u8,
            integer(&tech["armory"]["guns"]) as u8,
            integer(&tech["armory"]["armor"]) as u8,
        )
        .unwrap(),
        busy: array(&tech["lab"])
            .iter()
            .map(|j| technology(&j[0]))
            .collect(),
        stock_base: own.len() == 2 && ready("hq") == 1 && ready("barracks") == 1,
        armory_ready: ready("armory") != 0,
        standard_army: frame["delivered"]["army"]["pop"].as_f64() == Some(1.0),
    });
    input
}
#[test]
fn original_live_research_then_movement_across_delivered_frames() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/commander-research.json")).unwrap();
    let mut frames = 0;
    for case in array(&fixture["cases"]) {
        let mut state = SourceState::default();
        for frame in array(&case["frames"]) {
            let observation = Observation::new(research_input(case, frame)).unwrap();
            let before = observation.clone();
            let fractions: Vec<_> = array(&frame["random"]).iter().map(number).collect();
            let planned = commander_ai::plan_with_fractions(
                &observation,
                &mut state,
                Level::Normal,
                &fractions,
            )
            .unwrap();
            let attempts = array(&frame["expected"]["attempts"]);
            let expected_tech: Vec<_> = attempts
                .iter()
                .filter(|a| a["command"]["t"] == "tech")
                .map(|a| technology(&a["command"]["kind"]))
                .collect();
            let actual_tech: Vec<_> = planned
                .commands
                .iter()
                .filter_map(|c| match c {
                    Command::Technology { kind } => Some(*kind),
                    _ => None,
                })
                .collect();
            assert_eq!(actual_tech, expected_tech, "{} tech prefix", case["name"]);
            if !actual_tech.is_empty() {
                assert!(matches!(
                    planned.commands.first(),
                    Some(Command::Technology { .. })
                ));
            }
            let movements: Vec<_> = planned
                .commands
                .iter()
                .filter(|c| matches!(c, Command::Move { .. }))
                .cloned()
                .collect();
            let expected: Value = Value::Array(
                attempts
                    .iter()
                    .filter(|a| a["command"]["t"] == "move" || a["command"]["t"] == "amove")
                    .map(|a| a["command"].clone())
                    .collect(),
            );
            assert_eq!(
                command_bits(&movements),
                command_bits(
                    &array(&expected)
                        .iter()
                        .map(|v| Command::from_json(v).unwrap())
                        .collect::<Vec<_>>()
                ),
                "{} movement",
                case["name"]
            );
            assert_eq!(planned.commands.len(), attempts.len());
            assert_eq!(planned.draws, integer(&frame["expected"]["draws"]));
            assert_eq!(
                aim_bits(state.aim),
                aim_bits(aim(&frame["expected"]["mind"]["aim"]))
            );
            assert_eq!(
                observation, before,
                "policy owns no mutable delivered facts"
            );
            frames += 1;
        }
    }
    assert_eq!(frames, 8);
}

#[test]
fn unsupported_research_budget_preserves_state_and_rng() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/commander-research.json")).unwrap();
    let case = &fixture["cases"][0];
    let mut input = research_input(case, &case["frames"][0]);
    input.manpower += 1.0;
    let observation = Observation::new(input).unwrap();
    let mut state = SourceState::default();
    let before = state.clone();
    let mut rng = MatchRandom::new(551);
    let before_rng = rng.clone();
    assert_eq!(
        commander_ai::plan(&observation, &mut state, Level::Normal, &mut rng).unwrap_err(),
        PolicyError::UnsupportedResearch
    );
    assert_eq!(state, before);
    assert_eq!(rng, before_rng);
}

#[test]
fn original_paid_mg_and_refused_purchase_across_delivered_frames() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/commander-shopping.json")).unwrap();
    let mut frames = 0;
    for case in array(&fixture["cases"]) {
        let mut state = SourceState::default();
        for frame in array(&case["frames"]) {
            let observation = Observation::new(research_input(case, frame)).unwrap();
            let fractions: Vec<_> = array(&frame["random"]).iter().map(number).collect();
            let before = observation.clone();
            let plan = commander_ai::plan_with_fractions(
                &observation,
                &mut state,
                Level::Normal,
                &fractions,
            )
            .unwrap();
            let attempts = array(&frame["expected"]["attempts"]);
            let expected: Vec<_> = attempts
                .iter()
                .map(|a| Command::from_json(&a["command"]).unwrap())
                .collect();
            let buys = |commands: &[Command]| {
                commands
                    .iter()
                    .filter_map(|c| match c {
                        Command::Buy { kind, from } => Some((kind.clone(), *from)),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(buys(&plan.commands), buys(&expected));
            if !buys(&expected).is_empty() {
                assert!(matches!(plan.commands.first(), Some(Command::Buy { .. })));
            }
            let moves = |commands: &[Command]| {
                commands
                    .iter()
                    .filter(|c| matches!(c, Command::Move { .. }))
                    .cloned()
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                command_bits(&moves(&plan.commands)),
                command_bits(&moves(&expected))
            );
            assert_eq!(plan.commands.len(), attempts.len());
            assert_eq!(plan.draws, integer(&frame["expected"]["draws"]));
            assert_eq!(
                aim_bits(state.aim),
                aim_bits(aim(&frame["expected"]["mind"]["aim"]))
            );
            assert_eq!(observation, before);
            if case["name"] == "germany-mg-accepted" && frame["tick"] == 3640 {
                let ids: Vec<_> = moves(&plan.commands)
                    .into_iter()
                    .flat_map(|c| match c {
                        Command::Move { orders, .. } => {
                            orders.into_iter().map(|o| o.unit).collect::<Vec<_>>()
                        }
                        _ => unreachable!(),
                    })
                    .collect();
                assert_eq!(
                    ids,
                    vec![11],
                    "the actually purchased source MG is assigned next frame"
                );
            }
            frames += 1;
        }
    }
    assert_eq!(frames, 4);
}
#[test]
fn unexercised_multiple_purchase_budget_retains_private_state() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/commander-shopping.json")).unwrap();
    let case = &fixture["cases"][0];
    let mut input = research_input(case, &case["frames"][0]);
    input.research.as_mut().unwrap().standard_army = false;
    let observation = Observation::new(input).unwrap();
    let mut state = SourceState::default();
    let before = state.clone();
    let mut rng = MatchRandom::new(559);
    let before_rng = rng.clone();
    assert_eq!(
        commander_ai::plan(&observation, &mut state, Level::Normal, &mut rng).unwrap_err(),
        PolicyError::UnsupportedShopping
    );
    assert_eq!(state, before);
    assert_eq!(rng, before_rng);
}

#[test]
fn if25_quiet_research_equal_armory_levels_select_first_authored_line() {
    use ww2_core::economy::Technology;
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/commander-research.json")).unwrap();
    let case = array(&fixture["cases"])
        .iter()
        .find(|c| c["name"] == "infantry-line-without-armory")
        .unwrap();
    let frame = &case["frames"][0];
    let observation = Observation::new(research_input(case, frame)).unwrap();
    let planned = commander_ai::plan_with_fractions(
        &observation,
        &mut SourceState::default(),
        Level::Normal,
        &array(&frame["random"])
            .iter()
            .map(number)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let technology = planned
        .commands
        .iter()
        .filter_map(|c| match c {
            Command::Technology { kind } => Some(*kind),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(technology, [Technology::Infantry]);
}
