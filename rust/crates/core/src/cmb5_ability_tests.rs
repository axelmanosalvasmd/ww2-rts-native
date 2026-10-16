//! Ability payments run in Classic; World recipient admission stays guarded.
use crate::{
    catalog::{Catalog, Value},
    economy::{self, AbilityCost, Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Denial, UnitControl},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    random::MatchRandom,
    recipient_snapshot::{SnapshotCache, SnapshotError},
    unit_spawn::initial_unit,
    weather::WeatherChoice,
};
use serde_json::json;

fn scene(mode: Mode) -> Match {
    let world = mode == Mode::World;
    let side = if world { 512 } else { 96 };
    let mut raw = json!({
        "name":"Ability payment", "w":side,"h":side,
        "rows":vec![".".repeat(side);side],
        "spawns":[{"x":12,"y":12},{"x":side-13,"y":side-13}]
    });
    if world {
        raw["points"] = json!([]);
        raw["world"] = json!({
            "size":"huge","total":64,"regionCells":64,
            "regions":(0..64).map(|i| {
                let x = i % 8 * 64;
                let y = i / 8 * 64;
                let mut region = json!({"id":i,"x":x+32,"y":y+32,
                    "bounds":[x,y,x+64,y+64],"kind":"rural"});
                if i == 0 { region["home"] = json!(0); }
                if i == 63 { region["home"] = json!(1); }
                region
            }).collect::<Vec<_>>()
        });
    } else {
        raw["points"] = json!([{"x":48,"y":48,"vp":0,"mp":0}]);
    }
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let options = MatchOptions {
        seed: 701,
        mode,
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
    };
    if world {
        Match::new_world(&map, options, MatchRandom::new(701)).unwrap()
    } else if mode == Mode::Classic {
        Match::new_classic(&map, options).unwrap()
    } else {
        Match::new(&map, options).unwrap()
    }
}

fn actor(m: &mut Match, name: &str) -> u32 {
    let id = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && !m.rules.kinds[u.kind].structure)
        .unwrap()
        .id;
    let at = m.world.game.units[&id].at();
    let unit = initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        m.rules.kind(name).unwrap(),
        id,
        0,
        at,
    )
    .unwrap();
    let mut control = UnitControl::new_for_mode(&unit, &m.rules, m.options.mode).unwrap();
    control.auto_cast = false;
    control.auto_retreat = false;
    control.hold_fire = true;
    m.commands.units.insert(id, control);
    m.world.game.units.insert(id, unit);
    *m.abilities.get_mut(id).unwrap() = Default::default();
    *m.firing.get_mut(id).unwrap() = Default::default();
    m.grid.rebuild(&m.world.game.units);
    id
}

#[test]
fn cmb112_cost_table_and_catalog_overrides_are_exact() {
    for mode in [
        Mode::Classic,
        Mode::World,
        Mode::Conquest,
        Mode::Default,
        Mode::Assault,
        Mode::Annihilation,
        Mode::Horde,
        Mode::Tutorial,
        Mode::Other,
    ] {
        for (id, cost) in [
            ("grenade", 15.),
            ("suppress", 10.),
            ("ap", 15.),
            ("smoke", 10.),
            ("satchel", 30.),
            ("ura", 10.),
            ("barrage", 25.),
            ("none", 0.),
            ("unknown", 0.),
        ] {
            assert_eq!(
                economy::ab_cost(
                    mode,
                    AbilityCost {
                        id,
                        munitions: None
                    }
                )
                .unwrap(),
                if mode.is_construction() { cost } else { 0. }
            );
        }
        for (name, cost) in [
            ("rocket", 0.),
            ("mortar", 15.),
            ("howitzer", 25.),
            ("destroyer", 40.),
        ] {
            let ab = Catalog::compiled()
                .unwrap()
                .unit(name)
                .unwrap()
                .get("ab")
                .unwrap();
            assert_eq!(ab.get("mun").and_then(Value::number), Some(cost));
            assert_eq!(
                economy::ab_cost(
                    mode,
                    AbilityCost {
                        id: ab.get("id").and_then(Value::string).unwrap(),
                        munitions: ab.get("mun").and_then(Value::number),
                    }
                )
                .unwrap(),
                if mode.is_construction() { cost } else { 0. }
            );
        }
    }
}

