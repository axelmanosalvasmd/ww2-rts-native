use super::*;

fn map() -> Value {
    json!({"w":32,"h":32,"spawns":[{"x":2,"y":2},{"x":29,"y":29}],
        "points":[{"id":"crossroads","x":16,"y":16},{"x":20,"y":20}],
        "world":{"regions":[{"id":7}]},
        "scenario":{"version":1,
            "areas":[{"id":"gate","box":[8,8,10,10]}],
            "groups":[{"id":"squad","units":[{"id":"leader","type":"rifle","side":0,"x":9,"y":9}]}],
            "objectives":[{"id":"hold","text":{"en":"Hold the gate","es":"Defiende la entrada"},"recipients":"side","side":0}],
            "triggers":[trigger("intro",json!({"kind":"time","seconds":0}),vec![json!({"kind":"say","text":"Ready"})])]}})
}

fn trigger(id: &str, condition: Value, actions: Vec<Value>) -> Value {
    json!({"id":id,"scope":"match","condition":condition,"actions":actions})
}

fn check(map: &Value) -> Result<()> {
    validate_scenario(map, Catalog::compiled().unwrap())
}

fn rejects(map: &Value, expected: &str) {
    assert_eq!(
        check(map).unwrap_err().to_string(),
        format!("scenario: {expected}")
    );
}

fn reinforce() -> Value {
    json!({"kind":"reinforce","group":"squad","side":0,"at":[9,9],
        "expires":5,"roster":[{"type":"rifle","count":3}],"order":{"kind":"hold"}})
}

#[test]
fn mode80_migration_preserves_authored_definitions_and_legacy_actions() {
    let original = map();
    assert_eq!(
        migrate_scenario(&original),
        Some(original["scenario"].clone())
    );
    assert_eq!(check(&original), Ok(()));
    let mut map = json!({"w":32,"h":32,"spawns":[{},{}],"triggers":[
        {"at":3,"say":"Intro"},{"at":420,"say":"Bridge is down","blow":[2,3,4,5]}]});
    assert_eq!(
        migrate_scenario(&map),
        Some(
            json!({"version":1,"areas":[],"groups":[],"objectives":[],"triggers":[
                {"id":"legacy:0","scope":"match","recipients":"public","condition":{"kind":"time","seconds":3},"repeat":{"mode":"once"},"actions":[{"kind":"say","text":{"en":"Intro"},"recipients":"public"}]},
                {"id":"legacy:1","scope":"match","recipients":"public","condition":{"kind":"time","seconds":420},"repeat":{"mode":"once"},"actions":[{"kind":"say","text":{"en":"Bridge is down"},"recipients":"public"},{"kind":"destroy","box":[2,3,4,5]}]}
            ]})
        )
    );
    assert_eq!(check(&map), Ok(()));
    for malformed in [json!(1), json!(true)] {
        let mut invalid = map.clone();
        invalid["triggers"][0]["say"] = malformed.clone();
        assert_eq!(
            migrate_scenario(&invalid).unwrap()["triggers"][0]["actions"][0]["text"],
            json!({"en":malformed})
        );
        rejects(&invalid, "trigger legacy:0 has invalid announcement");
    }
    map["triggers"] = json!([]);
    assert_eq!(migrate_scenario(&map), None);
    assert_eq!(check(&map), Ok(()));
    map["scenario"] = json!({"version":2});
    rejects(&map, "version must be 1");
    map["scenario"] = json!({"version":1});
    assert_eq!(check(&map), Ok(()), "absent collections are empty");
}

