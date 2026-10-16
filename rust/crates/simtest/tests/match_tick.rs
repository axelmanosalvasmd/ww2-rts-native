//! Actual reference steps over friendly matches and the connected ordered phases.
use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn make(c: &Value) -> Match {
    make_with_faction(c, Faction::Usa)
}
fn make_with_faction(c: &Value, faction: Faction) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&c["map"]).unwrap()).unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction,
                },
                PlayerConfig {
                    name: "B".into(),
                    team: if c["enemy"] == true { 1 } else { 0 },
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: match c["army"].as_str().unwrap() {
                "standard" => ArmySize::Standard,
                "large" => ArmySize::Large,
                "massive" => ArmySize::Massive,
                "endless" => ArmySize::Endless,
                _ => unreachable!(),
            },
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: true,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
fn snapshot(m: &Match) -> Value {
    let g = &m.world.game;
    json!({"tick":g.tick,"visionTick":g.vision_tick,"seed":m.start.environment_seed,"wind":[bits(m.start.wind.0),bits(m.start.wind.1)],
        "units":g.units.values().map(|u|json!([u.id,bits(u.x),bits(u.z),bits(u.rot),bits(u.hp),bits(u.supp),bits(u.stuck),bits(u.repath),bits(u.sprint),u.path.iter().map(|p|json!([bits(p.x),bits(p.z)])).collect::<Vec<_>>()])).collect::<Vec<_>>(),
        "weapons":g.units.values().map(|u|{let w=m.firing.get(u.id).expect("canonical weapon row");json!([u.id,w.target_id,bits(w.cooldown),bits(w.retarget),bits(m.commands.units[&u.id].aim)])}).collect::<Vec<_>>(),
        "players":m.start.economy.players.iter().map(|p|json!([bits(p.mp),bits(p.vp),bits(p.inc)])).collect::<Vec<_>>(),
        "points":m.start.points.iter().enumerate().map(|(i,p)|json!([p.owner.map_or(-1,|v|v as i64),p.capper.map_or(-1,|v|v as i64),bits(p.progress),p.cut,p.contested,p.on_point,m.capture_ticks[i]])).collect::<Vec<_>>()})
}
fn packed(point: Option<Point>) -> Value {
    point.map_or(Value::Null, |point| json!([bits(point.x), bits(point.z)]))
}
fn source_truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().unwrap() != 0.0,
        _ => true,
    }
}
fn reaction_snapshot(m: &Match) -> Value {
    use ww2_core::match_commands::MoveOutcome;
    json!(
        m.world
            .game
            .units
            .values()
            .map(|unit| {
                let control = &m.commands.units[&unit.id];
                let combat = m.combat.get(unit.id).expect("canonical combat history");
                json!({
                    "id":unit.id,"react":bits(control.react),"drift":unit.drift,
                    "rallyDrift":control.rally_drift,"reverse":packed(unit.reverse),
                    "retreating":unit.retreating,"hitBy":combat.hit_by.unwrap_or(0),
                    "hitAt":bits(combat.hit_at),"hitFrom":packed(unit.hit_from),
                    "shotAt":unit.shot_at.map(bits),"worldGoal":packed(unit.world_goal),
                    "amove":packed(unit.amove),"orders":control.orders.len(),
                    "attack":control.attack_id,"holdPos":unit.hold_pos,"holdFire":control.hold_fire,
                    "autoRetreat":control.auto_retreat,"auto":control.auto_cast,
                    "nade":packed(control.nade),"enter":control.enter.map_or(-1,|cell|cell as i64),
                    "fireAt":control.fire_at.map_or(-1,|cell|cell as i64),"board":unit.board,
                    "build":unit.build,"dig":unit.dig,"entrench":unit.entrench,
                    "moveOutcome":control.outcome.map(|outcome|match outcome {
                        MoveOutcome::Interrupted=>"interrupted",MoveOutcome::Moving=>"moving",
                        MoveOutcome::Unreachable=>"unreachable",MoveOutcome::Arrived=>"completed",
                    }),"moveOutcomeTick":control.outcome_tick
                })
            })
            .collect::<Vec<_>>()
    )
}
#[test]
fn connected_tick_phases_match_actual_reference_steps() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    for c in fixture["cases"].as_array().unwrap() {
        replay_connected_tick_cases(std::slice::from_ref(c), |_, _| {});
    }
}

