use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    environment::{Blast, Context, DamageContext},
    grid::SpatialGrid,
    legacy_scenario_runtime::step_legacy_scenario_emitting,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat, SnapshotCache},
    weather::WeatherChoice,
    world::Recipient,
};
use std::collections::BTreeSet;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/recipient-falling.json")).unwrap()
}
fn bits(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(format!(
            "{:016x}",
            n.as_f64().unwrap().to_bits().swap_bytes()
        )),
        Value::Array(rows) => Value::Array(rows.iter().map(bits).collect()),
        Value::Object(fields) => {
            Value::Object(fields.iter().map(|(k, v)| (k.clone(), bits(v))).collect())
        }
        _ => value.clone(),
    }
}
fn initial(trace: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&trace["map"]).unwrap()).unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: trace["teams"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(slot, team)| PlayerConfig {
                    name: format!("P{slot}"),
                    team: team.as_i64().unwrap() as i32,
                    faction: Faction::try_from(slot as u8).unwrap(),
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
    .unwrap();
    // Complete original constructor facts, before any damage or filtering.
    // Preserve every canonical actor and sidecar; only declared initial poses
    // are restored, independent of the expected falling outputs.
    let rows = trace["initial"]["units"].as_array().unwrap();
    assert_eq!(state.world.game.units.len(), rows.len());
    for row in rows {
        let unit = state
            .world
            .game
            .units
            .get_mut(&(row["id"].as_u64().unwrap() as u32))
            .unwrap();
        assert_eq!(
            state.rules.kinds[unit.kind].name,
            row["type"].as_str().unwrap()
        );
        assert_eq!(unit.owner, row["owner"].as_u64().unwrap() as usize);
        assert_eq!(unit.hp.to_bits(), row["hp"].as_f64().unwrap().to_bits());
        unit.x = row["x"].as_f64().unwrap();
        unit.z = row["z"].as_f64().unwrap();
    }
    state.grid = SpatialGrid::new(&state.world.game.units, 16.);
    assert_eq!(
        u64::from(state.start.environment_seed),
        trace["initial"]["seed"].as_u64().unwrap()
    );
    assert_eq!(
        bits(&json!({"a":state.start.wind.0,"v":state.start.wind.1})),
        trace["initial"]["wind"]
    );
    state
}
fn step(state: &mut Match, count: usize) {
    let order = state.world.game.units.keys().copied().collect::<Vec<_>>();
    let mut context = Context {
        tick: state.world.game.tick as u64,
        seed: &mut state.start.environment_seed,
        next_id: &mut state.next_unit_id,
        unit_order: &order,
        grid: &state.grid,
        rain: state.world.game.rain,
        wet: state.world.game.wet.unwrap_or(0.),
        wind_angle: state.start.wind.0,
        wind_strength: state.start.wind.1,
    };
    for _ in 0..count {
        state
            .environment
            .step_sections(&mut state.world, 0.05, &mut context)
            .unwrap();
    }
}
fn ordinary_destroy(state: &mut Match) {
    let order = state.world.game.units.keys().copied().collect::<Vec<_>>();
    let skipped = BTreeSet::new();
    let mut hit = DamageContext::plain(&skipped);
    let mut context = Context {
        tick: state.world.game.tick as u64,
        seed: &mut state.start.environment_seed,
        next_id: &mut state.next_unit_id,
        unit_order: &order,
        grid: &state.grid,
        rain: state.world.game.rain,
        wet: state.world.game.wet.unwrap_or(0.),
        wind_angle: state.start.wind.0,
        wind_strength: state.start.wind.1,
    };
    for x in [40, 50] {
        let contact = format!("ordinary:{x}:{}", 25 * state.world.game.w + x);
        hit.contact_id = Some(&contact);
        state
            .environment
            .damage_cells(
                &mut state.world,
                Blast {
                    at: Point {
                        x: (x as f64 + 0.5) * crate::game::CELL,
                        z: 25.5 * crate::game::CELL,
                    },
                    radius: crate::game::CELL * 0.49,
                    damage: 100_000.,
                    scar: 0.,
                },
                &mut context,
                &mut hit,
            )
            .unwrap();
    }
}
fn apply(state: &mut Match, action: &Value, ordinary: bool) {
    if let Some(tick) = action["tick"].as_i64() {
        state.world.game.tick = tick;
    }
    if let Some(rows) = action["move"].as_array() {
        for row in rows {
            let unit = state
                .world
                .game
                .units
                .get_mut(&(row["id"].as_u64().unwrap() as u32))
                .unwrap();
            unit.x = row["x"].as_f64().unwrap();
            unit.z = row["z"].as_f64().unwrap();
        }
    }
    if let Some(reveal) = action["reveal"].as_bool() {
        state.world.game.reveal = reveal;
    }
    state.grid = SpatialGrid::new(&state.world.game.units, 16.);
    if action["destroy"] == true {
        if ordinary {
            ordinary_destroy(state);
        } else {
            let tick = state.world.game.tick as u64;
            step_legacy_scenario_emitting(state, tick, &mut |_| {}).unwrap();
        }
    }
    step(state, action["steps"].as_u64().unwrap_or(0) as usize);
}

#[test]
fn actual_bridge_damage_and_motion_match_whole_source_falling_lists() {
    let fixture = fixture();
    for trace in fixture["traces"].as_array().unwrap() {
        let mut state = initial(trace);
        let random = state.start.random.snapshot();
        let ordinary = trace["name"] == "ordinary";
        for action in trace["actions"].as_array().unwrap() {
            apply(&mut state, action, ordinary);
            assert_eq!(
                state.environment.falling_sections().len() as u64,
                action["count"].as_u64().unwrap()
            );
            assert_eq!(
                u64::from(state.start.environment_seed),
                action["seed"].as_u64().unwrap()
            );
            assert_eq!(state.start.random.snapshot(), random);
            if action["sourceOnly"] == true {
                continue;
            }
            let memories = format!("{:?}", state.recipients);
            let pending = (0..state.recipients.len())
                .map(|slot| format!("{:?}", state.world.pending(Recipient::Player(slot))))
                .collect::<Vec<_>>();
            for output in action["outputs"].as_array().unwrap() {
                let slot = output["slot"].as_u64().unwrap() as usize;
                assert_eq!(
                    bits(&project(&state, slot).unwrap()),
                    output["falling"],
                    "{} / {} / slot{slot}",
                    trace["name"],
                    action["name"]
                );
                if ordinary {
                    assert_eq!(
                        bits(
                            &SnapshotCache::new(&state)
                                .unwrap()
                                .recipient(slot)
                                .unwrap()
                                .fields["falling"]
                        ),
                        output["falling"]
                    );
                }
            }
            assert_eq!(format!("{:?}", state.recipients), memories);
            assert_eq!(
                (0..state.recipients.len())
                    .map(|slot| format!("{:?}", state.world.pending(Recipient::Player(slot))))
                    .collect::<Vec<_>>(),
                pending
            );
        }
    }
}

#[test]
fn actual_falling_selected_delivery_keeps_order_and_skipped_memory() {
    let fixture = fixture();
    let trace = fixture["traces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "ordinary")
        .unwrap();
    let mut state = initial(trace);
    let mut delivery = DeliveryState::new(&state).unwrap();
    for action in trace["actions"].as_array().unwrap() {
        apply(&mut state, action, true);
        if action["sourceOnly"] == true {
            continue;
        }
        let outputs = action["outputs"].as_array().unwrap();
        let selected = outputs
            .iter()
            .map(|output| {
                let slot = output["slot"].as_u64().unwrap() as usize;
                let input = action["selected"].as_array().and_then(|rows| {
                    rows.iter()
                        .find(|s| s["slot"].as_u64() == Some(slot as u64))
                });
                SelectedSeat {
                    slot,
                    full: input.is_some_and(|i| i["full"] == true),
                    omit: input.is_some_and(|i| i["omit"] == true),
                }
            })
            .collect::<Vec<_>>();
        let skipped = (0..state.recipients.len())
            .filter(|slot| !selected.iter().any(|s| s.slot == *slot))
            .map(|slot| {
                (
                    slot,
                    format!("{:?}", state.recipients[slot]),
                    format!("{:?}", state.world.pending(Recipient::Player(slot))),
                    delivery.seat_fog_for_test(slot),
                )
            })
            .collect::<Vec<_>>();
        {
            let frame = delivery
                .capture_selected_with_lists(&mut state, &selected, &[])
                .unwrap()
                .unwrap();
            assert_eq!(
                frame.slots().collect::<Vec<_>>(),
                selected.iter().map(|s| s.slot).collect::<Vec<_>>()
            );
            for output in outputs {
                let slot = output["slot"].as_u64().unwrap() as usize;
                let fields = frame.recipient(slot).unwrap().into_json();
                assert_eq!(
                    bits(&fields["falling"]),
                    output["falling"],
                    "{} / slot{slot}",
                    action["name"]
                );
                if selected.iter().find(|s| s.slot == slot).unwrap().omit {
                    assert!(fields.get("fog").is_none());
                }
            }
        }
        for (slot, memory, pending, fog) in skipped {
            assert_eq!(format!("{:?}", state.recipients[slot]), memory);
            assert_eq!(
                format!("{:?}", state.world.pending(Recipient::Player(slot))),
                pending
            );
            assert_eq!(delivery.seat_fog_for_test(slot), fog);
        }
    }
}
