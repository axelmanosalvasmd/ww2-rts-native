//! Original private FPS phases, replayed against canonical Match rows.
//! Dormant ordinary fields outside the declared projection and the numeric carrier
//! ID are not represented by this bounded phase. Riding compares its canonical boolean meaning. Source
//! command routes use the canonical actuator; server clock gates remain source-only proof.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use ww2_core::{
    companions::{Admission, EDGE_NAMES, RoleMessage, role_allows},
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{MoveOutcome, UnitControl},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/fps-runtime.json")).unwrap()
}
fn n(v: &Value) -> f64 {
    v.as_f64().expect("finite fixture number")
}
fn id(v: &Value) -> u32 {
    v.as_u64().expect("fixture identity") as u32
}
fn point(p: Point) -> Value {
    json!({"x":p.x,"z":p.z})
}
fn optional_point(p: Option<Point>) -> Value {
    p.map_or(Value::Null, point)
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(_) => n(v) != 0.0,
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
fn equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, a)| b.get(k).is_some_and(|b| equivalent(a, b)))
        }
        _ => a == b,
    }
}
fn apply_delta(state: &mut Value, delta: &Value) {
    for change in delta.as_array().unwrap() {
        let change = change.as_array().unwrap();
        let path = change[0].as_array().unwrap();
        if path.is_empty() {
            *state = change[1].clone();
            continue;
        }
        let mut at = &mut *state;
        for key in &path[..path.len() - 1] {
            at = if let Some(key) = key.as_str() {
                at.get_mut(key).unwrap()
            } else {
                at.get_mut(key.as_u64().unwrap() as usize).unwrap()
            };
        }
        let key = path.last().unwrap();
        if change.len() == 1 {
            at.as_object_mut().unwrap().remove(key.as_str().unwrap());
        } else if let Some(key) = key.as_str() {
            at.as_object_mut()
                .unwrap()
                .insert(key.to_owned(), change[1].clone());
        } else {
            at[key.as_u64().unwrap() as usize] = change[1].clone();
        }
    }
}
fn numeric_bits(v: &Value, path: &str, out: &mut BTreeMap<String, String>) {
    match v {
        Value::Number(v) => {
            let number = v.as_f64().unwrap();
            assert!(number.is_finite());
            out.insert(path.to_owned(), format!("{:016x}", number.to_bits()));
        }
        Value::Array(v) => {
            for (i, v) in v.iter().enumerate() {
                numeric_bits(v, &format!("{path}.{i}"), out);
            }
        }
        Value::Object(v) => {
            for (k, v) in v {
                numeric_bits(
                    v,
                    &if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    },
                    out,
                );
            }
        }
        _ => {}
    }
}
fn compare_row(actual: &Value, expected: &Value, bits: &Value, context: &str, skip: &[&str]) {
    let projected: serde_json::Map<String, Value> = expected
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, _)| !skip.contains(&k.as_str()))
        .map(|(k, _)| {
            (
                k.clone(),
                actual
                    .get(k)
                    .unwrap_or_else(|| panic!("{context}: represented field {k} absent"))
                    .clone(),
            )
        })
        .collect();
    let expected: serde_json::Map<String, Value> = expected
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, _)| !skip.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let projected = Value::Object(projected);
    let expected = Value::Object(expected);
    assert!(
        equivalent(&projected, &expected),
        "{context}: actual {projected}, expected {expected}"
    );
    let mut actual_bits = BTreeMap::new();
    numeric_bits(&projected, "", &mut actual_bits);
    for (path, expected) in bits.as_object().unwrap() {
        if skip.contains(&path.split('.').next().unwrap()) {
            continue;
        }
        assert_eq!(
            actual_bits.get(path).map(String::as_str),
            expected.as_str(),
            "{context}: exact float {path}"
        );
    }
}
fn native_match(f: &Value, initial: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&f["map"]).unwrap()).unwrap();
    let config = initial["players"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| PlayerConfig {
            name: format!("Player {}", p["slot"]),
            team: n(&p["team"]) as i32,
            faction: Faction::try_from(n(&p["faction"]) as u8).unwrap(),
        })
        .collect();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 1,
            mode: Mode::Default,
            players: config,
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
        },
    )
    .unwrap();
    m.world.game.units.clear();
    m.commands.units.clear();
    m.firing.units.clear();
    m.recruit_metadata.clear();
    m.world.game.flags = f["terrain"]["flags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| n(v) as u16)
        .collect();
    m.world.game.height = f["terrain"]["height"]
        .as_array()
        .map(|v| v.iter().map(|v| n(v) as i8).collect());
    for raw in initial["units"].as_array().unwrap() {
        let unit = simtest::load::unit(
            raw,
            &m.rules,
            m.start.players.len(),
            m.world.game.w * m.world.game.h,
        )
        .unwrap();
        let mut control = UnitControl::new(&unit, &m.rules).unwrap();
        control.aim = n(&raw["aim"]);
        control.auto_cast = truthy(&raw["auto"]);
        control.auto_retreat = truthy(&raw["autoRetreat"]);
        control.hold_fire = truthy(&raw["holdFire"]);
        m.commands.insert(unit.id, control);
        let firing = m.firing.initialize(unit.id);
        firing.cooldown = n(&raw["cooldown"]);
        firing.target_id = id(&raw["targetId"]);
        firing.retarget = n(&raw["retarget"]);
        m.world.game.units.insert(unit.id, unit);
    }
    for p in initial["players"].as_array().unwrap() {
        let owner = p["slot"].as_u64().unwrap() as usize;
        m.start.players[owner].spawn = Point {
            x: n(&p["spawn"]["x"]),
            z: n(&p["spawn"]["z"]),
        };
        m.start.economy.players[owner].mp = n(&p["mp"]);
        m.start.economy.players[owner].out = p["out"].as_bool().unwrap();
        m.start.economy.players[owner].away = p["away"].as_bool().unwrap();
    }
    m.next_unit_id = id(&initial["nextId"]);
    m.world.game.tick = n(&initial["tick"]) as i64;
    m.grid.rebuild(&m.world.game.units);
    m
}
fn unit_row(m: &Match, id: u32) -> Value {
    let u = &m.world.game.units[&id];
    let c = &m.commands.units[&id];
    let follow = m
        .companions
        .records
        .get(&c.follow)
        .map_or(json!(0), |op| json!(op.key));
    let paths: Vec<_> = u.path.iter().copied().map(point).collect();
    let mut row = json!({"id":u.id,"type":m.rules.kinds[u.kind].name,"owner":u.owner,"x":u.x,"z":u.z,"rot":u.rot,"aim":c.aim,"moveSpeed":u.move_speed,"vx":u.vx,"vz":u.vz,"hp":u.hp,"supp":u.supp,"auto":c.auto_cast,"autoRetreat":c.auto_retreat,"holdFire":c.hold_fire,"holdPos":u.hold_pos,"follow":follow,"riding":u.riding,"garrison":u.garrison.map_or(-1,|c|c as i64),"retreating":u.retreating,"path":paths,"orders":[],"amove":optional_point(u.amove),"attackId":c.attack_id,"worldGoal":optional_point(u.world_goal),"pace":u.pace,"traffic":null,"trafficWait":u.traffic_wait,"entrench":null,"face":null,"moveOutcome":c.outcome.map(|o|match o {MoveOutcome::Interrupted=>"interrupted",MoveOutcome::Moving=>"moving",MoveOutcome::Unreachable=>"unreachable",MoveOutcome::Arrived=>"arrived"}),"moveOutcomeTick":c.outcome_tick});
    assert!(
        c.orders.is_empty(),
        "queued orders need the command actuator projection"
    );
    let firing = m.firing.get(id).unwrap();
    row["cooldown"] = json!(firing.cooldown);
    row["targetId"] = json!(firing.target_id);
    row["retarget"] = json!(firing.retarget);
    if let Some(body) = m.companions.soldiers.get(&id) {
        let op = &m.companions.records[&body.companion];
        for (k, v) in [
            ("operative", json!(op.key)),
            ("vy", json!(body.vy)),
            ("jy", json!(body.jy)),
            ("crouch", json!(body.crouch)),
            ("weapon", json!(body.weapon)),
            ("mags", json!(body.mags)),
            ("reloadT", json!(body.reload)),
            ("nades", json!(body.nades)),
            ("nadeT", json!(body.nade_time)),
        ] {
            row[k] = v;
        }
    }
    row
}
fn companion_row(op: &ww2_core::companions::Companion) -> Value {
    let mut seen = serde_json::Map::new();
    let mut want = serde_json::Map::new();
    for (i, name) in EDGE_NAMES.iter().enumerate() {
        if let Some(n) = op.seen[i] {
            seen.insert((*name).to_owned(), json!(n));
        }
        if op.want[i] {
            want.insert((*name).to_owned(), json!(true));
        }
    }
    let input=op.input.as_ref().map(|i|json!({"forward":i.forward,"strafe":i.strafe,"yaw":i.yaw,"pitch":i.pitch,"fire":i.fire,"ads":i.ads,"crouch":i.crouch,"sprint":i.sprint,"weapon":i.weapon}));
    json!({"key":op.key,"owner":op.owner,"unitId":op.unit_id,"squadId":op.squad_id,"respawnAt":op.respawn_at,"online":op.online,"input":input,"lastInput":op.last_input,"lastSeq":op.last_seq,"shots":op.shots,"hits":op.hits,"kills":op.kills,"seen":seen,"want":want})
}
fn compare(m: &Match, expected: &Value, winner: &Value, context: &str, f: &Value) {
    compare_row(
        &json!({"tick":m.world.game.tick,"winner":winner,"nextId":m.next_unit_id}),
        &json!({"tick":expected["tick"],"winner":expected["winner"],"nextId":expected["nextId"]}),
        &expected["floatBits"]["game"],
        context,
        &[],
    );
    assert_eq!(
        json!(m.world.game.units.keys().copied().collect::<Vec<_>>()),
        expected["unitOrder"],
        "{context}: insertion order"
    );
    // Query the maintained index itself. Do not rebuild here: publication,
    // retirement and movement must have applied the correct source updates.
    let units = &m.world.game.units;
    for probe in expected["grid"]["queries"].as_array().unwrap() {
        let at = (n(&probe["x"]), n(&probe["z"]));
        let radius = n(&probe["radius"]);
        assert_eq!(
            json!(m.grid.in_order(units, at, radius, |_, _| true)),
            probe["inOrder"],
            "{context}: ordered grid {at:?}/{radius}"
        );
        assert_eq!(
            json!(m.grid.unordered(units, at, radius, |_, _| true)),
            probe["unordered"],
            "{context}: unordered grid {at:?}/{radius}"
        );
        assert_eq!(
            json!(m.grid.heavy(units, at, radius, |_, _| true)),
            probe["heavy"],
            "{context}: heavy grid {at:?}/{radius}"
        );
    }
    for owner in expected["grid"]["owned"].as_array().unwrap() {
        let slot = n(&owner["owner"]) as i32;
        assert_eq!(
            json!(m.grid.owned_by(units, slot)),
            owner["ids"],
            "{context}: owner {slot} grid"
        );
    }
    let unit_keys = f["unitKeys"].as_array().unwrap();
    for (i, raw) in expected["units"].as_array().unwrap().iter().enumerate() {
        let id = id(&raw["id"]);
        let actual = unit_row(m, id);
        let projected: serde_json::Map<String, Value> = raw
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, _)| unit_keys.iter().any(|v| v.as_str() == Some(k.as_str())))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut source = Value::Object(projected);
        if let Some(r) = source.get_mut("riding") {
            *r = json!(truthy(r));
        }
        let mut bits = expected["floatBits"]["units"][i]["fields"].clone();
        bits.as_object_mut().unwrap().retain(|k, _| {
            unit_keys
                .iter()
                .any(|v| v.as_str() == Some(k.split('.').next().unwrap()))
                && k != "riding"
        });
        let skip: Vec<&str> = Vec::new();
        compare_row(
            &actual,
            &source,
            &bits,
            &format!("{context}: unit {id}"),
            &skip,
        );
    }
    assert_eq!(
        m.companions.records.len(),
        expected["operatives"].as_array().unwrap().len(),
        "{context}: operative count"
    );
    for (i, op) in m.companions.records.values().enumerate() {
        compare_row(
            &companion_row(op),
            &expected["operatives"][i],
            &expected["floatBits"]["operatives"][i]["fields"],
            &format!("{context}: companion {}", op.key),
            &[],
        );
    }
    for (i, p) in m.start.economy.players.iter().enumerate() {
        let source = &expected["players"][i];
        let start = &m.start.players[i];
        let actual = json!({"slot":i,"team":p.team,"faction":start.config.faction as u8,"mp":p.mp,"out":p.out,"away":p.away,"spawn":point(start.spawn)});
        compare_row(
            &actual,
            source,
            &expected["floatBits"]["players"][i]["fields"],
            &format!("{context}: player {i}"),
            &[],
        );
    }
}
fn patch_unit(m: &mut Match, id: u32, fields: &Value) {
    for (k, v) in fields.as_object().unwrap() {
        let u = m.world.game.units.get_mut(&id).unwrap();
        match k.as_str() {
            "x" => u.x = n(v),
            "z" => u.z = n(v),
            "rot" => u.rot = n(v),
            "hp" => u.hp = n(v),
            "supp" => u.supp = n(v),
            "vx" => u.vx = Some(n(v)),
            "vz" => u.vz = Some(n(v)),
            "riding" => u.riding = truthy(v),
            "garrison" => u.garrison = (n(v) >= 0.0).then_some(n(v) as usize),
            "retreating" => u.retreating = truthy(v),
            "follow" => {
                m.commands.units.get_mut(&id).unwrap().follow = if let Some(key) = v.as_str() {
                    m.companions.get(key).unwrap().id
                } else {
                    id_value(v)
                }
            }
            "cooldown" => m.firing.get_mut(id).unwrap().cooldown = n(v),
            "nadeT" => m.companions.soldiers.get_mut(&id).unwrap().nade_time = n(v),
            "mags" => {
                m.companions.soldiers.get_mut(&id).unwrap().mags =
                    [id_value(&v[0]), id_value(&v[1])]
            }
            _ => panic!("unhandled trusted fixture unit field {k}"),
        }
    }
    m.grid.update(id, &m.world.game.units[&id]);
}
fn id_value(v: &Value) -> u32 {
    id(v)
}

