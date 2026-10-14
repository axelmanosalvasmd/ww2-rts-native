use super::*;
use crate::{
    catalog::Catalog, game::Player, grid::SpatialGrid, map::AuthoritativeMap, movement, unit_spawn,
    world::World,
};
use serde_json::json;

fn world() -> World {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"traffic", "w":32,"h":24,"rows":vec!["................................";24],
            "spawns":[{"x":0,"y":0},{"x":31,"y":23}],"points":[{"id":"p","x":12,"y":12}]
        }))
        .unwrap(),
    )
    .unwrap();
    World::from_map(&map).unwrap()
}

#[test]
fn rejects_a_view_from_another_recipient() {
    let rules = Rules::compiled().unwrap();
    let knowledge = RouteKnowledge::world(32, 24, 7, rules).unwrap();
    let query = knowledge.query();
    let mut view = query.navigation();
    let foreign = view.flags.to_vec();
    view.flags = &foreign;
    assert!(matches!(
        TrafficKnowledge::observed(view, &knowledge, &[]),
        Err(KnowledgeError::InvalidShape)
    ));
}

#[test]
fn unknown_world_cover_uses_open_baseline_without_losing_the_moving_unit() {
    let rules = Rules::compiled().unwrap();
    let catalog = Catalog::compiled().unwrap();
    let mut world = world();
    world.game.players = vec![Player {
        slot: 0,
        team: 0,
        visible: Vec::new(),
    }];
    let mut rifle = unit_spawn::initial_unit(
        &rules,
        catalog,
        rules.kind("rifle").unwrap(),
        1,
        0,
        Point { x: 21.0, z: 23.0 },
    )
    .unwrap();
    rifle.path = vec![Point { x: 50.0, z: 23.0 }];
    rifle.traffic_wait = 1.0;
    let tank = unit_spawn::initial_unit(
        &rules,
        catalog,
        rules.kind("tank").unwrap(),
        2,
        0,
        Point { x: 23.0, z: 23.0 },
    )
    .unwrap();
    world.game.units.insert(1, rifle);
    world.game.units.insert(2, tank);
    let mut grid = SpatialGrid::new(&world.game.units, 16.0);
    let knowledge = RouteKnowledge::world(32, 24, 7, rules.clone()).unwrap();
    let query = knowledge.query();
    let input = TrafficKnowledge::observed(query.navigation(), &knowledge, &[]).unwrap();
    let result = movement::move_unit_with_knowledge(
        &mut world.game,
        &rules,
        &mut grid,
        1,
        1.0,
        &mut |_, _, _, _, _| panic!("bounded contact does not replan"),
        &input,
    );
    assert!(result.is_ok());
    assert_eq!(world.game.units.len(), 2);
    assert_eq!(world.game.units[&1].at(), Point { x: 21.0, z: 23.0 });
}

