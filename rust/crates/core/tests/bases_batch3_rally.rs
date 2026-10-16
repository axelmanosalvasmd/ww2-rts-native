use serde_json::json;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{CELL, MOVE, Point},
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::TickPathConfig,
    orders::OrderKey,
    recipient_snapshot::DeliveryState,
    weather::WeatherChoice,
};

fn ordinary() -> Match {
    let map = AuthoritativeMap::from_json(include_bytes!(
        "../../server/src/authoritative-maps/el-alamein.json"
    ))
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
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(217),
            living_weather: false,
            map_key: Some("el-alamein".into()),
            tech: false,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    m.initialize_tick_scene(0, TickPathConfig::default())
        .unwrap();
    m
}

fn own(m: &Match, owner: usize, kind: &str) -> u32 {
    m.world
        .game
        .units
        .values()
        .find(|u| u.owner == owner && m.rules.kinds[u.kind].name == kind)
        .unwrap()
        .id
}

fn snapshots(m: &mut Match) {
    m.preflight_ordinary_snapshot().unwrap();
    let mut delivery = DeliveryState::new(m).unwrap();
    let frame = delivery
        .capture_with_lists(m, &[true; 2], &[false; 2], &[])
        .unwrap();
    for owner in 0..2 {
        let body = frame.recipient(owner).unwrap().into_json();
        assert_eq!(body["t"], "s");
        assert!(body["units"].is_array());
        assert!(!serde_json::to_vec(&body).unwrap().is_empty());
    }
}

fn rally_points(m: &Match) -> [Point; 2] {
    let hq = &m.world.game.units[&own(m, 0, "hq")];
    let points = m
        .world
        .game
        .flags
        .iter()
        .enumerate()
        .filter_map(|(cell, flags)| {
            let at = Point {
                x: (cell % m.world.game.w) as f64 * CELL + CELL / 2.,
                z: (cell / m.world.game.w) as f64 * CELL + CELL / 2.,
            };
            let distance = (at.x - hq.x).powi(2) + (at.z - hq.z).powi(2);
            (flags & MOVE == 0 && (144. ..400.).contains(&distance)).then_some(at)
        })
        .collect::<Vec<_>>();
    [points[0], points[points.len() - 1]]
}

#[test]
fn base23_personal_and_building_rallies_route_real_recruits_and_keep_ticks_and_snapshots() {
    let mut m = ordinary();
    let hq = own(&m, 0, "hq");
    let [personal, producer] = rally_points(&m);
    assert_ne!(personal, producer);
    m.enqueue_command(
        OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        },
        Command::from_json(&json!({"t":"rally","x":personal.x,"z":personal.z})).unwrap(),
    )
    .unwrap();
    let report = m.advance_tick().unwrap();
    assert_eq!(report.commands.len(), 1);
    assert_eq!(report.commands[0].denial, None);
    assert_eq!(m.player_rallies[0], Some(personal));
    snapshots(&mut m);
    let first = m.recorded_buy(0, "rifle", Some(hq)).unwrap().unit;
    assert_eq!(m.world.game.units[&first].world_goal, Some(personal));
    assert!(m.world.game.units[&first].drift);
    assert!(!m.world.game.units[&first].path.is_empty());
    snapshots(&mut m);
    m.advance_tick().unwrap();
    snapshots(&mut m);
    // Earn the next purchase on the real clock, keeping the original wallet.
    while m.start.economy.players[0].mp < 100. {
        m.advance_tick().unwrap();
        snapshots(&mut m);
    }
    assert_eq!(
        m.execute_command(
            0,
            &Command::from_json(&json!({
                "t":"rally", "id":hq, "x":producer.x, "z":producer.z
            }))
            .unwrap(),
            false
        )
        .unwrap(),
        None
    );
    assert_eq!(m.player_rallies[0], Some(personal));
    snapshots(&mut m);
    let second = m.recorded_buy(0, "rifle", Some(hq)).unwrap().unit;
    assert_eq!(m.world.game.units[&second].world_goal, Some(producer));
    assert!(m.world.game.units[&second].drift);
    assert!(!m.world.game.units[&second].path.is_empty());
    snapshots(&mut m);
    for _ in 0..5 {
        m.advance_tick().unwrap();
        snapshots(&mut m);
    }
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn base23_denials_scalar_precedence_clamping_and_unfinished_producers() {
    let mut m = ordinary();
    let hq = own(&m, 0, "hq");
    let enemy = own(&m, 1, "hq");
    let rifle = own(&m, 0, "rifle");
    let [at, _] = rally_points(&m);
    let before = m.building_rallies.clone();
    for (ids, scalar, expected) in [
        (vec![-1], None, Denial::Unseen),
        (vec![i64::from(enemy)], None, Denial::Unseen),
        (vec![i64::from(rifle)], None, Denial::Needs),
        (vec![i64::from(hq)], Some(i64::from(enemy)), Denial::Unseen),
    ] {
        assert_eq!(
            m.set_rally(0, &ids, scalar, Some(at)).unwrap(),
            Some(expected)
        );
        assert_eq!(m.building_rallies, before);
        assert_eq!(m.player_rallies[0], None);
    }
    for bad in [
        None,
        Some(Point {
            x: f64::NAN,
            z: at.z,
        }),
        Some(Point {
            x: f64::INFINITY,
            z: at.z,
        }),
        Some(Point { x: -1., z: at.z }),
        Some(Point {
            x: m.world.game.w as f64 * CELL,
            z: at.z,
        }),
    ] {
        assert_eq!(
            m.set_rally(0, &[], None, bad).unwrap(),
            Some(Denial::Blocked)
        );
    }
    let blocked = m
        .world
        .game
        .flags
        .iter()
        .position(|flags| flags & MOVE != 0)
        .unwrap();
    let blocked = Point {
        x: (blocked % m.world.game.w) as f64 * CELL + CELL / 2.,
        z: (blocked / m.world.game.w) as f64 * CELL + CELL / 2.,
    };
    assert_eq!(
        m.set_rally(0, &[], None, Some(blocked)).unwrap(),
        Some(Denial::Blocked)
    );
    let hp = m.world.game.units[&hq].hp;
    m.world.game.units.get_mut(&hq).unwrap().hp = 0.;
    assert_eq!(
        m.set_rally(0, &[i64::from(hq)], None, Some(at)).unwrap(),
        Some(Denial::Needs)
    );
    m.world.game.units.get_mut(&hq).unwrap().hp = hp;
    let built = m.world.game.units[&hq].built;
    m.world.game.units.get_mut(&hq).unwrap().built = Some(0.5);
    assert_eq!(
        m.set_rally(0, &[i64::from(enemy)], Some(i64::from(hq)), Some(at))
            .unwrap(),
        None
    );
    m.world.game.units.get_mut(&hq).unwrap().built = built;
    assert_eq!(m.building_rallies[&hq], Some(at));
    assert_eq!(m.building_rallies[&enemy], before[&enemy]);
    let width = m.world.game.w as f64 * CELL;
    assert_eq!(
        m.set_rally(
            0,
            &[],
            Some(i64::from(hq)),
            Some(Point {
                x: width + 10.,
                z: -10.
            })
        )
        .unwrap(),
        None
    );
    assert_eq!(m.building_rallies[&hq], Some(Point { x: width, z: 0. }));
    snapshots(&mut m);
    m.advance_tick().unwrap();
    snapshots(&mut m);
}

