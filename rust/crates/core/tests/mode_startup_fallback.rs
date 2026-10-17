//! MODE-02: createGame's ineligible shared-enemy modes start ordinary Conquest.
use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::ModeKind,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    weather::WeatherChoice,
};

fn source(defend: bool) -> Value {
    let mut source = json!({
        "name":"Mode fallback", "w":80, "h":80,
        "rows":vec![".".repeat(80);80],
        "spawns":[
            {"x":10,"y":10}, {"x":69,"y":69},
            {"x":10,"y":69}, {"x":69,"y":10},
            {"x":40,"y":10}, {"x":40,"y":69}
        ],
        "points":[{"x":30,"y":40},{"x":50,"y":40},{"x":40,"y":30,"mp":0}]
    });
    if defend {
        source["defend"] = json!([0]);
    }
    source
}

fn map(source: &Value) -> AuthoritativeMap {
    AuthoritativeMap::from_json(&serde_json::to_vec(source).unwrap()).unwrap()
}

fn options(mode: Mode, count: usize) -> MatchOptions {
    MatchOptions {
        seed: 0x4d02,
        mode,
        players: (0..count)
            .map(|slot| PlayerConfig {
                name: format!("Commander {slot}"),
                team: (slot as i32 + 2) % 6,
                faction: [Faction::Usa, Faction::Germany, Faction::Ussr][slot % 3],
            })
            .collect(),
        shuffle: true,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: Some(31),
        living_weather: false,
        map_key: Some("fallback-intent".into()),
        tech: true,
        supply: true,
        logistics: false,
        sandbox: false,
    }
}

fn construct(mode: Mode, map: &AuthoritativeMap, options: MatchOptions) -> Result<Match, String> {
    match mode {
        Mode::Horde => Match::new_horde(map, options),
        Mode::Tutorial => Match::new_tutorial(map, options),
        _ => unreachable!(),
    }
}

fn assert_ordinary_start(actual: &Match, expected: &Match) {
    assert_eq!(actual.options.mode, Mode::Conquest);
    assert_eq!(actual.world.game.mode, None);
    assert!(actual.horde.is_none());
    assert!(actual.tutorial_state().is_none());
    assert_eq!(
        serde_json::to_value(&actual.options).unwrap(),
        serde_json::to_value(&expected.options).unwrap()
    );
    let count = expected.options.players.len();
    assert_eq!(actual.start.players.len(), count);
    assert_eq!(actual.world.game.players.len(), count);
    assert_eq!(actual.start.economy, expected.start.economy);
    assert_eq!(actual.start.points, expected.start.points);
    assert_eq!(actual.start.win_vp, expected.start.win_vp);
    for slot in 0..count {
        let player = &actual.world.game.players[slot];
        let configured = &expected.options.players[slot];
        assert_eq!(player.team, configured.team);
        assert_eq!(player.slot, slot);
        assert_eq!(actual.start.players[slot].config.name, configured.name);
        assert_eq!(
            actual.start.players[slot].config.faction,
            configured.faction
        );
        assert_eq!(
            actual.start.players[slot].spawn,
            expected.start.players[slot].spawn
        );
        assert!(actual.world.game.units.values().any(|u| u.owner == slot));
    }
    let force = |m: &Match| {
        m.world
            .game
            .units
            .values()
            .map(|u| (u.id, u.owner, u.kind, u.x, u.z))
            .collect::<Vec<_>>()
    };
    assert_eq!(force(actual), force(expected));
    assert!(actual.world.game.units.values().all(|u| u.owner < count));
}

#[test]
fn mode02_ineligible_horde_and_tutorial_start_conquest_and_deliver_every_seat() {
    // Prototype e4e579a shared/sim.js:686-689. No enemy or special mode is set
    // when the defender list is absent or the six human seats are occupied.
    for (mode, defend, count) in [
        (Mode::Horde, false, 1),
        (Mode::Horde, false, 2),
        (Mode::Horde, false, 5),
        (Mode::Horde, false, 6),
        (Mode::Horde, true, 6),
        (Mode::Tutorial, false, 6),
        (Mode::Tutorial, true, 6),
    ] {
        let map = map(&source(defend));
        let requested = options(mode, count);
        let mut effective = requested.clone();
        effective.mode = Mode::Conquest;
        effective.logistics = false;
        effective.tech = mode != Mode::Tutorial;
        let expected = Match::new(&map, effective).unwrap();
        let mut actual = construct(mode, &map, requested).unwrap();
        assert_ordinary_start(&actual, &expected);
        assert!(!actual.options.logistics);
        assert_eq!(actual.options.tech, mode != Mode::Tutorial);
        // The MP-zero objective survives ordinary startup, unlike the shared modes.
        assert_eq!(actual.start.points.len(), 3);
        let seats = (0..count)
            .map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect::<Vec<_>>();
        let mut delivery = DeliveryState::new(&actual).unwrap();
        for tick in 1..=6 {
            let report = actual.advance_tick().unwrap();
            let frame = delivery
                .capture_selected_with_tick_events(&mut actual, &seats, &report.events)
                .unwrap()
                .unwrap();
            for seat in &seats {
                let snapshot = frame.recipient(seat.slot).unwrap().into_json();
                assert_eq!(snapshot["t"], "s");
                assert_eq!(snapshot["tick"], tick);
                assert!(snapshot.get("mode").is_none());
            }
        }
        assert!(actual.tick_state.fault().is_none());
        assert_eq!(actual.world.game.tick, 6);
    }
}

