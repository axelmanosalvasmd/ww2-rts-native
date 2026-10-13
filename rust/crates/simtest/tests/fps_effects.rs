//! Independent original-source geometry, sampled rays and carried-target generic Hurt.
//! These tests load trusted source rows into canonical Match state and publish
//! fire/grenade effects. They do not schedule complete phases, settle grenades or
//! adopt a hit-direction policy.
//! Dormant ordinary firing fields outside Unit/UnitControl/UnitCombat/Soldier are
//! retained in the source fixture but excluded from the Hurt state projection.
use serde_json::{Value, json};
use ww2_core::{
    combat::DeathImpulse,
    companion_ray,
    companions::{Companion, EDGE_NAMES, Input, Soldier},
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{ModeKind, Point, Smoke},
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_combat::{CombatState, DamageWorld, ExplosionSource},
    match_commands::{Command, MoveOrder, UnitControl},
    match_firing::LaunchEvent,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    random::{MatchRandom, StreamId},
    squad_men,
    weather::WeatherChoice,
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/fps-effects.json")).unwrap()
}
fn n(v: &Value) -> f64 {
    v.as_f64().expect("source finite number")
}
fn id(v: &Value) -> u32 {
    n(v) as u32
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
fn point(v: &Value) -> Point {
    Point {
        x: n(&v["x"]),
        z: n(&v["z"]),
    }
}
fn point_json(p: Point) -> Value {
    json!({"x":p.x,"z":p.z})
}
fn child(path: &str, key: impl std::fmt::Display) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

// JSON's raw numeric value is useful for loading, but signed zero is defined by
// the source bit projection. Restore all finite values before any native setup.
fn restore(v: &mut Value, bits: &Value, path: &str) {
    match v {
        Value::Number(_) => {
            let encoded = bits[path]
                .as_str()
                .unwrap_or_else(|| panic!("missing source bits {path}"));
            let value = f64::from_bits(u64::from_str_radix(encoded, 16).unwrap());
            assert!(value.is_finite());
            *v = json!(value);
        }
        Value::Array(a) => {
            for (i, v) in a.iter_mut().enumerate() {
                restore(v, bits, &child(path, i));
            }
        }
        Value::Object(o) => {
            for (key, v) in o {
                restore(v, bits, &child(path, key));
            }
        }
        _ => {}
    }
}
fn decoded(snapshot: &Value) -> Value {
    let mut raw = snapshot["raw"].clone();
    restore(&mut raw, &snapshot["floatBits"], "");
    raw
}
fn exact(actual: &Value, expected: &Value, context: &str) {
    match (actual, expected) {
        (Value::Number(_), Value::Number(_)) => assert_eq!(
            n(actual).to_bits(),
            n(expected).to_bits(),
            "{context}: numeric bits: {actual} vs {expected}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{context}: array length");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                exact(a, b, &child(context, i));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{context}: field count");
            for (key, a) in a {
                exact(a, b.get(key).unwrap(), &child(context, key));
            }
        }
        _ => assert_eq!(actual, expected, "{context}"),
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
                .insert(key.into(), change[1].clone());
        } else {
            at[key.as_u64().unwrap() as usize] = change[1].clone();
        }
    }
}
fn source_order(raw: &Value) -> Vec<u32> {
    raw["unitOrder"]
        .as_array()
        .unwrap()
        .iter()
        .map(id)
        .collect()
}
fn input(raw: &Value) -> Input {
    Input {
        forward: raw["forward"].as_f64().unwrap_or(0.0),
        strafe: raw["strafe"].as_f64().unwrap_or(0.0),
        yaw: n(&raw["yaw"]),
        pitch: n(&raw["pitch"]),
        fire: truthy(&raw["fire"]),
        ads: truthy(&raw["ads"]),
        crouch: truthy(&raw["crouch"]),
        sprint: truthy(&raw["sprint"]),
        weapon: raw["weapon"].as_f64().unwrap_or(0.0) as usize,
    }
}
fn rle(raw: &Value) -> Vec<i64> {
    raw.as_array()
        .unwrap()
        .iter()
        .flat_map(|run| std::iter::repeat_n(n(&run[0]) as i64, n(&run[1]) as usize))
        .collect()
}

