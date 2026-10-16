//! MOV-90: aircraft bypass ground hazards while remaining population-bearing units.
use serde_json::json;
use ww2_core::{
    economy::{self, Faction, Mode},
    economy_state::{self, ArmySize},
    environment::Context,
    game::{PlaneState, Point},
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_aviation::{MissionKind, PlaneMission},
    match_combat::{DamageWorld, ExplosionSource},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    separation::separate_crowds,
    terrain,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};

fn point(x: f64, z: f64) -> Point {
    Point { x, z }
}

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Aircraft ground immunity", "w":64, "h":64,
            "rows":vec![".".repeat(64); 64],
            "spawns":[{"x":2,"y":2},{"x":61,"y":61}],
            "points":[{"x":32,"y":32}]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 709,
            mode: Mode::Default,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
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
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}

fn publish(m: &mut Match, name: &str, at: Point) -> u32 {
    let id = m.next_unit_id;
    let staged = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        1,
        m.rules.kind(name).unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[1].spawn,
        Some(at),
    )
    .unwrap();
    prepare_publication(
        PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        2,
        vec![staged],
    )
    .unwrap()
    .publish_with(|u, _| {
        m.combat.initialize(u.id);
        m.firing.initialize(u.id);
        m.abilities.initialize(u.id);
        m.recovery.initialize(u.id);
    });
    id
}

fn place(m: &mut Match, id: u32, at: Point) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = at.x;
    u.z = at.z;
    m.grid.update(id, u);
}

fn population(m: &Match) -> f64 {
    let units: Vec<_> = m
        .world
        .game
        .units
        .values()
        .map(|u| economy_state::Unit {
            id: u.id.into(),
            owner: Some(u.owner),
            kind: &m.rules.kinds[u.kind].name,
            hp: u.hp,
            built: u.built.unwrap_or(0.0),
            queue: &[],
        })
        .collect();
    m.start
        .economy
        .population(
            &economy_state::Context {
                mode: economy_state::ModeContext::default(),
                army: ArmySize::Standard,
                units: &units,
                drops: &[],
                points: &[],
                nodes: &[],
                world: None,
            },
            0,
        )
        .unwrap()
}

