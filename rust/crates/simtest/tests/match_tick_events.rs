//! Original complete Source steps establish one chronological actual-runner output sequence.
#[path = "common/aviation.rs"]
#[allow(dead_code)]
mod common;

use common::{assert_json, bits};
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    match_aviation::AviationEvent,
    match_commands::Command,
    match_firing::LaunchEvent,
    match_runtime::Match,
    match_story::StoryKey,
    match_support::SupportEvent,
    match_tick::{TickEvent, TickPathConfig},
    orders::OrderKey,
    random::{MatchRandom, RandomSnapshot, StreamId},
};

fn fixture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(include_bytes!("fixtures/match-tick-events.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn make(trace: &Value) -> Match {
    // The shared factory uses real aircraft and ordinary actor publication.
    // Every patch below is an authored initial input, never a Source result.
    let mut m = common::scene(trace);
    for (spec, unit) in trace["specs"]
        .as_array()
        .unwrap()
        .iter()
        .zip(m.world.game.units.values_mut())
    {
        unit.hold_pos = true;
        let control = m.commands.units.get_mut(&unit.id).unwrap();
        control.hold_fire = spec["holdFire"].as_bool().unwrap_or(true);
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    for player in &mut m.start.economy.players {
        player.mp = 500.0;
    }
    for fire in trace["fires"].as_array().unwrap() {
        m.world.fires.insert(
            fire[0].as_u64().unwrap() as usize,
            fire[1].as_f64().unwrap(),
        );
    }
    m.start.environment_seed = trace["environmentSeed"].as_u64().unwrap() as u32;
    m.start.wind = (
        trace["wind"]["a"].as_f64().unwrap(),
        trace["wind"]["v"].as_f64().unwrap(),
    );
    assert_eq!(
        m.start.random.snapshot().seed,
        trace["seed"].as_u64().unwrap()
    );
    m.initialize_tick_scene(0, TickPathConfig::default())
        .unwrap();
    m
}

fn event_json(m: &Match, event: &TickEvent) -> Value {
    match event {
        TickEvent::Firing(event) => event.to_json(&m.rules),
        TickEvent::Environment(event) => event.to_json(),
        TickEvent::Support(event) => match event {
            SupportEvent::Shell(p) => json!({"x":p.x,"z":p.z,"k":"shell","pub":true}),
            SupportEvent::Bomb(p) => json!({"x":p.x,"z":p.z,"k":"bomb","pub":true}),
            SupportEvent::Chutes(p) => json!({"x":p.x,"z":p.z,"k":"chutes","pub":true}),
            SupportEvent::SmokeShells(p) => {
                json!({"x":p.x,"z":p.z,"k":"smokeshells","pub":true})
            }
            SupportEvent::Plane {
                kind,
                at,
                direction,
                owner,
            } => json!({"k":kind.name(),"x":at.x,"z":at.z,"dir":direction,"fo":owner,"pub":true}),
            SupportEvent::Flak { unit, owner, at } => {
                json!({"f":unit,"fo":owner,"x":at.x,"z":at.z,"k":"flak","pub":true})
            }
            SupportEvent::ShotDown {
                kind,
                at,
                direction,
                fighters,
            } => {
                json!({"k":"shotdown","x":at.x,"z":at.z,"dir":direction,"kind":kind.name(),"by":if *fighters {"fighters"} else {"flak"},"pub":true})
            }
            SupportEvent::Environment(effect) => effect.to_json(),
        },
        TickEvent::Aviation(event) => match event {
            AviationEvent::AntiAir {
                shooter,
                target,
                owner,
                target_owner,
                at,
            } => {
                json!({"f":shooter,"t":target,"fo":owner,"to":target_owner,"x":at.x,"z":at.z,"k":"aa"})
            }
            AviationEvent::PlaneDown {
                target,
                owner,
                at,
                direction,
            } => {
                json!({"k":"planedown","t":target,"to":owner,"x":at.x,"z":at.z,"dir":direction,"kill":true})
            }
        },
    }
}

fn snapshot(m: &Match, events: &[TickEvent]) -> Value {
    json!({"tick":m.world.game.tick,"nextId":m.next_unit_id,"seed":m.start.environment_seed,
        "units":m.world.game.units.values().map(|u|json!([u.id,bits(u.x),bits(u.z),bits(u.rot),bits(u.hp),bits(u.supp)])).collect::<Vec<_>>(),
        "flights":m.firing.flights.len(),"grenades":m.firing.grenades.len(),"salvos":m.firing.salvos.len(),"strikes":m.world.game.strikes.len(),
        "shots":events.iter().map(|event|event_json(m,event)).collect::<Vec<_>>(),
        "story":m.story.iter().map(|s|StoryKey::ALL.into_iter().map(|k|bits(s.value(k))).collect::<Vec<_>>()).collect::<Vec<_>>()})
}

fn compare_random(m: &Match, initial: &RandomSnapshot, source: &Value, context: &str) {
    let now = m.start.random.snapshot();
    let represented = [StreamId::Combat, StreamId::OffMap];
    assert_eq!(
        now.streams
            .iter()
            .filter(|(purpose, _)| !represented.contains(purpose))
            .collect::<Vec<_>>(),
        initial
            .streams
            .iter()
            .filter(|(purpose, _)| !represented.contains(purpose))
            .collect::<Vec<_>>(),
        "{context}: unrelated native random purpose changed"
    );
    for (name, purpose) in [("Combat", StreamId::Combat), ("OffMap", StreamId::OffMap)] {
        let count = source[name]["count"].as_u64().unwrap();
        let actual = now
            .streams
            .iter()
            .find_map(|(p, state)| (*p == purpose).then_some(*state));
        if count == 0 {
            assert!(actual.is_none(), "{context}: premature {name} draw");
        } else {
            // This independent predictor checks draws. It never writes to MatchRandom.
            let mut predictor = MatchRandom::new(initial.seed);
            for _ in 0..count {
                predictor.next_fraction(purpose);
            }
            let predicted = predictor
                .snapshot()
                .streams
                .into_iter()
                .find_map(|(p, state)| (p == purpose).then_some(state));
            assert_eq!(actual, predicted, "{context}: {name} draw count");
            assert_eq!(
                format!("{:016x}", actual.unwrap()),
                source[name]["state"].as_str().unwrap(),
                "{context}: Source {name} state"
            );
        }
    }
}

#[test]
fn actual_tick_owns_complete_original_chronological_outputs() {
    let fixture = fixture();
    let mut ticks = 0;
    let mut events = 0;
    let mut seen = [false; 4];
    let mut nested = [false; 2];
    for trace in fixture["traces"].as_array().unwrap() {
        let name = trace["name"].as_str().unwrap();
        let mut m = make(trace);
        let initial_random = m.start.random.snapshot();
        let mut sequences = [0; 2];
        let mut first_owned: Option<(Vec<TickEvent>, Value)> = None;
        for (i, frame) in trace["frames"].as_array().unwrap().iter().enumerate() {
            let tick = i as u64 + 1;
            let context = format!("{name}:{tick}");
            assert!(
                m.firing.events.is_empty()
                    && m.support.events.is_empty()
                    && m.aviation.events.is_empty(),
                "{context}: previous producer output retained"
            );
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
            // One actual runner call for one complete original Source step.
            let report = m
                .advance_tick()
                .unwrap_or_else(|error| panic!("{context}: {error}"));
            assert_eq!(report.tick, tick as i64, "{context}: clock");
            assert_eq!(
                m.commands.next_tick(),
                Some(tick + 1),
                "{context}: next boundary order"
            );
            assert_eq!(
                report.commands.len(),
                frame["inputs"].as_array().unwrap().len(),
                "{context}: command count"
            );
            assert!(
                report.commands.iter().all(|result| result.denial.is_none()),
                "{context}: native command denial"
            );
            let prior = json!(
                report
                    .prior
                    .iter()
                    .map(|(id, p)| json!([id, bits(p.x), bits(p.z), bits(p.rot)]))
                    .collect::<Vec<_>>()
            );
            assert_eq!(prior, frame["prior"], "{context}: actual prior poses");
            assert_eq!(
                json!(report.retired.iter().map(|r| r.unit.id).collect::<Vec<_>>()),
                frame["retired"],
                "{context}: canonical death removal"
            );
            // Exact full-array comparison protects payload, order, loss and duplicates.
            assert_eq!(
                report.events.len(),
                frame["emissions"].as_array().unwrap().len(),
                "{context}: original append count"
            );
            assert_json(&snapshot(&m, &report.events), &frame["expected"], &context);
            compare_random(&m, &initial_random, &frame["random"], &context);
            assert!(
                m.firing.events.is_empty()
                    && m.support.events.is_empty()
                    && m.aviation.events.is_empty(),
                "{context}: output not detached from producers"
            );
            for event in &report.events {
                match event {
                    TickEvent::Firing(LaunchEvent::Environment(_)) => nested[0] = true,
                    TickEvent::Support(SupportEvent::Environment(_)) => nested[1] = true,
                    _ => (),
                }
                seen[match event {
                    TickEvent::Firing(_) => 0,
                    TickEvent::Support(_) => 1,
                    TickEvent::Aviation(_) => 2,
                    TickEvent::Environment(_) => 3,
                }] = true;
            }
            if let Some((owned, expected)) = &first_owned {
                assert_eq!(
                    &json!(owned.iter().map(|e| event_json(&m, e)).collect::<Vec<_>>()),
                    expected,
                    "{context}: later tick mutated prior owned output"
                );
            } else {
                let owned = report.events.clone();
                let expected = json!(owned.iter().map(|e| event_json(&m, e)).collect::<Vec<_>>());
                first_owned = Some((owned, expected));
            }
            events += report.events.len();
            ticks += 1;
        }
        assert!(m.tick_state.fault().is_none());
        assert_eq!(m.commands.pending_commands(), 0);
    }
    assert_eq!(
        seen, [true; 4],
        "fixture must cover every represented producer family"
    );
    assert_eq!(
        nested, [true; 2],
        "producer environment effects must remain nested"
    );
    assert_eq!((ticks, events), (375, 103));
}

fn native_failure_scene(specs: Value) -> Match {
    let trace = json!({"map":{
        "name":"Native committed-output failure arena","w":40,"h":40,
        "rows":vec![".".repeat(40);40],"spawns":[{"x":2,"y":2},{"x":37,"y":37}],
        "points":[{"id":"front","x":30,"y":15}]},"specs":specs,"visible":true});
    let mut m = common::scene(&trace);
    for u in m.world.game.units.values_mut() {
        u.hold_pos = true;
        let control = m.commands.units.get_mut(&u.id).unwrap();
        control.auto_cast = false;
        control.auto_retreat = false;
        control.hold_fire = true;
    }
    m.initialize_tick_scene(0, TickPathConfig::default())
        .unwrap();
    m
}

fn protected_after_failure(m: &Match) -> Vec<String> {
    vec![
        format!("{:?}", m.world),
        format!("{:?}", m.environment),
        format!("{:?}", m.firing),
        format!("{:?}", m.combat),
        format!("{:?}", m.commands.units),
        format!("{:?}", m.commands.path_requests()),
        format!("{:?}", m.abilities),
        format!("{:?}", m.recovery),
        format!("{:?}", m.support),
        format!("{:?}", m.aviation),
        format!("{:?}", m.start.random.snapshot()),
        format!("{:?}", m.start.economy.players),
        format!("{:?}", m.story),
        format!("{:?}", m.tick_state),
        format!(
            "{:?}",
            (
                m.commands.pending_commands(),
                m.commands.next_tick(),
                m.next_unit_id,
                m.start.environment_seed
            )
        ),
    ]
}

#[test]
fn actual_rifle_launch_survives_later_route_identity_failure() {
    use ww2_core::orders::QueueError;
    let mut m = native_failure_scene(json!([
        {"kind":"rifle","owner":0,"x":25,"z":25,"rot":0,"still":20},
        {"kind":"rifle","owner":0,"x":25,"z":45,"rot":0,"still":20},
        {"kind":"rifle","owner":1,"x":37,"z":25,"rot":0,"still":20}
    ]));
    m.commands.units.get_mut(&1000).unwrap().hold_fire = false;
    m.world.game.units.get_mut(&1001).unwrap().path_gen = u64::MAX - 2;
    let initial_random = m.start.random.snapshot();
    m.enqueue_command(
        OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        },
        Command::from_json(&json!({"t":"move","orders":[[1001,35,45]]})).unwrap(),
    )
    .unwrap();
    let future_key = OrderKey {
        tick: 2,
        issuer: 0,
        sequence: 2,
    };
    let future_command = Command::from_json(&json!({"t":"stop","ids":[1001]})).unwrap();
    m.enqueue_command(future_key, future_command.clone())
        .unwrap();

    let failure = m
        .advance_tick()
        .expect_err("later mover must exhaust its route identity");
    assert_eq!(failure.reason, "unit path identity exhausted");
    assert_eq!(failure.tick, 1);
    assert_eq!(m.world.game.tick, 1);
    assert_eq!(m.tick_state.fault(), Some(failure.reason.as_str()));
    assert_eq!(
        m.world.game.units[&1001].path_gen,
        u64::MAX - 1,
        "actual Move must advance its route identity once before movement refuses"
    );
    assert_eq!(failure.retirement, Default::default());
    assert!(
        !failure.events.is_empty(),
        "earlier ordinary rifle turn must produce real launches"
    );
    let mut flights = std::collections::BTreeSet::new();
    for event in &failure.events {
        let TickEvent::Firing(LaunchEvent::Flight {
            id, shooter, owner, ..
        }) = event
        else {
            panic!(
                "failure prefix must contain only the earlier rifle's actual launches: {event:?}"
            );
        };
        assert_eq!((*shooter, *owner), (1000, 0));
        assert!(flights.insert(*id), "launch output duplicated");
        let flight = m.firing.flights.get(id).expect("committed real flight");
        assert_eq!(flight.shooter, 1000);
    }
    assert_eq!(
        flights.len(),
        m.firing.flights.len(),
        "a committed launch must not be lost"
    );
    assert_ne!(
        m.start.random.snapshot(),
        initial_random,
        "real rifle firing must consume naturally seeded Combat randomness"
    );
    assert!(
        m.firing.events.is_empty() && m.support.events.is_empty() && m.aviation.events.is_empty()
    );
    assert_eq!(m.commands.pending_commands(), 1);
    assert_eq!(m.commands.next_tick(), Some(2));
    let retained = json!(
        failure
            .events
            .iter()
            .map(|e| event_json(&m, e))
            .collect::<Vec<_>>()
    );
    let protected = protected_after_failure(&m);
    let retry = m
        .advance_tick()
        .expect_err("faulted runner must refuse the next tick");
    assert!(retry.reason.starts_with("tick runner faulted:"));
    assert_eq!(retry.tick, 1);
    assert!(retry.events.is_empty());
    assert_eq!(retry.retirement, Default::default());
    assert_eq!(
        protected_after_failure(&m),
        protected,
        "fault refusal must not advance canonical state"
    );
    assert_eq!(
        json!(
            failure
                .events
                .iter()
                .map(|e| event_json(&m, e))
                .collect::<Vec<_>>()
        ),
        retained,
        "failure owns its detached launch prefix"
    );
    assert_eq!(
        m.enqueue_command(future_key, future_command),
        Err(QueueError::DuplicateKey),
        "the exact future queued key must remain present"
    );
    assert_eq!(
        protected_after_failure(&m),
        protected,
        "duplicate-key proof must not change the queue or actor state"
    );
}

#[test]
fn completed_building_section_effect_survives_later_invalid_origin() {
    use ww2_core::{catalog::Catalog, environment::Effect, game::Point};
    let mut m = native_failure_scene(json!([
        {"kind":"hq","owner":0,"x":25,"z":25,"built":1},
        {"kind":"hq","owner":1,"x":45,"z":45,"built":1}
    ]));
    let maximum = Catalog::compiled()
        .unwrap()
        .unit("hq")
        .unwrap()
        .get("hpPer")
        .unwrap()
        .number()
        .unwrap();
    let first_cell = 12 * 40 + 12;
    let second_cell = 22 * 40 + 22;
    for (id, cell) in [(1000, first_cell), (1001, second_cell)] {
        m.world.set_cell(cell, b'B').unwrap();
        m.world.game.units.get_mut(&id).unwrap().cells = Some(vec![cell as u32]);
        m.world
            .register_building_structure(id, &[cell], 100.0, maximum)
            .unwrap();
        m.world.game.units.get_mut(&id).unwrap().hp = if id == 1000 { 75.0 } else { 100.0 };
    }
    m.world.game.units.get_mut(&1001).unwrap().hit_from = Some(Point {
        x: 2_000_000_000.0,
        z: 0.0,
    });
    let initial_random = m.start.random.snapshot();
    let earlier_journal_count = m.world.journal().len();
    let failure = m
        .advance_tick()
        .expect_err("second actual building origin must refuse section sync");
    assert!(
        failure.reason.contains("building section sync:")
            && failure.reason.contains("building hit origin"),
        "actual section-sync origin error: {}",
        failure.reason
    );
    assert_eq!(failure.tick, 0);
    assert_eq!(
        m.world.game.tick, 0,
        "structure reconciliation precedes the clock"
    );
    assert_eq!(m.tick_state.fault(), Some(failure.reason.as_str()));
    assert_eq!(failure.retirement, Default::default());
    assert_eq!(
        m.world.game.sections[&(first_cell as u32)].hp,
        75.0,
        "earlier building reconciliation must have committed"
    );
    assert_eq!(
        m.world.game.sections[&(second_cell as u32)].hp,
        100.0,
        "later invalid origin must not debit its section"
    );
    assert!(
        m.world.journal().len() > earlier_journal_count,
        "actual section effect must update the world journal"
    );
    assert_eq!(
        failure.events.len(),
        1,
        "completed earlier section hurt must be retained exactly once"
    );
    let TickEvent::Environment(Effect::Hurt(hurt)) = &failure.events[0] else {
        panic!(
            "earlier building must emit its real section hurt: {:?}",
            failure.events
        );
    };
    assert_eq!((hurt.unit, hurt.owner), (1000, 0));
    assert_eq!(hurt.at, Point { x: 25.0, z: 25.0 });
    assert!(!hurt.killed);
    assert_eq!(
        m.start.random.snapshot(),
        initial_random,
        "section-only failure must not consume match randomness"
    );
    assert!(
        m.firing.events.is_empty() && m.support.events.is_empty() && m.aviation.events.is_empty()
    );
    let retained = json!(
        failure
            .events
            .iter()
            .map(|e| event_json(&m, e))
            .collect::<Vec<_>>()
    );
    let protected = protected_after_failure(&m);
    let retry = m
        .advance_tick()
        .expect_err("faulted pre-clock runner must refuse retry");
    assert_eq!(retry.tick, 0);
    assert!(retry.events.is_empty());
    assert_eq!(retry.retirement, Default::default());
    assert_eq!(
        protected_after_failure(&m),
        protected,
        "fault refusal must not repeat section effects or mutations"
    );
    assert_eq!(
        json!(
            failure
                .events
                .iter()
                .map(|e| event_json(&m, e))
                .collect::<Vec<_>>()
        ),
        retained,
        "pre-clock failure owns its completed section output"
    );
}
