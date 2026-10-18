//! Currency initialization, owner projection and ordinary payment lifecycles.
use crate::{
    economy::{self, Currency, Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_support::SupportDenial,
    recipient_snapshot::{DeliveryState, SnapshotCache},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene(mode: Mode) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Economy intent", "w":128,"h":128,
            "rows":vec![".".repeat(128);128],
            "spawns":[{"x":18,"y":18},{"x":109,"y":109}],
            "defend":[0],
            "points":[{"x":64,"y":64,"mp":1,"vp":1}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut options = MatchOptions {
        seed: 701,
        mode,
        players: (0..2)
            .map(|slot| PlayerConfig {
                name: format!("p{slot}"),
                team: slot,
                faction: if slot == 0 {
                    Faction::Usa
                } else {
                    Faction::Germany
                },
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
    };
    match mode {
        Mode::Assault => Match::new_assault(&map, options, 0),
        Mode::Annihilation => Match::new_annihilation(&map, options),
        Mode::Classic => Match::new_classic(&map, options),
        Mode::Horde => Match::new_horde(&map, options),
        Mode::Tutorial => {
            options.players.truncate(1);
            let tutorial = AuthoritativeMap::from_json(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../server/src/authoritative-maps/tutorial.json"
            )))
            .unwrap();
            Match::new_tutorial(&tutorial, options)
        }
        _ => Match::new(&map, options),
    }
    .unwrap()
}

fn snapshots_after_ticks(m: &mut Match) {
    let count = m.start.players.len();
    let mut delivery = DeliveryState::new(m).unwrap();
    for _ in 0..4 {
        m.advance_tick().unwrap();
        let frame = delivery
            .capture(m, &vec![false; count], &vec![false; count])
            .unwrap();
        for slot in 0..count {
            frame.recipient(slot).unwrap();
        }
    }
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn base01_currency_initialization_and_owner_flooring_follow_mode() {
    for mode in [
        Mode::Default,
        Mode::Conquest,
        Mode::Assault,
        Mode::Annihilation,
        Mode::Classic,
    ] {
        let mut m = scene(mode);
        for slot in 0..2 {
            let p = &m.start.economy.players[slot];
            let initial = match mode {
                Mode::Classic => 200.,
                Mode::Annihilation => 300.,
                Mode::Assault => {
                    if slot == 0 {
                        250.
                    } else {
                        320.
                    }
                }
                _ => 150.,
            };
            assert_eq!(p.mp, initial, "{mode:?} seat {slot}");
            assert_eq!((p.mun, p.fuel), (0., 0.));
            let fields = SnapshotCache::new(&m)
                .unwrap()
                .recipient(slot)
                .unwrap()
                .fields;
            assert_eq!(fields["mp"], json!(initial));
            for key in ["mun", "fuel"] {
                assert_eq!(
                    fields.get(key),
                    mode.is_construction().then_some(&json!(0.)),
                    "{mode:?} {key}"
                );
            }
        }
        for (slot, p) in m.start.economy.players.iter_mut().enumerate() {
            let offset = slot as f64 * 10.;
            p.mp = 42.99 + offset;
            p.mun = 8.99 + offset;
            p.fuel = 6.99 + offset;
            p.inc = 3.26 + offset;
            p.fuel_inc = Some(0.56 + offset);
            p.upkeep = Some(1.24 + offset);
        }
        for slot in 0..2 {
            let f = SnapshotCache::new(&m)
                .unwrap()
                .recipient(slot)
                .unwrap()
                .fields;
            let offset = slot as f64 * 10.;
            assert_eq!(f["mp"], json!(42. + offset));
            assert_eq!(f["inc"], json!(3.3 + offset));
            assert_eq!(f["fuelInc"], json!(0.6 + offset));
            assert_eq!(f["upkeep"], json!(1.2 + offset));
            if mode.is_construction() {
                assert_eq!(f["mun"], json!(8. + offset));
                assert_eq!(f["fuel"], json!(6. + offset));
            } else {
                assert!(!f.contains_key("mun") && !f.contains_key("fuel"));
            }
        }
        snapshots_after_ticks(&mut m);
    }
}

#[test]
fn base01_absent_optional_income_fields_stay_absent() {
    let m = scene(Mode::Conquest);
    for slot in 0..2 {
        let f = SnapshotCache::new(&m)
            .unwrap()
            .recipient(slot)
            .unwrap()
            .fields;
        for key in ["mun", "fuel", "fuelInc", "upkeep"] {
            assert!(!f.contains_key(key), "{key}");
        }
    }
}

#[test]
fn base14_all_support_prices_charge_the_mode_currency_once() {
    for mode in [
        Mode::Classic,
        Mode::Conquest,
        Mode::Assault,
        Mode::Annihilation,
    ] {
        for (name, mun) in [
            ("recon", 25.),
            ("artillery", 60.),
            ("strafe", 80.),
            ("smoke", 20.),
            ("bombing", 100.),
            ("dive", 70.),
            ("para", 90.),
            ("cover", 40.),
        ] {
            let mut m = scene(mode);
            let price = economy::sup_cost(mode, name).unwrap();
            assert_eq!(
                price.currency,
                if mode.is_construction() {
                    Currency::Munitions
                } else {
                    Currency::Manpower
                }
            );
            if mode.is_construction() {
                assert_eq!(price.cost, mun);
            }
            m.start.economy.players[0].mp = 1000.;
            m.start.economy.players[0].mun = 1000.;
            let at = m.start.players[0].spawn;
            let spent = m.story[0].mp_spent;
            assert_eq!(
                m.admit_support(0, name, at, None).unwrap(),
                None,
                "{mode:?} {name}"
            );
            assert_eq!(
                m.start.economy.players[0].mp,
                1000.
                    - if mode.is_construction() {
                        0.
                    } else {
                        price.cost
                    }
            );
            assert_eq!(
                m.start.economy.players[0].mun,
                1000.
                    - if mode.is_construction() {
                        price.cost
                    } else {
                        0.
                    }
            );
            assert_eq!(
                m.story[0].mp_spent,
                spent
                    + if mode.is_construction() {
                        0.
                    } else {
                        price.cost
                    }
            );
            let paid = (
                m.start.economy.players[0].mp,
                m.start.economy.players[0].mun,
            );
            assert_eq!(
                m.admit_support(0, name, at, None).unwrap(),
                Some(SupportDenial::Cooldown)
            );
            assert_eq!(
                (
                    m.start.economy.players[0].mp,
                    m.start.economy.players[0].mun
                ),
                paid
            );
            snapshots_after_ticks(&mut m);
        }
    }
}

#[test]
fn base14_classic_aimed_payment_waits_for_throw_and_rechecks_wallet() {
    for pay in [true, false] {
        let mut m = scene(Mode::Classic);
        let mut uncharged = scene(Mode::Classic);
        uncharged.start.economy.players[0].mp = 123.;
        uncharged.start.economy.players[0].mun = 15.;
        let id = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
            .unwrap()
            .id;
        assert!(!m.commands.units[&id].auto_cast);
        m.commands.units.get_mut(&id).unwrap().hold_fire = true;
        let at = m.world.game.units[&id].at();
        let target = Point {
            x: at.x + 40.,
            z: at.z,
        };
        m.start.economy.players[0].mp = 123.;
        m.start.economy.players[0].mun = 15.;
        let command =
            Command::from_json(&json!({"t":"ability","ids":[id],"x":target.x,"z":target.z}))
                .unwrap();
        assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
        snapshots_after_ticks(&mut m);
        snapshots_after_ticks(&mut uncharged);
        assert_eq!(
            m.start.economy.players[0].mun,
            uncharged.start.economy.players[0].mun
        );
        assert_eq!(m.commands.units[&id].nade, Some(target));
        // A squad arriving in range pays at the throw. A competing purchase can exhaust it first.
        let unit = m.world.game.units.get_mut(&id).unwrap();
        unit.x = target.x - 2.;
        unit.z = target.z;
        unit.path.clear();
        m.grid.rebuild(&m.world.game.units);
        m.start.economy.players[0].mun = if pay { 15. } else { 14. };
        uncharged.start.economy.players[0].mun = m.start.economy.players[0].mun;
        uncharged.advance_tick().unwrap();
        m.advance_tick().unwrap();
        let paid = if pay { 15. } else { 0. };
        assert!(
            (uncharged.start.economy.players[0].mun - m.start.economy.players[0].mun - paid).abs()
                < 1e-10
        );
        assert_eq!(m.commands.units[&id].nade, None);
        assert_eq!(m.abilities.get(id).unwrap().cooldown > 0., pay);
        // The uncommanded run receives the same HQ income. The difference stays
        // exactly one charge after later ticks and every-seat delivery.
        snapshots_after_ticks(&mut m);
        snapshots_after_ticks(&mut uncharged);
        assert!(
            (uncharged.start.economy.players[0].mun - m.start.economy.players[0].mun - paid).abs()
                < 1e-10
        );
    }
    let mut free = scene(Mode::Conquest);
    let rifle = free
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && free.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    assert!(free.commands.units[&rifle].auto_cast);
    free.start.economy.players[0].mun = 0.;
    let target = free.world.game.units[&rifle].at();
    let cmd = Command::from_json(&json!({"t":"ability","ids":[rifle],"x":target.x,"z":target.z}))
        .unwrap();
    assert_eq!(free.execute_command(0, &cmd, false).unwrap(), None);
    snapshots_after_ticks(&mut free);
    assert_eq!(free.start.economy.players[0].mun, 0.);
}

#[test]
fn base01_tutorial_and_horde_initialize_mp_and_omit_construction_currencies() {
    for mode in [Mode::Tutorial, Mode::Horde] {
        let mut m = scene(mode);
        for slot in 0..m.start.players.len() {
            let enemy = slot + 1 == m.start.players.len();
            let initial = if enemy {
                0.
            } else if mode == Mode::Horde {
                250.
            } else {
                150.
            };
            assert_eq!(m.start.economy.players[slot].mp, initial);
            // This is the canonical wallet projection also used by SnapshotCache.
            // Horde's complete frame remains guarded by its separate hosting work.
            let fields = super::economy_fields(&m, slot).unwrap();
            assert_eq!(fields["mp"], json!(initial));
            for key in ["mun", "fuel"] {
                assert!(!fields.contains_key(key));
            }
        }
        if mode == Mode::Tutorial {
            snapshots_after_ticks(&mut m);
        }
    }
}
