//! MAP-92 checks authored geography through the native map validator.
use crate::map::{AuthoritativeMap, MapError};
use serde_json::{Value, json};

const SIDE: usize = 20;

fn authored(count: usize) -> Value {
    let columns = if count == 128 { 16 } else { 8 };
    let regions: Vec<_> = (0..count)
        .map(|id| {
            let mut region = json!({
                "id":id, "x":2 + id % columns, "y":2 + id / columns,
                "bounds":[0,0,SIDE,SIDE]
            });
            if id == 0 {
                region["home"] = json!(0);
            } else if id == count - 1 {
                region["home"] = json!(1);
            }
            region
        })
        .collect();
    json!({
        "name":"Authored World validation", "w":SIDE, "h":SIDE,
        "rows":vec![".".repeat(SIDE);SIDE],
        "heights":vec!["0".repeat(SIDE);SIDE],
        "spawns":[{"x":1,"y":1},{"x":18,"y":18}], "points":[],
        "world":{
            "size":if count == 128 { "massive" } else { "huge" },
            "total":count, "regionCells":64, "regions":regions,
            "regionMap":vec![-1;SIDE * SIDE]
        }
    })
}

fn load(value: &Value) -> Result<AuthoritativeMap, MapError> {
    AuthoritativeMap::from_json(&serde_json::to_vec(value).unwrap())
}

fn rejects(value: &Value, expected: &str) {
    let error = load(value).expect_err("corrupt geography must be rejected");
    assert_eq!(error.to_string(), expected);
    assert_eq!(validate(value), Err(expected));
}

fn validate(value: &Value) -> Result<(), &'static str> {
    let chars: Vec<_> = value["rows"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| row.as_str().unwrap().bytes())
        .collect();
    let heights: Option<Vec<_>> = value.get("heights").map(|rows| {
        rows.as_array()
            .unwrap()
            .iter()
            .flat_map(|row| row.as_str().unwrap().bytes())
            .map(|ch| match ch {
                b'a' => -1,
                b'b' => -2,
                _ => (ch - b'0') as i8,
            })
            .collect()
    });
    crate::world_map::validate_geography(
        &value["world"],
        SIDE,
        SIDE,
        &chars,
        heights.as_deref(),
        value["spawns"].as_array().unwrap().len(),
    )
}

fn rejects_route(value: &Value) {
    rejects(value, "world regions need connected ground routes");
}

fn cell(value: &mut Value, layer: &str, x: usize, y: usize, ch: u8) {
    let mut row = value[layer][y].as_str().unwrap().as_bytes().to_vec();
    row[x] = ch;
    value[layer][y] = json!(String::from_utf8(row).unwrap());
}

#[test]
fn map92_region_sizes_counts_and_spans_match_prototype_diagnostics() {
    for count in [64, 128] {
        assert_eq!(
            load(&authored(count))
                .unwrap()
                .geography()
                .unwrap()
                .regions
                .len(),
            count
        );
    }
    for (key, bad, field) in [
        ("size", json!("small"), "invalid world regions"),
        ("total", json!(128), "invalid world regions"),
        ("total", json!(0.5), "invalid world regions"),
        ("regionCells", json!(32), "invalid world regions"),
        ("regions", json!(null), "invalid world regions"),
        ("regions", json!([]), "invalid world regions"),
    ] {
        let mut value = authored(64);
        value["world"][key] = bad;
        rejects(&value, field);
    }
    let mut value = authored(64);
    value["world"]["regions"].as_array_mut().unwrap().pop();
    rejects(&value, "invalid world regions");
}

#[test]
fn map92_membership_is_optional_or_one_bounded_integer_per_cell() {
    for absent in [false, true] {
        let mut value = authored(64);
        if absent {
            value["world"].as_object_mut().unwrap().remove("regionMap");
        } else {
            value["world"]["regionMap"] = Value::Null;
        }
        assert!(
            load(&value)
                .unwrap()
                .geography()
                .unwrap()
                .region_map
                .is_none()
        );
    }
    for count in [64, 128] {
        let mut value = authored(count);
        value["world"]["regionMap"][0] = json!(0);
        value["world"]["regionMap"][1] = json!(count - 1);
        let map = load(&value).unwrap();
        let membership = map.geography().unwrap().region_map.as_ref().unwrap();
        assert_eq!(&membership[..3], &[0, (count - 1) as i16, -1]);
        for bad in [json!(-2), json!(count), json!(0.5), json!("0")] {
            value["world"]["regionMap"][0] = bad;
            rejects(&value, "invalid region membership");
        }
    }
    for bad in [json!([]), json!(vec![0; SIDE * SIDE - 1])] {
        let mut value = authored(64);
        value["world"]["regionMap"] = bad;
        rejects(&value, "invalid region membership");
    }
}

