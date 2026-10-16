//! Actual original full-step observations through native quiet policy and human input phases.
use super::*;
use crate::human_commander_test_facts as facts;
use crate::{
    commander_ai::{Input, PlayerObservation, PointObservation, UnitObservation},
    random::StreamId,
};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../tests/fixtures/human-commander-policy.json"
    ))
    .unwrap()
}

fn packet(frame: &Value, owner: usize) -> Observation {
    Observation::new(Input {
        width: frame["w"].as_u64().unwrap() as usize,
        height: frame["h"].as_u64().unwrap() as usize,
        tick: frame["tick"].as_i64().unwrap(),
        slot: owner,
        players: frame["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| PlayerObservation {
                team: p["team"].as_i64().unwrap() as i32,
                out: facts::truthy(&p["out"]),
                spawn: facts::point(&p["spawn"]),
            })
            .collect(),
        units: frame["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| UnitObservation {
                id: u["id"].as_u64().unwrap() as u32,
                kind: u["type"].as_str().unwrap().into(),
                owner: u["owner"].as_u64().unwrap() as usize,
                at: facts::point(u),
                hp: u["hp"].as_f64().unwrap(),
                supp: u["supp"].as_f64().unwrap(),
                target_id: u["targetId"].as_u64().unwrap() as u32,
                cover: u["cover"].as_u64().unwrap() as u8,
                path: u["path"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(facts::point)
                    .collect(),
                attack_id: facts::truthy(&u["attackId"]),
                retreating: facts::truthy(&u["retreating"]),
                riding: u["flags"].as_u64().unwrap() & 262_144 != 0,
                build: facts::truthy(&u["build"]),
                dig: facts::truthy(&u["dig"]),
                entrench: facts::truthy(&u["entrench"]),
                garrison: u["garrison"].as_i64().is_some_and(|n| n >= 0),
                enter: u["enter"].as_i64().is_some_and(|n| n >= 0),
                fire_at: u["fireAt"].as_i64().is_some_and(|n| n >= 0),
                nade: facts::truthy(&u["nade"]),
            })
            .collect(),
        points: frame["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| PointObservation {
                at: facts::point(p),
                vp: p["vp"].as_f64().unwrap(),
                mp: p["mp"].as_f64().unwrap(),
                owner: p["owner"].as_u64().map(|n| n as usize),
                cut: facts::truthy(&p["cut"]),
                locked: facts::truthy(&p["locked"]),
            })
            .collect(),
        chars: frame["chars"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| {
                std::iter::repeat_n(
                    r[0].as_str().unwrap().as_bytes()[0],
                    r[1].as_u64().unwrap() as usize,
                )
            })
            .collect(),
        flags: frame["flags"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| {
                std::iter::repeat_n(
                    r[0].as_u64().unwrap() as u16,
                    r[1].as_u64().unwrap() as usize,
                )
            })
            .collect(),
        manpower: frame["me"]["mp"].as_f64().unwrap(),
        research: None,
        runtime: None,
    })
    .unwrap()
}

fn delivered(record: &Value, owner: usize) -> HumanView<Observation> {
    let facts = facts::view(&record["viewSnapshot"]);
    HumanView {
        tick: facts.tick,
        spawn: facts.spawn,
        units: facts.units,
        visible: facts.visible,
        sightings: facts.sightings,
        packet: packet(&record["packetFrame"], owner),
    }
}

fn control(row: &Value) -> TickControl<'_> {
    let c = &row["control"];
    TickControl {
        now: c["now"].as_u64().unwrap(),
        live_owner_out: c["live_owner_out"].as_bool().unwrap(),
        winner: c["winner"].as_bool().unwrap(),
        level: c["level"].as_str().unwrap(),
    }
}

fn level(name: &str) -> Level {
    match name {
        "easy" => Level::Easy,
        "hard" => Level::Hard,
        _ => Level::Normal,
    }
}

