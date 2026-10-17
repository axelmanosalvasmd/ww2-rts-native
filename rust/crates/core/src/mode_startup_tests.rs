use super::*;

fn startup_map() -> AuthoritativeMap {
    let mut input = map().source().clone();
    input["spawns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"x":8,"y":112}));
    input["defend"] = json!([0]);
    AuthoritativeMap::from_json(&serde_json::to_vec(&input).unwrap()).unwrap()
}
fn create(mode: Mode, army: ArmySize) -> Result<Match, String> {
    let mut opts = options(mode, true);
    opts.army = army;
    let source = startup_map();
    match mode {
        Mode::Conquest => Match::new(&source, opts),
        Mode::Assault => Match::new_assault(&source, opts, 1),
        Mode::Annihilation => Match::new_annihilation(&source, opts),
        Mode::Classic => Match::new_classic(&source, opts),
        Mode::Horde => Match::new_horde(&source, opts),
        Mode::Tutorial => {
            let tutorial = AuthoritativeMap::from_json(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../server/src/authoritative-maps/tutorial.json"
            )))
            .unwrap();
            Match::new_tutorial(&tutorial, opts)
        }
        _ => unreachable!(),
    }
}
const MODES: [Mode; 6] = [
    Mode::Conquest,
    Mode::Assault,
    Mode::Annihilation,
    Mode::Classic,
    Mode::Horde,
    Mode::Tutorial,
];

#[test]
fn base02_mode06_non_world_constructors_scale_wallets_and_population() {
    for (army, income, population) in [
        (ArmySize::Standard, 1.0, 1.0),
        (ArmySize::Large, 3.0, 2.5),
        (ArmySize::Massive, 6.0, 5.0),
        (ArmySize::Endless, 20.0, 5.0),
    ] {
        for mode in MODES {
            let result = create(mode, army);
            if mode == Mode::Horde && army == ArmySize::Endless {
                assert!(result.err().unwrap().contains("Endless"));
                continue;
            }
            let mut m = result.unwrap();
            let expected = match mode {
                Mode::Conquest => vec![150.0, 150.0],
                Mode::Assault => vec![320.0, 250.0],
                Mode::Annihilation => vec![300.0, 300.0],
                Mode::Classic => vec![200.0, 200.0],
                Mode::Horde => vec![250.0, 250.0, 0.0],
                Mode::Tutorial => vec![150.0, 150.0, 0.0],
                _ => unreachable!(),
            };
            assert_eq!(
                m.start
                    .economy
                    .players
                    .iter()
                    .map(|p| p.mp)
                    .collect::<Vec<_>>(),
                expected.iter().map(|mp| mp * income).collect::<Vec<_>>(),
                "{mode:?} {army:?}"
            );
            let context = crate::economy_state::Context {
                mode: ModeContext {
                    kind: if mode == Mode::Conquest {
                        Mode::Default
                    } else {
                        mode
                    },
                    defender_team: (mode == Mode::Assault).then_some(1),
                    income_excluded_slot: m.horde.as_ref().map(|h| h.slot),
                },
                army,
                units: &[],
                drops: &[],
                points: &[],
                nodes: &[],
                world: None,
            };
            for slot in 0..m.start.players.len() {
                assert_eq!(
                    m.start.economy.population_cap(&context, slot).unwrap(),
                    if mode == Mode::Classic { 24.0 } else { 12.0 } * population,
                    "{mode:?} {army:?} seat {slot}"
                );
            }
            // Horde's full tick remains guarded. Other constructors use the ordinary tick.
            if mode != Mode::Horde {
                m.advance_tick().unwrap();
                assert!(
                    m.start
                        .economy
                        .players
                        .iter()
                        .all(|p| p.mp.is_finite() && p.inc.is_finite())
                );
            }
        }
    }
}

