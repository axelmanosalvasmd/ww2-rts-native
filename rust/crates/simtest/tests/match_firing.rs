use serde_json::Value;
use std::io::Read;
use ww2_core::{economy::Mode, match_firing::LaunchEvent, random::StreamId};
#[path = "common/firing.rs"]
mod common;
use common::{compare, make, snapshot};
fn fixture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("fixtures/match-firing.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
#[test]
fn assembled_match_targeting_and_launches_match_original_source() {
    let fixture: Value = fixture();
    let mut phases = 0;
    for c in fixture["cases"].as_array().unwrap() {
        let mut m = make(c);
        let run = c["variation"].as_str().unwrap().starts_with("flight");
        let mut last = 0;
        for s in c["steps"].as_array().unwrap() {
            let tick = s["tick"].as_i64().unwrap();
            for current in if run { last + 1..=tick } else { tick..=tick } {
                m.world.game.tick = current - 1;
                m.capture_firing_prior().unwrap();
                m.world.game.tick = current;
                if run && current == 2 {
                    if c["variation"] == "flight-wall" {
                        m.world
                            .set_cell(c["lateWall"].as_u64().unwrap() as usize, b'B')
                            .unwrap();
                    }
                    if c["variation"] == "flight-bounce" {
                        let z = m.world.game.units[&1].z;
                        let t = m.world.game.units.get_mut(&2).unwrap();
                        t.x = 35.0;
                        t.z = z + 1.18;
                        m.world.game.tick = current - 1;
                        m.capture_firing_prior().unwrap();
                        m.world.game.tick = current;
                        m.grid.rebuild(&m.world.game.units);
                        for p in m.firing.flights.values_mut() {
                            p.solution.velocity.y = 0.0;
                        }
                    }
                    if c["variation"] == "flight-dead-shooter" {
                        m.world.game.units.remove(&1);
                        m.firing.remove(1);
                        m.commands.units.remove(&1);
                        m.grid.rebuild(&m.world.game.units);
                    }
                    if c["variation"] == "flight-expiry" {
                        for p in m.firing.flights.values_mut() {
                            p.solution.velocity.y = 80.0;
                        }
                    }
                }
                if c["variation"] == "flight-motion" && current > 1 {
                    let t = m.world.game.units.get_mut(&2).unwrap();
                    t.z += if current < 80 { 0.15 } else { -0.15 };
                    t.rot += 0.025;
                    m.grid.rebuild(&m.world.game.units);
                }
                m.begin_firing_tick(&[1, 2, 3]).unwrap_or_else(|e| {
                    panic!("{} {} tick {current}: {e}", c["shooter"], c["variation"])
                });
                m.advance_firing_timers(1, 0.05).unwrap();
                if !run || current == 1 {
                    m.apply_unit_firing(1, c["variation"] == "moving", 0.05)
                        .unwrap();
                }
                if run {
                    m.step_firing_flights().unwrap();
                    m.advance_firing_salvos(0.05).unwrap();
                }
            }
            last = tick;
            compare(
                &snapshot(&mut m),
                &s["expected"],
                &format!("{} {} tick {}", c["shooter"], c["variation"], s["tick"]),
            );
            let state = m
                .start
                .random
                .snapshot()
                .streams
                .into_iter()
                .find(|(id, _)| *id == StreamId::Combat)
                .map(|(_, v)| v);
            if let Some(state) = state {
                assert_eq!(format!("{state:016x}"), s["random"].as_str().unwrap());
            }
            phases += 1;
        }
    }
    assert_eq!(phases, 588);
}

#[test]
fn unsupported_modes_and_later_bad_payloads_leave_firing_unchanged() {
    let fixture: Value = fixture();
    let c = &fixture["cases"][0];
    let mut m = make(c);
    m.world.game.tick = 1;
    m.begin_firing_tick(&[1, 2, 3]).unwrap();
    m.advance_firing_timers(1, 0.05).unwrap();
    m.apply_unit_firing(1, false, 0.05).unwrap();
    assert!(!m.firing.flights.is_empty());
    let first = m.firing.flights.values().next().unwrap().clone();
    let mut invalid = first.clone();
    invalid.id = 100;
    invalid.near = -1.0;
    m.firing.flights.insert(100, invalid);
    let before = m.firing.clone();
    let world = m.world.game.clone();
    let combat = m.combat.clone();
    let random = m.start.random.snapshot();
    m.world.game.tick = 2;
    assert!(m.step_firing_flights().is_err());
    assert_eq!(m.firing, before);
    assert_eq!(m.combat, combat);
    assert_eq!(m.start.random.snapshot(), random);
    m.world.game.tick = 1;
    let mut actual = m.world.game.units.clone();
    let mut expected = world.units;
    for (&id, u) in &mut actual {
        let other = expected.get_mut(&id).unwrap();
        assert_eq!(u.pace.to_bits(), other.pace.to_bits());
        u.pace = 0.0;
        other.pace = 0.0;
    }
    assert_eq!(actual, expected);
    assert_eq!(m.world.game.chars, world.chars);
    m.options.mode = Mode::World;
    m.world.game.units.get_mut(&1).unwrap().owner = u32::MAX as usize;
    let before = m.firing.clone();
    assert!(m.begin_firing_tick(&[1, 2, 3]).is_err());
    assert!(m.advance_firing_timers(1, 0.05).is_err());
    assert!(m.apply_unit_firing(1, false, 0.05).is_err());
    assert!(m.can_shoot_target(1, 2).is_err());
    assert_eq!(m.firing, before);
}

#[test]
fn equal_fraction_contacts_use_lower_id_despite_grid_history() {
    let fixture = fixture();
    let c = &fixture["cases"][0];
    for reverse in [false, true] {
        let mut m = make(c);
        m.world.game.tick = 1;
        m.begin_firing_tick(&[1, 2, 3]).unwrap();
        m.apply_unit_firing(1, false, 0.05).unwrap();
        let first = *m.firing.flights.keys().next().unwrap();
        m.firing.flights.retain(|id, _| *id == first);
        let p = m.firing.flights.get_mut(&first).unwrap();
        p.solution.velocity = ww2_core::projectiles::Point3::new(230.0, 0.0, 0.0);
        p.near = 0.0;
        let kind = m.world.game.units[&2].kind;
        for id in [2, 3] {
            let t = m.world.game.units.get_mut(&id).unwrap();
            t.kind = kind;
            t.x = 35.0;
            t.z = 15.0;
            t.rot = 0.0;
            t.hp = 100.0;
        }
        m.grid.rebuild(&m.world.game.units);
        if reverse {
            m.grid.remove_unit(2);
            m.grid.update(2, &m.world.game.units[&2]);
        }
        let order = m
            .grid
            .in_order(&m.world.game.units, (35.0, 15.0), 1.0, |id, _| id != 1);
        assert_eq!(order, if reverse { vec![3, 2] } else { vec![2, 3] });
        m.capture_firing_prior().unwrap();
        m.world.game.tick = 2;
        m.step_firing_flights().unwrap();
        let contact = m
            .firing
            .take_events()
            .into_iter()
            .find_map(|e| match e {
                LaunchEvent::Contact(c) => Some(c),
                _ => None,
            })
            .unwrap();
        assert_eq!(contact.target, Some(2));
        assert!(m.world.game.units[&2].hp < 100.0);
        assert_eq!(m.world.game.units[&3].hp, 100.0);
    }
}
