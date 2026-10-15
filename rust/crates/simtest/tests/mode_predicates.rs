//! Shared Source-backed MODE predicates. Fixture generation and native execution need a ledger grant.
use serde_json::{Value, json};
use ww2_core::{
    deployment::{self, Spawn},
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{LOCAL_DEFENDER_OWNER, ModeKind, Point},
    legacy_scenario::{LegacyScenario, PointOwner},
    map::AuthoritativeMap,
    match_commands::Command,
    match_end::{EndReason, MatchSample},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    objectives::{Objective, ObjectivePlayer, ObjectiveUnit, capture_points},
    recipient_snapshot::{DeliveryState, SelectedSeat, SnapshotCache},
    vision,
    weather::{WeatherChoice, WeatherKind},
};

fn fixture() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mode-predicates.json");
    let text = std::fs::read_to_string(path)
        .expect("generate mode-predicates.json with the authorized pinned-Source recorder first");
    let f: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(f["format"], 1);
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(f["kind"], "isolated-components-not-whole-match");
    f
}
fn encode(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(format!("{:016x}", n.as_f64().unwrap().to_bits())),
        Value::Array(a) => Value::Array(a.into_iter().map(encode).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, encode(v))).collect()),
        v => v,
    }
}
fn teams(v: &Value) -> Vec<i32> {
    v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap() as i32).collect()
}
fn options(f: &Value, roster: &[i32], mode: Mode) -> MatchOptions {
    MatchOptions {
        seed: f["seed"].as_u64().unwrap(),
        mode,
        players: roster.iter().enumerate().map(|(i, &team)| PlayerConfig {
            name: format!("P{i}"),
            team,
            faction: Faction::try_from((i % 4) as u8).unwrap(),
        }).collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Fixed(WeatherKind::Clear),
        weather_seed: Some(31),
        living_weather: true,
        map_key: None,
        tech: true,
        supply: false,
        logistics: false,
        sandbox: false,
    }
}
fn make(f: &Value, authored: &Value, roster: &[i32], mode: Mode, defender: i32) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(authored).unwrap()).unwrap();
    let o = options(f, roster, mode);
    let mut m = if mode == Mode::Assault {
        Match::new_assault(&map, o, defender)
    } else {
        Match::new(&map, o)
    }.unwrap();
    for u in m.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    for c in m.commands.units.values_mut() {
        c.hold_fire = true;
    }
    m
}
fn ordinary(f: &Value) -> Match {
    make(f, &f["map"], &teams(&f["roster"]), Mode::Conquest, 5)
}
fn recipients(m: &Match) -> Value {
    let cache = SnapshotCache::new(m).unwrap();
    json!((0..m.start.players.len()).map(|slot| {
        let s = cache.recipient(slot).unwrap();
        let mut units = s.fields["units"].as_array().unwrap().iter()
            .map(|u| Value::Array(u.as_array().unwrap()[..3].to_vec())).collect::<Vec<_>>();
        units.sort_by_key(|u| u[0].as_u64().unwrap());
        json!({"slot":slot,"tick":s.fields["tick"],"winner":s.fields["winner"],
            "end":s.fields.get("end").cloned().unwrap_or(Value::Null),"units":units})
    }).collect::<Vec<_>>())
}
fn anchors(m: &Match) -> Vec<(u32, usize, usize, u64, u64)> {
    m.world.game.units.values().map(|u| (u.id, u.owner, u.kind, u.x.to_bits(), u.z.to_bits())).collect()
}
fn finish_projection(m: &Match) -> Value {
    let timeline = m.timeline.iter().map(|s| match s {
        MatchSample::Conquest { t, vp } => json!({"t":t,"vp":vp}),
        _ => panic!("ordinary Conquest sample"),
    }).collect::<Vec<_>>();
    encode(json!({
        "tick":m.world.game.tick,"winner":m.finish.map(|x|x.winner),
        "reason":m.finish.map(|x|x.reason.as_str()),
        "at":m.finish.map(|x|json!({"x":x.end_at.x,"z":x.end_at.z})),
        "endTick":m.finish.map(|x|x.end_tick),"reveal":m.world.game.reveal,
        "timeline":timeline,"recipients":recipients(m),
        "allUnits":m.world.game.units.values().map(|u|json!([u.id,m.rules.kinds[u.kind].name,u.owner])).collect::<Vec<_>>(),
    }))
}
fn source_slot(slot: &Value) -> usize {
    match slot.as_i64().unwrap() {
        -1 => LOCAL_DEFENDER_OWNER,
        -2 => LOCAL_DEFENDER_OWNER - 1,
        n => n as usize,
    }
}

