//! Unchanged original full steps compared with the sole production Match runner.
use crate::shared::aviation as common;

use common::{aircraft_phase_snapshot_with_events, assert_json, bits, scene};
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    match_commands::Command,
    match_end::MatchSample,
    match_runtime::Match,
    match_story::StoryKey,
    match_tick::{TickEvent, TickPathConfig},
    orders::OrderKey,
};

fn fixture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("fixtures/match-aircraft-runner.json.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn make(trace: &Value) -> Match {
    // Shared real publication creates every aircraft and its canonical sortie row.
    // Only authored initial scene facts are restored, never observed phase outputs.
    let mut m = scene(trace);
    for unit in m.world.game.units.values_mut() {
        unit.hold_pos = true;
    }
    for control in m.commands.units.values_mut() {
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    for player in &mut m.start.economy.players {
        player.mp = 500.0;
    }
    m.start.environment_seed = 123;
    m.start.wind = (0.0, 0.5);
    m.initialize_tick_scene(0, TickPathConfig::default())
        .unwrap();
    m
}
fn snapshot(m: &Match, events: &[TickEvent]) -> Value {
    let g = &m.world.game;
    let aviation = events
        .iter()
        .map(|event| match event {
            TickEvent::Aviation(event) => event.clone(),
            other => panic!("unexpected non-Aviation output in the AIR fixture: {other:?}"),
        })
        .collect::<Vec<_>>();
    let aircraft = aircraft_phase_snapshot_with_events(m, &aviation);
    json!({"tick":g.tick,"nextId":m.next_unit_id,"aircraft":aircraft,
        "generic":g.units.values().map(|u| {
            let a=m.abilities.get(u.id).unwrap(); let c=&m.commands.units[&u.id];
            assert!(c.orders.is_empty(),"bounded aircraft scenes have no unactuated queue");
            json!({"id":u.id,"type":m.rules.kinds[u.kind].name,"owner":u.owner,"x":bits(u.x),"z":bits(u.z),"rot":bits(u.rot),"supp":bits(u.supp),"repath":bits(u.repath),"cd":bits(a.cooldown),"buff":bits(a.buff),"sprint":bits(u.sprint),"react":bits(c.react),"still":u.still.map(bits),"built":u.built.map(bits),"holdPos":u.hold_pos,"auto":c.auto_cast,"autoRetreat":c.auto_retreat,"moveSpeed":bits(u.move_speed),"path":u.path.iter().map(|p|json!({"x":p.x,"z":p.z})).collect::<Vec<_>>(),"orders":Vec::<Value>::new(),"attackId":c.attack_id,"follow":c.follow})
        }).collect::<Vec<_>>(),
        "players":m.start.players.iter().enumerate().map(|(i,p)| {
            let e=&m.start.economy.players[i]; let mut visible=g.players[i].visible.clone(); visible.sort_unstable();
            json!({"mp":bits(e.mp),"vp":bits(e.vp),"inc":bits(e.inc),"sup":p.support.iter().map(|(k,v)|(k.clone(),bits(*v))).collect::<serde_json::Map<_,_>>(),"visible":visible})
        }).collect::<Vec<_>>(),
        "points":m.start.points.iter().enumerate().map(|(i,p)|json!({"owner":p.owner.map_or(-1,|v|v as i64),"capper":p.capper.map_or(-1,|v|v as i64),"progress":bits(p.progress),"cut":p.cut,"onPoint":p.on_point,"contested":p.contested,"capTick":m.capture_ticks[i]})).collect::<Vec<_>>(),
        "story":m.story.iter().map(|p|StoryKey::ALL.map(|k|bits(p.value(k)))).collect::<Vec<_>>(),
        "shots":aircraft["events"],"timeline":m.timeline.iter().map(|s| match s {
                    MatchSample::Conquest { t, vp } => json!({"t":t,"vp":vp}),
                    MatchSample::Territories { .. } => panic!("AIR runner fixture cannot represent a Territories timeline"),
                }).collect::<Vec<_>>(),"seed":f64::from(m.start.environment_seed as i32),"wind":{"a":m.start.wind.0,"v":m.start.wind.1},
        "flightCount":m.firing.flights.len(),"nadeCount":m.firing.grenades.len(),"salvoCount":m.firing.salvos.len(),"strikeCount":g.strikes.len(),"coverCount":m.support.covers.len(),"smokeCount":g.smokes.len(),"wreckCount":g.wrecks.len()})
}
#[test]
fn actual_aircraft_runner_matches_original_whole_steps_without_output_injection() {
    let f = fixture();
    let mut ticks = 0;
    for trace in f["traces"].as_array().unwrap() {
        let name = trace["name"].as_str().unwrap();
        let mut m = make(trace);
        let random = m.start.random.snapshot();
        assert_json(&snapshot(&m, &[]), &trace["initialRunner"], name);
        let mut sequences = [0u64; 2];
        for (i, frame) in trace["frames"].as_array().unwrap().iter().enumerate() {
            let tick = i as u64 + 1;
            for input in frame["inputs"].as_array().unwrap() {
                let owner = input["owner"].as_u64().unwrap() as usize;
                sequences[owner] += 1;
                m.enqueue_command(
                    OrderKey {
                        tick,
                        issuer: owner as u32,
                        sequence: sequences[owner],
                    },
                    Command::from_json(&input["command"]).unwrap(),
                )
                .unwrap();
            }
            let report = m
                .advance_tick()
                .unwrap_or_else(|error| panic!("{name}:{tick}: {error}"));
            assert_eq!(report.tick, tick as i64);
            assert!(
                report.commands.iter().all(|r| r.denial.is_none()),
                "{name}:{tick}: command denial"
            );
            let prior = json!(
                report
                    .prior
                    .iter()
                    .map(|(id, p)| json!([id, bits(p.x), bits(p.z), bits(p.rot)]))
                    .collect::<Vec<_>>()
            );
            assert_eq!(prior, frame["prior"], "{name}:{tick}: actual prior poses");
            let captured = frame["phases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["op"] == "aa")
                .unwrap();
            assert_eq!(
                json!(report.unit_order),
                captured["actors"],
                "{name}:{tick}: late captured membership"
            );
            assert!(
                report.environment().next().is_none(),
                "{name}:{tick}: unexpected environment output"
            );
            assert!(
                m.firing.events.is_empty()
                    && m.support.events.is_empty()
                    && m.aviation.events.is_empty(),
                "{name}:{tick}: undrained global launch/support output"
            );
            assert_json(
                &snapshot(&m, &report.events),
                &frame["expectedRunner"],
                &format!("{name}:{tick}"),
            );
            assert_eq!(
                m.start.random.snapshot(),
                random,
                "{name}:{tick}: native MatchRandom must stay naturally seeded"
            );
            let before = frame["prior"].as_array().unwrap();
            let expected_removed = before
                .iter()
                .map(|v| v[0].as_u64().unwrap() as u32)
                .filter(|id| !m.world.game.units.contains_key(id))
                .collect::<Vec<_>>();
            assert_eq!(
                report.retired.iter().map(|r| r.unit.id).collect::<Vec<_>>(),
                expected_removed,
                "{name}:{tick}: actual retirement facts"
            );
            for id in expected_removed {
                assert!(!m.aviation.planes.contains_key(&id));
                assert!(m.combat.get(id).is_some() && m.firing.get(id).is_some());
            }
            ticks += 1;
        }
    }
    assert_eq!(ticks, f["steps"].as_u64().unwrap());
}
#[test]
fn aircraft_runner_admission_refuses_unrepresented_phases_before_queue_or_clock_writes() {
    let f = fixture();
    let trace = &f["traces"][0];
    for kind in 0..6 {
        let mut m = make(trace);
        let id = 1000;
        match kind {
            0 => {
                m.aviation.planes.remove(&id);
            }
            1 => {
                m.aviation.planes.insert(4000, m.aviation.planes[&id]);
            }
            2 => {
                m.world.game.units.get_mut(&id).unwrap().air =
                    Some(ww2_core::game::PlaneState::Station);
                m.commands.units.get_mut(&id).unwrap().hold_fire = false;
                m.aviation.planes.get_mut(&id).unwrap().mission =
                    Some(ww2_core::match_aviation::PlaneMission {
                        kind: ww2_core::match_aviation::MissionKind::Patrol,
                        target: None,
                        at: ww2_core::game::Point { x: 40., z: 40. },
                    });
            }
            3 => {
                m.world.game.units.get_mut(&id).unwrap().air =
                    Some(ww2_core::game::PlaneState::Station);
                m.aviation.planes.get_mut(&id).unwrap().mission =
                    Some(ww2_core::match_aviation::PlaneMission {
                        kind: ww2_core::match_aviation::MissionKind::Area,
                        target: None,
                        at: ww2_core::game::Point { x: 40., z: 40. },
                    });
            }
            4 => {
                m.world.game.units.get_mut(&id).unwrap().x = f64::INFINITY;
            }
            5 => {
                m.options.mode = ww2_core::economy::Mode::World;
            }
            _ => unreachable!(),
        }
        m.enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 1,
            },
            Command::from_json(&json!({"t":"stance","ids":[id],"key":"holdFire","on":true}))
                .unwrap(),
        )
        .unwrap();
        let before = format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            m.world.game.units,
            m.aviation,
            m.commands.units,
            m.commands.pending_commands(),
            m.start.random.snapshot(),
            m.start.economy.players,
            m.story,
            m.world.game.tick
        );
        assert!(
            m.validate_aircraft_runner_boundary(1).is_err(),
            "guard {kind}"
        );
        assert!(m.advance_tick().is_err(), "actual runner guard {kind}");
        assert_eq!(
            before,
            format!(
                "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
                m.world.game.units,
                m.aviation,
                m.commands.units,
                m.commands.pending_commands(),
                m.start.random.snapshot(),
                m.start.economy.players,
                m.story,
                m.world.game.tick
            )
        );
        assert!(m.tick_state.fault().is_none());
        assert_eq!(m.commands.pending_commands(), 1);
        assert!(m.validate_command_tick(1).is_ok());
    }
}