fn replay_connected_tick_cases(cases: &[Value], mut after_tick: impl FnMut(i64, &[Value])) {
    for c in cases {
        let mut m = make(c);
        let input = &c["input"];
        for unit in m.world.game.units.values_mut() {
            unit.hold_pos = input["holdPosition"].as_bool().unwrap_or(true);
        }
        if let Some(hold_fire) = input["holdFire"].as_bool() {
            for control in m.commands.units.values_mut() {
                control.hold_fire = hold_fire;
            }
        }
        // Match owns the input queue. Advance only its empty boundaries to reach
        // the source's explicit initial clock without running any unit phase.
        for boundary in 1..=input["startTick"].as_u64().unwrap_or(0) {
            assert_eq!(m.commands.pending_commands(), 0);
            assert!(m.apply_commands(boundary, false).unwrap().is_empty());
            m.world.game.tick = boundary as i64;
        }
        if let Some(keep) = input["keep"].as_array() {
            let discarded: Vec<_> = m
                .world
                .game
                .units
                .keys()
                .copied()
                .filter(|id| !keep.iter().any(|v| v.as_u64() == Some(u64::from(*id))))
                .collect();
            for id in discarded {
                m.production.remove_building(id);
                m.building_rallies.remove(&id);
                m.world.game.units.remove(&id);
            }
        }
        if let Some(owners) = input["owners"].as_array() {
            for row in owners {
                m.world
                    .game
                    .units
                    .get_mut(&(row[0].as_u64().unwrap() as u32))
                    .unwrap()
                    .owner = row[1].as_u64().unwrap() as usize;
            }
        }
        if input["disableAuto"] == true {
            for control in m.commands.units.values_mut() {
                control.auto_cast = false;
            }
        }
        if input["disableRetreat"] == true {
            for control in m.commands.units.values_mut() {
                control.auto_retreat = false;
            }
        }
        m.start.environment_seed = input["seed"].as_u64().unwrap() as u32;
        m.start.wind = (
            input["wind"][0].as_f64().unwrap(),
            input["wind"][1].as_f64().unwrap(),
        );
        for row in input["positions"].as_array().unwrap() {
            let u = m
                .world
                .game
                .units
                .get_mut(&(row[0].as_u64().unwrap() as u32))
                .unwrap();
            u.x = row[1].as_f64().unwrap();
            u.z = row[2].as_f64().unwrap();
        }
        if let Some(rows) = input["hp"].as_array() {
            for row in rows {
                m.world
                    .game
                    .units
                    .get_mut(&(row[0].as_u64().unwrap() as u32))
                    .unwrap()
                    .hp = row[1].as_f64().unwrap();
            }
        }
        for row in input["paths"].as_array().unwrap() {
            let u = m
                .world
                .game
                .units
                .get_mut(&(row[0].as_u64().unwrap() as u32))
                .unwrap();
            u.path = vec![Point {
                x: row[1].as_f64().unwrap(),
                z: row[2].as_f64().unwrap(),
            }];
            u.path_gen += 1;
        }
        if let Some(rows) = input["fields"].as_array() {
            for row in rows {
                let id = row[0].as_u64().unwrap() as u32;
                let fields = row[1].as_object().unwrap();
                for (name, value) in fields {
                    match name.as_str() {
                        "holdPos" => {
                            m.world.game.units.get_mut(&id).unwrap().hold_pos =
                                value.as_bool().unwrap()
                        }
                        "holdFire" => {
                            m.commands.units.get_mut(&id).unwrap().hold_fire =
                                value.as_bool().unwrap()
                        }
                        "react" => {
                            m.commands.units.get_mut(&id).unwrap().react = value.as_f64().unwrap()
                        }
                        "drift" => {
                            m.world.game.units.get_mut(&id).unwrap().drift = source_truth(value);
                            m.commands.units.get_mut(&id).unwrap().rally_drift =
                                value.as_str() == Some("rally");
                        }
                        "cooldown" => m.firing.initialize(id).cooldown = value.as_f64().unwrap(),
                        "auto" => {
                            m.commands.units.get_mut(&id).unwrap().auto_cast =
                                value.as_bool().unwrap()
                        }
                        _ => panic!("unmapped actual reaction fixture field {name}"),
                    }
                }
            }
        }
        if let Some(changes) = input["changes"].as_array() {
            for row in changes {
                m.world
                    .set_cell(
                        row[0].as_u64().unwrap() as usize,
                        row[1].as_str().unwrap().as_bytes()[0],
                    )
                    .unwrap();
            }
        }
        if let Some(fires) = input["fires"].as_array() {
            for row in fires {
                m.world
                    .fires
                    .insert(row[0].as_u64().unwrap() as usize, row[1].as_f64().unwrap());
            }
        }
        if let Some(smokes) = input["smokes"].as_array() {
            for cloud in smokes {
                m.world.game.smokes.push(ww2_core::game::Smoke {
                    id: cloud["id"].as_u64().unwrap() as u32,
                    x: cloud["x"].as_f64().unwrap(),
                    z: cloud["z"].as_f64().unwrap(),
                    r: cloud["r"].as_f64().unwrap(),
                    remaining: Some(cloud["t"].as_f64().unwrap()),
                });
            }
        }
        if let Some(commands) = input["commands"].as_array() {
            for row in commands {
                m.enqueue_command(
                    ww2_core::orders::OrderKey {
                        tick: row["tick"].as_u64().unwrap(),
                        issuer: row["issuer"].as_u64().unwrap() as u32,
                        sequence: row["sequence"].as_u64().unwrap(),
                    },
                    ww2_core::match_commands::Command::from_json(&row["command"]).unwrap(),
                )
                .unwrap();
            }
        }
        let mut expected = c["checkpoints"].as_array().unwrap().iter().peekable();
        let mut deaths = Vec::new();
        for _ in 0..c["ticks"].as_u64().unwrap_or(200) {
            let report = m.advance_tick().unwrap();
            for retired in &report.retired {
                deaths.push(json!([
                    report.tick,
                    retired.unit.id,
                    retired.unit.owner,
                    m.rules.kinds[retired.unit.kind].name,
                    retired.last_hit
                ]));
            }
            after_tick(report.tick, &deaths);
            assert!(!report.unavailable.is_empty());
            assert_eq!(
                report
                    .phases
                    .iter()
                    .filter(|phase| **phase == ww2_core::match_tick::TickPhase::TrafficKnowledge)
                    .count(),
                1,
                "recipient traffic is connected exactly once",
            );
            assert!(
                !report
                    .unavailable
                    .contains(&ww2_core::match_tick::TickPhase::TrafficKnowledge)
            );
            assert!(
                report
                    .phases
                    .contains(&ww2_core::match_tick::TickPhase::Reactions)
            );
            assert!(
                !report
                    .unavailable
                    .contains(&ww2_core::match_tick::TickPhase::Reactions)
            );
            assert!(
                report
                    .phases
                    .contains(&ww2_core::match_tick::TickPhase::Firing)
            );
            assert!(
                report
                    .phases
                    .contains(&ww2_core::match_tick::TickPhase::Projectiles)
            );
            if expected
                .peek()
                .is_some_and(|e| e["tick"].as_i64() == Some(report.tick))
            {
                let mut state = snapshot(&m);
                if c["enemy"] == true {
                    // A stream is absent until its first real draw. Advance only a
                    // clone, then undo SplitMix64's state increment to observe the
                    // exact current state even on a pre-contact checkpoint.
                    let mut observed = m.start.random.clone();
                    observed.next_u64(ww2_core::random::StreamId::Combat);
                    let value = observed
                        .snapshot()
                        .streams
                        .iter()
                        .find(|(stream, _)| *stream == ww2_core::random::StreamId::Combat)
                        .unwrap()
                        .1
                        .wrapping_sub(0x9e37_79b9_7f4a_7c15);
                    state["combatState"] = json!(format!("{value:016x}"));
                }
                if expected
                    .peek()
                    .is_some_and(|row| row.get("deaths").is_some())
                {
                    state["deaths"] = json!(deaths);
                }
                if expected
                    .peek()
                    .is_some_and(|row| row.get("story").is_some())
                {
                    state["story"] = json!(
                        m.story
                            .iter()
                            .map(|row| ww2_core::match_story::StoryKey::ALL
                                .into_iter()
                                .map(|key| bits(row.value(key)))
                                .collect::<Vec<_>>())
                            .collect::<Vec<_>>()
                    );
                }
                if c["abilityRecovery"] == true {
                    state["abilityRecovery"] = json!(
                        m.world
                            .game
                            .units
                            .values()
                            .map(|unit| {
                                let ability = m.abilities.get(unit.id).unwrap();
                                let recovery = m.recovery.get(unit.id).unwrap();
                                let control = &m.commands.units[&unit.id];
                                json!([
                                    unit.id,
                                    bits(ability.cooldown),
                                    bits(ability.buff),
                                    ability.ap,
                                    control.nade.map(|at| [bits(at.x), bits(at.z)]),
                                    ability.aimed_auto,
                                    ability.aimed_walked,
                                    recovery.hp_was.map(bits),
                                    recovery.wound_at,
                                    recovery.heal_at,
                                    bits(recovery.reinforcement)
                                ])
                            })
                            .collect::<Vec<_>>()
                    );
                    state["grenades"] = json!(
                        m.firing
                            .grenades
                            .iter()
                            .map(|ticket| {
                                json!([
                                    ticket.flight,
                                    bits(ticket.at.x),
                                    bits(ticket.at.z),
                                    bits(ticket.remaining),
                                    ticket.owner
                                ])
                            })
                            .collect::<Vec<_>>()
                    );
                }
                if c["field"] == true {
                    let g = &m.world.game;
                    state["field"] = json!({
                        "jobs":m.field_works.jobs().iter().map(|(id,job)|json!([
                            id,
                            job.kind.as_str(),
                            bits(job.at.x),
                            bits(job.at.z),
                            bits(job.direction),
                            bits(job.elapsed),
                            job.original_cells,
                            job.cells.iter().map(|cell|json!([cell.cell,char::from(cell.ch).to_string()])).collect::<Vec<_>>(),
                            job.front.map(bits),
                            job.reach.map(bits),
                            job.from.map(|at|[bits(at.x),bits(at.z)])
                        ])).collect::<Vec<_>>(),
                        "chars":String::from_utf8(g.chars.clone()).unwrap(),
                        "front":g.chars.iter().enumerate().filter(|(_,ch)|**ch==b'T').map(|(cell,_)|json!([
                            cell,
                            bits(f64::from(g.trench_front.as_ref().unwrap()[cell]))
                        ])).collect::<Vec<_>>()
                    });
                }
                if expected
                    .peek()
                    .is_some_and(|row| row.get("control").is_some())
                {
                    use ww2_core::match_commands::MoveOutcome;
                    state["control"] = json!(
                        m.world
                            .game
                            .units
                            .values()
                            .map(|u| {
                                let c = &m.commands.units[&u.id];
                                json!([
                                    u.id,
                                    c.outcome.map(|o| match o {
                                        MoveOutcome::Interrupted => "interrupted",
                                        MoveOutcome::Moving => "moving",
                                        MoveOutcome::Unreachable => "unreachable",
                                        MoveOutcome::Arrived => "completed",
                                    }),
                                    c.outcome_tick,
                                    c.orders.len(),
                                    u.world_goal.map(|p| [bits(p.x), bits(p.z)]),
                                    u.amove.map(|p| [bits(p.x), bits(p.z)])
                                ])
                            })
                            .collect::<Vec<_>>()
                    );
                }
                if input["fires"].is_array() {
                    state["environment"] = json!({
                        "fires":m.world.fires.iter().map(|(c,left)|json!([c,bits(*left)])).collect::<Vec<_>>(),
                        "smokes":m.world.game.smokes.iter().map(|s|json!([s.id,bits(s.x),bits(s.z),bits(s.r),bits(s.remaining.unwrap())])).collect::<Vec<_>>(),
                        "chars":String::from_utf8(m.world.game.chars.clone()).unwrap(),"burnt":m.world.burnt
                    });
                }
                let mut expected_state = expected.next().unwrap().clone();
                if c["reaction"] == true {
                    state["reactions"] = reaction_snapshot(&m);
                    for row in expected_state["reactions"].as_array_mut().unwrap() {
                        // The component oracle applies source truthiness to drift.
                        // Keep rally intent separate so an intercepted rally is checked.
                        row["rallyDrift"] = json!(row["drift"] == "rally");
                        row["drift"] = json!(source_truth(&row["drift"]));
                    }
                }
                assert_eq!(
                    state,
                    expected_state,
                    "case {} army {} tick {}",
                    c.get("name").unwrap_or(&Value::Null),
                    c["army"],
                    report.tick
                );
            }
        }
        assert!(expected.next().is_none());
    }
}
// The full final Source trace specifies the prefix due on every native tick.
// Neither this comparison nor its controls can initialize or mutate Match.
fn death_trace_matches_tick(actual: &[Value], expected: &[Value], tick: i64) -> bool {
    actual.iter().eq(expected
        .iter()
        .filter(|death| death[0].as_i64().unwrap() <= tick))
}

