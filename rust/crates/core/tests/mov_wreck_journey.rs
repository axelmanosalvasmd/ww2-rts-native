//! MOV-106: component journey from vehicle death to cover, obstruction and blast removal.
use serde_json::json;
use std::collections::BTreeSet;
use ww2_core::{
    combat::DeathImpulse,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    environment::{Blast, Context, DamageContext, EnvironmentRuntime},
    game::Point,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    movement::move_unit,
    navigation::{TerrainView, Versions},
    pathfinding::{PathRules, PathStatus, PathTerrain, Pathfinding},
    terrain,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
    world::World,
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

fn environment<T>(
    m: &mut Match,
    run: impl FnOnce(&mut EnvironmentRuntime, &mut World, &mut Context<'_>) -> T,
) -> T {
    let ids: Vec<_> = m.world.game.units.keys().copied().collect();
    let mut context = Context {
        tick: m.world.game.tick as u64,
        seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &ids,
        grid: &m.grid,
        rain: m.world.game.rain,
        wet: m.world.game.wet.unwrap_or(0.0),
        wind_angle: m.start.wind.0,
        wind_strength: m.start.wind.1,
    };
    run(&mut m.environment, &mut m.world, &mut context)
}

fn order(m: &mut Match, id: u32, goal: Point) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.path = vec![goal];
    u.path_gen += 1;
    u.world_goal = Some(goal);
    u.repath = 0.0;
}

fn route(m: &Match, id: u32, goal: Point) -> ww2_core::pathfinding::PathResult {
    let g = &m.world.game;
    let u = &g.units[&id];
    let near = vec![0; g.flags.len()];
    let hazards = vec![false; g.flags.len()];
    Pathfinding::new()
        .find_path(
            PathTerrain {
                navigation: TerrainView {
                    width: g.w,
                    height: g.h,
                    flags: &g.flags,
                    heights: None,
                    world_known: true,
                    knowledge_id: 1,
                    versions: Versions::default(),
                    changed: None,
                    changes: &[],
                },
                near_walls: &near,
                known_mines: &hazards,
                fires: &hazards,
                bunkers: &[],
                wear: g.wear.as_deref(),
                wetness: g.wet.unwrap_or(0.0),
                roads: false,
            },
            PathRules::compiled().unwrap(),
            &m.rules.kinds[u.kind],
            u.at(),
            goal,
            None,
        )
        .unwrap()
}

fn travel(m: &mut Match, id: u32, goal: Point) -> bool {
    // Public movement components consume the authored straight causeway route.
    // This does not claim recorded-command or whole-Match scheduling coverage.
    for _ in 0..600 {
        m.world.game.tick += 1;
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.repath -= m.rules.movement.tick;
        move_unit(&mut m.world.game, &m.rules, &mut m.grid, id, 1.0).unwrap();
        let u = &m.world.game.units[&id];
        if (u.x - goal.x).abs() < 1.0 && (u.z - goal.z).abs() < 1.0 {
            return true;
        }
    }
    false
}

#[test]
fn mov106_settled_tank_wreck_covers_infantry_and_blocks_vehicles_until_blast() {
    let mut m = scene();
    // A six-metre causeway crosses water. Constructor units remain at their home rosters.
    for z in 0..64 {
        for x in 20..45 {
            m.world
                .set_cell(z * 64 + x, if (29..=31).contains(&z) { b'.' } else { b'W' })
                .unwrap();
        }
    }
    let casualty = publish(&mut m, "tank", point(65.0, 61.0));
    m.combat.get_mut(casualty).unwrap().death_impulse = Some(DeathImpulse {
        direction: 0.0,
        magnitude: 0.25,
    });
    m.world.game.units.get_mut(&casualty).unwrap().hp = 0.0;
    let ids: Vec<_> = m.world.game.units.keys().copied().collect();
    let retired = m.retire_dead(&ids).unwrap();
    assert!(retired.retired.iter().any(|u| u.unit.id == casualty));
    assert!(!m.world.game.units.contains_key(&casualty));
    assert!(
        m.world
            .game
            .wrecks
            .iter()
            .any(|w| w.id == Some(casualty) && w.moving)
    );
    for _ in 0..60 {
        m.world.game.tick += 1;
        environment(&mut m, |e, w, context| {
            e.step_wrecks(w, 0.05, context).unwrap()
        });
    }
    let wreck = m
        .world
        .game
        .wrecks
        .iter()
        .find(|w| w.id == Some(casualty))
        .unwrap();
    assert!(!wreck.moving);
    let cell = wreck.cell.expect("settled wreck occupies a Q cell");
    let hull = point(wreck.x, wreck.z);
    assert!(ww2_core::math::hypot(hull.x - 65.0, hull.z - 61.0) < 2.0);
    assert!(m.world.burnt[30 * 64 + 32] > 0);
    assert_eq!(m.world.game.physical(cell), b'Q');
    let infantry = publish(&mut m, "rifle", point(hull.x - 2.0, hull.z));
    let protected = m.world.game.units[&infantry].at();
    let shooter = point(hull.x + 20.0, hull.z);
    assert!(terrain::cover_behind(&m.world.game, &m.rules, protected, shooter) > 0.9);
    assert_eq!(
        terrain::cover_behind(
            &m.world.game,
            &m.rules,
            point(hull.x + 2.0, hull.z),
            shooter,
        ),
        0.0,
    );

    let infantry_goal = point(95.0, 61.0);
    let infantry_route = route(&m, infantry, infantry_goal);
    assert_eq!(infantry_route.status, PathStatus::Found);
    order(&mut m, infantry, infantry_goal);
    m.world.game.units.get_mut(&infantry).unwrap().path = infantry_route.waypoints;
    assert!(
        travel(&mut m, infantry, infantry_goal),
        "infantry crosses the settled hull"
    );
    let goal = point(85.0, 61.0);
    let vehicle = publish(&mut m, "tank", point(43.0, 61.0));
    assert_eq!(route(&m, vehicle, goal).status, PathStatus::Unreachable);
    order(&mut m, vehicle, goal);
    assert!(
        !travel(&mut m, vehicle, goal),
        "the hull stops the vehicle journey"
    );
    let stopped = m.world.game.units[&vehicle].at();
    assert!(
        stopped.x > 43.0 && stopped.x < hull.x,
        "vehicle advanced to the obstruction"
    );
    assert_eq!(m.world.game.physical(cell), b'Q');

    let skip = BTreeSet::new();
    environment(&mut m, |e, w, context| {
        e.damage_cells(
            w,
            Blast {
                at: hull,
                radius: 2.0,
                damage: 10_000.0,
                scar: 0.0,
            },
            context,
            &mut DamageContext::plain(&skip),
        )
        .unwrap();
        e.reconcile_wreck_metadata(w);
    });
    assert_ne!(m.world.game.physical(cell), b'Q');
    assert!(m.world.game.wrecks.iter().all(|w| w.id != Some(casualty)));
    assert!(!m.environment.wreck_metadata().contains_key(&casualty));
    assert_eq!(
        terrain::cover_behind(&m.world.game, &m.rules, protected, shooter),
        0.0
    );
    let reopened = route(&m, vehicle, goal);
    assert_eq!(reopened.status, PathStatus::Found);
    order(&mut m, vehicle, goal);
    m.world.game.units.get_mut(&vehicle).unwrap().path = reopened.waypoints;
    assert!(
        travel(&mut m, vehicle, goal),
        "vehicle completes the same journey after blast removal"
    );
}