#[test]
fn mode80_collection_limits_and_stable_ids_are_exact() {
    for key in ["triggers", "objectives", "groups", "areas"] {
        let mut map = map();
        let original = map["scenario"][key][0].clone();
        map["scenario"][key] = Value::Array(
            (0..32)
                .map(|i| {
                    let mut item = original.clone();
                    item["id"] = json!(format!("id:{i}"));
                    if key == "groups" {
                        item["units"] = json!([]);
                    }
                    item
                })
                .collect(),
        );
        assert_eq!(check(&map), Ok(()), "32 {key}");
        let mut too_many = map.clone();
        too_many["scenario"][key]
            .as_array_mut()
            .unwrap()
            .push(original.clone());
        rejects(&too_many, &format!("{key} exceeds 32"));
        let mut duplicate = map.clone();
        duplicate["scenario"][key][1]["id"] = duplicate["scenario"][key][0]["id"].clone();
        rejects(&duplicate, &format!("{key} needs unique stable IDs"));
        for bad in [
            json!(""),
            json!("-bad"),
            json!("has space"),
            json!("é"),
            json!("a".repeat(65)),
            json!(null),
        ] {
            let mut invalid = map.clone();
            invalid["scenario"][key][0]["id"] = bad;
            rejects(&invalid, &format!("{key} needs unique stable IDs"));
        }
        map["scenario"][key][0]["id"] = json!("a".repeat(64));
        assert_eq!(check(&map), Ok(()));
        map["scenario"][key] = json!({});
        rejects(&map, &format!("{key} exceeds 32"));
    }
}

#[test]
fn mode80_localized_text_and_recipient_side_rules_are_bounded() {
    let mut map = map();
    for text in [
        json!("x".repeat(280)),
        json!({"en":"😀".repeat(140)}),
        Value::Object(
            (0..8)
                .map(|i| (format!("lang{i}"), json!("Valid")))
                .collect(),
        ),
    ] {
        map["scenario"]["objectives"][0]["text"] = text.clone();
        map["scenario"]["triggers"][0]["actions"][0]["text"] = text;
        assert_eq!(check(&map), Ok(()));
    }
    for text in [
        json!(""),
        json!("x".repeat(281)),
        json!({"en":"😀".repeat(141)}),
        json!({}),
        json!({"en":false}),
        Value::Object((0..9).map(|i| (i.to_string(), json!("Valid"))).collect()),
    ] {
        let mut invalid = map.clone();
        invalid["scenario"]["objectives"][0]["text"] = text.clone();
        rejects(&invalid, "objective hold needs text and valid recipients");
        invalid = map.clone();
        invalid["scenario"]["triggers"][0]["actions"][0]["text"] = text;
        rejects(&invalid, "trigger intro has invalid announcement");
    }
    for policy in ["public", "side", "team"] {
        let mut valid = map.clone();
        valid["scenario"]["triggers"][0]["recipients"] = json!(policy);
        if policy != "public" {
            rejects(&valid, "trigger intro has invalid scope or recipients");
        }
        valid["scenario"]["triggers"][0]["side"] = json!(0);
        assert_eq!(check(&valid), Ok(()));
        valid["scenario"]["triggers"][0]["side"] = json!(2);
        rejects(&valid, "trigger intro has invalid scope or recipients");
    }
    map["scenario"]["triggers"][0]["recipients"] = json!("private");
    rejects(&map, "trigger intro has invalid scope or recipients");
    map["scenario"]["triggers"][0]["recipients"] = json!("public");
    map["scenario"]["triggers"][0]["actions"][0]["recipients"] = json!("side");
    rejects(&map, "trigger intro has invalid announcement");
    map["scenario"]["triggers"][0]["actions"][0]["side"] = json!(1);
    assert_eq!(check(&map), Ok(()));
    map["scenario"]["objectives"][0]
        .as_object_mut()
        .unwrap()
        .remove("side");
    rejects(&map, "objective hold needs text and valid recipients");
}

