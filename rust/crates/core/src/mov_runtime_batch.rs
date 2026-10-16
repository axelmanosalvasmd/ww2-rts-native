//! Native runtime checks for movement contact, command boundaries and publication.
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{CELL, Point},
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    math,
    unit_publication::StagedUnit,
    weather::WeatherChoice,
};
use serde_json::json;

fn p(x: f64, z: f64) -> Point {
    Point { x, z }
}
fn scene() -> Match {
    scene_with_naval(false)
}
fn scene_with_naval(naval: bool) -> Match {
    let rows = if naval {
        vec![format!("{}{}", ".".repeat(40), "W".repeat(40)); 80]
    } else {
        vec![".".repeat(80); 80]
    };
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"MOV runtime contact arena", "w":80,"h":80,
            "rows":rows,"naval":naval,
            "spawns":[{"x":8,"y":8},{"x":if naval {8}else{72},"y":72}],
            "points":[{"x":20,"y":70,"vp":0,"mp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
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
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    for c in m.commands.units.values_mut() {
        c.hold_fire = true;
        c.auto_cast = false;
        c.auto_retreat = false;
    }
    for u in m.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    m.start.economy.players[0].mp = 50000.;
    m.world.game.reveal = true;
    m
}
fn add(m: &mut Match, kind: &str, at: Point) -> u32 {
    let id = m.next_unit_id;
    let draft = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        0,
        m.rules.kind(kind).unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[0].spawn,
        Some(at),
    )
    .unwrap();
    m.publish_starting_unit(draft).unwrap();
    let c = m.commands.units.get_mut(&id).unwrap();
    c.hold_fire = true;
    c.auto_cast = false;
    c.auto_retreat = false;
    id
}
fn command(m: &mut Match, id: u32, at: Point, queue: bool) -> Option<Denial> {
    m.execute_command(
        0,
        &Command::from_json(&json!({"t":"move","orders":[[id,at.x,at.z]],"queue":queue})).unwrap(),
        false,
    )
    .unwrap()
}

#[test]
fn mov45_queue_eight_clamp_replacement_and_full_army_selection() {
    let mut m = scene();
    let id = add(&mut m, "rifle", p(51., 51.));
    m.world.game.units.get_mut(&id).unwrap().hold_pos = true;
    assert_eq!(command(&mut m, id, p(-10., 999.), false), None);
    assert_eq!(m.world.game.units[&id].world_goal, Some(p(0., 160.)));
    for i in 0..8 {
        assert_eq!(command(&mut m, id, p(60. + i as f64, 61.), true), None);
    }
    assert_eq!(
        command(&mut m, id, p(70., 70.), true),
        Some(Denial::QueueFull)
    );
    assert_eq!(m.commands.units[&id].orders.len(), 8);
    m.world.game.units.get_mut(&id).unwrap().traffic_wait = 4.;
    assert_eq!(command(&mut m, id, p(70., 60.), false), None);
    assert!(m.commands.units[&id].orders.is_empty());
    assert_eq!(m.world.game.units[&id].traffic_wait, 0.);
    let ids = (0..60)
        .map(|i| add(&mut m, "rifle", p(20. + i as f64 * 1.8, 90.)))
        .collect::<Vec<_>>();
    let orders = ids
        .iter()
        .map(|id| json!([id, 90., 90.]))
        .collect::<Vec<_>>();
    let c = Command::from_json(&json!({"t":"move","orders":orders})).unwrap();
    assert_eq!(m.execute_command(0, &c, false).unwrap(), None);
    assert!(
        ids.iter()
            .all(|id| m.world.game.units[id].world_goal.is_some())
    );
}

#[test]
fn mov68_spawn_scatter_publication_has_monotonic_ids_defaults_and_current_grid() {
    let mut m = scene();
    for kind in ["rifle", "tank", "armoredcar"] {
        let count = m.world.game.units.len();
        let id = m.next_unit_id;
        let spawn = m.start.players[0].spawn;
        let a = count as f64 * 2.4;
        let raw = p(spawn.x + math::cos(a) * 4., spawn.z + math::sin(a) * 4.);
        let draft = StagedUnit::ordinary(
            &m.world.game,
            &m.rules,
            0,
            m.rules.kind(kind).unwrap(),
            id,
            count as u32,
            spawn,
            None,
        )
        .unwrap();
        let cell = crate::unit_spawn::SpawnPlacer::new()
            .nearest_free(
                crate::match_runtime::terrain_view(&m.world.game),
                raw,
                m.rules.kinds[draft.unit.kind].block(),
                Some(spawn),
            )
            .unwrap();
        assert_eq!(
            draft.unit.at(),
            p(
                (cell % 80) as f64 * CELL + 1.,
                (cell / 80) as f64 * CELL + 1.
            )
        );
        m.publish_starting_unit(draft).unwrap();
        assert_eq!(m.next_unit_id, id + 1);
        let u = &m.world.game.units[&id];
        assert!(u.path.is_empty() && !u.riding && u.world_goal.is_none());
        assert!(
            m.combat.get(id).is_some()
                && m.firing.get(id).is_some()
                && m.abilities.get(id).is_some()
        );
        assert!(
            m.grid
                .in_order(&m.world.game.units, (u.x, u.z), 0.1, |_, _| true)
                .contains(&id)
        );
    }
    for y in 0..80 {
        m.world.set_cell(y * 80 + 10, b'W').unwrap();
    }
    let id = m.next_unit_id;
    let draft = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        0,
        m.rules.kind("rifle").unwrap(),
        id,
        0,
        m.start.players[0].spawn,
        None,
    )
    .unwrap();
    assert!(
        draft.unit.x < 20.,
        "scatter stays connected to the spawn even when the nearest cell is across water"
    );
    m.publish_starting_unit(draft).unwrap();
}

