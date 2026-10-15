//! The original two-rifle groupMove oracle through Match command dispatch.
use super::Command;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    weather::WeatherChoice,
    world::CellChange,
};
use serde_json::{Value, json};

fn bits(value: f64) -> Value {
    json!(format!("{:016x}", value.to_bits()))
}

fn actors(state: &Match) -> Value {
    state
        .world
        .game
        .units
        .values()
        .map(|u| {
            json!({"id":u.id,"type":state.rules.kinds[u.kind].name,"owner":u.owner,
                "x":bits(u.x),"z":bits(u.z),"hp":bits(u.hp),"rot":bits(u.rot)})
        })
        .collect()
}

// Mirror Source clearFixtureUnits(g, u => UNITS[u.type].building), after real startup.
// Keep the original mobile actors and remove each building's linked footprint facts together.
fn clear_starting_buildings(state: &mut Match) {
    let buildings: Vec<_> = state
        .world
        .game
        .units
        .values()
        .filter(|u| state.rules.kinds[u.kind].structure)
        .cloned()
        .collect();
    assert_eq!(buildings.len(), 4, "canonical starting HQs and barracks");
    for building in buildings {
        let cells = building
            .cells
            .as_ref()
            .expect("canonical building footprint");
        let structure = state
            .world
            .structures
            .iter()
            .find(|s| s.entity_id == Some(building.id))
            .expect("canonical linked building structure");
        assert_eq!(
            structure.cells,
            cells.iter().map(|&c| c as usize).collect::<Vec<_>>()
        );
        let structure_id = structure.id.clone();
        state.world.structures.retain(|s| s.id != structure_id);
        for &cell in cells {
            let cell = cell as usize;
            let footprint = state.building_cells.remove(&cell).expect("owned footprint");
            assert_eq!(
                (footprint.unit, footprint.owner),
                (building.id, building.owner)
            );
            assert_eq!(footprint.at, building.at());
            let section = state
                .world
                .sections
                .remove(&cell)
                .expect("building section");
            assert_eq!(section.structure_id, structure_id);
            state
                .world
                .game
                .sections
                .remove(&(cell as u32))
                .expect("section health");
            let initial = state.world.initial();
            let change = CellChange {
                ground: Some(initial.ground[cell]),
                object: Some(initial.objects[cell]),
                mine: Some(initial.mines[cell] != 0),
                height: Some(initial.height.as_ref().map_or(0, |h| h[cell])),
                ..CellChange::default()
            };
            state.world.mutate_cell(cell, change).unwrap();
        }
        state.world.game.units.remove(&building.id).unwrap();
        state.grid.remove_unit(building.id);
        state.commands.units.remove(&building.id).unwrap();
        state.combat.remove(building.id);
        state.firing.remove(building.id);
        state.abilities.remove(building.id);
        state.recovery.remove(building.id);
        state.production.remove_building(building.id);
        state.building_rallies.remove(&building.id);
        state.recruit_metadata.remove(&building.id);
    }
    assert!(state.building_cells.is_empty());
    assert!(state.world.structures.is_empty());
    assert!(state.world.sections.is_empty());
    assert!(state.world.game.sections.is_empty());
    // Startup routing has not been queried. Rebind its existing owners to the cleared scene.
    state.initialize_recipient_routing().unwrap();
}

#[test]
fn original_group_move_acceptance_and_both_paths_match_source_oracle() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/mov-group-move-selected.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    assert_eq!(fixture["startupRandom"], 0.5);
    assert_eq!(fixture["sourceTest"], "test-path-performance.mjs:190-195");
    assert_eq!(
        fixture["oracle"],
        "8c0b031cc901b56c8dafc8d6ad52a3fed4ae8480457ad5ab3bc86f56cdc6271a"
    );
    assert_eq!(fixture["accepted"], true);
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&fixture["map"]).unwrap()).unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "Blue".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "Red".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
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
        },
    )
    .expect("canonical Match startup must admit the original Source map");
    clear_starting_buildings(&mut state);
    assert_eq!(
        actors(&state),
        fixture["initial"],
        "original starting actors"
    );
    assert_eq!(state.world.game.tick, fixture["tick"].as_i64().unwrap());
    assert_eq!(json!(state.world.game.flags), fixture["terrain"]["flags"]);
    assert_eq!(
        String::from_utf8(state.world.game.chars.clone()).unwrap(),
        fixture["terrain"]["chars"]
    );
    assert_eq!(
        String::from_utf8(state.world.game.ground.clone().unwrap()).unwrap(),
        fixture["terrain"]["ground"]
    );
    assert_eq!(
        String::from_utf8(state.world.game.objects.clone().unwrap()).unwrap(),
        fixture["terrain"]["objects"]
    );
    assert_eq!(json!(state.world.game.height), fixture["terrain"]["height"]);
    // Source supplies complete known state. Verify the real ordinary recipient provider
    // supplies those same flat-map facts before asking the command dispatcher for paths.
    state
        .refresh_recipient_routing(0)
        .expect("actual recipient provider must admit the scene");
    {
        let recipient = state.recipient_routing(0).unwrap();
        let query = recipient.query();
        let terrain = query.path_terrain(recipient.live()).unwrap();
        assert_eq!(json!(terrain.navigation.flags), fixture["terrain"]["flags"]);
        assert_eq!(json!(terrain.near_walls), fixture["terrain"]["near"]);
        assert!(
            terrain
                .navigation
                .heights
                .is_none_or(|h| h.iter().all(|&v| v == 0))
        );
        assert!(terrain.known_mines.iter().all(|v| !v));
        assert!(terrain.fires.iter().all(|v| !v));
        assert!(terrain.bunkers.is_empty());
        assert!(!terrain.roads && terrain.wetness == 0.0);
    }
    let rifles: Vec<_> = state
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "rifle")
        .map(|u| u.id)
        .collect();
    assert!(rifles.len() >= 2);
    assert_eq!(
        fixture["input"],
        json!({"t":"move","orders":[[rifles[0],73,31],[rifles[1],73,35]]})
    );
    assert!(
        rifles[..2]
            .iter()
            .all(|id| state.world.game.units[id].path.is_empty())
    );
    let command = Command::from_json(&fixture["input"]).unwrap();
    let denial = state
        .execute_command(0, &command, false)
        .expect("actual Match command/provider dispatch must admit the original groupMove scene");
    assert!(
        denial.is_none(),
        "original group move should be accepted: {denial:?}"
    );
    let paths: Value = rifles[..2]
        .iter()
        .map(|id| {
            state.world.game.units[id]
                .path
                .iter()
                .map(|p| json!([bits(p.x), bits(p.z)]))
                .collect::<Value>()
        })
        .collect();
    assert_eq!(
        paths, fixture["pathBits"],
        "both actual command-generated groupMove routes"
    );
    assert!(
        rifles[..2]
            .iter()
            .all(|id| !state.world.game.units[id].path.is_empty())
    );
}
