//! Bridge failure keeps structural support, casualties and rail queries in sync.
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    environment::{Context, DamageContext},
    game::Point,
    map::AuthoritativeMap,
    match_commands::UnitControl,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_world_rails::WorldRails,
    recipient_snapshot::{DeliveryState, SelectedSeat},
    unit_spawn,
    weather::WeatherChoice,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../tests/fixtures/misc-bridge.json")).unwrap()
}

fn scene(source: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(source).unwrap()).unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: [Faction::Usa, Faction::Germany]
                .into_iter()
                .enumerate()
                .map(|(slot, faction)| PlayerConfig {
                    name: format!("P{slot}"),
                    team: slot as i32,
                    faction,
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
    .unwrap()
}

fn add_units(m: &mut Match, rows: &Value) {
    for row in rows.as_array().unwrap() {
        let id = row["id"].as_u64().unwrap() as u32;
        let kind = m.rules.kind(row["type"].as_str().unwrap()).unwrap();
        let u = unit_spawn::initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            kind,
            id,
            0,
            Point {
                x: row["x"].as_f64().unwrap(),
                z: row["z"].as_f64().unwrap(),
            },
        )
        .unwrap();
        m.commands
            .insert(id, UnitControl::new(&u, &m.rules).unwrap());
        m.combat.initialize(id);
        m.firing.initialize(id);
        m.abilities.initialize(id);
        m.recovery.initialize(id);
        m.world.game.units.insert(id, u);
        m.next_unit_id = m.next_unit_id.max(id + 1);
    }
    for c in m.commands.units.values_mut() {
        c.auto_cast = false;
        c.auto_retreat = false;
        c.hold_fire = true;
    }
    m.grid.rebuild(&m.world.game.units);
}

fn rails(m: &Match, f: &Value) -> WorldRails {
    WorldRails::from_metadata(
        Some(&json!({"stations":[],"lines":f["lines"]})),
        &m.world.game.chars,
        60,
        60,
        4,
    )
    .unwrap()
}

fn assert_stage(m: &Match, rails: &WorldRails, stage: &Value) {
    for cell in stage["cells"].as_array().unwrap() {
        let c = cell[0].as_u64().unwrap() as usize;
        assert_eq!(
            m.world.game.chars[c],
            cell[1].as_str().unwrap().as_bytes()[0]
        );
        assert_eq!(
            m.world.game.sections[&(c as u32)].hp,
            cell[2].as_f64().unwrap()
        );
        assert_eq!(
            m.world.game.sections[&(c as u32)].failed,
            cell[3] == "failed"
        );
        assert_eq!(
            f64::from(m.world.game.cell_hp[c]),
            cell[4].as_f64().unwrap()
        );
    }
    for (i, hp) in stage["units"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            m.world.game.units[&(901 + i as u32)].hp,
            hp.as_f64().unwrap()
        );
    }
    for team in 0..=1 {
        for (i, line) in rails.lines.iter().enumerate() {
            assert_eq!(
                line.is_open(&m.world.game.chars, &[0, 0, 0, 1], team),
                stage["open"][team as usize][i].as_bool().unwrap()
            );
        }
    }
}

#[test]
fn eng58_bridge_failure_and_live_rail_connectivity_match_pinned_component() {
    let f = fixture();
    let mut m = scene(&f["map"]);
    add_units(&mut m, &f["units"]);
    let rails = rails(&m, &f);
    assert_stage(&m, &rails, &f["stages"][0]);
    let mut collapses = Vec::new();
    for (i, damage) in [199.0, 1.0].into_iter().enumerate() {
        let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
        let mut context = Context {
            tick: 10,
            seed: &mut m.start.environment_seed,
            next_id: &mut m.next_unit_id,
            unit_order: &order,
            grid: &m.grid,
            rain: 0.0,
            wet: 0.0,
            wind_angle: 0.0,
            wind_strength: 0.3,
        };
        let (taken, effects) = m
            .environment
            .damage_section(
                &mut m.world,
                25 * 60 + 25,
                damage,
                &mut context,
                &DamageContext::plain(&BTreeSet::new()),
            )
            .unwrap();
        assert_eq!(taken, damage);
        collapses.extend(effects.into_iter().filter_map(|effect| match effect {
            crate::environment::Effect::Collapse(c) => c.section,
            _ => None,
        }));
        assert_stage(&m, &rails, &f["stages"][i + 1]);
        assert_eq!(json!(collapses), f["stages"][i + 1]["collapses"]);
    }
    // A repaired physical deck restores connectivity without rebuilding the rail graph.
    for x in [25, 26] {
        m.world.set_cell(25 * 60 + x, b'=').unwrap();
    }
    assert!(rails.lines[0].is_open(&m.world.game.chars, &[0, 0, 0, 1], 0));
    assert!(!rails.lines[0].is_open(&m.world.game.chars, &[0, 1, 0, 1], 0));
    assert!(!rails.lines[0].is_open(&[], &[0, 0, 0, 1], 0));
    assert!(!rails.lines[0].is_open(&m.world.game.chars, &[], 0));
}

#[test]
fn eng58_bridge_collapse_survives_ordinary_ticks_and_every_seat_snapshot() {
    let f = fixture();
    let mut source = f["map"].clone();
    source["triggers"] = json!([{"at":0.05,"blow":[25,25,25,25]}]);
    let mut m = scene(&source);
    add_units(&mut m, &f["units"]);
    let rails = rails(&m, &f);
    let mut delivery = DeliveryState::new(&m).unwrap();
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    for tick in 1..=6 {
        let report = m.advance_tick().unwrap();
        assert_eq!(report.tick, tick);
        assert!(!rails.lines[0].is_open(&m.world.game.chars, &[0, 0, 0, 1], 0));
        assert!(rails.lines[1].is_open(&m.world.game.chars, &[0, 0, 0, 1], 0));
        let frame = delivery
            .capture_selected_with_tick_events(&mut m, &selected, &report.events)
            .unwrap()
            .unwrap();
        for seat in 0..2 {
            frame.recipient(seat).unwrap().into_json();
        }
        assert!(!m.world.game.units.contains_key(&901));
        assert!(!m.world.game.units.contains_key(&902));
        assert!(m.world.game.units[&903].hp > 0.0);
        for x in [25, 26] {
            assert_eq!(m.world.game.chars[25 * 60 + x], b'W');
            assert!(m.world.game.sections[&((25 * 60 + x) as u32)].failed);
        }
        for x in [24, 28] {
            assert_eq!(m.world.game.chars[25 * 60 + x], b'=');
            assert!(!m.world.game.sections[&((25 * 60 + x) as u32)].failed);
        }
    }
}
