//! HTTP presentation of existing map diagnostics, from reference sim.js:574-620.
//! Validation remains in the core. This adapter does not decide acceptance.
use serde_json::Value;
use ww2_core::map::MapError;

pub(crate) fn message(error: &MapError, raw: &Value) -> String {
    let MapError::Invalid { field, reason } = error else {
        return error.to_string();
    };
    let text = match field.as_str() {
        "document" if reason == "expected a map object" => {
            // JavaScript arrays are objects, so [] reaches the name check.
            if raw.is_array() {
                "name must be a string up to 40 chars"
            } else {
                "not a map"
            }
        }
        "name" => "name must be a string up to 40 chars",
        "w" | "h" => "size must be 20-1024 cells",
        "rows" | "heights" | "structures" => return reason.clone(),
        "scenario" => return scenario_message(reason, raw),
        "worldVersion" => {
            if raw.get("worldVersion").is_some() && raw["worldVersion"].as_f64() != Some(2.0) {
                "worldVersion must be 2"
            } else if raw.get("layers").is_none() {
                "version 2 needs world layers"
            } else {
                "world layers need worldVersion 2"
            }
        }
        "layers.ground" | "layers.objects" => {
            return format!(
                "{field} must contain {} rows of {} valid cells",
                raw["h"], raw["w"]
            );
        }
        "layers.mines" => "layers.mines must contain valid rows of mine markers",
        "layers" => match reason.as_str() {
            "rows must match composed layers" => "rows must match the composed world layers",
            "bridge sections need water" => "bridge sections need water below them",
            _ => return reason.clone(),
        },
        "materials" | "groundMaterials" | "objectMaterials" | "mineMaterials" => {
            "materials need unique valid cells and material names"
        }
        "spawns" => match reason.as_str() {
            "expected an array" | "needs 1-6 spawns" | "needs 2-6 spawns" => "needs 2-6 spawns",
            "needs a non-assault spawn" | "needs two non-assault spawns" => {
                "needs 2+ spawns that every mode can use"
            }
            _ => "spawns must be on open ground inside the map",
        },
        "assault" => "spawn assault flag must be true or false",
        "assaultTime" => "assaultTime must be 300-3600 seconds",
        "defend" => "defend must list some (not all) spawn numbers",
        "naval" => "naval must be true or false",
        "trenchFacing" => "trenchFacing must be true or false",
        "points" => match reason.as_str() {
            "expected an array"
            | "needs 1-9 capture points"
            | "World maps need zero capture points" => "needs 1-9 capture points",
            _ => "points must be on open ground, vp and mp 0-5",
        },
        "vp" | "mp" => "points must be on open ground, vp and mp 0-5",
        "pointSequence" if reason == "must exceed existing point IDs" => {
            "pointSequence must exceed existing point IDs"
        }
        "pointSequence" => "pointSequence must be a number from 0 to 1000000000",
        "points.kind" => "point kind must be one of radio, depot",
        "points.needs" if reason == "dependency cycle" => "points need each other in a loop",
        "points.needs" => "a point can only need another point",
        "points.id" if reason == "expected a stable identifier" => {
            "scenario: capture targets need stable IDs"
        }
        "buildings" if reason == "expected an array" || reason == "at most 500 entries" => {
            "buildings must be a list (up to 500)"
        }
        "buildings.kind" => return building_kind(raw),
        "buildings" if reason == "kind must match house or bridge cell" => {
            return building_kind(raw);
        }
        "buildings" => "buildings must be inside the map",
        "triggers" if reason == "expected an array" || reason == "at most 20 entries" => {
            "triggers must be a list (up to 20)"
        }
        "triggers" if reason == "must say or blow something" => {
            "a trigger must say or blow something"
        }
        "triggers.at" => "a trigger needs at: 0-7200 seconds",
        "triggers.say" => "a trigger says 1-140 characters",
        "triggers.blow" if reason != "scenario boxes contain at most 256 cells" => {
            "a trigger blows a box [x0, y0, x1, y1] inside the map"
        }
        "world" | "world.size" | "world.total" | "world.regionCells" => "invalid world regions",
        "world.regions" if reason == "count must match size" => "invalid world regions",
        "world.regions" if reason == "objectives need connected ground routes" => {
            "world regions need connected ground routes"
        }
        "world.regions.home" if reason == "each player needs one home" => {
            "each player needs one home"
        }
        "world.regions.home" => "invalid world home",
        "world.regionMap" => "invalid region membership",
        "world.regions"
        | "world.regions.id"
        | "world.regions.x"
        | "world.regions.y"
        | "world.regions.bounds" => "invalid world region",
        _ => return error.to_string(),
    };
    text.to_owned()
}

