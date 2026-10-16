use super::*;
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode, TechState},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::{MatchCommands, UnitControl},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::{SnapshotCache, SnapshotError},
    unit_spawn::{initial_aircraft, initial_unit},
    weather::WeatherChoice,
    world::World,
};
use serde_json::json;

fn map() -> AuthoritativeMap {
    AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"CMB5 visibility", "w":96, "h":96,
            "rows":vec![".".repeat(96);96],
            "spawns":[{"x":3,"y":3},{"x":92,"y":92},{"x":3,"y":92}],
            "defend":[0], "points":[{"id":"p","x":80,"y":80}]
        }))
        .unwrap(),
    )
    .unwrap()
}
fn options(teams: &[i32]) -> MatchOptions {
    MatchOptions {
        seed: 701,
        mode: Mode::Conquest,
        players: teams
            .iter()
            .enumerate()
            .map(|(slot, &team)| PlayerConfig {
                name: format!("P{slot}"),
                team,
                faction: if slot == 0 {
                    Faction::Usa
                } else {
                    Faction::Germany
                },
            })
            .collect(),
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
    }
}
fn add(m: &mut Match, name: &str, owner: usize, at: Point) -> u32 {
    let id = m.next_unit_id;
    m.next_unit_id += 1;
    let kind = m.rules.kind(name).unwrap();
    let factory = if m.rules.kinds[kind].air {
        initial_aircraft
    } else {
        initial_unit
    };
    let mut unit = factory(&m.rules, Catalog::compiled().unwrap(), kind, id, owner, at).unwrap();
    unit.hold_pos = true;
    let mut control = UnitControl::new(&unit, &m.rules).unwrap();
    control.hold_fire = true;
    control.auto_cast = false;
    control.auto_retreat = false;
    m.commands.insert(id, control);
    m.firing.initialize(id);
    m.combat.initialize(id);
    m.abilities.initialize(id);
    m.recovery.initialize(id);
    m.world.game.units.insert(id, unit);
    id
}
fn scene(teams: &[i32]) -> Match {
    scene_mode(teams, Mode::Conquest)
}
fn scene_mode(teams: &[i32], mode: Mode) -> Match {
    let map = map();
    let mut opts = options(teams);
    opts.mode = mode;
    let mut m = Match::new(&map, opts).unwrap();
    let players = m.world.game.players.clone();
    m.world = World::from_map(&map).unwrap();
    m.world.game.players = players;
    for player in &mut m.world.game.players {
        player.visible.clear();
    }
    m.building_cells.clear();
    m.commands = MatchCommands::new(4096);
    m.firing = Default::default();
    m.combat = Default::default();
    m.abilities = Default::default();
    m.recovery = Default::default();
    m.production = crate::production::ProductionState::new(vec![
        TechState::new(1, 0, 0, 0).unwrap();
        teams.len()
    ])
    .unwrap();
    m.next_unit_id = 1;
    m
}
fn rebuild(m: &mut Match) {
    m.grid.rebuild(&m.world.game.units);
}
fn at(x: f64, z: f64) -> Point {
    Point { x, z }
}
fn remembered(m: &Match, team: i32) -> &[KnownBuilding] {
    &m.classic_known_buildings
        .iter()
        .find(|(t, _)| *t == team)
        .unwrap()
        .1
}

