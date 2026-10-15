//! MODE-14 constructor consumers. Source candidates do not imply shared seeded Match RNG.
use serde_json::Value;
use std::collections::BTreeSet;
use ww2_core::{
    catalog::Catalog,
    deployment::{self, LayoutSelection, Spawn, TerrainView},
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::CELL,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    random::{MatchRandom, StreamId},
    weather::{WeatherChoice, WeatherKind},
};

fn array(v: &Value) -> &[Value] {
    v.as_array().unwrap()
}
fn integers(v: &Value) -> Vec<usize> {
    array(v).iter().map(|n| n.as_u64().unwrap() as usize).collect()
}
fn teams(v: &Value) -> Vec<i32> {
    array(v).iter().map(|n| n.as_i64().unwrap() as i32).collect()
}
fn fixture() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mode14-spawn-layout.json");
    let f: Value = serde_json::from_str(
        &std::fs::read_to_string(path).expect("generate the authorized MODE14 fixture before Cargo"),
    )
    .unwrap();
    assert_eq!(f["format"], 1);
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(
        f["kind"],
        "original-source-candidate-sets-not-seeded-whole-match-equality"
    );
    assert_eq!(
        f["sourceHash"],
        "7831cff6eb9df90f5c73cc849636c968599dd73f9ad86a2f93b3da17b4ca0207"
    );
    f
}
fn authored_distances(input: &Value, selected: &[usize]) -> Vec<f64> {
    let catalog = Catalog::compiled().unwrap();
    let table = catalog.table("TERRAIN").unwrap();
    let flags: Vec<u16> = array(&input["rows"])
        .iter()
        .flat_map(|r| r.as_str().unwrap().chars())
        .map(|c| table.get(&c.to_string()).and_then(|v| v.number()).unwrap() as u16)
        .collect();
    let heights: Option<Vec<i8>> = input.get("heights").map(|rows| {
        array(rows)
            .iter()
            .flat_map(|r| r.as_str().unwrap().bytes())
            .map(|b| match b {
                b'a' => -1,
                b'b' => -2,
                _ => (b - b'0') as i8,
            })
            .collect()
    });
    let spawns = spawns(input);
    deployment::spawn_distances(
        &TerrainView {
            width: input["w"].as_u64().unwrap() as usize,
            height: input["h"].as_u64().unwrap() as usize,
            flags: &flags,
            heights: heights.as_deref(),
        },
        &spawns,
        selected,
    )
    .unwrap()
}
fn spawns(input: &Value) -> Vec<Spawn> {
    array(&input["spawns"])
        .iter()
        .map(|s| Spawn {
            x: s["x"].as_u64().unwrap() as usize,
            y: s["y"].as_u64().unwrap() as usize,
            assault: s["assault"].as_bool().unwrap_or(false),
        })
        .collect()
}
fn assert_distances(actual: &[f64], expected: &Value) {
    assert_eq!(actual.len(), array(expected).len());
    for (a, b) in actual.iter().zip(array(expected)) {
        assert_eq!(format!("{:016x}", a.to_bits()), b.as_str().unwrap());
    }
}
fn options(roster: &[i32], seed: u64, shuffle: bool) -> MatchOptions {
    MatchOptions {
        seed,
        mode: Mode::Conquest,
        players: roster
            .iter()
            .enumerate()
            .map(|(i, &team)| PlayerConfig {
                name: format!("P{i}"),
                team,
                faction: Faction::try_from((i % 4) as u8).unwrap(),
            })
            .collect(),
        shuffle,
        army: ArmySize::Standard,
        weather: WeatherChoice::Fixed(WeatherKind::Clear),
        weather_seed: Some(31),
        living_weather: true,
        map_key: None,
        tech: true,
        supply: false,
        logistics: false,
        sandbox: false,
    }
}
fn actual_slots(m: &Match, input: &Value) -> Vec<usize> {
    m.start
        .players
        .iter()
        .map(|p| {
            array(&input["spawns"])
                .iter()
                .position(|s| {
                    p.spawn.x.to_bits() == ((s["x"].as_f64().unwrap() + 0.5) * CELL).to_bits()
                        && p.spawn.z.to_bits()
                            == ((s["y"].as_f64().unwrap() + 0.5) * CELL).to_bits()
                })
                .unwrap()
        })
        .collect()
}
fn native_constructor(
    map: &AuthoritativeMap,
    roster: &[i32],
    seed: u64,
    shuffle: bool,
    distances: &[f64],
    usable: &[usize],
) -> Vec<usize> {
    // The actual constructor recomputes its distances and selects from its own Terrain stream.
    // This separate native API check neither injects a draw nor supplies expected slots to Match.
    let m = Match::new(map, options(roster, seed, shuffle)).unwrap();
    let slots = actual_slots(&m, map.source());
    let mut expected_random = MatchRandom::new(seed);
    let selection = if shuffle {
        LayoutSelection::RandomFraction(expected_random.next_fraction(StreamId::Terrain))
    } else {
        LayoutSelection::First
    };
    let relative = deployment::spawn_slots(usable.len(), roster, Some(distances), selection).unwrap();
    assert_eq!(slots, relative.iter().map(|&i| usable[i]).collect::<Vec<_>>());
    expected_random.next_u64(StreamId::Weather);
    assert_eq!(m.start.random.snapshot(), expected_random.snapshot());
    assert_eq!(m.world.game.tick, 0);
    assert_eq!(m.start.players.len(), roster.len());
    assert!(!m.world.game.units.is_empty());
    for (i, p) in m.start.players.iter().enumerate() {
        assert_eq!(p.config.name, format!("P{i}"));
        assert_eq!(p.config.team, roster[i]);
        assert_eq!(p.config.faction as u8, (i % 4) as u8);
        assert_eq!(m.world.game.players[i].team, roster[i]);
    }
    let unique: BTreeSet<_> = slots.iter().collect();
    if roster.len() <= usable.len() {
        assert_eq!(unique.len(), roster.len());
    } else {
        assert!(unique.len() < roster.len(), "more players than spawns must share");
    }
    slots
}
fn source_good_contains(observation: &Value, usable: &[usize], actual: &[usize]) {
    assert!(!array(&observation["good"]).is_empty());
    assert!(array(&observation["good"]).iter().any(|c| {
        integers(&c["slots"]).iter().map(|&i| usable[i]).collect::<Vec<_>>().as_slice() == actual
    }));
}
fn assert_observation_input(
    observation: &Value,
    roster: &[i32],
    usable: &[usize],
    distances: &[f64],
    shuffle: bool,
) {
    assert_eq!(observation["nSpawns"].as_u64().unwrap() as usize, usable.len());
    assert_eq!(teams(&observation["teams"]).as_slice(), roster);
    assert_eq!(observation["shuffle"].as_bool().unwrap(), shuffle);
    let expected: Vec<Value> = array(&observation["dist"])
        .iter()
        .flat_map(|row| array(row).iter().cloned())
        .collect();
    assert_distances(distances, &Value::Array(expected));
    assert!(!array(&observation["all"]).is_empty());
    assert!(array(&observation["close"]).len() <= array(&observation["all"]).len());
    assert!(!array(&observation["good"]).is_empty());
    assert!(array(&observation["good"]).len() <= array(&observation["close"]).len());
    for candidate in array(&observation["good"]) {
        let slots = integers(&candidate["slots"]);
        assert_eq!(slots.len(), roster.len());
        assert!(slots.iter().all(|&slot| slot < usable.len()));
    }
}
fn shipped_map(c: &Value) -> AuthoritativeMap {
    let name = c["name"].as_str().unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../server/src/authoritative-maps")
        .join(name);
    let bytes = std::fs::read(path).unwrap();
    let input: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(input, c["map"], "complete shipped map input {name}");
    AuthoritativeMap::from_json(&bytes).unwrap()
}

