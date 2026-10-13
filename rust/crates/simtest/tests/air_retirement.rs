use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{PlaneState, Point},
    match_aviation::{AircraftRuntime, AviationState},
    match_combat::CombatState,
    match_firing::FiringState,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_story::{DeathRecord, StoryKey},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/air-retirement.json")).unwrap()
}
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn snapshot(m: &Match) -> Value {
    json!({
        "units":m.world.game.units.values().map(|u|json!([u.id,bits(u.hp),bits(u.x),bits(u.z),u.air.map(|a|match a {PlaneState::Base=>"base",PlaneState::Out=>"out",PlaneState::Station=>"station",PlaneState::Home=>"home",PlaneState::Rearm=>"rearm"})])).collect::<Vec<_>>(),
        "wallets":m.start.economy.players.iter().map(|p|bits(p.mp)).collect::<Vec<_>>(),
        "story":m.story.iter().map(|p|StoryKey::ALL.map(|k|bits(p.value(k)))).collect::<Vec<_>>(),
        "chars":String::from_utf8(m.world.game.chars.clone()).unwrap(),
        "wrecks":m.world.game.wrecks.iter().map(|w|w.id).collect::<Vec<_>>(),
        "seed":m.start.environment_seed,"nextId":m.next_unit_id,
    })
}
fn scene(f: &Value, case: &Value) -> Match {
    let map = ww2_core::map::AuthoritativeMap::from_json(&serde_json::to_vec(&f["map"]).unwrap())
        .unwrap();
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
    m.recruit_metadata.clear();
    m.combat = CombatState::default();
    m.firing = FiringState::default();
    m.aviation = AviationState::default();
    m.next_unit_id = 1000;
    m.grid.rebuild(&m.world.game.units);
    m.world.game.tick = 10;
    m.start.environment_seed = 123;
    for (slot, p) in m.start.economy.players.iter_mut().enumerate() {
        p.mp = 100.0;
        p.out = case["out"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_u64() == Some(slot as u64));
    }
    for (index, v) in case["units"].as_array().unwrap().iter().enumerate() {
        let id = v["id"].as_u64().unwrap() as u32;
        let owner = v["owner"].as_u64().unwrap() as usize;
        let kind = m.rules.kind(v["kind"].as_str().unwrap()).unwrap();
        let at = Point {
            x: v["x"].as_f64().unwrap(),
            z: v["z"].as_f64().unwrap(),
        };
        // Incoming source scene facts reuse the shared factory; pose is not a placement proof.
        let mut draft = if m.rules.kinds[kind].air {
            StagedUnit::aircraft(
                &m.world.game,
                &m.rules,
                owner,
                kind,
                id,
                index as u32,
                m.start.players[owner].spawn,
                at,
            )
        } else {
            StagedUnit::ordinary(
                &m.world.game,
                &m.rules,
                owner,
                kind,
                id,
                index as u32,
                m.start.players[owner].spawn,
                Some(at),
            )
        }
        .unwrap();
        draft.unit.hp = v["hp"].as_f64().unwrap();
        draft.unit.rot = v["rot"].as_f64().unwrap();
        draft.unit.still = Some(v["still"].as_f64().unwrap());
        let aircraft = (!v["air"].is_null()).then(|| {
            let a = &v["air"];
            draft.unit.air = Some(match a["state"].as_str().unwrap() {
                "base" => PlaneState::Base,
                "out" => PlaneState::Out,
                "station" => PlaneState::Station,
                "home" => PlaneState::Home,
                "rearm" => PlaneState::Rearm,
                _ => unreachable!(),
            });
            AircraftRuntime {
                fuel: a["fuel"].as_f64().unwrap(),
                ammo: a["ammo"].as_f64().unwrap(),
                timer: a["timer"].as_f64().unwrap(),
                orbit: a["ang"].as_f64().unwrap(),
                mission: None,
                downed: false,
            }
        });
        prepare_publication(
            PublicationParts {
                units: &mut m.world.game.units,
                grid: &mut m.grid,
                controls: &mut m.commands.units,
                recruitment: &mut m.recruit_metadata,
                next_id: &mut m.next_unit_id,
            },
            &m.rules,
            m.start.players.len(),
            vec![draft],
        )
        .unwrap()
        .publish_with(|u, _| {
            m.combat.initialize(u.id).last_hit = v["lastHit"].as_i64().map(|n| n as i32);
            m.firing.initialize(u.id);
            if let Some(a) = aircraft {
                m.aviation.planes.insert(u.id, a);
            }
        });
    }
    if case["aa"].as_bool().unwrap() {
        for p in &mut m.world.game.players {
            p.visible = m
                .world
                .game
                .units
                .values()
                .filter(|u| u.owner != p.slot)
                .map(|u| u.id)
                .collect();
        }
        let actors = m.world.game.units.keys().copied().collect::<Vec<_>>();
        m.step_aircraft_anti_air(&actors, 0.05).unwrap();
    }
    m
}
#[test]
fn registered_air_death_matches_original_bounty_story_and_no_wreck() {
    let f = fixture();
    for case in f["cases"].as_array().unwrap() {
        let mut m = scene(&f, case);
        assert_eq!(snapshot(&m), case["before"], "{} before", case["name"]);
        let order = case["order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect::<Vec<_>>();
        let before_events = m.aviation.events.clone();
        let before_fuel = m
            .start
            .economy
            .players
            .iter()
            .map(|p| (p.mun, p.fuel))
            .collect::<Vec<_>>();
        let before = snapshot(&m);
        m.validate_dead_retirement(&order, 11).unwrap();
        assert_eq!(snapshot(&m), before, "preclock admission is read only");
        let result = m.retire_dead(&order).unwrap();
        assert_eq!(
            json!(result.retired.iter().map(|u| u.unit.id).collect::<Vec<_>>()),
            case["dead"]
        );
        assert!(result.shots.is_empty());
        assert_eq!(
            m.aviation.events, before_events,
            "ordinary retirement keeps earlier AA events"
        );
        for record in &result.retired {
            assert!(!m.aviation.planes.contains_key(&record.unit.id));
            assert!(!m.aviation.anti_air.contains_key(&record.unit.id));
            assert!(m.combat.get(record.unit.id).is_some());
            assert!(m.firing.get(record.unit.id).is_some());
            let kind = m.rules.kinds[record.unit.kind].name.clone();
            m.record_death(
                DeathRecord {
                    owner: Some(record.unit.owner),
                    kind: &kind,
                    at: record.unit.at(),
                },
                record.enemy_killer,
            )
            .unwrap();
        }
        assert_eq!(snapshot(&m), case["after"], "{} after", case["name"]);
        assert_eq!(
            m.start
                .economy
                .players
                .iter()
                .map(|p| (p.mun, p.fuel))
                .collect::<Vec<_>>(),
            before_fuel
        );
        let after = snapshot(&m);
        assert!(m.retire_dead(&order).unwrap().retired.is_empty());
        assert_eq!(snapshot(&m), after, "no second bounty or died call");
    }
}
fn protected(m: &Match) -> String {
    format!(
        "{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
        snapshot(m),
        m.combat,
        m.firing,
        m.aviation,
        m.commands.units,
        m.world.versions,
        m.world.journal()
    )
}
#[test]
fn malformed_air_retirement_rejects_before_clock_seed_id_wallet_or_actor_changes() {
    let f = fixture();
    let changes: [fn(&mut Match); 7] = [
        |m| {
            m.aviation.planes.remove(&1000);
        },
        |m| {
            m.aviation.planes.get_mut(&1000).unwrap().fuel = f64::NAN;
        },
        |m| {
            m.world.game.units.get_mut(&1000).unwrap().air = None;
        },
        |m| {
            m.world.game.units.get_mut(&1000).unwrap().x = f64::INFINITY;
        },
        |m| {
            m.combat.get_mut(1000).unwrap().xp = f64::NAN;
        },
        |m| {
            m.world.game.units.get_mut(&1000).unwrap().hp = f64::NAN;
        },
        |m| {
            m.firing.get_mut(1000).unwrap().cooldown = f64::NAN;
        },
    ];
    for change in changes {
        let mut m = scene(&f, &f["cases"][0]);
        change(&mut m);
        let before = protected(&m);
        assert!(m.validate_dead_retirement(&[1000, 1001], 11).is_err());
        assert_eq!(protected(&m), before);
        assert_eq!(m.world.game.tick, 10);
        assert!(m.retire_dead(&[1000, 1001]).is_err());
        assert_eq!(protected(&m), before);
    }
    let m = scene(&f, &f["cases"][0]);
    let before = protected(&m);
    assert!(m.validate_dead_retirement(&[1000, 1001], u64::MAX).is_err());
    assert!(m.validate_dead_retirement(&[1000, 1000], 11).is_err());
    assert_eq!(protected(&m), before);
    assert_eq!(m.world.game.tick, 10);
}