fn native_match(f: &Value, raw: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&f["map"]).unwrap()).unwrap();
    let players = raw["players"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| PlayerConfig {
            name: format!("Source player {}", p["slot"]),
            team: n(&p["team"]) as i32,
            faction: Faction::try_from(n(&p["faction"]) as u8).unwrap(),
        })
        .collect();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 1,
            mode: Mode::Default,
            players,
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
    m.firing = ww2_core::match_firing::FiringState::default();
    m.recruit_metadata.clear();
    m.combat = CombatState::default();
    m.world.game.tick = n(&raw["game"]["tick"]) as i64;
    m.next_unit_id = id(&raw["game"]["nextId"]);
    m.start.environment_seed = id(&raw["game"]["seed"]);
    m.world.game.flags = rle(&raw["terrain"]["flags"])
        .into_iter()
        .map(|v| v as u16)
        .collect();
    m.world.game.height = raw["terrain"]["height"].as_array().map(|_| {
        rle(&raw["terrain"]["height"])
            .into_iter()
            .map(|v| v as i8)
            .collect()
    });
    m.world.game.smokes = raw["terrain"]["smokes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| Smoke {
            id: id(&s["id"]),
            x: n(&s["x"]),
            z: n(&s["z"]),
            r: n(&s["r"]),
            remaining: s["t"].as_f64(),
        })
        .collect();
    for p in raw["players"].as_array().unwrap() {
        let slot = id(&p["slot"]) as usize;
        m.world.game.players[slot].team = n(&p["team"]) as i32;
        let mut visible: Vec<_> = p["visible"].as_array().unwrap().iter().map(id).collect();
        // Source Set membership is unordered. The native membership representation is sorted.
        visible.sort_unstable();
        m.world.game.players[slot].visible = visible;
        m.start.players[slot].spawn = point(&p["spawn"]);
        m.start.economy.players[slot].mp = n(&p["mp"]);
    }
    // Public rows are trusted fixture setup. These tests do not invoke admission by key.
    for (index, op) in raw["operatives"].as_array().unwrap().iter().enumerate() {
        let companion_id = index as u32 + 1;
        let seen = EDGE_NAMES.map(|edge| op["seen"][edge].as_f64().map(|v| v as i64));
        let want = EDGE_NAMES.map(|edge| truthy(&op["want"][edge]));
        m.companions.records.insert(
            companion_id,
            Companion {
                id: companion_id,
                key: op["key"].as_str().unwrap().into(),
                owner: id(&op["owner"]) as usize,
                unit_id: id(&op["unitId"]),
                squad_id: id(&op["squadId"]),
                respawn_at: n(&op["respawnAt"]) as i64,
                online: truthy(&op["online"]),
                input: op["input"].as_object().map(|_| input(&op["input"])),
                last_input: n(&op["lastInput"]) as i64,
                last_seq: n(&op["lastSeq"]) as i64,
                shots: n(&op["shots"]) as u64,
                hits: n(&op["hits"]) as u64,
                kills: n(&op["kills"]) as u64,
                seen,
                want,
            },
        );
    }
    for source in raw["units"].as_array().unwrap() {
        // The general movement loader rejects operative tags. The canonical
        // Soldier and control rows below own that additional body state.
        let mut ordinary = source.clone();
        ordinary.as_object_mut().unwrap().remove("operative");
        let unit = simtest::load::unit(
            &ordinary,
            &m.rules,
            m.world.game.players.len(),
            m.world.game.w * m.world.game.h,
        )
        .unwrap();
        let mut c = UnitControl::new(&unit, &m.rules).unwrap();
        c.aim = n(&source["aim"]);
        c.auto_cast = truthy(&source["auto"]);
        c.auto_retreat = truthy(&source["autoRetreat"]);
        c.hold_fire = truthy(&source["holdFire"]);
        c.attack_id = id(&source["attackId"]);
        c.operative = truthy(&source["operative"]);
        for order in source["orders"].as_array().unwrap() {
            assert_eq!(
                order["t"], "move",
                "fixture queue requires a represented command"
            );
            c.orders.push_back(Command::Move {
                orders: vec![MoveOrder {
                    unit: i64::from(unit.id),
                    at: point(order),
                }],
                control_ids: None,
                attack_move: false,
                queue: true,
                together: false,
                face: None,
            });
        }
        if c.operative {
            let companion = m
                .companions
                .records
                .values()
                .find(|op| op.key == source["operative"].as_str().unwrap())
                .unwrap();
            assert_eq!(companion.unit_id, unit.id);
            m.companions.soldiers.insert(
                unit.id,
                Soldier {
                    companion: companion.id,
                    vy: n(&source["vy"]),
                    jy: n(&source["jy"]),
                    crouch: truthy(&source["crouch"]),
                    weapon: id(&source["weapon"]) as usize,
                    mags: [id(&source["mags"][0]), id(&source["mags"][1])],
                    reload: n(&source["reloadT"]),
                    nades: id(&source["nades"]),
                    nade_time: n(&source["nadeT"]),
                },
            );
        }
        m.commands.insert(unit.id, c);
        let firing = m.firing.initialize(unit.id);
        firing.cooldown = n(&source["cooldown"]);
        firing.target_id = id(&source["targetId"]);
        firing.retarget = n(&source["retarget"]);
        let combat = m.combat.initialize(unit.id);
        combat.xp = n(&source["xp"]);
        combat.last_hit = source["lastHit"].as_f64().map(|v| v as i32);
        combat.hit_by = (id(&source["hitBy"]) != 0).then(|| id(&source["hitBy"]));
        combat.hit_at = n(&source["hitAt"]);
        combat.cargo = source["cargo"].as_array().unwrap().iter().map(id).collect();
        combat.carrier = (id(&source["riding"]) != 0).then(|| id(&source["riding"]));
        combat.death_impulse = source["deathImpulse"].as_object().map(|_| DeathImpulse {
            direction: n(&source["deathImpulse"]["dir"]),
            magnitude: n(&source["deathImpulse"]["impulse"]),
        });
        m.world.game.units.insert(unit.id, unit);
    }
    m.grid = SpatialGrid::new(&m.world.game.units, 16.0);
    m
}

