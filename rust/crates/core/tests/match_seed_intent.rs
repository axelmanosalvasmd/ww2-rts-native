use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, CommandResult, MoveOrder, MoveOutcome},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    orders::OrderKey,
    weather::WeatherChoice,
};

// Ordered (id, kind, owner, x, z, hp), with every floating-point bit preserved.
fn units(m: &Match) -> Vec<(u32, usize, usize, u64, u64, u64)> {
    m.world
        .game
        .units
        .values()
        .map(|u| {
            (
                u.id,
                u.kind,
                u.owner,
                u.x.to_bits(),
                u.z.to_bits(),
                u.hp.to_bits(),
            )
        })
        .collect()
}

fn assert_same_seed_state(a: &Match, b: &Match) {
    assert_eq!(units(a), units(b));
    assert_eq!(a.start.random.snapshot(), b.start.random.snapshot());
    assert_eq!(a.start.weather_seed, b.start.weather_seed);
    assert_eq!(a.start.weather, b.start.weather);
    let environment = |m: &Match| {
        (
            m.start.environment_seed,
            (m.start.wind.0.to_bits(), m.start.wind.1.to_bits()),
            m.start.living.map(|l| (l.raining, l.next.to_bits())),
            m.world.game.weather,
            m.world.game.rain.to_bits(),
            m.world.game.wet.map(f64::to_bits),
        )
    };
    assert_eq!(environment(a), environment(b));
}

#[test]
fn if21_same_seed_replays_constructed_units_and_recorded_commands() {
    // Source test.js:4828-4832 compares seeded playMatch unit projections.
    // Native purpose streams need not reproduce Source's global random draws.
    let authored = serde_json::json!({
        "name": "Seed intent open field",
        "w": 64,
        "h": 40,
        "rows": vec![".".repeat(64); 40],
        "spawns": [{"x": 8, "y": 10}, {"x": 48, "y": 30}],
        "points": [{"x": 32, "y": 20, "vp": 0, "mp": 0}]
    });
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&authored).unwrap()).unwrap();
    let options = MatchOptions {
        seed: 5,
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
        weather: WeatherChoice::Random,
        weather_seed: None,
        living_weather: true,
        map_key: None,
        tech: true,
        supply: true,
        logistics: false,
        sandbox: false,
    };
    let mut first = Match::new(&map, options.clone()).unwrap();
    let mut repeated = Match::new(&map, options.clone()).unwrap();
    let mut different = Match::new(&map, MatchOptions { seed: 7, ..options }).unwrap();
    assert_same_seed_state(&first, &repeated);
    assert!(first.start.living.is_some());
    let initial = units(&first);
    assert!(!initial.is_empty());
    assert_ne!(
        initial,
        units(&different),
        "different seed must change units"
    );
    let environment_seed = first.start.environment_seed;
    let mover = first
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && first.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let initial_at = first.world.game.units[&mover].at();
    let inputs = [
        (
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 1,
            },
            Command::Move {
                orders: vec![MoveOrder {
                    unit: i64::from(mover),
                    at: Point { x: 256.0, z: 160.0 },
                }],
                control_ids: None,
                attack_move: false,
                queue: false,
                together: false,
                face: None,
            },
        ),
        (
            OrderKey {
                tick: 25,
                issuer: 0,
                sequence: 2,
            },
            Command::Stop {
                ids: vec![i64::from(mover)],
            },
        ),
    ];
    for m in [&mut first, &mut repeated, &mut different] {
        m.validate_tick_state().expect("admitted ordinary match");
        for (key, command) in &inputs {
            m.enqueue_command(*key, command.clone()).unwrap();
        }
        assert_eq!(m.commands.pending_commands(), inputs.len());
    }
    for tick in 1..=40 {
        let expected: Vec<_> = inputs
            .iter()
            .filter(|(key, _)| key.tick == tick)
            .map(|(key, _)| CommandResult {
                key: *key,
                denial: None,
            })
            .collect();
        for m in [&mut first, &mut repeated, &mut different] {
            let report = m.advance_tick().expect("real seeded match tick");
            assert_eq!(report.tick, tick as i64);
            assert_eq!(m.world.game.tick, tick as i64);
            assert_eq!(report.commands, expected);
            assert_eq!(m.commands.next_tick(), Some(tick + 1));
        }
        assert_same_seed_state(&first, &repeated);
        if tick == 24 {
            assert_ne!(first.world.game.units[&mover].at(), initial_at);
            assert!(!first.world.game.units[&mover].path.is_empty());
        }
        if tick == 25 {
            assert_eq!(
                first.commands.units[&mover].outcome,
                Some(MoveOutcome::Interrupted)
            );
            assert!(first.world.game.units[&mover].path.is_empty());
        }
    }
    assert_eq!(first.commands.pending_commands(), 0);
    assert_eq!(repeated.commands.pending_commands(), 0);
    assert_eq!(different.commands.pending_commands(), 0);
    assert_ne!(first.start.environment_seed, environment_seed);
    assert_ne!(
        units(&first),
        initial,
        "ticks must change the unit projection"
    );
    assert_ne!(
        units(&first),
        units(&different),
        "different seeded match units"
    );
}
