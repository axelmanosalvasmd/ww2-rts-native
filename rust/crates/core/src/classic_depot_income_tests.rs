//! Live node income and missing-ID refusals, using independent canonical constructors.
use super::*;

fn value_bits(v: &Json) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn fixture() -> Json {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/classic-depot-income.json");
    let f: Json = serde_json::from_str(&std::fs::read_to_string(path)
        .expect("the separate income Source recipe needs its own exact-head grant")).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(f["cases"].as_array().unwrap().len(), 2);
    f
}
fn make_ordinary() -> Match {
    let input = input(false);
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&input["map"]).unwrap()).unwrap();
    Match::new(&map, MatchOptions {
        seed: 701, mode: Mode::Conquest,
        players: vec![PlayerConfig { name: "A".into(), team: 0, faction: Faction::Usa },
            PlayerConfig { name: "B".into(), team: 1, faction: Faction::Germany }],
        shuffle: false, army: ArmySize::Standard, weather: WeatherChoice::Map,
        weather_seed: Some(217), living_weather: false, map_key: None,
        tech: true, supply: true, logistics: false, sandbox: false,
    }).unwrap()
}
fn compare_live_nodes(m: &Match, source: &Json) {
    let nodes = &m.classic_runtime.as_ref().unwrap().nodes;
    assert_eq!(nodes.len(), source["nodes"].as_array().unwrap().len());
    for (n, row) in nodes.iter().zip(source["nodes"].as_array().unwrap()) {
        assert_eq!(json!(n.cell), row["c"]);
        assert_eq!(json!(n.depot.unwrap_or(0)), row["depot"]);
        assert_eq!(bits(n.rate), row["rate"].as_str().unwrap());
        assert_eq!(json!(n.fuel), row["fuel"]);
    }
}

#[test]
fn base26_classic_actual_live_node_income_stays_off_until_complete_and_after_cancel() {
    let fixture = fixture();
    let case = fixture["cases"].as_array().unwrap().iter().find(|c| c["mode"] == "classic").unwrap();
    assert_eq!(case["input"], input(false));
    let mut m = make(&input(false));
    let mut commands = 0;
    let mut ticks = 0;
    let mut baseline = None;
    for row in case["records"].as_array().unwrap() {
        assert_eq!(json!(m.world.game.tick), row["before"]["tick"]);
        match row["op"].as_str().unwrap() {
            "command" => {
                let command = Command::from_json(&row["cmd"]).unwrap();
                let before = AtomicState::capture(&m);
                let wallet = m.start.economy.players[0].mp;
                let denial = m.execute_command(0, &command, false).unwrap();
                assert_eq!(json!(denial.map(|d| d.name())), row["result"]);
                if denial.is_some() { before.unchanged(&m); }
                else {
                    let delta = value_bits(&row["after"]["players"][0]["mp"])
                        - value_bits(&row["before"]["players"][0]["mp"]);
                    assert_eq!(m.start.economy.players[0].mp, wallet + delta);
                }
                commands += 1;
            }
            "advance" => {
                let count = row["count"].as_u64().unwrap();
                let wallet = m.start.economy.players[0].mp;
                for _ in 0..count { m.advance_classic_depot_tick().unwrap(); }
                ticks += count;
                let p = &m.start.economy.players[0];
                let source = &row["after"]["players"][0];
                assert_eq!(bits(p.inc), source["inc"].as_str().unwrap());
                assert_eq!(p.fuel_inc.map(bits).unwrap(), source["fuelInc"].as_str().unwrap());
                assert_eq!(p.upkeep.map(bits).unwrap(), source["upkeep"].as_str().unwrap());
                match row["phase"].as_str().unwrap() {
                    "baseline" => baseline = Some(p.inc),
                    "unfinished" | "cancelled" | "started" => assert_eq!(p.inc, baseline.unwrap()),
                    "paying" | "still-paying" => {
                        let paying = m.classic_runtime.as_ref().unwrap().nodes.iter().find(|n| n.depot.is_some()).unwrap();
                        let id = paying.depot.unwrap();
                        assert_eq!(m.world.game.units[&id].built, Some(1.));
                        assert!((p.inc - baseline.unwrap() - paying.rate).abs() < 1e-12);
                    }
                    "completed" => assert_eq!(m.world.game.units[&own(&m, 0, "depot")].built, Some(1.)),
                    _ => panic!("unexpected income phase"),
                }
                // One-step deltas compare the same live-node component. Earlier
                // construction-time wallet totals are not a whole-match gate.
                if count == 1 {
                    let delta = value_bits(&row["after"]["players"][0]["mp"])
                        - value_bits(&row["before"]["players"][0]["mp"]);
                    assert!((p.mp - wallet - delta).abs() < 1e-10);
                }
            }
            _ => panic!("unexpected income record"),
        }
        assert_eq!(json!(m.world.game.tick), row["after"]["tick"]);
        compare_live_nodes(&m, &row["after"]);
    }
    assert_eq!((commands, ticks), (8, 705));
}

#[test]
fn base26_ordinary_and_classic_missing_cancel_identity_refuses_without_selected_fallback() {
    let fixture = fixture();
    let case = fixture["cases"].as_array().unwrap().iter().find(|c| c["mode"] == "conquest").unwrap();
    let mut authored = input(false);
    authored["settings"]["mode"] = json!("conquest");
    assert_eq!(case["input"], authored);
    let mut m = make_ordinary();
    assert!(m.classic_runtime.is_none());
    assert_eq!(case["records"].as_array().unwrap().len(), 4);
    for row in case["records"].as_array().unwrap() {
        assert_eq!(row["result"], "unseen");
        let command = Command::from_json(&row["cmd"]).unwrap();
        let before = AtomicState::capture(&m);
        assert_eq!(m.execute_command(0, &command, false).unwrap(), Some(Denial::Unseen));
        before.unchanged(&m);
    }
    // Exercise the existing canonical input boundary as well as direct dispatch.
    // This carries only a refusal; it admits no physical site teardown.
    let mut m = make(&input(false));
    let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let wallet = m.start.economy.players[0].mp;
    m.enqueue_command(crate::orders::OrderKey { tick: 1, issuer: 0, sequence: 1 },
        Command::from_json(&json!({"t":"cancel"})).unwrap()).unwrap();
    let results = m.advance_classic_base_tick().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].denial, Some(Denial::Unseen));
    assert_eq!(m.world.game.tick, 1);
    assert_eq!(m.world.game.units.keys().copied().collect::<Vec<_>>(), ids);
    assert!(m.start.economy.players[0].mp > wallet, "only ordinary income advances");
}
