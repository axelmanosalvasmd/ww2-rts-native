use crate::shared::firing as common;
use common::{compare, make, make_with_mode, snapshot};
use serde_json::Value;
use std::io::Read;
use ww2_core::{
    economy::Mode,
    game::ModeKind,
    match_firing::{AbilityBlast, LaunchEvent},
    random::StreamId,
};
fn fixture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("fixtures/match-firing.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
#[test]
fn default_and_conquest_targeting_and_launches_match_original_source() {
    let fixture: Value = fixture();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 123);
    let mut phases = 0;
    for (c, mode) in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| [Mode::Default, Mode::Conquest].map(|mode| (c, mode)))
    {
        let mut m = make_with_mode(c, mode);
        assert!(m.world.game.mode.is_none());
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
                let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
                m.begin_firing_tick(&order).unwrap_or_else(|e| {
                    panic!("{} {} tick {current}: {e}", c["shooter"], c["variation"])
                });
                m.advance_firing_timers(1, 0.05).unwrap();
                if !run || current == 1 {
                    m.apply_unit_firing(1, c["variation"] == "moving", 0.05)
                        .unwrap();
                    if c["variation"] == "choice-claims" {
                        m.apply_unit_firing(4, false, 0.05).unwrap();
                    }
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
    assert_eq!(phases, 624 * 2);
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
    let blast = AbilityBlast::from_unit(m.world.game.units[&1].kind, &m.rules).unwrap();
    let at = m.world.game.units[&2].at();
    m.world.game.units.get_mut(&1).unwrap().owner = u32::MAX as usize;
    let before = m.firing.clone();
    let combat = m.combat.clone();
    let logistics = m.logistics.clone();
    let commands = m.commands.units.clone();
    let random = m.start.random.snapshot();
    let unsupported = [
        Mode::Classic,
        Mode::World,
        Mode::Assault,
        Mode::Annihilation,
        Mode::Horde,
        Mode::Tutorial,
        Mode::Other,
    ]
    .map(|mode| (mode, None));
    let inconsistent = [
        ModeKind::Classic,
        ModeKind::World,
        ModeKind::Assault,
        ModeKind::Annihilation,
        ModeKind::Horde,
        ModeKind::Tutorial,
    ]
    .into_iter()
    .flat_map(|kind| [Mode::Default, Mode::Conquest].map(|mode| (mode, Some(kind))));
    for (mode, kind) in unsupported.into_iter().chain(inconsistent) {
        m.options.mode = mode;
        m.world.game.mode = kind;
        let world = format!("{:?}", m.world.game);
        for error in [
            m.capture_firing_prior().unwrap_err(),
            m.begin_firing_tick(&[1, 2, 3]).unwrap_err(),
            m.advance_firing_timers(1, 0.05).unwrap_err(),
            m.apply_unit_firing(1, false, 0.05).unwrap_err(),
            m.can_shoot_target(1, 2).unwrap_err(),
            m.step_firing_flights().unwrap_err(),
            m.advance_firing_salvos(0.05).unwrap_err(),
            m.advance_firing_grenades(0.05).unwrap_err(),
            m.launch_ability_flight(1, at, blast, None, Some(1))
                .unwrap_err(),
            m.launch_weapon_salvo(1, at, None).unwrap_err(),
        ] {
            assert!(
                error.contains("ordinary Conquest mode"),
                "{mode:?}/{kind:?}: {error}"
            );
        }
        assert_eq!(m.firing, before);
        assert_eq!(m.combat, combat);
        assert_eq!(m.logistics, logistics);
        assert_eq!(m.commands.units, commands);
        assert_eq!(m.start.random.snapshot(), random);
        assert_eq!(format!("{:?}", m.world.game), world);
    }
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