#[test]
fn mov69_actual_recruitment_uses_free_ring_exits_and_spawn_ghosts() {
    let mut m = scene();
    let barracks = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "barracks")
        .unwrap()
        .id;
    let mut born = Vec::new();
    for _ in 0..10 {
        let id = m.recorded_buy(0, "rifle", Some(barracks)).unwrap().unit;
        let u = &m.world.game.units[&id];
        assert!(u.ghost >= m.world.game.tick as f64 + 80.);
        assert_eq!(
            m.world.game.flags[m.world.game.cell_of(u.x, u.z).unwrap()]
                & m.rules.kinds[u.kind].block(),
            0
        );
        for old in &born {
            let v = &m.world.game.units[old];
            assert!(
                math::hypot(v.x - u.x, v.z - u.z)
                    > (m.rules.kinds[v.kind].radius + m.rules.kinds[u.kind].radius) * 0.8
            );
        }
        assert!(
            m.grid
                .in_order(&m.world.game.units, (u.x, u.z), 0.1, |_, _| true)
                .contains(&id)
        );
        born.push(id);
    }
    let producer = &m.world.game.units[&barracks];
    assert!(born.iter().any(|id| m.world.game.units[id].z < producer.z));
    assert!(born.iter().any(|id| m.world.game.units[id].z > producer.z));
}

#[test]
fn mov104_naval_purchases_require_sea_launch_from_shipyard_and_respect_destroyer_max() {
    let mut land = scene();
    assert!(matches!(
        land.recorded_buy(0, "lcvp", None),
        Err(crate::match_runtime::RecruitError::Admission(
            crate::production::Error::Denied(crate::production::Denial::Needs)
        ))
    ));
    let mut m = scene_with_naval(true);
    let yard = m.next_unit_id;
    m.place_starting_building(0, "shipyard", 25 * 80 + 32)
        .unwrap();
    for kind in ["lcvp", "gunboat", "destroyer", "destroyer"] {
        let id = m.recorded_buy(0, kind, Some(yard)).unwrap().unit;
        let u = &m.world.game.units[&id];
        let k = &m.rules.kinds[u.kind];
        let c = m.world.game.cell_of(u.x, u.z).unwrap();
        assert!(k.naval);
        assert_eq!(m.world.game.chars[c], b'W');
        assert_eq!(
            m.world.game.flags[c] & k.block(),
            0,
            "{kind} must launch in its navigation depth"
        );
        assert!(
            m.grid
                .in_order(&m.world.game.units, (u.x, u.z), 0.1, |_, _| true)
                .contains(&id)
        );
        assert_eq!(m.recruit_metadata[&id].building, yard);
    }
    assert!(matches!(
        m.recorded_buy(0, "destroyer", Some(yard)),
        Err(crate::match_runtime::RecruitError::Admission(
            crate::production::Error::Denied(crate::production::Denial::Max)
        ))
    ));
}

