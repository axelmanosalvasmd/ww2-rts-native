//! The complete original 60-unit benchmark, at the public routing-component boundary.
//! No native actor table, match state or RNG is initialized from Source after-images.
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::{BufWriter, Write}, path::PathBuf};
use ww2_core::{
    game::Point,
    navigation::{TerrainChange, TerrainView, Versions},
    pathfinding::{PathRules, PathStatus, PathTerrain, Pathfinding},
    rules::Rules,
};

fn integer(v: &Value) -> usize {
    usize::try_from(v.as_u64().expect("nonnegative integer input")).unwrap()
}
fn number(v: &Value) -> f64 {
    let text = v.as_str().expect("recorded f64 bits");
    assert_eq!(text.len(), 16);
    let n = f64::from_bits(u64::from_str_radix(text, 16).unwrap());
    assert!(n.is_finite(), "finite routing input");
    n
}
fn point(v: &Value) -> Point {
    Point { x: number(&v[0]), z: number(&v[1]) }
}
fn bits(p: Point) -> Value {
    json!([format!("{:016x}", p.x.to_bits()), format!("{:016x}", p.z.to_bits())])
}
fn runs<T: Copy>(v: &Value, convert: impl Fn(&Value) -> T) -> Vec<T> {
    let mut out = Vec::new();
    for row in v.as_array().unwrap() {
        assert_eq!(row.as_array().unwrap().len(), 2);
        out.extend(std::iter::repeat_n(convert(&row[0]), integer(&row[1])));
    }
    out
}
fn preserve_storage<T: Copy>(buffers: &mut BTreeMap<u64, Vec<T>>, key: u64, values: Vec<T>) {
    assert!(!values.is_empty());
    let buffer = buffers.entry(key).or_insert_with(|| vec![values[0]; values.len()]);
    assert_eq!(buffer.len(), values.len(), "recorded array identity changed shape");
    buffer.copy_from_slice(&values);
}
fn change(v: &Value) -> (u64, u64, Vec<usize>) {
    (v["from"].as_u64().unwrap(), v["to"].as_u64().unwrap(),
        v["cells"].as_array().unwrap().iter().map(integer).collect())
}
fn exact_integer(n: f64) -> String {
    assert!(n.is_finite() && n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0,
        "the fixed benchmark needs exact integral JSON coordinates, no rounding");
    format!("{}", n as i64)
}
fn route_json(order: &[u64], paths: &BTreeMap<u64, Vec<Point>>) -> String {
    format!("[{}]", order.iter().map(|id| {
        let points = paths[id].iter().map(|p| format!("{{\"x\":{},\"z\":{}}}",
            exact_integer(p.x), exact_integer(p.z))).collect::<Vec<_>>().join(",");
        format!("[{id},[{points}]]")
    }).collect::<Vec<_>>().join(","))
}
fn digest_shape(v: &Value) {
    let text = v.as_str().unwrap();
    assert_eq!(text.len(), 64);
    assert!(text.bytes().all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)));
}
fn replay_case(case: &Value, name: &str, writer: &mut impl Write) -> usize {
    assert_eq!(case["format"], "mov33-original-benchmark-case-v1");
    assert_eq!(case["failure"], Value::Null);
    assert_eq!(case["exit"], 0);
    assert_eq!(case["instrumentation"]["erasesExactly"], true);
    assert_eq!(case["constructors"].as_array().unwrap().len(), 1);
    let commands = case["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 32, "all trials, including four warmups");
    assert_eq!(case["result"]["trials"], 32);
    assert_eq!(case["result"]["warmupExcluded"], 4);
    assert_eq!(case["result"]["packets"].as_array().unwrap().len(), 32);
    assert_eq!(case["result"]["seed"], 0x3c1055_u64);
    assert_eq!(case["result"]["rifles"], 30);
    assert_eq!(case["result"]["tanks"], 30);
    let bunkers = if name == "six-bunkers" { 6 } else { 0 };
    assert_eq!(case["result"]["bunkers"], bunkers);
    let initial = commands[0]["beforeRoster"].as_array().unwrap();
    assert_eq!(initial.len(), 60 + bunkers as usize);
    assert_eq!(case["result"]["units"], initial.len());
    let actor_order = initial.iter().map(|v| v["id"].as_u64().unwrap()).collect::<Vec<_>>();
    assert_eq!(actor_order, (1..=initial.len() as u64).collect::<Vec<_>>());
    let mut paths = BTreeMap::new();
    for (i, actor) in initial.iter().enumerate() {
        assert_eq!(actor["path"], json!([]), "actual original initial routes must be empty");
        assert_eq!(actor["type"], if i >= 60 { "bunker" } else if i % 2 == 0 { "rifle" } else { "tank" });
        assert_eq!(actor["owner"], if i >= 60 { 1 } else { 0 });
        paths.insert(actor_order[i], Vec::new());
    }
    let input: Value = serde_json::from_str(case["inputJson"].as_str().unwrap()).unwrap();
    assert_eq!(input["map"], case["constructors"][0]["map"]);
    let input_units = initial.iter().map(|v| json!({"id":v["id"],"type":v["type"],"owner":v["owner"],
        "x":v["x"],"z":v["z"],"hp":v["hp"]})).collect::<Vec<_>>();
    assert_eq!(input["units"], json!(input_units));
    assert_eq!(input["commands"], json!([commands[0]["input"],commands[1]["input"]]));
    digest_shape(&case["result"]["inputHash"]);
    let rules = Rules::compiled().unwrap();
    let path_rules = PathRules::compiled().unwrap();
    // One actual Pathfinder across all 1,920 queries, no per-query reset.
    let mut finder = Pathfinding::new();
    let mut flags = BTreeMap::<u64, Vec<u16>>::new();
    let mut heights = BTreeMap::<u64, Vec<i8>>::new();
    let mut knowledge = BTreeMap::new();
    let terrains = case["terrains"].as_array().unwrap();
    let mut count = 0;
    for (trial, command) in commands.iter().enumerate() {
        assert_eq!(command["trial"], trial);
        assert_eq!(command["owner"], 0);
        assert_eq!(command["accepted"], true);
        assert_eq!(command["input"], commands[trial % 2]["input"]);
        assert_eq!(command["input"]["t"], "move");
        assert_eq!(command["input"]["face"], 0);
        let expected_orders = (0..60).map(|i| {
            let (x,z) = if trial % 2 == 0 {
                (235 + i % 10 * 3, 215 + i / 10 * 3)
            } else { (210 + i % 10 * 3, 240 + i / 10 * 3) };
            json!([i+1,x,z])
        }).collect::<Vec<_>>();
        assert_eq!(command["input"], json!({"t":"move","face":0,"orders":expected_orders}),
            "both complete original command descriptors, independently reconstructed");
        let orders = command["input"]["orders"].as_array().unwrap();
        let roots = command["roots"].as_array().unwrap();
        let queries = command["queries"].as_array().unwrap();
        assert_eq!(orders.len(), 60);
        assert_eq!(roots.len(), 60);
        assert_eq!(queries.len(), 60);
        let before = command["beforeRoster"].as_array().unwrap();
        assert_eq!(before.len(), initial.len());
        for (a, b) in before.iter().zip(initial) {
            assert_eq!(json!(paths[&a["id"].as_u64().unwrap()].iter().copied().map(bits).collect::<Vec<_>>()), a["path"],
                "all prior native paths persist into the next original command");
            for key in ["id", "type", "owner", "x", "z", "hp"] {
                assert_eq!(a[key], b[key], "benchmark has no ticks or actor movement");
            }
        }
        let mut expansions = 0usize;
        for (at, (root, query)) in roots.iter().zip(queries).enumerate() {
            let actor = at as u64 + 1;
            assert_eq!(orders[at][0], actor);
            assert_eq!(root["actor"], actor);
            assert_eq!(root["query"], at);
            assert_eq!(query["index"], at);
            assert_eq!(query["actor"], actor);
            assert_eq!(query["kind"], initial[at]["type"]);
            let from = point(&query["from"]);
            assert_eq!(from.x.to_bits(), initial[at]["x"].as_f64().unwrap().to_bits());
            assert_eq!(from.z.to_bits(), initial[at]["z"].as_f64().unwrap().to_bits());
            let terrain = &terrains[integer(&query["terrain"])];
            let view_id = terrain["viewIdentity"].as_u64().unwrap();
            let prior = knowledge.entry(view_id).or_insert_with(|| terrain["rawMetadata"]["navigationKnowledge"].clone());
            assert_eq!(*prior, terrain["rawMetadata"]["navigationKnowledge"],
                "the fixed benchmark must retain its actual recipient knowledge identity");
            let width = integer(&terrain["width"]);
            let height = integer(&terrain["height"]);
            let cells = width.checked_mul(height).unwrap();
            let flag_id = terrain["flagsIdentity"].as_u64().unwrap();
            preserve_storage(&mut flags, flag_id, runs(&terrain["flags"], |v| u16::try_from(v.as_u64().unwrap()).unwrap()));
            assert_eq!(flags[&flag_id].len(), cells);
            let height_id = terrain["heightIdentity"].as_u64();
            if let Some(id) = height_id {
                preserve_storage(&mut heights, id, runs(&terrain["heights"], |v| i8::try_from(v.as_i64().unwrap()).unwrap()));
                assert_eq!(heights[&id].len(), cells);
            } else {
                assert_eq!(terrain["heights"], Value::Null);
            }
            let near = runs(&terrain["near"], |v| u8::try_from(v.as_u64().unwrap()).unwrap());
            assert_eq!(near.len(), cells);
            let wear = (!terrain["wear"].is_null()).then(|| runs(&terrain["wear"], |v| {
                let original = number(v);
                let value = original as f32;
                assert_eq!(f64::from(value).to_bits(), original.to_bits());
                value
            }));
            let mut mines = vec![false; cells];
            let mut fires = vec![false; cells];
            for v in terrain["mines"].as_array().unwrap() { mines[integer(v)] = true; }
            for v in terrain["fires"].as_array().unwrap() { fires[integer(v)] = true; }
            let bunkers = terrain["bunkers"].as_array().unwrap().iter().map(point).collect::<Vec<_>>();
            let direct = (!terrain["changed"].is_null()).then(|| change(&terrain["changed"]));
            let history = terrain["changes"].as_array().unwrap().iter().map(change).collect::<Vec<_>>();
            let history_views = history.iter().map(|(from,to,cells)| TerrainChange { from:*from,to:*to,cells }).collect::<Vec<_>>();
            let versions = &terrain["versions"];
            let navigation = TerrainView {
                width, height, flags: &flags[&flag_id], heights: height_id.map(|id| heights[&id].as_slice()),
                world_known: terrain["worldKnown"].as_bool().unwrap(),
                knowledge_id: terrain["viewIdentity"].as_u64().unwrap(),
                versions: Versions { infantry: versions["infantry"].as_u64().unwrap(), vehicle: versions["vehicle"].as_u64().unwrap(),
                    naval: versions["naval"].as_u64().unwrap(), obstruction: versions["obstruction"].as_u64().unwrap(), knowledge: versions["knowledge"].as_u64().unwrap() },
                changed: direct.as_ref().map(|(from,to,cells)| TerrainChange { from:*from,to:*to,cells }), changes: &history_views,
            };
            let result = finder.find_path(PathTerrain {
                navigation, near_walls: &near, known_mines: &mines, fires: &fires, bunkers: &bunkers,
                wear: wear.as_deref(), wetness: number(&terrain["wetness"]), roads: terrain["roads"].as_bool().unwrap(),
            }, path_rules, &rules.kinds[rules.kind(query["kind"].as_str().unwrap()).unwrap()],
                point(&query["from"]), point(&query["to"]),
                if query["limit"].is_null() { None } else { Some(integer(&query["limit"])) }).unwrap();
            let status = match result.status { PathStatus::Found => "Found", PathStatus::Unreachable => "Unreachable", PathStatus::ExpansionLimit => "ExpansionLimit" };
            let actual = json!({"points":result.waypoints.iter().copied().map(bits).collect::<Vec<_>>(),
                "routeEnd":bits(result.route_end),"expansions":result.expansions,"status":status});
            serde_json::to_writer(&mut *writer, &json!({"case":name,"trial":trial,"actor":actor,"query":at,"actual":actual})).unwrap();
            writer.write_all(b"\n").unwrap();
            writer.flush().unwrap();
            assert_eq!(actual, query["expected"], "{name} trial {trial} actor {actor}: first actual public-Pathfinder difference");
            expansions += result.expansions;
            paths.insert(actor, result.waypoints);
            count += 1;
        }
        let raw_json = route_json(&actor_order, &paths);
        assert_eq!(raw_json, command["routeJson"].as_str().unwrap(), "{name} trial {trial}: all-actor original RouteHash input bytes");
        assert_eq!(expansions, integer(&case["result"]["packets"][trial]["expansions"]));
        assert_eq!(case["result"]["packets"][trial]["calls"], 60);
        assert_eq!(case["result"]["packets"][trial]["trial"], trial);
        assert_eq!(paths.values().map(Vec::len).sum::<usize>(), integer(&case["result"]["packets"][trial]["waypoints"]));
        assert_eq!(command["routeHash"], case["result"]["packets"][trial]["routeHash"]);
        digest_shape(&command["routeHash"]);
        serde_json::to_writer(&mut *writer, &json!({"case":name,"trial":trial,"routeJson":raw_json,
            "sourceRouteHash":command["routeHash"],"actualCallbacks":queries.len(),"actualExpansions":expansions})).unwrap();
        writer.write_all(b"\n").unwrap();
        writer.flush().unwrap();
    }
    count
}

#[test]
#[ignore = "requires the complete private original MOV33 capture and Root execution grant"]
fn original_two_32_trial_route_hash_inputs_match_public_pathfinder() {
    let manifest = PathBuf::from(std::env::var_os("WW2_MOV33_TRACE").expect("private manifest required"));
    let trace: Value = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    assert_eq!(trace["format"], "mov33-original-benchmark-v1");
    assert_eq!(trace["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(trace["node"], "v24.21.0");
    assert_eq!(trace["failure"], Value::Null, "complete authentic Source capture first");
    let cases = trace["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    let output = std::fs::File::create(std::env::var_os("WW2_MOV33_REPORT").expect("private native report required")).unwrap();
    let mut writer = BufWriter::new(output);
    serde_json::to_writer(&mut writer, &json!({"format":"mov33-native-public-pathfinder-v1",
        "nativeHead":std::env::var("WW2_MOV33_NATIVE_HEAD").expect("frozen native head required"),"sourceHead":trace["sourceHead"],
        "scope":"routing component; no native actor table, Match command composition or timing claim"})).unwrap();
    writer.write_all(b"\n").unwrap();
    let mut queries = 0;
    for (index, name) in ["no-bunkers", "six-bunkers"].into_iter().enumerate() {
        let item = &cases[index];
        assert_eq!(item["name"], name);
        assert_eq!(item["commands"], 32);
        assert_eq!(item["queries"], 1920);
        assert_eq!(item["failure"], Value::Null);
        let file = format!("{name}.json");
        assert_eq!(item["file"], file);
        let case: Value = serde_json::from_slice(&std::fs::read(manifest.parent().unwrap().join(file)).unwrap()).unwrap();
        queries += replay_case(&case, name, &mut writer);
    }
    assert_eq!(queries, 3840);
    println!("MOV33 complete original component: 2 cases, 64 trials including 8 warmups, {queries} actual public-Pathfinder queries; exact RouteHash input bytes");
}
