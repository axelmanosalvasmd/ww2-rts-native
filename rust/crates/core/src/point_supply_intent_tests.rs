//! Point connectivity and cut consumers on explicit component and ordinary match inputs.
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::{self as economy, ArmySize},
    game::{CELL, MOVE, Point, VBLOCK},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    navigation::{TerrainView, Versions},
    objectives::{self, Objective, ObjectivePlayer, ObjectiveRules, ObjectiveUnit},
    recipient_snapshot::SnapshotCache,
    supply_network::{GroundSupplyNetwork, SupplyInput},
    unit_spawn::SpawnPlacer,
    weather::WeatherChoice,
};
use serde_json::json;

fn at(x: usize, z: usize) -> Point {
    Point {
        x: (x as f64 + 0.5) * CELL,
        z: (z as f64 + 0.5) * CELL,
    }
}
fn objective() -> Objective {
    Objective {
        id: json!(0),
        at: at(65, 20),
        vp: 3.,
        mp: 5.,
        kind: Some("depot".into()),
        needs: None,
        owner: Some(0),
        capper: Some(0),
        progress: 1.,
        cut: false,
        on_point: vec![],
        contested: false,
    }
}
fn players() -> [ObjectivePlayer; 2] {
    [
        ObjectivePlayer {
            team: 0,
            out: false,
            spawn: at(5, 20),
        },
        ObjectivePlayer {
            team: 1,
            out: false,
            spawn: at(75, 20),
        },
    ]
}
fn view<'a>(flags: &'a [u16], heights: Option<&'a [i8]>) -> TerrainView<'a> {
    TerrainView {
        width: 80,
        height: 40,
        flags,
        heights,
        world_known: false,
        knowledge_id: 0,
        versions: Versions::default(),
        changed: None,
        changes: &[],
    }
}
#[test]
fn intent_log45_vehicle_corridor_height_and_enemy_control_cut_and_restore() {
    // A single vehicle corridor represents the bridge or street between spawn and point.
    let mut flags = vec![MOVE; 80 * 40];
    flags[20 * 80..21 * 80].fill(0);
    let mut heights = vec![0; flags.len()];
    let mut points = [objective()];
    let mut players = players();
    let rules = ObjectiveRules {
        capture_time: 10.,
        point_radius: 0.,
        zoc_radius: 6.,
        free_radius: 0.,
    };
    let enemy = ObjectiveUnit {
        id: 1,
        owner: 1,
        kind: "rifle",
        at: at(35, 20),
        hp: 100.,
        airborne: false,
        riding: false,
        retreating: false,
        forced_reserve: false,
    };
    let mut placer = SpawnPlacer::new();
    for (blocked, height, hostile, cut) in [
        (0, 0, false, false),
        (VBLOCK, 0, false, true),
        (MOVE, 0, false, true),
        (0, 2, false, true),
        (0, 1, false, false),
        (0, 0, true, true),
        (0, 0, false, false),
    ] {
        flags[20 * 80 + 35] = blocked;
        heights[20 * 80 + 35] = height;
        let units = if hostile {
            std::slice::from_ref(&enemy)
        } else {
            &[]
        };
        let reports = objectives::supply_lines_with_rules(
            view(&flags, Some(&heights)),
            &mut points,
            units,
            &players,
            &mut placer,
            rules,
        )
        .unwrap();
        assert_eq!(
            points[0].cut, cut,
            "blocked={blocked}, height={height}, hostile={hostile}"
        );
        assert_eq!(reports[0].reached[20 * 80 + 65], u8::from(!cut));
    }
    flags[20 * 80 + 35] = MOVE;
    players[1].team = 0;
    objectives::supply_lines_with_rules(
        view(&flags, None),
        &mut points,
        &[],
        &players,
        &mut placer,
        rules,
    )
    .unwrap();
    assert!(
        !points[0].cut,
        "an allied spawn on the far side supplies the point"
    );
    players[1].out = true;
    objectives::supply_lines_with_rules(
        view(&flags, None),
        &mut points,
        &[],
        &players,
        &mut placer,
        rules,
    )
    .unwrap();
    assert!(points[0].cut, "eliminated allies do not seed the flood");
    points[0].owner = None;
    points[0].cut = true;
    objectives::supply_lines_with_rules(
        view(&flags, None),
        &mut points,
        &[],
        &players,
        &mut placer,
        rules,
    )
    .unwrap();
    assert!(!points[0].cut, "neutral points clear stale cut flags");
    flags[20 * 80 + 35] = 0;
    points[0].owner = Some(0);
    let square_rules = ObjectiveRules {
        point_radius: 2. * CELL,
        ..rules
    };
    for (row, cut) in [(22, false), (23, true)] {
        points[0].at = at(65, row);
        objectives::supply_lines_with_rules(
            view(&flags, None),
            &mut points,
            &[],
            &players,
            &mut placer,
            square_rules,
        )
        .unwrap();
        assert_eq!(
            points[0].cut, cut,
            "a reached cell on the capture-square edge supplies the point"
        );
    }
}