#[test]
fn mov90_planes_ignore_ground_hazards_leave_no_wreck_and_use_population() {
    let mut m = scene();
    let before_population = population(&m);
    let plane = m.spawn_aircraft(0, "bomber").unwrap();
    let plane_population = economy::pop_use("bomber").unwrap();
    assert!(plane_population > 0.0);
    assert_eq!(population(&m), before_population + plane_population);
    m.commands.units.get_mut(&plane).unwrap().hold_fire = true;
    place(&mut m, plane, point(41.0, 41.0));
    assert!(
        m.send_aircraft_mission(
            0,
            plane,
            PlaneMission {
                kind: MissionKind::Patrol,
                target: None,
                at: point(101.0, 41.0),
            },
        )
        .unwrap()
    );
    let speed = m.rules.kinds[m.world.game.units[&plane].kind].speed;
    let destination = point(41.0 + speed * 0.5, 41.0);
    let tank = publish(&mut m, "tank", destination);
    let wall = 20 * 64 + 22;
    m.world.set_cell(wall, b'Y').unwrap();
    assert_ne!(
        m.world.game.flags[wall] & m.rules.kinds[m.world.game.units[&tank].kind].block(),
        0
    );
    assert!(!terrain::clear(
        &m.world.game,
        &m.rules,
        (41.0, 41.0),
        (destination.x, destination.z),
        m.rules.kinds[m.world.game.units[&tank].kind].block(),
        true,
    ));
    m.step_aircraft(plane, 0.5).unwrap();
    assert!((m.world.game.units[&plane].x - destination.x).abs() < 1e-9);
    assert_eq!(m.world.game.units[&plane].z, destination.z);
    assert_eq!(m.world.game.units[&tank].at(), destination);
    m.world.set_cell(wall, b'.').unwrap();

    // Real overlapping ground squads move apart, while the plane and tank do not push each other.
    let first = publish(&mut m, "rifle", point(81.0, 81.0));
    let second = publish(&mut m, "rifle", point(81.5, 81.0));
    let old_first = m.world.game.units[&first].at();
    let old_second = m.world.game.units[&second].at();
    separate_crowds(&mut m.world.game, &m.rules, &mut m.grid);
    assert_eq!(m.world.game.units[&plane].at(), destination);
    assert_eq!(m.world.game.units[&tank].at(), destination);
    assert!(
        m.world.game.units[&first].at() != old_first
            || m.world.game.units[&second].at() != old_second
    );

    let at = point(61.0, 61.0);
    place(&mut m, plane, at);
    place(&mut m, tank, at);
    let plane_hp = m.world.game.units[&plane].hp;
    let tank_hp = m.world.game.units[&tank].hp;
    let ids: Vec<_> = m.world.game.units.keys().copied().collect();
    let mut damage = DamageWorld {
        world: &mut m.world,
        combat: &mut m.combat,
        grid: &mut m.grid,
        rules: &m.rules,
        logistics: &m.logistics,
        production: &m.production,
        mode: m.options.mode,
        tech_enabled: m.options.tech,
        environment: &mut m.environment,
        environment_seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &ids,
        wind_angle: m.start.wind.0,
        wind_strength: m.start.wind.1,
    };
    damage
        .explosion(
            at,
            4.0,
            ExplosionSource {
                infantry: 20.0,
                vehicle: 20.0,
                terrain: None,
                suppression: 0.0,
                scar: 0.0,
                anti_garrison: None,
                weapon_kind: None,
                off_map_support: false,
            },
            0,
            None,
            None,
            None,
            &mut m.firing,
        )
        .unwrap();
    assert_eq!(m.world.game.units[&plane].hp, plane_hp);
    assert!(m.world.game.units[&tank].hp < tank_hp);

    m.start.economy.players[0].mp = 100_000.0;
    assert_eq!(m.admit_support(0, "strafe", at, Some(0.0)).unwrap(), None);
    let delay = m
        .world
        .game
        .strikes
        .last()
        .unwrap()
        .runtime
        .as_ref()
        .unwrap()
        .delay;
    let tank_hp = m.world.game.units[&tank].hp;
    m.step_support_strikes(&ids, delay + 0.05).unwrap();
    assert!(m.world.game.strikes.is_empty());
    assert!(m.world.game.units[&tank].hp < tank_hp);
    assert_eq!(m.world.game.units[&plane].hp, plane_hp);

    // Both occupants are on the bridge when the environment actually demolishes it.
    let bridge = 30 * 64 + 30;
    m.world.set_cell(bridge, b'=').unwrap();
    assert!(m.world.game.units[&tank].hp > 0.0);
    let grid = SpatialGrid::new(&m.world.game.units, 16.0);
    let mut context = Context {
        tick: m.world.game.tick as u64,
        seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &ids,
        grid: &grid,
        rain: m.world.game.rain,
        wet: m.world.game.wet.unwrap_or(0.0),
        wind_angle: m.start.wind.0,
        wind_strength: m.start.wind.1,
    };
    m.environment
        .wreck_cell(&mut m.world, bridge, Some(b'W'), &mut context)
        .unwrap();
    assert_eq!(m.world.game.units[&tank].hp, 0.0);
    assert_eq!(m.world.game.units[&plane].hp, plane_hp);
    assert_eq!(m.world.game.units[&plane].air, Some(PlaneState::Out));

    // Dry open ground would create a vehicle wreck. A dead plane still consumes population
    // until retirement removes it, then contributes neither population nor a wreck.
    place(&mut m, plane, point(101.0, 81.0));
    place(&mut m, tank, point(101.0, 85.0));
    m.world.game.units.get_mut(&plane).unwrap().hp = 0.0;
    assert_eq!(population(&m), before_population + plane_population);
    let retired = m.retire_dead(&ids).unwrap();
    assert!(retired.retired.iter().any(|u| u.unit.id == plane));
    assert!(!m.world.game.units.contains_key(&plane));
    assert_eq!(population(&m), before_population);
    assert!(m.world.game.wrecks.iter().any(|w| w.id == Some(tank)));
    assert!(m.world.game.wrecks.iter().all(|w| w.id != Some(plane)));
}