#[test]
fn mode14_original_walking_and_layout_checklist_matches_source() {
    let f = fixture();
    assert_eq!(array(&f["walking"]).len(), 2);
    for (i, c) in array(&f["walking"]).iter().enumerate() {
        let actual = authored_distances(&c["map"], &integers(&c["indices"]));
        assert_distances(&actual, &c["distances"]);
        assert_eq!(actual[1].to_bits(), [28.0_f64, 7.0][i].to_bits());
    }
    let ring = |a: usize, b: usize| a.abs_diff(b).min(6 - a.abs_diff(b));
    let fixed = array(&f["ringFixed"]);
    assert_eq!(fixed.len(), 3);
    let mut selected = Vec::new();
    for c in fixed {
        let slots = deployment::spawn_slots(6, &teams(&c["teams"]), None, LayoutSelection::First).unwrap();
        assert_eq!(slots, integers(&c["slots"]));
        selected.push(slots);
    }
    assert_eq!(selected[0], [0, 3]);
    let side = &selected[1];
    assert_eq!(ring(side[0], side[2]) + ring(side[2], side[4]) + ring(side[0], side[4]), 4);
    let ffa = &selected[2];
    assert!(ring(ffa[0], ffa[1]) == 2 && ring(ffa[1], ffa[2]) == 2);
    assert_eq!(array(&f["ringShuffled"]).len(), 20);
    for c in array(&f["ringShuffled"]) {
        let slots = deployment::spawn_slots(
            6,
            &teams(&c["teams"]),
            None,
            LayoutSelection::RandomFraction(c["fraction"].as_f64().unwrap()),
        )
        .unwrap();
        assert_eq!(slots, integers(&c["slots"]));
        assert_eq!(slots.iter().collect::<BTreeSet<_>>().len(), 6);
    }
    let peg = array(&f["maps"]).iter().find(|c| c["name"] == "pegasus-bridge.json").unwrap();
    let map = shipped_map(peg);
    let usable = integers(&peg["usable"]);
    let distances = authored_distances(map.source(), &usable);
    assert_distances(&distances, &peg["distances"]);
    assert_eq!(array(&f["pegasusConstructors"]).len(), 20);
    for (seed, source) in array(&f["pegasusConstructors"]).iter().enumerate() {
        assert_observation_input(&source["observation"], &[0, 1, 0, 1], &usable, &distances, true);
        let slots = native_constructor(&map, &[0, 1, 0, 1], seed as u64, true, &distances, &usable);
        source_good_contains(&source["observation"], &usable, &slots);
        let north = |slot: usize| map.spawns()[slots[slot]].y < map.dimensions().1 / 2;
        assert_eq!(north(0), north(2));
        assert_ne!(north(0), north(1));
    }
    eprintln!("MODE14_CHECKLIST walking=2 ring=23 native_pegasus_constructors=20");
}

