use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    logistics::Stock,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn optional(n: Option<f64>) -> Value {
    n.map_or(Value::Null, bits)
}
fn stock(s: Stock) -> Value {
    json!([bits(s.ammo), bits(s.provisions), bits(s.fuel)])
}
fn hash(bytes: impl IntoIterator<Item = u8>) -> Value {
    let mut h = 0xcbf29ce484222325u64;
    for b in bytes {
        h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    json!(format!("{h:016x}"))
}
fn options(c: &Value, map_key: &str) -> MatchOptions {
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
        weather_seed: c["weatherSeed"].as_u64().map(|v| v as u32),
        living_weather: c["living"].as_bool().unwrap(),
        map_key: Some(map_key.to_owned()),
        tech: c["tech"].as_bool().unwrap(),
        supply: true,
        logistics: true,
        sandbox: false,
    }
}
#[test]
fn real_conquest_logistics_initialization_matches_original_create_game() {
    let base: Value = serde_json::from_str(include_str!("fixtures/match-start.json")).unwrap();
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-supply.json")).unwrap();
    for row in fixture["rows"].as_array().unwrap() {
        let i = row["sourceCase"].as_u64().unwrap() as usize;
        let c = &base["cases"][i];
        let input = &base["maps"][c["map"].as_u64().unwrap() as usize];
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&input["map"]).unwrap()).unwrap();
        let instance = Match::new(&map, options(c, input["name"].as_str().unwrap())).unwrap();
        let reserves = instance
            .logistics
            .iter()
            .map(|(id, r)| {
                json!([
                    id,
                    [
                        bits(r.ammo),
                        bits(r.ammo_max),
                        bits(r.ammo_price),
                        optional(r.ammo_credit),
                        bits(r.provisions),
                        optional(r.fuel),
                        optional(r.emergency),
                        optional(r.warning),
                        r.forced,
                        r.stranded,
                        r.exhausted,
                        optional(r.supply_rate),
                        optional(r.cut_for)
                    ]
                ])
            })
            .collect::<Vec<_>>();
        assert_eq!(json!(reserves), row["reserves"], "case{i} reserves");
        assert_eq!(
            instance.supply_net.teams.len(),
            row["net"].as_array().unwrap().len()
        );
        for net in row["net"].as_array().unwrap() {
            let team = net["team"].as_i64().unwrap() as i32;
            let actual = instance
                .supply_net
                .teams
                .iter()
                .find(|n| n.team == team)
                .unwrap();
            assert_eq!(actual.steps.len(), net["cells"].as_u64().unwrap() as usize);
            assert_eq!(
                hash(actual.steps.iter().flat_map(|n| n.to_le_bytes())),
                net["stepsHash"],
                "case{i} steps team{team}"
            );
            assert_eq!(
                hash(actual.closed.iter().copied()),
                net["closedHash"],
                "case{i} closed"
            );
        }
        let state = instance.convoys.as_ref().unwrap();
        let stores = state
            .stores
            .iter()
            .map(|s| {
                json!([
                    s.id.key(),
                    s.owner,
                    [bits(s.at.x), bits(s.at.z)],
                    s.source,
                    s.entity,
                    stock(s.capacity),
                    s.buckets
                        .iter()
                        .map(|(p, b)| json!([p, stock(*b)]))
                        .collect::<Vec<_>>(),
                    s.donations
                        .iter()
                        .map(|(p, rs)| json!([
                            p,
                            rs.iter()
                                .map(|(q, b)| json!([q, stock(*b)]))
                                .collect::<Vec<_>>()
                        ]))
                        .collect::<Vec<_>>(),
                    s.active,
                    s.support,
                    s.point
                ])
            })
            .collect::<Vec<_>>();
        assert_eq!(json!(stores), row["stores"], "case{i} stores");
        assert_eq!(
            json!(
                state
                    .jobs
                    .iter()
                    .map(|(p, n)| json!([p, n]))
                    .collect::<Vec<_>>()
            ),
            row["jobs"],
            "case{i} jobs"
        );
        assert_eq!(json!(state.next_schedule), row["nextSchedule"]);
        assert_eq!(
            json!(
                state
                    .next_spawn
                    .iter()
                    .map(|(p, n)| json!([p, n]))
                    .collect::<Vec<_>>()
            ),
            row["nextSpawn"]
        );
        assert_eq!(
            json!(state.trucks.keys().copied().collect::<Vec<_>>()),
            row["trucks"]
        );
        let misc = json!({"losses":state.losses.iter().map(|(p,ns)|json!([p,ns])).collect::<Vec<_>>(),"events":[],"alerts":[],"nextEvent":state.next_event,"refreshQueue":state.refresh_queue,"scheduleQueue":state.schedule_queue,"scheduled":state.scheduled,"assignments":[],"routed":state.routed});
        assert_eq!(misc, row["misc"], "case{i} schedule fields");
        assert!(state.events.is_empty() && state.alerts.is_empty() && state.assignments.is_empty());
    }
}