#[test]
fn post_command_aircraft_admission_keeps_the_old_clock_and_the_advanced_queue() {
    let f = fixture();
    let mut m = make(&f["traces"][0]);
    m.enqueue_command(
        OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        },
        Command::from_json(&json!({"t":"move","orders":[[1000,25,25]]})).unwrap(),
    )
    .unwrap();
    m.validate_aircraft_runner_boundary(1).unwrap();
    let dispatched = m.apply_commands(1, false).unwrap();
    assert_eq!(dispatched.len(), 1);
    assert!(dispatched[0].denial.is_none());
    assert_eq!(m.world.game.tick, 0);
    assert_eq!(m.commands.next_tick(), Some(2));
    assert!(m.validate_aircraft_runner_boundary(1).is_err());
    let before = format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        m.world.game.units,
        m.aviation,
        m.commands.units,
        m.start.random.snapshot(),
        m.start.economy.players,
        m.world.game.tick,
        m.commands.next_tick()
    );
    m.validate_aircraft_runner_state(1).unwrap();
    assert!(m.validate_aircraft_runner_state(2).is_err());
    assert_eq!(
        before,
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            m.world.game.units,
            m.aviation,
            m.commands.units,
            m.start.random.snapshot(),
            m.start.economy.players,
            m.world.game.tick,
            m.commands.next_tick()
        )
    );
}
