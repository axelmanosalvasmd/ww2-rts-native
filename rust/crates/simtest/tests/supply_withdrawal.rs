use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, MoveOrder, MoveOutcome, UnitControl},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    unit_spawn::initial_unit,
    weather::WeatherChoice,
};

fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn point(p: Point) -> Value {
    json!([bits(p.x), bits(p.z)])
}
fn optional_point(p: Option<Point>) -> Value {
    p.map_or(Value::Null, point)
}
fn read_point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn options(c: &Value, key: &str) -> MatchOptions {
    MatchOptions {
        seed: c["seed"].as_str().unwrap().parse().unwrap(),
        mode: Mode::Conquest,
        players: c["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| PlayerConfig {
                name: p["name"].as_str().unwrap().to_owned(),
                team: p["team"].as_i64().unwrap() as i32,
                faction: Faction::try_from(p["faction"].as_u64().unwrap() as u8).unwrap(),
            })
            .collect(),
        shuffle: c["shuffle"].as_bool().unwrap(),
        army: match c["army"].as_str().unwrap() {
            "standard" => ArmySize::Standard,
            "large" => ArmySize::Large,
            "massive" => ArmySize::Massive,
            "endless" => ArmySize::Endless,
            _ => unreachable!(),
        },
        weather: WeatherChoice::from_reference(c["weather"].as_str()),
        weather_seed: c["weatherSeed"].as_u64().map(|n| n as u32),
        living_weather: c["living"].as_bool().unwrap(),
        map_key: Some(key.to_owned()),
        tech: c["tech"].as_bool().unwrap(),
        supply: true,
        logistics: true,
        sandbox: false,
    }
}
fn setup(base: &Value, trace: &Value) -> (Match, u32) {
    let c = &base["cases"][trace["sourceCase"].as_u64().unwrap() as usize];
    let input = &base["maps"][c["map"].as_u64().unwrap() as usize];
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&input["map"]).unwrap()).unwrap();
    let mut instance = Match::new(&map, options(c, input["name"].as_str().unwrap())).unwrap();
    let actor = &trace["initial"];
    let id = actor["id"].as_u64().unwrap() as u32;
    assert_eq!(id, instance.next_unit_id);
    let kind = instance
        .rules
        .kinds
        .iter()
        .position(|k| k.name == actor["kind"].as_str().unwrap())
        .unwrap();
    // The actor's initial pose is an explicit input recorded before the withdrawal turn.
    let unit = initial_unit(
        &instance.rules,
        Catalog::compiled().unwrap(),
        kind,
        id,
        0,
        read_point(actor),
    )
    .unwrap();
    let reserve = instance.prepare_new_unit_reserves(&unit).unwrap().unwrap();
    let control = UnitControl::new(&unit, &instance.rules).unwrap();
    instance.world.game.units.insert(id, unit);
    instance.grid.update(id, &instance.world.game.units[&id]);
    instance.commands.insert(id, control);
    instance.combat.initialize(id);
    instance.firing.initialize(id);
    instance.abilities.initialize(id);
    instance.recovery.initialize(id);
    instance.logistics.insert(id, reserve);
    instance.next_unit_id += 1;
    if let Some(field) = trace.get("fieldStart") {
        use ww2_core::field_works::{FortKind, StartRequest};
        instance.world.game.tick = field["tick"].as_i64().unwrap();
        let visibility = field["visibility"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                let at = read_point(v);
                let seen = instance.recipient_sees(0, at).unwrap();
                assert_eq!(
                    seen,
                    v["seen"].as_bool().unwrap(),
                    "field admission visibility"
                );
                ((at.x.to_bits(), at.z.to_bits()), seen)
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let request = &field["command"];
        let prepared = instance
            .field_works
            .prepare_start(
                &instance.world,
                &mut instance.start.economy.players[0],
                StartRequest {
                    control_eligible: true,
                    owner: 0,
                    unit: id,
                    kind: FortKind::Trench,
                    at: read_point(request),
                    direction: request["dir"].as_f64(),
                },
                |at| {
                    *visibility
                        .get(&(at.x.to_bits(), at.z.to_bits()))
                        .expect("original planned visibility")
                },
            )
            .unwrap();
        prepared.commit(&mut instance.field_works);
        instance.world.game.units.get_mut(&id).unwrap().dig = true;
        let control = instance.commands.units.get_mut(&id).unwrap();
        control.outcome = Some(MoveOutcome::Interrupted);
        control.outcome_tick = Some(instance.world.game.tick);
        assert_eq!(
            row(&instance, id),
            field["initial"],
            "actual paid field input"
        );
        assert_eq!(bits(instance.start.economy.players[0].mp), field["mpAfter"]);
    }
    (instance, id)
}
fn patch(instance: &mut Match, id: u32, input: &Value) {
    instance.world.game.tick = input["tick"].as_i64().unwrap();
    let unit = instance.world.game.units.get_mut(&id).unwrap();
    let control = instance.commands.units.get_mut(&id).unwrap();
    for (key, v) in input["patch"].as_object().unwrap() {
        match key.as_str() {
            "x" => unit.x = v.as_f64().unwrap(),
            "z" => unit.z = v.as_f64().unwrap(),
            "path" => unit.path = v.as_array().unwrap().iter().map(read_point).collect(),
            "worldGoal" => unit.world_goal = (!v.is_null()).then(|| read_point(v)),
            "stuck" => unit.stuck = v.as_f64().unwrap(),
            "garrison" => unit.garrison = v.as_u64().map(|n| n as usize),
            "supplyStarted" => control.supply_started = v.as_bool().unwrap(),
            "supplyEscape" => control.supply_escape = v.as_bool().unwrap(),
            "supplyHold" => control.supply_hold = v.as_bool().unwrap(),
            "supplyRetry" => control.supply_retry = v.as_u64(),
            "orders" => {
                control.orders = v
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|order| {
                        if order["t"] == "move" {
                            Command::Move {
                                orders: order["orders"]
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .map(|r| MoveOrder {
                                        unit: r[0].as_i64().unwrap(),
                                        at: Point {
                                            x: r[1].as_f64().unwrap(),
                                            z: r[2].as_f64().unwrap(),
                                        },
                                    })
                                    .collect(),
                                control_ids: None,
                                queue: false,
                                together: false,
                                attack_move: false,
                                face: None,
                            }
                        } else {
                            assert_eq!(order["t"], "ability");
                            Command::Ability {
                                ids: vec![i64::from(id)],
                                at: Some(read_point(order)),
                                queue: false,
                                automatic: false,
                            }
                        }
                    })
                    .collect()
            }
            "attackId" => {
                control.attack_id = v.as_u64().unwrap() as u32;
                unit.attack = control.attack_id != 0;
            }
            "targetId" => {
                instance.firing.get_mut(id).unwrap().target_id = v.as_u64().unwrap() as u32
            }
            "enter" => control.enter = v.as_u64().map(|n| n as usize),
            "board" => unit.board = v.as_u64().unwrap() as u32,
            "nade" => control.nade = Some(read_point(v)),
            "fireAt" => control.fire_at = v.as_u64().map(|n| n as usize),
            "riding" => {
                unit.riding = v.as_u64().is_some_and(|n| n != 0);
                instance.combat.get_mut(id).unwrap().carrier = v.as_u64().map(|n| n as u32);
            }
            _ => panic!("unhandled withdrawal input {key}"),
        }
    }
    for (key, v) in input["reserve"].as_object().unwrap() {
        let reserve = instance.logistics.get_mut(id).unwrap();
        match key.as_str() {
            "forced" => reserve.forced = v.as_bool().unwrap(),
            "fuel" => reserve.fuel = v.as_f64(),
            "emergency" => reserve.emergency = v.as_f64(),
            _ => panic!("unhandled withdrawal reserve {key}"),
        }
    }
    instance.grid.update(id, unit);
}
fn row(instance: &Match, id: u32) -> Value {
    let u = &instance.world.game.units[&id];
    let c = &instance.commands.units[&id];
    let orders = c
        .orders
        .iter()
        .map(|order| match order {
            Command::Move { orders, .. } => json!([
                "move",
                orders
                    .iter()
                    .map(|r| json!([r.unit, bits(r.at.x), bits(r.at.z)]))
                    .collect::<Vec<_>>()
            ]),
            Command::Ability { at: Some(at), .. } => json!(["ability", bits(at.x), bits(at.z)]),
            _ => panic!("unrepresented waiting withdrawal order"),
        })
        .collect::<Vec<_>>();
    json!({
        "at":point(u.at()),"path":u.path.iter().copied().map(point).collect::<Vec<_>>(),
        "worldGoal":optional_point(u.world_goal),"routeEnd":optional_point(c.route_end),
        "retreating":u.retreating,"garrison":u.garrison.map_or(-1, |n|n as i64),"stuck":bits(u.stuck),
        "started":c.supply_started,"escape":c.supply_escape,"hold":c.supply_hold,"retry":c.supply_retry,
        "orders":orders,"attack":c.attack_id,"target":instance.firing.get(id).unwrap().target_id,
        "enter":c.enter.map_or(-1,|n|n as i64),"board":u.board,"nade":optional_point(c.nade),
        "fireAt":c.fire_at.map_or(-1,|n|n as i64),"face":c.face.map(|f|json!({"a":f.angle,"x":f.at.x,"z":f.at.z})),
        "build":u.build,"dig":u.dig,"entrench":u.entrench,"amove":optional_point(u.amove),
        "outcome":c.outcome.map(|o|match o { MoveOutcome::Interrupted=>"interrupted", MoveOutcome::Moving=>"moving", MoveOutcome::Unreachable=>"unreachable", MoveOutcome::Arrived=>"completed" }),
        "outcomeTick":c.outcome_tick,"reserve":instance.logistics.get(id).map(|r|json!({"forced":r.forced,"stranded":r.stranded,"fuel":r.fuel.map(bits),"emergency":r.emergency.map(bits)}))
    })
}
#[test]
fn forced_withdrawal_matches_original_source_transitions_and_home_search_effects() {
    let base: Value = serde_json::from_str(include_str!("fixtures/match-start.json")).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/supply-withdrawal.json")).unwrap();
    let mut transitions = 0;
    let mut homes = 0;
    for (trace_index, trace) in fixture["traces"].as_array().unwrap().iter().enumerate() {
        let (mut instance, id) = setup(&base, trace);
        let active = instance
            .convoys
            .as_ref()
            .unwrap()
            .stores
            .iter()
            .map(|s| s.active)
            .collect::<Vec<_>>();
        for step in trace["steps"].as_array().unwrap() {
            let input = &step["input"];
            patch(&mut instance, id, input);
            if let Some(mode) = input["storeMode"].as_str() {
                for (store, was_active) in instance
                    .convoys
                    .as_mut()
                    .unwrap()
                    .stores
                    .iter_mut()
                    .zip(&active)
                {
                    store.active = mode == "restore" && *was_active;
                }
            }
            instance.advance_supply_withdrawal(id).unwrap();
            if let Some(field) = trace.get("fieldStart") {
                assert!(
                    !instance.field_works.is_active(id),
                    "paid work cancels synchronously"
                );
                assert_eq!(
                    bits(instance.start.economy.players[0].mp),
                    field["mpAfter"],
                    "cancellation has no refund"
                );
            }
            assert_eq!(
                row(&instance, id),
                step["expected"],
                "trace{trace_index} {}",
                input["name"]
            );
            if input["name"] == "real source route"
                && !instance.world.game.units[&id].path.is_empty()
            {
                assert_ne!(
                    instance.commands.units[&id].route_end,
                    instance.world.game.units[&id].world_goal
                );
                homes += 1;
            }
            transitions += 1;
        }
    }
    assert_eq!(transitions, 103);
    assert_eq!(homes, 5);
}

