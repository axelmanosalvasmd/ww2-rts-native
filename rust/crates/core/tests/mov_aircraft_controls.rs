//! Native aircraft component checks and ordinary weather match evidence.
use serde_json::json;
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{PlaneState, Point},
    match_aviation::{AviationEvent, MissionKind, PlaneMission},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};

fn scene() -> Match {
    scene_with(WeatherChoice::Map, json!({"x":32,"y":32}))
}
fn scene_with(weather: WeatherChoice, objective: serde_json::Value) -> Match {
    let map = ww2_core::map::AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Aircraft controls", "w":64, "h":64,
            "rows":vec![".".repeat(64);64],
            "spawns":[{"x":8,"y":8},{"x":54,"y":54}],
            "points":[objective]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 7301,
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
            weather,
            weather_seed: Some(1),
            living_weather: weather != WeatherChoice::Map,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}

fn at(x: f64, z: f64) -> Point {
    Point { x, z }
}
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-8, "{actual} != {expected}");
}
fn place(m: &mut Match, id: u32, position: Point, state: PlaneState) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = position.x;
    u.z = position.z;
    u.air = Some(state);
    m.grid.update(id, u);
}
fn ground(m: &mut Match, kind: &str) -> u32 {
    let id = m.next_unit_id;
    let draft = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        0,
        m.rules.kind(kind).unwrap(),
        id,
        0,
        m.start.players[0].spawn,
        Some(at(60., 60.)),
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
        vec![draft],
    )
    .unwrap()
    .publish();
    m.combat.initialize(id);
    m.firing.initialize(id);
    m.abilities.initialize(id);
    id
}

#[test]
fn mov85_sorties_orbit_return_for_each_trigger_and_fully_rearm() {
    for kind in ["fighter", "attacker", "bomber"] {
        let mut m = scene();
        let id = m.spawn_aircraft(0, kind).unwrap();
        m.commands.units.get_mut(&id).unwrap().hold_fire = true;
        let initial = m.aviation.planes[&id];
        let maximum = m.world.game.units[&id].hp;
        let mission = PlaneMission {
            kind: MissionKind::Patrol,
            target: None,
            at: at(80., 80.),
        };
        assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Base));
        assert!(m.send_aircraft_mission(0, id, mission).unwrap());
        assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Out));
        let mut reached_station = false;
        for _ in 0..400 {
            let before = m.world.game.units[&id].at();
            m.step_aircraft(id, 0.05).unwrap();
            assert_ne!(m.world.game.units[&id].at(), before);
            close(m.aviation.planes[&id].fuel, initial.fuel);
            if m.world.game.units[&id].air == Some(PlaneState::Station) {
                assert!(
                    ww2_core::math::hypot(before.x - mission.at.x, before.z - mission.at.z) < 24.
                );
                reached_station = true;
                break;
            }
        }
        assert!(reached_station, "{kind} reaches its patrol");
        let angle = m.aviation.planes[&id].orbit;
        let speed = Catalog::compiled()
            .unwrap()
            .unit(kind)
            .unwrap()
            .get("speed")
            .unwrap()
            .number()
            .unwrap();
        m.step_aircraft(id, 0.05).unwrap();
        close(m.aviation.planes[&id].fuel, initial.fuel - 0.05);
        close(m.aviation.planes[&id].orbit - angle, speed / 18. * 0.05);
        for _ in 0..100 {
            let before = m.world.game.units[&id].at();
            m.step_aircraft(id, 0.05).unwrap();
            let u = &m.world.game.units[&id];
            assert_eq!(u.air, Some(PlaneState::Station));
            assert!(ww2_core::math::hypot(u.x - before.x, u.z - before.z) > 0.001);
            assert!(ww2_core::math::hypot(u.x - mission.at.x, u.z - mission.at.z) < 24.);
        }
        let u = &m.world.game.units[&id];
        assert!((ww2_core::math::hypot(u.x - mission.at.x, u.z - mission.at.z) - 18.).abs() < 3.);
        close(m.aviation.planes[&id].fuel, initial.fuel - 5.05);

        for trigger in ["fuel", "ammo", "wounded", "mission"] {
            place(&mut m, id, at(70., 70.), PlaneState::Station);
            *m.aviation.planes.get_mut(&id).unwrap() = initial;
            m.aviation.planes.get_mut(&id).unwrap().mission = Some(mission);
            m.world.game.units.get_mut(&id).unwrap().hp = maximum * 0.35;
            m.step_aircraft(id, 0.05).unwrap();
            assert_eq!(
                m.world.game.units[&id].air,
                Some(PlaneState::Station),
                "35 percent is not below bail threshold"
            );
            match trigger {
                "fuel" => m.aviation.planes.get_mut(&id).unwrap().fuel = 0.,
                "ammo" => m.aviation.planes.get_mut(&id).unwrap().ammo = 0.,
                "wounded" => m.world.game.units.get_mut(&id).unwrap().hp = maximum * 0.34,
                "mission" => m.aviation.planes.get_mut(&id).unwrap().mission = None,
                _ => unreachable!(),
            }
            m.step_aircraft(id, 0.05).unwrap();
            assert_eq!(
                m.world.game.units[&id].air,
                Some(PlaneState::Home),
                "{kind}: {trigger}"
            );
            let mut rearming = false;
            for _ in 0..400 {
                m.step_aircraft(id, 0.05).unwrap();
                if m.world.game.units[&id].air == Some(PlaneState::Rearm) {
                    rearming = true;
                    break;
                }
            }
            assert!(rearming, "{kind} returns after {trigger}");
            assert_eq!(m.aviation.planes[&id].mission, None);
            let base = m.aircraft_base(0, m.world.game.units[&id].at()).unwrap();
            assert!(
                ww2_core::math::hypot(
                    m.world.game.units[&id].x - base.x,
                    m.world.game.units[&id].z - base.z
                ) < 6.
            );
            let hp = m.world.game.units[&id].hp;
            m.step_aircraft(id, 1.).unwrap();
            close(m.world.game.units[&id].hp, hp + maximum / 30.);
            assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Rearm));
            assert!(!m.send_aircraft_mission(0, id, mission).unwrap());
            m.step_aircraft(id, 29.).unwrap();
            assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Base));
            close(m.world.game.units[&id].hp, maximum);
            close(m.aviation.planes[&id].fuel, initial.fuel);
            close(m.aviation.planes[&id].ammo, initial.ammo);
        }
    }
}