#[test]
fn original_quiet_policy_runs_on_human_demand_before_delayed_input_attempts() {
    let fixture = fixture();
    let mut calls = 0;
    let mut physical = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let owner = case["slot"].as_u64().unwrap() as u32;
        let mut human = HumanState::new(
            case["initializationSeed"].as_i64().unwrap() as u32,
            owner,
            facts::point(&case["sourceSpawn"]),
        );
        let mut source = SourceState::default();
        let mut latest = delivered(&case["deliveries"][0], owner as usize);
        let mut inputs = Vec::new();
        for row in case["batches"].as_array().unwrap() {
            let now = row["tick"].as_u64().unwrap();
            if let Some(view) = case["deliveries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["tick"].as_u64() == Some(now))
            {
                latest = delivered(view, owner as usize);
            }
            let result = human
                .advance(control(row), Some(&latest), &facts::batch(row), |screen| {
                    let expected = case["policyCalls"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|p| p["tick"].as_u64() == Some(now))
                        .unwrap();
                    assert!(!commander_ai::due(
                        now as i64,
                        owner as usize,
                        level(case["level"].as_str().unwrap())
                    ));
                    let observation = human_screen_observation(&screen).map_err(Error::Planner)?;
                    assert_eq!(observation, packet(&expected["frame"], owner as usize));
                    let fractions = expected["fractions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|n| n.as_f64().unwrap())
                        .collect::<Vec<_>>();
                    let plan = commander_ai::plan_with_fractions(
                        &observation,
                        &mut source,
                        level(case["level"].as_str().unwrap()),
                        &fractions,
                    )
                    .unwrap();
                    assert_eq!(plan.draws, fractions.len());
                    let raw = plan
                        .commands
                        .iter()
                        .map(raw_command)
                        .collect::<Result<Vec<_>, _>>()?;
                    let attempts = expected["attempts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|a| {
                            assert_eq!(a["returned"]["kind"], "undefined");
                            a["command"].clone()
                        })
                        .collect::<Vec<_>>();
                    assert!(facts::exact(&json!(raw), &json!(attempts)));
                    calls += 1;
                    Ok(raw)
                })
                .unwrap();
            assert!(result.new_alerts.is_empty());
            if let Some(input) = result.input {
                inputs.push(facts::input_receipt(&input));
                physical += 1;
            }
        }
        let expected = &case["policyCalls"][0]["memoryAfter"]["aim"];
        let actual = source.aim.unwrap();
        assert_eq!(actual.i, expected["i"].as_u64().unwrap() as usize);
        assert_eq!(
            actual.until.to_bits(),
            expected["until"].as_f64().unwrap().to_bits()
        );
        let mut expected = case["inputs"].clone();
        for row in expected.as_array_mut().unwrap() {
            row.as_object_mut().unwrap().remove("accepted");
        }
        assert!(facts::exact(&json!(inputs), &expected));
    }
    assert_eq!((calls, physical), (3, 6));
}

