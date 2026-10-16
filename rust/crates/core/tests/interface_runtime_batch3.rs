use serde_json::{Value, json};
use ww2_core::{
    client_view::ClientView,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{Point, Wreck},
    map::AuthoritativeMap,
    match_commands::{Command, Stance},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    orders::OrderKey,
    random::{MatchRandom, StreamId},
    recipient_snapshot::{DeliveryState, SnapshotCache},
    route_knowledge::{
        CellMetadata, CoverBody, CoverClass, InitialTerrain, KnowledgeError, RememberedCell,
        RouteKnowledge,
    },
    rules::Rules,
    weather::WeatherChoice,
};

fn terrain(rows: &[&str]) -> RouteKnowledge {
    let chars: Vec<_> = rows.iter().flat_map(|r| r.bytes()).collect();
    let houses = vec![0; chars.len()];
    RouteKnowledge::observed(
        InitialTerrain {
            width: rows[0].len(),
            height: rows.len(),
            chars: &chars,
            ground: None,
            objects: None,
            heights: None,
            shoals: None,
            house_kinds: Some(&houses),
            toughness: None,
            trench_front: None,
        },
        1,
        Rules::compiled().unwrap(),
    )
    .unwrap()
}

#[test]
fn if07_cover_classes_house_edges_and_exploration_match_prototype_rules() {
    use CoverClass::{Heavy, Light, None, Open};
    // Expected classes come from client/cover-preview.js cellCover and houseEdge.
    let rows = [
        "............",
        ".BBB........",
        ".BBB........",
        ".BBB........",
        "............",
        ".T+ORH#W....",
    ];
    let knowledge = terrain(&rows);
    let explored = vec![true; 72];
    for (x, expected) in [
        (0, Open),
        (1, Heavy),
        (2, Light),
        (3, Light),
        (4, Light),
        (5, Light),
        (6, Light),
        (7, None),
    ] {
        let at = Point {
            x: x as f64 * 2.0 + 1.0,
            z: 11.0,
        };
        assert_eq!(
            knowledge
                .cover_preview(at, &explored, true, Option::None, &[], &[])
                .unwrap()
                .class,
            expected,
            "terrain {}",
            char::from(rows[5].as_bytes()[x])
        );
    }
    for y in 1..=3 {
        for x in 1..=3 {
            let at = Point {
                x: x as f64 * 2.0 + 1.0,
                z: y as f64 * 2.0 + 1.0,
            };
            let expected = if x == 2 && y == 2 { None } else { Heavy };
            assert_eq!(
                knowledge
                    .cover_preview(at, &explored, true, Option::None, &[], &[])
                    .unwrap()
                    .class,
                expected
            );
            assert_eq!(
                knowledge
                    .cover_preview(at, &explored, false, Option::None, &[], &[])
                    .unwrap()
                    .class,
                None
            );
        }
    }
    let at = Point { x: 1.0, z: 1.0 };
    assert_eq!(
        knowledge
            .cover_preview(at, &[false; 72], true, Option::None, &[], &[])
            .unwrap()
            .class,
        None
    );
    let unknown = RouteKnowledge::world(12, 6, 2, Rules::compiled().unwrap()).unwrap();
    assert_eq!(
        unknown
            .cover_preview(at, &explored, true, Option::None, &[], &[])
            .unwrap()
            .class,
        None
    );
    assert_eq!(
        knowledge.cover_preview(at, &[], true, Option::None, &[], &[]),
        Err(KnowledgeError::InvalidShape)
    );
}

