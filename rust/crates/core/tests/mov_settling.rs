//! MOV-46 intent checks against sim.js endSpots and test.js Spreading controls.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use ww2_core::{
    catalog::Catalog,
    command_destinations::{self, Context, Request},
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{Game, LOCAL_DEFENDER_OWNER, ModeKind, Player, Point},
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    math,
    orders::OrderKey,
    recipient_snapshot::DeliveryState,
    rules::Rules,
    unit_spawn::initial_unit,
    vision,
    weather::WeatherChoice,
    world::World,
};

fn p(x: f64, z: f64) -> Point {
    Point { x, z }
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn map() -> AuthoritativeMap {
    AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Movement settling", "w":128,"h":96,
            "rows":vec![".".repeat(128);96],
            "spawns":[{"x":8,"y":8},{"x":119,"y":87}],
            "points":[{"id":"centre","x":64,"y":48}]
        }))
        .unwrap(),
    )
    .unwrap()
}

struct Scene {
    game: Game,
    rules: Rules,
    grid: SpatialGrid,
    points: Vec<Point>,
}
impl Scene {
    fn new(kind: &str, count: u32) -> Self {
        let mut game = World::from_map(&map()).unwrap().game;
        game.players = vec![
            Player {
                slot: 0,
                team: 0,
                visible: vec![],
            },
            Player {
                slot: 1,
                team: 0,
                visible: vec![],
            },
            Player {
                slot: 2,
                team: 1,
                visible: vec![],
            },
        ];
        let rules = Rules::compiled().unwrap();
        for id in 1..=count {
            let unit = initial_unit(
                &rules,
                Catalog::compiled().unwrap(),
                rules.kind(kind).unwrap(),
                id,
                0,
                p(30., 60. + (f64::from(id) - f64::from(count + 1) / 2.) * 5.),
            )
            .unwrap();
            game.units.insert(id, unit);
        }
        let grid = SpatialGrid::new(&game.units, 16.);
        Self {
            game,
            rules,
            grid,
            points: vec![],
        }
    }
    fn ends(&self, requests: &[Request]) -> Vec<Request> {
        command_destinations::end_spots(
            &Context {
                game: &self.game,
                rules: &self.rules,
                grid: &self.grid,
                points: &self.points,
                hit_at: &BTreeMap::new(),
            },
            requests,
            None,
        )
        .unwrap()
    }
    fn orders(&self, at: Point) -> Vec<Request> {
        self.game
            .units
            .keys()
            .map(|&id| Request { id, at })
            .collect()
    }
    fn rebuild(&mut self) {
        self.grid.rebuild(&self.game.units);
    }
}

#[test]
fn mov46_rounded_clicks_form_five_metre_rows_and_capture_spots_stay_inside() {
    let mut scene = Scene::new("rifle", 6);
    let requests: Vec<_> = (1..=6)
        .map(|id| Request {
            id,
            at: p(100.1 + f64::from(id % 2) * 0.1, 60.1),
        })
        .collect();
    let ends = scene.ends(&requests);
    for (i, a) in ends.iter().enumerate() {
        assert!(distance(a.at, p(100.1, 60.1)) < 12.);
        for b in &ends[i + 1..] {
            assert!(distance(a.at, b.at) >= 3.);
        }
    }
    // Holds bypass cover/spacing, exposing the formation's three columns and two rows.
    for u in scene.game.units.values_mut() {
        u.hold_pos = true;
    }
    let ends = scene.ends(&scene.orders(p(100., 60.)));
    for (index, end) in ends.iter().enumerate() {
        assert_eq!(
            end.at,
            p(
                100. - (index / 3) as f64 * 5.,
                55. + (index % 3) as f64 * 5.
            )
        );
    }
    let mut scene = Scene::new("rifle", 25);
    let centre = p(128., 96.);
    scene.points.push(centre);
    let radius = Catalog::compiled()
        .unwrap()
        .table("CFG")
        .unwrap()
        .get("pointRadius")
        .unwrap()
        .number()
        .unwrap();
    for u in scene.game.units.values_mut() {
        u.hold_pos = true;
    }
    let ends = scene.ends(&scene.orders(centre));
    assert!(
        ends.iter()
            .all(|r| distance(r.at, centre) <= radius - 1.5 + 1e-9)
    );
    assert!(ends.iter().any(|r| r.at.x < centre.x));
    assert!(ends.iter().any(|r| r.at.x > centre.x));

    // The ordinary capture case also runs cover seeking and crowding after formation.
    let mut scene = Scene::new("rifle", 9);
    scene.points.push(centre);
    let ends = scene.ends(&scene.orders(centre));
    for (index, end) in ends.iter().enumerate() {
        assert!(distance(end.at, centre) <= radius);
        assert!(
            ends[index + 1..]
                .iter()
                .all(|other| distance(end.at, other.at) >= 3.)
        );
    }
}