#[test]
fn mov85_base_and_rearming_aircraft_cannot_be_seen_or_hit_by_aa() {
    let mut m = scene();
    let gun = ground(&mut m, "flak");
    m.world.game.units.get_mut(&gun).unwrap().still = Some(2.);
    let plane = m.spawn_aircraft(1, "fighter").unwrap();
    for state in [PlaneState::Out, PlaneState::Base, PlaneState::Rearm] {
        place(&mut m, plane, at(65., 60.), state);
        ww2_core::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
        assert_eq!(
            m.world.game.players[0].sees(plane),
            state == PlaneState::Out
        );
        // Even a stale visibility entry must not make a parked aircraft targetable.
        m.world.game.players[0].visible = vec![plane];
        let hp = m.world.game.units[&plane].hp;
        m.step_aircraft_anti_air(&[gun], 0.05).unwrap();
        assert_eq!(m.world.game.units[&plane].hp < hp, state == PlaneState::Out);
    }
}

#[test]
fn mov89_all_aa_kinds_apply_continuous_damage_only_to_eligible_nearest_planes() {
    for (kind, dps, range) in [
        ("flak", 25., 45.),
        ("flaktrack", 25., 45.),
        ("flakpos", 40., 55.),
        ("fighter", 30., 30.),
        ("destroyer", 30., 45.),
        ("mg", 5., 30.),
    ] {
        let mut m = scene();
        let gun = if kind == "fighter" {
            let id = m.spawn_aircraft(0, kind).unwrap();
            place(&mut m, id, at(60., 60.), PlaneState::Station);
            id
        } else {
            ground(&mut m, kind)
        };
        {
            let u = m.world.game.units.get_mut(&gun).unwrap();
            u.still = Some(2.);
            if u.built.is_some() {
                u.built = Some(1.);
            }
        }
        let near = m.spawn_aircraft(1, "bomber").unwrap();
        let far = m.spawn_aircraft(1, "bomber").unwrap();
        let ally = m.spawn_aircraft(0, "bomber").unwrap();
        place(&mut m, near, at(65., 60.), PlaneState::Station);
        place(&mut m, far, at(70., 60.), PlaneState::Out);
        place(&mut m, ally, at(61., 60.), PlaneState::Out);
        let hp = m.world.game.units[&near].hp;
        let far_hp = m.world.game.units[&far].hp;
        m.world.game.players[0].visible = vec![near, far, ally];
        m.step_aircraft_anti_air(&[gun], 0.2).unwrap();
        close(m.world.game.units[&near].hp, hp - dps * 0.2);
        close(m.world.game.units[&far].hp, far_hp);
        for control in ["hidden", "parked", "dead"] {
            let u = m.world.game.units.get_mut(&near).unwrap();
            u.air = Some(PlaneState::Station);
            u.hp = hp;
            m.world.game.players[0].visible = if control == "hidden" {
                vec![far, ally]
            } else {
                vec![near, far, ally]
            };
            if control == "parked" {
                m.world.game.units.get_mut(&near).unwrap().air = Some(PlaneState::Base);
            }
            if control == "dead" {
                m.world.game.units.get_mut(&near).unwrap().hp = 0.;
            }
            let before = m.world.game.units[&far].hp;
            m.step_aircraft_anti_air(&[gun], 0.1).unwrap();
            close(m.world.game.units[&far].hp, before - dps * 0.1);
        }
        m.world.game.players[0].visible = vec![far];
        place(&mut m, far, at(60. + range + 0.001, 60.), PlaneState::Home);
        let before = m.world.game.units[&far].hp;
        m.step_aircraft_anti_air(&[gun], 0.1).unwrap();
        close(m.world.game.units[&far].hp, before);
        place(&mut m, far, at(60. + range, 60.), PlaneState::Home);
        m.step_aircraft_anti_air(&[gun], 0.1).unwrap();
        close(m.world.game.units[&far].hp, before - dps * 0.1);
        for gate in ["hold", "dead", "unfinished", "setup", "parked"] {
            let original = m.world.game.units[&gun].clone();
            m.commands.units.get_mut(&gun).unwrap().hold_fire = gate == "hold";
            let u = m.world.game.units.get_mut(&gun).unwrap();
            match gate {
                "dead" => u.hp = 0.,
                "unfinished" => u.built = Some(0.5),
                "setup" if ["mg", "flak"].contains(&kind) => u.still = Some(1.99),
                "parked" if kind == "fighter" => u.air = Some(PlaneState::Rearm),
                "setup" | "parked" => continue,
                _ => {}
            }
            let hp = m.world.game.units[&far].hp;
            m.step_aircraft_anti_air(&[gun], 0.1).unwrap();
            close(m.world.game.units[&far].hp, hp);
            m.world.game.units.insert(gun, original);
        }
        m.commands.units.get_mut(&gun).unwrap().hold_fire = false;
        m.aviation.take_events();
        m.world.game.units.get_mut(&far).unwrap().hp = dps * 0.15;
        m.step_aircraft_anti_air(&[gun], 0.1).unwrap();
        assert!(m.world.game.units[&far].hp > 0.);
        m.step_aircraft_anti_air(&[gun], 0.1).unwrap();
        m.step_aircraft_anti_air(&[gun], 0.1).unwrap();
        assert!(m.world.game.units[&far].hp <= 0.);
        assert_eq!(
            m.aviation
                .take_events()
                .iter()
                .filter(|event| matches!(event,AviationEvent::PlaneDown{target,..} if *target==far))
                .count(),
            1,
            "{kind} reports one down"
        );
    }
}