#[test]
fn if07_directional_cover_uses_delivered_damage_hulls_and_settled_wrecks() {
    let mut knowledge = terrain(&["........", "....H...", "........"]);
    let explored = vec![true; 24];
    let at = Point { x: 5.5, z: 3.0 };
    let east = Some(Point { x: 25.5, z: 3.0 });
    let west = Some(Point { x: -14.5, z: 3.0 });
    let query = |k: &RouteKnowledge, shooter, bodies: &[CoverBody], wrecks: &[Wreck]| {
        k.cover_preview(at, &explored, true, shooter, bodies, wrecks)
            .unwrap()
            .directional
    };
    assert!(query(&knowledge, east, &[], &[]));
    assert!(!query(&knowledge, west, &[], &[]));
    assert!(!query(&knowledge, None, &[], &[]));
    knowledge
        .apply_rows(
            1,
            &[RememberedCell {
                cell: 12,
                revision: 1,
                character: b'H',
                height: 0,
                state: 2 << 3,
                metadata: CellMetadata::default(),
                cover_basis: None,
            }],
        )
        .unwrap();
    assert!(
        !query(&knowledge, east, &[], &[]),
        "nearly destroyed distant hedge no longer covers"
    );
    let hull = CoverBody {
        at: Point { x: 7.5, z: 3.0 },
        hp: 100.0,
        infantry: false,
        air: false,
    };
    assert!(query(&knowledge, east, &[hull], &[]));
    for rejected in [
        CoverBody { hp: 0.0, ..hull },
        CoverBody {
            infantry: true,
            ..hull
        },
        CoverBody { air: true, ..hull },
        CoverBody {
            at: Point { x: 5.5, z: 5.0 },
            ..hull
        },
        CoverBody {
            at: Point { x: 9.0, z: 3.0 },
            ..hull
        },
    ] {
        assert!(!query(&knowledge, east, &[rejected], &[]));
    }
    let wreck = Wreck {
        id: Some(99),
        cell: None,
        x: 7.5,
        z: 3.0,
        moving: false,
    };
    assert!(query(&knowledge, east, &[], std::slice::from_ref(&wreck)));
    assert!(!query(
        &knowledge,
        east,
        &[],
        &[Wreck {
            moving: true,
            ..wreck
        }]
    ));
    let corner = terrain(&["........", "....B...", "........"]);
    assert!(query(&corner, east, &[], &[]));
    assert!(!query(&corner, west, &[], &[]));
}

pub(super) fn make(seed: u64) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Interface runtime", "w":40, "h":30,
            "rows":vec!["........................................"; 30],
            "spawns":[{"x":3,"y":3},{"x":35,"y":25}],
            "points":[{"id":"front","x":10,"y":15}]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed,
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
            weather_seed: None,
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

fn snapshot(state: &mut Match, delivery: &mut DeliveryState) -> Value {
    delivery
        .capture_quiescent(state, &[true, true], &[false, false], &[])
        .unwrap()
        .recipient(0)
        .unwrap()
        .into_json()
}

#[test]
fn if07_preview_survives_ordinary_ticks_and_complete_recipient_snapshots() {
    let mut state = make(701);
    let mut delivery = DeliveryState::new(&state).unwrap();
    let knowledge = terrain(&["........", "....H...", "........"]);
    for tick in 1..=8 {
        state.advance_tick().unwrap();
        let before = snapshot(&mut state, &mut delivery);
        let result = knowledge
            .cover_preview(
                Point { x: 5.5, z: 3.0 },
                &[true; 24],
                true,
                Some(Point { x: 25.5, z: 3.0 }),
                &[],
                &[],
            )
            .unwrap();
        assert!(result.directional);
        assert_eq!(before, snapshot(&mut state, &mut delivery));
        assert_eq!(state.world.game.tick, tick);
        assert!(state.tick_state.fault().is_none());
    }
}

#[test]
fn if14_recipient_view_cannot_recover_undelivered_enemy_rows() {
    let mut state = make(701);
    state.advance_tick().unwrap();
    let hidden: Vec<_> = state
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner == 1 && !state.world.game.players[0].sees(u.id))
        .map(|u| u.id)
        .collect();
    assert!(!hidden.is_empty());
    let mut view = ClientView::new();
    view.apply(&json!({"t":"start","map":{"w":40,"h":30},"fog":{"v":"","e":""}}).to_string())
        .unwrap();
    for variant in 0..32 {
        for id in &hidden {
            let u = state.world.game.units.get_mut(id).unwrap();
            u.x = 65.0 + variant as f64 / 32.0;
            u.z = 50.0;
            u.hp = 100.0 + variant as f64;
        }
        let mut projected = SnapshotCache::new(&state)
            .unwrap()
            .recipient(0)
            .unwrap()
            .fields;
        projected.insert("t".into(), json!("s"));
        view.apply(&Value::Object(projected).to_string()).unwrap();
        for id in &hidden {
            assert!(view.unit(*id).is_none());
        }
        assert!(view.units().all(|u| u.owner == 0));
    }
    // Replacing a delivered set and starting a fresh match also discard old rows.
    view.apply(&json!({"t":"s","units":[]}).to_string())
        .unwrap();
    assert_eq!(view.unit_count(), 0);
    view.apply(&json!({"t":"start","map":{"w":1,"h":1}}).to_string())
        .unwrap();
    assert_eq!(view.unit_count(), 0);
}