#[test]
fn drawn_squad_slots_match_all_original_cases() {
    let f = fixture();
    let cases = f["menCases"].as_array().unwrap();
    assert_eq!(cases.len(), 39);
    for case in cases {
        let input = decoded(&case["input"]);
        let u = &input["unit"];
        let drawn = squad_men::drawn_men(
            u["type"].as_str().unwrap(),
            n(&u["hp"]),
            n(&input["maxHp"]),
            point(u),
            n(&u["rot"]),
            Point {
                x: n(&u["vx"]),
                z: n(&u["vz"]),
            },
        )
        .unwrap();
        let actual = json!({"drawn":drawn.count,"positions":drawn.positions.into_iter().map(point_json).collect::<Vec<_>>()});
        exact(
            &actual,
            &decoded(&case["expected"]),
            case["name"].as_str().unwrap(),
        );
    }
}

#[test]
fn sampled_rays_match_all_original_endpoints_and_selected_bodies() {
    let f = fixture();
    let traces: Vec<_> = f["traces"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["category"] == "fire")
        .collect();
    assert_eq!(traces.len(), 39);
    let mut draws_used = 0;
    for trace in traces {
        let context = trace["name"].as_str().unwrap();
        let initial = decoded(&trace["initial"]);
        let m = native_match(&f, &initial);
        let operations = trace["operations"].as_array().unwrap();
        assert_eq!(operations.len(), 1, "{context}: direct sampled ray phase");
        let op = &operations[0];
        assert_eq!(op["op"], "fire");
        let mut args = json!({"unit":op["unit"],"input":op["input"],"moving":op["moving"]});
        restore(&mut args, &op["argumentFloatBits"], "");
        let draws = decoded(&op["rng"]);
        assert_eq!(
            draws.as_array().unwrap().len(),
            2,
            "{context}: radius then azimuth draws"
        );
        draws_used += 2;
        let result = companion_ray::trace(
            &m.world.game,
            &m.rules,
            &m.companions.soldiers,
            &source_order(&initial),
            id(&args["unit"]),
            &input(&args["input"]),
            n(&args["moving"]),
            [n(&draws[0]), n(&draws[1])],
        )
        .unwrap_or_else(|e| panic!("{context}: {e}"));
        let mut snapshot = trace["initial"].clone();
        apply_delta(&mut snapshot, &op["delta"]);
        let expected = decoded(&snapshot);
        let shot = expected["shots"].as_array().unwrap().last().unwrap();
        assert!(shot["k"] == "sniper" || shot["k"] == "commando");
        exact(
            &json!({"x":result.end.x,"y":result.end.y,"z":result.end.z}),
            &json!({"x":shot["x"],"y":shot["y"],"z":shot["z"]}),
            context,
        );
        let actual_hit = result
            .enemy_hit
            .map(|hit| json!({"unit":hit.unit,"owner":hit.owner,"man":hit.man}));
        let source_hit = truthy(&shot["hit"]).then(|| json!({"unit":shot["t"],"owner":shot["to"],"man":shot.get("m").unwrap_or(&Value::Null)}));
        exact(&json!(actual_hit), &json!(source_hit), context);
    }
    assert_eq!(draws_used, 78);
}