#[test]
fn original_fps_phase_replay() {
    let f = fixture();
    assert_eq!(f["schema"], 2);
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    let mut tested = 0;
    for trace in f["traces"].as_array().unwrap() {
        let name = trace["name"].as_str().unwrap();
        let mut expected = trace["initial"].clone();
        let mut winner = expected["winner"].clone();
        let mut m = native_match(&f, &expected);
        compare(&m, &expected, &winner, name, &f);
        for (i, action) in trace["operations"].as_array().unwrap().iter().enumerate() {
            apply_delta(&mut expected, &action["delta"]);
            let key = action["key"].as_str().unwrap_or("friend");
            let decided = !winner.is_null();
            match action["op"].as_str().unwrap() {
                "join" => {
                    let actual = m
                        .join_companion(key, action["owner"].as_u64().unwrap() as usize, decided)
                        .unwrap();
                    assert!(
                        equivalent(&json!(actual), &action["result"]),
                        "{name}/{i}: join result"
                    );
                }
                "input" => assert_eq!(
                    m.companion_input(key, &action["input"], decided),
                    action["result"].as_bool().unwrap(),
                    "{name}/{i}: input acceptance"
                ),
                "patch_unit" => patch_unit(&mut m, id(&action["id"]), &action["fields"]),
                "patch_op" => {
                    for (k, v) in action["fields"].as_object().unwrap() {
                        match k.as_str() {
                            "online" => {
                                m.companions.get_mut(key).unwrap().online = v.as_bool().unwrap()
                            }
                            _ => panic!("unhandled operative patch {k}"),
                        }
                    }
                }
                "patch_player" => {
                    let p =
                        &mut m.start.economy.players[action["owner"].as_u64().unwrap() as usize];
                    for (k, v) in action["fields"].as_object().unwrap() {
                        match k.as_str() {
                            "mp" => p.mp = n(v),
                            "out" => p.out = v.as_bool().unwrap(),
                            "away" => p.away = v.as_bool().unwrap(),
                            _ => panic!("unhandled player patch {k}"),
                        }
                    }
                }
                "set_tick" => m.world.game.tick = n(&action["tick"]) as i64,
                "set_winner" => winner = action["winner"].clone(),
                "step" => {
                    m.world.game.tick += 1;
                    m.grid.rebuild(&m.world.game.units);
                    let unit = m.companions.get(key).unwrap().unit_id;
                    if unit != 0 {
                        let result = m.step_companion(unit, decided).unwrap();
                        assert!(
                            result.combat.is_none(),
                            "{name}/{i}: unsupported combat trigger"
                        );
                    }
                }
                "lead" => m
                    .lead_companion_squad(m.companions.get(key).unwrap().unit_id, decided)
                    .unwrap(),
                "command" => {
                    let command =
                        ww2_core::match_commands::Command::from_json(&action["command"]).unwrap();
                    let result = m
                        .execute_command(
                            action["owner"].as_u64().unwrap() as usize,
                            &command,
                            decided,
                        )
                        .unwrap();
                    assert!(
                        equivalent(&json!(result.map(|d| d.name())), &action["result"]),
                        "{name}/{i}: command denial"
                    );
                }
                "pre_respawn" => {
                    m.respawn_companions(decided).unwrap();
                }
                "death" => {
                    let unit = m.companions.get(key).unwrap().unit_id;
                    assert!(m.companion_died(unit).unwrap());
                    m.world.game.units.remove(&unit);
                    m.commands.units.remove(&unit);
                    m.firing.remove(unit);
                }
                "reset" => m
                    .companions
                    .get_mut(key)
                    .unwrap()
                    .reconnect(action["online"].as_bool().unwrap()),
                "pick" => m
                    .pick_companion_squad(m.companions.get(key).unwrap().unit_id)
                    .unwrap(),
                "terrain" => {
                    if action["height"] == true && m.world.game.height.is_none() {
                        m.world.game.height = Some(vec![0; m.world.game.w * m.world.game.h]);
                    }
                    for row in action["cells"].as_array().unwrap() {
                        let c = row[0].as_u64().unwrap() as usize;
                        m.world.game.flags[c] = n(&row[1]) as u16;
                        if !row[2].is_null()
                            && let Some(h) = &mut m.world.game.height
                        {
                            h[c] = n(&row[2]) as i8;
                        }
                    }
                }
                operation => panic!("unhandled operation {operation}"),
            }
            compare(
                &m,
                &expected,
                &winner,
                &format!("{name}/{i}/{}", action["op"]),
                &f,
            );
            tested += 1;
        }
    }
    assert_eq!(tested, 536);
}