fn point(p: ww2_core::game::Point) -> Value {
    json!([bits(p.x), bits(p.z)])
}
fn runtime(instance: &Match) -> Value {
    use ww2_core::supply_convoys::{RouteFailure, TargetId};
    let state = instance.convoys.as_ref().unwrap();
    let reserves = instance
        .logistics
        .iter()
        .map(|(id, r)| {
            json!([
                id,
                [
                    bits(r.ammo),
                    bits(r.ammo_max),
                    bits(r.ammo_price),
                    optional(r.ammo_credit),
                    bits(r.provisions),
                    optional(r.fuel),
                    optional(r.emergency),
                    optional(r.warning),
                    r.forced,
                    r.stranded,
                    r.exhausted,
                    optional(r.supply_rate),
                    optional(r.cut_for)
                ]
            ])
        })
        .collect::<Vec<_>>();
    let trucks = state
        .trucks
        .iter()
        .map(|(id, c)| {
            let u = &instance.world.game.units[id];
            let controls = &instance.commands.units[id];
            let target = c.target.as_ref().map(|t| {
                json!([
                    match &t.id {
                        TargetId::Unit(id) => format!("unit:{id}"),
                        TargetId::Store(id) => id.key(),
                    },
                    t.owner,
                    point(t.at),
                    stock(t.need),
                    t.members,
                    t.unit,
                    t.store.as_ref().map(|id| id.key())
                ])
            });
            json!([
                id,
                u.owner,
                point(u.at()),
                u.path.iter().copied().map(point).collect::<Vec<_>>(),
                u.world_goal.map(point),
                bits(u.repath),
                u.retreating,
                controls.attack_id,
                instance.firing.get(*id).unwrap().target_id,
                controls.auto_retreat,
                c.state.name(),
                stock(c.cargo),
                c.origin.key(),
                bits(c.timer),
                c.manual,
                c.hold,
                c.destination.map(point),
                c.route.iter().copied().map(point).collect::<Vec<_>>(),
                target,
                c.route_failure.map(|r| match r {
                    RouteFailure::Blocked => "blocked",
                    RouteFailure::Dangerous => "dangerous",
                    RouteFailure::Unreachable => "unreachable",
                }),
                c.retry.as_ref().map(|r| json!([
                    format!("{},{}", r.key.0, r.key.1),
                    bits(r.wait),
                    r.at
                ])),
                controls
                    .orders
                    .iter()
                    .map(|cmd| match cmd {
                        ww2_core::match_commands::Command::Move { orders, .. } =>
                            json!([orders[0].unit, point(orders[0].at)]),
                        _ => panic!("unexpected waiting order"),
                    })
                    .collect::<Vec<_>>(),
                controls.route_end.map(point)
            ])
        })
        .collect::<Vec<_>>();
    json!({"reserves":reserves,"trucks":trucks,"stores":state.stores.iter().map(|s|json!([s.id.key(),s.buckets.iter().map(|(p,b)|json!([p,stock(*b)])).collect::<Vec<_>>()])).collect::<Vec<_>>(),"jobs":state.jobs.iter().map(|(p,n)|json!([p,n])).collect::<Vec<_>>(),"losses":state.losses.iter().map(|(p,ns)|json!([p,ns.iter().map(|n|bits(*n)).collect::<Vec<_>>()])).collect::<Vec<_>>(),"nextSpawn":state.next_spawn.iter().map(|(p,n)|json!([p,bits(*n)])).collect::<Vec<_>>(),"nextSchedule":state.next_schedule,"refresh":state.refresh_queue,"schedule":state.schedule_queue,"scheduled":state.scheduled,"routed":state.routed,"events":state.events.iter().map(|e|json!([e.id,e.owner,e.kind,e.count,e.at.map(|p|bits(p.x)),e.at.map(|p|bits(p.z)),e.message,bits(e.seconds)])).collect::<Vec<_>>(),"alerts":state.alerts.iter().map(|((p,k),n)|json!([format!("{p}:{k}"),bits(*n)])).collect::<Vec<_>>()})
}
#[test]
fn real_supply_hooks_spawn_route_service_and_retire_in_canonical_match() {
    let base: Value = serde_json::from_str(include_str!("fixtures/match-start.json")).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/match-supply-runtime.json")).unwrap();
    let mut branches = std::collections::BTreeSet::new();
    for trace in fixture["traces"].as_array().unwrap() {
        let i = trace["sourceCase"].as_u64().unwrap() as usize;
        let c = &base["cases"][i];
        let input = &base["maps"][c["map"].as_u64().unwrap() as usize];
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&input["map"]).unwrap()).unwrap();
        let mut instance = Match::new(&map, options(c, input["name"].as_str().unwrap())).unwrap();
        for step in trace["steps"].as_array().unwrap() {
            let action = &step["action"];
            let tick = action["tick"].as_i64().unwrap();
            instance.world.game.tick = tick;
            for patch in action["deplete"].as_array().unwrap() {
                let id = patch["id"].as_u64().unwrap() as u32;
                let u = instance.world.game.units.get_mut(&id).unwrap();
                u.x = patch["x"].as_f64().unwrap();
                u.z = patch["z"].as_f64().unwrap();
                instance.grid.update(id, u);
                let r = instance.logistics.get_mut(id).unwrap();
                r.ammo = 0.0;
                r.provisions = 0.0;
                r.cut_for = Some(30.0);
            }
            if action["arrive"].as_bool().unwrap() {
                for (id, truck) in &instance.convoys.as_ref().unwrap().trucks {
                    if let Some(at) = truck.destination.filter(|_| !truck.manual && !truck.hold) {
                        let u = instance.world.game.units.get_mut(id).unwrap();
                        u.x = at.x;
                        u.z = at.z;
                        u.path.clear();
                        u.world_goal = None;
                        u.stuck = 0.0;
                        instance.grid.update(*id, u);
                    }
                }
            }
            if let Some(id) = action["dead"].as_u64() {
                let id = id as u32;
                let u = instance.world.game.units[&id].clone();
                instance.record_convoy_death(&u).unwrap();
                instance.world.game.units.remove(&id);
                instance.logistics.remove(id);
                instance.commands.units.remove(&id);
                instance.firing.remove(id);
                instance.combat.remove(id);
                instance.grid.rebuild(&instance.world.game.units);
            }
            if let Some(commands) = action["commands"].as_array() {
                for (index, cmd) in commands.iter().enumerate() {
                    use ww2_core::{
                        game::Point,
                        match_commands::{Command, MoveOrder},
                    };
                    let id = cmd["id"].as_u64().unwrap() as u32;
                    let result = match cmd["kind"].as_str().unwrap() {
                        "resume" => {
                            if instance.resume_convoys(0, &[id]).unwrap() {
                                Value::Null
                            } else {
                                json!("needs")
                            }
                        }
                        kind => {
                            let command = if kind == "stop" {
                                Command::Stop {
                                    ids: vec![i64::from(id)],
                                }
                            } else {
                                Command::Move {
                                    control_ids: None,
                                    orders: vec![MoveOrder {
                                        unit: i64::from(id),
                                        at: Point {
                                            x: cmd["x"].as_f64().unwrap(),
                                            z: cmd["z"].as_f64().unwrap(),
                                        },
                                    }],
                                    attack_move: false,
                                    queue: cmd["queue"].as_bool().unwrap_or(false),
                                    together: false,
                                    face: None,
                                }
                            };
                            instance
                                .execute_command(0, &command, false)
                                .unwrap()
                                .map_or(Value::Null, |r| json!(r.name()))
                        }
                    };
                    assert_eq!(
                        result, action["results"][index],
                        "case{i} tick{tick} manual result"
                    );
                }
            }
            if !action["support"].is_null() {
                let request = &action["support"];
                assert_eq!(
                    instance
                        .spend_support_provisions(
                            request["id"].as_u64().unwrap() as u32,
                            request["owner"].as_u64().unwrap() as usize,
                            request["amount"].as_f64().unwrap()
                        )
                        .unwrap(),
                    action["supportResult"].as_bool().unwrap()
                );
            }
            let ids = instance
                .world
                .game
                .units
                .keys()
                .copied()
                .collect::<Vec<_>>();
            for id in &ids {
                instance.advance_unit_reserves(*id).unwrap();
            }
            if tick % 20 == 0 {
                instance.refresh_supply_network(&ids).unwrap();
            }
            instance
                .step_supply_convoys(action["winner"].as_bool().unwrap())
                .unwrap();
            if !step["expected"].is_null() {
                assert_eq!(runtime(&instance), step["expected"], "case{i} tick{tick}");
            }
            for c in instance.convoys.as_ref().unwrap().trucks.values() {
                branches.insert(c.state.name().to_owned());
            }
        }
    }
    for state in ["loading", "delivering", "unloading", "returning"] {
        assert!(branches.contains(state), "missing real hook branch {state}");
    }
}