#[test]
fn malformed_later_dependencies_cannot_publish_withdrawal_metadata_or_cancel_work() {
    let base: Value = serde_json::from_str(include_str!("fixtures/match-start.json")).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/supply-withdrawal.json")).unwrap();
    let (mut instance, id) = setup(&base, &fixture["traces"][0]);
    instance.logistics.get_mut(id).unwrap().forced = true;
    let before = row(&instance, id);
    let hp = instance.world.game.cell_hp.pop().unwrap();
    assert!(instance.advance_supply_withdrawal(id).is_err());
    assert_eq!(row(&instance, id), before);
    instance.world.game.cell_hp.push(hp);
    let last = instance
        .convoys
        .as_mut()
        .unwrap()
        .stores
        .last_mut()
        .unwrap();
    let position = last.at;
    last.at.x = f64::NAN;
    assert!(instance.advance_supply_withdrawal(id).is_err());
    assert_eq!(row(&instance, id), before);
    instance
        .convoys
        .as_mut()
        .unwrap()
        .stores
        .last_mut()
        .unwrap()
        .at = position;
    instance.world.game.units.get_mut(&id).unwrap().build = true;
    let before = row(&instance, id);
    assert!(instance.advance_supply_withdrawal(id).is_err());
    assert_eq!(row(&instance, id), before);
    instance.world.game.units.get_mut(&id).unwrap().build = false;
    instance.world.game.tick = i64::MAX;
    let before = row(&instance, id);
    assert!(instance.advance_supply_withdrawal(id).is_err());
    assert_eq!(row(&instance, id), before);
    instance.world.game.tick = 0;
    instance.world.game.units.get_mut(&id).unwrap().path_gen = u64::MAX;
    let before = row(&instance, id);
    assert!(instance.advance_supply_withdrawal(id).is_err());
    assert_eq!(row(&instance, id), before);
}
