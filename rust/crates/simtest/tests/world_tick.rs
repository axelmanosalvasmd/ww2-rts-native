//! Actual World runner replay against immutable complete original steps.
//! Canonical comparisons explicitly report the remaining Source projection gaps.
#[path = "common/world_tick.rs"]
mod common;
use common::{Exact, same, unpack};
use serde_json::{Value, json};
use ww2_core::match_tick::{TickEvent, TickPhase};

#[test]
fn cmb137_world_recorded_compound_moves_pass_the_existing_actuator_boundary() {
    use ww2_core::{match_commands::Command, orders::OrderKey};
    let fixture = common::fixture();
    let case = &fixture["cases"][0];
    for unsupported in [false, true] {
        let mut m = common::construct(&fixture, case).unwrap();
        let unit = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && !m.rules.kinds[u.kind].structure && u.air.is_none())
            .unwrap();
        let id = unit.id;
        let at = unit.at();
        let mut parts = vec![
            json!({"t":"move","orders":[[id,"invalid",at.z]]}),
            json!({"t":"move","orders":[[id,at.x,at.z]],"queue":true}),
            json!({"t":"amove","orders":[[id,at.x,at.z]],"queue":true}),
        ];
        if unsupported {
            parts.push(json!({"t":"board","ids":[id],"target":999999}));
        }
        let tick = m.world.game.tick;
        let key = OrderKey {
            tick: tick as u64 + 1,
            issuer: 0,
            sequence: 0,
        };
        m.enqueue_command(
            key,
            Command::from_json(&json!({"t":"orders","queue":false,"commands":parts})).unwrap(),
        )
        .unwrap();
        let before = format!("{:?}", (&m.world.game.units, &m.commands.units));
        if unsupported {
            let error = m.advance_tick().unwrap_err();
            assert!(
                format!("{error:?}")
                    .contains("World command requires an unrepresented tick actuator")
            );
            assert_eq!(m.world.game.tick, tick);
            assert_eq!(
                format!("{:?}", (&m.world.game.units, &m.commands.units)),
                before
            );
        } else {
            let report = m.advance_tick().unwrap();
            assert_eq!(report.tick, tick + 1);
            assert_eq!(report.commands.len(), 1);
            assert_eq!(report.commands[0].key, key);
            assert_eq!(report.commands[0].denial, None);
        }
    }
}

#[test]
fn lossless_oracle_numbers_survive_parent_replacement_and_deletion() {
    let mut image = unpack(
        &json!({"raw":{"tick":0,"values":[null,0,null,null],"old":1},"numberBits":{
            "values.0":"7ff8000000000001","values.1":"8000000000000000",
            "values.2":"7ff0000000000000","values.3":"fff0000000000000"
        }}),
    );
    assert_eq!(
        image.get("values").array(),
        &[
            Exact::Number(0x7ff8000000000001),
            Exact::Number(0x8000000000000000),
            Exact::Number(0x7ff0000000000000),
            Exact::Number(0xfff0000000000000),
        ]
    );
    common::advance_image(
        &mut image,
        &json!({"baseTick":0,"tick":1,"patches":[
            {"op":"set","path":["values"],"value":{"raw":{"x":0},"numberBits":{"x":"8000000000000000"}}},
            {"op":"set","path":["values","x"],"value":{"raw":0.1,"numberBits":{"":"3fb999999999999a"}}},
            {"op":"delete","path":["old"]},
            {"op":"set","path":["tick"],"value":{"raw":1,"numberBits":{}}}
        ]}),
    );
    assert_eq!(
        image.get("values").get("x"),
        &Exact::Number(0x3fb999999999999a)
    );
    assert!(image.optional("old").is_none());
}