#[test]
fn cmb130_classic_remembers_last_seen_buildings_until_an_airborne_observer_revisits() {
    let mut m = scene(&[0, 1, 0]);
    let scout = add(&mut m, "rifle", 2, at(70., 70.));
    let building = add(&mut m, "hq", 1, at(80., 70.));
    m.world.game.units.get_mut(&building).unwrap().built = Some(0.25);
    m.options.mode = Mode::Classic;
    m.world.game.mode = Some(ModeKind::Classic);
    rebuild(&mut m);
    m.refresh_classic_visibility().unwrap();
    assert!(m.world.game.players[0].sees(building));
    assert_eq!(
        m.world.game.players[0].visible,
        m.world.game.players[2].visible
    );
    assert_eq!(
        remembered(&m, 0),
        &[KnownBuilding {
            id: building,
            kind: m.rules.kind("hq").unwrap(),
            owner: 1,
            at: at(80., 70.),
            built: Some(0.25),
        }]
    );
    m.world.game.units.get_mut(&building).unwrap().built = Some(1.);
    m.refresh_classic_visibility().unwrap();
    assert_eq!(remembered(&m, 0)[0].built, Some(1.));
    m.world.game.units.get_mut(&scout).unwrap().x = 5.;
    m.world.game.units.get_mut(&scout).unwrap().z = 5.;
    m.world.game.units.get_mut(&building).unwrap().built = Some(0.5);
    rebuild(&mut m);
    m.refresh_classic_visibility().unwrap();
    assert!(!m.world.game.players[0].sees(building));
    assert_eq!(
        remembered(&m, 0)[0].built,
        Some(1.),
        "hidden changes cannot replace observed facts"
    );
    m.world.game.units.remove(&building);
    m.refresh_classic_visibility().unwrap();
    assert_eq!(
        remembered(&m, 0).len(),
        1,
        "unseen destruction retains the ghost"
    );
    let plane = add(&mut m, "fighter", 0, at(60., 70.));
    // A wall blocks ground sight. A parked plane cannot invalidate the ghost.
    let c = m.world.game.cell_of(70., 70.).unwrap();
    m.world.set_cell(c, b'K').unwrap();
    rebuild(&mut m);
    assert!(!los(&m.world.game, &m.rules, (60., 70.), (80., 70.)));
    m.refresh_classic_visibility().unwrap();
    assert_eq!(remembered(&m, 0).len(), 1);
    m.world.game.units.get_mut(&plane).unwrap().air = Some(PlaneState::Out);
    m.refresh_classic_visibility().unwrap();
    assert!(remembered(&m, 0).is_empty());
    assert!(
        m.advance_tick()
            .unwrap_err()
            .reason
            .contains("requires represented ordinary or World mode")
    );
    assert!(
        m.tick_state.fault().is_none(),
        "unsupported mode refuses before tick mutation"
    );
    assert!(matches!(
        SnapshotCache::new(&m),
        Err(SnapshotError::UnsupportedMode)
    ));
}

#[test]
fn cmb130_world_and_classic_share_building_memory_and_ground_invalidation_rules() {
    let mut m = scene(&[0, 1]);
    let scout = add(&mut m, "rifle", 0, at(70., 70.));
    let building = add(&mut m, "barracks", 1, at(80., 70.));
    rebuild(&mut m);
    update_vision(&mut m.world.game, &m.rules, &m.grid);
    let mut memory = Vec::new();
    m.world.game.mode = Some(ModeKind::World);
    refresh_building_memory(&m.world.game, &m.rules, &mut memory);
    let first = memory.clone();
    assert_eq!(first[0].1[0].id, building);
    m.world.game.units.get_mut(&scout).unwrap().x = 5.;
    rebuild(&mut m);
    update_vision(&mut m.world.game, &m.rules, &m.grid);
    refresh_building_memory(&m.world.game, &m.rules, &mut memory);
    assert_eq!(
        memory[0].1, first[0].1,
        "a hidden live building remains remembered"
    );
    m.world.game.units.remove(&building);
    refresh_building_memory(&m.world.game, &m.rules, &mut memory);
    assert_eq!(memory[0].1, first[0].1);
    m.world.game.units.get_mut(&scout).unwrap().x = 70.;
    rebuild(&mut m);
    update_vision(&mut m.world.game, &m.rules, &m.grid);
    refresh_building_memory(&m.world.game, &m.rules, &mut memory);
    assert!(
        memory[0].1.is_empty(),
        "the ground observer revisits the empty building location"
    );
}

