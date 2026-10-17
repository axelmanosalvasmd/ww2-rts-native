use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::UnitControl,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    navigation::{TerrainView, Versions},
    objectives::{Objective, ObjectivePlayer, ObjectiveUnit},
    supply_network::{GroundSupplyNetwork, SupplyInput, SupplyUnit},
    unit_spawn::{SpawnPlacer, initial_unit},
    weather::WeatherChoice,
};
use serde_json::json;

fn cell(x: usize) -> Point {
    Point {
        x: x as f64 * 2.0 + 1.0,
        z: 1.0,
    }
}

fn point(x: usize, owner: Option<usize>, kind: Option<&str>) -> Objective {
    Objective {
        id: json!(x),
        at: cell(x),
        vp: 0.0,
        mp: 0.0,
        kind: kind.map(str::to_owned),
        needs: None,
        owner,
        capper: None,
        progress: 0.0,
        cut: false,
        on_point: Vec::new(),
        contested: false,
    }
}

fn network(
    mode: Mode,
    players: &[ObjectivePlayer],
    points: &[Objective],
    units: &[SupplyUnit<'_>],
) -> GroundSupplyNetwork {
    let flags = [0u16; 32];
    let chars = [b'.'; 32];
    GroundSupplyNetwork::compute_for_mode(
        SupplyInput {
            view: TerrainView {
                width: 32,
                height: 1,
                flags: &flags,
                heights: None,
                world_known: false,
                knowledge_id: 0,
                versions: Versions::default(),
                changed: None,
                changes: &[],
            },
            chars: &chars,
            initial_chars: &chars,
            initial_heights: None,
            points,
            units,
            players,
        },
        &mut SpawnPlacer::new(),
        mode,
    )
    .unwrap()
}

fn hq(owner: usize, hp: f64, built: Option<f64>) -> SupplyUnit<'static> {
    SupplyUnit {
        fact: ObjectiveUnit {
            id: 80,
            owner,
            kind: "hq",
            at: cell(10),
            hp,
            airborne: false,
            riding: false,
            retreating: false,
            forced_reserve: false,
        },
        built,
    }
}

#[test]
fn log22_network_sources_follow_hq_fallback_point_kind_and_point_status() {
    let mut players = [
        ObjectivePlayer {
            team: 0,
            out: false,
            spawn: cell(0),
        },
        ObjectivePlayer {
            team: 0,
            out: false,
            spawn: cell(2),
        },
        ObjectivePlayer {
            team: 1,
            out: false,
            spawn: cell(31),
        },
    ];
    let steps =
        |mode, points: &[Objective], units: &[SupplyUnit<'_>], players: &[ObjectivePlayer]| {
            network(mode, players, points, units).teams[0].steps.clone()
        };
    assert_eq!(
        steps(Mode::Conquest, &[], &[hq(0, 400.0, Some(1.0))], &players)[10],
        0
    );
    assert_eq!(
        steps(Mode::Conquest, &[], &[hq(0, 400.0, Some(1.0))], &players)[0],
        10
    );
    assert_eq!(
        steps(Mode::Conquest, &[], &[hq(0, 400.0, None)], &players)[0],
        10
    );
    assert_eq!(
        steps(Mode::Conquest, &[], &[hq(1, 400.0, Some(1.0))], &players)[0],
        10
    );
    assert_eq!(
        steps(Mode::Conquest, &[], &[hq(2, 400.0, Some(1.0))], &players)[0],
        0
    );
    for invalid in [hq(0, 0.0, Some(1.0)), hq(0, 400.0, Some(0.5))] {
        assert_eq!(steps(Mode::Conquest, &[], &[invalid], &players)[0], 0);
    }
    players[0].out = true;
    assert_eq!(
        steps(Mode::Conquest, &[], &[hq(0, 400.0, Some(1.0))], &players)[2],
        0
    );
    assert_eq!(
        steps(Mode::Conquest, &[], &[hq(0, 400.0, Some(1.0))], &players)[0],
        2
    );
    players[0].out = false;

    let held = point(20, Some(0), Some("depot"));
    assert_eq!(
        steps(Mode::Conquest, std::slice::from_ref(&held), &[], &players)[20],
        0
    );
    let mut cut = held.clone();
    cut.cut = true;
    let mut contested = held;
    contested.contested = true;
    for excluded in [
        cut,
        contested,
        point(20, Some(2), Some("depot")),
        point(20, None, Some("depot")),
    ] {
        assert!(steps(Mode::Conquest, &[excluded], &[], &players)[20] > 0);
    }
    let ordinary = point(20, Some(0), None);
    assert!(
        steps(
            Mode::Conquest,
            std::slice::from_ref(&ordinary),
            &[],
            &players
        )[20]
            > 0
    );
    assert_eq!(steps(Mode::Annihilation, &[ordinary], &[], &players)[20], 0);
}