#[test]
fn actual_policy_purpose_stream_is_replayable_and_failed_projection_publishes_neither_provider() {
    let fixture = fixture();
    let case = &fixture["cases"][1];
    let owner = case["slot"].as_u64().unwrap() as u32;
    let seed = fixture["seed"].as_u64().unwrap();
    let mut a = HumanState::new(
        case["initializationSeed"].as_i64().unwrap() as u32,
        owner,
        facts::point(&case["sourceSpawn"]),
    );
    let mut b = a.clone();
    let mut pa = HumanPolicy::new(owner, seed);
    let mut pb = pa.clone();
    let initial_rng = pa.random.clone();
    let mut latest = delivered(&case["deliveries"][0], owner as usize);
    let plan_tick = case["policyCalls"][0]["tick"].as_u64().unwrap();
    for row in case["batches"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["tick"].as_u64().unwrap() <= plan_tick)
    {
        let now = row["tick"].as_u64().unwrap();
        if let Some(view) = case["deliveries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["tick"].as_u64() == Some(now))
        {
            latest = delivered(view, owner as usize);
        }
        if now == plan_tick {
            let mut forged = latest.clone();
            forged.units[0].hp -= 1.0;
            let before = (a.clone(), pa.clone());
            let (mut rejected_state, mut rejected_policy) = before.clone();
            assert!(
                advance(
                    &mut rejected_state,
                    &mut rejected_policy,
                    control(row),
                    Some(&forged),
                    &facts::batch(row)
                )
                .is_err()
            );
            assert_eq!((rejected_state, rejected_policy), before);
        }
        assert_eq!(
            advance(
                &mut a,
                &mut pa,
                control(row),
                Some(&latest),
                &facts::batch(row)
            )
            .unwrap(),
            advance(
                &mut b,
                &mut pb,
                control(row),
                Some(&latest),
                &facts::batch(row)
            )
            .unwrap()
        );
        assert_eq!(a, b);
        assert_eq!(pa, pb);
    }
    let mut expected = initial_rng;
    for _ in 0..3 {
        expected.next_fraction(StreamId::Ai(owner));
    }
    assert_eq!(pa.random, expected);
    assert!(pa.source.aim.is_some());
    assert_eq!(pa.random.snapshot().streams.len(), 1);
}

/// The fixture is pending until Root grants the original Source recorder a slot.
/// Do not replace its complete original check with hand-authored expected orders.
#[test]
fn original_cut_reconnection_check_and_human_inputs_match() {
    use crate::human_commander_inputs::{self as inputs, Cursor, InputKind, Unit, View};

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ai-cut-reconnection.json");
    let bytes = std::fs::read_to_string(path)
        .expect("Root must run harness/ai-cut-reconnection-fixture.mjs in a granted Source slot");
    let fixture: Value = serde_json::from_str(&bytes).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    for (case, name) in cases.iter().zip(["easy", "normal", "hard"]) {
        assert_eq!(case["level"], name);
        let owner = case["slot"].as_u64().unwrap() as usize;
        let observation = packet(&case["frame"], owner);
        assert_eq!(observation.input().points.len(), 1);
        assert_eq!(observation.input().points[0].owner, Some(owner));
        assert!(observation.input().points[0].cut);
        assert_eq!(observation.input().tick, 0);
        assert_eq!(observation.input().units.len(), 2);
        assert_eq!(case["memoryBefore"], json!({}));
        let fractions = case["fractions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect::<Vec<_>>();
        let mut memory = SourceState::default();
        let plan =
            commander_ai::plan_with_fractions(&observation, &mut memory, level(name), &fractions)
                .unwrap();
        assert_eq!(plan.draws, fractions.len());
        let raw = plan
            .commands
            .iter()
            .map(raw_command)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(facts::exact(&json!(raw), &case["commands"]), "{name}");
        // Preserve the original check's all-squad membership predicate as well as
        // comparing its complete captured command coordinates and sequence.
        let amove = raw.iter().find(|c| c["t"] == "amove").unwrap();
        assert_eq!(
            amove["orders"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r[0].as_u64().unwrap())
                .collect::<Vec<_>>(),
            observation
                .input()
                .units
                .iter()
                .map(|u| u64::from(u.id))
                .collect::<Vec<_>>()
        );
        let aim = memory.aim.unwrap();
        assert!(facts::exact(
            &json!({"i":aim.i,"until":aim.until}),
            &case["memoryAfter"]["mind"]["aim"]
        ));

        let view = View {
            units: case["frame"]["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|u| Unit {
                    id: u["id"].as_u64().unwrap() as u32,
                    at: facts::point(u),
                    world_goal: (!u["worldGoal"].is_null()).then(|| facts::point(&u["worldGoal"])),
                    amove: (!u["amove"].is_null()).then(|| facts::point(&u["amove"])),
                    target_id: u["targetId"].as_u64().unwrap() as u32,
                    retreating: facts::truthy(&u["retreating"]),
                })
                .collect(),
        };
        let news = raw
            .iter()
            .map(|c| inputs::news(c, &view).unwrap())
            .collect::<Vec<_>>();
        assert!(facts::exact(&json!(news), &case["human"]["news"]));
        let merged = inputs::formation_moves(news.into_iter().flatten().collect(), &view).unwrap();
        assert!(facts::exact(&json!(merged), &case["human"]["formation"]));
        let expected_inputs = case["human"]["inputs"].as_array().unwrap();
        assert_eq!(merged.len(), expected_inputs.len());
        let mut cursor = Cursor {
            camera: facts::point(&case["human"]["initialCursor"]["camera"]),
            selection: Vec::new(),
        };
        for (command, expected) in merged.iter().zip(expected_inputs) {
            assert!(facts::exact(command, &expected["command"]));
            let actual = inputs::inputs_for(&mut cursor, command, &view).unwrap();
            let rows = actual
                .iter()
                .map(|input| {
                    let mut row = json!({"kind":match input.kind {
                        InputKind::Camera=>"camera",InputKind::Select=>"select",
                        InputKind::Key=>"key",InputKind::Click=>"click",
                    },"x":input.at.x,"z":input.at.z});
                    if let Some(ids) = &input.ids {
                        row["ids"] = json!(ids);
                    }
                    if let Some(minimap) = input.minimap {
                        row["minimap"] = json!(minimap);
                    }
                    if let Some(command) = &input.command {
                        row["cmd"] = command.clone();
                    }
                    row
                })
                .collect::<Vec<_>>();
            assert!(facts::exact(&json!(rows), &expected["emitted"]), "{name}");
        }
        assert!(facts::exact(
            &json!({"camera":{"x":cursor.camera.x,"z":cursor.camera.z},"selection":cursor.selection.iter().map(i64::to_string).collect::<Vec<_>>().join(",")}),
            &case["human"]["finalCursor"]
        ));

        // No new RNG owner: the production path still draws from this seat only.
        let mut first = MatchRandom::new(37);
        let mut second = first.clone();
        second.next_u64(StreamId::Combat);
        second.next_u64(StreamId::Ai(owner as u32 + 1));
        let mut a = SourceState::default();
        let mut b = a.clone();
        let p = commander_ai::plan(&observation, &mut a, level(name), &mut first).unwrap();
        let q = commander_ai::plan(&observation, &mut b, level(name), &mut second).unwrap();
        assert_eq!(p, q);
        assert_eq!(a, b);
        assert_eq!(p.draws, fractions.len());

        // This extension cannot enter the source's support-gun assault branch.
        let mut unsupported = observation.input().clone();
        unsupported.units[0].kind = "mg".into();
        let unsupported = Observation::new(unsupported).unwrap();
        let before = (a.clone(), first.clone());
        assert_eq!(
            commander_ai::plan(&unsupported, &mut a, level(name), &mut first),
            Err(commander_ai::PolicyError::NotOpening)
        );
        assert_eq!((a, first), before);
    }
}

fn runtime_match() -> crate::match_runtime::Match {
    runtime_match_with_mode(crate::economy::Mode::Conquest)
}

fn runtime_match_with_mode(mode: crate::economy::Mode) -> crate::match_runtime::Match {
    use crate::{
        economy::Faction,
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_runtime::{Match, MatchOptions, PlayerConfig},
        weather::WeatherChoice,
    };
    let map=AuthoritativeMap::from_json(&serde_json::to_vec(&json!({"name":"AI runtime intent","w":64,"h":64,
        "rows":vec![".".repeat(64);64],"spawns":[{"x":4,"y":4},{"x":59,"y":59}],"points":[{"x":32,"y":32}]})).unwrap()).unwrap();
    let options = MatchOptions {
        seed: 771,
        mode,
        players: [0, 1]
            .into_iter()
            .map(|team| PlayerConfig {
                name: format!("Seat {team}"),
                team,
                faction: Faction::Germany,
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: None,
        living_weather: false,
        map_key: None,
        tech: false,
        supply: false,
        logistics: false,
        sandbox: false,
    };
    if mode == crate::economy::Mode::Classic {
        Match::new_classic(&map, options).unwrap()
    } else {
        Match::new(&map, options).unwrap()
    }
}
fn screen(view: &HumanView<Observation>) -> ScreenView<'_, Observation> {
    ScreenView {
        tick: view.tick,
        spawn: view.spawn,
        units: view.units.iter().collect(),
        visible: view.visible.clone(),
        sightings: view.sightings.iter().collect(),
        packet: &view.packet,
    }
}

#[test]
fn intent_ai01_detached_delivered_view_drives_decisions_under_hidden_perturbations() {
    let mut a = runtime_match();
    let mut b = runtime_match();
    for u in b.world.game.units.values_mut().filter(|u| u.owner == 1) {
        u.x -= 0.2;
        u.hp *= 0.5;
        u.supp = 95.0;
    }
    b.start.economy.players[1].mp = 1e6;
    b.start.economy.players[1].mun = 1e6;
    b.world.set_cell(60 * 64 + 40, b'O').unwrap();
    a.start.economy.players[0].mp = 100.2;
    b.start.economy.players[0].mp = 100.8;
    let va = a
        .human_commander_observation(
            &mut crate::match_human_observation::HumanObservationMemory::new(0),
        )
        .unwrap();
    let vb = b
        .human_commander_observation(
            &mut crate::match_human_observation::HumanObservationMemory::new(0),
        )
        .unwrap();
    assert_eq!(va, vb);
    let mut pa = HumanPolicy::new(0, 771);
    let mut pb = pa.clone();
    let before = va.clone();
    let commands = pa.plan(screen(&va), Level::Normal).unwrap();
    assert!(!commands.is_empty());
    assert_eq!(commands, pb.plan(screen(&vb), Level::Normal).unwrap());
    assert_eq!(pa, pb);
    assert_eq!(va, before);
    // A visible health change changes the decision while hidden state cannot.
    let id = *a
        .world
        .game
        .units
        .iter()
        .find(|(_, u)| u.owner == 0 && a.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .0;
    let u = a.world.game.units.get_mut(&id).unwrap();
    u.x = 40.0;
    u.z = 40.0;
    u.hp = 20.0;
    let hurt = a
        .human_commander_observation(
            &mut crate::match_human_observation::HumanObservationMemory::new(0),
        )
        .unwrap();
    let changed = HumanPolicy::new(0, 771)
        .plan(screen(&hurt), Level::Normal)
        .unwrap();
    assert!(
        changed
            .iter()
            .any(|c| c["t"] == "retreat" && c["ids"].as_array().unwrap().contains(&json!(id)))
    );
    assert_ne!(commands, changed);
}

#[test]
fn intent_ai36_runtime_policy_retreat_reaches_the_canonical_command_queue() {
    use crate::{match_commands::Command, orders::OrderKey};
    let mut m = runtime_match();
    m.start.economy.players[0].mp = 0.0;
    let id = *m
        .world
        .game
        .units
        .iter()
        .find(|(_, u)| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .0;
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = 40.0;
    u.z = 40.0;
    u.hp = 20.0;
    m.grid.rebuild(&m.world.game.units);
    let view = m
        .human_commander_observation(
            &mut crate::match_human_observation::HumanObservationMemory::new(0),
        )
        .unwrap();
    let commands = HumanPolicy::new(0, 771)
        .plan(screen(&view), Level::Normal)
        .unwrap();
    let raw = commands.iter().find(|c| c["t"] == "retreat").unwrap();
    m.enqueue_command(
        OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 0,
        },
        Command::from_json(raw).unwrap(),
    )
    .unwrap();
    let results = m.apply_commands(1, false).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].denial, None);
    assert!(m.world.game.units[&id].retreating);
    assert!(m.world.game.units[&id].world_goal.is_some());
}

#[test]
fn intent_ai28_delivered_stock_uses_public_two_decimal_precision() {
    let mut m = runtime_match();
    m.options.logistics = true;
    m.initialize_logistics().unwrap();
    let id = *m
        .world
        .game
        .units
        .iter()
        .find(|(_, u)| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .0;
    let stock = m.logistics.get_mut(id).unwrap();
    stock.provisions = 59.6;
    stock.fuel = Some(36.4);
    stock.ammo = stock.ammo_max * 0.499;
    let view = m
        .human_commander_observation(
            &mut crate::match_human_observation::HumanObservationMemory::new(0),
        )
        .unwrap();
    let delivered = view.packet.input().runtime.as_ref().unwrap().units[&id]
        .stock
        .as_ref()
        .unwrap();
    assert_eq!(delivered.provisions, 59.6);
    assert_eq!(delivered.fuel, Some(36.4));
    assert_eq!(delivered.ammo, Some(0.5));
    let commands = HumanPolicy::new(0, 771)
        .plan(screen(&view), Level::Normal)
        .unwrap();
    assert!(!commands.iter().any(|c| c["t"] == "retreat"));
}

#[test]
fn intent_ai11_ai_attempts_and_human_replay_share_costs_and_denials() {
    use crate::{match_commands::Command, orders::OrderKey};
    let mut ai = runtime_match();
    let mut human = runtime_match();
    ai.start.economy.players[0].mp = 1000.0;
    let delivered = ai
        .human_commander_observation(
            &mut crate::match_human_observation::HumanObservationMemory::new(0),
        )
        .unwrap();
    let commands = HumanPolicy::new(0, 771)
        .plan(screen(&delivered), Level::Normal)
        .unwrap();
    let buy = commands.iter().find(|c| c["t"] == "buy").unwrap();
    let price = crate::economy::price_of(ai.options.mode, buy["unit"].as_str().unwrap())
        .unwrap()
        .mp;
    for state in [&mut ai, &mut human] {
        state.start.economy.players[0].mp = price;
        state
            .enqueue_command(
                OrderKey {
                    tick: 1,
                    issuer: 0,
                    sequence: 0,
                },
                Command::from_json(buy).unwrap(),
            )
            .unwrap();
    }
    let a = ai.apply_commands(1, false).unwrap();
    let h = human.apply_commands(1, false).unwrap();
    assert_eq!(a, h);
    assert_eq!(a[0].denial, None);
    assert_eq!(ai.start.economy.players[0].mp, 0.0);
    assert_eq!(human.start.economy.players[0].mp, 0.0);
    // Refusal happens against the current wallet, after planning from an earlier delivery.
    let mut denied = runtime_match();
    denied.start.economy.players[0].mp = 0.0;
    denied
        .enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 0,
            },
            Command::from_json(buy).unwrap(),
        )
        .unwrap();
    let results = denied.apply_commands(1, false).unwrap();
    assert_eq!(
        results[0].denial,
        Some(crate::match_commands::Denial::Manpower)
    );
    assert_eq!(denied.start.economy.players[0].mp, 0.0);
}

