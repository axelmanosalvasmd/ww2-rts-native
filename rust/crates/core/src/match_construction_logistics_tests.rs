use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    environment::{Context, DamageContext},
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_runtime::{MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene(mode: Mode, logistics: bool) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Repair and cache lifecycle", "w":100,"h":100,
            "rows":vec![".".repeat(100);100], "heights":vec!["0".repeat(100);100],
            "spawns":[{"x":12,"y":30},{"x":12,"y":70},{"x":88,"y":50}],
            "points":[{"x":50,"y":50,"mp":0,"vp":1}]
        }))
        .unwrap(),
    )
    .unwrap();
    let options = MatchOptions {
        seed: 701,
        mode,
        players: [0, 0, 1]
            .into_iter()
            .enumerate()
            .map(|(slot, team)| PlayerConfig {
                name: format!("p{slot}"),
                team,
                faction: Faction::Usa,
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
        logistics,
        sandbox: false,
    };
    let mut m = match mode {
        Mode::Classic => Match::new_classic(&map, options),
        Mode::Annihilation => Match::new_annihilation(&map, options),
        _ => Match::new(&map, options),
    }
    .unwrap();
    for unit in m.world.game.units.values_mut() {
        unit.hold_pos = true;
    }
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    m
}
fn id(m: &Match, owner: usize, name: &str) -> u32 {
    m.world
        .game
        .units
        .values()
        .find(|u| u.owner == owner && m.rules.kinds[u.kind].name == name)
        .unwrap()
        .id
}
fn place(m: &mut Match, unit: u32, at: Point) {
    let u = m.world.game.units.get_mut(&unit).unwrap();
    u.x = at.x;
    u.z = at.z;
    m.grid.update(unit, u);
}
fn capture_tick(m: &mut Match, delivery: &mut DeliveryState) {
    let report = m.advance_tick().unwrap();
    let seats = (0..m.start.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let logistics = m.options.logistics;
    let frame = delivery
        .capture_selected_with_tick_events(m, &seats, &report.events)
        .unwrap()
        .unwrap();
    for seat in &seats {
        let value = frame.recipient(seat.slot).unwrap().into_json();
        assert!(value["units"].is_array());
        if logistics {
            assert_eq!(value["logistics"]["enabled"], true);
            for row in value["logistics"]["units"].as_array().unwrap() {
                assert!(
                    value["units"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|unit| { unit[0] == row["id"] && unit[2] == seat.slot })
                );
            }
        }
    }
    drop(frame);
    assert!(m.tick_state.fault().is_none());
}
fn finish_and_capture(m: &mut Match, delivery: &mut DeliveryState) {
    if matches!(m.options.mode, Mode::Classic | Mode::Annihilation) {
        for unit in m.world.game.units.values_mut().filter(|u| u.owner == 2) {
            // Annihilation survives through its bunker, which is a structure
            // rather than an engineer-built production building.
            if m.rules.kinds[unit.kind].building || m.rules.kinds[unit.kind].name == "bunker" {
                unit.hp = 0.;
            }
        }
    } else {
        m.start.points[0].owner = Some(0);
        m.start.points[0].progress = 1.;
        m.start.win_vp = 0.05;
    }
    let mut finished_at = None;
    for _ in 0..50 {
        capture_tick(m, delivery);
        if m.finish.is_some() {
            let start = *finished_at.get_or_insert(m.world.game.tick);
            if m.world.game.tick >= start + 8 {
                break;
            }
        }
    }
    assert!(
        m.finish.is_some(),
        "mode={:?}, logistics={}, elimination={:?}, scores={:?}",
        m.options.mode,
        m.options.logistics,
        m.elimination,
        m.start
            .economy
            .players
            .iter()
            .map(|p| (p.team, p.out, p.vp))
            .collect::<Vec<_>>()
    );
    assert_eq!(m.finish.as_ref().unwrap().winner, 0);
    assert!(m.world.game.tick >= finished_at.unwrap() + 8);
}

#[test]
fn base30_allied_repair_restores_failed_sections_without_manpower_through_match_delivery() {
    // Prototype sim.js:3710-3722, 5591-5596, 2629-2645.
    for mode in [
        Mode::Default,
        Mode::Conquest,
        Mode::Annihilation,
        Mode::Classic,
    ] {
        for enabled in [false, true] {
            let mut m = scene(mode, enabled);
            let site = id(&m, 0, "hq");
            let builder = if mode == Mode::Classic {
                "engineer"
            } else {
                "rifle"
            };
            let workers = [id(&m, 0, builder), id(&m, 1, builder)];
            let structure = m
                .world
                .structures
                .iter()
                .find(|s| s.entity_id == Some(site))
                .unwrap()
                .clone();
            let section = structure
                .section_ids
                .iter()
                .position(|s| m.world.section_by_id(s).unwrap().0.anchor)
                .unwrap();
            let section_id = &structure.section_ids[section];
            let cell = structure.cells[section];
            let damage = m.world.section_by_id(section_id).unwrap().1.hp;
            let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
            let mut context = Context {
                tick: m.world.game.tick as u64,
                seed: &mut m.start.environment_seed,
                next_id: &mut m.next_unit_id,
                unit_order: &order,
                grid: &m.grid,
                rain: 0.,
                wet: 0.,
                wind_angle: 0.,
                wind_strength: 0.5,
            };
            let skip = Default::default();
            let hit = DamageContext {
                owner: None,
                contact_id: None,
                direction: None,
                actor_order: None,
                debit_entity: true,
                skip_buildings: &skip,
                memory: None,
            };
            m.environment
                .damage_section(&mut m.world, cell, damage, &mut context, &hit)
                .unwrap();
            assert!(m.world.section_by_id(section_id).unwrap().1.failed);
            let at = m.world.game.units[&site].at();
            for (owner, worker) in workers.into_iter().enumerate() {
                place(
                    &mut m,
                    worker,
                    Point {
                        x: at.x + 4.5,
                        z: at.z + owner as f64 * 0.25,
                    },
                );
                assert_eq!(
                    m.execute_assist(owner, &[i64::from(worker)], i64::from(site), false)
                        .unwrap(),
                    None
                );
            }
            let spent = m.story.iter().map(|s| s.mp_spent).collect::<Vec<_>>();
            let full = number(Catalog::compiled().unwrap().unit("hq").unwrap(), "hpPer").unwrap();
            if enabled {
                m.logistics.get_mut(workers[0]).unwrap().provisions = 29.;
                m.logistics.get_mut(workers[1]).unwrap().provisions = 120.;
            }
            let hp_before = m.world.game.units[&site].hp;
            let mut frame = m.begin_construction_tick().unwrap();
            for worker in workers {
                m.step_construction_worker(worker, &mut frame).unwrap();
            }
            m.finish_construction_tick(frame).unwrap();
            let repaired = m.world.game.units[&site].hp - hp_before;
            let rate = if enabled { 0.75 } else { 1. };
            let expected = full * (0.005 * math::pow(2., 0.72) * rate) * m.rules.movement.tick;
            assert!((repaired - expected).abs() < 1e-9);
            if enabled {
                assert!(
                    (m.logistics.get(workers[0]).unwrap().provisions - (29. - repaired / 5.)).abs()
                        < 1e-10
                );
                assert_eq!(m.logistics.get(workers[1]).unwrap().provisions, 120.);
            }
            let mut delivery = DeliveryState::new(&m).unwrap();
            for _ in 0..600 {
                capture_tick(&mut m, &mut delivery);
                assert_eq!(
                    m.story.iter().map(|s| s.mp_spent).collect::<Vec<_>>(),
                    spent
                );
                if m.world.game.units[&site].hp == full {
                    break;
                }
            }
            assert_eq!(m.world.game.units[&site].hp, full);
            assert!(!m.world.section_by_id(section_id).unwrap().1.failed);
            assert_eq!(m.world.game.objects.as_ref().unwrap()[cell], b'K');
            for _ in 0..3 {
                capture_tick(&mut m, &mut delivery);
            }
            assert!(
                workers
                    .into_iter()
                    .all(|w| m.commands.units[&w].build_target.is_none())
            );
            finish_and_capture(&mut m, &mut delivery);
        }
    }
}

#[test]
fn base30_classic_sudden_death_caps_live_crew_repair_below_decay() {
    let mut m = scene(Mode::Classic, true);
    let site = id(&m, 0, "hq");
    let at = m.world.game.units[&site].at();
    let workers = [id(&m, 0, "engineer"), id(&m, 1, "engineer")];
    // A nonfatal damaged producer keeps the ordinary structural sync active.
    m.world.game.units.get_mut(&site).unwrap().hp -= 200.;
    for (owner, worker) in workers.into_iter().enumerate() {
        place(
            &mut m,
            worker,
            Point {
                x: at.x + 4.5,
                z: at.z + owner as f64 * 0.25,
            },
        );
        assert_eq!(
            m.execute_assist(owner, &[i64::from(worker)], i64::from(site), false)
                .unwrap(),
            None
        );
    }
    m.elimination.as_mut().unwrap().time_left = Some(0.);
    let mut delivery = DeliveryState::new(&m).unwrap();
    capture_tick(&mut m, &mut delivery);
    assert!(m.elimination_sudden_death());
    let full = number(Catalog::compiled().unwrap().unit("hq").unwrap(), "hpPer").unwrap();
    let net_decay = full * (0.01 - 0.006) * m.rules.movement.tick;
    for _ in 0..5 {
        let before = m.world.game.units[&site].hp;
        capture_tick(&mut m, &mut delivery);
        assert!((m.world.game.units[&site].hp - (before - net_decay)).abs() < 1e-8);
    }
    finish_and_capture(&mut m, &mut delivery);
}

#[test]
fn log20_classic_engineer_builds_cache_only_with_logistics_and_delivers_completed_store() {
    for enabled in [false, true] {
        let mut m = scene(Mode::Classic, enabled);
        let worker = id(&m, 0, "engineer");
        let home = m.start.players[0].spawn;
        let at = Point {
            x: home.x + 18.,
            z: home.z + 2.,
        };
        place(
            &mut m,
            worker,
            Point {
                x: at.x - 3.,
                z: at.z,
            },
        );
        // Refresh normal vision before exercising the visible placement path.
        let mut delivery = DeliveryState::new(&m).unwrap();
        capture_tick(&mut m, &mut delivery);
        m.start.economy.players[0].mp = 1000.;
        let before = m.start.economy.players[0].mp;
        let cache = m.next_unit_id;
        let result = m
            .execute_command(
                0,
                &Command::Build {
                    ids: vec![i64::from(worker)],
                    kind: "supplycache".into(),
                    at: Some(at),
                    direction: None,
                    queue: false,
                },
                false,
            )
            .unwrap();
        if !enabled {
            assert_eq!(result, Some(Denial::Blocked));
            assert_eq!(m.start.economy.players[0].mp, before);
            assert!(!m.world.game.units.contains_key(&cache));
            capture_tick(&mut m, &mut delivery);
            continue;
        }
        assert_eq!(result, None);
        assert_eq!(m.start.economy.players[0].mp, before - 60.);
        assert_eq!(m.world.game.units[&cache].cells.as_ref().unwrap().len(), 4);
        assert_eq!(m.world.game.units[&cache].built, Some(0.));
        let mut saw_store = false;
        // Completion occurs just after tick 240. Store discovery then waits
        // for the normal 40-tick convoy synchronization at tick 280.
        for _ in 0..320 {
            capture_tick(&mut m, &mut delivery);
            saw_store |= m
                .convoys
                .as_ref()
                .unwrap()
                .stores
                .iter()
                .any(|s| s.entity == Some(cache) && s.active);
            if m.world.game.units[&cache].built == Some(1.) && saw_store {
                break;
            }
        }
        assert_eq!(m.world.game.units[&cache].built, Some(1.));
        assert_eq!(m.world.game.units[&cache].hp, 400.);
        assert!(
            saw_store,
            "completed cache absent at tick {}: {:?}",
            m.world.game.tick,
            m.convoys.as_ref().unwrap().stores
        );
        finish_and_capture(&mut m, &mut delivery);
    }
}