fn recovery_scene() -> (Match, u32, u32) {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name": "Depot recovery", "w": 60, "h": 60,
            "rows": vec![".".repeat(60); 60],
            "heights": vec!["0".repeat(60); 60],
            "spawns": [{"x": 4, "y": 4}, {"x": 55, "y": 55}],
            "points": [{"x": 30, "y": 30}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 703,
            mode: Mode::Conquest,
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
    m.start.economy.players[0].mp = 5_000.0;
    m.start.points[0].kind = Some("depot".into());
    m.start.points[0].owner = Some(0);
    let depot = m.start.points[0].at;
    let rifle = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let car = m.next_unit_id;
    let vehicle = initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        m.rules.kind("armoredcar").unwrap(),
        car,
        0,
        depot,
    )
    .unwrap();
    m.commands
        .units
        .insert(car, UnitControl::new(&vehicle, &m.rules).unwrap());
    m.combat.initialize(car);
    m.firing.initialize(car);
    m.abilities.initialize(car);
    m.recovery.initialize(car);
    m.world.game.units.insert(car, vehicle);
    m.grid.update(car, &m.world.game.units[&car]);
    m.next_unit_id += 1;
    let rifle_unit = m.world.game.units.get_mut(&rifle).unwrap();
    rifle_unit.x = depot.x;
    rifle_unit.z = depot.z;
    rifle_unit.hp -= 20.0;
    m.grid.update(rifle, &m.world.game.units[&rifle]);
    m.world.game.units.get_mut(&car).unwrap().hp -= 100.0;
    m.recovery.get_mut(rifle).unwrap().reinforcement = 2.0;
    (m, rifle, car)
}

fn recover(m: &mut Match, rifle: u32, car: u32) -> (f64, f64) {
    let rifle_before = m.world.game.units[&rifle].hp;
    let car_before = m.world.game.units[&car].hp;
    let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let frame = m.prepare_recovery(&ids).unwrap();
    m.reinforce(&frame, m.rules.movement.tick).unwrap();
    (
        m.world.game.units[&rifle].hp - rifle_before,
        m.world.game.units[&car].hp - car_before,
    )
}

#[test]
fn log47_held_allied_uncut_depot_reinforces_and_repairs_only_within_radius() {
    let (mut held, rifle, car) = recovery_scene();
    let (infantry, vehicle) = recover(&mut held, rifle, car);
    assert!(infantry > 0.0);
    assert!(vehicle > 0.0);

    let (mut allied, rifle, car) = recovery_scene();
    allied.options.players[1].team = 0;
    allied.start.players[1].config.team = 0;
    allied.start.economy.players[1].team = 0;
    allied.world.game.players[1].team = 0;
    allied.start.points[0].owner = Some(1);
    let (infantry, vehicle) = recover(&mut allied, rifle, car);
    assert!(infantry > 0.0);
    assert!(vehicle > 0.0);

    for condition in ["cut", "enemy", "outside"] {
        let (mut m, rifle, car) = recovery_scene();
        match condition {
            "cut" => m.start.points[0].cut = true,
            "enemy" => m.start.points[0].owner = Some(1),
            "outside" => {
                let at = m.start.points[0].at;
                for id in [rifle, car] {
                    m.world.game.units.get_mut(&id).unwrap().x = at.x + 15.1;
                    m.grid.update(id, &m.world.game.units[&id]);
                }
            }
            _ => unreachable!(),
        }
        assert_eq!(recover(&mut m, rifle, car), (0.0, 0.0), "{condition}");
    }
}