#[test]
fn intent_ai53_forward_hq_uses_real_build_admission_and_next_observation_keeps_worker_busy() {
    use crate::{match_commands::Command, orders::OrderKey};
    let mut m = runtime_match();
    m.start.economy.players[0].mp = 200.0;
    for u in m
        .world
        .game
        .units
        .values_mut()
        .filter(|u| u.owner == 0 && !m.rules.kinds[u.kind].structure)
    {
        u.x = 100.0;
        u.z = 40.0;
    }
    m.grid.rebuild(&m.world.game.units);
    let mut memory = crate::match_human_observation::HumanObservationMemory::new(0);
    let view = m.human_commander_observation(&mut memory).unwrap();
    let commands = HumanPolicy::new(0, 771)
        .plan(screen(&view), Level::Normal)
        .unwrap();
    let build = commands.iter().find(|c| c["t"] == "build").unwrap();
    let builder = build["ids"][0].as_u64().unwrap() as u32;
    m.enqueue_command(
        OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 0,
        },
        Command::from_json(build).unwrap(),
    )
    .unwrap();
    let results = m.apply_commands(1, false).unwrap();
    assert_eq!(results[0].denial, None);
    assert_eq!(m.start.economy.players[0].mp, 0.0);
    assert!(m.world.game.units[&builder].build);
    assert!(m.commands.units[&builder].build_target.is_some());
    let next = m.human_commander_observation(&mut memory).unwrap();
    let worker = next.units.iter().find(|u| u.motion.id == builder).unwrap();
    assert!(worker.build);
    assert!(worker.motion.amove.is_none());
    assert!(worker.motion.world_goal.is_none());
    let planned = HumanPolicy::new(0, 771)
        .plan(screen(&next), Level::Normal)
        .unwrap();
    assert!(!planned.iter().any(|c| c["t"] == "build"));
    assert!(!planned.iter().any(|c| {
        c["orders"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|r| r[0] == builder))
    }));
}