fn assert_units(left: &Match, right: &Match) {
    assert_eq!(left.world.game.units.len(), right.world.game.units.len());
    for (id, expected) in &left.world.game.units {
        let actual = &right.world.game.units[id];
        // An unset pace is an intentional NaN, so compare its bits separately.
        assert_eq!(expected.pace.to_bits(), actual.pace.to_bits());
        let mut expected = expected.clone();
        let mut actual = actual.clone();
        expected.pace = 0.0;
        actual.pace = 0.0;
        assert_eq!(expected, actual, "unit {id}");
    }
}

#[test]
fn if20_match_streams_replay_across_rooms_and_unrelated_ai_draws() {
    let mut control = make(0x3c1055);
    let mut replay = make(0x3c1055);
    let mut other_room = make(987);
    assert_units(&control, &replay);
    assert_ne!(
        control.start.random.snapshot(),
        other_room.start.random.snapshot()
    );
    let rifle = control.rules.kind("rifle").unwrap();
    // Put an ordinary live rifle from each army in direct-fire range.
    for state in [&mut control, &mut replay] {
        for owner in 0..2 {
            let u = state
                .world
                .game
                .units
                .values_mut()
                .find(|u| u.owner == owner && u.kind == rifle)
                .unwrap();
            u.x = 30.0 + owner as f64 * 10.0;
            u.z = 25.0;
        }
        state.grid.rebuild(&state.world.game.units);
    }
    let mut left_delivery = DeliveryState::new(&control).unwrap();
    let mut right_delivery = DeliveryState::new(&replay).unwrap();
    for _ in 0..40 {
        other_room.advance_tick().unwrap();
        for _ in 0..7 {
            replay.start.random.next_fraction(StreamId::Ai(99));
        }
        control.advance_tick().unwrap();
        replay.advance_tick().unwrap();
        assert_units(&control, &replay);
        assert_eq!(
            control.start.environment_seed,
            replay.start.environment_seed
        );
        assert_eq!(control.start.weather_seed, replay.start.weather_seed);
        assert!(control.tick_state.fault().is_none() && replay.tick_state.fault().is_none());
    }
    // Persistent flights remain outside complete snapshot admission. Stop firing
    // through the recorded boundary and let the actual flight phase settle them.
    for state in [&mut control, &mut replay] {
        for owner in 0..2 {
            let ids = state.world.game.units.values().filter(|u| u.owner == owner).map(|u| i64::from(u.id)).collect();
            state.enqueue_command(OrderKey { tick: 41, issuer: owner as u32, sequence: 1 }, Command::Stance { ids, stance: Stance::HoldFire, on: true }).unwrap();
        }
    }
    for _ in 0..8 {
        let a = control.advance_tick().unwrap();
        let b = replay.advance_tick().unwrap();
        assert_eq!(a.commands, b.commands);
        assert!(a.commands.iter().all(|r| r.denial.is_none()));
        assert_units(&control, &replay);
    }
    assert!(control.firing.flights.is_empty() && replay.firing.flights.is_empty());
    assert_eq!(snapshot(&mut control, &mut left_delivery), snapshot(&mut replay, &mut right_delivery));
    assert!(control.tick_state.fault().is_none() && replay.tick_state.fault().is_none());
    let left = control.start.random.snapshot();
    let mut right = replay.start.random.snapshot();
    assert!(
        left.streams
            .iter()
            .any(|(purpose, _)| *purpose == StreamId::Combat),
        "ordinary combat must actually draw"
    );
    right
        .streams
        .retain(|(purpose, _)| *purpose != StreamId::Ai(99));
    assert_eq!(left, right);
    let mut restored = MatchRandom::restore(replay.start.random.snapshot()).unwrap();
    for purpose in [
        StreamId::Terrain,
        StreamId::Weather,
        StreamId::Combat,
        StreamId::OffMap,
        StreamId::Ai(0),
        StreamId::Ai(1),
    ] {
        for _ in 0..64 {
            assert_eq!(
                restored.next_u64(purpose),
                replay.start.random.next_u64(purpose)
            );
        }
    }
}