#[test]
fn cmb69_seeded_retirement_rejects_wrong_tick_identity_and_missing_or_extra_deaths() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    let selected: Vec<_> = cases
        .iter()
        .filter(|case| case["retirement"] == true && case["ticks"] == 800)
        .collect();
    assert_eq!(selected.len(), 1, "one authentic long retirement scene");
    let case = selected[0];
    assert!(case["input"].get("hp").is_none(), "no health priming");
    assert_eq!(case["input"]["seed"], 0x1234_5678);
    let checkpoints = case["checkpoints"].as_array().unwrap();
    assert_eq!(checkpoints.len(), 15);
    let expected = checkpoints.last().unwrap()["deaths"].as_array().unwrap();
    assert_eq!(expected.as_slice(), &[json!([483, 2, 1, "rifle", 0])]);

    let mut early = expected.clone();
    early[0][0] = json!(482);
    let mut late = expected.clone();
    late[0][0] = json!(484);
    let mut wrong_id = expected.clone();
    wrong_id[0][1] = json!(1);
    let mut wrong_owner = expected.clone();
    wrong_owner[0][2] = json!(0);
    let mut wrong_kind = expected.clone();
    wrong_kind[0][3] = json!("mg");
    let mut wrong_hitter = expected.clone();
    wrong_hitter[0][4] = json!(1);
    let mut duplicate = expected.clone();
    duplicate.push(expected[0].clone());
    let controls = [
        ("early death", early, 482),
        ("late death", late, 483),
        ("wrong unit ID", wrong_id, 483),
        ("wrong owner", wrong_owner, 483),
        ("wrong kind", wrong_kind, 483),
        ("wrong last hitter", wrong_hitter, 483),
        ("missing death", Vec::new(), 483),
        ("duplicate death", duplicate, 483),
    ];
    let mut first_rejection = vec![None; controls.len()];
    let mut ticks = 0;
    replay_connected_tick_cases(std::slice::from_ref(case), |tick, deaths| {
        ticks += 1;
        assert_eq!(tick, ticks, "every canonical tick is observed");
        assert!(
            death_trace_matches_tick(deaths, expected, tick),
            "authentic death trace at tick {tick}"
        );
        for (index, (_, changed, _)) in controls.iter().enumerate() {
            if !death_trace_matches_tick(deaths, changed, tick) {
                first_rejection[index].get_or_insert(tick);
            }
        }
    });
    assert_eq!(ticks, 800);
    for ((name, _, tick), rejected) in controls.iter().zip(first_rejection) {
        assert_eq!(rejected, Some(*tick), "decisive control: {name}");
    }
}

