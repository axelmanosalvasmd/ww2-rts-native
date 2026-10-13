//! The fixture executes the source capture statements and supply functions unchanged.
use serde_json::{Value, json};
use ww2_core::{
    game::Point,
    navigation::{TerrainView, Versions},
    objectives::*,
    unit_spawn::SpawnPlacer,
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/objectives.json")).unwrap()
}
fn n(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn b(v: &Value) -> bool {
    v.as_bool().unwrap_or(false)
}
fn point(v: &Value) -> Point {
    Point {
        x: n(&v["x"]),
        z: n(&v["z"]),
    }
}
fn index(v: &Value) -> Option<usize> {
    v.as_i64().and_then(|n| (n >= 0).then_some(n as usize))
}
fn player_rows(v: &Value) -> Vec<ObjectivePlayer> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| ObjectivePlayer {
            team: p["team"].as_i64().unwrap() as i32,
            out: b(&p["out"]),
            spawn: point(&p["spawn"]),
        })
        .collect()
}
fn objective_rows(v: &Value) -> Vec<Objective> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| Objective {
            id: p["id"].clone(),
            at: point(p),
            vp: n(&p["vp"]),
            mp: n(&p["mp"]),
            kind: p["kind"].as_str().map(str::to_owned),
            needs: index(&p["needs"]),
            owner: index(&p["owner"]),
            capper: index(&p["capper"]),
            progress: n(&p["progress"]),
            cut: b(&p["cut"]),
            on_point: vec![],
            contested: false,
        })
        .collect()
}
fn unit_rows(v: &Value) -> Vec<ObjectiveUnit<'_>> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|u| ObjectiveUnit {
            id: u["id"].as_u64().unwrap() as u32,
            owner: u["owner"].as_u64().unwrap() as usize,
            kind: u["type"].as_str().unwrap(),
            at: point(u),
            hp: n(&u["hp"]),
            airborne: !u["air"].is_null(),
            riding: u["riding"].as_i64().unwrap_or(0) != 0,
            retreating: b(&u["retreating"]),
            forced_reserve: b(&u["logistics"]["forced"]),
        })
        .collect()
}
fn bit(v: f64) -> String {
    format!("{:016x}", v.to_bits())
}
fn index_json(v: Option<usize>) -> Value {
    json!(v.map_or(-1, |i| i as i64))
}
fn view<'a>(w: usize, h: usize, flags: &'a [u16], heights: Option<&'a [i8]>) -> TerrainView<'a> {
    TerrainView {
        width: w,
        height: h,
        flags,
        heights,
        world_known: false,
        knowledge_id: 0,
        versions: Versions::default(),
        changed: None,
        changes: &[],
    }
}
#[test]
fn capture_progress_order_prerequisites_and_events_match_source() {
    let f = fixture();
    let mut count = 0;
    for row in f["captures"].as_array().unwrap() {
        let players = player_rows(&row["players"]);
        let units = unit_rows(&row["units"]);
        let mut points = objective_rows(&row["points"]);
        let mut events = vec![];
        for (step, expected) in row["snapshots"].as_array().unwrap().iter().enumerate() {
            let changes = capture_points(&mut points, &units, &players, n(&row["dt"])).unwrap();
            events.extend(changes.into_iter().map(|e|json!({"index":e.objective_index,"id":e.objective_id,"owner":e.owner,"tick":step+1})));
            let result:Vec<_>=points.iter().map(|p|json!({"owner":index_json(p.owner),"capper":index_json(p.capper),"progress":bit(p.progress),"onPoint":p.on_point,"contested":p.contested,"cut":p.cut})).collect();
            assert_eq!(
                json!(result),
                *expected,
                "capture {} step {step}",
                row["name"]
            );
            count += 1;
        }
        assert_eq!(
            json!(events),
            row["events"],
            "capture events {}",
            row["name"]
        );
    }
    assert!(count > 1000);
}
#[test]
fn supply_masks_flood_order_and_cut_flags_match_source() {
    let f = fixture();
    let mut placer = SpawnPlacer::new();
    for row in f["supplies"].as_array().unwrap() {
        let players = player_rows(&row["players"]);
        let units = unit_rows(&row["units"]);
        let mut points = objective_rows(&row["points"]);
        let flags: Vec<_> = row["flags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u16)
            .collect();
        let heights: Option<Vec<_>> = row["heights"]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_i64().unwrap() as i8).collect());
        let terrain = view(
            row["w"].as_u64().unwrap() as usize,
            row["h"].as_u64().unwrap() as usize,
            &flags,
            heights.as_deref(),
        );
        let reports = supply_lines(terrain, &mut points, &units, &players, &mut placer).unwrap();
        let reports: Vec<_> = reports
            .into_iter()
            .map(|r| json!({"team":r.team,"closed":r.closed,"reached":r.reached}))
            .collect();
        assert_eq!(json!(reports), row["teams"], "supply masks {}", row["name"]);
        assert_eq!(
            json!(points.iter().map(|p| p.cut).collect::<Vec<_>>()),
            row["cuts"],
            "cuts {}",
            row["name"]
        );
    }
}
#[test]
fn invalid_inputs_leave_all_objectives_unchanged() {
    let f = fixture();
    let row = &f["captures"][0];
    let players = player_rows(&row["players"]);
    let units = unit_rows(&row["units"]);
    let original = objective_rows(&row["points"]);
    for dt in [f64::NAN, f64::INFINITY, -1.0] {
        let mut points = original.clone();
        assert!(capture_points(&mut points, &units, &players, dt).is_err());
        assert_eq!(points, original);
    }
    let mut points = original.clone();
    points.push(Objective {
        owner: Some(99),
        ..original[0].clone()
    });
    let before = points.clone();
    assert!(capture_points(&mut points, &units, &players, 0.05).is_err());
    assert_eq!(points, before);
    points = original.clone();
    points[0].needs = Some(1);
    let before = points.clone();
    assert!(capture_points(&mut points, &units, &players, 0.05).is_err());
    assert_eq!(points, before);
    let mut invalid_units = units.clone();
    invalid_units.push(ObjectiveUnit {
        kind: "missing",
        ..units[0]
    });
    let mut points = original.clone();
    assert!(capture_points(&mut points, &invalid_units, &players, 0.05).is_err());
    assert_eq!(points, original);
    let flags = [0];
    let terrain = view(2, 2, &flags, None);
    assert!(
        supply_lines(
            terrain,
            &mut points,
            &units,
            &players,
            &mut SpawnPlacer::new()
        )
        .is_err()
    );
    assert_eq!(points, original);
    let mut invalid_players = players.clone();
    invalid_players[1].spawn.x = f64::NAN;
    assert!(
        supply_lines(
            view(1, 1, &flags, None),
            &mut points,
            &units,
            &invalid_players,
            &mut SpawnPlacer::new()
        )
        .is_err()
    );
    assert_eq!(points, original);
}

#[test]
fn catalog_rules_and_custom_invalid_rules_are_checked() {
    let rules = ObjectiveRules::compiled().unwrap();
    assert_eq!(
        rules,
        ObjectiveRules {
            capture_time: 8.0,
            point_radius: 8.0,
            zoc_radius: 8.0,
            free_radius: 16.0
        }
    );
    let f = fixture();
    let row = &f["captures"][0];
    let players = player_rows(&row["players"]);
    let units = unit_rows(&row["units"]);
    let original = objective_rows(&row["points"]);
    for bad in [
        ObjectiveRules {
            capture_time: 0.0,
            ..rules
        },
        ObjectiveRules {
            zoc_radius: f64::NAN,
            ..rules
        },
        ObjectiveRules {
            free_radius: -1.0,
            ..rules
        },
    ] {
        let mut points = original.clone();
        assert!(capture_points_with_rules(&mut points, &units, &players, 0.05, bad).is_err());
        assert_eq!(points, original);
        let flags = [0u16];
        assert!(
            supply_lines_with_rules(
                view(1, 1, &flags, None),
                &mut points,
                &units,
                &players,
                &mut SpawnPlacer::new(),
                bad
            )
            .is_err()
        );
        assert_eq!(points, original);
    }
}