#[test]
fn mov89_aa_spends_physical_rounds_and_respects_forced_and_empty_controls() {
    let mut m = scene();
    let gun = ground(&mut m, "flak");
    m.world.game.units.get_mut(&gun).unwrap().still = Some(2.);
    let target = m.spawn_aircraft(1, "bomber").unwrap();
    place(&mut m, target, at(65., 60.), PlaneState::Station);
    m.world.game.players[0].visible = vec![target];
    m.logistics.initialize_unit(
        ww2_core::logistics::Context {
            enabled_override: Some(true),
            scenario: false,
            mode: ww2_core::logistics::Mode::Conquest,
        },
        ww2_core::logistics::UnitFacts {
            id: gun,
            kind: "flak",
            owner: Some(0),
            owner_exists: true,
            guard_home: false,
            riding: false,
        },
        Some(&ww2_core::logistics::Definition::from_catalog("flak").unwrap()),
    );
    let initial = m.logistics.get(gun).unwrap().clone();
    for (ammo, forced, stranded, allowed) in [
        (0., false, false, false),
        (2., true, false, false),
        (2., true, true, true),
        (2., false, false, true),
    ] {
        let r = m.logistics.get_mut(gun).unwrap();
        *r = initial.clone();
        r.ammo = ammo;
        r.forced = forced;
        r.stranded = stranded;
        let hp = m.world.game.units[&target].hp;
        m.step_aircraft_anti_air(&[gun], 0.1).unwrap();
        assert_eq!(m.world.game.units[&target].hp < hp, allowed);
    }
    *m.logistics.get_mut(gun).unwrap() = initial;
    m.logistics.get_mut(gun).unwrap().ammo = 1.;
    m.aviation.anti_air.remove(&gun);
    let interval = Catalog::compiled()
        .unwrap()
        .unit("flak")
        .unwrap()
        .get("w")
        .unwrap()
        .get("interval")
        .unwrap()
        .number()
        .unwrap();
    let hp = m.world.game.units[&target].hp;
    m.step_aircraft_anti_air(&[gun], interval).unwrap();
    close(m.world.game.units[&target].hp, hp - 25. * interval / 2.);
    close(m.logistics.get(gun).unwrap().ammo, 1.);
    m.step_aircraft_anti_air(&[gun], interval).unwrap();
    close(m.logistics.get(gun).unwrap().ammo, 0.);
    let hp = m.world.game.units[&target].hp;
    m.step_aircraft_anti_air(&[gun], interval).unwrap();
    close(m.world.game.units[&target].hp, hp);
}

