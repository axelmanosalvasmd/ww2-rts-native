use serde_json::{Value, json};
use ww2_core::weather::{
    MapWeatherInfo, RoadMap, WeatherChoice, WeatherKind, WeatherPlan, WeatherRow, WeatherRules,
    map_mood, road_mask,
};

// Reuse the existing public map bytes. This test does not link to the server crate.
mod authored {
    include!("../../server/src/map-data.rs");
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/weather.json")).unwrap()
}
fn metadata(value: &Value) -> MapWeatherInfo<'_> {
    MapWeatherInfo {
        name: value["name"].as_str(),
        mood: value["mood"].as_str(),
        weather: value["weather"].as_str(),
    }
}
fn plan(value: &Value) -> Option<WeatherPlan> {
    (!value.is_null()).then(|| WeatherPlan {
        now: WeatherKind::try_from(value["now"].as_str().unwrap()).unwrap(),
        next: value["next"]
            .as_str()
            .map(|s| WeatherKind::try_from(s).unwrap()),
        at: value["at"].as_u64().unwrap() as u32,
    })
}
fn plan_json(plan: Option<WeatherPlan>) -> Value {
    match plan {
        None => Value::Null,
        Some(p) => json!({"now":p.now.name(),"next":p.next.map(WeatherKind::name),"at":p.at}),
    }
}
fn row_json(row: WeatherRow) -> Value {
    match row {
        WeatherRow::Current(now) => json!([now.name()]),
        WeatherRow::Changing { now, next, seconds } => json!([now.name(), next.name(), seconds]),
    }
}
fn hash(bytes: &[u8]) -> String {
    format!(
        "{:016x}",
        bytes
            .iter()
            .fold(0xcbf29ce484222325_u64, |h, b| (h ^ u64::from(*b))
                .wrapping_mul(0x100000001b3))
    )
}

#[test]
fn map_defaults_and_seeded_plans_match_original_javascript() {
    let f = fixture();
    let rules = WeatherRules::compiled().unwrap();
    for row in f["metadata"].as_array().unwrap() {
        assert_eq!(
            map_mood(metadata(&row["map"]), row["key"].as_str()).name(),
            row["mood"].as_str().unwrap(),
            "{row}"
        );
        assert_eq!(
            plan_json(Some(
                rules.map_weather(metadata(&row["map"]), row["key"].as_str())
            )),
            row["expected"],
            "{row}"
        );
    }
    let mut kinds = [false; 5];
    let mut changed = false;
    let mut held = false;
    for row in f["plans"].as_array().unwrap() {
        let m = &f["metadata"][row[1].as_u64().unwrap() as usize];
        let p = rules.plan_weather(
            WeatherChoice::from_reference(row[0].as_str()),
            metadata(&m["map"]),
            m["key"].as_str(),
            row[2].as_u64().unwrap() as u32,
        );
        kinds[p.now as usize] = true;
        changed |= p.next.is_some();
        held |= p.next.is_none();
        assert_eq!(plan_json(Some(p)), row[3], "{row}");
    }
    assert!(kinds.iter().all(|v| *v) && changed && held);
}
#[test]
fn transition_ticks_and_public_warning_rows_match_javascript() {
    let f = fixture();
    let rules = WeatherRules::compiled().unwrap();
    for row in f["transitions"].as_array().unwrap() {
        let mut p = plan(&row["plan"]);
        let tick = row["tick"].as_u64().unwrap();
        assert_eq!(
            row_json(rules.weather_row(p.as_ref(), tick)),
            row["row"],
            "{row}"
        );
        if let Some(p) = p.as_mut() {
            p.step(tick);
        }
        assert_eq!(plan_json(p), row["after"], "{row}");
        if let Some(p) = p.as_mut() {
            assert!(!p.step(tick), "a completed step must be stable");
        }
    }
}
#[test]
fn descriptions_and_ai_caution_match_javascript() {
    let f = fixture();
    let rules = WeatherRules::compiled().unwrap();
    for row in f["effects"].as_array().unwrap() {
        let kind = row["kind"]
            .as_str()
            .map(|s| WeatherKind::try_from(s).unwrap());
        assert_eq!(
            serde_json::to_value(rules.weather_effects(kind)).unwrap(),
            row["effects"]
        );
        let p = kind.map(WeatherPlan::fixed);
        assert_eq!(
            rules.ai_caution(p.as_ref()).to_bits(),
            u64::from_str_radix(row["caution"].as_str().unwrap(), 16).unwrap()
        );
    }
}
#[test]
fn road_masks_match_synthetic_edges_and_all_authored_maps() {
    let f = fixture();
    for row in f["roads"].as_array().unwrap() {
        let rows: Vec<_> = row["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let map = RoadMap::new(
            row["w"].as_u64().unwrap() as usize,
            row["h"].as_u64().unwrap() as usize,
            &rows,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(road_mask(&map)).unwrap(),
            row["expected"],
            "{row}"
        );
    }
    assert_eq!(authored::MAPS.len(), 33);
    for row in f["authored"].as_array().unwrap() {
        let (_, text) = authored::MAPS
            .iter()
            .find(|(name, _)| *name == row["name"].as_str().unwrap())
            .unwrap();
        let m: Value = serde_json::from_str(text).unwrap();
        let rows: Vec<_> = m["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let map = RoadMap::new(
            m["w"].as_u64().unwrap() as usize,
            m["h"].as_u64().unwrap() as usize,
            &rows,
        )
        .unwrap();
        let mask = road_mask(&map);
        assert_eq!(mask.len() as u64, row["cells"].as_u64().unwrap());
        assert_eq!(
            hash(&mask),
            row["hash"].as_str().unwrap(),
            "{}",
            row["name"]
        );
    }
}
#[test]
fn invalid_typed_kinds_and_map_shapes_are_rejected() {
    for kind in ["", "unknown", "Snow", "__proto__", "constructor", "rain\0"] {
        assert!(WeatherKind::try_from(kind).is_err());
        assert_eq!(
            WeatherChoice::from_reference(Some(kind)),
            WeatherChoice::Map
        );
    }
    for (w, h, rows) in [
        (0, 0, vec![]),
        (2, 1, vec!["."]),
        (1, 2, vec!["."]),
        (1, 1, vec!["é"]),
        (usize::MAX, 2, vec![".", "."]),
    ] {
        assert!(RoadMap::new(w, h, &rows).is_err());
    }
}