#[test]
fn cmb127_horde_reveals_only_active_final_ground_enemies_and_bosses_to_defenders() {
    let mut opts = options(&[0, 1]);
    opts.mode = Mode::Horde;
    let mut m = Match::new_horde(&map(), opts).unwrap();
    for unit in m.world.game.units.values_mut() {
        unit.x = 5.;
        unit.z = 5.;
    }
    let slot = m.horde.as_ref().unwrap().slot;
    let enemy = add(&mut m, "rifle", slot, at(175., 175.));
    let boss = add(&mut m, "kaiju", slot, at(175., 175.));
    let rider = add(&mut m, "rifle", slot, at(175., 175.));
    m.world.game.units.get_mut(&rider).unwrap().riding = true;
    let plane = add(&mut m, "fighter", slot, at(175., 175.));
    m.world.game.units.get_mut(&plane).unwrap().air = Some(PlaneState::Out);
    rebuild(&mut m);
    for (active, budget, left, expected) in [
        (false, 0., 3., vec![]),
        (true, 100., 3., vec![boss]),
        (true, 0., 4., vec![boss]),
        // The Source Horde override includes carried ground units after ordinary vision excludes them.
        (true, 0., 3., vec![enemy, boss, rider]),
    ] {
        let horde = m.horde.as_mut().unwrap();
        horde.wave = 1;
        horde.active = active;
        horde.budget = budget;
        horde.left = left;
        m.refresh_horde_visibility().unwrap();
        for owner in 0..slot {
            let actual: Vec<_> = m.world.game.players[owner]
                .visible
                .iter()
                .copied()
                .filter(|id| [enemy, boss, plane, rider].contains(id))
                .collect();
            assert_eq!(actual, expected);
        }
        assert!(!m.world.game.players[slot].sees(enemy));
    }
    assert!(
        m.advance_tick()
            .unwrap_err()
            .reason
            .contains("requires represented ordinary or World mode")
    );
    assert!(m.tick_state.fault().is_none());
    assert!(matches!(
        SnapshotCache::new(&m),
        Err(SnapshotError::UnsupportedMode)
    ));
}