#[test]
fn mov86_component_readiness_matrix_preserves_rejected_sorties() {
    for kind in ["fighter", "attacker", "bomber"] {
        let mut m = scene();
        let id = m.spawn_aircraft(0, kind).unwrap();
        m.commands.units.get_mut(&id).unwrap().hold_fire = true;
        let initial = m.aviation.planes[&id];
        let mission = PlaneMission {
            kind: MissionKind::Patrol,
            target: None,
            at: at(80., 80.),
        };
        for state in [
            PlaneState::Base,
            PlaneState::Rearm,
            PlaneState::Out,
            PlaneState::Station,
            PlaneState::Home,
        ] {
            for (fuel, ammo) in [
                (initial.fuel, initial.ammo),
                (0., initial.ammo),
                (initial.fuel, 0.),
                (0., 0.),
            ] {
                m.world.game.units.get_mut(&id).unwrap().air = Some(state);
                *m.aviation.planes.get_mut(&id).unwrap() = initial;
                m.aviation.planes.get_mut(&id).unwrap().fuel = fuel;
                m.aviation.planes.get_mut(&id).unwrap().ammo = ammo;
                let before = m.aviation.planes[&id];
                let ready = state != PlaneState::Rearm && fuel > 0. && ammo > 0.;
                assert_eq!(
                    m.send_aircraft_mission(0, id, mission).unwrap(),
                    ready,
                    "{kind}: {state:?}, fuel={fuel}, ammo={ammo}"
                );
                if ready {
                    assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Out));
                    assert_eq!(
                        m.aviation.planes[&id],
                        ww2_core::match_aviation::AircraftRuntime {
                            mission: Some(mission),
                            ..before
                        }
                    );
                } else {
                    assert_eq!(m.world.game.units[&id].air, Some(state));
                    assert_eq!(m.aviation.planes[&id], before);
                }
            }
        }
    }
}

