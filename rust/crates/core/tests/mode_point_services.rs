//! MODE-24 component intent on canonical Conquest matches and shipped map content.
//! Ownership, cut state, damage and proximity are explicit phase inputs. This does
//! not claim a capture-to-supply-to-recovery whole-match journey.
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::UnitControl,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    unit_spawn,
    weather::WeatherChoice,
};

fn make(name: &str) -> Match {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../server/src/authoritative-maps")
        .join(format!("{name}.json"));
    let map = AuthoritativeMap::from_json(&std::fs::read(path).unwrap()).unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: [0, 0, 1]
                .into_iter()
                .enumerate()
                .map(|(slot, team)| PlayerConfig {
                    name: format!("P{slot}"),
                    team,
                    faction: Faction::Germany,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(31),
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

fn rule(name: &str) -> f64 {
    Catalog::compiled()
        .unwrap()
        .table("CFG")
        .unwrap()
        .get(name)
        .unwrap()
        .number()
        .unwrap()
}

#[test]
fn mode24_radio_accelerates_only_the_uncut_holding_team() {
    // Source shared/sim.js:5634-5636: all cooldowns, with no stacking by radio count.
    assert_eq!(rule("radioCd"), 1.5);
    let mut m = make("the-great-bridge");
    let radio = m
        .start
        .points
        .iter()
        .position(|point| point.kind.as_deref() == Some("radio"))
        .unwrap();
    for (owner, cut) in [
        (Some(0), false),
        (Some(1), false),
        (Some(2), false),
        (None, false),
        (Some(0), true),
        (Some(2), true),
    ] {
        m.start.points[radio].owner = owner;
        m.start.points[radio].cut = cut;
        for player in &mut m.start.players {
            assert!(!player.support.is_empty());
            for remaining in player.support.values_mut() {
                *remaining = 0.25;
            }
        }
        m.advance_support_timers(0.5).unwrap();
        for (slot, player) in m.start.players.iter().enumerate() {
            let accelerated = !cut && owner.is_some_and(|owner| m.world.game.allied(owner, slot));
            let expected = 0.25 - 0.5 * if accelerated { 1.5 } else { 1.0 };
            for remaining in player.support.values() {
                assert_eq!(
                    *remaining, expected,
                    "owner={owner:?}, cut={cut}, slot={slot}"
                );
            }
        }
    }
}

#[test]
fn mode24_depot_reinforces_and_repairs_only_allies_within_home_radius() {
    // Source shared/sim.js:5648-5650 and the following reinforce loop. Keep every
    // constructor actor. The added damaged vehicle is an explicit recovery input,
    // initialized by the public unit constructor, not a recruitment assertion.
    let mut m = make("the-great-bridge");
    let depot = m
        .start
        .points
        .iter()
        .position(|point| point.kind.as_deref() == Some("depot"))
        .unwrap();
    let at = m.start.points[depot].at;
    let rifle = m
        .world
        .game
        .units
        .values()
        .find(|unit| unit.owner == 0 && m.rules.kinds[unit.kind].name == "rifle")
        .unwrap()
        .id;
    let vehicle = m.next_unit_id;
    m.next_unit_id += 1;
    let kind = m.rules.kind("tank").unwrap();
    let unit =
        unit_spawn::initial_unit(&m.rules, Catalog::compiled().unwrap(), kind, vehicle, 0, at)
            .unwrap();
    m.commands
        .insert(vehicle, UnitControl::new(&unit, &m.rules).unwrap());
    m.combat.initialize(vehicle);
    m.firing.initialize(vehicle);
    m.abilities.initialize(vehicle);
    m.recovery.initialize(vehicle);
    m.world.game.units.insert(vehicle, unit);
    let catalog = Catalog::compiled().unwrap();
    let rifle_def = catalog.unit("rifle").unwrap();
    let hp_per = rifle_def.get("hpPer").unwrap().number().unwrap();
    let models = rifle_def.get("models").unwrap().number().unwrap();
    let rifle_cost = rifle_def.get("cost").unwrap().number().unwrap() / models * 0.5;
    let rifle_hp = (models - 2.0) * hp_per;
    let vehicle_full = m.world.game.units[&vehicle].hp;
    let radius = rule("reinforceRadius");
    let dt = rule("reinforceEvery");
    let cases = [
        (Some(0), false, 0.0, true),
        (Some(1), false, 0.0, true),
        (Some(2), false, 0.0, false),
        (None, false, 0.0, false),
        (Some(0), true, 0.0, false),
        (Some(1), true, 0.0, false),
        (Some(0), false, radius, true),
        (Some(0), false, radius + 0.001, false),
    ];
    for (owner, cut, distance, available) in cases {
        m.start.points[depot].owner = owner;
        m.start.points[depot].cut = cut;
        let position = Point {
            x: at.x + distance,
            z: at.z,
        };
        for id in [rifle, vehicle] {
            let unit = m.world.game.units.get_mut(&id).unwrap();
            unit.x = position.x;
            unit.z = position.z;
            unit.hp = if id == rifle {
                rifle_hp
            } else {
                vehicle_full - 100.0
            };
            m.recovery.get_mut(id).unwrap().reinforcement = 0.0;
        }
        // The uncut depot is the only possible home source for these actors.
        for base in m
            .world
            .game
            .units
            .values()
            .filter(|unit| unit.owner == 0 && m.rules.kinds[unit.kind].name == "hq")
        {
            assert!(ww2_core::math::hypot(base.x - position.x, base.z - position.z) > radius);
        }
        m.grid.rebuild(&m.world.game.units);
        m.start.economy.players[0].mp = 1000.0;
        let story_before = m.story[0].mp_spent;
        let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
        let frame = m.prepare_recovery(&ids).unwrap();
        m.reinforce(&frame, dt).unwrap();
        let repaired = if available {
            (18.0 * dt).min(100.0)
        } else {
            0.0
        };
        let spent = if available {
            rifle_cost + repaired * 0.5
        } else {
            0.0
        };
        assert_eq!(
            m.world.game.units[&rifle].hp,
            rifle_hp + if available { hp_per } else { 0.0 }
        );
        assert_eq!(
            m.world.game.units[&vehicle].hp,
            vehicle_full - 100.0 + repaired
        );
        assert_eq!(m.start.economy.players[0].mp, 1000.0 - spent);
        assert_eq!(m.story[0].mp_spent, story_before + spent);
    }
}

#[test]
fn mode24_all_named_shipped_maps_keep_their_point_kinds() {
    for (map, radios, depots) in [
        ("hurtgen-forest", 2, 0),
        ("seelow-heights", 1, 0),
        ("prokhorovka", 0, 2),
        ("el-alamein", 0, 1),
        ("the-great-bridge", 1, 1),
    ] {
        let m = make(map);
        let count = |kind| {
            m.start
                .points
                .iter()
                .filter(|point| point.kind.as_deref() == Some(kind))
                .count()
        };
        assert_eq!(count("radio"), radios, "{map}");
        assert_eq!(count("depot"), depots, "{map}");
    }
}