fn scenario_message(reason: &str, raw: &Value) -> String {
    // The core validates migrated scenarios before legacy trigger diagnostics.
    // The prototype reports those legacy fields first. Use the native error's
    // trigger identity and category to select the corresponding old message.
    let legacy = raw.get("scenario").is_none_or(|v| match v {
        Value::Null => true,
        Value::Bool(v) => !v,
        Value::Number(v) => v.as_f64() == Some(0.0),
        Value::String(v) => v.is_empty(),
        _ => false,
    });
    if legacy && reason == "triggers exceeds 32" {
        return "triggers must be a list (up to 20)".into();
    }
    if legacy
        && let Some(tail) = reason.strip_prefix("trigger legacy:")
        && let Some((index, detail)) = tail.split_once(' ')
        && let Ok(index) = index.parse::<usize>()
    {
        let trigger = &raw["triggers"][index];
        match detail {
            "has an invalid condition or reference" => {
                return "a trigger needs at: 0-7200 seconds".into();
            }
            "has invalid announcement" => return "a trigger says 1-140 characters".into(),
            "has an invalid damage area" => {
                // A well-shaped box can still exceed the scenario work limit.
                // That remains a scenario error rather than a legacy shape error.
                if let Some(bounds) = trigger["blow"].as_array()
                    && bounds.len() == 4
                    && bounds.iter().all(|v| v.as_u64().is_some())
                {
                    let bounds = bounds
                        .iter()
                        .map(|v| v.as_u64().unwrap())
                        .collect::<Vec<_>>();
                    if bounds[0] <= bounds[2]
                        && bounds[1] <= bounds[3]
                        && raw["w"].as_u64().is_some_and(|w| bounds[2] < w)
                        && raw["h"].as_u64().is_some_and(|h| bounds[3] < h)
                    {
                        return format!("scenario: {reason}");
                    }
                }
                return "a trigger blows a box [x0, y0, x1, y1] inside the map".into();
            }
            "needs 1-8 actions" => {
                return if trigger.get("say").is_some() {
                    "a trigger says 1-140 characters"
                } else if trigger.get("blow").is_some() {
                    "a trigger blows a box [x0, y0, x1, y1] inside the map"
                } else {
                    "a trigger must say or blow something"
                }
                .into();
            }
            _ => {}
        }
    }
    format!("scenario: {reason}")
}

