use super::*;

#[test]
fn authored_objectives_refuse_unimplemented_missions_before_startup() {
    let mut data = json!({"version":1,"objectives":[],"groups":[],"triggers":[]});
    assert!(validate_definition(&data, 2, Mode::Conquest).is_ok());
    for mode in [
        Mode::World,
        Mode::Classic,
        Mode::Annihilation,
        Mode::Horde,
        Mode::Tutorial,
    ] {
        assert!(validate_definition(&data, 2, mode).is_err());
    }
    data["groups"] = json!([{"id":"g","units":[]}]);
    assert!(validate_definition(&data, 2, Mode::Conquest).is_err());
    data["groups"] = json!([]);
    data["triggers"] = json!([{"id":"t","scope":"match","condition":{"kind":"time","seconds":0},"actions":[{"kind":"reinforce"}]}]);
    assert!(validate_definition(&data, 2, Mode::Conquest).is_err());
    data["triggers"][0]["actions"] = json!([{"kind":"say","text":"OK","side":2}]);
    assert!(validate_definition(&data, 2, Mode::Conquest).is_err());
    data["triggers"][0]["actions"] = json!([{"kind":"say","text":{"es":"First","fr":"Second"}}]);
    assert!(validate_definition(&data, 2, Mode::Conquest).is_err());
    data["triggers"][0]["actions"][0]["text"] = json!({"en":"Default","es":"Translation"});
    assert!(validate_definition(&data, 2, Mode::Conquest).is_ok());
}

#[test]
fn authored_checkpoint_preserves_canonical_trigger_and_objective_history() {
    use crate::{
        economy::Faction,
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_runtime::{MatchOptions, PlayerConfig},
        weather::WeatherChoice,
    };

    let data = json!({"version":1,"areas":[{"id":"entry","box":[20,20,24,24]}],
    "groups":[],"objectives":[{"id":"mission","text":"Complete the mission"}],
    "triggers":[
        {"id":"complete","scope":"match","condition":{"kind":"time","seconds":0.1},
         "repeat":{"mode":"once","cooldown":3},
         "actions":[{"kind":"objectiveComplete","objective":"mission"}]},
        {"id":"announce","scope":"match","condition":{"kind":"time","seconds":0.25},
         "repeat":{"mode":"once","cooldown":7},
         "actions":[{"kind":"say","text":"Done"}]}
    ]});
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Canonical authored checkpoint", "w":64, "h":40,
            "rows":vec![".".repeat(64);40],
            "spawns":[{"x":8,"y":10},{"x":40,"y":32}],
            "points":[{"id":"bridgehead","x":4,"y":36,"vp":0,"mp":0}],
            "scenario":data
        }))
        .unwrap(),
    )
    .unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 383386,
            mode: Mode::Conquest,
            players: [Faction::Usa, Faction::Germany]
                .into_iter()
                .enumerate()
                .map(|(team, faction)| PlayerConfig {
                    name: format!("seat{team}"),
                    team: team as i32,
                    faction,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Endless,
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
    state.advance_tick().unwrap();
    let before = state.authored_scenario.clone();
    let scenario = before.as_ref().unwrap();
    assert_eq!(
        scenario.executions["complete@match"],
        TriggerState::default()
    );
    assert_eq!(
        scenario.executions["announce@match"],
        TriggerState::default()
    );
    assert_eq!(scenario.facts.trigger_counts.len(), 2);
    assert_eq!(
        scenario.facts.trigger_counts[&("complete".into(), Scope::Match)],
        0
    );
    assert_eq!(scenario.facts.areas["entry"], [20, 20, 24, 24]);
    assert_eq!(scenario.owners["bridgehead"], -1);
    assert!(scenario.deferred.is_empty());
    state = Match::from_save_json(state.save_json().unwrap()).unwrap();
    assert_eq!(state.authored_scenario, before);

    state.start.points[0].owner = Some(0);
    state.advance_tick().unwrap();
    let scenario = state.authored_scenario.as_ref().unwrap();
    let complete = &scenario.executions["complete@match"];
    assert_eq!(complete.count, 1);
    assert_eq!(complete.next, 3.1);
    assert!(complete.previous);
    assert_eq!(complete.execution.as_deref(), Some("scenario:1"));
    assert_eq!(scenario.objectives[0].scope, "match");
    assert_eq!(scenario.objectives[0].state, "complete");
    assert!(
        scenario
            .facts
            .completed_objectives
            .contains(&("mission".into(), Scope::Match))
    );
    assert!(scenario.facts.captured.contains(&("bridgehead".into(), 0)));
    assert_eq!(scenario.owners["bridgehead"], 0);
    assert_eq!(scenario.executions["announce@match"].count, 0);
    let before = state.authored_scenario.clone();
    state = Match::from_save_json(state.save_json().unwrap()).unwrap();
    assert_eq!(state.authored_scenario, before);

    for _ in 0..4 {
        state.advance_tick().unwrap();
    }
    state.ordinary_snapshot_raw_lists().unwrap();
    let scenario = state.authored_scenario.as_ref().unwrap();
    assert_eq!(scenario.executions["complete@match"].count, 1);
    assert_eq!(scenario.executions["announce@match"].count, 1);
    assert_eq!(scenario.executions["announce@match"].next, 7.25);
    assert_eq!(scenario.next_execution, 3);
    assert_eq!(state.tick_state.fault(), None);
    let before = state.authored_scenario.clone();
    state = Match::from_save_json(state.save_json().unwrap()).unwrap();
    assert_eq!(state.authored_scenario, before);
}