#[test]
fn rejected_clock_does_not_consume_a_tick() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    let mut m = make(&fixture["cases"][0]);
    m.world.game.tick = i64::MAX;
    let before = snapshot(&m);
    assert!(m.advance_tick().is_err());
    assert_eq!(snapshot(&m), before);
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn trusted_tick_scene_stages_clock_paths_and_empty_queue_without_replay() {
    use ww2_core::{match_commands::Command, match_tick::TickPathConfig, orders::OrderKey};
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    let mut m = make(&fixture["cases"][0]);
    assert_eq!(m.tick_state.path_config(), TickPathConfig::default());
    let random = m.start.random.snapshot();
    let wallet = m.start.economy.clone();
    m.initialize_tick_scene(
        18,
        TickPathConfig {
            budget: 0,
            retry_limit: 1,
        },
    )
    .unwrap();
    assert_eq!(m.world.game.tick, 18);
    assert_eq!(m.commands.next_tick(), Some(19));
    assert_eq!(m.commands.pending_commands(), 0);
    assert_eq!(m.start.random.snapshot(), random);
    assert_eq!(m.start.economy, wallet);
    assert_eq!(m.tick_state.path_config().budget, 0);
    let clock = m.world.game.tick;
    assert!(
        m.initialize_tick_scene(0, TickPathConfig::default())
            .is_err()
    );
    assert_eq!(m.world.game.tick, clock);
    let report = m.advance_tick().unwrap();
    assert_eq!(report.tick, 19);
    assert_eq!(m.commands.next_tick(), Some(20));
    assert!(
        m.initialize_tick_scene(0, TickPathConfig::default())
            .is_err()
    );

    let mut pending = make(&fixture["cases"][0]);
    pending
        .enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 1,
            },
            Command::Stop { ids: vec![1] },
        )
        .unwrap();
    let before = snapshot(&pending);
    assert!(
        pending
            .initialize_tick_scene(
                18,
                TickPathConfig {
                    budget: 4096,
                    retry_limit: 1
                }
            )
            .is_err()
    );
    assert_eq!(snapshot(&pending), before);
    assert_eq!(pending.commands.pending_commands(), 1);
    assert_eq!(pending.commands.next_tick(), Some(1));
    assert_eq!(pending.tick_state.path_config(), TickPathConfig::default());
    assert!(pending.tick_state.fault().is_none());

    let mut invalid = make(&fixture["cases"][0]);
    let before = snapshot(&invalid);
    assert!(
        invalid
            .initialize_tick_scene(u64::MAX, TickPathConfig::default())
            .is_err()
    );
    assert_eq!(snapshot(&invalid), before);
    assert_eq!(invalid.commands.next_tick(), Some(1));
    assert_eq!(invalid.tick_state.path_config(), TickPathConfig::default());
}