fn grid_projection(m: &Match) -> Value {
    let units = &m.world.game.units;
    json!({
        "inOrder":m.grid.in_order(units,(40.0,40.0),128.0,|_,_|true),
        "unordered":m.grid.unordered(units,(40.0,40.0),128.0,|_,_|true),
        "heavy":m.grid.heavy(units,(40.0,40.0),128.0,|_,_|true),
    })
}
fn represented_unit(m: &Match, unit: u32, source: &Value) -> Value {
    let u = &m.world.game.units[&unit];
    let c = &m.commands.units[&unit];
    let combat = m.combat.get(unit).unwrap();
    let firing = m.firing.get(unit).unwrap();
    let orders: Vec<_> = c
        .orders
        .iter()
        .map(|command| match command {
            Command::Move { orders, .. } => {
                assert_eq!(orders.len(), 1);
                assert_eq!(orders[0].unit, i64::from(unit));
                json!({"t":"move","x":orders[0].at.x,"z":orders[0].at.z})
            }
            _ => panic!("unrepresented fixture queue"),
        })
        .collect();
    let mut all = json!({
        "id":u.id,"type":m.rules.kinds[u.kind].name,"owner":u.owner,
        "x":u.x,"z":u.z,"rot":u.rot,"aim":c.aim,"moveSpeed":u.move_speed,
        "travelDir":u.travel_dir,"vx":u.vx,"vz":u.vz,"hp":u.hp,"xp":combat.xp,"supp":u.supp,
        "cooldown":firing.cooldown,"shotAt":u.shot_at,"lastHit":combat.last_hit,
        "hitBy":combat.hit_by.unwrap_or(0),"hitAt":combat.hit_at,"hitFrom":u.hit_from.map(point_json),
        "retarget":firing.retarget,"targetId":firing.target_id,"attackId":c.attack_id,
        "garrison":u.garrison.map_or(-1,|v|v as i64),"riding":combat.carrier.unwrap_or(0),
        "cargo":combat.cargo,"retreating":u.retreating,"sprint":u.sprint,"still":u.still,
        "auto":c.auto_cast,"autoRetreat":c.auto_retreat,"holdFire":c.hold_fire,"holdPos":u.hold_pos,
        "path":u.path.iter().copied().map(point_json).collect::<Vec<_>>(),"orders":orders,
        "amove":u.amove.map(point_json),"worldGoal":u.world_goal.map(point_json),"built":u.built,"cells":u.cells,
        "deathImpulse":combat.death_impulse.map(|d|json!({"dir":d.direction,"impulse":d.magnitude})),
    });
    assert_eq!(
        u.riding,
        combat.carrier.is_some(),
        "carrier boolean is canonical"
    );
    if let Some(s) = m.companions.soldiers.get(&unit) {
        let op = &m.companions.records[&s.companion];
        for (key, value) in json!({"operative":op.key,"vy":s.vy,"jy":s.jy,"crouch":s.crouch,
            "weapon":s.weapon,"mags":s.mags,"reloadT":s.reload,"nades":s.nades,"nadeT":s.nade_time})
        .as_object()
        .unwrap()
        {
            all.as_object_mut()
                .unwrap()
                .insert(key.clone(), value.clone());
        }
    }
    let represented = all.as_object().unwrap();
    Value::Object(
        source
            .as_object()
            .unwrap()
            .keys()
            .filter_map(|key| {
                represented
                    .get(key)
                    .map(|value| (key.clone(), value.clone()))
            })
            .collect(),
    )
}
fn compare_hurt_units(m: &Match, expected: &Value, f: &Value) {
    for source in expected["units"].as_array().unwrap() {
        let actual = represented_unit(m, id(&source["id"]), source);
        // Every present declared runtime field is compared. No numeric field in
        // this projection uses a tolerance. Ordinary dormant setup extras stay
        // in the fixture and are intentionally absent from this projection.
        for key in f["unitKeys"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
        {
            if source.get(key).is_some() {
                assert!(
                    actual.get(key).is_some(),
                    "represented Hurt field missing: {key}"
                );
            }
        }
        let expected = Value::Object(
            actual
                .as_object()
                .unwrap()
                .keys()
                .map(|key| (key.clone(), source[key].clone()))
                .collect(),
        );
        exact(&actual, &expected, &format!("hurt unit {}", source["id"]));
    }
}

#[test]
fn generic_hurt_emits_carried_contact_before_disembark_and_preserves_unrelated_state() {
    let f = fixture();
    let traces: Vec<_> = f["traces"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["category"] == "hurt")
        .collect();
    assert_eq!(traces.len(), 1);
    let trace = traces[0];
    let initial = decoded(&trace["initial"]);
    let op = &trace["operations"][0];
    assert_eq!(trace["operations"].as_array().unwrap().len(), 1);
    assert_eq!(op["op"], "hurt");
    assert!(op["rng"]["raw"].as_array().unwrap().is_empty());
    let mut m = native_match(&f, &initial);
    compare_hurt_units(&m, &initial, &f);
    exact(
        &grid_projection(&m),
        &initial["grid"],
        "initial maintained grid",
    );
    let target = id(&op["target"]);
    let carrier = m
        .combat
        .get(target)
        .unwrap()
        .carrier
        .expect("real carried infantry");
    assert!(m.world.game.units[&target].riding);
    assert_eq!(m.combat.get(carrier).unwrap().cargo, vec![target]);
    assert_eq!(
        m.rules.kinds[m.world.game.units[&carrier].kind].name,
        "tank"
    );
    let before_controls = m.commands.units.clone();
    let before_firing = format!("{:?}", m.firing);
    let before_companions = m.companions.clone();
    let before_alloc = (m.next_unit_id, m.start.environment_seed);
    let before_wallet: Vec<_> = m
        .start
        .economy
        .players
        .iter()
        .map(|p| p.mp.to_bits())
        .collect();
    let before_environment = format!("{:?}", m.environment);
    let before_metadata = m.combat.clone();
    let order = source_order(&initial);
    let source = &op["source"];
    let profile = ExplosionSource {
        infantry: n(&source["inf"]),
        vehicle: n(&source["veh"]),
        terrain: source["terrain"].as_f64(),
        suppression: n(&source["supp"]),
        scar: source["scar"].as_f64().unwrap_or(0.0),
        anti_garrison: source["antiGarrison"].as_f64(),
        weapon_kind: None,
        off_map_support: false,
    };
    let (wind_angle, wind_strength) = m.start.wind;
    let hurt = DamageWorld {
        world: &mut m.world,
        combat: &mut m.combat,
        grid: &mut m.grid,
        rules: &m.rules,
        logistics: &m.logistics,
        production: &m.production,
        mode: m.options.mode,
        tech_enabled: m.options.tech,
        environment: &mut m.environment,
        environment_seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &order,
        wind_angle,
        wind_strength,
    }
    .hurt(
        target,
        profile,
        n(&op["fall"]),
        n(&op["owner"]) as i32,
        &mut m.firing,
    )
    .unwrap();
    let mut snapshot = trace["initial"].clone();
    apply_delta(&mut snapshot, &op["delta"]);
    let expected = decoded(&snapshot);
    let event = json!({"t":hurt.unit,"fo":hurt.from.unwrap(),"to":hurt.owner,
        "x":hurt.at.x,"z":hurt.at.z,"k":"hurt","kill":hurt.killed});
    exact(&json!([event]), &expected["shots"], "returned Hurt contact");
    assert!(!hurt.local);
    assert_ne!(
        hurt.at,
        m.world.game.units[&target].at(),
        "contact must precede nearestFree rider relocation"
    );
    compare_hurt_units(&m, &expected, &f);
    // No rebuild here: DamageWorld must update the maintained index when it places the rider.
    exact(
        &grid_projection(&m),
        &expected["grid"],
        "post-hurt maintained grid",
    );
    assert_eq!(
        m.commands.units, before_controls,
        "hurt preserves queued orders and control intent"
    );
    assert_eq!(
        format!("{:?}", m.firing),
        before_firing,
        "hurt does not retarget or tick weapons"
    );
    assert_eq!(
        m.companions, before_companions,
        "plain hurt has no fabricated shooter statistics"
    );
    assert_eq!((m.next_unit_id, m.start.environment_seed), before_alloc);
    assert_eq!(
        m.start
            .economy
            .players
            .iter()
            .map(|p| p.mp.to_bits())
            .collect::<Vec<_>>(),
        before_wallet
    );
    assert_eq!(format!("{:?}", m.environment), before_environment);
    for (id, before) in before_metadata.iter() {
        let after = m.combat.get(id).unwrap();
        assert_eq!(
            after.xp.to_bits(),
            before.xp.to_bits(),
            "plain hurt grants no XP"
        );
        assert_eq!(
            after.death_impulse, before.death_impulse,
            "plain hurt creates no radial DeathImpulse"
        );
        assert_eq!(after.hit_by, before.hit_by);
        assert_eq!(after.hit_at.to_bits(), before.hit_at.to_bits());
        assert_eq!(after.at_hit, before.at_hit);
    }
    // The source mutation tape proves its order. The public DamageWorld adapter
    // exposes the contact and completed mutations, not an intermediate native tape.
    let events = decoded(&op["events"]);
    let ordering: Vec<_> = events
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["actor"].as_str().unwrap(),
                e["field"].as_str().unwrap_or("append"),
            )
        })
        .collect();
    assert_eq!(
        ordering,
        vec![
            ("unit:18", "lastHit"),
            ("unit:18", "hp"),
            ("unit:18", "supp"),
            ("shots", "append"),
            ("unit:17", "cargo"),
            ("unit:18", "riding"),
            ("unit:18", "x"),
            ("unit:18", "z"),
            ("unit:18", "supp")
        ]
    );
    exact(&events[3]["value"], &event, "ordered source Hurt contact");
}