#[test]
fn cmb127_bunkers_show_through_fog_and_survive_ordinary_ticks_and_recipient_snapshots() {
    let mut m = scene(&[0, 1]);
    add(&mut m, "rifle", 0, at(10., 10.));
    let bunker = add(&mut m, "bunker", 1, at(175., 175.));
    let hidden = add(&mut m, "rifle", 1, at(170., 175.));
    rebuild(&mut m);
    crate::cmb5_match_tests::prepare_finish(&mut m);
    m.initialize_recipient_routing().unwrap();
    m.initialize_tick_scene(200, Default::default()).unwrap();
    for _ in 0..6 {
        m.advance_tick().unwrap();
        assert!(m.tick_state.fault().is_none());
        assert!(m.world.game.players[0].sees(bunker));
        assert!(!m.world.game.players[0].sees(hidden));
        let cache = SnapshotCache::new(&m).unwrap();
        for slot in 0..m.world.game.players.len() {
            cache.recipient(slot).unwrap();
        }
        let projection = cache.recipient(0).unwrap();
        let ids: Vec<_> = projection.fields["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row[0].as_u64().unwrap() as u32)
            .collect();
        assert!(ids.contains(&bunker));
        assert!(!ids.contains(&hidden));
    }
    crate::cmb5_match_tests::through_finish(&mut m, |m, cache| {
        let projection = cache.recipient(0).unwrap();
        let units = projection.fields["units"].as_array().unwrap();
        assert!(units.iter().any(|row| row[0] == bunker));
        assert_eq!(
            units.iter().any(|row| row[0] == hidden),
            m.finish.is_some(),
            "only the canonical match finish reveals the hidden enemy"
        );
        assert!(m.world.game.players[0].sees(bunker));
    });
    // World bunkers need ordinary sight, just like buildings.
    m.world.game.mode = Some(ModeKind::World);
    update_vision(&mut m.world.game, &m.rules, &m.grid);
    assert!(!m.world.game.players[0].sees(bunker));
}

fn snapshots(m: &Match) -> Vec<serde_json::Value> {
    let cache = SnapshotCache::new(m).unwrap();
    (0..m.world.game.players.len())
        .map(|slot| serde_json::Value::Object(cache.recipient(slot).unwrap().fields))
        .collect()
}

#[test]
fn cmb130_ordinary_building_memories_are_private_last_observed_facts_through_match_finish() {
    for mode in [Mode::Default, Mode::Conquest] {
        for teams in [[0, 1, 2], [0, 1, 0]] {
            let mut m = scene_mode(&teams, mode);
            let scout = add(&mut m, "rifle", 0, at(70., 70.));
            add(&mut m, "rifle", 2, at(10., 170.));
            let building = m.next_unit_id;
            let cell = m.world.game.cell_of(80., 70.).unwrap();
            let building_at = m.place_starting_building(1, "barracks", cell).unwrap();
            let secret = m.next_unit_id;
            let cell = m.world.game.cell_of(170., 170.).unwrap();
            m.place_starting_building(1, "barracks", cell).unwrap();
            m.world.game.units.get_mut(&building).unwrap().built = Some(0.25);
            rebuild(&mut m);
            m.initialize_recipient_routing().unwrap();
            crate::cmb5_match_tests::prepare_finish(&mut m);
            m.advance_tick().unwrap();
            let observed = snapshots(&m);
            assert!(
                observed[0]["units"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r[0] == building)
            );
            assert_eq!(observed[0]["ghosts"], json!([]));
            assert!(
                !observed[0]["units"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r[0] == secret)
            );
            let expected = json!([[building, "barracks", 1, building_at.x, building_at.z, 0.3]]);
            m.world.game.units.get_mut(&scout).unwrap().x = 5.;
            m.world.game.units.get_mut(&scout).unwrap().z = 5.;
            m.world.game.units.get_mut(&building).unwrap().built = Some(0.9);
            rebuild(&mut m);
            for _ in 0..4 {
                m.advance_tick().unwrap();
                snapshots(&m);
                assert_eq!(m.tick_state.fault(), None);
            }
            let check = |m: &Match, cache: &SnapshotCache<'_>| {
                for (slot, team) in teams.iter().enumerate() {
                    let snap = cache.recipient(slot).unwrap();
                    let ghosts = &snap.fields["ghosts"];
                    if *team == 0 {
                        assert_eq!(ghosts, &expected);
                        let units = snap.fields["units"].as_array().unwrap();
                        assert!(!units.iter().any(|r| r[0] == building));
                        assert_eq!(
                            units.iter().any(|r| r[0] == secret),
                            m.finish.is_some(),
                            "the unobserved building appears only after canonical match reveal"
                        );
                    } else {
                        assert_eq!(ghosts, &json!([]));
                    }
                }
                assert_eq!(m.tick_state.fault(), None);
            };
            check(&m, &SnapshotCache::new(&m).unwrap());
            // Retirement happens under fog. No recipient learns the destruction until a revisit.
            m.world.game.units.get_mut(&building).unwrap().hp = 0.;
            crate::cmb5_match_tests::through_finish(&mut m, check);
            assert!(!m.world.game.units.contains_key(&building));
            assert_eq!(snapshots(&m)[0]["ghosts"], expected);
            m.world.game.units.get_mut(&scout).unwrap().x = 70.;
            m.world.game.units.get_mut(&scout).unwrap().z = 70.;
            rebuild(&mut m);
            for _ in 0..4 {
                m.advance_tick().unwrap();
                snapshots(&m);
                assert_eq!(m.tick_state.fault(), None);
            }
            for snap in snapshots(&m) {
                assert_eq!(snap["ghosts"], json!([]));
            }
        }
    }
}
