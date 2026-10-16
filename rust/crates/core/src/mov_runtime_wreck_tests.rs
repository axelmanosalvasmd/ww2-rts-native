use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{MOVE, Point, VBLOCK},
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_commands::UnitControl,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    movement,
    unit_spawn::initial_unit,
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Wreck movement intent", "w":80, "h":60,
            "rows":vec![".".repeat(80);60],
            "spawns":[{"x":8,"y":10},{"x":70,"y":50}],
            "points":[{"x":4,"y":55,"vp":0,"mp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 1906,
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
            army: ArmySize::Endless,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}

fn add_tank(m: &mut Match, at: Point) -> u32 {
    let id = m.next_unit_id;
    m.next_unit_id += 1;
    let unit = initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        m.rules.kind("tank").unwrap(),
        id,
        0,
        at,
    )
    .unwrap();
    let control = UnitControl::new(&unit, &m.rules).unwrap();
    m.commands.insert(id, control);
    m.firing.initialize(id);
    m.combat.initialize(id);
    m.abilities.initialize(id);
    m.recovery.initialize(id);
    m.grid.update(id, &unit);
    m.world.game.units.insert(id, unit);
    id
}

#[test]
fn mov106_dead_tank_retires_into_moving_wreck_then_solid_q_with_inf_cover() {
    let mut m = scene();
    let id = add_tank(&mut m, Point { x: 80.0, z: 60.0 });
    m.world.game.units.get_mut(&id).unwrap().hp = 0.0;
    let first = m.advance_tick().expect("native death tick");
    assert!(first.retired.iter().any(|r| r.unit.id == id));
    assert!(!m.world.game.units.contains_key(&id));
    assert!(
        m.grid
            .unordered(&m.world.game.units, (80.0, 60.0), 16.0, |other, _| other
                == id)
            .is_empty()
    );
    assert_eq!(m.world.game.wrecks.len(), 1);
    assert_eq!(m.world.game.wrecks[0].id, Some(id));
    assert!(m.world.game.wrecks[0].moving);
    for _ in 0..65 {
        if !m.world.game.wrecks[0].moving {
            break;
        }
        m.advance_tick().expect("native debris tick");
    }
    let wreck = &m.world.game.wrecks[0];
    assert!(!wreck.moving, "the bounded debris body must settle");
    let cell = wreck
        .cell
        .expect("settled open ground becomes a wreck cell");
    assert_eq!(m.world.game.physical(cell), b'Q');
    assert_eq!(m.world.game.flags[cell] & (MOVE | VBLOCK), VBLOCK);
    let cover_target = initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        m.rules.kind("rifle").unwrap(),
        id + 1,
        0,
        Point {
            x: wreck.x - 2.0,
            z: wreck.z,
        },
    )
    .unwrap();
    assert!(
        m.cover_behind(
            &cover_target,
            Point {
                x: wreck.x + 4.0,
                z: wreck.z
            }
        )
        .unwrap()
            > 0.0
    );
    let mut infantry = m.world.game.clone();
    infantry.units.clear();
    let mut walker = cover_target.clone();
    walker.x = wreck.x - 4.0;
    walker.z = wreck.z;
    walker.path = vec![Point {
        x: wreck.x + 4.0,
        z: wreck.z,
    }];
    infantry.units.insert(walker.id, walker);
    let mut grid = SpatialGrid::new(&infantry.units, 16.0);
    for _ in 0..40 {
        movement::move_unit(&mut infantry, &m.rules, &mut grid, id + 1, 1.0).unwrap();
    }
    assert!(
        infantry.units[&(id + 1)].x > wreck.x,
        "infantry crosses the wreck cell"
    );
}