#[test]
fn mode02_fallback_preserves_options_except_requested_mode_feature_gates() {
    for (mode, defend, count) in [
        (Mode::Horde, false, 2),
        (Mode::Horde, true, 6),
        (Mode::Tutorial, false, 6),
    ] {
        let mut source = source(defend);
        // A Tutorial-specific objective requirement must not apply on fallback.
        source["points"] = json!([{"x":40,"y":40}]);
        let map = map(&source);
        let mut requested = options(mode, count);
        requested.army = ArmySize::Endless;
        requested.logistics = true;
        requested.sandbox = true;
        let mut effective = requested.clone();
        effective.mode = Mode::Conquest;
        // sim.js:729 excludes both requested modes from logistics; :799 excludes
        // Tutorial from tech even when the shared-enemy setup falls back.
        effective.logistics = false;
        effective.tech = mode != Mode::Tutorial;
        let expected = Match::new(&map, effective).unwrap();
        let actual = construct(mode, &map, requested).unwrap();
        assert_ordinary_start(&actual, &expected);
        assert!(!actual.options.logistics);
        assert_eq!(actual.options.tech, mode != Mode::Tutorial);
    }
}

#[test]
fn mode02_eligible_horde_and_tutorial_keep_shared_enemy_startup() {
    let map = map(&source(true));
    for mode in [Mode::Horde, Mode::Tutorial] {
        for count in [1, 5] {
            let actual = construct(mode, &map, options(mode, count)).unwrap();
            assert_eq!(actual.options.mode, mode);
            assert_eq!(actual.options.players.len(), count + 1);
            assert!(actual.options.players[..count].iter().all(|p| p.team == 0));
            assert_eq!(actual.options.players[count].team, 1);
            assert!(!actual.options.supply);
            assert!(!actual.options.logistics);
            match mode {
                Mode::Horde => {
                    assert_eq!(actual.world.game.mode, Some(ModeKind::Horde));
                    let horde = actual.horde.as_ref().unwrap();
                    assert_eq!(horde.slot, count);
                    assert_eq!(horde.defenders, count);
                    assert_eq!(horde.wave, 0);
                    assert_eq!(actual.options.players[count].name, "Horde");
                }
                Mode::Tutorial => {
                    assert_eq!(actual.world.game.mode, Some(ModeKind::Tutorial));
                    assert_eq!(actual.tutorial_state().unwrap().enemy(), count);
                    assert_eq!(actual.options.players[count].name, "Enemy");
                    assert!(!actual.options.tech);
                    assert!(!actual.options.shuffle);
                }
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn mode02_fallback_retains_option_and_mode_guards() {
    for mode in [Mode::Horde, Mode::Tutorial] {
        let map = map(&source(false));
        for count in [0, 7] {
            assert!(construct(mode, &map, options(mode, count)).is_err());
        }
        assert!(construct(mode, &map, options(Mode::Conquest, 6)).is_err());
        let mut invalid_team = options(mode, 6);
        invalid_team.players[0].team = 6;
        assert!(construct(mode, &map, invalid_team).is_err());
        let mut invalid_weather = options(mode, 6);
        invalid_weather.weather = WeatherChoice::Random;
        assert!(construct(mode, &map, invalid_weather).is_err());
    }
    let map = map(&source(true));
    let mut endless_horde = options(Mode::Horde, 2);
    endless_horde.army = ArmySize::Endless;
    let mut endless = Match::new_horde(&map, endless_horde).unwrap();
    assert_eq!(endless.options.mode, Mode::Horde);
    assert_eq!(endless.options.army, ArmySize::Endless);
    assert_eq!(endless.horde.as_ref().unwrap().defenders, 2);
    for _ in 0..4 {
        endless.advance_tick().unwrap();
        let cache = ww2_core::recipient_snapshot::SnapshotCache::new(&endless).unwrap();
        for slot in 0..endless.start.players.len() {
            cache.recipient(slot).unwrap();
        }
        assert!(endless.tick_state.fault().is_none());
    }
}

#[test]
fn mode02_short_spawn_maps_do_not_open_world_startup() {
    let mut source = source(false);
    source["spawns"] = json!([{"x":2,"y":2}]);
    // Ordinary AuthoritativeMap rejects the prototype's short-spawn input.
    assert!(AuthoritativeMap::from_json(&serde_json::to_vec(&source).unwrap()).is_err());
    let regions = (0..64)
        .map(|id| {
            let mut region = json!({
                "id":id,"x":2+id%8*8,"y":2+id/8*8,"bounds":[0,0,80,80]
            });
            if id == 0 {
                region["home"] = json!(0);
            }
            region
        })
        .collect::<Vec<_>>();
    source["world"] = json!({
        "size":"huge","total":64,"regionCells":64,"regions":regions
    });
    source["points"] = json!([]);
    let world = map(&source);
    assert_eq!(world.spawns().len(), 1);
    assert!(world.geography().is_some());
    for mode in [Mode::Horde, Mode::Tutorial] {
        let error = construct(mode, &world, options(mode, 1)).err().unwrap();
        assert!(error.contains("World"), "{error}");
    }
}