#[test]
fn alliance_predicate_and_existing_consumers_match_source() {
    let f = fixture();
    for c in f["alliance"].as_array().unwrap() {
        let m = make(&f, &f["map"], &teams(&c["teams"]), Mode::Conquest, 5);
        // Pure Game predicate input only. No World Match, geography, tick or admission.
        let mut game = m.world.game.clone();
        game.mode = (c["mode"] == "world").then_some(ModeKind::World);
        let slots = c["slots"].as_array().unwrap();
        let actual = slots.iter().map(|a| slots.iter()
            .map(|b| game.allied(source_slot(a), source_slot(b))).collect::<Vec<_>>()).collect::<Vec<_>>();
        assert_eq!(json!(actual), c["expected"], "alliance {}", c["mode"]);
    }
    let mut m = ordinary(&f);
    let before = anchors(&m);
    vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    let c = &f["allianceConsumers"];
    let shooter = c["shooter"].as_u64().unwrap() as u32;
    let ally = c["ally"].as_u64().unwrap() as u32;
    let command = Command::Attack { ids: vec![shooter.into()], target: ally.into(), queue: false, at: None };
    m.execute_command(0, &command, false).unwrap();
    let actual = encode(json!({
        "visible":m.world.game.players.iter().map(|p|p.visible.clone()).collect::<Vec<_>>(),
        "recipients":recipients(&m),"attackId":m.commands.units[&shooter].attack_id,
    }));
    assert_eq!(actual, c["expected"], "shared vision, ally row, rejected attack");
    assert_eq!(anchors(&m), before);
    for c in f["captureCases"].as_array().unwrap() {
        let players = teams(&f["roster"]).into_iter().map(|team| ObjectivePlayer {
            team, out: false, spawn: Point { x: 0.0, z: 0.0 },
        }).collect::<Vec<_>>();
        let units = c["owners"].as_array().unwrap().iter().enumerate().map(|(i, owner)| ObjectiveUnit {
            id: i as u32 + 1, owner: owner.as_u64().unwrap() as usize, kind: "rifle",
            at: Point { x: 40.0, z: 40.0 }, hp: 100.0, airborne: false,
            riding: false, retreating: false, forced_reserve: false,
        }).collect::<Vec<_>>();
        let slot = |v: &Value| v.as_i64().filter(|&x| x >= 0).map(|x| x as usize);
        let mut points = vec![Objective {
            id: json!("point:0"), at: Point { x: 40.0, z: 40.0 }, vp: 1.0, mp: 1.0,
            kind: None, needs: None, owner: slot(&c["owner"]), capper: slot(&c["capper"]),
            progress: c["progress"].as_f64().unwrap(), cut: false, on_point: vec![], contested: false,
        }];
        capture_points(&mut points, &units, &players, m.rules.movement.tick).unwrap();
        let p = &points[0];
        let actual = encode(json!({"owner":p.owner.map_or(-1,|x|x as i64),
            "capper":p.capper.map_or(-1,|x|x as i64),"progress":p.progress,
            "contested":p.contested,"onPoint":p.on_point}));
        assert_eq!(actual, c["expected"], "capture {}", c["name"]);
    }
    let mut m = ordinary(&f);
    let before = anchors(&m);
    for (player, score) in m.start.economy.players.iter_mut().zip(f["teamVictory"]["scores"].as_array().unwrap()) {
        player.vp = score.as_f64().unwrap();
    }
    m.advance_tick().unwrap();
    assert_eq!(finish_projection(&m), f["teamVictory"]["expected"]);
    assert_eq!(anchors(&m), before);
    assert_eq!(f["alliance"].as_array().unwrap().len(), 4);
    assert_eq!(f["captureCases"].as_array().unwrap().len(), 4);
}