fn arena(supply: bool) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"point supply arena", "w":80,"h":80,"rows":vec![".".repeat(80);80],
            "spawns":[{"x":12,"y":40},{"x":68,"y":40}],
            "points":[{"x":65,"y":8,"vp":3,"mp":5}],"defend":[0]
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
                    name: "a".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "b".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    m.start.points[0].owner = Some(0);
    m.start.points[0].progress = 1.;
    // Physical logistics stays disabled, while point connectivity remains enabled.
    for z in 0..m.world.game.h {
        m.world.game.flags[z * m.world.game.w + 40] |= MOVE | VBLOCK;
    }
    m
}
#[test]
fn intent_log45_match_supply_beat_disabled_rule_and_log46_public_cut_survive_ticks() {
    for enabled in [true, false] {
        let mut m = arena(enabled);
        let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
        m.apply_point_supply(&ids).unwrap();
        assert!(!m.start.points[0].cut, "tick zero is not a supply beat");
        for _ in 0..3 {
            m.advance_tick().unwrap();
            assert_eq!(m.start.points[0].cut, enabled);
            assert!(m.tick_state.fault().is_none());
            let cache = SnapshotCache::new(&m).unwrap();
            for slot in 0..2 {
                assert_eq!(
                    cache.recipient(slot).unwrap().fields["points"][0][4],
                    json!(u8::from(enabled))
                );
            }
        }
        let expected_vp = if enabled {
            0.
        } else {
            3. * m.rules.movement.tick * 3.
        };
        assert!((m.start.economy.players[0].vp - expected_vp).abs() < 1e-12);
        // Restoring ground between beats does not eagerly clear the cut flag.
        for z in 0..m.world.game.h {
            m.world.game.flags[z * m.world.game.w + 40] &= !(MOVE | VBLOCK);
        }
        m.apply_point_supply(&ids).unwrap();
        assert_eq!(m.start.points[0].cut, enabled);
        m.world.game.tick = 40;
        m.apply_point_supply(&ids).unwrap();
        assert_eq!(
            m.start.points[0].cut, enabled,
            "tick 40 is between supply beats"
        );
        m.world.game.tick = 41;
        m.apply_point_supply(&ids).unwrap();
        assert!(
            !m.start.points[0].cut,
            "tick 41 restores connectivity on the next beat"
        );
    }
}