#[test]
fn mode80_authored_units_share_global_ids_and_128_limit_and_exclude_non_ground_types() {
    let mut map = map();
    let unit = map["scenario"]["groups"][0]["units"][0].clone();
    map["scenario"]["groups"][0]["units"] = Value::Array(
        (0..128)
            .map(|i| {
                let mut unit = unit.clone();
                unit["id"] = json!(format!("unit:{i}"));
                unit
            })
            .collect(),
    );
    assert_eq!(check(&map), Ok(()));
    map["scenario"]["groups"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"other","units":[unit.clone()]}));
    rejects(&map, "too many authored units");
    map["scenario"]["groups"][0]["units"] = json!([unit]);
    rejects(&map, "group other has an invalid unit");
    map["scenario"]["groups"].as_array_mut().unwrap().pop();
    let catalog = Catalog::compiled().unwrap();
    let units = catalog.table("UNITS").unwrap().object().unwrap();
    let mut exclusions = BTreeSet::new();
    for (name, kind) in units {
        map["scenario"]["groups"][0]["units"][0]["type"] = json!(name);
        let forbidden: Vec<_> = ["structure", "building", "air"]
            .into_iter()
            .filter(|key| kind.get(key).and_then(crate::catalog::Value::boolean) == Some(true))
            .collect();
        if forbidden.is_empty() {
            assert_eq!(check(&map), Ok(()), "{name}");
        } else {
            rejects(&map, "group squad has an invalid unit");
            exclusions.extend(forbidden);
        }
    }
    assert_eq!(exclusions, BTreeSet::from(["structure", "building", "air"]));
    for (key, value) in [
        ("type", json!("missing")),
        ("side", json!(2)),
        ("x", json!(32)),
        ("y", json!(-1)),
        ("id", json!("bad id")),
    ] {
        let mut invalid = map.clone();
        invalid["scenario"]["groups"][0]["units"][0] = unit.clone();
        invalid["scenario"]["groups"][0]["units"][0][key] = value;
        rejects(&invalid, "group squad has an invalid unit");
    }
}

#[test]
fn mode80_capture_ids_default_to_points_and_reject_collisions_with_regions() {
    let mut map = map();
    map["world"]["regions"][0]["id"] = json!(7.0);
    for target in ["crossroads", "point:1", "region:7"] {
        map["scenario"]["triggers"][0]["condition"] =
            json!({"kind":"capture","target":target,"side":0});
        assert_eq!(check(&map), Ok(()));
    }
    map["scenario"]["triggers"][0]["condition"]["target"] = json!("point:0");
    rejects(&map, "trigger intro has an invalid condition or reference");
    map["scenario"]["triggers"][0]["condition"]["target"] = json!("region:7");
    map["points"][0]["id"] = json!("region:7");
    rejects(&map, "capture targets need unique IDs");
    map["points"][0]["id"] = json!("point:1");
    rejects(&map, "capture targets need unique IDs");
    map["points"][0]["id"] = json!("bad id");
    rejects(&map, "capture targets need stable IDs");
}