#[test]
fn mov46_river_and_cliff_spots_fall_back_to_the_clicks_near_side() {
    for cliff in [false, true] {
        let mut scene = Scene::new("rifle", 9);
        for (id, u) in &mut scene.game.units {
            u.x = 5.3;
            u.z = 10.3 + f64::from(*id - 1) * 3.;
        }
        if cliff {
            scene.game.height = Some(
                (0..scene.game.w * scene.game.h)
                    .map(|c| if c / scene.game.w >= 20 { 2 } else { 0 })
                    .collect(),
            );
        } else {
            for x in 0..scene.game.w {
                scene.game.flags[20 * scene.game.w + x] |= ww2_core::game::MOVE;
            }
        }
        scene.rebuild();
        let click = p(60.3, 37.3);
        let requests = scene.orders(click);
        let ends = scene.ends(&requests);
        assert!(ends.iter().all(|r| r.at.z < 40.), "cliff={cliff}: {ends:?}");
        assert!(ends.iter().all(|r| distance(r.at, click) < 12.));
        assert!(
            ends.iter()
                .enumerate()
                .all(|(i, r)| ww2_core::terrain::clear(
                    &scene.game,
                    &scene.rules,
                    (click.x, click.z),
                    (r.at.x, r.at.z),
                    ww2_core::game::MOVE | ww2_core::game::WIRE,
                    true
                ) && ww2_core::terrain::segment_cliff_clear(
                    &scene.game,
                    click,
                    r.at
                ) && r.id == requests[i].id)
        );
    }
}

#[test]
fn mov46_infantry_seeks_nearby_cover_and_hold_position_keeps_exact_click() {
    let mut scene = Scene::new("rifle", 1);
    let click = p(99.9, 60.);
    let cover = p(101., 61.);
    let cell = scene.game.cell_of(cover.x, cover.z).unwrap();
    scene.game.flags[cell] |= ww2_core::game::TRENCH;
    let ends = scene.ends(&scene.orders(click));
    assert_eq!(ends[0].at, cover);
    assert!(distance(ends[0].at, click) <= 4.);
    scene.game.units.get_mut(&1).unwrap().hold_pos = true;
    assert_eq!(scene.ends(&scene.orders(click))[0].at, click);
    // Cover beyond the search radius must not change the click.
    scene.game.units.get_mut(&1).unwrap().hold_pos = false;
    scene.game.flags[cell] &= !ww2_core::game::TRENCH;
    let far = scene.game.cell_of(107., 61.).unwrap();
    scene.game.flags[far] |= ww2_core::game::TRENCH;
    assert_eq!(scene.ends(&scene.orders(click))[0].at, click);
}

#[test]
fn mov46_infantry_and_vehicle_spacing_reserve_allied_current_and_route_end_spots() {
    for (kind, space) in [("rifle", 3.), ("tank", 5.)] {
        for heading in [false, true] {
            let mut scene = Scene::new(kind, 2);
            let click = p(100., 60.);
            let held = scene.game.units.get_mut(&2).unwrap();
            held.owner = 1;
            if heading {
                held.path = vec![click];
            } else {
                held.x = click.x;
                held.z = click.z;
            }
            scene.rebuild();
            let ends = scene.ends(&[Request { id: 1, at: click }]);
            assert!(
                distance(ends[0].at, click) >= space,
                "{kind}, heading={heading}: {ends:?}"
            );
            scene.game.units.get_mut(&2).unwrap().owner = 2;
            scene.rebuild();
            assert_eq!(scene.ends(&[Request { id: 1, at: click }])[0].at, click);
        }
    }
}

