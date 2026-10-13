use serde_json::json;
use std::collections::BTreeMap;
use ww2_core::{
    map::{AuthoritativeMap, Building, Spawn},
    world::World,
    world_map::{WorldDefinition, WorldMapInput, WorldRegion, WorldSize},
};
fn input() -> WorldMapInput {
    WorldMapInput {
        name: "Typed World".into(),
        width: 20,
        height: 20,
        chars: vec![b'.'; 400],
        heights: Some(vec![0; 400]),
        layers: None,
        spawns: vec![Spawn {
            x: 2,
            y: 2,
            assault: false,
        }],
        buildings: vec![],
        geography: WorldDefinition {
            size: WorldSize::Huge,
            region_cells: 64,
            regions: (0..64)
                .map(|id| WorldRegion {
                    id,
                    x: 2 + id as usize % 8 * 2,
                    y: 2 + id as usize / 8 * 2,
                    bounds: [0, 0, 20, 20],
                    home: (id == 0).then_some(0),
                    kind: Some("rural".into()),
                    name: Some(format!("Region {id}")),
                    biome: None,
                    team: (id == 0).then_some(4),
                })
                .collect(),
            region_map: Some(vec![0; 400]),
            biomes: None,
            seed: Some(42),
            generator_version: Some(1),
            metadata: BTreeMap::from([("diagnostic".into(), json!({"retained":true}))]),
        },
    }
}
#[test]
fn typed_world_handoff_preserves_authoritative_layers_and_atomic_failures() {
    let mut source = input();
    source.chars[210] = b'N';
    source.chars[399] = b'B';
    source.buildings.push(Building {
        x: 19,
        y: 19,
        kind: "church".into(),
    });
    source.geography.regions[0].biome = Some("farmland".into());
    let map = AuthoritativeMap::from_world(source.clone()).unwrap();
    source.layers = Some(map.layers().clone());
    let map = AuthoritativeMap::from_world(source.clone()).unwrap();
    let world = World::from_map(&map).unwrap();
    assert_eq!(world.geography(), map.geography());
    assert!(world.game.players.is_empty() && world.game.units.is_empty());
    assert_eq!(world.mine_layer[210], 1);
    assert_eq!(world.game.physical(210), b'.');
    assert_eq!(world.geography().unwrap().regions[0].team, Some(4));
    assert_eq!(
        world.geography().unwrap().regions[0].biome.as_deref(),
        Some("farmland")
    );
    assert_eq!(world.game.house[399], 3);
    assert_eq!(world.game.cell_hp[399], 700.0);
    for field in 0..9 {
        let mut bad = source.clone();
        match field {
            0 => bad.width = 0,
            1 => {
                bad.chars.pop();
            }
            2 => bad.heights.as_mut().unwrap()[0] = 5,
            3 => bad.geography.region_map.as_mut().unwrap()[0] = 64,
            4 => bad.geography.regions[1].home = Some(0),
            5 => {
                bad.geography.metadata.insert("total".into(), json!(64));
            }
            6 => bad.layers.as_mut().unwrap().mines[210] = 2,
            7 => {
                bad.layers
                    .as_mut()
                    .unwrap()
                    .materials
                    .insert(1, "stone".into());
            }
            _ => bad.geography.biomes = Some(vec![0; 399]),
        }
        assert!(
            AuthoritativeMap::from_world(bad).is_err(),
            "bad typed field {field}"
        );
        assert_eq!(world.game.chars[210], b'N');
        assert_eq!(world.mine_layer[210], 1);
        assert_eq!(world.geography(), map.geography());
    }
    let baseline = map.source().clone();
    for field in 0..10 {
        let mut bad = baseline.clone();
        match field {
            0 => bad["world"]["regions"][0]["bounds"][0] = json!("0"),
            1 => bad["world"]["regions"][0]["bounds"][2] = json!(-1),
            2 => bad["world"]["regions"][0]["id"] = json!(2147483648_i64),
            3 => bad["world"]["seed"] = json!(-1),
            4 => bad["world"]["generatorVersion"] = json!(0.5),
            5 => bad["world"]["regions"][0]["kind"] = json!({}),
            6 => bad["world"]["regions"][0]["team"] = json!(0.5),
            7 => bad["world"]["biomes"] = json!([256]),
            8 => bad["world"] = json!(null),
            _ => bad["world"] = json!(false),
        }
        assert!(
            AuthoritativeMap::from_json(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "malformed geography {field}"
        );
        assert_eq!(map.source(), &baseline);
    }
}

#[test]
fn ordinary_match_start_refuses_newly_supported_world_maps() {
    use ww2_core::{
        economy::{Faction, Mode},
        economy_state::ArmySize,
        match_runtime::{Match, MatchOptions, PlayerConfig},
        weather::WeatherChoice,
    };
    let map = AuthoritativeMap::from_world(input()).unwrap();
    let options = MatchOptions {
        seed: 42,
        mode: Mode::Conquest,
        players: vec![PlayerConfig {
            name: "Home".into(),
            team: 0,
            faction: Faction::try_from(0_u8).unwrap(),
        }],
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: None,
        living_weather: false,
        map_key: None,
        tech: false,
        supply: false,
        logistics: false,
        sandbox: false,
    };
    assert!(
        matches!(Match::new(&map, options),Err(e) if e == "native startup does not yet initialize World matches")
    );
}