fn number(v: &serde_json::Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn point(v: &serde_json::Value) -> Option<Point> {
    v.as_array().map(|a| Point {
        x: number(&a[0]),
        z: number(&a[1]),
    })
}
fn pose(v: &serde_json::Value) -> Option<crate::game::Pose> {
    v.as_array().map(|a| crate::game::Pose {
        x: number(&a[0]),
        z: number(&a[1]),
        rot: number(&a[2]),
    })
}
fn bits(v: f64) -> serde_json::Value {
    json!(format!("{:016x}", v.to_bits()))
}
fn pt(p: Option<Point>) -> serde_json::Value {
    p.map_or(serde_json::Value::Null, |p| json!([bits(p.x), bits(p.z)]))
}
fn ps(p: Option<crate::game::Pose>) -> serde_json::Value {
    p.map_or(serde_json::Value::Null, |p| {
        json!([bits(p.x), bits(p.z), bits(p.rot)])
    })
}
fn unit(row: &serde_json::Value, rules: &Rules) -> crate::game::Unit {
    use crate::game::{Clearance, LaneScan, Traffic};
    let mut u = unit_spawn::initial_unit(
        rules,
        Catalog::compiled().unwrap(),
        rules.kind(row["kind"].as_str().unwrap()).unwrap(),
        row["id"].as_u64().unwrap() as u32,
        row["owner"].as_u64().unwrap() as usize,
        Point {
            x: number(&row["x"]),
            z: number(&row["z"]),
        },
    )
    .unwrap();
    u.rot = number(&row["rot"]);
    u.hp = number(&row["hp"]);
    u.pace = number(&row["pace"]);
    u.sprint = number(&row["sprint"]);
    u.supp = number(&row["supp"]);
    u.stuck = number(&row["stuck"]);
    u.repath = number(&row["repath"]);
    u.move_speed = number(&row["moveSpeed"]);
    u.traffic_wait = number(&row["trafficWait"]);
    u.ghost = number(&row["ghost"]);
    u.still = Some(number(&row["still"]));
    u.attack = row["attack"].as_bool().unwrap();
    u.retreating = row["retreating"].as_bool().unwrap();
    u.drift = row["drift"].as_bool().unwrap();
    u.traffic_entered = row["trafficEntered"].as_bool().unwrap();
    u.riding = row["riding"].as_bool().unwrap();
    u.hold_pos = row["holdPos"].as_bool().unwrap();
    u.build = row["build"].as_bool().unwrap();
    u.dig = row["dig"].as_bool().unwrap();
    u.entrench = row["entrench"].as_bool().unwrap();
    u.rail = row["rail"].as_bool().unwrap();
    u.reverse = point(&row["reverse"]);
    u.hit_from = point(&row["hitFrom"]);
    u.world_goal = point(&row["worldGoal"]);
    u.amove = point(&row["amove"]);
    u.travel_dir = row["travelDir"].as_str().map(|_| number(&row["travelDir"]));
    u.vx = row["vx"].as_str().map(|_| number(&row["vx"]));
    u.vz = row["vz"].as_str().map(|_| number(&row["vz"]));
    u.path = row["path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| point(v).unwrap())
        .collect();
    u.motion_before = pose(&row["motionBefore"]);
    u.path_gen = 0;
    u.clearance = (!row["motionClearance"].is_null()).then(|| {
        let c = &row["motionClearance"];
        Clearance {
            goal: pose(&c["goal"]).unwrap(),
            reverse: c["reverse"].as_bool().unwrap(),
            path: if c["matchesPath"].as_bool().unwrap() {
                0
            } else {
                u64::MAX
            },
            until: number(&c["until"]),
        }
    });
    u.traffic = (!row["traffic"].is_null()).then(|| {
        let t = &row["traffic"];
        Traffic {
            goal: point(&t["goal"]).unwrap(),
            legs: t["legs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| point(v).unwrap())
                .collect(),
            passing: t["passing"].as_bool().unwrap(),
            by: t["by"].as_u64().unwrap() as u32,
            until: number(&t["until"]),
        }
    });
    u.traffic_trail = row["trafficTrail"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| pose(v).unwrap())
        .collect();
    u.queued_behind = row["queuedBehind"].as_u64().unwrap() as u32;
    u.lane_scan = (!row["laneScan"].is_null()).then(|| {
        let c = &row["laneScan"];
        LaneScan {
            path: if c["matchesPath"].as_bool().unwrap() {
                0
            } else {
                u64::MAX
            },
            until: number(&c["until"]),
            lane: (!c["lane"].is_null()).then(|| {
                (
                    point(&c["lane"]["entry"]).unwrap(),
                    point(&c["lane"]["exit"]).unwrap(),
                )
            }),
        }
    });
    u.garrison = row["garrison"].as_u64().map(|v| v as usize);
    u.board = row["board"].as_u64().unwrap() as u32;
    u
}
fn row(u: &crate::game::Unit, rules: &Rules) -> serde_json::Value {
    let mut row = json!({"id":u.id,"kind":rules.kinds[u.kind].name,"owner":u.owner,"x":bits(u.x),"z":bits(u.z)});
    let out = row.as_object_mut().unwrap();
    out.insert("rot".into(), bits(u.rot));
    out.insert("hp".into(), bits(u.hp));
    out.insert("pace".into(), bits(u.pace));
    out.insert("sprint".into(), bits(u.sprint));
    out.insert("supp".into(), bits(u.supp));
    out.insert("stuck".into(), bits(u.stuck));
    out.insert("repath".into(), bits(u.repath));
    out.insert("moveSpeed".into(), bits(u.move_speed));
    out.insert("trafficWait".into(), bits(u.traffic_wait));
    out.insert("ghost".into(), bits(u.ghost));
    out.insert("still".into(), bits(u.still.unwrap_or(f64::NAN)));
    out.insert("attack".into(), json!(u.attack));
    out.insert("retreating".into(), json!(u.retreating));
    out.insert("drift".into(), json!(u.drift));
    out.insert("trafficEntered".into(), json!(u.traffic_entered));
    out.insert("riding".into(), json!(u.riding));
    out.insert("holdPos".into(), json!(u.hold_pos));
    out.insert("build".into(), json!(u.build));
    out.insert("dig".into(), json!(u.dig));
    out.insert("entrench".into(), json!(u.entrench));
    out.insert("rail".into(), json!(u.rail));
    out.insert("reverse".into(), pt(u.reverse));
    out.insert("hitFrom".into(), pt(u.hit_from));
    out.insert("worldGoal".into(), pt(u.world_goal));
    out.insert("amove".into(), pt(u.amove));
    out.insert(
        "travelDir".into(),
        u.travel_dir.map_or(serde_json::Value::Null, bits),
    );
    out.insert("vx".into(), u.vx.map_or(serde_json::Value::Null, bits));
    out.insert("vz".into(), u.vz.map_or(serde_json::Value::Null, bits));
    out.insert(
        "path".into(),
        json!(u.path.iter().map(|p| pt(Some(*p))).collect::<Vec<_>>()),
    );
    out.insert("motionBefore".into(), ps(u.motion_before));
    out.insert("motionClearance".into(),u.clearance.as_ref().map_or(serde_json::Value::Null,|c|json!({"goal":ps(Some(c.goal)),"reverse":c.reverse,"matchesPath":c.path==u.path_gen,"until":bits(c.until)})));
    out.insert("traffic".into(),u.traffic.as_ref().map_or(serde_json::Value::Null,|t|json!({"goal":pt(Some(t.goal)),"legs":t.legs.iter().map(|p|pt(Some(*p))).collect::<Vec<_>>(),"passing":t.passing,"by":t.by,"until":bits(t.until)})));
    out.insert(
        "trafficTrail".into(),
        json!(
            u.traffic_trail
                .iter()
                .map(|p| ps(Some(*p)))
                .collect::<Vec<_>>()
        ),
    );
    out.insert("queuedBehind".into(), json!(u.queued_behind));
    out.insert("laneScan".into(),u.lane_scan.as_ref().map_or(serde_json::Value::Null,|c|json!({"matchesPath":c.path==u.path_gen,"until":bits(c.until),"lane":c.lane.map(|(a,b)|json!({"entry":pt(Some(a)),"exit":pt(Some(b))}))})));
    out.insert(
        "garrison".into(),
        json!(u.garrison.map_or(-1, |c| c as i64)),
    );
    out.insert("board".into(), json!(u.board));
    row
}

#[test]
fn original_ground_traffic_through_match_recipient_routing() {
    use crate::{
        economy::{Faction, Mode},
        economy_state::ArmySize,
        match_commands::{MatchCommands, UnitControl},
        match_runtime::{Match, MatchOptions, PlayerConfig},
        weather::WeatherChoice,
    };
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/ground-traffic.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let mut count = 0;
    let mut calls = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
        let mut m = Match::new(
            &map,
            MatchOptions {
                seed: case["seed"].as_u64().unwrap(),
                mode: Mode::Conquest,
                players: case["players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .map(|(slot, p)| PlayerConfig {
                        name: format!("p{slot}"),
                        team: p["team"].as_i64().unwrap() as i32,
                        faction: Faction::try_from(slot as u8).unwrap(),
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
            },
        )
        .unwrap();
        m.world = World::from_map(&map).unwrap();
        m.building_cells.clear();
        m.commands = MatchCommands::new(4096);
        m.world.game.players = case["players"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(slot, p)| Player {
                slot,
                team: p["team"].as_i64().unwrap() as i32,
                visible: Vec::new(),
            })
            .collect();
        m.world.game.units = case["starting"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                let u = unit(v, &m.rules);
                (u.id, u)
            })
            .collect();
        m.grid.rebuild(&m.world.game.units);
        m.initialize_recipient_routing().unwrap();
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            m.world.game.tick = step["tick"].as_i64().unwrap();
            m.world.game.vision_tick = step["visionTick"].as_i64();
            for edit in step["action"]["cells"].as_array().into_iter().flatten() {
                m.world
                    .set_cell(
                        edit[0].as_u64().unwrap() as usize,
                        edit[1].as_str().unwrap().as_bytes()[0],
                    )
                    .unwrap();
            }
            m.world.game.units = step["before"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    let u = unit(v, &m.rules);
                    (u.id, u)
                })
                .collect();
            m.grid.rebuild(&m.world.game.units);
            for u in m.world.game.units.values() {
                m.commands
                    .insert(u.id, UnitControl::new(u, &m.rules).unwrap());
                m.firing.initialize(u.id);
                m.combat.initialize(u.id);
            }
            for (p, visible) in m
                .world
                .game
                .players
                .iter_mut()
                .zip(step["visible"].as_array().unwrap())
            {
                p.visible = visible
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u32)
                    .collect();
            }
            for owner in 0..m.world.game.players.len() {
                m.refresh_recipient_routing(owner).unwrap();
                let query = m.recipients[owner].query();
                let expected =
                    &fixture["terrains"][step["knowledge"][owner].as_u64().unwrap() as usize];
                assert_eq!(
                    json!(query.navigation().flags),
                    expected["flags"],
                    "{} step{index} owner{owner} flags",
                    case["name"]
                );
                if !expected["height"].is_null() {
                    assert_eq!(
                        json!(query.navigation().heights.unwrap()),
                        expected["height"]
                    );
                }
            }
            m.begin_path_tick(4096, 20).unwrap();
            for result in step["results"].as_array().unwrap() {
                calls += 1;
                let id = result["id"].as_u64().unwrap() as u32;
                let owner = m.world.game.units[&id].owner;
                m.refresh_recipient_routing(owner).unwrap();
                let recipient = &m.recipients[owner];
                let query = recipient.query();
                let input = TrafficKnowledge::observed(
                    query.navigation(),
                    recipient.knowledge(),
                    recipient.wrecks(),
                )
                .unwrap();
                let mut game = m.world.game.clone();
                let mut active = game.units.remove(&id).unwrap();
                crate::traffic::remember_position(&mut active);
                let goal = active.path.first().copied();
                let steer = crate::traffic::traffic_step_with_knowledge(
                    &game,
                    &m.rules,
                    &m.grid,
                    &mut active,
                    goal,
                    m.rules.movement.tick,
                    &input,
                )
                .unwrap();
                assert_eq!(
                    json!({"goal":pt(steer.goal),"blocked":steer.blocked,"yielding":steer.yielding,"waiting":steer.waiting,"replan":steer.replan}),
                    json!({"goal":result["goal"],"blocked":result["blocked"],"yielding":result["yielding"],"waiting":result["waiting"],"replan":result["replan"]}),
                    "{} step{index} steer",
                    case["name"]
                );
                let supp = movement::supp_speed(m.world.game.units[&id].supp);
                let moved = m.move_unit_with_command_routes(id, supp).unwrap().unwrap();
                assert_eq!(
                    bits(moved.speed),
                    result["speed"],
                    "{} step{index} speed",
                    case["name"]
                );
            }
            let actual: Vec<_> = m
                .world
                .game
                .units
                .values()
                .map(|u| row(u, &m.rules))
                .collect();
            assert_eq!(
                json!(actual),
                step["expected"],
                "{} step{index}",
                case["name"]
            );
            count += 1;
        }
    }
    assert_eq!(count, 63);
    assert_eq!(calls, 65);
    assert_eq!(
        count,
        fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["steps"].as_array().unwrap().len())
            .sum::<usize>()
    );
}
