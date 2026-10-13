//! Engineering recipient reads at actual source command boundaries and full ticks.
//! Scene actors use the canonical factory; no expected work, path or project is installed.
use flate2::read::GzDecoder;
use serde_json::{Value, json};
use std::{collections::BTreeSet, io::Read};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::{TickPathConfig, TickPhase},
    recipient_snapshot::DeliveryState,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};

fn tagged(value: Value) -> Value {
    match value {
        Value::Number(number) => {
            json!({"f64":format!("{:016x}", number.as_f64().unwrap().to_bits())})
        }
        Value::Array(values) => Value::Array(values.into_iter().map(tagged).collect()),
        Value::Object(values) => {
            Value::Object(values.into_iter().map(|(k, v)| (k, tagged(v))).collect())
        }
        value => value,
    }
}
fn number(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}
fn make(case: &Value) -> Match {
    let setup = &case["setup"];
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&setup["map"]).unwrap()).unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: match setup["nativeMode"].as_str().unwrap() {
                "default" => Mode::Default,
                "conquest" => Mode::Conquest,
                mode => panic!("unrepresented engineering mode {mode}"),
            },
            players: setup["names"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(slot, name)| PlayerConfig {
                    name: name.as_str().unwrap().into(),
                    team: setup["teams"][slot].as_i64().unwrap() as i32,
                    faction: match setup["factions"][slot].as_u64().unwrap() {
                        0 => Faction::Usa,
                        1 => Faction::Germany,
                        faction => panic!("unrepresented faction {faction}"),
                    },
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
    assert!(state.world.game.mode.is_none());
    for actor in setup["spawns"].as_array().unwrap() {
        let owner = actor["owner"].as_u64().unwrap() as usize;
        let kind = state.rules.kind(actor["kind"].as_str().unwrap()).unwrap();
        let id = actor["id"].as_u64().unwrap() as u32;
        assert_eq!(id, state.next_unit_id);
        assert!(state.combat.get(id).is_none() && state.firing.get(id).is_none());
        assert!(state.abilities.get(id).is_none() && state.recovery.get(id).is_none());
        let mut draft = StagedUnit::ordinary(
            &state.world.game,
            &state.rules,
            owner,
            kind,
            id,
            state.world.game.units.len().try_into().unwrap(),
            state.start.players[owner].spawn,
            Some(Point {
                x: actor["x"].as_f64().unwrap(),
                z: actor["z"].as_f64().unwrap(),
            }),
        )
        .unwrap();
        draft.unit.rot = actor["rot"].as_f64().unwrap();
        draft.control.aim = actor["aim"].as_f64().unwrap();
        let publication = prepare_publication(
            PublicationParts {
                units: &mut state.world.game.units,
                grid: &mut state.grid,
                controls: &mut state.commands.units,
                recruitment: &mut state.recruit_metadata,
                next_id: &mut state.next_unit_id,
            },
            &state.rules,
            state.start.players.len(),
            vec![draft],
        )
        .unwrap();
        assert_eq!(
            publication.publish_with(|unit, _| {
                state.combat.initialize(unit.id);
                state.firing.initialize(unit.id);
                state.abilities.initialize(unit.id);
                state.recovery.initialize(unit.id);
            }),
            [id]
        );
    }
    for unit in state.world.game.units.values_mut() {
        unit.hold_pos = true;
    }
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    for (slot, wallet) in state.start.economy.players.iter_mut().enumerate() {
        wallet.mp = setup["mp"][slot].as_f64().unwrap();
    }
    ww2_core::vision::update_vision(&mut state.world.game, &state.rules, &state.grid);
    state
        .initialize_tick_scene(0, TickPathConfig::default())
        .unwrap();
    state
}
fn focused(fields: &Value, ids: &BTreeSet<u32>) -> Value {
    let mut selected = serde_json::Map::new();
    selected.insert("works".into(), fields["works"].clone());
    for key in ["units", "plans", "orders"] {
        selected.insert(
            key.into(),
            Value::Array(
                fields[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|row| ids.contains(&(row[0].as_u64().unwrap() as u32)))
                    .cloned()
                    .collect(),
            ),
        );
    }
    tagged(Value::Object(selected))
}
fn expected_focus(fields: &Value, ids: &BTreeSet<u32>) -> Value {
    let mut selected = fields.as_object().unwrap().clone();
    for key in ["units", "plans", "orders"] {
        selected.insert(
            key.into(),
            Value::Array(
                fields[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|row| ids.contains(&(number(&row[0]["f64"]) as u32)))
                    .cloned()
                    .collect(),
            ),
        );
    }
    Value::Object(selected)
}
fn compare(
    state: &mut Match,
    delivery: &mut DeliveryState,
    source: &Value,
    ids: &BTreeSet<u32>,
    context: &str,
) {
    assert_eq!(
        state.world.game.tick,
        source["tick"].as_i64().unwrap(),
        "{context}: clock"
    );
    for unit in source["units"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|u| ids.contains(&(u["id"].as_u64().unwrap() as u32)))
    {
        let id = unit["id"].as_u64().unwrap() as u32;
        match (state.field_works.jobs().get(&id), unit["paid"].is_null()) {
            (None, true) => {}
            (Some(job), false) => {
                let cells = job
                    .cells
                    .iter()
                    .map(|cell| json!([cell.cell, char::from(cell.ch).to_string()]))
                    .collect::<Vec<_>>();
                assert_eq!(
                    json!(cells),
                    unit["paid"]["cells"],
                    "{context}: paid cells {id}"
                );
                assert_eq!(
                    job.project,
                    unit["paid"]["project"].as_u64(),
                    "{context}: paid binding {id}"
                );
            }
            _ => panic!("{context}: paid job presence {id}"),
        }
    }
    for (slot, wallet) in state.start.economy.players.iter().enumerate() {
        assert_eq!(
            wallet.mp.to_bits(),
            number(&source["players"][slot]["mp"]).to_bits(),
            "{context}: wallet {slot}"
        );
    }
    // The original snapshotFor collects terrain and fog for every seat.
    // Retain those reads even though the focused comparison omits their payloads.
    let frame = delivery
        .capture_with_lists(state, &[false; 3], &[false; 3], &[])
        .unwrap_or_else(|e| panic!("{context}: {e:?}"));
    for slot in 0..3 {
        let actual = frame.recipient(slot).unwrap().into_json();
        assert_eq!(
            focused(&actual, ids),
            expected_focus(&source["recipients"][slot], ids),
            "{context}: recipient {slot}"
        );
    }
}

#[test]
fn actual_engineering_commands_ticks_and_recipient_reads() {
    let mut text = String::new();
    GzDecoder::new(&include_bytes!("fixtures/recipient-engineering.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    let fixture: Value = serde_json::from_str(&text).unwrap();
    let mut states = 0;
    let mut ticks = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let ids = case["setup"]["spawns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|actor| actor["id"].as_u64().unwrap() as u32)
            .collect();
        let mut state = make(case);
        let random = state.start.random.snapshot();
        let mut delivery = DeliveryState::new(&state).unwrap();
        let mut cursor = 0;
        compare(
            &mut state,
            &mut delivery,
            &case["states"][cursor],
            &ids,
            name,
        );
        states += 1;
        for record in case["records"].as_array().unwrap() {
            let action = &record["action"];
            if action["op"] == "command" {
                let denial = match Command::from_json(&action["command"]) {
                    Ok(command) => state
                        .execute_command(action["slot"].as_u64().unwrap() as usize, &command, false)
                        .unwrap(),
                    Err(denial) => Some(denial),
                };
                assert_eq!(
                    denial.map(|d| d.name()),
                    record["result"].as_str(),
                    "{name}: admission"
                );
                cursor += 1;
                compare(
                    &mut state,
                    &mut delivery,
                    &case["states"][cursor],
                    &ids,
                    name,
                );
                states += 1;
            } else {
                assert_eq!(action["op"], "steps");
                for _ in 0..action["count"].as_u64().unwrap() {
                    let report = state
                        .advance_tick()
                        .unwrap_or_else(|e| panic!("{name}: {e}"));
                    assert!(report.commands.is_empty());
                    assert!(
                        report.environment.is_empty(),
                        "{name}: unexpected outside effects"
                    );
                    assert!(report.phases.contains(&TickPhase::FieldProjects));
                    assert!(report.phases.contains(&TickPhase::FieldWork));
                    assert_eq!(
                        state.start.random.snapshot(),
                        random,
                        "{name}: quiet streams"
                    );
                    cursor += 1;
                    compare(
                        &mut state,
                        &mut delivery,
                        &case["states"][cursor],
                        &ids,
                        name,
                    );
                    states += 1;
                    ticks += 1;
                }
            }
        }
    }
    assert_eq!((states, ticks), (513, 481));
}