// Native diagnostics omit the offending building's index. Locate that row only
// when presenting an already-rejected kind or footprint; do not revalidate maps.
fn building_kind(raw: &Value) -> String {
    let catalog = ww2_core::catalog::Catalog::compiled().expect("compiled catalogue");
    let houses = catalog
        .table("CFG")
        .unwrap()
        .get("houses")
        .unwrap()
        .array()
        .unwrap()
        .iter()
        .map(|house| house.get("name").unwrap().string().unwrap())
        .collect::<Vec<_>>();
    let building = raw["buildings"]
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|row| {
                let cell = row["x"].as_u64().and_then(|x| {
                    row["y"].as_u64().and_then(|y| {
                        raw["rows"]
                            .get(y as usize)?
                            .as_str()?
                            .as_bytes()
                            .get(x as usize)
                            .copied()
                    })
                });
                match row["kind"].as_str() {
                    Some("stone bridge") => cell != Some(b'='),
                    Some(kind) => cell != Some(b'B') || !houses.contains(&kind),
                    None => true,
                }
            })
        })
        .expect("rejected building kind has an offending row");
    format!(
        "building at {},{}: a house type ({}) on a house, or 'stone bridge' on a bridge",
        building["x"],
        building["y"],
        houses.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ordinary() -> Value {
        json!({"name":"Editor","w":20,"h":20,"rows":vec!["....................";20],
            "spawns":[{"x":1,"y":1},{"x":18,"y":18}],"points":[{"x":10,"y":10}]})
    }
    fn rejected(raw: &Value) -> String {
        let bytes = serde_json::to_vec(raw).unwrap();
        let error = ww2_core::map_save::prepare_map_save(&bytes).unwrap_err();
        message(&error, raw)
    }

    #[test]
    fn net12_editor_diagnostics_match_literal_prototype_errors() {
        for (raw, expected) in [
            (Value::Null, "not a map"),
            (json!(7), "not a map"),
            (json!([]), "name must be a string up to 40 chars"),
        ] {
            assert_eq!(rejected(&raw), expected);
        }
        let cases = [
            ("name", Value::Null, "name must be a string up to 40 chars"),
            ("w", json!(19), "size must be 20-1024 cells"),
            ("rows", json!([]), "row count must equal height"),
            (
                "heights",
                json!([]),
                "heights must be 20 rows of 0-4 (a/b for depths -1/-2)",
            ),
            ("spawns", Value::Null, "needs 2-6 spawns"),
            (
                "spawns",
                json!([{"x":-1,"y":1},{"x":18,"y":18}]),
                "spawns must be on open ground inside the map",
            ),
            (
                "spawns",
                json!([{"x":1,"y":1,"assault":"yes"},{"x":18,"y":18}]),
                "spawn assault flag must be true or false",
            ),
            (
                "spawns",
                json!([{"x":1,"y":1,"assault":true},{"x":18,"y":18}]),
                "needs 2+ spawns that every mode can use",
            ),
            (
                "assaultTime",
                json!(1),
                "assaultTime must be 300-3600 seconds",
            ),
            (
                "defend",
                json!([0, 0]),
                "defend must list some (not all) spawn numbers",
            ),
            ("naval", json!(1), "naval must be true or false"),
            (
                "trenchFacing",
                json!(1),
                "trenchFacing must be true or false",
            ),
            ("points", Value::Null, "needs 1-9 capture points"),
            (
                "points",
                json!([{"x":10,"y":10,"vp":6}]),
                "points must be on open ground, vp and mp 0-5",
            ),
            (
                "points",
                json!([{"x":10,"y":10,"kind":"bad"}]),
                "point kind must be one of radio, depot",
            ),
            (
                "points",
                json!([{"x":10,"y":10,"needs":0}]),
                "a point can only need another point",
            ),
            (
                "pointSequence",
                json!(-1),
                "pointSequence must be a number from 0 to 1000000000",
            ),
            (
                "buildings",
                json!(false),
                "buildings must be a list (up to 500)",
            ),
            (
                "buildings",
                json!([{"x":-1,"y":0,"kind":"bad"}]),
                "buildings must be inside the map",
            ),
            (
                "triggers",
                json!(false),
                "triggers must be a list (up to 20)",
            ),
            (
                "triggers",
                json!([{"at":-1}]),
                "a trigger needs at: 0-7200 seconds",
            ),
            (
                "triggers",
                json!([{"at":1,"say":""}]),
                "a trigger says 1-140 characters",
            ),
            (
                "triggers",
                json!([{"at":1,"blow":[0,0]}]),
                "a trigger blows a box [x0, y0, x1, y1] inside the map",
            ),
            (
                "triggers",
                json!([{"at":1}]),
                "a trigger must say or blow something",
            ),
            ("worldVersion", json!(3), "worldVersion must be 2"),
            ("worldVersion", json!(2), "version 2 needs world layers"),
            (
                "structures",
                json!(false),
                "structures must be a bounded list",
            ),
            (
                "scenario",
                json!({"version":2}),
                "scenario: version must be 1",
            ),
        ];
        for (field, value, expected) in cases {
            let mut raw = ordinary();
            raw[field] = value;
            assert_eq!(rejected(&raw), expected, "{field}");
        }
    }

    #[test]
    fn net12_layer_diagnostics_match_literal_prototype_errors() {
        let mut raw = ordinary();
        raw["worldVersion"] = json!(2);
        raw["layers"] = json!({"ground":vec!["....................";20],"objects":vec!["....................";20]});
        for (field, value, expected) in [
            (
                "ground",
                json!([]),
                "layers.ground must contain 20 rows of 20 valid cells",
            ),
            (
                "objects",
                json!([]),
                "layers.objects must contain 20 rows of 20 valid cells",
            ),
            (
                "mines",
                json!([]),
                "layers.mines must contain valid rows of mine markers",
            ),
            (
                "materials",
                json!([{"c":0,"material":"bad"}]),
                "materials need unique valid cells and material names",
            ),
        ] {
            let mut bad = raw.clone();
            bad["layers"][field] = value;
            assert_eq!(rejected(&bad), expected);
        }
    }
    #[test]
    fn net12_invalid_maps_report_prototype_first_error_across_fields() {
        let mut raw = ordinary();
        raw["spawns"] = json!([{"x":1,"y":1,"assault":"bad"},{"x":-1,"y":18}]);
        assert_eq!(
            rejected(&raw),
            "spawns must be on open ground inside the map"
        );
        let mut raw = ordinary();
        raw["pointSequence"] = json!(-1);
        raw["points"][0]["x"] = json!(-1);
        assert_eq!(
            rejected(&raw),
            "points must be on open ground, vp and mp 0-5"
        );
        let mut raw = ordinary();
        raw["points"] = json!([{"x":1,"y":1,"kind":"bad"},{"x":2,"y":2,"vp":8}]);
        assert_eq!(
            rejected(&raw),
            "points must be on open ground, vp and mp 0-5"
        );
        let mut raw = ordinary();
        raw["scenario"] = json!({"version":2});
        raw["buildings"] = json!(false);
        assert_eq!(rejected(&raw), "buildings must be a list (up to 500)");
        let mut raw = ordinary();
        raw["scenario"] = json!({"version":2});
        raw["triggers"] = json!([{"at":-1}]);
        assert_eq!(rejected(&raw), "a trigger needs at: 0-7200 seconds");
    }
}
