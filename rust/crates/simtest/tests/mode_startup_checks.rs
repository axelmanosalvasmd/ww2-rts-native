//! Existing constructor consumers for MODE-15/19/20. Full mode admission remains separately held.
use serde_json::{Value, json};
use std::collections::BTreeSet;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    weather::{WeatherChoice, WeatherKind},
};
fn fixture() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mode-startup-checks.json");
    let f: Value = serde_json::from_str(
        &std::fs::read_to_string(path)
            .expect("generate the authorized startup fixture before Cargo"),
    )
    .unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["format"], 1);
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(
        f["sourceHash"],
        "7831cff6eb9df90f5c73cc849636c968599dd73f9ad86a2f93b3da17b4ca0207"
    );
    assert_eq!(
        f["kind"],
        "bounded-actual-constructors-not-complete-mode-admission"
    );
    f
}
fn bits(v: f64) -> String {
    format!("{:016x}", v.to_bits())
}
fn options(teams: &[i32], mode: Mode, shuffle: bool) -> MatchOptions {
    MatchOptions {
        seed: 701,
        mode,
        players: teams
            .iter()
            .enumerate()
            .map(|(i, &team)| PlayerConfig {
                name: format!("P{i}"),
                team,
                faction: Faction::try_from((i % 3) as u8).unwrap(),
            })
            .collect(),
        shuffle,
        army: ArmySize::Standard,
        weather: WeatherChoice::Fixed(WeatherKind::Clear),
        weather_seed: Some(31),
        living_weather: true,
        map_key: None,
        tech: false,
        supply: true,
        logistics: false,
        sandbox: false,
    }
}
fn map(input: &Value) -> AuthoritativeMap {
    let source = &input["map"];
    let name = input["name"].as_str().unwrap();
    if name.ends_with(".json") {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../server/src/authoritative-maps")
            .join(name);
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), *source);
        AuthoritativeMap::from_json(&bytes).unwrap()
    } else {
        AuthoritativeMap::from_json(&serde_json::to_vec(source).unwrap()).unwrap()
    }
}
#[test]
fn mode15_defend_cycles_use_actual_assault_constructors() {
    let f = fixture();
    let cases = f["defendCycles"].as_array().unwrap();
    assert_eq!(cases.len(), 68);
    let mut names = BTreeSet::new();
    for c in cases {
        names.insert(c["input"]["name"].as_str().unwrap());
        let map = map(&c["input"]);
        let teams = c["teams"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap() as i32)
            .collect::<Vec<_>>();
        let m = Match::new_assault(
            &map,
            options(&teams, Mode::Assault, c["shuffle"].as_bool().unwrap()),
            1,
        )
        .unwrap();
        let actual = m
            .start
            .players
            .iter()
            .enumerate()
            .map(|(slot, p)| {
                json!([
                    slot,
                    p.config.name,
                    p.config.team,
                    p.config.faction as u8,
                    bits(p.spawn.x),
                    bits(p.spawn.z)
                ])
            })
            .collect::<Vec<_>>();
        assert_eq!(json!(actual), c["expected"]);
        assert_eq!(m.world.game.tick, 0);
        assert!(!m.world.game.units.is_empty());
    }
    assert_eq!(names.len(), 17);
    assert!(AuthoritativeMap::from_json(&serde_json::to_vec(&f["invalidHill"]).unwrap()).is_err());
    // This consumer does not replace the held Hill route or Assault runner with a helper.
}
#[test]
fn mode20_assault_points_and_needs_keep_complete_projection() {
    let f = fixture();
    let cases = f["pointCases"].as_array().unwrap();
    assert_eq!(cases.len(), 32);
    for c in cases {
        let map = map(&c["input"]);
        let mode = Mode::from_kind(c["mode"].as_str());
        let o = options(&[0, 1], mode, false);
        let m = if mode == Mode::Assault {
            Match::new_assault(&map, o, 1)
        } else {
            Match::new(&map, o)
        }
        .unwrap();
        let actual = m
            .start
            .points
            .iter()
            .map(|p| {
                json!([
                    p.id,
                    bits(p.at.x),
                    bits(p.at.z),
                    bits(p.vp),
                    bits(p.mp),
                    p.kind,
                    p.needs.map_or(-1, |n| n as i64),
                    p.owner.map_or(-1, |n| n as i64),
                    p.capper.map_or(-1, |n| n as i64),
                    bits(p.progress),
                    p.cut
                ])
            })
            .collect::<Vec<_>>();
        assert_eq!(
            json!(actual),
            c["expected"],
            "{} {}",
            c["input"]["name"],
            c["mode"]
        );
        assert_eq!(m.world.game.tick, 0);
    }
}
#[test]
fn mode19_three_player_force_runs_original_five_seconds() {
    let f = fixture();
    let c = &f["defaultThree"];
    assert_eq!(c["tick"], 100);
    for mode in [Mode::Default, Mode::Conquest] {
        let map = map(&c["input"]);
        let mut o = options(&[0, 1, 2], mode, true);
        for (p, name) in o.players.iter_mut().zip(["a", "b", "c"]) {
            p.name = name.into();
        }
        o.weather = WeatherChoice::Map;
        o.weather_seed = None;
        let mut m = Match::new(&map, o).unwrap();
        let types = (0..3)
            .map(|owner| {
                m.world
                    .game
                    .units
                    .values()
                    .filter(|u| u.owner == owner && !m.rules.kinds[u.kind].building)
                    .map(|u| m.rules.kinds[u.kind].name.clone())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(types.iter().map(Vec::len).sum::<usize>(), 9);
        assert_eq!(json!(types), c["startTypes"]);
        for _ in 0..100 {
            m.advance_tick().unwrap();
        }
        assert_eq!(m.world.game.tick, 100);
    }
}