#[test]
fn mode80_all_condition_references_counts_depth_and_repeat_limits_are_checked() {
    let base = map();
    let valid = [
        json!({"kind":"time","seconds":7200}),
        json!({"kind":"ownership","target":"crossroads","side":0}),
        json!({"kind":"capture","target":"point:1","side":1}),
        json!({"kind":"unitLost","unit":"leader"}),
        json!({"kind":"groupLost","group":"squad"}),
        json!({"kind":"groupCount","group":"squad","op":"atMost","count":128}),
        json!({"kind":"groupCount","group":"squad","op":"atLeast","count":0}),
        json!({"kind":"groupCount","group":"squad","op":"equal","count":1}),
        json!({"kind":"areaEntry","area":"gate","group":"squad"}),
        json!({"kind":"areaEntry","area":"gate","side":0}),
        json!({"kind":"objectiveComplete","objective":"hold"}),
    ];
    for condition in &valid {
        let mut map = base.clone();
        map["scenario"]["triggers"][0]["condition"] = condition.clone();
        assert_eq!(check(&map), Ok(()), "{condition}");
        let key = match condition["kind"].as_str().unwrap() {
            "ownership" | "capture" => "target",
            "unitLost" => "unit",
            "groupLost" | "groupCount" => "group",
            "areaEntry" => "area",
            "objectiveComplete" => "objective",
            _ => "seconds",
        };
        map["scenario"]["triggers"][0]["condition"][key] = json!("missing");
        rejects(&map, "trigger intro has an invalid condition or reference");
    }
    for operator in ["all", "any"] {
        let mut map = base.clone();
        map["scenario"]["triggers"][0]["condition"] = json!({"kind":operator,"conditions":valid});
        assert_eq!(check(&map), Ok(()));
        map["scenario"]["triggers"][0]["condition"]["conditions"] = json!([]);
        rejects(&map, "trigger intro has an invalid condition or reference");
        map["scenario"]["triggers"][0]["condition"]["conditions"] =
            json!(vec![json!({"kind":"time","seconds":0}); 31]);
        assert_eq!(check(&map), Ok(()), "root plus 31 leaves is 32 conditions");
        map["scenario"]["triggers"][0]["condition"]["conditions"]
            .as_array_mut()
            .unwrap()
            .push(json!({"kind":"time","seconds":0}));
        rejects(&map, "trigger intro has an invalid condition or reference");
    }
    let mut map = base.clone();
    let mut nested = json!({"kind":"time","seconds":0});
    for _ in 0..6 {
        nested = json!({"kind":"all","conditions":[nested]});
    }
    map["scenario"]["triggers"][0]["condition"] = nested.clone();
    assert_eq!(check(&map), Ok(()));
    map["scenario"]["triggers"][0]["condition"] = json!({"kind":"any","conditions":[nested]});
    rejects(&map, "trigger intro has an invalid condition or reference");
    for mode in ["risingEdge", "whileTrue"] {
        let mut map = base.clone();
        for (cooldown, max) in [(0.05, 1), (7200.0, 32)] {
            map["scenario"]["triggers"][0]["repeat"] =
                json!({"mode":mode,"cooldown":cooldown,"max":max});
            assert_eq!(check(&map), Ok(()));
        }
        for repeat in [
            json!({"mode":mode,"cooldown":0,"max":2}),
            json!({"mode":mode,"cooldown":7201,"max":2}),
            json!({"mode":mode,"cooldown":1,"max":33}),
            json!({"mode":mode,"cooldown":1,"max":1.5}),
            json!({"mode":mode,"max":1}),
        ] {
            map["scenario"]["triggers"][0]["repeat"] = repeat;
            rejects(
                &map,
                "trigger intro needs a positive cooldown and finite execution limit",
            );
        }
    }
    for scope in ["player", "team"] {
        let mut map = base.clone();
        map["scenario"]["triggers"][0]["scope"] = json!(scope);
        map["scenario"]["triggers"][0]["condition"] =
            json!({"kind":"ownership","target":"crossroads"});
        map["scenario"]["triggers"][0]["recipients"] = json!("team");
        assert_eq!(check(&map), Ok(()));
    }
}

