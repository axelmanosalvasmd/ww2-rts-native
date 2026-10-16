//! Complete named Source house checks through existing cover, snapshot and damage producers.
use super::SnapshotCache;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    environment::{Blast, Context, DamageContext},
    game::Point,
    grid::SpatialGrid,
    map::{AuthoritativeMap, MapError},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn fixture() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/map-house-selected.json"
    );
    let input = std::fs::read_to_string(path)
        .expect("record map-house-selected.json from pinned Source before running these tests");
    let f: Value = serde_json::from_str(&input).unwrap();
    assert_eq!(f["schemaVersion"], 1);
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(f["cases"].as_array().unwrap().len(), 2);
    f
}
fn bits(n: f64) -> String {
    format!("{:016x}", n.to_bits())
}
fn number(v: &Value, field: &str) -> f64 {
    v[field].as_f64().unwrap()
}
fn initialize(case: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 1,
            mode: Mode::Default,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: ["a", "b"][slot].into(),
                    team: slot as i32,
                    faction: Faction::try_from(slot as u8).unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: None,
            living_weather: false,
            map_key: None,
            tech: true,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
fn check_cells(m: &Match, expected: &Value) {
    for row in expected.as_array().unwrap() {
        let c = row["c"].as_u64().unwrap() as usize;
        // A starting footprint must never replace a measured authored section.
        assert!(
            !m.building_cells.contains_key(&c),
            "measured cell {c} is a starting footprint"
        );
        let g = &m.world.game;
        assert_eq!(String::from_utf8(vec![g.chars[c]]).unwrap(), row["char"]);
        assert_eq!(json!(g.house[c]), row["house"]);
        assert_eq!(bits(f64::from(g.cell_hp[c])), row["hp"]);
        assert_eq!(
            g.tough.as_ref().map_or(1.0, |v| f64::from(v[c])).to_bits(),
            row["tough"].as_f64().unwrap().to_bits()
        );
        let s = &row["section"];
        let (metadata, state) = m.world.section_by_id(s[0].as_str().unwrap()).unwrap();
        assert_eq!(metadata.structure_id, s[1]);
        assert_eq!(metadata.material, s[2]);
        assert_eq!(json!(metadata.anchor), s[3]);
        assert_eq!(json!(metadata.supports), s[4]);
        assert!(!state.failed);
        assert_eq!(
            match metadata.appearance {
                crate::world::Appearance::Intact => "intact",
                crate::world::Appearance::Damaged => "damaged",
                crate::world::Appearance::Burning => "burning",
            },
            s[5]
        );
        assert_eq!(json!(metadata.rubble), s[6]);
        assert_eq!(bits(state.hp), s[7]);
        assert_eq!(bits(state.max_hp.unwrap()), s[8]);
        assert_eq!(m.world.sections[&c].id, s[0]);
        assert!(
            m.world
                .structures
                .iter()
                .any(|structure| structure.id == metadata.structure_id
                    && structure.cells.contains(&c)
                    && structure.section_ids.contains(&metadata.id))
        );
    }
}
fn garrison(m: &mut Match, actor: &Value) -> u32 {
    let input = &actor["input"];
    assert_eq!(input["type"], "rifle");
    assert_eq!(input["owner"], 0);
    assert_eq!(input["autoRetreat"], false);
    // Retain a genuinely published native rifle and all its canonical metadata.
    // Source's purchased rifle has another ID; only the named component fields compare.
    let id = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = number(input, "x");
    u.z = number(input, "z");
    u.rot = number(input, "rot");
    u.garrison = Some(input["garrison"].as_u64().unwrap() as usize);
    let control = m.commands.units.get_mut(&id).unwrap();
    control.aim = number(input, "aim");
    control.auto_retreat = false;
    m.grid = SpatialGrid::new(&m.world.game.units, 16.0);
    id
}
fn check_snapshot(m: &Match, id: u32, actor: &Value, kind: u64) {
    let projection = SnapshotCache::new(m).unwrap().recipient(0).unwrap();
    let row = projection.fields["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[0].as_u64() == Some(u64::from(id)))
        .unwrap();
    assert_eq!(row[2], actor["row"][2]);
    let flags = row[12].as_u64().unwrap();
    assert_ne!(flags & 32, 0);
    assert_eq!((flags >> 20) & 7, kind);
    assert_eq!(actor["kind"].as_u64(), Some(kind));
    assert_eq!((actor["row"][12].as_u64().unwrap() >> 20) & 7, kind);
}

#[test]
fn named_house_types_match_original_selected_check() {
    let f = fixture();
    let case = &f["cases"][0];
    assert_eq!(case["name"], "House types");
    assert_eq!(case["source"], "test.js:773-784");
    let correction = &case["setupCorrection"];
    assert_eq!(correction["from"], json!([10, 10]));
    assert_eq!(correction["to"], json!([10, 9]));
    let mut original_map = correction["originalMap"].clone();
    original_map["points"] = json!([{ "x": 10, "y": 9 }]);
    assert_eq!(original_map, case["map"]);
    assert!(matches!(
        AuthoritativeMap::from_json(&serde_json::to_vec(&correction["originalMap"]).unwrap()),
        Err(MapError::Invalid { .. })
    ));
    for key in ["initial", "structures", "full", "half", "halfHp", "actor"] {
        assert_eq!(
            case[key], correction["original"][key],
            "Source setup invariance: {key}"
        );
    }
    let mut m = initialize(case);
    check_cells(&m, &case["initial"]);
    assert_eq!(
        [
            m.world.game.house[20],
            m.world.game.house[80],
            m.world.game.house[208]
        ],
        [0, 1, 2]
    );
    let id = garrison(&mut m, &case["actor"]);
    for (i, c) in [20, 80, 208].into_iter().enumerate() {
        m.world.game.units.get_mut(&id).unwrap().garrison = Some(c);
        let full = m
            .cover_mul(&m.world.game.units[&id], Point { x: 0.0, z: 0.0 })
            .unwrap();
        assert_eq!(bits(full), case["full"][i]);
    }
    m.world.game.cell_hp[80] /= 2.0;
    assert_eq!(bits(f64::from(m.world.game.cell_hp[80])), case["halfHp"]);
    m.world.game.units.get_mut(&id).unwrap().garrison = Some(80);
    let half = m
        .cover_mul(&m.world.game.units[&id], Point { x: 0.0, z: 0.0 })
        .unwrap();
    assert_eq!(bits(half), case["half"]);
    m.world.game.units.get_mut(&id).unwrap().garrison = Some(208);
    check_snapshot(&m, id, &case["actor"], 2);
}

#[test]
fn named_map_data_matches_original_selected_check() {
    let f = fixture();
    let case = &f["cases"][1];
    assert_eq!(case["name"], "Map data");
    assert_eq!(case["source"], "test.js:788-807");
    let mut m = initialize(case);
    check_cells(&m, &case["initial"]);
    assert_eq!(m.world.game.house[20], 3);
    assert_eq!(m.world.game.tough.as_ref().unwrap()[149], 12.0);
    assert_eq!(m.world.game.tough.as_ref().unwrap()[150], 12.0);
    let id = garrison(&mut m, &case["actor"]);
    check_snapshot(&m, id, &case["actor"], 3);
    let blast = &case["blast"];
    assert_eq!(blast["actors"], json!([]));
    assert_eq!(number(blast, "damage"), 500.0);
    let context = &case["context"];
    let mut seed = context["seed"].as_u64().unwrap() as u32;
    let mut next_id = context["nextId"].as_u64().unwrap() as u32;
    let order: Vec<_> = m.world.game.units.keys().copied().collect();
    let mut ctx = Context {
        tick: context["tick"].as_u64().unwrap(),
        seed: &mut seed,
        next_id: &mut next_id,
        unit_order: &order,
        grid: &m.grid,
        rain: number(context, "rain"),
        wet: number(context, "wet"),
        wind_angle: number(context, "windAngle"),
        wind_strength: number(context, "windStrength"),
    };
    let skip = BTreeSet::new();
    let mut hit = DamageContext::plain(&skip);
    hit.actor_order = Some(&[]);
    m.environment
        .damage_cells(
            &mut m.world,
            Blast {
                at: Point {
                    x: number(blast, "x"),
                    z: number(blast, "z"),
                },
                radius: number(blast, "radius"),
                damage: number(blast, "damage"),
                scar: number(blast, "scar"),
            },
            &mut ctx,
            &mut hit,
        )
        .unwrap();
    check_cells(&m, &case["after"]);
    assert_eq!(m.world.game.chars[149], b'=');
    assert_eq!(case["invalid"].as_array().unwrap().len(), 2);
    for invalid in case["invalid"].as_array().unwrap() {
        assert!(
            invalid["error"]
                .as_str()
                .is_some_and(|text| !text.is_empty())
        );
        assert!(matches!(
            AuthoritativeMap::from_json(&serde_json::to_vec(&invalid["map"]).unwrap()),
            Err(MapError::Invalid { .. })
        ));
    }
}
