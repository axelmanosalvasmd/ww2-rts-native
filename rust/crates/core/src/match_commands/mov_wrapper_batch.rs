//! Authored component scenes, using public Match behavior after explicit setup.
use super::{Command, MoveOutcome};
use super::{initial, point, set};
use crate::game::Point;
use crate::match_runtime::Match;
use crate::path_requests::RequestKind;
use serde_json::{Value, json};

fn bits(value: f64) -> Value {
    json!(format!("{:016x}", value.to_bits()))
}
fn at(value: &Value) -> Point {
    point(value).expect("fixture point")
}
fn pt(value: Option<Point>) -> Value {
    value.map_or(Value::Null, |p| json!([bits(p.x), bits(p.z)]))
}
fn projection(m: &Match) -> Value {
    m.world
        .game
        .units
        .values()
        .map(|u| {
            let c = &m.commands.units[&u.id];
            json!({"id":u.id,"x":bits(u.x),"z":bits(u.z),"rot":bits(u.rot),
                "path":u.path.iter().map(|p|pt(Some(*p))).collect::<Vec<_>>(),
                "repath":bits(u.repath),"stuck":bits(u.stuck),
                "worldGoal":pt(u.world_goal),"amove":pt(u.amove),"routeEnd":pt(c.route_end),
                "outcome":c.outcome.map(MoveOutcome::name),"outcomeTick":c.outcome_tick})
        })
        .collect()
}
fn fields(m: &mut Match, id: u32, value: &Value) {
    let mut remaining = value.as_object().unwrap().clone();
    {
        let u = m.world.game.units.get_mut(&id).unwrap();
        if let Some(v) = remaining.remove("hp") {
            u.hp = v.as_f64().unwrap();
        }
        if let Some(v) = remaining.remove("rot") {
            u.rot = v.as_f64().unwrap();
        }
        if let Some(v) = remaining.remove("reverse") {
            u.reverse = point(&v);
        }
    }
    set(m, id, &Value::Object(remaining));
}
fn kind(name: &str) -> RequestKind {
    match name {
        "world" => RequestKind::World,
        "recovery" => RequestKind::Recovery,
        "amove" => RequestKind::Amove,
        _ => panic!("unsupported fixture request kind"),
    }
}

#[test]
fn original_walk_recipient_routes_and_live_deferred_requests_match() {
    let fixture: Value = serde_json::from_slice(include_bytes!(
        "../../tests/fixtures/mov-wrapper-batch.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let mut actions = 0;
    let mut cases = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let mut m = initial(case);
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let action = &step["action"];
            let id = action["id"].as_u64().unwrap_or(1) as u32;
            if let Some(tick) = action["tick"].as_i64() {
                m.world.game.tick = tick;
            }
            let mut result = Value::Null;
            match action["op"].as_str().unwrap() {
                "set" => fields(&mut m, id, &action["fields"]),
                "walk" => {
                    m.begin_path_tick(4096, 20).unwrap();
                    m.move_unit_with_command_routes(id, 1.0).unwrap().unwrap();
                }
                "clear" => {
                    result = json!(crate::terrain::walkable(
                        &m.world.game,
                        &m.rules,
                        at(&action["a"]),
                        at(&action["b"]),
                        action["mask"].as_u64().unwrap_or(1) as u16,
                        action["half"].as_f64().unwrap_or(0.9),
                    ));
                }
                "begin" => {
                    m.begin_path_tick(
                        action["budget"].as_u64().unwrap_or(4096) as usize,
                        action["retry"].as_u64().unwrap_or(20) as u32,
                    )
                    .unwrap();
                }
                "request" => {
                    m.request_command_path(
                        id,
                        at(&action["to"]),
                        kind(action["kind"].as_str().unwrap()),
                    )
                    .unwrap();
                }
                "command" => {
                    let command = Command::from_json(&action["command"]).unwrap();
                    let denial = m
                        .execute_command(
                            action["owner"].as_u64().unwrap_or(0) as usize,
                            &command,
                            false,
                        )
                        .unwrap();
                    assert!(denial.is_none(), "fixture command must be accepted");
                }
                op => panic!("unknown fixture action {op}"),
            }
            assert_eq!(
                result, step["result"],
                "{} action {index}: result",
                case["name"]
            );
            assert_eq!(
                projection(&m),
                step["units"],
                "{} action {index}: state",
                case["name"]
            );
            actions += 1;
        }
        cases += 1;
    }
    assert!(cases >= 20 && actions >= 100, "fixture lost batch coverage");
}