#[test]
fn unsupported_reaction_actors_reject_before_clock_commands_or_timers() {
    use ww2_core::{match_commands::Command, orders::OrderKey};
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    for actor in [
        "rail",
        "riding",
        "reverse riding",
        "idle suppression",
        "logistics",
        "convoys",
        "mode mismatch",
        "unsupported mode",
    ] {
        let mut m = make(&fixture["cases"][0]);
        let id = *m.world.game.units.keys().next().unwrap();
        let rail_kind = m.rules.kinds.iter().position(|kind| kind.rail).unwrap();
        let unit = m.world.game.units.get_mut(&id).unwrap();
        unit.x = 25.0;
        unit.z = 25.0;
        unit.repath = 0.5;
        unit.sprint = 0.25;
        match actor {
            "rail" => unit.kind = rail_kind,
            "riding" => {
                unit.riding = true;
                m.combat.get_mut(id).unwrap().carrier = Some(900);
            }
            "reverse riding" => m.combat.get_mut(id).unwrap().carrier = Some(900),
            "idle suppression" => unit.supp = -1.0,
            "logistics" => m.options.logistics = true,
            "convoys" => m.convoys = ww2_core::supply_convoys::ConvoyState::setup(true),
            // World is represented only with matching canonical options and runtime.
            "mode mismatch" => {
                assert_eq!(m.options.mode, Mode::Conquest);
                m.world.game.mode = Some(ww2_core::game::ModeKind::World);
            }
            "unsupported mode" => {
                m.options.mode = Mode::Classic;
                m.world.game.mode = Some(ww2_core::game::ModeKind::Classic);
            }
            _ => unreachable!(),
        }
        let control = m.commands.units.get_mut(&id).unwrap();
        control.react = 0.75;
        control.auto_retreat = true;
        m.firing.initialize(id).cooldown = 1.0;
        m.firing.initialize(id).retarget = 0.3;
        m.abilities.initialize(id).cooldown = 2.0;
        m.abilities.initialize(id).buff = 1.0;
        m.enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 1,
            },
            Command::Stop {
                ids: vec![i64::from(id)],
            },
        )
        .unwrap();
        m.enqueue_command(
            OrderKey {
                tick: 2,
                issuer: 0,
                sequence: 2,
            },
            Command::Stop {
                ids: vec![i64::from(id)],
            },
        )
        .unwrap();
        let before = snapshot(&m);
        let units = m.world.game.units.clone();
        let controls = m.commands.units.clone();
        let firing = m.firing.clone();
        let abilities = m.abilities.clone();
        let recovery = m.recovery.clone();
        let combat = m.combat.clone();
        let support = m
            .start
            .players
            .iter()
            .map(|player| player.support.clone())
            .collect::<Vec<_>>();
        let error = m.advance_tick().unwrap_err();
        assert!(
            error.reason.contains(match actor {
                "logistics" | "convoys" => "reserve and convoy tick phases",
                "mode mismatch" | "unsupported mode" => {
                    "tick execution requires represented ordinary or World mode"
                }
                "riding" => "thrown carrier ejection tick phase",
                "reverse riding" | "idle suppression" => "invalid canonical carrier unit",
                _ => "rail tick actor",
            }),
            "{actor}: {error}"
        );
        assert_eq!(error.tick, m.world.game.tick, "{actor}");
        assert!(error.events.is_empty(), "{actor}");
        assert_eq!(error.retirement, Default::default(), "{actor}");
        assert_eq!(snapshot(&m), before, "{actor}");
        // The absent pace sentinel is NaN, so PartialEq cannot prove equality.
        // Check its bits separately, then compare every remaining Unit field.
        let mut after_units = m.world.game.units.clone();
        let mut before_units = units;
        for (&id, unit) in &mut after_units {
            let before = before_units.get_mut(&id).expect("same admitted unit set");
            assert_eq!(unit.pace.to_bits(), before.pace.to_bits(), "{actor} pace");
            unit.pace = 0.0;
            before.pace = 0.0;
        }
        assert_eq!(after_units, before_units, "{actor}");
        assert_eq!(m.commands.units, controls, "{actor}");
        assert_eq!(m.firing, firing, "{actor}");
        assert_eq!(m.abilities, abilities, "{actor}");
        assert_eq!(m.recovery, recovery, "{actor}");
        assert_eq!(m.combat, combat, "{actor}");
        assert_eq!(
            m.start
                .players
                .iter()
                .map(|player| player.support.clone())
                .collect::<Vec<_>>(),
            support,
            "{actor}"
        );
        assert_eq!(m.commands.next_tick(), Some(1), "{actor}");
        assert_eq!(m.commands.pending_commands(), 2, "{actor}");
        assert!(m.tick_state.fault().is_none(), "{actor}");
    }
}

