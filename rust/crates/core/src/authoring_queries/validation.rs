//! Raw editor validation in the pinned prototype's check order.
use super::Result;
use crate::map::MapError;
use serde_json::Value;
fn int(v: &Value, lo: usize, hi: usize) -> bool {
    v.as_f64()
        .is_some_and(|v| v.is_finite() && v.fract() == 0.0 && v >= lo as f64 && v <= hi as f64)
}
fn num(v: &Value, lo: f64, hi: f64) -> bool {
    v.as_f64()
        .is_some_and(|v| v.is_finite() && v >= lo && v <= hi)
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
fn message(e: MapError) -> String {
    match e {
        MapError::Invalid { reason, .. } => reason,
        _ => e.to_string(),
    }
}
pub(super) fn validate(m: &Value) -> Result<()> {
    macro_rules! need {
        ($ok:expr,$text:expr) => {
            if !$ok {
                return Err($text.to_owned());
            }
        };
    }
    need!(m.is_object() || m.is_array(), "not a map");
    need!(
        m["name"]
            .as_str()
            .is_some_and(|s| s.encode_utf16().count() <= 40),
        "name must be a string up to 40 chars"
    );
    need!(
        int(&m["w"], 20, 1024) && int(&m["h"], 20, 1024),
        "size must be 20-1024 cells"
    );
    let w = m["w"].as_f64().unwrap() as usize;
    let h = m["h"].as_f64().unwrap() as usize;
    need!(
        m["rows"].as_array().is_some_and(|a| a.len() == h),
        "row count must equal height"
    );
    let rows = m["rows"].as_array().unwrap();
    need!(
        rows.iter().all(|r| r
            .as_str()
            .is_some_and(|s| s.len() == w && s.bytes().all(|c| b".BH#+TWF=RXYDMNOQ".contains(&c)))),
        format!("rows must be {w} valid terrain chars")
    );
    let chars = rows
        .iter()
        .flat_map(|r| r.as_str().unwrap().bytes())
        .collect::<Vec<_>>();
    if let Some(v) = m.get("heights") {
        need!(
            v.as_array().is_some_and(|a| a.len() == h
                && a.iter().all(|r| r
                    .as_str()
                    .is_some_and(|s| s.len() == w && s.bytes().all(|c| b"01234ab".contains(&c))))),
            format!("heights must be {h} rows of 0-4 (a/b for depths -1/-2)")
        );
    }
    if let Some(version) = m.get("worldVersion") {
        need!(version.as_f64() == Some(2.0), "worldVersion must be 2");
    }
    if m.get("layers").is_none() {
        need!(
            m.get("worldVersion").is_none(),
            "version 2 needs world layers"
        );
    }
    if let Some(layers) = m.get("layers") {
        need!(
            m["worldVersion"].as_f64() == Some(2.0),
            "world layers need worldVersion 2"
        );
        for (key, allowed) in [
            ("ground", b".D+MWF".as_slice()),
            ("objects", b".BH#TXY=RNO".as_slice()),
        ] {
            need!(
                layers[key].as_array().is_some_and(|a| a.len() == h
                    && a.iter().all(|v| v
                        .as_str()
                        .is_some_and(|s| s.len() == w && s.bytes().all(|c| allowed.contains(&c))))),
                format!("layers.{key} must contain {h} rows of {w} valid cells")
            );
        }
        if let Some(mines) = layers.get("mines") {
            need!(
                mines.as_array().is_some_and(|a| a.len() == h
                    && a.iter().all(|v| v
                        .as_str()
                        .is_some_and(|s| s.len() == w && s.bytes().all(|c| b".N".contains(&c))))),
                "layers.mines must contain valid rows of mine markers"
            );
        }
        for c in 0..w * h {
            let ground = layers["ground"][c / w].as_str().unwrap().as_bytes()[c % w];
            let authored = layers["objects"][c / w].as_str().unwrap().as_bytes()[c % w];
            let object = if authored == b'N' { b'.' } else { authored };
            let mine = authored == b'N'
                || layers
                    .get("mines")
                    .is_some_and(|a| a[c / w].as_str().unwrap().as_bytes()[c % w] == b'N');
            need!(
                crate::map::compose_world_cell(ground, object, mine) == chars[c],
                "rows must match the composed world layers"
            );
            need!(!(mine && b"WF".contains(&ground)), "mines need dry ground");
            need!(
                !(object == b'=' && ground != b'W'),
                "bridge sections need water below them"
            );
            need!(
                !(object != b'.' && object != b'=' && b"WF".contains(&ground)),
                "land objects need dry ground"
            );
        }
        for key in [
            "materials",
            "groundMaterials",
            "objectMaterials",
            "mineMaterials",
        ] {
            if let Some(list) = layers.get(key) {
                need!(
                    list.as_array().is_some_and(|a| a.len() <= w * h
                        && a.iter().all(|r| int(&r["c"], 0, w * h - 1)
                            && r["material"].as_str().is_some_and(|s| [
                                "soil", "road", "wood", "stone", "concrete", "steel", "water"
                            ]
                            .contains(&s)))
                        && (0..a.len()).all(|i| !a[..i]
                            .iter()
                            .any(|r| r["c"].as_f64() == a[i]["c"].as_f64()))),
                    "materials need unique valid cells and material names"
                );
            }
        }
    }
    let layers = crate::map::world_layers(m).map_err(message)?;
    crate::structures::parse(m.get("structures"), w, h, &layers).map_err(message)?;
    let world = truthy(&m["world"]);
    let minimum = if world { 1 } else { 2 };
    need!(
        m["spawns"]
            .as_array()
            .is_some_and(|a| (minimum..=6).contains(&a.len())),
        "needs 2-6 spawns"
    );
    let spawns = m["spawns"].as_array().unwrap();
    let pos = |v: &Value| int(&v["x"], 0, w - 1) && int(&v["y"], 0, h - 1);
    let cell =
        |v: &Value| v["y"].as_f64().unwrap() as usize * w + v["x"].as_f64().unwrap() as usize;
    let open = |v: &Value| pos(v) && crate::map::flags(chars[cell(v)]) & crate::game::MOVE == 0;
    need!(
        spawns.iter().all(open),
        "spawns must be on open ground inside the map"
    );
    need!(
        spawns
            .iter()
            .all(|s| s.get("assault").is_none_or(Value::is_boolean)),
        "spawn assault flag must be true or false"
    );
    need!(
        spawns.iter().filter(|s| !truthy(&s["assault"])).count() >= minimum,
        "needs 2+ spawns that every mode can use"
    );
    if let Some(v) = m.get("assaultTime") {
        need!(int(v, 300, 3600), "assaultTime must be 300-3600 seconds");
    }
    if let Some(v) = m.get("defend") {
        need!(
            v.as_array().is_some_and(|a| !a.is_empty()
                && a.len() < spawns.len()
                && a.iter().all(|v| int(v, 0, spawns.len() - 1))
                && (0..a.len()).all(|i| !a[..i].contains(&a[i]))),
            "defend must list some (not all) spawn numbers"
        );
    }
    for key in ["naval", "trenchFacing"] {
        if let Some(v) = m.get(key) {
            need!(v.is_boolean(), format!("{key} must be true or false"));
        }
    }
    need!(
        m["points"].as_array().is_some_and(|a| if world {
            a.is_empty()
        } else {
            (1..=9).contains(&a.len())
        }),
        "needs 1-9 capture points"
    );
    let points = m["points"].as_array().unwrap();
    need!(
        points.iter().all(|p| open(p)
            && ["vp", "mp"]
                .iter()
                .all(|k| p.get(k).is_none_or(|v| v.is_null() || num(v, 0.0, 5.0)))),
        "points must be on open ground, vp and mp 0-5"
    );
    if let Some(v) = m.get("pointSequence") {
        need!(
            int(v, 0, 1_000_000_000),
            "pointSequence must be a number from 0 to 1000000000"
        );
        need!(
            points.iter().all(|p| p["id"]
                .as_str()
                .and_then(|s| s.strip_prefix("point:"))
                .filter(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
                .is_none_or(|s| s.parse::<f64>().unwrap_or(f64::INFINITY) < v.as_f64().unwrap())),
            "pointSequence must exceed existing point IDs"
        );
    }
    need!(
        points.iter().all(|p| p
            .get("kind")
            .is_none_or(|v| matches!(v.as_str(), Some("radio" | "depot")))),
        "point kind must be one of radio, depot"
    );
    need!(
        points
            .iter()
            .enumerate()
            .all(|(i, p)| p.get("needs").is_none_or(|v| !points.is_empty()
                && int(v, 0, points.len() - 1)
                && v.as_f64() != Some(i as f64))),
        "a point can only need another point"
    );
    for i in 0..points.len() {
        let mut j = i;
        for k in 0..=points.len() + 1 {
            let Some(next) = points[j].get("needs") else {
                break;
            };
            j = next.as_f64().unwrap() as usize;
            need!(k <= points.len(), "points need each other in a loop");
        }
    }
    let catalog = crate::catalog::Catalog::compiled().map_err(|e| e.to_string())?;
    let houses = catalog
        .table("CFG")
        .map_err(|e| e.to_string())?
        .get("houses")
        .and_then(crate::catalog::Value::array)
        .ok_or("missing house rules")?;
    let names = houses
        .iter()
        .filter_map(|h| h.get("name").and_then(crate::catalog::Value::string))
        .collect::<Vec<_>>();
    if let Some(v) = m.get("buildings") {
        need!(
            v.as_array().is_some_and(|a| a.len() <= 500),
            "buildings must be a list (up to 500)"
        );
        for b in v.as_array().unwrap() {
            need!(pos(b), "buildings must be inside the map");
            let kind = b["kind"].as_str();
            need!(
                if kind == Some("stone bridge") {
                    chars[cell(b)] == b'='
                } else {
                    chars[cell(b)] == b'B' && kind.is_some_and(|k| names.contains(&k))
                },
                format!(
                    "building at {},{}: a house type ({}) on a house, or 'stone bridge' on a bridge",
                    b["x"],
                    b["y"],
                    names.join(", ")
                )
            );
        }
    }
    if let Some(v) = m.get("triggers") {
        need!(
            v.as_array().is_some_and(|a| a.len() <= 20),
            "triggers must be a list (up to 20)"
        );
        for t in v.as_array().unwrap() {
            need!(
                num(&t["at"], 0.0, 7200.0),
                "a trigger needs at: 0-7200 seconds"
            );
            if let Some(v) = t.get("say") {
                need!(
                    v.as_str()
                        .is_some_and(|s| (1..=140).contains(&s.encode_utf16().count())),
                    "a trigger says 1-140 characters"
                );
            }
            if let Some(v) = t.get("blow") {
                need!(
                    v.as_array().is_some_and(|a| a.len() == 4
                        && int(&a[0], 0, w - 1)
                        && int(&a[1], 0, h - 1)
                        && int(&a[2], a[0].as_f64().unwrap() as usize, w - 1)
                        && int(&a[3], a[1].as_f64().unwrap() as usize, h - 1)),
                    "a trigger blows a box [x0, y0, x1, y1] inside the map"
                );
            }
            need!(
                t.get("say").is_some() || t.get("blow").is_some(),
                "a trigger must say or blow something"
            );
        }
    }
    crate::scenario_validation::validate_scenario(m, catalog).map_err(|e| e.to_string())?;
    if world {
        let heights = m.get("heights").map(|v| {
            v.as_array()
                .unwrap()
                .iter()
                .flat_map(|r| r.as_str().unwrap().bytes())
                .map(|c| crate::map::level_of(c).unwrap())
                .collect::<Vec<_>>()
        });
        crate::world_map::validate_geography(
            &m["world"],
            w,
            h,
            &chars,
            heights.as_deref(),
            spawns.len(),
        )
        .map_err(str::to_owned)?;
    }
    Ok(())
}