#[test]
fn mode80_actions_rosters_boxes_and_total_tick_work_use_exact_limits() {
    let base = map();
    let valid = [
        reinforce(),
        json!({"kind":"objectiveActivate","objective":"hold"}),
        json!({"kind":"objectiveComplete","objective":"hold"}),
        json!({"kind":"damage","box":[0,0,15,15],"damage":0.01}),
        json!({"kind":"damage","box":[0,0,15,15],"damage":100000}),
        json!({"kind":"destroy","box":[0,0,15,15]}),
    ];
    for action in &valid {
        let mut map = base.clone();
        map["scenario"]["triggers"][0]["actions"] = json!([action]);
        assert_eq!(check(&map), Ok(()));
    }
    for (key, value) in [
        ("group", json!("missing")),
        ("side", json!(2)),
        ("at", json!([32, 0])),
        ("expires", json!(0)),
        ("expires", json!(7201)),
        ("roster", json!([])),
        ("roster", json!([{"type":"missing","count":1}])),
        ("roster", json!([{"type":"rifle","count":33}])),
        (
            "roster",
            json!([{"type":"rifle","count":16},{"type":"rifle","count":17}]),
        ),
        ("order", json!({"kind":"stop"})),
        ("order", json!({"kind":"move","at":[32,0]})),
    ] {
        let mut action = reinforce();
        action[key] = value;
        let mut map = base.clone();
        map["scenario"]["triggers"][0]["actions"] = json!([action]);
        rejects(
            &map,
            if key == "side" {
                "trigger intro has invalid action recipients"
            } else {
                "trigger intro has invalid reinforcements"
            },
        );
    }
    for order in [
        json!({"kind":"move","at":[31,0]}),
        json!({"kind":"attackMove","at":[0,31]}),
        json!({"kind":"hold"}),
    ] {
        let mut action = reinforce();
        action["order"] = order;
        action["roster"] = json!([{"type":"rifle","count":32.0}]);
        let mut map = base.clone();
        map["scenario"]["triggers"][0]["actions"] = json!([action]);
        assert_eq!(check(&map), Ok(()));
    }
    for bounds in [
        json!([0, 0, 16, 15]),
        json!([1, 0, 0, 0]),
        json!([0, 0, 32, 0]),
        json!([0, 0, 0]),
        json!([0.5, 0, 1, 1]),
    ] {
        let mut map = base.clone();
        map["scenario"]["triggers"][0]["actions"] = json!([{"kind":"destroy","box":bounds}]);
        rejects(&map, "trigger intro has an invalid damage area");
        map["scenario"]["triggers"][0]["actions"] = json!([{"kind":"say","text":"Ready"}]);
        map["scenario"]["areas"][0]["box"] = bounds;
        rejects(&map, "area gate needs a valid box of up to 256 cells");
    }
    for damage in [0.0, 0.009, 100001.0] {
        let mut map = base.clone();
        map["scenario"]["triggers"][0]["actions"] =
            json!([{"kind":"damage","box":[0,0,0,0],"damage":damage}]);
        rejects(&map, "trigger intro has an invalid damage area");
    }
    let mut map = base.clone();
    map["scenario"]["triggers"][0]["actions"] =
        json!(vec![json!({"kind":"say","text":"Ready"}); 8]);
    assert_eq!(check(&map), Ok(()));
    map["scenario"]["triggers"][0]["actions"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"say","text":"Ready"}));
    rejects(&map, "trigger intro needs 1-8 actions");
    map["scenario"]["triggers"][0]["actions"] = json!([]);
    rejects(&map, "trigger intro needs 1-8 actions");
    // 16 * (one condition + one action + 254 cells) = exactly 4096.
    let actions = vec![json!({"kind":"destroy","box":[0,0,0,253]})];
    map["h"] = json!(256);
    map["scenario"]["triggers"] = json!(
        (0..16)
            .map(|i| trigger(
                &format!("work{i}"),
                json!({"kind":"time","seconds":0}),
                actions.clone()
            ))
            .collect::<Vec<_>>()
    );
    assert_eq!(check(&map), Ok(()));
    map["scenario"]["triggers"][0]["actions"][0]["box"] = json!([0, 0, 0, 254]);
    rejects(&map, "total trigger work exceeds 4096");
    map["scenario"]["triggers"]
        .as_array_mut()
        .unwrap()
        .truncate(8);
    map["scenario"]["triggers"][0]["actions"][0]["box"] = json!([0, 0, 0, 253]);
    for trigger in map["scenario"]["triggers"].as_array_mut().unwrap() {
        trigger["scope"] = json!("team");
    }
    assert_eq!(check(&map), Ok(()), "team/player work uses spawn count");
    map["scenario"]["triggers"][0]["scope"] = json!("player");
    map["scenario"]["triggers"][0]["actions"][0]["box"] = json!([0, 0, 0, 254]);
    rejects(&map, "total trigger work exceeds 4096");
}