#[test]
fn queued_companion_join_enters_prior_and_skips_ordinary_timers() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    let mut m = make(&fixture["cases"][0]);
    let count = m.world.game.units.len();
    m.enqueue_command(
        ww2_core::orders::OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        },
        ww2_core::match_commands::Command::CompanionJoin {
            identity: ww2_core::recorded_input::CompanionJournalId::new(1).unwrap(),
        },
    )
    .unwrap();
    let report = m.advance_tick().unwrap();
    assert_eq!(m.world.game.tick, 1);
    assert_eq!(m.world.game.units.len(), count + 1);
    let id = *m
        .commands
        .units
        .iter()
        .find(|(_, control)| control.operative)
        .unwrap()
        .0;
    assert!(report.prior.contains_key(&id));
    assert!(
        report
            .phases
            .contains(&ww2_core::match_tick::TickPhase::Operative)
    );
    assert!(
        !report
            .unavailable
            .contains(&ww2_core::match_tick::TickPhase::Operative)
    );
    assert_eq!(m.world.game.units[&id].repath, 0.0);
    assert_eq!(m.world.game.units[&id].sprint, 0.0);
    assert_eq!(m.firing.get(id).unwrap().retarget, 0.0);
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn malformed_support_metadata_preserves_whole_recorded_boundary() {
    use ww2_core::{match_commands::Command, match_support::FighterCover, orders::OrderKey};
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    for malformed in ["cooldown", "player team", "cover team"] {
        let mut m = make(&fixture["cases"][0]);
        match malformed {
            "cooldown" => {
                m.start.players[0].support.remove("artillery");
            }
            "player team" => m.start.players[0].config.team += 1,
            "cover team" => m.support.covers.push(FighterCover {
                owner: 0,
                team: m.world.game.players[0].team + 1,
                at: Point { x: 8.0, z: 8.0 },
                radius: 40.0,
                remaining: 60.0,
            }),
            _ => unreachable!(),
        }
        let id = *m.world.game.units.keys().next().unwrap();
        for (sequence, command) in [
            (
                1,
                Command::Stop {
                    ids: vec![i64::from(id)],
                },
            ),
            (
                2,
                Command::Support {
                    kind: "artillery".into(),
                    at: Point { x: 8.0, z: 8.0 },
                    direction: None,
                },
            ),
        ] {
            m.enqueue_command(
                OrderKey {
                    tick: 1,
                    issuer: 0,
                    sequence,
                },
                command,
            )
            .unwrap();
        }
        let state = |m: &Match| {
            format!(
                "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
                m.world,
                m.commands.units,
                m.start.players,
                m.start.economy,
                m.support,
                m.start.random.snapshot(),
                m.firing,
                m.abilities
            )
        };
        let before = state(&m);
        assert!(m.advance_tick().is_err(), "{malformed}");
        assert_eq!(state(&m), before, "{malformed}");
        assert_eq!(m.commands.next_tick(), Some(1));
        assert_eq!(m.commands.pending_commands(), 2);
        assert!(m.tick_state.fault().is_none());
    }
}