#[test]
fn mov51_aircraft_purchases_stay_at_maker_with_full_reserves_and_no_rally_walk() {
    let mut m = scene();
    let field = m.next_unit_id;
    m.place_starting_building(0, "airfield", 30 * 80 + 30)
        .unwrap();
    m.player_rallies[0] = Some(p(90., 90.));
    m.building_rallies.insert(field, Some(p(110., 110.)));
    for kind in ["fighter", "attacker"] {
        let before = m.start.economy.players[0].mp;
        let id = m.recorded_buy(0, kind, Some(field)).unwrap().unit;
        let u = &m.world.game.units[&id];
        assert_eq!(u.at(), m.world.game.units[&field].at());
        assert!(u.path.is_empty() && u.world_goal.is_none() && !u.drift);
        assert!(!m.commands.units[&id].rally_drift);
        assert_eq!(u.air, Some(crate::game::PlaneState::Base));
        let air = &m.aviation.planes[&id];
        assert!(air.fuel > 0. && air.ammo > 0. && air.mission.is_none());
        assert!(m.start.economy.players[0].mp < before);
        let pose = u.pose();
        for _ in 0..3 {
            m.advance_tick().unwrap();
        }
        assert_eq!(m.world.game.units[&id].pose(), pose);
    }
}

#[test]
fn mov51_ground_recruit_uses_building_rally_answers_fire_and_resumes() {
    let mut m = scene();
    let barracks = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "barracks")
        .unwrap()
        .id;
    let rally = p(100., 55.);
    m.player_rallies[0] = Some(p(120., 70.));
    m.building_rallies.insert(barracks, Some(rally));
    let id = m.recorded_buy(0, "rifle", Some(barracks)).unwrap().unit;
    assert_eq!(m.world.game.units[&id].world_goal, Some(rally));
    assert!(m.commands.units[&id].rally_drift);
    let at = m.world.game.units[&id].at();
    let enemy = m.next_unit_id;
    let draft = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        1,
        m.rules.kind("rifle").unwrap(),
        enemy,
        m.world.game.units.len() as u32,
        m.start.players[1].spawn,
        Some(p(at.x + 8., at.z)),
    )
    .unwrap();
    m.publish_starting_unit(draft).unwrap();
    m.world.game.players[0].visible.push(enemy);
    let c = m.commands.units.get_mut(&id).unwrap();
    c.hold_fire = false;
    c.auto_retreat = false;
    c.auto_cast = false;
    m.combat.get_mut(id).unwrap().hit_by = Some(enemy);
    m.combat.get_mut(id).unwrap().hit_at = m.world.game.tick as f64;
    m.world.game.units.get_mut(&id).unwrap().hit_from = Some(p(at.x + 8., at.z));
    m.react_unit(id).unwrap();
    assert!(m.world.game.units[&id].path.is_empty());
    assert_eq!(m.firing.get(id).unwrap().target_id, enemy);
    assert_eq!(m.world.game.units[&id].amove, Some(rally));
    m.world.game.units.get_mut(&enemy).unwrap().hp = 0.;
    m.advance_tick().unwrap();
    assert!(
        !m.world.game.units[&id].path.is_empty(),
        "rally travel resumes after the threat is gone"
    );
    let building = &m.world.game.units[&barracks];
    let placed = crate::produced_units::ProducedPlacer::new()
        .produce(
            &m.rules,
            crate::catalog::Catalog::compiled().unwrap(),
            crate::match_runtime::terrain_view(&m.world.game),
            &m.world.game.units,
            crate::produced_units::ProduceInput {
                id: m.next_unit_id,
                owner: 0,
                kind: m.rules.kind("rifle").unwrap(),
                scatter_index: m.world.game.units.len() as u32,
                player_spawn: m.start.players[0].spawn,
                player_rally: Some(rally),
                building: crate::produced_units::ProductionBuilding {
                    at: building.at(),
                    kind: building.kind,
                    rally: None,
                },
                mode: Mode::Classic,
                tick: m.world.game.tick as u64,
            },
        )
        .unwrap();
    assert!(
        placed.rally.is_none() && placed.unit.path.is_empty() && placed.unit.world_goal.is_none(),
        "Classic does not use the player rally walk"
    );
}

#[test]
fn mov45_shift_move_starts_after_first_destination_and_finishes_second() {
    let mut m = scene();
    let id = add(&mut m, "rifle", p(51., 51.));
    m.world.game.units.get_mut(&id).unwrap().hold_pos = true;
    let first = p(56., 51.);
    let second = p(63., 51.);
    assert_eq!(command(&mut m, id, first, false), None);
    assert_eq!(command(&mut m, id, second, true), None);
    let mut second_started = false;
    for _ in 0..400 {
        let before = m.world.game.units[&id].at();
        m.advance_tick().unwrap();
        let u = &m.world.game.units[&id];
        if !second_started && u.world_goal == Some(second) {
            assert!(math::hypot(before.x - first.x, before.z - first.z) <= 0.25);
            second_started = true;
        }
        if second_started && u.world_goal.is_none() {
            assert!(math::hypot(u.x - second.x, u.z - second.z) <= 0.25);
            assert!(m.commands.units[&id].orders.is_empty());
            return;
        }
    }
    panic!("both ordered destinations must complete");
}