#[test]
fn mov46_world_any_unseen_click_bypasses_every_group_and_uses_current_team_sight() {
    let mut scene = Scene::new("rifle", 3);
    scene.game.mode = Some(ModeKind::World);
    let seen = p(40., 60.);
    let unseen = p(220., 160.);
    let requests = vec![
        Request { id: 1, at: seen },
        Request { id: 2, at: seen },
        Request { id: 3, at: unseen },
    ];
    assert!(vision::team_sees(
        &scene.game,
        &scene.rules,
        &scene.grid,
        0,
        seen
    ));
    assert!(!vision::team_sees(
        &scene.game,
        &scene.rules,
        &scene.grid,
        0,
        unseen
    ));
    assert_eq!(scene.ends(&requests), requests);
    // Hidden cover and end-of-match reveal do not replace current physical point sight.
    let cell = scene.game.cell_of(unseen.x, unseen.z).unwrap();
    scene.game.flags[cell] |= ww2_core::game::TRENCH;
    scene.game.reveal = true;
    assert_eq!(scene.ends(&requests), requests);
    // An ally observing the distant point makes settling eligible for the whole command.
    let mut eye = scene.game.units[&3].clone();
    eye.id = 4;
    eye.owner = 1;
    eye.x = unseen.x - 10.;
    eye.z = unseen.z;
    scene.game.units.insert(4, eye);
    scene.rebuild();
    assert!(vision::team_sees(
        &scene.game,
        &scene.rules,
        &scene.grid,
        0,
        unseen
    ));
    assert_ne!(scene.ends(&requests), requests);
    scene.game.units.get_mut(&4).unwrap().owner = 2;
    scene.rebuild();
    assert_eq!(scene.ends(&requests), requests);
    // World locals have a canonical team without occupying a commander slot.
    scene.game.units.get_mut(&3).unwrap().owner = LOCAL_DEFENDER_OWNER;
    assert_eq!(scene.ends(&requests), requests);
}

fn r1() -> Match {
    Match::new(
        &map(),
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
            shuffle: true,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: true,
            map_key: None,
            tech: true,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}

#[test]
fn mov46_r1_recorded_moves_keep_ticking_and_delivering_both_seat_snapshots() {
    let mut state = r1();
    let selected: Vec<_> = state
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner == 0 && state.rules.kinds[u.kind].infantry)
        .map(|u| u.id)
        .collect();
    assert!(selected.len() >= 2);
    let mut delivery = DeliveryState::new(&state).unwrap();
    // Shuffle may assign either spawn. Keep the route within the fixed tick budget.
    let spawn = state.start.players[0].spawn;
    let click = p(
        spawn.x + if spawn.x < 128. { 30. } else { -30. },
        spawn.z + if spawn.z < 96. { 12. } else { -12. },
    );
    let orders: Vec<Value> = selected
        .iter()
        .map(|id| json!([id, click.x, click.z]))
        .collect();
    state
        .enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 0,
            },
            Command::from_json(&json!({"t":"move","orders":orders})).unwrap(),
        )
        .unwrap();
    let mut moved = false;
    let original = state.world.game.units[&selected[0]].at();
    for tick in 1..=600 {
        let report = state
            .advance_tick()
            .unwrap_or_else(|e| panic!("tick {tick}: {e}"));
        assert_eq!(report.tick, tick);
        if tick == 1 {
            assert_eq!(report.commands.len(), 1);
            assert_eq!(report.commands[0].denial, None);
        }
        assert!(
            report.events.is_empty(),
            "quiet route should stay away from the enemy"
        );
        let frame = delivery
            .capture_with_lists(&mut state, &[tick == 1; 2], &[false; 2], &[])
            .unwrap();
        let snapshots: Vec<_> = (0..2)
            .map(|owner| frame.recipient(owner).unwrap().into_json())
            .collect();
        drop(frame);
        for (owner, snapshot) in snapshots.iter().enumerate() {
            assert_eq!(snapshot["tick"], tick);
            assert!(snapshot["plans"].as_array().unwrap().iter().all(|row| {
                state.world.game.units[&(row[0].as_u64().unwrap() as u32)].owner == owner
            }));
        }
        moved |= distance(state.world.game.units[&selected[0]].at(), original) > 1.;
    }
    assert!(moved);
    assert!(state.tick_state.fault().is_none());
    for id in selected {
        let unit = &state.world.game.units[&id];
        assert!(
            distance(unit.at(), state.commands.units[&id].route_end.unwrap()) < 3.,
            "id={id} kind={} at={:?} end={:?} path={:?} goal={:?} outcome={:?} drift={} stuck={}",
            state.rules.kinds[unit.kind].name,
            unit.at(),
            state.commands.units[&id].route_end,
            unit.path,
            unit.world_goal,
            state.commands.units[&id].outcome,
            unit.drift,
            unit.stuck
        );
    }
}