#[test]
fn offmap_support_boundary_dispatches_once_and_decays_timers_once() {
    use ww2_core::{catalog::Catalog, match_commands::Command, orders::OrderKey};
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    for boundary in [1, 2] {
        let mut m = make(&fixture["cases"][0]);
        m.start.economy.players[0].mp = 1000.0;
        let first = *m.world.game.units.keys().next().unwrap();
        let before_id = m.next_unit_id;
        m.enqueue_command(
            OrderKey {
                tick: boundary,
                issuer: 0,
                sequence: 1,
            },
            Command::Stop {
                ids: vec![i64::from(first)],
            },
        )
        .unwrap();
        m.enqueue_command(
            OrderKey {
                tick: boundary,
                issuer: 0,
                sequence: 2,
            },
            Command::Support {
                kind: "bombing".into(),
                at: Point { x: 8.0, z: 8.0 },
                direction: None,
            },
        )
        .unwrap();
        let dt = m.rules.movement.tick;
        let initial = m.start.players[0].support.clone();
        if boundary == 2 {
            let report = m.advance_tick().unwrap();
            assert!(report.commands.is_empty());
            assert_eq!(m.commands.pending_commands(), 2);
            for (kind, value) in &initial {
                assert_eq!(m.start.players[0].support[kind], value - dt);
            }
            assert!(m.world.game.strikes.is_empty());
        }
        let before = m.start.players[0].support.clone();
        let wallet = m.start.economy.players[0].mp;
        let spent = m.story[0].mp_spent;
        let calls = m.story[0].support_calls;
        let report = m.advance_tick().unwrap();
        assert_eq!(report.commands.len(), 2);
        assert!(report.commands.iter().all(|result| result.denial.is_none()));
        assert_eq!(m.commands.pending_commands(), 0);
        assert_eq!(m.commands.next_tick(), Some(boundary + 1));
        assert_eq!(m.world.game.tick, boundary as i64);
        assert_eq!(m.next_unit_id, before_id);
        assert_eq!(m.world.game.strikes.len(), 1);
        let strike = &m.world.game.strikes[0];
        let timing = strike.runtime.as_ref().unwrap();
        assert_eq!(timing.kind, ww2_core::match_support::SupportKind::Bombing);
        let def = Catalog::compiled().unwrap().support("bombing").unwrap();
        let field = |key| def.get(key).unwrap().number().unwrap();
        assert_eq!(timing.delay, field("delay") - dt);
        assert!(!strike.live);
        assert_eq!(
            m.start.economy.players[0].mp,
            (wallet - field("cost")) + m.start.economy.players[0].inc * dt
        );
        assert_eq!(m.story[0].mp_spent, spent + field("cost"));
        assert_eq!(m.story[0].support_calls, calls + 1.0);
        assert_eq!(m.start.players[0].support["bombing"], field("cd") - dt);
        for (kind, value) in before.iter().filter(|(kind, _)| kind.as_str() != "bombing") {
            assert_eq!(m.start.players[0].support[kind], value - dt);
        }
        for phase in [
            ww2_core::match_tick::TickPhase::Support,
            ww2_core::match_tick::TickPhase::AntiAir,
            ww2_core::match_tick::TickPhase::SupportTimers,
        ] {
            assert_eq!(
                report
                    .phases
                    .iter()
                    .filter(|actual| **actual == phase)
                    .count(),
                1
            );
        }
        assert!(m.tick_state.fault().is_none());
    }
}