fn seeded_fire_match() -> (Match, Input, u32, u32) {
    let f = fixture();
    let trace = f["traces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "standing-operative-body")
        .unwrap();
    let raw = decoded(&trace["initial"]);
    let mut m = native_match(&f, &raw);
    let shooter = id(&trace["operations"][0]["unit"]);
    let target = m
        .world
        .game
        .units
        .keys()
        .copied()
        .find(|id| *id != shooter)
        .unwrap();
    m.start.random = MatchRandom::new(0xf17e);
    m.companions.soldiers.get_mut(&shooter).unwrap().crouch = true;
    m.combat.get_mut(shooter).unwrap().xp = 17.3;
    m.combat.get_mut(target).unwrap().death_impulse = Some(DeathImpulse {
        direction: 0.2,
        magnitude: 0.8,
    });
    // A standing operative's cylinder remains present at low health. A crouched
    // ADS shot stays inside its body for every spread draw at this distance.
    m.world.game.units.get_mut(&target).unwrap().hp = 1.0;
    let mut aim = input(&trace["operations"][0]["input"]);
    aim.ads = true;
    aim.fire = true;
    (m, aim, shooter, target)
}

#[test]
fn canonical_fire_is_seeded_uses_two_draws_and_publishes_hurt_before_small_arms() {
    for mode in [Mode::Default, Mode::Conquest] {
        // Recorded source tapes validate pure ray arithmetic in the 39-case test.
        // This test validates the real producer with its recorded Combat RNG stream;
        // no test hook substitutes source draws into the runtime.
        let (mut m, aim, shooter, target) = seeded_fire_match();
        m.options.mode = mode;
        let (mut replay, replay_aim, replay_shooter, replay_target) = seeded_fire_match();
        replay.options.mode = mode;
        assert_eq!((replay_shooter, replay_target), (shooter, target));
        let soldier = m.companions.soldiers[&shooter].clone();
        let companion_id = soldier.companion;
        let counts = m.companions.records[&companion_id].clone();
        let xp = m.combat.get(shooter).unwrap().xp.to_bits();
        let impulse = m.combat.get(target).unwrap().death_impulse;
        let target_retarget = m.firing.get(target).unwrap().retarget.to_bits();
        let before_order: Vec<_> = m.world.game.units.keys().copied().collect();
        let mut expected_random = m.start.random.clone();
        let draws = [
            expected_random.next_fraction(StreamId::Combat),
            expected_random.next_fraction(StreamId::Combat),
        ];
        let expected_ray = companion_ray::trace(
            &m.world.game,
            &m.rules,
            &m.companions.soldiers,
            &before_order,
            shooter,
            &aim,
            m.world.game.units[&shooter].move_speed,
            draws,
        )
        .unwrap();
        assert_eq!(
            expected_ray.enemy_hit.unwrap().unit,
            target,
            "seeded fixture must exercise actual damage"
        );
        m.fire_companion(shooter, &aim, false).unwrap();
        replay.fire_companion(shooter, &replay_aim, false).unwrap();
        assert_eq!(
            m.start.random.snapshot(),
            expected_random.snapshot(),
            "exactly two Combat draws and no other stream changes"
        );
        assert_eq!(m.start.random, replay.start.random);
        assert_eq!(m.firing, replay.firing);
        assert_eq!(m.companions, replay.companions);
        assert_eq!(m.combat, replay.combat);
        for id in &before_order {
            // Unit carries dormant undefined-as-NaN fields, so compare its full Debug
            // representation instead of NaN's deliberately false PartialEq.
            assert_eq!(
                format!("{:?}", m.world.game.units[id]),
                format!("{:?}", replay.world.game.units[id])
            );
        }
        exact(
            &grid_projection(&m),
            &grid_projection(&replay),
            "seeded maintained grid",
        );
        let weapon = ww2_core::companions::WEAPONS[soldier.weapon];
        let after = &m.companions.soldiers[&shooter];
        assert_eq!(after.mags[soldier.weapon], soldier.mags[soldier.weapon] - 1);
        assert_eq!(
            after.mags[1 - soldier.weapon],
            soldier.mags[1 - soldier.weapon]
        );
        assert_eq!(after.reload.to_bits(), soldier.reload.to_bits());
        assert_eq!(after.nades, soldier.nades);
        assert_eq!(after.nade_time.to_bits(), soldier.nade_time.to_bits());
        assert_eq!(
            m.firing.get(shooter).unwrap().cooldown.to_bits(),
            weapon.interval.to_bits()
        );
        assert_eq!(
            m.world.game.units[&shooter].shot_at.unwrap().to_bits(),
            (m.world.game.tick as f64).to_bits()
        );
        let op = &m.companions.records[&companion_id];
        assert_eq!(
            (op.shots, op.hits, op.kills),
            (counts.shots + 1, counts.hits + 1, counts.kills + 1)
        );
        assert_eq!(
            m.world.game.units[&target].hp.to_bits(),
            (1.0 - weapon.damage).to_bits()
        );
        assert!(
            m.world.game.units.contains_key(&target),
            "fire does not retire the victim in this phase"
        );
        assert_eq!(m.combat.get(target).unwrap().last_hit, Some(0));
        assert_eq!(m.combat.get(target).unwrap().hit_by, Some(shooter));
        assert_eq!(
            m.combat.get(target).unwrap().hit_at.to_bits(),
            (m.world.game.tick as f64).to_bits()
        );
        assert_eq!(
            m.world.game.units[&target].hit_from,
            Some(m.world.game.units[&shooter].at())
        );
        assert_eq!(
            m.combat.get(shooter).unwrap().xp.to_bits(),
            xp,
            "FPS fire grants no fabricated shooter XP"
        );
        assert_eq!(
            m.combat.get(target).unwrap().death_impulse,
            impulse,
            "FPS fire creates no radial DeathImpulse"
        );
        assert_eq!(
            m.firing.get(target).unwrap().retarget.to_bits(),
            target_retarget
        );
        assert_eq!(m.firing.events.len(), 2);
        match (&m.firing.events[0], &m.firing.events[1]) {
            (
                LaunchEvent::Environment(ww2_core::environment::Effect::Hurt(hurt)),
                LaunchEvent::SmallArms(shot),
            ) => {
                assert_eq!(hurt.unit, target);
                assert_eq!(hurt.at, m.world.game.units[&target].at());
                assert!(hurt.killed);
                assert!(!hurt.local);
                assert_eq!(shot.at, expected_ray.end);
                assert_eq!(shot.effect, weapon.fx);
                assert_eq!((shot.shooter, shot.owner), (shooter, 0));
                let hit = shot.hit.unwrap();
                assert_eq!((hit.target, hit.owner, hit.man), (target, 1, None));
                let projected = m.firing.events[1].to_json(&m.rules);
                exact(
                    &projected,
                    &json!({"k":weapon.fx,"f":shooter,"fo":0,
                "x":expected_ray.end.x,"y":expected_ray.end.y,"z":expected_ray.end.z,
                "t":target,"to":1,"hit":true}),
                    "existing small-arms projection",
                );
            }
            events => panic!("expected Hurt then SmallArms, received {events:?}"),
        }
        assert!(m.firing.flights.is_empty());
        assert!(m.firing.salvos.is_empty());
    }
}

fn guard_state(m: &Match) -> String {
    // Debug retains dormant NaNs without pretending that NaN equals itself.
    // Explicit bits below also retain NaN payloads and every combat/body number.
    let combat_bits: Vec<_> = m
        .combat
        .iter()
        .map(|(id, c)| {
            (
                id,
                c.xp.to_bits(),
                c.hit_at.to_bits(),
                c.at_hit.map(f64::to_bits),
                c.death_impulse
                    .map(|d| (d.direction.to_bits(), d.magnitude.to_bits())),
            )
        })
        .collect();
    let soldier_bits: Vec<_> = m
        .companions
        .soldiers
        .iter()
        .map(|(id, s)| {
            (
                id,
                s.vy.to_bits(),
                s.jy.to_bits(),
                s.reload.to_bits(),
                s.nade_time.to_bits(),
            )
        })
        .collect();
    let units = &m.world.game.units;
    let mut grids = Vec::new();
    for at in [(16.0, 16.0), (32.0, 32.0), (40.0, 40.0)] {
        for radius in [0.0, 15.0, 80.0, 128.0] {
            grids.push((
                m.grid.in_order(units, at, radius, |_, _| true),
                m.grid.unordered(units, at, radius, |_, _| true),
                m.grid.heavy(units, at, radius, |_, _| true),
            ));
        }
    }
    let owned: Vec<_> = (0..m.world.game.players.len())
        .map(|owner| m.grid.owned_by(units, owner as i32))
        .collect();
    format!(
        "{:?}",
        (
            (
                &m.world,
                &m.commands.units,
                &m.combat,
                &m.companions,
                &m.firing
            ),
            (
                &m.environment,
                &m.logistics,
                &m.production,
                &m.start.economy
            ),
            (
                m.start.random.snapshot(),
                m.start.environment_seed,
                m.next_unit_id
            ),
            (combat_bits, soldier_bits, grids, owned)
        )
    )
}

#[test]
fn canonical_fire_rejects_invalid_metadata_and_exhausted_counters_atomically() {
    for failure in [
        "nonfinite source XP",
        "missing target metadata",
        "shots",
        "hits",
        "kills",
        "unsupported mode",
        "internal mode mismatch",
    ] {
        let (mut m, aim, shooter, target) = seeded_fire_match();
        match failure {
            "unsupported mode" => m.options.mode = Mode::World,
            "internal mode mismatch" => m.world.game.mode = Some(ModeKind::Horde),
            "nonfinite source XP" => {
                m.combat.get_mut(shooter).unwrap().xp = f64::from_bits(0x7ff8_0000_0000_0042)
            }
            "missing target metadata" => {
                m.combat.remove(target);
            }
            counter => {
                let companion = m.companions.soldiers[&shooter].companion;
                let op = m.companions.records.get_mut(&companion).unwrap();
                let exhausted = ww2_core::companions::SAFE_INTEGER as u64;
                match counter {
                    "shots" => op.shots = exhausted,
                    "hits" => op.hits = exhausted,
                    "kills" => op.kills = exhausted,
                    _ => unreachable!(),
                }
            }
        }
        let before = guard_state(&m);
        assert!(
            m.fire_companion(shooter, &aim, false).is_err(),
            "{failure}: expected rejection"
        );
        assert_eq!(
            guard_state(&m),
            before,
            "{failure}: rejection changed canonical state/RNG/events/grid"
        );
    }
}

fn point3_json(p: ww2_core::projectiles::Point3) -> Value {
    json!({"x":p.x,"y":p.y,"z":p.z})
}

#[test]
fn companion_grenades_match_source_launches_clamps_and_trajectory_without_spending_fire() {
    let f = fixture();
    let traces: Vec<_> = f["traces"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["category"] == "grenade")
        .collect();
    assert_eq!(traces.len(), 9);
    for mode in [Mode::Default, Mode::Conquest] {
        let mut launches = 0;
        let mut samples = 0;
        for trace in &traces {
            let context = trace["name"].as_str().unwrap();
            let mut snapshot = trace["initial"].clone();
            let initial = decoded(&snapshot);
            let mut m = native_match(&f, &initial);
            // Both ordinary aliases replay the same original source rows.
            m.options.mode = mode;
            m.firing.next_flight = n(&initial["game"]["nextFlight"]) as u64;
            let random = m.start.random.snapshot();
            let economics = m.start.economy.clone();
            let companion_counts = m.companions.records.clone();
            for op in trace["operations"].as_array().unwrap() {
                let mut args = match op["op"].as_str().unwrap() {
                    "throw" => json!({"unit":op["unit"],"input":op["input"]}),
                    "set_tick" => json!({"tick":op["tick"]}),
                    "patch_unit" => json!({"id":op["id"],"fields":op["fields"]}),
                    "sample_flight" => json!({"id":op["id"],"times":op["times"]}),
                    other => panic!("unexpected grenade operation: {other}"),
                };
                restore(&mut args, &op["argumentFloatBits"], "");
                apply_delta(&mut snapshot, &op["delta"]);
                let expected = decoded(&snapshot);
                assert!(decoded(&op["rng"]).as_array().unwrap().is_empty());
                match op["op"].as_str().unwrap() {
                    "throw" => {
                        let unit = id(&args["unit"]);
                        let mut aim = Input {
                            forward: 0.0,
                            strafe: 0.0,
                            yaw: 0.0,
                            pitch: 0.0,
                            fire: false,
                            ads: false,
                            crouch: false,
                            sprint: false,
                            weapon: 0,
                        };
                        aim.yaw = n(&args["input"]["yaw"]);
                        aim.pitch = n(&args["input"]["pitch"]);
                        let old_body = m.companions.soldiers[&unit].clone();
                        let cooldown = m.firing.get(unit).unwrap().cooldown;
                        let retained: Vec<_> = m
                            .firing
                            .flights
                            .iter()
                            .map(|(id, p)| (*id, format!("{p:?}")))
                            .collect();
                        let flight = m
                            .throw_companion(unit, &aim, false)
                            .unwrap_or_else(|e| panic!("{context}: {e}"));
                        launches += 1;
                        let body = &m.companions.soldiers[&unit];
                        assert_eq!(body.nades, old_body.nades - 1);
                        assert_eq!(body.nade_time.to_bits(), 1.0f64.to_bits());
                        assert_eq!(body.mags, old_body.mags);
                        assert_eq!(body.reload.to_bits(), old_body.reload.to_bits());
                        assert_eq!(
                            m.firing.get(unit).unwrap().cooldown.to_bits(),
                            cooldown.to_bits()
                        );
                        assert_eq!(
                            m.firing.next_flight,
                            n(&expected["game"]["nextFlight"]) as u64
                        );
                        let order: Vec<_> = m.firing.flights.keys().copied().collect();
                        exact(&json!(order), &expected["flightOrder"], context);
                        for (id, before) in retained {
                            assert_eq!(
                                format!("{:?}", m.firing.flights[&id]),
                                before,
                                "{context}: registration changed retained flight {id}"
                            );
                        }
                        let p = &m.firing.flights[&flight];
                        let source = expected["flights"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|p| n(&p["id"]) as u64 == flight)
                            .unwrap();
                        let blast = p.blast_source.unwrap();
                        let actual = json!({"id":p.id,"owner":p.owner,"shooter":p.shooter,
                        "kind":m.rules.kinds[p.source_kind].name,
                        "x":p.solution.origin.x,"y":p.solution.origin.y,"z":p.solution.origin.z,
                        "vx":p.solution.velocity.x,"vy":p.solution.velocity.y,"vz":p.solution.velocity.z,
                        "gravity":p.solution.gravity,"stamp":p.solution.stamp,
                        "launched":p.launched,"last":p.last,"expires":p.expires,
                        "energy":p.energy,"sequence":p.sequence,"maxContacts":p.max_contacts,
                        "near":p.near,"launchOrigin":point3_json(p.launch_origin),
                        "startCell":p.start_cell,"explosive":p.explosive,"blastRadius":p.blast_radius,
                        "effect":p.effect,"sent":p.sent,
                        "inf":blast.infantry,"veh":blast.vehicle,"terrain":blast.terrain,
                        "supp":blast.suppression});
                        let mut wanted = Value::Object(serde_json::Map::new());
                        for key in actual.as_object().unwrap().keys() {
                            wanted[key] = match key.as_str() {
                                "inf" | "veh" | "terrain" | "supp" => {
                                    source["blastSource"][key].clone()
                                }
                                _ => source[key].clone(),
                            };
                        }
                        exact(&actual, &wanted, context);
                        let tickets: Vec<_> = m
                            .firing
                            .grenades
                            .iter()
                            .map(|ticket| {
                                json!({"x":ticket.at.x,"z":ticket.at.z,"t":ticket.remaining,
                            "owner":ticket.owner,"flight":ticket.flight})
                            })
                            .collect();
                        let source_tickets: Vec<_> = expected["nades"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|ticket| {
                                json!({"x":ticket["x"],"z":ticket["z"],"t":ticket["t"],
                            "owner":ticket["owner"],"flight":ticket["flight"]})
                            })
                            .collect();
                        exact(&json!(tickets), &json!(source_tickets), context);
                        let shots: Vec<_> = m
                            .firing
                            .events
                            .iter()
                            .map(|shot| shot.to_json(&m.rules))
                            .collect();
                        exact(&json!(shots), &expected["shots"], context);
                    }
                    "set_tick" => m.world.game.tick = n(&args["tick"]) as i64,
                    "patch_unit" => {
                        let fields = args["fields"].as_object().unwrap();
                        assert_eq!(fields.len(), 1);
                        m.companions
                            .soldiers
                            .get_mut(&id(&args["id"]))
                            .unwrap()
                            .nade_time = n(&fields["nadeT"]);
                    }
                    "sample_flight" => {
                        let p = &m.firing.flights[&(n(&args["id"]) as u64)];
                        let actual: Vec<_> = args["times"].as_array().unwrap().iter().map(|time| {
                        let time = n(time);
                        samples += 1;
                        json!({"time":time,
                            "position":point3_json(ww2_core::projectiles::flight_at(p.solution,time).unwrap()),
                            "velocity":point3_json(ww2_core::projectiles::velocity_at(p.solution,time).unwrap())})
                    }).collect();
                        exact(&json!(actual), &decoded(&op["result"]), context);
                    }
                    other => panic!("unexpected grenade operation: {other}"),
                }
                assert_eq!(
                    m.start.random.snapshot(),
                    random,
                    "{context}: grenade spent RNG"
                );
                assert_eq!(
                    m.start.economy, economics,
                    "{context}: grenade spent wallet or army resources"
                );
                assert_eq!(
                    m.companions.records, companion_counts,
                    "{context}: grenade counted as fire"
                );
            }
        }
        assert_eq!(launches, 11);
        assert_eq!(samples, 77);
    }
}

#[test]
fn companion_grenade_rejection_preserves_counters_flights_events_and_randomness() {
    for failure in [
        "flight capacity",
        "stale flight ID",
        "missing combat",
        "invalid pitch",
        "empty grenades",
        "unsupported mode",
        "internal mode mismatch",
    ] {
        let (mut m, mut aim, shooter, _) = seeded_fire_match();
        m.options.mode = Mode::Conquest;
        match failure {
            "flight capacity" => m.firing.next_flight = 9_007_199_254_740_991,
            "stale flight ID" => {
                m.firing
                    .suppressed
                    .insert(m.firing.next_flight, Default::default());
            }
            "missing combat" => {
                m.combat.remove(shooter);
            }
            "invalid pitch" => aim.pitch = f64::NAN,
            "empty grenades" => m.companions.soldiers.get_mut(&shooter).unwrap().nades = 0,
            "unsupported mode" => m.options.mode = Mode::World,
            "internal mode mismatch" => m.world.game.mode = Some(ModeKind::Horde),
            _ => unreachable!(),
        }
        let before = guard_state(&m);
        assert!(
            m.throw_companion(shooter, &aim, false).is_err(),
            "{failure}"
        );
        assert_eq!(
            guard_state(&m),
            before,
            "{failure}: rejected grenade mutated state"
        );
    }
}