#[test]
fn runtime_observation_attack_move_goal_obeys_owner_order_precedence() {
    let mut m = runtime_match();
    let id = *m
        .world
        .game
        .units
        .iter()
        .find(|(_, u)| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .0;
    m.world.game.units.get_mut(&id).unwrap().amove = Some(crate::game::Point { x: 50.0, z: 30.0 });
    m.commands.units.get_mut(&id).unwrap().fire_at = Some(20 * 64 + 20);
    let view = m
        .human_commander_observation(
            &mut crate::match_human_observation::HumanObservationMemory::new(0),
        )
        .unwrap();
    assert!(
        view.packet
            .input()
            .units
            .iter()
            .find(|u| u.id == id)
            .unwrap()
            .fire_at
    );
    assert_eq!(
        view.packet.input().runtime.as_ref().unwrap().units[&id].amove,
        None
    );
}

#[test]
fn intent_ai33_construction_spending_survives_ordinary_ticks_and_recipient_snapshots() {
    use crate::{match_commands::Command, orders::OrderKey, recipient_snapshot::SnapshotCache};
    let mut m = runtime_match();
    m.start.economy.players[0].mp = 1000.0;
    let mut policy = HumanPolicy::new(0, 771);
    let view = m
        .human_commander_observation(
            &mut crate::match_human_observation::HumanObservationMemory::new(0),
        )
        .unwrap();
    let commands = policy.plan(screen(&view), Level::Normal).unwrap();
    let raw = commands
        .iter()
        .find(|c| c["t"] == "build")
        .expect("base construction precedes shopping");
    assert_eq!(raw["kind"], "motorpool");
    let before = m.start.economy.players[0].mp;
    let cost = crate::catalog::Catalog::compiled()
        .unwrap()
        .unit("motorpool")
        .unwrap()
        .get("cost")
        .unwrap()
        .number()
        .unwrap();
    let buy_cost: f64 = commands
        .iter()
        .filter(|c| c["t"] == "buy")
        .map(|c| {
            crate::economy::price_of(m.options.mode, c["unit"].as_str().unwrap())
                .unwrap()
                .mp
        })
        .sum();
    for (sequence, raw) in commands.iter().enumerate() {
        m.enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: sequence as u64,
            },
            Command::from_json(raw).unwrap(),
        )
        .unwrap();
    }
    let report = m.advance_tick().unwrap();
    assert_eq!(report.commands.len(), commands.len());
    assert!(
        report.commands.iter().all(|c| c.denial.is_none()),
        "{:?}",
        report.commands
    );
    assert!((m.start.economy.players[0].mp - (before - cost - buy_cost + 0.2)).abs() < 1e-9);
    let id = *m
        .world
        .game
        .units
        .iter()
        .find(|(_, u)| m.rules.kinds[u.kind].name == "motorpool")
        .unwrap()
        .0;
    assert!(m.world.game.units[&id].built.unwrap() < 1.0);
    // Construction must remain admitted while walking, working and publishing owner orders.
    for _ in 0..1200 {
        m.advance_tick().unwrap();
        if m.world.game.tick % 20 == 0 {
            let cache = SnapshotCache::new(&m).unwrap();
            for seat in 0..2 {
                cache.recipient(seat).unwrap();
            }
        }
    }
    assert_eq!(m.tick_state.fault(), None);
    assert_eq!(m.world.game.units[&id].built, Some(1.0));
    let view = m.human_commander_observation(
        &mut crate::match_human_observation::HumanObservationMemory::new(0),
    );
    // Delivery is on even ticks, so advance once to the next ordinary observation boundary.
    assert!(view.is_err());
    m.advance_tick().unwrap();
    m.human_commander_observation(
        &mut crate::match_human_observation::HumanObservationMemory::new(0),
    )
    .unwrap();
}

