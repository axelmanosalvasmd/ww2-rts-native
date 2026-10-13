use serde_json::{Value, json};
use ww2_core::{
    game::Point,
    human_commander_inputs::{self as human, Cursor, InputKind, Unit, View},
};

fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn selection(v: &Value) -> Vec<i64> {
    let s = v.as_str().unwrap();
    if s.is_empty() {
        vec![]
    } else {
        s.split(',').map(|s| s.parse().unwrap()).collect()
    }
}
fn exact(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            a.as_f64().unwrap().to_bits() == b.as_f64().unwrap().to_bits()
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| exact(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| exact(a, b)))
        }
        _ => a == b,
    }
}

#[test]
fn actual_human_duplicate_formation_and_input_helpers_match_source() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/human-commander-inputs.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let view = View {
            units: case["view"]["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|u| Unit {
                    id: u["id"].as_u64().unwrap() as u32,
                    at: point(u),
                    world_goal: (!u["worldGoal"].is_null()).then(|| point(&u["worldGoal"])),
                    amove: (!u["amove"].is_null()).then(|| point(&u["amove"])),
                    target_id: u["targetId"].as_u64().unwrap() as u32,
                    retreating: u["retreating"].as_bool().unwrap(),
                })
                .collect(),
        };
        let mut cursor = Cursor {
            camera: point(&case["initialCursor"]["camera"]),
            selection: selection(&case["initialCursor"]["selection"]),
        };
        let news: Vec<_> = case["commands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| human::news(c, &view).unwrap())
            .collect();
        let json_news = Value::Array(
            news.iter()
                .map(|c| c.clone().unwrap_or(Value::Null))
                .collect(),
        );
        assert!(
            exact(&json_news, &case["expected"]["news"]),
            "news {}",
            case["name"]
        );
        let formation =
            human::formation_moves(news.into_iter().flatten().collect(), &view).unwrap();
        assert!(
            exact(&json!(formation), &case["expected"]["formation"]),
            "formation {}",
            case["name"]
        );
        for (command, expected) in formation
            .iter()
            .zip(case["expected"]["inputs"].as_array().unwrap())
        {
            let inputs = human::inputs_for(&mut cursor, command, &view).unwrap();
            let rows:Vec<_> = inputs.iter().map(|input| {
                let mut row=json!({"kind":match input.kind {InputKind::Camera=>"camera",InputKind::Select=>"select",InputKind::Key=>"key",InputKind::Click=>"click"},"x":input.at.x,"z":input.at.z});
                if let Some(ids)=&input.ids {row["ids"]=json!(ids);}
                if let Some(minimap)=input.minimap {row["minimap"]=json!(minimap);}
                if let Some(command)=&input.command {row["cmd"]=command.clone();}
                row
            }).collect();
            assert!(
                exact(&json!(rows), &expected["emitted"]),
                "inputs {}",
                case["name"]
            );
        }
        assert_eq!(
            cursor.camera,
            point(&case["expected"]["finalCursor"]["camera"])
        );
        assert_eq!(
            cursor.selection,
            selection(&case["expected"]["finalCursor"]["selection"])
        );
    }
}

#[test]
fn malformed_human_input_does_not_publish_cursor() {
    let view = View { units: vec![] };
    let mut cursor = Cursor {
        camera: Point { x: 0.0, z: 0.0 },
        selection: vec![],
    };
    let before = cursor.clone();
    assert!(
        human::inputs_for(
            &mut cursor,
            &json!({"t":"move","orders":[[1,"invalid",0]]}),
            &view
        )
        .is_err()
    );
    assert_eq!(cursor, before);
}