#[test]
fn cmb112_classic_and_world_charge_on_use_and_aimed_arrival() {
    for mode in [Mode::Classic, Mode::World] {
        let mut m = scene(mode);
        for (name, cost, aimed) in [
            ("mg", 10., false),
            ("at", 15., false),
            ("tank", 10., false),
            ("conscript", 10., false),
            ("rifle", 15., true),
            ("ranger", 30., true),
            ("rocket", 0., true),
            ("mortar", 15., true),
            ("howitzer", 25., true),
        ] {
            let id = actor(&mut m, name);
            m.start.economy.players[0].mun = 100.;
            let at = m.world.game.units[&id].at();
            let target = Point {
                x: at.x + 2.,
                z: at.z,
            };
            assert_eq!(
                m.apply_ability_selection(0, &[Some(id)], Some(target), false, false),
                Ok(None),
                "{mode:?} {name}"
            );
            assert_eq!(
                m.start.economy.players[0].mun,
                if aimed { 100. } else { 100. - cost }
            );
            if aimed {
                assert_eq!(m.commands.units[&id].nade, Some(target));
                // The target remains pending while the actor is outside ability range.
                let unit = m.world.game.units.get_mut(&id).unwrap();
                unit.x += 150.;
                unit.repath = 1.;
                m.step_aimed_ability(id).unwrap();
                assert_eq!(m.start.economy.players[0].mun, 100.);
                assert_eq!(m.commands.units[&id].nade, Some(target));
                m.world.game.units.get_mut(&id).unwrap().x = at.x;
                m.step_aimed_ability(id).unwrap();
                assert_eq!(m.commands.units[&id].nade, None);
                assert_eq!(
                    m.start.economy.players[0].mun,
                    100. - cost,
                    "{mode:?} {name}"
                );
                m.step_aimed_ability(id).unwrap();
                assert_eq!(m.start.economy.players[0].mun, 100. - cost);
            }
            assert!(m.abilities.get(id).unwrap().cooldown > 0.);
        }
        let id = actor(&mut m, "mg");
        m.start.economy.players[0].mun = 9.;
        assert_eq!(
            m.apply_ability_selection(0, &[Some(id)], None, false, false),
            Ok(Some(Denial::Munitions))
        );
        assert_eq!(m.start.economy.players[0].mun, 9.);
        assert_eq!(m.abilities.get(id).unwrap().buff, 0.);

        let id = actor(&mut m, "rifle");
        m.start.economy.players[0].mun = 15.;
        let target = m.world.game.units[&id].at();
        assert_eq!(
            m.apply_ability_selection(0, &[Some(id)], Some(target), false, false),
            Ok(None)
        );
        // Another purchase consumed the wallet while the aimed order waited.
        m.start.economy.players[0].mun = 14.;
        let flights = m.firing.flights.len();
        m.step_aimed_ability(id).unwrap();
        assert_eq!(m.start.economy.players[0].mun, 14.);
        assert_eq!(m.commands.units[&id].nade, None);
        assert_eq!(m.firing.flights.len(), flights);
        assert_eq!(m.abilities.get(id).unwrap().cooldown, 0.);

        // Classic ticks are represented; its recipient adapter and World hosting stay guarded.
        assert!(matches!(
            SnapshotCache::new(&m),
            Err(SnapshotError::UnsupportedMode)
        ));
        if mode == Mode::Classic {
            m.firing.take_events();
            m.validate_tick_state().unwrap();
            for _ in 0..4 {
                m.advance_tick().unwrap();
                assert!(m.tick_state.fault().is_none());
            }
        }
    }
}

#[test]
fn cmb112_free_ordinary_abilities_survive_ticks_and_recipient_snapshots() {
    let mut m = scene(Mode::Conquest);
    crate::cmb5_match_tests::prepare_finish(&mut m);
    let id = actor(&mut m, "mg");
    m.start.economy.players[0].mun = 0.;
    assert_eq!(
        m.apply_ability_selection(0, &[Some(id)], None, false, false),
        Ok(None)
    );
    assert!(m.abilities.get(id).unwrap().buff > 0.);
    for _ in 0..4 {
        m.advance_tick().unwrap();
        assert!(m.tick_state.fault().is_none());
        for recipient in 0..2 {
            SnapshotCache::new(&m)
                .unwrap()
                .recipient(recipient)
                .unwrap();
        }
        assert_eq!(m.start.economy.players[0].mun, 0.);
        assert!(m.abilities.get(id).unwrap().buff > 0.);
    }
    crate::cmb5_match_tests::through_finish(&mut m, |_, _| {});
}

#[test]
fn cmb112_paid_world_ability_survives_later_core_ticks() {
    let mut m = scene(Mode::World);
    let id = actor(&mut m, "mg");
    m.start.economy.players[0].mun = 10.;
    assert_eq!(
        m.apply_ability_selection(0, &[Some(id)], None, false, false),
        Ok(None)
    );
    assert_eq!(m.start.economy.players[0].mun, 0.);
    for _ in 0..4 {
        m.advance_tick().unwrap();
        assert!(m.tick_state.fault().is_none());
        assert!(m.abilities.get(id).unwrap().buff > 0.);
        assert!(matches!(
            SnapshotCache::new(&m),
            Err(SnapshotError::UnsupportedMode)
        ));
    }
}