#[test]
fn intent_ai33_fortification_state_survives_full_match_and_every_seat_snapshot() {
    use crate::{
        match_commands::{Command, Denial},
        orders::OrderKey,
        recipient_snapshot::SnapshotCache,
    };
    let snapshot_all = |m: &crate::match_runtime::Match| {
        let cache = SnapshotCache::new(m).unwrap();
        for seat in 0..m.start.players.len() {
            cache.recipient(seat).unwrap();
        }
    };
    let mut m = runtime_match();
    for _ in 0..3000 {
        m.advance_tick().unwrap();
    }
    let mut policy = HumanPolicy::new(0, 771);
    let mut sequence = 0;
    let required = ["motorpool", "airfield", "tower", "pillbox"];
    for look in 0..4 {
        if required.iter().all(|kind| {
            m.world
                .game
                .units
                .values()
                .any(|u| m.rules.kinds[u.kind].name == *kind && u.built == Some(1.0))
        }) {
            break;
        }
        if m.world.game.tick % 2 != 0 {
            m.advance_tick().unwrap();
        }
        m.start.economy.players[0].mp = if look == 0 { 400.0 } else { 2000.0 };
        let view = m
            .human_commander_observation(
                &mut crate::match_human_observation::HumanObservationMemory::new(0),
            )
            .unwrap();
        let commands = policy.plan(screen(&view), Level::Normal).unwrap();
        let builds: Vec<_> = commands.iter().filter(|c| c["t"] == "build").collect();
        assert!(!builds.is_empty(), "look {look}: {commands:?}");
        assert!(
            builds
                .iter()
                .all(|c| required.contains(&c["kind"].as_str().unwrap()))
        );
        let tick = m.world.game.tick as u64 + 1;
        for raw in &commands {
            m.enqueue_command(
                OrderKey {
                    tick,
                    issuer: 0,
                    sequence,
                },
                Command::from_json(raw).unwrap(),
            )
            .unwrap();
            sequence += 1;
        }
        let report = m.advance_tick().unwrap();
        assert_eq!(report.commands.len(), commands.len());
        for (raw, result) in commands.iter().zip(&report.commands) {
            if raw["t"] == "build" || raw["t"] == "buy" {
                assert_eq!(result.denial, None, "look {look}: {raw:?}");
            } else {
                assert!(
                    matches!(result.denial, None | Some(Denial::NoCover)),
                    "look {look}: {raw:?}: {result:?}"
                );
            }
        }
        for _ in 0..2000 {
            if !m.world.game.units.values().any(|u| u.build) {
                break;
            }
            m.advance_tick().unwrap();
            if m.world.game.tick % 20 == 0 {
                snapshot_all(&m);
            }
        }
        assert!(!m.world.game.units.values().any(|u| u.build));
        snapshot_all(&m);
    }
    for kind in required {
        assert!(
            m.world
                .game
                .units
                .values()
                .any(|u| m.rules.kinds[u.kind].name == kind && u.built == Some(1.0)),
            "{kind}"
        );
    }
    let hq = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    assert_eq!(policy.tactics.fort_steps.get(&hq), Some(&2));
    // Gate commands reject an ineligible or missing builder.
    for raw in [json!({"t":"dig","kind":"gate","ids":[3],"x":50.0,"z":50.0})] {
        let tick = m.world.game.tick as u64 + 1;
        m.enqueue_command(
            OrderKey {
                tick,
                issuer: 0,
                sequence,
            },
            Command::from_json(&raw).unwrap(),
        )
        .unwrap();
        sequence += 1;
        assert_eq!(
            m.advance_tick().unwrap().commands[0].denial,
            Some(Denial::NoBuilders)
        );
        for _ in 0..8 {
            m.advance_tick().unwrap();
        }
        snapshot_all(&m);
    }
    // Capture income reaches ordinary victory while the policy keeps its fortification state.
    for _ in 0..30000 {
        if m.finish.is_some() {
            break;
        }
        m.advance_tick().unwrap();
        if m.world.game.tick % 20 == 0 {
            snapshot_all(&m);
        }
    }
    assert!(m.finish.is_some());
    for _ in 0..4 {
        m.advance_tick().unwrap();
        snapshot_all(&m);
    }
    snapshot_all(&m);
    assert_eq!(policy.tactics.fort_steps.get(&hq), Some(&2));
    assert_eq!(m.tick_state.fault(), None);
}

