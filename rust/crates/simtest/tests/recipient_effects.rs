#![recursion_limit = "256"]
//! Pure ordinary RTS projection from original Source boundary inputs and events.
//! This test does not replay gameplay, restore MatchRandom, or adopt transport policy.
//! Only complete `shots` rows are compared. Other snapshot fields, dormant ordinary
//! state, and source-only falling bodies are retained in the fixture but not claimed.
#[allow(dead_code)]
#[path = "common/companion_tick.rs"]
mod common;

use flate2::read::GzDecoder;
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    economy::Mode,
    environment::{Collapse, Effect, Hurt},
    game::{Point, Strike, StrikeKind},
    match_aviation::AviationEvent,
    match_firing::{ContactEvent, LaunchEvent, LiveFlight, SmallArmsEvent},
    match_runtime::Match,
    match_support::{SupportEvent, SupportKind, SupportTiming},
    match_tick::TickEvent,
    projectiles::{Flight, Point3},
    recipient_snapshot::{DeliveryState, SelectedSeat, SnapshotCapability, SnapshotError},
    rules::Rules,
};

fn fixture() -> Value {
    let mut text = String::new();
    GzDecoder::new(include_bytes!("fixtures/recipient-effects.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["seed"], 701);
    assert_eq!(
        value["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    value
}
fn n(v: &Value) -> f64 {
    v.as_f64().expect("recorded finite numeric input")
}
fn id(v: &Value) -> u32 {
    n(v) as u32
}
fn point(v: &Value) -> Point {
    Point {
        x: n(&v["x"]),
        z: n(&v["z"]),
    }
}
fn point3(v: &Value) -> Point3 {
    Point3::new(n(&v["x"]), n(&v["y"]), n(&v["z"]))
}
fn vector(v: &Value) -> Point3 {
    Point3::new(n(&v[0]), n(&v[1]), n(&v[2]))
}
fn kind(rules: &Rules, v: &Value) -> usize {
    rules.kind(v.as_str().unwrap()).unwrap()
}
fn text(v: &Value) -> Option<&'static str> {
    if v.is_null() {
        return None;
    }
    Some(match v.as_str().unwrap() {
        "steel" => "steel",
        "soil" => "soil",
        "stone" => "stone",
        "timber" => "timber",
        other => panic!("unrepresented static producer field {other}"),
    })
}
fn expand(v: &Value) -> Vec<Value> {
    let mut values = Vec::new();
    for row in v.as_array().unwrap() {
        values.extend(std::iter::repeat_n(row[0].clone(), n(&row[1]) as usize));
    }
    values
}
fn float32(v: &Value) -> Vec<f32> {
    expand(v)
        .iter()
        .map(|v| {
            let value = n(v) as f32;
            assert_eq!(
                f64::from(value).to_bits(),
                n(v).to_bits(),
                "recorded Float32 input bits"
            );
            value
        })
        .collect()
}

fn make(trace: &Value, source: &Value) -> Match {
    assert!(source["operatives"].as_array().unwrap().is_empty());
    assert_eq!(source["skipFog"], false);
    assert!(source["falling"].as_array().unwrap().is_empty());
    let adapted =
        json!({"game":source,"players":source["players"],"units":source["units"],"operatives":[]});
    let mut state = common::make(&trace["setup"], &adapted, Mode::Conquest);
    let random = state.start.random.snapshot();
    let terrain = &source["terrain"];
    let chars = expand(&terrain["chars"]);
    assert_eq!(chars.len(), state.world.game.chars.len());
    for (cell, ch) in chars.iter().enumerate() {
        let ch = ch.as_str().unwrap().as_bytes()[0];
        if state.world.game.chars[cell] != ch {
            state.world.set_cell(cell, ch).unwrap();
        }
    }
    // Trusted boundary facts use canonical World mutation for letters and public
    // typed scalar storage. No expected recipient rows or random stream are installed.
    state.world.game.flags = expand(&terrain["flags"])
        .iter()
        .map(|v| n(v) as u16)
        .collect();
    state.world.game.cell_hp = float32(&terrain["cellHp"]);
    state.world.game.wear = Some(float32(&terrain["wear"]));
    state.world.scar = float32(&terrain["scar"]);
    state.world.burnt = expand(&terrain["burnt"])
        .iter()
        .map(|v| id(v) as u8)
        .collect();
    state.world.cell_state = expand(&terrain["cellState"])
        .iter()
        .map(|v| id(v) as u8)
        .collect();
    state.world.game.terrain_version = id(&terrain["versions"][0]);
    assert!(terrain["height"].is_null(), "these original maps are flat");
    state.world.fires.clear();
    for row in source["environment"]["fires"].as_array().unwrap() {
        state.world.fires.insert(id(&row[0]) as usize, n(&row[1]));
    }
    assert!(source["smokes"].as_array().unwrap().is_empty());
    assert!(source["covers"].as_array().unwrap().is_empty());
    assert!(
        source["environment"]["wrecks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    state.world.game.strikes = source["strikes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            assert_eq!(s["kind"], "artillery");
            Strike {
                live: s["live"] == true,
                kind: StrikeKind::Other,
                owner: id(&s["owner"]) as usize,
                x: n(&s["x"]),
                z: n(&s["z"]),
                dir: n(&s["dir"]),
                runtime: Some(SupportTiming {
                    kind: SupportKind::Artillery,
                    delay: n(&s["t"]),
                    left: n(&s["left"]),
                    next: n(&s["next"]),
                }),
            }
        })
        .collect();
    assert_eq!(state.start.random.snapshot(), random);
    state
}

// Import unredacted original producer payloads, in their recorded append order.
// Unknown payloads fail; recipient `outputs` never supply event or actor inputs.
fn event(rules: &Rules, row: &Value, support: bool) -> TickEvent {
    let firing = match row["k"].as_str().unwrap() {
        "flight" => LaunchEvent::Flight {
            id: n(&row["flight"]) as u64,
            kind: kind(rules, &row["kind"]),
            effect: text(&row["effect"]),
            shooter: id(&row["f"]),
            owner: id(&row["fo"]) as usize,
            at: point3(row),
            velocity: vector(&row["v"]),
            gravity: n(&row["gravity"]),
            time: n(&row["time"]),
            expires: n(&row["expires"]),
        },
        "contact" => LaunchEvent::Contact(ContactEvent {
            kind: kind(rules, &row["kind"]),
            flight: n(&row["flight"]) as u64,
            sequence: id(&row["seq"]),
            contact_id: row["contactId"].as_str().unwrap().into(),
            time: n(&row["time"]),
            at: point3(row),
            shooter: id(&row["f"]),
            owner: id(&row["fo"]) as usize,
            target: row.get("t").map(id),
            target_owner: row.get("to").map(|v| id(v) as usize),
            hit: row["hit"] == true,
            killed: row["kill"] == true,
            terminal: row["terminal"] == true,
            explosive: row["explosive"] == true,
            effect: text(&row["effect"]),
            blast: row.get("blast").map(n),
            normal: vector(&row["normal"]),
            direction: vector(&row["direction"]),
            impulse: n(&row["impulse"]),
            material: text(&row["material"]).unwrap(),
        }),
        "shell" => {
            assert!(support);
            return TickEvent::Support(SupportEvent::Shell(point(row)));
        }
        "hurt" => {
            assert!(support);
            return TickEvent::Support(SupportEvent::Environment(Effect::Hurt(Hurt {
                unit: id(&row["t"]),
                owner: id(&row["to"]) as usize,
                at: point(row),
                killed: row["kill"] == true,
                from: row.get("fo").map(|v| n(v) as i32),
                local: row["local"] == true,
            })));
        }
        "collapse" => {
            assert!(support);
            return TickEvent::Support(SupportEvent::Environment(Effect::Collapse(Collapse {
                id: row["id"].as_str().unwrap().into(),
                at: n(&row["at"]),
                position: point(row),
                section: row.get("section").map(|v| v.as_str().unwrap().into()),
                impulse: row.get("impulse").map(n),
                direction: row.get("dir").map(n),
                material: row.get("material").map(|v| v.as_str().unwrap().into()),
                entity: row.get("t").map(id),
                owner: row.get("to").map(|v| id(v) as usize),
            })));
        }
        other => panic!("unclassified original ordinary event {other}"),
    };
    TickEvent::Firing(firing)
}
fn source_wire(mut value: Value) -> Value {
    fn zero(v: &mut Value) {
        match v {
            Value::Number(n) if n.as_f64() == Some(0.0) => *v = json!(0),
            Value::Array(v) => v.iter_mut().for_each(zero),
            Value::Object(v) => v.values_mut().for_each(zero),
            _ => {}
        }
    }
    zero(&mut value);
    value
}
fn raw(events: &[TickEvent], rules: &Rules) -> Value {
    json!(
        events
            .iter()
            .map(|e| match e {
                TickEvent::Firing(e) => e.to_json(rules),
                TickEvent::Support(SupportEvent::Shell(p)) =>
                    json!({"k":"shell","x":p.x,"z":p.z,"pub":true}),
                TickEvent::Support(SupportEvent::Environment(e)) | TickEvent::Environment(e) =>
                    e.to_json(),
                _ => panic!("unrepresented caller batch type"),
            })
            .collect::<Vec<_>>()
    )
}
fn live_flight(state: &mut Match, source: &Value) {
    assert_eq!(source["flights"].as_array().unwrap().len(), 1);
    let p = &source["flights"][0];
    assert!(p.get("blastSource").is_none());
    let flight = LiveFlight {
        id: n(&p["id"]) as u64,
        owner: id(&p["owner"]) as usize,
        shooter: id(&p["shooter"]),
        source_kind: kind(&state.rules, &p["kind"]),
        solution: Flight {
            origin: point3(p),
            velocity: Point3::new(n(&p["vx"]), n(&p["vy"]), n(&p["vz"])),
            gravity: n(&p["gravity"]),
            stamp: n(&p["stamp"]),
        },
        launch_origin: point3(&p["launchOrigin"]),
        launched: n(&p["launched"]),
        last: n(&p["last"]),
        expires: n(&p["expires"]),
        sent: p["sent"] == true,
        energy: n(&p["energy"]),
        sequence: id(&p["sequence"]),
        max_contacts: id(&p["maxContacts"]),
        near: n(&p["near"]),
        start_cell: id(&p["startCell"]) as usize,
        intended: p.get("intended").map(id),
        damage_scale: n(&p["damageScale"]),
        armor_piercing: p["ap"] == true,
        ap_multiplier: n(&p["apMultiplier"]),
        ap_naval: p["apNaval"] == true,
        suppression: n(&p["suppression"]),
        suppression_scale: n(&p["suppressionScale"]),
        volley_group: n(&p["id"]) as u64,
        explosive: p["explosive"] == true,
        terrain: p.get("terrain").map(n).unwrap_or(0.0),
        scar: p.get("scar").map(n).unwrap_or(0.0),
        blast_radius: p.get("blastRadius").map(n),
        blast_source: None,
        effect: text(&p["effect"]),
    };
    state.firing.suppressed.insert(
        flight.volley_group,
        p["suppressed"].as_array().unwrap().iter().map(id).collect(),
    );
    state.firing.flights.insert(flight.id, flight);
}
fn seats(frame: &Value) -> Vec<SelectedSeat> {
    frame["selected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| SelectedSeat {
            slot: id(v) as usize,
            full: false,
            omit: false,
        })
        .collect()
}
fn receipt(state: &mut Match, air: bool) -> DeliveryState {
    if !air {
        return DeliveryState::new(state).unwrap();
    }
    // The actual recorded AIR actor prevents complete admission. Temporarily
    // withhold that one input row to admit this same Match's receipt owner, then
    // restore it before the refusal call. No baseline frame is published.
    let ids: Vec<_> = state
        .world
        .game
        .units
        .values()
        .filter(|u| state.rules.kinds[u.kind].air)
        .map(|u| u.id)
        .collect();
    assert_eq!(ids.len(), 1);
    let actor = state.world.game.units.remove(&ids[0]).unwrap();
    let result = DeliveryState::new(state);
    state.world.game.units.insert(actor.id, actor);
    result.unwrap()
}
fn guard(state: &Match, delivery: &DeliveryState) -> String {
    // Public owned state only. Private routing/fog atomicity also has core tests;
    // this observer performs no recipient query that could obscure AIR refusal.
    format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        state.world,
        state.commands.units,
        state.firing,
        state.support,
        state.combat,
        state.aviation,
        state.abilities,
        state.recovery,
        state.environment,
        state.start.economy,
        state.start.players,
        state.start.random.snapshot(),
        delivery,
    )
}

#[test]
fn original_ordinary_batches_match_selected_shot_rows_and_numeric_bits() {
    let fixture = fixture();
    let (
        mut traces,
        mut frames,
        mut recipients,
        mut shots,
        mut supplied,
        mut empty,
        mut air,
        mut flights,
        mut falling,
    ) = (0, 0, 0, 0, 0, 0, 0, 0, 0);
    for trace in fixture["traces"].as_array().unwrap() {
        if trace["spec"]["kind"] == "companion" || trace["spec"]["skipFog"] == true {
            continue;
        }
        traces += 1;
        let name = trace["name"].as_str().unwrap();
        let mut recorded = trace["initial"].clone();
        let initial = common::decoded(&recorded);
        let seed_state = make(trace, &initial);
        let rules = &seed_state.rules;
        common::apply_delta(&mut recorded, &trace["admittedDelta"]);
        let mut batch = Vec::new();
        for frame in trace["frames"].as_array().unwrap() {
            common::apply_delta(&mut recorded, &frame["beforeDelta"]);
            let before = common::decoded(&recorded);
            let emitted = common::decoded(&frame["generated"]);
            batch.extend(emitted.as_array().unwrap().iter().map(|row| {
                event(
                    rules,
                    row,
                    trace["spec"]["kind"] == "support" || trace["spec"]["kind"] == "falling",
                )
            }));
            common::exact(
                &source_wire(raw(&batch, rules)),
                &source_wire(before["shots"].clone()),
                &format!("{name}: unredacted chronological batch"),
            );
            if frame["sent"] == true {
                let selected = seats(frame);
                if frame["refusals"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v == "falling")
                {
                    assert!(!before["falling"].as_array().unwrap().is_empty());
                    assert!(
                        common::decoded(&frame["outputs"])
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|v| !v["fields"]["falling"].as_array().unwrap().is_empty())
                    );
                    falling += 1; // Source witness only. No private body import or native claim.
                } else {
                    let mut state = make(trace, &before);
                    // Each boundary has its own Match and fresh receipt owner.
                    // No acknowledgments cross reconstructed Match instances.
                    let mut delivery = receipt(
                        &mut state,
                        frame["refusals"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|v| v == "air"),
                    );
                    if !before["flights"].as_array().unwrap().is_empty() {
                        live_flight(&mut state, &before);
                    }
                    let random = state.start.random.snapshot();
                    let state_before = guard(&state, &delivery);
                    let result =
                        delivery.capture_selected_with_tick_events(&mut state, &selected, &batch);
                    if frame["refusals"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|v| v == "air" || v == "live-flights")
                    {
                        let capability = if frame["refusals"][0] == "air" {
                            air += 1;
                            SnapshotCapability::Air
                        } else {
                            flights += 1;
                            SnapshotCapability::ShotsAndFlights
                        };
                        assert!(
                            matches!(result, Err(SnapshotError::Unsupported { capability: c, .. }) if c == capability)
                        );
                        assert_eq!(guard(&state, &delivery), state_before);
                    } else if selected.is_empty() {
                        assert!(result.unwrap().is_none());
                        assert_eq!(guard(&state, &delivery), state_before);
                        assert_eq!(batch.len(), 2);
                        empty += 1;
                    } else {
                        let view = result.unwrap().unwrap();
                        assert_eq!(
                            view.slots().collect::<Vec<_>>(),
                            selected.iter().map(|s| s.slot).collect::<Vec<_>>()
                        );
                        let expected = common::decoded(&frame["outputs"]);
                        for row in expected.as_array().unwrap() {
                            let actual = view
                                .recipient(id(&row["slot"]) as usize)
                                .unwrap()
                                .into_json();
                            common::exact(
                                &actual["shots"],
                                &row["fields"]["shots"],
                                &format!("{name}:{} seat{} shots", frame["tick"], row["slot"]),
                            );
                            recipients += 1;
                            shots += row["fields"]["shots"].as_array().unwrap().len();
                        }
                        frames += 1;
                        supplied += batch.len();
                    }
                    assert_eq!(state.start.random.snapshot(), random);
                }
                // Source resets each scheduled batch regardless of eligible seats.
                // Native refusal witnesses establish no delivery or host fault
                // policy. Accumulation and reset remain caller-owned.
                batch.clear();
            }
            common::apply_delta(&mut recorded, &frame["afterDelta"]);
            common::exact(
                &source_wire(raw(&batch, rules)),
                &source_wire(common::decoded(&recorded)["shots"].clone()),
                &format!("{name}: after scheduled reset"),
            );
        }
        common::apply_delta(&mut recorded, &trace["finalDelta"]);
    }
    assert_eq!(
        (
            traces, frames, recipients, shots, supplied, empty, air, flights, falling
        ),
        (7, 157, 785, 90, 24, 1, 1, 1, 4)
    );
}

