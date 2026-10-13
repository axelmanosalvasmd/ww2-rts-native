//! Actual original full-step observations through native quiet policy and human input phases.
use super::*;
use crate as core_api;
use crate::{
    commander_ai::{Input, PlayerObservation, PointObservation, UnitObservation},
    random::StreamId,
};
#[path = "../tests/support/human_commander_facts.rs"]
mod facts;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../tests/fixtures/human-commander-policy.json"
    ))
    .unwrap()
}

fn packet(frame: &Value, owner: usize) -> Observation {
    Observation::new(Input {
        width: frame["w"].as_u64().unwrap() as usize,
        height: frame["h"].as_u64().unwrap() as usize,
        tick: frame["tick"].as_i64().unwrap(),
        slot: owner,
        players: frame["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| PlayerObservation {
                team: p["team"].as_i64().unwrap() as i32,
                out: facts::truthy(&p["out"]),
                spawn: facts::point(&p["spawn"]),
            })
            .collect(),
        units: frame["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| UnitObservation {
                id: u["id"].as_u64().unwrap() as u32,
                kind: u["type"].as_str().unwrap().into(),
                owner: u["owner"].as_u64().unwrap() as usize,
                at: facts::point(u),
                hp: u["hp"].as_f64().unwrap(),
                supp: u["supp"].as_f64().unwrap(),
                target_id: u["targetId"].as_u64().unwrap() as u32,
                cover: u["cover"].as_u64().unwrap() as u8,
                path: u["path"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(facts::point)
                    .collect(),
                attack_id: facts::truthy(&u["attackId"]),
                retreating: facts::truthy(&u["retreating"]),
                riding: u["flags"].as_u64().unwrap() & 262_144 != 0,
                build: facts::truthy(&u["build"]),
                dig: facts::truthy(&u["dig"]),
                entrench: facts::truthy(&u["entrench"]),
                garrison: u["garrison"].as_i64().is_some_and(|n| n >= 0),
                enter: u["enter"].as_i64().is_some_and(|n| n >= 0),
                fire_at: u["fireAt"].as_i64().is_some_and(|n| n >= 0),
                nade: facts::truthy(&u["nade"]),
            })
            .collect(),
        points: frame["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| PointObservation {
                at: facts::point(p),
                vp: p["vp"].as_f64().unwrap(),
                mp: p["mp"].as_f64().unwrap(),
                owner: p["owner"].as_u64().map(|n| n as usize),
                cut: facts::truthy(&p["cut"]),
                locked: facts::truthy(&p["locked"]),
            })
            .collect(),
        chars: frame["chars"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| {
                std::iter::repeat_n(
                    r[0].as_str().unwrap().as_bytes()[0],
                    r[1].as_u64().unwrap() as usize,
                )
            })
            .collect(),
        flags: frame["flags"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| {
                std::iter::repeat_n(
                    r[0].as_u64().unwrap() as u16,
                    r[1].as_u64().unwrap() as usize,
                )
            })
            .collect(),
        manpower: frame["me"]["mp"].as_f64().unwrap(),
        research: None,
    })
    .unwrap()
}

fn delivered(record: &Value, owner: usize) -> HumanView<Observation> {
    let facts = facts::view(&record["viewSnapshot"]);
    HumanView {
        tick: facts.tick,
        spawn: facts.spawn,
        units: facts.units,
        visible: facts.visible,
        sightings: facts.sightings,
        packet: packet(&record["packetFrame"], owner),
    }
}

fn control(row: &Value) -> TickControl<'_> {
    let c = &row["control"];
    TickControl {
        now: c["now"].as_u64().unwrap(),
        live_owner_out: c["live_owner_out"].as_bool().unwrap(),
        winner: c["winner"].as_bool().unwrap(),
        level: c["level"].as_str().unwrap(),
    }
}

fn level(name: &str) -> Level {
    match name {
        "easy" => Level::Easy,
        "hard" => Level::Hard,
        _ => Level::Normal,
    }
}