#[test]
fn mov86_component_hold_fire_fighter_only_hits_its_explicit_attack_target() {
    let mut m = scene();
    let ally = ground(&mut m, "rifle");
    let fighter = m.spawn_aircraft(0, "fighter").unwrap();
    let near = m.spawn_aircraft(1, "bomber").unwrap();
    let ordered = m.spawn_aircraft(1, "bomber").unwrap();
    place(&mut m, fighter, at(60., 60.), PlaneState::Station);
    place(&mut m, near, at(62., 60.), PlaneState::Station);
    place(&mut m, ordered, at(70., 60.), PlaneState::Station);
    m.world.game.players[0].visible = vec![near, ordered];
    m.commands.units.get_mut(&fighter).unwrap().hold_fire = true;
    let near_hp = m.world.game.units[&near].hp;
    let ordered_hp = m.world.game.units[&ordered].hp;
    for kind in [
        MissionKind::Patrol,
        MissionKind::Escort,
        MissionKind::Attack,
    ] {
        let target = match kind {
            MissionKind::Patrol => None,
            MissionKind::Escort => Some(ally),
            MissionKind::Attack => Some(ordered),
            MissionKind::Area => unreachable!(),
        };
        assert!(
            m.send_aircraft_mission(
                0,
                fighter,
                PlaneMission {
                    kind,
                    target,
                    at: at(70., 60.)
                }
            )
            .unwrap()
        );
        m.step_aircraft_anti_air(&[fighter], 0.1).unwrap();
        close(m.world.game.units[&near].hp, near_hp);
        close(
            m.world.game.units[&ordered].hp,
            ordered_hp
                - if kind == MissionKind::Attack {
                    3.0
                } else {
                    0.0
                },
        );
    }
    // Losing visibility never permits a held fighter to switch to a nearer enemy.
    m.world.game.players[0].visible = vec![near];
    m.step_aircraft_anti_air(&[fighter], 0.1).unwrap();
    close(m.world.game.units[&near].hp, near_hp);
    close(m.world.game.units[&ordered].hp, ordered_hp - 3.0);
}

#[test]
fn mac_air_weather_survives_ordinary_match_finish_and_every_seat_snapshot() {
    use ww2_core::{
        match_commands::Command, match_end::EndReason, recipient_snapshot::DeliveryState,
        weather::WeatherKind,
    };
    for weather in [WeatherKind::Mud, WeatherKind::Snow] {
        // The authored point uses the highest admitted income; capture and victory stay ordinary.
        let mut m = scene_with(WeatherChoice::Fixed(weather), json!({"x":10,"y":10,"vp":5}));
        assert_eq!(m.start.weather.now, weather);
        assert!(m.finish.is_none());
        assert!(m.start.economy.players.iter().all(|p| p.vp == 0.));
        let ids: Vec<_> = m
            .world
            .game
            .units
            .values()
            .filter(|u| u.owner == 0 && m.rules.kinds[u.kind].infantry)
            .map(|u| u.id)
            .collect();
        let command = Command::from_json(&json!({"t":"move","ids":ids,"x":21.,"z":21.})).unwrap();
        m.enqueue_command(
            ww2_core::orders::OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 0,
            },
            command,
        )
        .unwrap();
        let mut delivery = DeliveryState::new(&m).unwrap();
        let mut snapshots = 0;
        for _ in 0..10000 {
            let report = m.advance_tick().unwrap();
            assert_eq!(
                m.world.game.weather,
                Some(m.rules.weather_kind(weather.name()).unwrap())
            );
            assert_eq!(m.start.weather.now, weather);
            assert!(m.tick_state.fault().is_none());
            let effects: Vec<_> = report.environment().cloned().collect();
            let frame = delivery
                .capture_with_lists(&mut m, &[false, false], &[false, false], &effects)
                .unwrap();
            for slot in 0..2 {
                let snapshot = frame.recipient(slot).unwrap().into_json();
                assert_eq!(snapshot["t"], "s");
                assert_eq!(snapshot["weather"], json!([weather.name()]));
                assert!(snapshot["units"].is_array());
                snapshots += 1;
            }
            if m.finish
                .is_some_and(|finish| m.world.game.tick >= finish.end_tick + 5)
            {
                break;
            }
        }
        let finish = m.finish.expect("ordinary point income reaches victory");
        assert_eq!(finish.reason, EndReason::VictoryPoints);
        assert_eq!(finish.winner, 0);
        assert!(finish.end_tick > 1);
        assert!(m.world.game.tick >= finish.end_tick + 5);
        assert_eq!(snapshots, m.world.game.tick * 2);
        assert!(m.aviation.planes.is_empty());
        assert!(m.firing.flights.is_empty());
        println!(
            "weather={} finish_tick={} final_tick={} snapshots={}",
            weather.name(),
            finish.end_tick,
            m.world.game.tick,
            snapshots
        );
    }
}
