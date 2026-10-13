use serde_json::Value;
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{PlaneState, Point},
    map::AuthoritativeMap,
    match_aviation::{AircraftRuntime, MissionKind, PlaneMission},
    match_commands::UnitControl,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    unit_spawn,
    weather::WeatherChoice,
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/air-firing-context.json")).unwrap()
}
fn number(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}
fn make(case: &Value, frame: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 811,
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
    m.world.game.units.clear();
    m.commands.units.clear();
    m.next_unit_id = 1000;
    m.grid.rebuild(&m.world.game.units);
    for row in frame["actors"].as_array().unwrap() {
        let id = row["id"].as_u64().unwrap() as u32;
        let owner = row["owner"].as_u64().unwrap() as usize;
        let name = row["kind"].as_str().unwrap();
        let kind = m.rules.kind(name).unwrap();
        let at = Point {
            x: number(&row["x"]),
            z: number(&row["z"]),
        };
        if row["air"].is_null() {
            let u = unit_spawn::initial_unit(
                &m.rules,
                Catalog::compiled().unwrap(),
                kind,
                id,
                owner,
                at,
            )
            .unwrap();
            m.commands
                .units
                .insert(id, UnitControl::new(&u, &m.rules).unwrap());
            m.combat.initialize(id);
            m.firing.initialize(id);
            m.world.game.units.insert(id, u);
        } else {
            m.next_unit_id = id;
            assert_eq!(m.spawn_aircraft(owner, name).unwrap(), id);
            let a = &row["aviation"];
            let mission = (!a["mission"].is_null()).then(|| {
                let p = &a["mission"];
                PlaneMission {
                    kind: match p["kind"].as_str().unwrap() {
                        "patrol" => MissionKind::Patrol,
                        "escort" => MissionKind::Escort,
                        "attack" => MissionKind::Attack,
                        "area" => MissionKind::Area,
                        value => panic!("unknown mission {value}"),
                    },
                    target: p["id"].as_u64().filter(|id| *id != 0).map(|id| id as u32),
                    at: Point {
                        x: number(&p["x"]),
                        z: number(&p["z"]),
                    },
                }
            });
            m.aviation.planes.insert(
                id,
                AircraftRuntime {
                    fuel: number(&a["fuel"]),
                    ammo: number(&a["ammo"]),
                    timer: number(&a["timer"]),
                    orbit: number(&a["ang"]),
                    mission,
                    downed: false,
                },
            );
        }
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.x = at.x;
        u.z = at.z;
        u.rot = number(&row["rot"]);
        u.hp = number(&row["hp"]);
        u.air = row["air"].as_str().map(|value| match value {
            "base" => PlaneState::Base,
            "rearm" => PlaneState::Rearm,
            "out" => PlaneState::Out,
            "home" => PlaneState::Home,
            "station" => PlaneState::Station,
            value => panic!("unknown plane state {value}"),
        });
        m.commands.units.get_mut(&id).unwrap().hold_fire = row["holdFire"].as_bool().unwrap();
        let firing = m.firing.initialize(id);
        firing.cooldown = number(&row["cooldown"]);
        firing.retarget = number(&row["retarget"]);
        firing.target_id = row["targetId"].as_u64().unwrap() as u32;
    }
    m.world.game.tick = frame["tick"].as_i64().unwrap();
    m.grid.rebuild(&m.world.game.units);
    m
}

#[test]
fn actual_source_offmap_air_poses_are_context_without_admitting_air_weapons() {
    let f = fixture();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    for case in f["cases"].as_array().unwrap() {
        for frame in [&case["before"], &case["after"]] {
            assert_eq!(frame["flights"], 0);
            assert!(frame["shots"].as_array().unwrap().is_empty());
            let mut m = make(case, frame);
            let plane = frame["actors"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| !a["air"].is_null())
                .unwrap()["id"]
                .as_u64()
                .unwrap() as u32;
            let u = &m.world.game.units[&plane];
            assert!(m.world.game.cell_of(u.x, u.z).is_none());
            let actors: Vec<_> = m.world.game.units.keys().copied().collect();
            let poses: Vec<_> = m
                .world
                .game
                .units
                .values()
                .map(|u| (u.pose(), u.hp.to_bits(), u.air))
                .collect();
            let aviation = m.aviation.planes.clone();
            m.capture_firing_prior().unwrap();
            m.begin_firing_tick(&actors).unwrap();
            m.step_firing_flights().unwrap();
            assert_eq!(
                m.world
                    .game
                    .units
                    .values()
                    .map(|u| (u.pose(), u.hp.to_bits(), u.air))
                    .collect::<Vec<_>>(),
                poses
            );
            assert_eq!(m.aviation.planes, aviation);
            assert!(m.firing.events.is_empty());
            let before = m.firing.clone();
            assert!(m.apply_unit_firing(plane, false, 0.05).is_err());
            assert!(m.advance_firing_timers(plane, 0.05).is_err());
            assert_eq!(m.firing, before);
        }
    }
}

#[test]
fn mismatched_air_and_offmap_ground_refuse_before_firing_state_changes() {
    let f = fixture();
    let case = &f["cases"][0];
    for invalid in 0..8 {
        let mut m = make(case, &case["before"]);
        let plane = 1000;
        match invalid {
            0 => {
                m.aviation.planes.remove(&plane);
            }
            1 => {
                m.world.game.units.get_mut(&plane).unwrap().air = None;
            }
            2 => {
                m.world.game.units.get_mut(&plane).unwrap().kind = m.rules.kind("rifle").unwrap();
            }
            3 => {
                m.world.game.units.get_mut(&plane).unwrap().owner = m.world.game.players.len();
            }
            4 => {
                m.aviation.planes.get_mut(&plane).unwrap().fuel = f64::NAN;
            }
            5 => {
                m.world.game.units.get_mut(&plane).unwrap().x = 1_073_741_825.0;
            }
            6 => {
                m.world.game.units.get_mut(&1001).unwrap().x = -1.0;
            }
            _ => {
                m.commands.units.remove(&plane);
            }
        }
        let actors: Vec<_> = m.world.game.units.keys().copied().collect();
        let before = m.firing.clone();
        assert!(m.capture_firing_prior().is_err(), "invalid {invalid}");
        assert_eq!(m.firing, before);
        assert!(m.begin_firing_tick(&actors).is_err(), "invalid {invalid}");
        assert_eq!(m.firing, before);
        assert!(m.step_firing_flights().is_err(), "invalid {invalid}");
        assert_eq!(m.firing, before);
    }
}