#[test]
fn mode80_dependency_graph_rejects_direct_indirect_and_objective_cycles() {
    let mut map = map();
    map["scenario"]["triggers"] = json!([
        trigger(
            "first",
            json!({"kind":"time","seconds":0}),
            vec![json!({"kind":"objectiveComplete","objective":"hold"})]
        ),
        trigger(
            "second",
            json!({"kind":"objectiveComplete","objective":"hold"}),
            vec![json!({"kind":"say","text":"Done"})]
        ),
        trigger(
            "third",
            json!({"kind":"triggerComplete","trigger":"second"}),
            vec![json!({"kind":"say","text":"Continue"})]
        )
    ]);
    assert_eq!(
        check(&map),
        Ok(()),
        "shared acyclic dependencies are allowed"
    );
    let base = map.clone();
    map["scenario"]["triggers"][0]["condition"] =
        json!({"kind":"triggerComplete","trigger":"first"});
    rejects(&map, "objective or trigger dependencies form a cycle");
    map = base.clone();
    map["scenario"]["triggers"][0]["condition"] =
        json!({"kind":"triggerComplete","trigger":"third"});
    rejects(&map, "objective or trigger dependencies form a cycle");
    map = base.clone();
    map["scenario"]["triggers"][0]["condition"] =
        json!({"kind":"objectiveComplete","objective":"hold"});
    rejects(&map, "objective or trigger dependencies form a cycle");
    map = base;
    map["scenario"]["triggers"][2]["condition"]["trigger"] = json!("missing");
    rejects(&map, "trigger third has an invalid condition or reference");
}

#[test]
fn mode80_invalid_condition_and_action_fields_fail_without_executing() {
    let base = map();
    for condition in [
        json!({"kind":"time","seconds":-0.01}),
        json!({"kind":"time","seconds":7200.01}),
        json!({"kind":"time","seconds":0,"side":2}),
        json!({"kind":"time","seconds":null}),
        json!({"kind":"ownership","target":"crossroads"}),
        json!({"kind":"capture","target":"crossroads"}),
        json!({"kind":"areaEntry","area":"gate"}),
        json!({"kind":"groupCount","group":"squad","op":"over","count":1}),
        json!({"kind":"groupCount","group":"squad","op":"equal","count":129}),
        json!({"kind":"groupCount","group":"squad","op":"equal","count":1.5}),
        json!({"kind":"unknown"}),
        json!(null),
    ] {
        let mut map = base.clone();
        map["scenario"]["triggers"][0]["condition"] = condition;
        rejects(&map, "trigger intro has an invalid condition or reference");
    }
    for kind in ["ownership", "capture", "areaEntry"] {
        let mut map = base.clone();
        map["scenario"]["triggers"][0]["side"] = json!(0);
        map["scenario"]["triggers"][0]["condition"] =
            json!({"kind":kind,"target":"crossroads","area":"gate"});
        assert_eq!(check(&map), Ok(()), "trigger side supplies condition side");
    }
    for action in [
        json!({"kind":"say","text":"Ready","recipients":"private"}),
        json!({"kind":"say","text":"Ready","side":2}),
        json!(null),
        json!(false),
        json!(0),
        json!(""),
    ] {
        let mut map = base.clone();
        map["scenario"]["triggers"][0]["actions"] = json!([action]);
        rejects(&map, "trigger intro has invalid action recipients");
    }
    for kind in ["objectiveActivate", "objectiveComplete"] {
        let mut map = base.clone();
        map["scenario"]["triggers"][0]["actions"] = json!([{"kind":kind,"objective":"missing"}]);
        rejects(&map, "trigger intro references an unknown objective");
    }
    let mut map = base.clone();
    map["scenario"]["triggers"][0]["actions"] = json!([{"kind":"unknown"}]);
    rejects(&map, "trigger intro has an unknown action");
    for scope in [json!("unknown"), json!(null)] {
        let mut map = base.clone();
        map["scenario"]["triggers"][0]["scope"] = scope;
        rejects(&map, "trigger intro has invalid scope or recipients");
    }
    let mut map = base;
    map["scenario"]["triggers"][0]["repeat"] = json!({"mode":"unknown"});
    rejects(
        &map,
        "trigger intro needs a positive cooldown and finite execution limit",
    );
}