#[test]
fn late_terrain_change_does_not_recompute_the_earlier_supply_beat() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    let mut c = fixture["ordering"].clone();
    c["enemy"] = json!(true);
    let mut m = make(&c);
    m.start.points[0].owner = Some(0);
    m.start.points[0].capper = Some(0);
    m.start.points[0].progress = 1.0;
    for step in c["steps"].as_array().unwrap() {
        m.world.game.tick = step["tick"].as_i64().unwrap();
        let ids: Vec<_> = m.world.game.units.keys().copied().collect();
        m.apply_point_supply(&ids).unwrap();
        for change in step["changes"].as_array().unwrap() {
            m.world
                .set_cell(
                    change[0].as_u64().unwrap() as usize,
                    change[1].as_str().unwrap().as_bytes()[0],
                )
                .unwrap();
        }
        m.apply_point_capture_income(&ids).unwrap();
        let state = snapshot(&m);
        assert_eq!(
            json!({"players":state["players"],"points":state["points"]}),
            step["expected"]
        );
    }
    assert!(m.start.points[0].cut);
}

#[test]
fn cmb41_four_actual_conscript_recruits_add_three_population() {
    use std::collections::BTreeSet;
    use ww2_core::economy_state::{Context, ModeContext, Unit};
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    let mut m = make_with_faction(&fixture["cases"][0], Faction::Ussr);
    let original = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
    assert!(m.world.game.strikes.is_empty());
    m.start.economy.players[0].mp = 1000.0;
    let population = |m: &Match| {
        let queues = m
            .world
            .game
            .units
            .values()
            .map(|unit| {
                m.production
                    .building(unit.id)
                    .map_or_else(Vec::new, |building| building.queue().collect::<Vec<_>>())
            })
            .collect::<Vec<_>>();
        let units = m
            .world
            .game
            .units
            .values()
            .zip(&queues)
            .map(|(unit, queue)| Unit {
                id: u64::from(unit.id),
                owner: Some(unit.owner),
                kind: &m.rules.kinds[unit.kind].name,
                hp: unit.hp,
                built: unit.built.unwrap_or(1.0),
                queue,
            })
            .collect::<Vec<_>>();
        m.start
            .economy
            .population(
                &Context {
                    mode: ModeContext {
                        kind: m.options.mode,
                        ..Default::default()
                    },
                    army: m.options.army,
                    units: &units,
                    drops: &[],
                    points: &[],
                    nodes: &[],
                    world: None,
                },
                0,
            )
            .unwrap()
    };
    let before = population(&m);
    let wallet = m.start.economy.players[0].mp;
    let price = ww2_core::economy::price_of(Mode::Conquest, "conscript").unwrap();
    let mut bought = BTreeSet::new();
    for count in 1..=4 {
        let born = m.recorded_buy(0, "conscript", None).unwrap();
        assert!(bought.insert(born.unit));
        assert!(!original.contains(&born.unit));
        let actor = &m.world.game.units[&born.unit];
        assert_eq!(m.rules.kinds[actor.kind].name, "conscript");
        assert_eq!(actor.owner, 0);
        assert!(m.recruit_metadata.contains_key(&born.unit));
        assert_eq!(population(&m) - before, f64::from(count) * 0.75);
    }
    assert_eq!(bought.len(), 4);
    assert_eq!(population(&m) - before, 3.0);
    assert_eq!(m.start.economy.players[0].mp, wallet - 4.0 * price.mp);
    assert!(
        original
            .iter()
            .all(|id| m.world.game.units.contains_key(id))
    );
}