#[test]
fn base23_world_recorded_rally_sets_producer_and_refuses_personal_without_faulting() {
    use ww2_core::random::MatchRandom;
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../simtest/tests/fixtures/world-phases.json"
    ))
    .unwrap();
    let definitions = fixture["definitions"].as_array().unwrap();
    let regions: Vec<_> = definitions
        .iter()
        .enumerate()
        .map(|(i, raw)| {
            let mut region = raw.clone();
            // This authored rural map keeps its real constructor guards. Mortar
            // salvos on city/resource maps require a separate World actuator.
            region["kind"] = json!("rural");
            region["bounds"] = json!([i % 8 * 64, i / 8 * 64, i % 8 * 64 + 64, i / 8 * 64 + 64]);
            if let Some(home) = [0, definitions.len() - 1].iter().position(|&at| at == i) {
                region["home"] = json!(home);
            }
            region
        })
        .collect();
    let height = definitions.len() / 8 * 64;
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"World rally", "w":512, "h":height, "rows":vec![".".repeat(512);height],
            "heights":vec!["0".repeat(512);height],
            "spawns":[definitions[0].clone(), definitions[definitions.len()-1].clone()],
            "points":[],
            "world":{"size":"huge","total":definitions.len(),"regionCells":64,"regions":regions}
        }))
        .unwrap(),
    )
    .unwrap();
    let mut config = ordinary().options;
    config.mode = Mode::World;
    config.map_key = None;
    config.seed = 9182;
    let mut m = Match::new_world(&map, config, MatchRandom::new(9182)).unwrap();
    m.initialize_tick_scene(0, TickPathConfig::default())
        .unwrap();
    let hq = own(&m, 0, "hq");
    let at = m.world.game.units[&hq].at();
    for (sequence, fields) in [
        json!({"t":"rally","x":at.x + 12.,"z":at.z}),
        json!({"t":"rally","id":hq,"x":at.x + 12.,"z":at.z}),
    ]
    .into_iter()
    .enumerate()
    {
        m.enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: sequence as u64,
            },
            Command::from_json(&fields).unwrap(),
        )
        .unwrap();
    }
    let report = m.advance_tick().unwrap();
    assert_eq!(report.commands.len(), 2);
    assert_eq!(report.commands[0].denial, Some(Denial::Unseen));
    assert_eq!(report.commands[1].denial, None);
    assert_eq!(m.player_rallies[0], None);
    assert_eq!(
        m.building_rallies[&hq],
        Some(Point {
            x: at.x + 12.,
            z: at.z
        })
    );
    m.advance_tick().unwrap();
    assert!(m.tick_state.fault().is_none());
}