#[test]
fn mode14_all_shipped_map_constructor_layouts_preserve_source_candidates() {
    let f = fixture();
    assert_eq!(array(&f["maps"]).len(), 33);
    assert_eq!(integers(&f["nativeSeeds"]), [701, 702]);
    let mut roster_count = 0;
    let mut constructor_count = 0;
    let mut names = BTreeSet::new();
    for c in array(&f["maps"]) {
        assert!(names.insert(c["name"].as_str().unwrap()));
        let map = shipped_map(c);
        let usable = deployment::spawns_for(&spawns(map.source()), Mode::Conquest).unwrap();
        assert_eq!(usable, integers(&c["usable"]));
        let distances = authored_distances(map.source(), &usable);
        assert_distances(&distances, &c["distances"]);
        assert_eq!(array(&c["cases"]).len(), 9);
        let mut rosters = BTreeSet::new();
        for case in array(&c["cases"]) {
            let roster = teams(&case["teams"]);
            assert!((2..=6).contains(&roster.len()));
            assert!(rosters.insert(roster.clone()));
            assert_observation_input(&case["fixed"]["observation"], &roster, &usable, &distances, false);
            assert_observation_input(&case["shuffled"]["observation"], &roster, &usable, &distances, true);
            let fixed = native_constructor(&map, &roster, 701, false, &distances, &usable);
            assert_eq!(fixed, integers(&case["fixed"]["slots"]));
            source_good_contains(&case["fixed"]["observation"], &usable, &fixed);
            constructor_count += 1;
            for seed in array(&f["nativeSeeds"]) {
                let slots = native_constructor(&map, &roster, seed.as_u64().unwrap(), true, &distances, &usable);
                // Only Source candidate membership is compared for independent RNG streams.
                source_good_contains(&case["shuffled"]["observation"], &usable, &slots);
                constructor_count += 1;
            }
            roster_count += 1;
        }
        for count in 2..=6 {
            assert!(rosters.contains(&(0..count).collect::<Vec<i32>>()));
            assert!(rosters.contains(&(0..count).map(|i| i % 2).collect::<Vec<i32>>()));
        }
    }
    let map_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../server/src/authoritative-maps");
    let shipped: BTreeSet<String> = std::fs::read_dir(map_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.into_iter().map(str::to_owned).collect::<BTreeSet<_>>(), shipped);
    assert_eq!(roster_count, 297);
    assert_eq!(constructor_count, 891);
    eprintln!("MODE14 maps=33 rosters=297 native_constructors=891 seeded_whole_match_equality=held");
}