#[test]
fn assault_spawn_filter_constructor_and_minimum_match_source() {
    let f = fixture();
    for c in f["spawnCases"].as_array().unwrap() {
        let source = f["maps"].as_array().unwrap().iter().find(|m| m["name"] == c["name"]).unwrap();
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&source["map"]).unwrap()).unwrap();
        assert!(map.spawns()[0].assault);
        assert!(map.defend().unwrap().contains(&0));
        let spawns = map.spawns().iter().map(|p| Spawn { x:p.x, y:p.y, assault:p.assault }).collect::<Vec<_>>();
        let selected = deployment::spawns_for(&spawns, Mode::from_kind(c["mode"].as_str())).unwrap();
        assert_eq!(json!(selected), c["expected"]["indices"], "{} {}", c["name"], c["mode"]);
    }
    for c in f["spawnConstructors"].as_array().unwrap() {
        let source = f["maps"].as_array().unwrap().iter().find(|m| m["name"] == c["name"]).unwrap();
        let mode = if c["mode"] == "assault" { Mode::Assault } else { Mode::Conquest };
        let m = make(&f, &source["map"], &[0, 1], mode, 0);
        let actual = encode(json!(m.start.players.iter().map(|p| json!({
            "team":p.config.team,"spawn":{"x":p.spawn.x,"z":p.spawn.z},
        })).collect::<Vec<_>>()));
        assert_eq!(actual, c["expected"], "constructor {} {}", c["name"], c["mode"]);
    }
    for c in f["minimumCases"].as_array().unwrap() {
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&c["map"]).unwrap());
        assert_eq!(map.is_ok(), c["error"].is_null(), "minimum {}", c["name"]);
    }
    assert_eq!(f["spawnCases"].as_array().unwrap().len(), 21);
    assert_eq!(f["spawnConstructors"].as_array().unwrap().len(), 6);
    assert_eq!(f["minimumCases"].as_array().unwrap().len(), 3);
}

#[test]
fn assault_team_normalization_all_canonical_tables_match_source() {
    let f = fixture();
    for c in f["normalizationCases"].as_array().unwrap() {
        let defender = c["defender"].as_i64().unwrap() as i32;
        let m = make(&f, &f["map"], &teams(&c["teams"]), Mode::Assault, defender);
        let expected = &c["expected"]["teams"];
        for actual in [
            m.options.players.iter().map(|p|p.team).collect::<Vec<_>>(),
            m.start.players.iter().map(|p|p.config.team).collect(),
            m.start.economy.players.iter().map(|p|p.team).collect(),
            m.world.game.players.iter().map(|p|p.team).collect(),
        ] {
            assert_eq!(json!(actual), *expected, "canonical team table {}", c["teams"]);
        }
        let state = m.assault.unwrap();
        assert_eq!(json!({"teams":m.options.players.iter().map(|p|p.team).collect::<Vec<_>>(),
            "defender":state.defender_team,"attacker":state.attacker_team}), c["expected"]);
    }
    assert_eq!(f["normalizationCases"].as_array().unwrap().len(), 10);
}