#[test]
fn supply_preflights_reject_invalid_shapes_orders_and_identity_without_publication() {
    use ww2_core::match_commands::Command;
    let base: Value = serde_json::from_str(include_str!("fixtures/match-start.json")).unwrap();
    let c = &base["cases"][60];
    let input = &base["maps"][c["map"].as_u64().unwrap() as usize];
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&input["map"]).unwrap()).unwrap();
    let mut disabled = options(c, input["name"].as_str().unwrap());
    disabled.logistics = false;
    let mut instance = Match::new(&map, disabled).unwrap();
    instance.options.logistics = true;
    let hp = instance.world.game.cell_hp.pop().unwrap();
    let wallets = instance.start.economy.players.to_vec();
    assert!(instance.initialize_logistics().is_err());
    assert!(instance.convoys.is_none());
    assert!(instance.logistics.iter().next().is_none());
    assert!(instance.supply_net.teams.is_empty());
    assert_eq!(instance.start.economy.players.to_vec(), wallets);
    instance.world.game.cell_hp.push(hp);
    instance.initialize_logistics().unwrap();
    let baseline = runtime(&instance);
    let id = *instance.world.game.units.keys().next().unwrap();
    instance
        .commands
        .units
        .get_mut(&id)
        .unwrap()
        .orders
        .push_back(Command::Unsupported);
    assert!(instance.step_supply_convoys(false).is_err());
    assert_eq!(runtime(&instance), baseline);
    instance.commands.units.get_mut(&id).unwrap().orders.clear();
    instance.world.game.units.get_mut(&id).unwrap().path_gen = u64::MAX - 3;
    assert!(instance.step_supply_convoys(false).is_err());
    assert_eq!(runtime(&instance), baseline);
    instance.world.game.units.get_mut(&id).unwrap().path_gen = 0;
    assert!(instance.spend_support_provisions(id, 0, f64::NAN).is_err());
    assert!(instance.spend_support_provisions(id, 0, -1.0).is_err());
    assert_eq!(runtime(&instance), baseline);
    let mut draft = instance.world.game.units[&id].clone();
    draft.id = instance.next_unit_id;
    assert!(
        instance
            .prepare_new_unit_reserves(&draft)
            .unwrap()
            .is_some()
    );
    assert!(instance.logistics.get(draft.id).is_none());
    assert_eq!(runtime(&instance), baseline);
}