#[test]
fn original_quiet_policy_runs_on_human_demand_before_delayed_input_attempts() {
    let fixture = fixture();
    let mut calls = 0;
    let mut physical = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let owner = case["slot"].as_u64().unwrap() as u32;
        let mut human = HumanState::new(
            case["initializationSeed"].as_i64().unwrap() as u32,
            owner,
            facts::point(&case["sourceSpawn"]),
        );
        let mut source = SourceState::default();
        let mut latest = delivered(&case["deliveries"][0], owner as usize);
        let mut inputs = Vec::new();
        for row in case["batches"].as_array().unwrap() {
            let now = row["tick"].as_u64().unwrap();
            if let Some(view) = case["deliveries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["tick"].as_u64() == Some(now))
            {
                latest = delivered(view, owner as usize);
            }
            let result = human
                .advance(control(row), Some(&latest), &facts::batch(row), |screen| {
                    let expected = case["policyCalls"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|p| p["tick"].as_u64() == Some(now))
                        .unwrap();
                    assert!(!commander_ai::due(
                        now as i64,
                        owner as usize,
                        level(case["level"].as_str().unwrap())
                    ));
                    let observation = human_screen_observation(&screen).map_err(Error::Planner)?;
                    assert_eq!(observation, packet(&expected["frame"], owner as usize));
                    let fractions = expected["fractions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|n| n.as_f64().unwrap())
                        .collect::<Vec<_>>();
                    let plan = commander_ai::plan_with_fractions(
                        &observation,
                        &mut source,
                        level(case["level"].as_str().unwrap()),
                        &fractions,
                    )
                    .unwrap();
                    assert_eq!(plan.draws, fractions.len());
                    let raw = plan
                        .commands
                        .iter()
                        .map(raw_command)
                        .collect::<Result<Vec<_>, _>>()?;
                    let attempts = expected["attempts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|a| {
                            assert_eq!(a["returned"]["kind"], "undefined");
                            a["command"].clone()
                        })
                        .collect::<Vec<_>>();
                    assert!(facts::exact(&json!(raw), &json!(attempts)));
                    calls += 1;
                    Ok(raw)
                })
                .unwrap();
            assert!(result.new_alerts.is_empty());
            if let Some(input) = result.input {
                inputs.push(facts::input_receipt(&input));
                physical += 1;
            }
        }
        let expected = &case["policyCalls"][0]["memoryAfter"]["aim"];
        let actual = source.aim.unwrap();
        assert_eq!(actual.i, expected["i"].as_u64().unwrap() as usize);
        assert_eq!(
            actual.until.to_bits(),
            expected["until"].as_f64().unwrap().to_bits()
        );
        let mut expected = case["inputs"].clone();
        for row in expected.as_array_mut().unwrap() {
            row.as_object_mut().unwrap().remove("accepted");
        }
        assert!(facts::exact(&json!(inputs), &expected));
    }
    assert_eq!((calls, physical), (3, 6));
}

#[test]
fn actual_policy_purpose_stream_is_replayable_and_failed_projection_publishes_neither_provider() {
    let fixture = fixture();
    let case = &fixture["cases"][1];
    let owner = case["slot"].as_u64().unwrap() as u32;
    let seed = fixture["seed"].as_u64().unwrap();
    let mut a = HumanState::new(
        case["initializationSeed"].as_i64().unwrap() as u32,
        owner,
        facts::point(&case["sourceSpawn"]),
    );
    let mut b = a.clone();
    let mut pa = HumanPolicy::new(owner, seed);
    let mut pb = pa.clone();
    let initial_rng = pa.random.clone();
    let mut latest = delivered(&case["deliveries"][0], owner as usize);
    let plan_tick = case["policyCalls"][0]["tick"].as_u64().unwrap();
    for row in case["batches"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["tick"].as_u64().unwrap() <= plan_tick)
    {
        let now = row["tick"].as_u64().unwrap();
        if let Some(view) = case["deliveries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["tick"].as_u64() == Some(now))
        {
            latest = delivered(view, owner as usize);
        }
        if now == plan_tick {
            let mut forged = latest.clone();
            forged.units[0].hp -= 1.0;
            let before = (a.clone(), pa.clone());
            let (mut rejected_state, mut rejected_policy) = before.clone();
            assert!(
                advance(
                    &mut rejected_state,
                    &mut rejected_policy,
                    control(row),
                    Some(&forged),
                    &facts::batch(row)
                )
                .is_err()
            );
            assert_eq!((rejected_state, rejected_policy), before);
        }
        assert_eq!(
            advance(
                &mut a,
                &mut pa,
                control(row),
                Some(&latest),
                &facts::batch(row)
            )
            .unwrap(),
            advance(
                &mut b,
                &mut pb,
                control(row),
                Some(&latest),
                &facts::batch(row)
            )
            .unwrap()
        );
        assert_eq!(a, b);
        assert_eq!(pa, pb);
    }
    let mut expected = initial_rng;
    for _ in 0..3 {
        expected.next_fraction(StreamId::Ai(owner));
    }
    assert_eq!(pa.random, expected);
    assert!(pa.source.aim.is_some());
    assert_eq!(pa.random.snapshot().streams.len(), 1);
}