#[test]
fn intent_ai66_private_streams_drive_ordinary_ai_ticks_and_every_seat_snapshot() {
    use crate::{
        economy::Mode,
        human_commander_timing::{Skill, Timing},
        match_human_observation::HumanObservationMemory,
        orders::OrderKey,
        recipient_snapshot::{DeliveryState, SelectedSeat},
        seat_commander::{Advance, Brain, Delivered},
    };
    for mode in [Mode::Conquest, Mode::Classic] {
        let mut game = runtime_match_with_mode(mode);
        if mode == Mode::Conquest {
            game.start.win_vp = 1.0;
        } else {
            // Start after two existing depots, so the ordinary non-depot branch
            // is reached without depending on the separate node-economy planner.
            for (slot, cells) in [
                (0, [11 * 64 + 6, 6 * 64 + 11]),
                (1, [52 * 64 + 57, 57 * 64 + 52]),
            ] {
                for cell in cells {
                    game.place_starting_building(slot, "depot", cell).unwrap();
                }
            }
        }
        let mut seats: Vec<_> = (0..2)
            .map(|slot| game.create_seat_commander(slot, Level::Hard).unwrap())
            .collect();
        let mut memories: Vec<_> = (0..2).map(HumanObservationMemory::new).collect();
        let mut delivery = DeliveryState::new(&game).unwrap();
        let mut sequence = [0; 2];
        let mut emitted = [0; 2];
        let mut maximum_draws = [0; 2];
        let mut snapshots = 0;
        let mut built = false;
        let mut ended = None;
        let limit = if mode == Mode::Classic { 600 } else { 1800 };
        for _ in 0..limit {
            let tick = game.world.game.tick;
            for slot in 0..2 {
                let before_other = seats[1 - slot].clone();
                let terminal = game.finish.is_some();
                let before_self = terminal.then(|| seats[slot].clone());
                let before_match_rng = game.start.random.clone();
                let before_environment = game.start.environment_seed;
                let first = matches!(seats[slot].brain(), Brain::Human { state, .. } if state.snapshot().is_none());
                let view = seats[slot].wants_view(tick, tick % 2 == 0).then(|| {
                    Delivered::Human(Box::new(
                        game.human_commander_observation(&mut memories[slot])
                            .unwrap(),
                    ))
                });
                let Advance::Human(advance) = game
                    .advance_seat_commander(&mut seats[slot], view, tick as u64, &[])
                    .unwrap()
                else {
                    panic!("ordinary seat must use human timing")
                };
                assert_eq!(seats[1 - slot], before_other);
                if let Some(before) = before_self {
                    assert_eq!(seats[slot], before);
                    assert_eq!(
                        advance,
                        crate::human_commander_state::HumanAdvance::default()
                    );
                }
                assert_eq!(game.start.random, before_match_rng);
                assert_eq!(game.start.environment_seed, before_environment);
                let Brain::Human { state, .. } = seats[slot].brain() else {
                    panic!()
                };
                let runtime = state.snapshot().unwrap();
                assert_eq!(runtime.timing.draws() % 2, 0);
                maximum_draws[slot] = maximum_draws[slot].max(runtime.timing.draws());
                if first {
                    let mut expected = Timing::new(game.options.seed as u32, slot as u32);
                    let delay = expected.opening_delay(Skill::for_level("hard")).unwrap();
                    assert_eq!(runtime.timing, expected);
                    assert_eq!(runtime.look.as_ref().unwrap().at, tick as u64 + delay);
                }
                if let Some(raw) = advance.input.and_then(|input| input.command) {
                    emitted[slot] += 1;
                    let command = Command::from_json(&raw).unwrap();
                    built |= raw["t"] == "build";
                    game.enqueue_command(
                        OrderKey {
                            tick: tick as u64 + 1,
                            issuer: slot as u32,
                            sequence: sequence[slot],
                        },
                        command,
                    )
                    .unwrap();
                    sequence[slot] += 1;
                }
            }
            let report = game.advance_tick().unwrap();
            assert_eq!(game.tick_state.fault(), None);
            let selected = [0, 1].map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            });
            let frame = delivery
                .capture_selected_with_tick_events(&mut game, &selected, &report.events)
                .unwrap()
                .unwrap();
            for slot in 0..2 {
                let packet = frame.recipient(slot).unwrap().into_json();
                assert_eq!(packet["tick"], report.tick);
                snapshots += 1;
            }
            if game.finish.is_some() {
                let finish_tick = *ended.get_or_insert(game.world.game.tick);
                if game.world.game.tick >= finish_tick + 8 {
                    break;
                }
            }
        }
        assert!(
            emitted.iter().all(|count| *count > 0),
            "{mode:?}: {emitted:?}"
        );
        assert!(maximum_draws.iter().all(|count| *count > 2));
        assert_eq!(snapshots, game.world.game.tick as usize * 2);
        if mode == Mode::Classic {
            assert!(
                built,
                "Classic planner must deliver an actual build attempt"
            );
            assert!(
                game.world
                    .game
                    .units
                    .values()
                    .any(|u| game.rules.kinds[u.kind].name == "barracks")
            );
        } else {
            assert!(
                ended.is_some(),
                "point income must finish the ordinary match"
            );
        }
    }
}