#[test]
fn complete_legacy_migration_definitions_match_original_source() {
    let f = fixture();
    for c in f["legacyCases"].as_array().unwrap() {
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&c["map"]).unwrap());
        assert_eq!(map.is_ok(), c["error"].is_null(), "legacy validation {}", c["name"]);
        if let Ok(map) = map {
            let names = map.points().iter().enumerate().map(|(i, _)| {
                c["map"]["points"][i]["id"].as_str().map(str::to_owned).unwrap_or_else(||format!("point:{i}"))
            }).collect::<Vec<_>>();
            let points = names.iter().map(|id|PointOwner { id, owner:-1 }).collect::<Vec<_>>();
            let scenario = LegacyScenario::from_map(&map, &points);
            let actual = scenario.as_ref().map_or(Value::Null, LegacyScenario::definition_json);
            assert_eq!(actual, c["expected"], "complete migration {}", c["name"]);
        }
    }
    // Actual ordinary ticks, detached event delivery and once-only visibility for all four seats.
    let mut state = make(&f, &f["legacyTrace"]["map"], &teams(&f["roster"]), Mode::Conquest, 5);
    let before = anchors(&state);
    let mut delivery = DeliveryState::new(&state).unwrap();
    let seats = (0..state.start.players.len()).map(|slot|SelectedSeat {
        slot, full: false, omit: false,
    }).collect::<Vec<_>>();
    for expected in f["legacyTrace"]["frames"].as_array().unwrap() {
        let report = state.advance_tick().unwrap();
        assert_eq!(json!(report.tick), expected["tick"]);
        assert_eq!(state.scenario.as_ref().unwrap().state_json(), expected["scenario"]);
        {
            let captured = delivery.capture_selected_with_tick_events(&mut state, &seats, &report.events)
                .unwrap().unwrap();
            for row in expected["recipients"].as_array().unwrap() {
                let slot = row["slot"].as_u64().unwrap() as usize;
                let actual = captured.recipient(slot).unwrap().into_json();
                assert_eq!(actual["shots"], row["shots"], "public Say seat{slot}");
                assert_eq!(actual["scenario"], row["scenario"], "legacy HUD seat{slot}");
            }
        }
        assert_eq!(anchors(&state), before);
    }
    assert_eq!(f["legacyTrace"]["frames"].as_array().unwrap().len(), 2);
    assert_eq!(f["legacyCases"].as_array().unwrap().len(), 21);
}

fn reason(value: &Value) -> EndReason {
    match value.as_str().unwrap() {
        "vp" => EndReason::VictoryPoints,
        "hq" => EndReason::Headquarters,
        "bunkers" => EndReason::Bunkers,
        "structures" => EndReason::Structures,
        "timer" => EndReason::Timer,
        "world" => EndReason::World,
        "nations" => EndReason::Nations,
        "tutorial" => EndReason::Tutorial,
        _ => panic!("declared finish reason"),
    }
}
#[test]
fn finish_recipient_id_projection_and_sample_multiplicity_match_source() {
    let f = fixture();
    for c in f["finishCases"].as_array().unwrap() {
        let mut m = ordinary(&f);
        let before = anchors(&m);
        if let Some(tick) = c["finishTick"].as_i64() {
            while m.world.game.tick < tick - 1 {
                m.advance_tick().unwrap();
            }
            assert_eq!(finish_projection(&m), c["unfinished"], "before {}", c["name"]);
            m.start.economy.players[0].vp = 1200.125;
            m.start.economy.players[2].vp = 1200.125;
            m.advance_tick().unwrap();
            assert_eq!(m.world.game.tick, tick);
            // Tick 1/200 has an early sample and its distinct final sample at the same time.
            let expected_samples = match tick { 1 => 2, 199 => 2, 200 => 3, 201 => 3, _ => unreachable!() };
            assert_eq!(m.timeline.len(), expected_samples);
        } else {
            assert_eq!(finish_projection(&m), c["unfinished"], "fresh {}", c["name"]);
            let at = c["at"].as_object().map(|p|Point { x:p["x"].as_f64().unwrap(), z:p["z"].as_f64().unwrap() });
            m.initialize_tick_scene_finish(c["winner"].as_i64().unwrap() as i32, reason(&c["reason"]), at).unwrap();
            assert_eq!(m.timeline.len(), 1);
        }
        let expected = &c["expected"];
        assert_eq!(finish_projection(&m), *expected, "finished {}", c["name"]);
        // Every identity, including fog-hidden enemy actors, is present for every recipient.
        let all = expected["allUnits"].clone();
        for recipient in expected["recipients"].as_array().unwrap() {
            assert_eq!(recipient["units"], all);
        }
        // Actual guarded late-phase retry, not a claim that the fresh-scene hook is a no-op.
        m.start.economy.players[0].vp = 0.0;
        m.start.economy.players[2].vp = 0.0;
        m.start.economy.players[1].vp = m.start.win_vp;
        assert_eq!(m.apply_conquest_victory().unwrap(), None);
        assert_eq!(finish_projection(&m), *expected, "first finish {}", c["name"]);
        assert_eq!(anchors(&m), before, "canonical actors {}", c["name"]);
    }
    assert_eq!(f["finishCases"].as_array().unwrap().len(), 13);
}