#[test]
fn base52_non_world_starting_bases_and_forces_follow_each_mode() {
    for mode in MODES {
        let m = create(mode, ArmySize::Standard).unwrap();
        for owner in 0..m.start.players.len() {
            let units = m
                .world
                .game
                .units
                .values()
                .filter(|u| u.owner == owner)
                .collect::<Vec<_>>();
            let count = |name: &str| {
                units
                    .iter()
                    .filter(|u| m.rules.kinds[u.kind].name == name)
                    .count()
            };
            let humans = if matches!(mode, Mode::Horde | Mode::Tutorial) {
                2
            } else {
                m.start.players.len()
            };
            assert_eq!(
                count("rifle"),
                if owner >= humans {
                    0
                } else if mode == Mode::Classic {
                    1
                } else {
                    2
                },
                "{mode:?} {owner}"
            );
            assert_eq!(
                count("mg"),
                usize::from(owner < humans && mode != Mode::Classic),
                "{mode:?} {owner}"
            );
            assert_eq!(
                count("engineer"),
                usize::from(mode == Mode::Classic),
                "{mode:?} {owner}"
            );
            let bases = mode != Mode::Tutorial && (mode != Mode::Horde || owner == 0);
            assert_eq!(count("hq"), usize::from(bases), "{mode:?} {owner}");
            assert_eq!(
                count("barracks"),
                usize::from(bases && mode != Mode::Classic),
                "{mode:?} {owner}"
            );
            for u in &units {
                if matches!(m.rules.kinds[u.kind].name.as_str(), "hq" | "barracks") {
                    assert_eq!(u.built, Some(1.0));
                    assert_eq!(u.cells.as_ref().unwrap().len(), 9);
                    if mode == Mode::Classic {
                        assert_eq!(u.at(), m.start.players[owner].spawn);
                    }
                } else if !m.rules.kinds[u.kind].structure {
                    assert_eq!(
                        m.world.game.flags_at(u.x, u.z) & crate::game::MOVE,
                        0,
                        "{mode:?} unit {}",
                        u.id
                    );
                }
            }
            if bases && mode != Mode::Classic {
                let hq = units
                    .iter()
                    .find(|u| m.rules.kinds[u.kind].name == "hq")
                    .unwrap();
                let barracks = units
                    .iter()
                    .find(|u| m.rules.kinds[u.kind].name == "barracks")
                    .unwrap();
                assert!(crate::math::hypot(hq.x - barracks.x, hq.z - barracks.z) >= 12.0);
                if mode == Mode::Conquest {
                    assert_eq!(hq.at(), m.start.players[owner].spawn);
                    assert_eq!(
                        barracks.at(),
                        crate::game::Point {
                            x: hq.x + 12.0,
                            z: hq.z
                        }
                    );
                }
                for building in [hq, barracks] {
                    for other in m
                        .world
                        .game
                        .units
                        .values()
                        .filter(|u| m.rules.kinds[u.kind].structure && u.id != building.id)
                    {
                        let prior_structure = other.id < building.id;
                        if prior_structure && !(other.owner == owner && other.id == hq.id) {
                            assert!(
                                crate::math::hypot(building.x - other.x, building.z - other.z)
                                    >= m.rules.kinds[other.kind].radius + 9.0,
                                "{mode:?} {} near {}",
                                building.id,
                                other.id
                            );
                        }
                    }
                    for point in &m.start.points {
                        assert!(
                            crate::math::hypot(building.x - point.at.x, building.z - point.at.z)
                                >= 13.0
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn base52_cramped_start_clears_fallback_pads_and_relocates_trapped_force() {
    let mut rows = vec!["W".repeat(64); 64];
    for y in [8, 32, 55] {
        rows[y] = ".".repeat(64);
    }
    let source = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"One-cell causeways", "w":64,"h":64,"rows":rows,
            "spawns":[{"x":8,"y":8},{"x":55,"y":55}],"points":[{"x":32,"y":32}]
        }))
        .unwrap(),
    )
    .unwrap();
    let opts = options(Mode::Conquest, true);
    let rules = crate::rules::Rules::compiled().unwrap();
    let plan = crate::match_runtime::StartPlan::prepare(&source, &opts, &rules).unwrap();
    let mut before = crate::world::World::from_map(&source).unwrap();
    plan.spawn_initial_force(&mut before.game, &rules).unwrap();
    // No 3x3 land footprint exists anywhere, so construction must clear its fallback.
    assert!(crate::match_runtime::find_site(&before.game, plan.players[0].spawn, 3, &[]).is_none());
    let mut m = Match::new(&source, opts).unwrap();
    let mut trapped = 0;
    for owner in 0..2 {
        let spawn = m.start.players[owner].spawn;
        for (name, offset) in [("hq", 0), ("barracks", 4)] {
            let cell =
                (spawn.z / 2.0).floor() as usize * 64 - 64 + (spawn.x / 2.0).floor() as usize - 1
                    + offset;
            let building = &m.world.game.units[&own(&m, owner, name)];
            assert_eq!(
                building.cells.as_ref().unwrap(),
                &crate::match_runtime::footprint(&before.game, cell, 3)
                    .unwrap()
                    .into_iter()
                    .map(|c| c as u32)
                    .collect::<Vec<_>>()
            );
            assert!(
                building
                    .cells
                    .as_ref()
                    .unwrap()
                    .iter()
                    .any(|&c| before.game.chars[c as usize] == b'W')
            );
        }
    }
    for old in before.game.units.values() {
        let current = &m.world.game.units[&old.id];
        if m.world.game.flags_at(old.x, old.z) & crate::game::MOVE != 0 {
            trapped += 1;
            assert_ne!(old.at(), current.at());
        }
        assert_eq!(
            m.world.game.flags_at(current.x, current.z) & crate::game::MOVE,
            0
        );
    }
    assert!(
        trapped > 0,
        "the real starting force must intersect a new footprint"
    );
    let mut delivery = crate::recipient_snapshot::DeliveryState::new(&m).unwrap();
    for tick in 0..6 {
        m.advance_tick().unwrap();
        let frame = delivery
            .capture(&mut m, &[tick == 0; 2], &[false; 2])
            .unwrap();
        for slot in 0..2 {
            frame.recipient(slot).unwrap();
        }
    }
}

#[test]
fn base52_first_fitting_site_moves_away_from_a_nearby_income_point() {
    let mut input = map().source().clone();
    input["points"] = json!([{"x":8,"y":8,"mp":2,"vp":1}]);
    let source = AuthoritativeMap::from_json(&serde_json::to_vec(&input).unwrap()).unwrap();
    let m = Match::new(&source, options(Mode::Conquest, true)).unwrap();
    let hq = &m.world.game.units[&own(&m, 0, "hq")];
    let barracks = &m.world.game.units[&own(&m, 0, "barracks")];
    // The open-map HQ would be at (17,17). East is the first valid BFS site.
    assert_eq!(hq.at(), crate::game::Point { x: 31.0, z: 17.0 });
    assert_eq!(barracks.at(), crate::game::Point { x: 3.0, z: 17.0 });
    for building in [hq, barracks] {
        let point = m.start.points[0].at;
        assert!(crate::math::hypot(building.x - point.x, building.z - point.z) >= 13.0);
    }
}

#[test]
fn mode01_non_world_mode_rows_match_the_prototype_public_fields() {
    use crate::recipient_snapshot::{
        DeliveryState, SelectedSeat, SnapshotCache, SnapshotError, mode_row,
    };

    // Prototype sim.js:5895-5898; Tutorial's initial goal is tutorial.js:11.
    // Full delivery follows current canonical mode admission independently of
    // the component row, which also represents modes whose snapshots refuse.
    for mode in MODES {
        let mut m = create(mode, ArmySize::Standard).unwrap();
        let expected = match mode {
            Mode::Conquest => Json::Null,
            Mode::Assault => json!({
                "kind":"assault", "defenderTeam":1,"attackerTeam":0,
                "timeLeft":900.0,"suddenDeath":false,"total":1
            }),
            Mode::Annihilation => json!({
                "kind":"annihilation","suddenDeath":false,"bunkers":[1,1]
            }),
            Mode::Classic => json!({
                "kind":"classic","timeLeft":1500.0,"suddenDeath":false
            }),
            Mode::Horde => json!({
                "kind":"horde","defenderTeam":0,"attackerTeam":1,
                "timeLeft":45.0,"suddenDeath":false,"wave":0,"left":0.0,
                "active":false,"slot":2,
                "nextProfile":m.horde_projection().unwrap().next_profile.unwrap().as_str()
            }),
            Mode::Tutorial => json!({
                "kind":"tutorial","suddenDeath":false,"slot":2,
                "goal":"Move your squads up to the hedgerow","step":0,"steps":9
            }),
            _ => unreachable!(),
        };
        assert_eq!(mode_row(&m).unwrap(), expected, "{mode:?}");
        if !matches!(mode, Mode::Conquest | Mode::Classic | Mode::Horde) {
            assert!(matches!(
                SnapshotCache::new(&m),
                Err(SnapshotError::UnsupportedMode)
            ));
            continue;
        }
        let cache = SnapshotCache::new(&m).unwrap();
        for slot in 0..m.start.players.len() {
            let projection = cache.recipient(slot).unwrap();
            assert_eq!(
                projection.fields.get("mode"),
                (!expected.is_null()).then_some(&expected),
                "{mode:?}, seat {slot}"
            );
        }
        let selected: Vec<_> = (0..m.start.players.len())
            .map(|slot| SelectedSeat {
                slot,
                full: true,
                omit: false,
            })
            .collect();
        let mut delivery = DeliveryState::new(&m).unwrap();
        for _ in 0..6 {
            let report = m.advance_tick().unwrap();
            let current = mode_row(&m).unwrap();
            let frame = delivery
                .capture_selected_with_tick_events(&mut m, &selected, &report.events)
                .unwrap()
                .unwrap();
            for seat in &selected {
                let snapshot = frame.recipient(seat.slot).unwrap().into_json();
                assert_eq!(snapshot["t"], "s");
                assert!(snapshot["units"].is_array());
                assert!(snapshot["cells"].is_array());
                assert!(snapshot["fog"]["v"].is_string());
                assert!(snapshot["fog"]["e"].is_string());
                assert_eq!(snapshot["mode"], current, "{mode:?}, seat {}", seat.slot);
            }
            assert!(m.tick_state.fault().is_none());
        }
    }
    let mut default = create(Mode::Conquest, ArmySize::Standard).unwrap();
    default.options.mode = Mode::Default;
    assert_eq!(mode_row(&default).unwrap(), Json::Null);
}

#[test]
fn mode01_horde_row_omits_unset_profile_boss_and_active_next_profile() {
    use crate::recipient_snapshot::mode_row;

    let mut m = create(Mode::Horde, ArmySize::Standard).unwrap();
    // setupHorde (sim.js:859-861) sets neither profile nor boss before wave 1.
    let startup = mode_row(&m).unwrap();
    assert!(startup.get("profile").is_none());
    assert!(startup.get("boss").is_none());
    assert!(startup.get("nextProfile").is_some());
    let horde = m.horde.as_mut().unwrap();
    horde.wave = 1;
    horde.active = true;
    horde.profile = crate::match_horde::HordeProfile::Armor;
    horde.boss = Some([100.0, 200.0]);
    let active = mode_row(&m).unwrap();
    assert_eq!(active["profile"], "armor");
    assert_eq!(active["boss"], json!([100.0, 200.0]));
    assert!(active.get("nextProfile").is_none());
    // sim.js:955 writes null once an active wave has no living boss.
    m.horde.as_mut().unwrap().boss = None;
    assert_eq!(mode_row(&m).unwrap().get("boss"), Some(&Json::Null));
}

#[test]
fn mode01_mode_rows_round_timers_and_refuse_invalid_values_or_mode_pairs() {
    use crate::recipient_snapshot::{SnapshotError, mode_row};

    let mut assault = create(Mode::Assault, ArmySize::Standard).unwrap();
    let mut classic = create(Mode::Classic, ArmySize::Standard).unwrap();
    for (time, expected) in [(3.01, 4.0), (3.0, 3.0), (-0.1, 0.0), (-3.5, 0.0)] {
        assault.assault.as_mut().unwrap().time_left = time;
        classic.elimination.as_mut().unwrap().time_left = Some(time);
        assert_eq!(mode_row(&assault).unwrap()["timeLeft"], json!(expected));
        assert_eq!(mode_row(&classic).unwrap()["timeLeft"], json!(expected));
    }
    for time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assault.assault.as_mut().unwrap().time_left = time;
        classic.elimination.as_mut().unwrap().time_left = Some(time);
        assert_eq!(mode_row(&assault), Err(SnapshotError::InvalidNumber));
        assert_eq!(mode_row(&classic), Err(SnapshotError::InvalidNumber));
    }
    classic.elimination.as_mut().unwrap().time_left = None;
    assert_eq!(mode_row(&classic), Err(SnapshotError::Rules));
    for mode in MODES {
        let mut m = create(mode, ArmySize::Standard).unwrap();
        m.world.game.mode = if mode == Mode::Conquest {
            Some(crate::game::ModeKind::Tutorial)
        } else {
            None
        };
        assert_eq!(mode_row(&m), Err(SnapshotError::UnsupportedMode));
    }
}
