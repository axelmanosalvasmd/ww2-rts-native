//! Authored native component inputs. These helpers do not replay expected state.
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{Game, Point, Unit},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    rules::Rules,
    unit_publication::{self, PublicationParts, StagedUnit},
    unit_spawn,
    weather::WeatherChoice,
    world::World,
};
use serde_json::json;
pub fn point(x: f64, z: f64) -> Point {
    Point { x, z }
}
pub fn map(rows: Vec<String>) -> AuthoritativeMap {
    let h = rows.len();
    let w = rows[0].len();
    AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({"name":"MOV native behavior",
        "w":w,"h":h,"rows":rows,"spawns":[{"x":2,"y":2},{"x":w-3,"y":h-3}],
        "points":[{"id":"p","x":w/2,"y":h/2}]}))
        .unwrap(),
    )
    .unwrap()
}
pub fn ground() -> Game {
    let mut g = World::from_map(&map(vec![".".repeat(64); 64]))
        .unwrap()
        .game;
    g.players = vec![crate::game::Player {
        slot: 0,
        team: 0,
        visible: vec![],
    }];
    g
}
pub fn unit(rules: &Rules, name: &str, id: u32, at: Point) -> Unit {
    unit_spawn::initial_unit(
        rules,
        Catalog::compiled().unwrap(),
        rules.kind(name).unwrap(),
        id,
        0,
        at,
    )
    .unwrap()
}
pub fn scene() -> Match {
    scene_rows(vec![".".repeat(64); 64])
}
pub fn scene_rows(rows: Vec<String>) -> Match {
    Match::new(
        &map(rows),
        MatchOptions {
            seed: 7301,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "B".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
// Explicit component spawns use the native publication transaction and preserve the constructor roster.
pub fn spawn(m: &mut Match, name: &str, owner: usize, at: Point) -> u32 {
    let id = m.next_unit_id;
    let draft = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        owner,
        m.rules.kind(name).unwrap(),
        id,
        0,
        m.start.players[owner].spawn,
        Some(at),
    )
    .unwrap();
    let parts = PublicationParts {
        units: &mut m.world.game.units,
        grid: &mut m.grid,
        controls: &mut m.commands.units,
        recruitment: &mut m.recruit_metadata,
        next_id: &mut m.next_unit_id,
    };
    unit_publication::prepare_publication(parts, &m.rules, m.start.players.len(), vec![draft])
        .unwrap()
        .publish();
    m.combat.initialize(id);
    m.firing.initialize(id);
    m.abilities.initialize(id);
    id
}
pub fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}