#[test]
fn original_world_steps_use_the_actual_runner_and_explicit_whole_admission() {
    let fixture = common::fixture();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["format"], 2);
    assert_eq!(fixture["steps"], 118);
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 9);
    assert_eq!(fixture["preservation"]["cases"], 7);
    assert_eq!(fixture["preservation"]["steps"], 114);
    assert_eq!(fixture["preservation"]["phaseEntries"], 1486);
    for (path, reason) in common::GAPS {
        eprintln!("Explicit World comparison gap {path}: {reason}");
    }
    let (mut steps, mut refused, mut authored_wind_steps) = (0, 0, 0);
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        assert_eq!(case["setup"]["sourceRandom"]["nativeEquivalent"], false);
        let mut image = unpack(&case["initial"]);
        // Decode every original frame even when native admission cannot represent its setup.
        // This image is used only as an oracle, never as an input to Match.
        if case["setup"]["options"]["logistics"] == true {
            let error = match common::construct(&fixture, case) {
                Ok(_) => panic!("{name}: unsupported logistics constructor admitted"),
                Err(e) => e,
            };
            assert!(
                error.contains("World startup does not yet initialize logistics"),
                "{name}: {error}"
            );
            for frame in case["frames"].as_array().unwrap() {
                common::advance_image(&mut image, &frame["after"]);
            }
            refused += 1;
            continue;
        }
        let mut m = common::construct(&fixture, case)
            .unwrap_or_else(|e| panic!("{name}: native constructor: {e}"));
        let compare_wind = case["setup"].get("wind").is_some();
        let native_random = m.start.random.snapshot();
        let mut shots = Vec::<Value>::new();
        common::compare_state(
            &m,
            &image,
            &shots,
            &format!("{name}: initial genuine constructor and authored inputs"),
            compare_wind,
        );
        for frame in case["frames"].as_array().unwrap() {
            let context = format!("{name}: tick {}", frame["tick"]);
            // Exact prior poses are taken before the sole actual advance call.
            let prior = json!(
                m.world
                    .game
                    .units
                    .values()
                    .map(|u| json!([
                        u.id,
                        format!("{:016x}", u.x.to_bits()),
                        format!("{:016x}", u.z.to_bits()),
                        format!("{:016x}", u.rot.to_bits())
                    ]))
                    .collect::<Vec<_>>()
            );
            assert_eq!(prior, frame["prior"], "{context}: pre-step prior poses");
            assert_eq!(
                m.start.environment_seed as i32,
                frame["seedBefore"].as_i64().unwrap() as i32,
                "{context}: actual environment stream"
            );
            let report = m
                .advance_tick()
                .unwrap_or_else(|e| panic!("{context}: actual runner refused: {e:?}"));
            steps += 1;
            if compare_wind {
                authored_wind_steps += 1;
            }
            assert_eq!(report.tick, frame["tick"].as_i64().unwrap());
            if let Some(receipts) = frame.get("commandReceipts") {
                let receipts = unpack(receipts);
                assert_eq!(
                    report.commands.len(),
                    receipts.array().len(),
                    "{context}: actual command receipt count"
                );
                for (actual, source) in report.commands.iter().zip(receipts.array()) {
                    assert_eq!(actual.key.tick, frame["tick"].as_u64().unwrap());
                    same(
                        source.get("index"),
                        json!(actual.key.sequence),
                        &format!("{context}: declared command order"),
                    );
                    same(
                        source.get("slot"),
                        json!(actual.key.issuer),
                        &format!("{context}: actual commander"),
                    );
                    same(
                        source.get("result"),
                        json!(actual.denial.map(|d| d.name())),
                        &format!("{context}: actual command admission"),
                    );
                }
            } else {
                assert!(report.commands.is_empty(), "{context}: undeclared commands");
            }
            assert_eq!(report.prior.len(), frame["prior"].as_array().unwrap().len());
            for raw in frame["prior"].as_array().unwrap() {
                let pose = report
                    .prior
                    .get(&(raw[0].as_u64().unwrap() as u32))
                    .unwrap();
                assert_eq!(format!("{:016x}", pose.x.to_bits()), raw[1]);
                assert_eq!(format!("{:016x}", pose.z.to_bits()), raw[2]);
                assert_eq!(format!("{:016x}", pose.rot.to_bits()), raw[3]);
            }
            let added: Vec<_> = report
                .events
                .iter()
                .map(|event| match event {
                    TickEvent::Environment(effect) => effect.to_json(),
                    TickEvent::Firing(event) => event.to_json(&m.rules),
                    other => panic!("{context}: unexpected quiet World producer {other:?}"),
                })
                .collect();
            same(
                &unpack(&frame["shotsAdded"]),
                json!(added),
                &format!("{context}: all chronological effects"),
            );
            assert_eq!(
                added.len(),
                frame["emissions"].as_array().unwrap().len(),
                "{context}: no output loss or duplicate"
            );
            shots.extend(added);
            let captured = unpack(&frame["captured"]);
            same(
                captured.get("ids"),
                json!(report.unit_order),
                &format!("{context}: actual captured late list"),
            );
            let missing: Vec<_> = captured
                .get("identity")
                .array()
                .iter()
                .filter(|v| v.get("current") == &Exact::Bool(false))
                .map(|v| v.get("id").id())
                .collect();
            let removals: Vec<_> = report
                .retired
                .iter()
                .map(|r| r.unit.id)
                .chain(report.world_removed.iter().map(|u| u.id))
                .collect();
            assert_eq!(
                removals, missing,
                "{context}: actual ordinary and administrative removals"
            );
            for record in &report.retired {
                let row = captured
                    .get("facts")
                    .array()
                    .iter()
                    .find(|v| v.get("id").id() == record.unit.id)
                    .unwrap();
                same(
                    row.get("hp"),
                    json!(record.unit.hp),
                    &format!("{context}: final retired captured HP"),
                );
            }
            let phase = |p| {
                report
                    .phases
                    .iter()
                    .position(|v| *v == p)
                    .unwrap_or_else(|| panic!("{context}: missing actual {p:?}"))
            };
            assert!(phase(TickPhase::Death) < phase(TickPhase::WorldIncome));
            assert!(phase(TickPhase::WorldIncome) < phase(TickPhase::Convoys));
            assert!(phase(TickPhase::Convoys) < phase(TickPhase::WorldRegions));
            assert!(
                m.firing.events.is_empty()
                    && m.support.events.is_empty()
                    && m.aviation.events.is_empty(),
                "{context}: output is owned by report"
            );
            assert_eq!(
                m.start.random.snapshot(),
                native_random,
                "{context}: held quiet World must not consume native MatchRandom"
            );
            common::advance_image(&mut image, &frame["after"]);
            common::compare_state(&m, &image, &shots, &context, compare_wind);
        }
    }
    assert_eq!(
        (steps, refused, authored_wind_steps),
        (58, 2, 2),
        "Actual native steps, whole-admission refusals and authored-wind comparisons"
    );
}
