// Original react is not represented by canonical Match metadata. This transcript
// performs no reaction or tick phases, so react is explicitly excluded below.
use crate::{
    companions::EDGE_NAMES,
    economy::{Faction, Mode, Technology},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    orders::OrderKey,
    recorded_input::CompanionJournalId,
    weather::WeatherChoice,
};
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/recorded-gameplay.json")).unwrap()
}
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn at(p: Option<Point>) -> Value {
    p.map_or(Value::Null, |p| json!([bits(p.x), bits(p.z)]))
}
fn technology(kind: &str) -> Technology {
    match kind {
        "tier" => Technology::Tier,
        "inf" => Technology::Infantry,
        "guns" => Technology::Guns,
        "armor" => Technology::Armor,
        _ => panic!("unknown source technology"),
    }
}
fn technology_name(kind: Technology) -> &'static str {
    match kind {
        Technology::Tier => "tier",
        Technology::Infantry => "inf",
        Technology::Guns => "guns",
        Technology::Armor => "armor",
    }
}
fn make(f: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&f["map"]).unwrap()).unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: f["seed"].as_u64().unwrap(),
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "Commander".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "Enemy".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(217),
            living_weather: false,
            map_key: None,
            tech: true,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
fn boundary(m: &mut Match, f: &Value, tick: u64) {
    m.world.game.tick = tick as i64;
    for b in f["boundaries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["tick"].as_u64() == Some(tick))
    {
        for patch in b["players"].as_array().unwrap() {
            let owner = patch["owner"].as_u64().unwrap() as usize;
            let p = &mut m.start.economy.players[owner];
            if let Some(n) = patch["fields"]["mp"].as_f64() {
                p.mp = n;
            }
            if let Some(v) = patch["fields"]["away"].as_bool() {
                p.away = v;
            }
        }
    }
}
fn decode(row: &Value) -> Result<Command, Denial> {
    let op = &row["operation"];
    match op["kind"].as_str().unwrap() {
        "command" => Command::from_json(&op["value"]),
        "trusted_join" => Ok(Command::CompanionJoin {
            identity: CompanionJournalId::new(op["journalId"].as_u64().unwrap()).unwrap(),
        }),
        "trusted_input" => Ok(Command::CompanionInput {
            identity: CompanionJournalId::new(op["journalId"].as_u64().unwrap()).unwrap(),
            input: op["value"].clone(),
        }),
        _ => panic!("unknown fixture operation"),
    }
}
fn key(row: &Value) -> OrderKey {
    // The queue applies tick+1 before the Match clock advances; JS handlers see tick.
    OrderKey {
        tick: row["key"]["tick"].as_u64().unwrap() + 1,
        issuer: row["key"]["issuer"].as_u64().unwrap() as u32,
        sequence: row["key"]["sequence"].as_u64().unwrap(),
    }
}
fn denial_value(denial: Option<Denial>) -> Value {
    denial.map_or(Value::Null, |d| json!(d.name()))
}
fn expected_denial(row: &Value) -> Value {
    if row["operation"]["kind"] == "command" {
        row["sourceResult"].clone()
    } else {
        Value::Null
    }
}

fn compare(m: &Match, expected: &Value) {
    assert_eq!(m.world.game.tick, expected["tick"].as_i64().unwrap());
    assert_eq!(m.next_unit_id, expected["nextId"].as_u64().unwrap() as u32);
    assert_eq!(
        m.firing.next_flight,
        expected["nextFlight"].as_u64().unwrap()
    );
    for row in expected["players"].as_array().unwrap() {
        let owner = row["slot"].as_u64().unwrap() as usize;
        let p = &m.start.economy.players[owner];
        let start = &m.start.players[owner];
        let research = m.production.player(owner).unwrap();
        let tech = research.tech();
        assert_eq!(start.config.team, row["team"].as_i64().unwrap() as i32);
        assert_eq!(
            start.config.faction as u8,
            row["faction"].as_u64().unwrap() as u8
        );
        assert_eq!(p.away, row["away"].as_bool().unwrap());
        assert_eq!(p.out, row["out"].as_bool().unwrap());
        for (name, n) in [("mp", p.mp), ("mun", p.mun), ("fuel", p.fuel)] {
            assert_eq!(bits(n), row[name], "wallet {owner}/{name}");
        }
        assert_eq!(tech.tier(), row["tier"].as_u64().unwrap() as u8);
        for k in ["inf", "guns", "armor"] {
            assert_eq!(
                tech.level(technology(k)),
                row["armory"][k].as_u64().unwrap() as u8
            );
        }
        assert_eq!(research.jobs().len(), row["lab"].as_array().unwrap().len());
        for (job, source) in research.jobs().iter().zip(row["lab"].as_array().unwrap()) {
            assert_eq!(technology_name(job.kind), source["kind"].as_str().unwrap());
            assert_eq!(bits(job.progress), bits(source["prog"].as_f64().unwrap()));
            assert_eq!(bits(job.time), bits(source["time"].as_f64().unwrap()));
        }
        for (kind, value) in row["support"].as_object().unwrap() {
            assert_eq!(bits(start.support[kind]), *value);
        }
    }
    let units = expected["units"].as_array().unwrap();
    assert_eq!(
        m.world.game.units.keys().copied().collect::<Vec<_>>(),
        units
            .iter()
            .map(|u| u["id"].as_u64().unwrap() as u32)
            .collect::<Vec<_>>()
    );
    for row in units {
        let id = row["id"].as_u64().unwrap() as u32;
        let u = &m.world.game.units[&id];
        let c = &m.commands.units[&id];
        let a = m.abilities.get(id).unwrap();
        let fire = &m.firing.units[&id];
        let combat = m.combat.get(id).unwrap();
        let soldier = m.companions.soldiers.get(&id);
        let kind = m.rules.kinds[u.kind].name.as_str();
        assert_eq!(kind, row["kind"].as_str().unwrap());
        assert_eq!(u.owner, row["owner"].as_u64().unwrap() as usize);
        assert_eq!(at(Some(u.at())), row["at"]);
        for (name, value) in row["scalars"].as_object().unwrap() {
            let n = match name.as_str() {
                "rot" => u.rot,
                "aim" => c.aim,
                "hp" => u.hp,
                "supp" => u.supp,
                "cd" => a.cooldown,
                "buff" => a.buff,
                "sprint" => u.sprint,
                "cooldown" => fire.cooldown,
                "retarget" => fire.retarget,
                "stuck" => u.stuck,
                "repath" => u.repath,
                "pace" => u.pace,
                "ghost" => u.ghost,
                "moveSpeed" => u.move_speed,
                "xp" => combat.xp,
                "hitAt" => combat.hit_at,
                "jy" => soldier.unwrap().jy,
                "vy" => soldier.unwrap().vy,
                "reloadT" => soldier.unwrap().reload,
                "nadeT" => soldier.unwrap().nade_time,
                "react" => {
                    assert_eq!(*value, bits(0.0), "excluded source reaction timer changed");
                    continue;
                }
                _ => panic!("unmapped represented scalar {name}"),
            };
            assert_eq!(bits(n), *value, "unit {id}/{name}");
        }
        assert_eq!(
            json!(u.path.iter().map(|p| at(Some(*p))).collect::<Vec<_>>()),
            row["path"]
        );
        for (name, p) in [
            ("worldGoal", u.world_goal),
            ("amove", u.amove),
            ("reverse", u.reverse),
            ("routeEnd", c.route_end),
        ] {
            assert_eq!(at(p), row[name]);
        }
        assert_eq!(c.auto_cast, row["auto"].as_bool().unwrap());
        assert_eq!(c.auto_retreat, row["autoRetreat"].as_bool().unwrap());
        assert_eq!(c.hold_fire, row["holdFire"].as_bool().unwrap());
        assert_eq!(u.hold_pos, row["holdPos"].as_bool().unwrap());
        assert_eq!(a.ap, row["ap"].as_bool().unwrap());
        let drift = if c.rally_drift {
            json!("rally")
        } else {
            json!(u.drift)
        };
        assert_eq!(drift, row["drift"]);
        assert_eq!(
            u.garrison.map_or(-1, |v| v as i64),
            row["garrison"].as_i64().unwrap()
        );
        assert_eq!(c.follow, row["follow"].as_u64().unwrap() as u32);
        if let Some(v) = row["attackId"].as_u64() {
            assert_eq!(c.attack_id, v as u32);
        }
        if let Some(v) = row["targetId"].as_u64() {
            assert_eq!(fire.target_id, v as u32);
        }
        assert_eq!(bits(u.traffic_wait), row["trafficWait"]);
        assert_eq!(
            json!(c.outcome.map(|o| match o {
                crate::match_commands::MoveOutcome::Interrupted => "interrupted",
                crate::match_commands::MoveOutcome::Moving => "moving",
                crate::match_commands::MoveOutcome::Unreachable => "unreachable",
                crate::match_commands::MoveOutcome::Arrived => "arrived",
            })),
            row["outcome"]
        );
        assert_eq!(json!(c.outcome_tick), row["outcomeTick"]);
        // These source structures stay empty in this bounded handler transcript.
        assert!(row["orders"].as_array().unwrap().is_empty() && c.orders.is_empty());
        assert!(row["face"].is_null() && c.face.is_none());
        assert!(row["traffic"].is_null() && u.traffic.is_none());
        assert!(row["nade"].is_null() && c.nade.is_none());
        if let Some(s) = soldier {
            let op = &m.companions.records[&s.companion];
            assert_eq!(json!(op.key), row["operative"]);
            assert_eq!(json!(s.weapon), row["weapon"]);
            assert_eq!(json!(s.mags), row["mags"]);
            assert_eq!(json!(s.nades), row["nades"]);
        } else {
            assert!(row["operative"].is_null());
        }
    }
    let ops = expected["companions"].as_array().unwrap();
    assert_eq!(m.companions.records.len(), ops.len());
    for (op, source) in m.companions.records.values().zip(ops) {
        for (name, v) in [
            ("unitId", op.unit_id as u64),
            ("squadId", op.squad_id as u64),
            ("shots", op.shots),
            ("hits", op.hits),
            ("kills", op.kills),
        ] {
            assert_eq!(json!(v), source[name]);
        }
        assert_eq!(json!(op.key), source["key"]);
        assert_eq!(json!(op.owner), source["owner"]);
        assert_eq!(json!(op.online), source["online"]);
        assert_eq!(json!(op.last_seq), source["lastSeq"]);
        assert_eq!(json!(op.last_input), source["lastInput"]);
        assert_eq!(json!(op.respawn_at), source["respawnAt"]);
        for (i, name) in EDGE_NAMES.iter().enumerate() {
            assert_eq!(op.seen[i], source["seen"][name].as_i64());
            assert_eq!(op.want[i], source["want"][name].as_bool().unwrap_or(false));
        }
        if let Some(input) = &op.input {
            let s = &source["input"];
            for (name, n) in [
                ("forward", input.forward),
                ("strafe", input.strafe),
                ("yaw", input.yaw),
                ("pitch", input.pitch),
            ] {
                assert_eq!(bits(n), bits(s[name].as_f64().unwrap()));
            }
            for (name, v) in [
                ("fire", input.fire),
                ("ads", input.ads),
                ("crouch", input.crouch),
                ("sprint", input.sprint),
            ] {
                assert_eq!(json!(v), s[name]);
            }
            assert_eq!(json!(input.weapon), s["weapon"]);
        } else {
            assert!(source["input"].is_null());
        }
    }
    assert_eq!(
        m.support.strikes.len(),
        expected["strikes"].as_array().unwrap().len()
    );
    for (s, row) in m
        .support
        .strikes
        .iter()
        .zip(expected["strikes"].as_array().unwrap())
    {
        assert_eq!(json!(s.kind.name()), row["kind"]);
        assert_eq!(json!(s.owner), row["owner"]);
        assert_eq!(json!(s.live), row["live"]);
        for (name, n) in [
            ("x", s.at.x),
            ("z", s.at.z),
            ("dir", s.direction),
            ("t", s.delay),
            ("left", s.left),
            ("next", s.next),
        ] {
            assert_eq!(bits(n), bits(row[name].as_f64().unwrap()));
        }
    }
    assert!(expected["smokes"].as_array().unwrap().is_empty() && m.world.game.smokes.is_empty());
    assert!(expected["covers"].as_array().unwrap().is_empty() && m.support.covers.is_empty());
    assert!(
        expected["shots"].as_array().unwrap().is_empty()
            && m.firing.events.is_empty()
            && m.support.events.is_empty()
    );
    // Source story storage is lazy. Absence denotes the same untouched zero counters.
    let rows = expected["story"].as_array().unwrap();
    if rows.is_empty() {
        assert!(
            m.story
                .iter()
                .all(|s| s.mp_spent == 0.0 && s.support_calls == 0.0)
        );
    } else {
        assert_eq!(m.story.len(), rows.len());
        for (s, row) in m.story.iter().zip(rows) {
            assert_eq!(bits(s.mp_spent), row["mpSpent"]);
            assert_eq!(bits(s.support_calls), row["supportCalls"]);
        }
    }
}

#[test]
fn mixed_original_handlers_match_canonical_metadata() {
    let f = fixture();
    let mut m = make(&f);
    boundary(&mut m, &f, 0);
    compare(&m, &f["initial"]);
    let mut tick = None;
    for row in f["checkpoints"].as_array().unwrap() {
        let k = key(row);
        if tick != Some(k.tick) {
            tick = Some(k.tick);
            boundary(&mut m, &f, k.tick - 1);
        }
        let before_seq = row["operation"]["journalId"].as_u64().and_then(|id| {
            m.companions
                .get(&format!("journal-companion:{id}"))
                .map(|op| op.last_seq)
        });
        let denial = match decode(row) {
            Ok(cmd) => m.execute_command(k.issuer as usize, &cmd, false).unwrap(),
            Err(d) => Some(d),
        };
        assert_eq!(denial_value(denial), expected_denial(row), "{:?}", k);
        compare(&m, &row["expected"]);
        if row["operation"]["kind"] == "trusted_input" && row["sourceResult"] == false {
            let id = row["operation"]["journalId"].as_u64().unwrap();
            assert_eq!(
                m.companions
                    .get(&format!("journal-companion:{id}"))
                    .unwrap()
                    .last_seq,
                before_seq.unwrap()
            );
        }
    }
    assert_eq!(m.companions.get("journal-companion:7").unwrap().id, 1);
    assert_eq!(m.companions.get("journal-companion:3").unwrap().id, 2);
}
#[test]
fn mixed_queue_arrival_order_keeps_source_dispatch_results() {
    let f = fixture();
    let rows = f["checkpoints"].as_array().unwrap();
    for reverse in [false, true] {
        let mut m = make(&f);
        for tick in 0..=3 {
            boundary(&mut m, &f, tick);
            let group: Vec<_> = rows
                .iter()
                .filter(|r| r["key"]["tick"].as_u64() == Some(tick))
                .collect();
            let mut admitted = Vec::new();
            let mut parser_denials = Vec::new();
            for row in &group {
                match decode(row) {
                    Ok(cmd) => admitted.push((key(row), cmd)),
                    Err(d) => {
                        assert_eq!(denial_value(Some(d)), expected_denial(row));
                        parser_denials.push(key(row));
                    }
                }
            }
            if reverse {
                admitted.reverse();
            }
            for (k, cmd) in admitted {
                m.enqueue_command(k, cmd).unwrap();
            }
            let result = m.apply_commands(tick + 1, false).unwrap();
            for r in result {
                let source = group.iter().find(|s| key(s) == r.key).unwrap();
                assert_eq!(denial_value(r.denial), expected_denial(source));
            }
            assert_eq!(
                parser_denials.len(),
                group.iter().filter(|r| decode(r).is_err()).count()
            );
            compare(&m, &group.last().unwrap()["expected"]);
        }
    }
}