#[test]
fn original_admission_and_role_capabilities() {
    let f = fixture();
    for case in f["permissions"]["admission"].as_array().unwrap() {
        let commander = case["commander"].as_f64().unwrap();
        let admission = Admission {
            human_commander: commander == 0.0 && !case["ai"].as_bool().unwrap(),
            identity_present: !case["token"].as_str().unwrap().is_empty(),
            companions_in_room: case["count"].as_u64().unwrap() as usize,
            world_mode: case["mode"] == "world",
        };
        assert_eq!(
            admission.allowed(),
            case["accepted"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
    for case in f["permissions"]["whitelist"].as_array().unwrap() {
        let message = match case["t"].as_str().unwrap() {
            "fps" => RoleMessage::Input,
            "resync" => RoleMessage::Resync,
            "name" => RoleMessage::Name,
            "cmd" => RoleMessage::ArmyOrder,
            "addAi" => RoleMessage::HostAction,
            "team" => RoleMessage::SeatChange,
            t => panic!("unhandled source message {t}"),
        };
        // The blocked FPS rows exercise caller-owned room and clock gates.
        // Compare role capability against original successful dispatch only when
        // those transport gates are open; retain the other rows as source proof.
        let transport_open = case["t"] != "fps"
            || (case["state"] == "play"
                && case["game"] == true
                && case["pause"] == false
                && n(&case["now"]) - n(&case["inputAt"]) >= 20.0);
        if transport_open {
            let calls = case["expected"]["calls"].as_array().unwrap();
            let admitted = !calls.is_empty() || case["expected"]["full"] == true;
            assert_eq!(role_allows(message), admitted, "{}", case["name"]);
        }
    }
}

// Debug rows cover all dormant fields. Explicit bits also preserve NaN payloads
// and signed zero in the numeric state these guarded operations could mutate.
fn guard_state(m: &Match) -> String {
    let mut floats = Vec::new();
    for u in m.world.game.units.values() {
        floats.extend(
            [
                u.x,
                u.z,
                u.rot,
                u.hp,
                u.supp,
                u.move_speed,
                u.vx.unwrap_or(f64::NAN),
                u.vz.unwrap_or(f64::NAN),
                u.pace,
                u.stuck,
                u.repath,
            ]
            .map(f64::to_bits),
        );
    }
    for c in m.commands.units.values() {
        floats.push(c.aim.to_bits());
    }
    for body in m.companions.soldiers.values() {
        floats.extend([body.vy, body.jy, body.reload, body.nade_time].map(f64::to_bits));
    }
    for firing in m.firing.units.values() {
        floats.extend([firing.cooldown, firing.retarget].map(f64::to_bits));
    }
    for op in m.companions.records.values() {
        if let Some(input) = &op.input {
            floats.extend([input.forward, input.strafe, input.yaw, input.pitch].map(f64::to_bits));
        }
    }
    for p in &m.start.economy.players {
        floats.extend([p.mp, p.mun, p.fuel, p.vp, p.inc].map(f64::to_bits));
    }
    let units = &m.world.game.units;
    let grid: Vec<_> = [(16.0, 16.0), (32.0, 32.0)]
        .into_iter()
        .flat_map(|at| {
            [0.0, 15.0, 80.0].into_iter().map(move |radius| {
                (
                    m.grid.in_order(units, at, radius, |_, _| true),
                    m.grid.unordered(units, at, radius, |_, _| true),
                    m.grid.heavy(units, at, radius, |_, _| true),
                )
            })
        })
        .collect();
    let owned: Vec<_> = (0..m.start.players.len())
        .map(|owner| m.grid.owned_by(units, owner as i32))
        .collect();
    format!(
        "{:?}",
        (
            &m.world.game.units,
            &m.commands.units,
            &m.companions,
            (&m.firing, &m.combat),
            &m.start.economy.players,
            m.next_unit_id,
            m.world.game.tick,
            &m.world.game.flags,
            &m.world.game.height,
            &m.recruit_metadata,
            floats,
            (grid, owned)
        )
    )
}
fn guarded_match(f: &Value) -> (Match, u32) {
    let mut m = native_match(f, &f["traces"][0]["initial"]);
    let unit = m.join_companion("guard", 0, false).unwrap().unwrap();
    let op = m.companions.get_mut("guard").unwrap();
    op.input = Some(ww2_core::companions::Input {
        forward: 1.0,
        strafe: 0.0,
        yaw: 0.0,
        pitch: 0.0,
        fire: false,
        ads: false,
        crouch: false,
        sprint: false,
        weapon: 0,
    });
    op.last_input = 0;
    op.last_seq = 7;
    op.seen = [Some(0); 4];
    op.want = [true; 4];
    let body = m.companions.soldiers.get_mut(&unit).unwrap();
    body.reload = 0.25;
    body.nade_time = 0.25;
    m.firing.get_mut(unit).unwrap().cooldown = 0.25;
    m.world.game.units.get_mut(&unit).unwrap().supp = 4.0;
    (m, unit)
}
#[test]
fn invalid_fps_runtime_rejects_before_any_mutation() {
    use ww2_core::companions::{RESPAWN_TICKS, SAFE_INTEGER};
    let f = fixture();
    for fault in [
        "short-flags",
        "short-height",
        "stored-weapon",
        "stored-aim-nan",
        "unit-coordinate-nan",
        "unit-coordinate-outside",
        "tick-exhausted",
    ] {
        let (mut m, unit) = guarded_match(&f);
        match fault {
            "short-flags" => {
                m.world.game.flags.pop();
            }
            "short-height" => m.world.game.height = Some(vec![0]),
            "stored-weapon" => {
                m.companions
                    .get_mut("guard")
                    .unwrap()
                    .input
                    .as_mut()
                    .unwrap()
                    .weapon = 2
            }
            "stored-aim-nan" => {
                m.companions
                    .get_mut("guard")
                    .unwrap()
                    .input
                    .as_mut()
                    .unwrap()
                    .yaw = f64::from_bits(0x7ff8000000000123)
            }
            "unit-coordinate-nan" => {
                m.world.game.units.get_mut(&unit).unwrap().x = f64::from_bits(0x7ff8000000000456)
            }
            "unit-coordinate-outside" => m.world.game.units.get_mut(&unit).unwrap().z = -0.5,
            "tick-exhausted" => m.world.game.tick = SAFE_INTEGER - RESPAWN_TICKS + 1,
            _ => unreachable!(),
        }
        let before = guard_state(&m);
        assert!(
            m.step_companion(unit, false).is_err(),
            "{fault}: invalid phase admitted"
        );
        assert_eq!(
            guard_state(&m),
            before,
            "{fault}: rejected phase changed state"
        );
    }
    let (mut m, unit) = guarded_match(&f);
    let before = guard_state(&m);
    assert!(
        m.companion_died(unit).is_err(),
        "a living soldier cannot retire"
    );
    assert_eq!(guard_state(&m), before, "living retirement changed state");
    m.world.game.units.get_mut(&unit).unwrap().hp = 0.0;
    m.world.game.tick = SAFE_INTEGER - RESPAWN_TICKS + 1;
    let before = guard_state(&m);
    assert!(
        m.prepare_companion_deaths(&[unit]).is_err(),
        "unsafe delay must fail during preflight"
    );
    assert_eq!(guard_state(&m), before, "death preflight changed state");
    assert!(
        m.world.game.units.contains_key(&unit),
        "preflight must precede canonical removal"
    );
}

#[test]
fn operative_tables_match_the_pinned_source() {
    use ww2_core::companions::{OPERATIVE as r, RESPAWN_TICKS, WEAPONS};
    let f = fixture();
    let rules = json!({"hp":r.hp,"eye":r.eye,"crouchEye":r.crouch_eye,"respawn":r.respawn,
        "cost":r.cost,"nades":r.nades,"nadeCd":r.nade_cd,"swap":r.swap,"walk":r.walk,
        "sprint":r.sprint,"crouch":r.crouch,"ads":r.ads,"accel":r.accel,"airAccel":r.air_accel,
        "jump":r.jump,"gravity":r.gravity});
    assert!(equivalent(&rules, &f["constants"]["operative"]));
    let weapons: Vec<_> = WEAPONS
        .iter()
        .map(|w| {
            json!({"id":w.id,"name":w.name,"mag":w.mag,
        "interval":w.interval,"reload":w.reload,"range":w.range,"damage":w.damage,"veh":w.veh,
        "supp":w.supp,"hip":w.hip,"ads":w.ads,"fx":w.fx})
        })
        .collect();
    assert!(equivalent(&json!(weapons), &f["constants"]["weapons"]));
    assert_eq!(
        RESPAWN_TICKS as f64,
        (r.respawn / n(&f["constants"]["tick"])).ceil()
    );
}

#[test]
fn exhausted_respawn_batch_preserves_squad_wallet_and_publication_state() {
    use ww2_core::{companions::Input, match_runtime::RecruitMetadata};
    let f = fixture();
    let (mut m, first) = guarded_match(&f);
    let second = m.join_companion("wallet", 0, false).unwrap().unwrap();
    let companion = m.companions.get("guard").unwrap().id;
    let squad = m
        .world
        .game
        .units
        .values()
        .find(|u| {
            u.owner == 0 && m.rules.kinds[u.kind].infantry && !m.commands.units[&u.id].operative
        })
        .unwrap()
        .id;
    m.world.game.units.get_mut(&squad).unwrap().hp = 40.0;
    m.commands.units.get_mut(&squad).unwrap().follow = companion;
    m.companions.get_mut("guard").unwrap().squad_id = squad;
    m.recruit_metadata.insert(
        squad,
        RecruitMetadata {
            building: 7,
            initial_rally: None,
            initial_route_status: None,
        },
    );
    for unit in [first, second] {
        m.world.game.units.get_mut(&unit).unwrap().hp = 0.0;
    }
    m.prepare_companion_deaths(&[first, second]).unwrap();
    for unit in [first, second] {
        assert!(m.companion_died(unit).unwrap());
        m.world.game.units.remove(&unit);
        m.commands.units.remove(&unit);
        m.firing.remove(unit);
        m.recruit_metadata.remove(&unit);
    }
    for (key, weapon) in [("guard", 1), ("wallet", 0)] {
        let op = m.companions.get_mut(key).unwrap();
        op.input = Some(Input {
            forward: 0.0,
            strafe: 0.0,
            yaw: 0.25,
            pitch: 0.0,
            fire: false,
            ads: false,
            crouch: false,
            sprint: false,
            weapon,
        });
    }
    m.world.game.tick = m.companions.get("guard").unwrap().respawn_at;
    m.start.economy.players[0].mp = 10.0;
    m.next_unit_id = u32::MAX - 1;
    let before = guard_state(&m);
    let error = m
        .respawn_companions(false)
        .expect_err("second staged identity must exhaust the batch allocator");
    assert!(
        error.contains("unit ID capacity exhausted"),
        "unexpected batch rejection: {error}"
    );
    assert_eq!(
        guard_state(&m),
        before,
        "failed batch changed HP, wallet, input, allocator, grid or metadata"
    );
    assert_eq!(m.world.game.units[&squad].hp.to_bits(), 40.0f64.to_bits());
    assert_eq!(m.start.economy.players[0].mp.to_bits(), 10.0f64.to_bits());
    assert_eq!(m.companions.get("guard").unwrap().unit_id, 0);
    assert_eq!(m.companions.get("wallet").unwrap().unit_id, 0);
}