#[test]
fn map92_objectives_require_unique_integer_ids_open_sites_and_four_bounds() {
    let mut value = authored(64);
    value["world"]["regions"][1]["id"] = json!(0);
    rejects(&value, "invalid world region");
    for (key, bad, field) in [
        ("id", json!(0.5), "invalid world region"),
        ("x", json!(-1), "invalid world region"),
        ("x", json!(SIDE), "invalid world region"),
        ("x", json!(2.5), "invalid world region"),
        ("y", json!(-1), "invalid world region"),
        ("y", json!(SIDE), "invalid world region"),
        ("y", json!(2.5), "invalid world region"),
        ("bounds", json!(null), "invalid world region"),
        ("bounds", json!([0, 0, SIDE]), "invalid world region"),
        (
            "bounds",
            json!([0, 0, SIDE, SIDE, 0]),
            "invalid world region",
        ),
    ] {
        let mut value = authored(64);
        value["world"]["regions"][0][key] = bad;
        rejects(&value, field);
    }
    let mut value = authored(64);
    value["world"]["regions"][0] = Value::Null;
    rejects(&value, "invalid world region");

    // The objective is distinct from both spawns, so spawn validation cannot mask this guard.
    let mut value = authored(64);
    cell(&mut value, "rows", 2, 2, b'W');
    rejects(&value, "invalid world region");

    // IDs identify regions independently of their ordered membership indices.
    let mut value = authored(64);
    value["world"]["regions"][0]["id"] = json!(-7);
    assert_eq!(load(&value).unwrap().geography().unwrap().regions[0].id, -7);
}

#[test]
fn map92_each_spawn_has_exactly_one_valid_home() {
    for bad in [json!(-1), json!(2), json!(0.5), json!(null)] {
        let mut value = authored(64);
        value["world"]["regions"][0]["home"] = bad;
        rejects(&value, "invalid world home");
    }
    let mut value = authored(64);
    value["world"]["regions"][1]["home"] = json!(0);
    rejects(&value, "invalid world home");
    let mut value = authored(64);
    value["world"]["regions"][0]
        .as_object_mut()
        .unwrap()
        .remove("home");
    rejects(&value, "each player needs one home");
}

#[test]
fn map92_objective_routes_require_four_way_unblocked_one_level_steps() {
    // Move one objective east of a full separating column. All other sites remain west.
    let baseline = || {
        let mut value = authored(64);
        value["world"]["regions"][63]["x"] = json!(15);
        value
    };
    for barrier in *b"WR" {
        let mut value = baseline();
        for y in 0..SIDE {
            cell(&mut value, "rows", 11, y, barrier);
        }
        rejects_route(&value);
        // A single open crossing permits the four-way search to take a detour.
        cell(&mut value, "rows", 11, 18, b'.');
        assert!(load(&value).is_ok());
    }
    for level in *b"12" {
        let mut value = baseline();
        for y in 0..SIDE {
            for x in 11..SIDE {
                cell(&mut value, "heights", x, y, level);
            }
        }
        if level == b'1' {
            assert!(load(&value).is_ok());
        } else {
            rejects_route(&value);
        }
    }
    let mut value = baseline();
    value.as_object_mut().unwrap().remove("heights");
    assert!(load(&value).is_ok());

    // VBLOCK is allowed at an objective site. The initial site can be left, but another
    // VBLOCK objective cannot be entered, exactly as the Source walk seeds its queue.
    let mut value = authored(64);
    cell(&mut value, "rows", 2, 2, b'R');
    assert!(load(&value).is_ok());
    cell(&mut value, "rows", 3, 2, b'R');
    rejects_route(&value);

    // The isolated corner touches reachable ground diagonally, which is insufficient.
    let mut value = authored(64);
    value["world"]["regions"][63]["x"] = json!(19);
    value["world"]["regions"][63]["y"] = json!(19);
    cell(&mut value, "rows", 18, 19, b'W');
    cell(&mut value, "rows", 19, 18, b'W');
    rejects_route(&value);
    cell(&mut value, "rows", 18, 19, b'.');
    assert!(load(&value).is_ok());
}

#[test]
fn map92_raw_authoring_validation_preserves_prototype_acceptance_and_error_order() {
    // The pure query preserves the Source validator's permissive authoring checks.
    // Typed map construction separately refuses values its storage cannot represent.
    let mut value = authored(64);
    value["world"]["regions"][0]["bounds"] = json!(["0", -1, null, {}]);
    value["world"]["regions"][0]["id"] = json!(2147483648_u64);
    assert_eq!(validate(&value), Ok(()));
    assert!(load(&value).is_err());
    for membership in [Value::Null, json!(false), json!(0), json!("")] {
        value["world"]["regionMap"] = membership;
        assert_eq!(validate(&value), Ok(()));
    }
    let mut value = authored(64);
    value["world"]["regions"][0]["id"] = json!(-0.0);
    value["world"]["regions"][1]["id"] = json!(0.0);
    rejects(&value, "invalid world region");

    // Membership precedes region checks, and a region's home precedes later regions.
    let mut value = authored(64);
    value["world"]["regions"][0]["home"] = json!(2);
    value["world"]["regions"][1]["x"] = json!(-1);
    value["world"]["regionMap"] = json!([]);
    rejects(&value, "invalid region membership");
    value["world"]["total"] = json!(128);
    rejects(&value, "invalid world regions");
    value["world"]["total"] = json!(64);
    value["world"].as_object_mut().unwrap().remove("regionMap");
    rejects(&value, "invalid world home");
    value["world"]["regions"][0]["home"] = json!(0);
    rejects(&value, "invalid world region");
    value["world"]["regions"][1]["x"] = json!(3);
    assert_eq!(validate(&value), Ok(()));

    // Missing homes precede disconnected routes.
    value["world"]["regions"][63]["x"] = json!(15);
    value["world"]["regions"][0]
        .as_object_mut()
        .unwrap()
        .remove("home");
    for y in 0..SIDE {
        cell(&mut value, "rows", 11, y, b'W');
    }
    rejects(&value, "each player needs one home");
    value["world"]["regions"][0]["home"] = json!(0);
    rejects_route(&value);
}