#[test]
fn caller_batch_refusals_do_not_publish_a_valid_original_prefix() {
    let fixture = fixture();
    let trace = fixture["traces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "local-hidden-shooter-and-fog-bound")
        .unwrap();
    let source = common::decoded(&trace["initial"]);
    let mut state = make(trace, &source);
    let mut delivery = DeliveryState::new(&state).unwrap();
    let valid = common::decoded(&trace["frames"][0]["generated"]);
    let prefix = event(&state.rules, &valid[0], false);
    let selected = [SelectedSeat {
        slot: 0,
        full: false,
        omit: false,
    }];
    for (row, capability) in [
        (
            TickEvent::Environment(Effect::LastHit { unit: 1, owner: 0 }),
            SnapshotCapability::ShotsAndFlights,
        ),
        (
            TickEvent::Firing(LaunchEvent::Throw {
                shooter: 1,
                owner: 0,
                at: point(&source["units"][0]),
            }),
            SnapshotCapability::ShotsAndFlights,
        ),
        (
            TickEvent::Firing(LaunchEvent::SmallArms(SmallArmsEvent {
                effect: "sniper",
                shooter: 1,
                owner: 0,
                at: point3(&valid[0]),
                hit: None,
            })),
            SnapshotCapability::ShotsAndFlights,
        ),
        (
            TickEvent::Aviation(AviationEvent::PlaneDown {
                target: 1,
                owner: 0,
                at: point(&source["units"][0]),
                direction: 0.0,
            }),
            SnapshotCapability::Air,
        ),
    ] {
        let batch = [prefix.clone(), row];
        let before = guard(&state, &delivery);
        assert!(
            matches!(delivery.capture_selected_with_tick_events(&mut state, &selected, &batch),
            Err(SnapshotError::Unsupported { capability: c, .. }) if c == capability)
        );
        assert_eq!(guard(&state, &delivery), before);
        // Empty selection bypasses admission and never consumes the caller's batch.
        assert!(
            delivery
                .capture_selected_with_tick_events(&mut state, &[], &batch)
                .unwrap()
                .is_none()
        );
        assert_eq!(guard(&state, &delivery), before);
        assert_eq!(batch.len(), 2);
    }
}