#[test]
fn intent_log46_cut_removes_point_mp_vp_and_classic_team_munitions() {
    for mode in [Mode::Default, Mode::Classic] {
        let mut earned = Vec::new();
        for cut in [false, true] {
            let mut state = economy::EconomyState {
                players: vec![
                    economy::PlayerEconomy {
                        team: 0,
                        ..Default::default()
                    },
                    economy::PlayerEconomy {
                        team: 0,
                        ..Default::default()
                    },
                ],
            };
            let points = [economy::Point {
                owner: Some(0),
                cut,
                mp: 5.,
                vp: 3.,
            }];
            state
                .advance_income(
                    &economy::Context {
                        mode: economy::ModeContext {
                            kind: mode,
                            ..Default::default()
                        },
                        army: ArmySize::Standard,
                        units: &[],
                        drops: &[],
                        points: &points,
                        nodes: &[],
                        world: None,
                    },
                    1.,
                    false,
                )
                .unwrap();
            earned.push(state.players);
        }
        if mode == Mode::Default {
            let cfg = Catalog::compiled().unwrap().table("CFG").unwrap();
            let catchup_per = cfg.get("catchupPer").unwrap().number().unwrap();
            let expected = 5. - 3. / catchup_per;
            assert!((earned[0][0].mp - earned[1][0].mp - expected).abs() < 1e-12);
            assert_eq!(
                earned[1][0].mp,
                cfg.get("mpBase").unwrap().number().unwrap()
            );
            assert_eq!(earned[0][0].vp - earned[1][0].vp, 3.);
            assert_eq!(earned[1][0].vp, 0.);
        } else {
            for (slot, (supplied, cut)) in earned[0].iter().zip(&earned[1]).enumerate() {
                assert!(supplied.mun > cut.mun, "allied Classic slot {slot}");
                assert_eq!(supplied.mp, cut.mp);
            }
        }
    }
}

#[test]
fn intent_log46_cut_depot_is_excluded_from_territory_sources() {
    let flags = vec![0; 80 * 40];
    let chars = vec![b'.'; flags.len()];
    let players = players();
    for (cut, contested, source) in [
        (false, false, true),
        (true, false, false),
        (false, true, false),
    ] {
        let mut point = objective();
        point.cut = cut;
        point.contested = contested;
        let points = [point];
        let network = GroundSupplyNetwork::compute(
            SupplyInput {
                view: view(&flags, None),
                chars: &chars,
                initial_chars: &chars,
                initial_heights: None,
                points: &points,
                units: &[],
                players: &players,
            },
            &mut SpawnPlacer::new(),
        )
        .unwrap();
        let team = network.teams.iter().find(|row| row.team == 0).unwrap();
        assert_eq!(
            team.steps[20 * 80 + 65] == 0,
            source,
            "cut={cut}, contested={contested}"
        );
    }
}

#[test]
fn intent_log45_nonconquest_point_supply_seam_refuses_before_mutation() {
    use crate::{game::ModeKind, recipient_snapshot::SnapshotError};
    // The flood component has no mode input. Mode exclusions belong to adapters.
    // Inject only mode identity into the ordinary scene to test its admission
    // boundary. These inputs are not admitted Horde, Tutorial or World rooms.
    for (mode, kind, refusal, snapshot_error) in [
        (
            Mode::Horde,
            ModeKind::Horde,
            "missing Horde state",
            SnapshotError::UnsupportedMode,
        ),
        (
            Mode::Tutorial,
            ModeKind::Tutorial,
            "missing Tutorial runtime",
            SnapshotError::Rules,
        ),
        (
            Mode::World,
            ModeKind::World,
            "ordinary finish state requires Conquest or Tutorial",
            SnapshotError::UnsupportedMode,
        ),
    ] {
        for supply in [true, false] {
            let mut m = arena(supply);
            m.options.mode = mode;
            m.world.game.mode = Some(kind);
            m.world.game.tick = 1;
            let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
            let points = m.start.points.clone();
            let wallets = m.start.economy.clone();
            assert_eq!(m.apply_point_supply(&ids), Err(refusal.into()));
            assert_eq!(m.start.points, points);
            assert_eq!(m.start.economy, wallets);
            assert!(m.tick_state.fault().is_none());
            assert_eq!(SnapshotCache::new(&m).err(), Some(snapshot_error.clone()));
        }
    }
}
