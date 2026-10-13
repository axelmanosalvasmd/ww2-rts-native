//! Pure selected projection from the shipped legacy maps and independent Source inputs.
//! This is not a replay of ordinary movement, terrain destruction or RNG.
use flate2::read::GzDecoder;
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    legacy_scenario::{LegacyAction, PointOwner},
    map::AuthoritativeMap,
    match_commands::UnitControl,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat, ShotRef},
    unit_spawn::initial_unit,
    weather::WeatherChoice,
};

fn fixture() -> Value {
    let mut text = String::new();
    GzDecoder::new(include_bytes!("fixtures/legacy-scenario-runtime.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        value["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    value
}
fn make(trace: &Value) -> Match {
    let map = AuthoritativeMap::from_json(trace["map"].to_string().as_bytes()).unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|team| PlayerConfig {
                    name: format!("P{team}"),
                    team,
                    faction: Faction::try_from(team as u8).unwrap(),
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
    .unwrap()
}
fn reload(state: &mut Match, frame: &Value) {
    let input = &frame["recipientInputs"];
    assert!(input["mode"].is_null());
    assert_eq!(input["weather"], "clear");
    assert!(input["smokes"].as_array().unwrap().is_empty());
    assert!(input["strikes"].as_array().unwrap().is_empty());
    let random = state.start.random.snapshot();
    state.world.game.tick = frame["tick"].as_i64().unwrap();
    state.world.game.reveal = input["reveal"].as_bool().unwrap();
    for row in input["players"].as_array().unwrap() {
        let slot = row["slot"].as_u64().unwrap() as usize;
        assert_eq!(
            state.world.game.players[slot].team,
            row["team"].as_i64().unwrap() as i32
        );
        state.start.economy.players[slot].out = row["out"].as_bool().unwrap();
        state.world.game.players[slot].visible = row["visible"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap() as u32)
            .collect();
    }
    state.world.game.units.clear();
    let catalog = Catalog::compiled().unwrap();
    for row in input["units"].as_array().unwrap() {
        assert!(row["air"].is_null());
        assert_eq!(row["garrison"], -1);
        assert_eq!(row["riding"], false);
        let id = row["id"].as_u64().unwrap() as u32;
        let kind = state.rules.kind(row["type"].as_str().unwrap()).unwrap();
        let mut unit = initial_unit(
            &state.rules,
            &catalog,
            kind,
            id,
            row["owner"].as_u64().unwrap() as usize,
            Point {
                x: row["x"].as_f64().unwrap(),
                z: row["z"].as_f64().unwrap(),
            },
        )
        .unwrap();
        unit.hp = row["hp"].as_f64().unwrap();
        unit.built = row["built"].as_f64();
        state
            .commands
            .insert(id, UnitControl::new(&unit, &state.rules).unwrap());
        state.firing.initialize(id);
        state.combat.initialize(id);
        state.abilities.initialize(id);
        state.recovery.initialize(id);
        state.world.game.units.insert(id, unit);
    }
    state.grid.rebuild(&state.world.game.units);
    let terrain = &input["terrain"];
    assert_eq!(terrain["w"].as_u64().unwrap() as usize, state.world.game.w);
    assert_eq!(terrain["h"].as_u64().unwrap() as usize, state.world.game.h);
    if let Some(heights) = terrain["height"].as_array() {
        assert_eq!(heights.len(), state.world.game.chars.len());
        for (cell, level) in heights.iter().enumerate() {
            let level = i8::try_from(level.as_i64().unwrap()).unwrap();
            if state.world.game.height.as_ref().map_or(0, |h| h[cell]) != level {
                state.world.set_level(cell, level).unwrap();
            }
        }
    } else {
        assert!(state.world.game.height.is_none());
    }
    state.world.game.flags = terrain["flags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|flag| u16::try_from(flag.as_u64().unwrap()).unwrap())
        .collect();
    assert_eq!(state.world.game.flags.len(), state.world.game.chars.len());
    assert_eq!(state.start.random.snapshot(), random);
}

#[test]
fn legacy_public_rows_and_empty_objective_hud_match_original_selected_source() {
    let fixture = fixture();
    let mut frames = 0;
    let mut recipients = 0;
    let mut say_rows = 0;
    let mut falling = 0;
    for trace in fixture["traces"].as_array().unwrap() {
        let mut state = make(trace);
        let initial = std::ptr::from_ref(state.world.initial());
        for frame in trace["steps"].as_array().unwrap() {
            reload(&mut state, frame);
            assert_eq!(std::ptr::from_ref(state.world.initial()), initial);
            let points: Vec<_> = state
                .start
                .points
                .iter()
                .map(|p| PointOwner {
                    id: p.id.as_str().unwrap(),
                    owner: p.owner.map_or(-1, |slot| slot as i32),
                })
                .collect();
            let actions = state
                .scenario
                .as_mut()
                .unwrap()
                .step(frame["tick"].as_u64().unwrap(), &points)
                .unwrap();
            let say: Vec<_> = actions
                .iter()
                .filter_map(|action| match action {
                    LegacyAction::Say(say) => Some(say),
                    LegacyAction::Destroy(_) => None,
                })
                .collect();
            // Compare real typed legacy messages to raw Source emissions first.
            let source_say: Vec<_> = frame["shots"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row["k"] == "say")
                .cloned()
                .collect();
            assert_eq!(
                json!(say.iter().map(|row| row.shot_json()).collect::<Vec<_>>()),
                json!(source_say)
            );
            // Unsettled body facts remain Source-only. The complete native
            // collector deliberately has no body-import test seam.
            if frame["recipientInputs"]["fallingCount"].as_u64().unwrap() != 0 {
                falling += 1;
                continue;
            }
            assert_eq!(frame["shots"].as_array().unwrap().len(), say.len());
            let shots: Vec<_> = say.into_iter().map(ShotRef::Scenario).collect();
            let selected = [
                SelectedSeat {
                    slot: 0,
                    full: false,
                    omit: false,
                },
                SelectedSeat {
                    slot: 1,
                    full: false,
                    omit: false,
                },
            ];
            let random = state.start.random.snapshot();
            let mut delivery = DeliveryState::new(&state).unwrap();
            let captured = delivery
                .capture_selected_with_shots(&mut state, &selected, &shots)
                .unwrap()
                .unwrap();
            for expected in frame["recipients"].as_array().unwrap() {
                let slot = expected["slot"].as_u64().unwrap() as usize;
                let actual = captured.recipient(slot).unwrap().into_json();
                assert_eq!(
                    actual["shots"], expected["shots"],
                    "{}:{} seat{slot}",
                    trace["name"], frame["tick"]
                );
                assert_eq!(actual["scenario"], expected["scenario"]);
                recipients += 1;
                say_rows += actual["shots"].as_array().unwrap().len();
            }
            frames += 1;
            drop(captured);
            assert_eq!(state.start.random.snapshot(), random);
        }
    }
    assert_eq!((frames, recipients, say_rows, falling), (19, 38, 12, 2));
}
