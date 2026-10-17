use super::*;

#[test]
fn authored_scenarios_validate_ordinary_sides_and_localization_before_startup() {
    let mut data = json!({"version":1,"objectives":[],"groups":[],"triggers":[]});
    assert!(validate_definition(&data, 2, Mode::Conquest).is_ok());
    assert!(validate_definition(&data, 2, Mode::Classic).is_ok());
    for mode in [Mode::World, Mode::Annihilation, Mode::Horde, Mode::Tutorial] {
        assert!(validate_definition(&data, 2, mode).is_err());
    }
    data["groups"] = json!([{"id":"g","units":[]}]);
    assert!(validate_definition(&data, 2, Mode::Classic).is_err());
    assert!(validate_definition(&data, 2, Mode::Conquest).is_ok());
    data["triggers"] = json!([{"id":"t","scope":"match","condition":{"kind":"time","seconds":0},"actions":[{"kind":"reinforce","group":"g","side":0,"at":[4,4],"expires":10,"roster":[{"type":"rifle","count":1}],"order":{"kind":"hold"}}]}]);
    assert!(validate_definition(&data, 2, Mode::Conquest).is_ok());
    data["triggers"][0]["actions"] = json!([{"kind":"say","text":"OK","side":2}]);
    assert!(validate_definition(&data, 2, Mode::Conquest).is_err());
    data["triggers"][0]["actions"] = json!([{"kind":"say","text":{"es":"First","fr":"Second"}}]);
    assert!(validate_definition(&data, 2, Mode::Conquest).is_err());
    data["triggers"][0]["actions"][0]["text"] = json!({"en":"Default","es":"Translation"});
    assert!(validate_definition(&data, 2, Mode::Conquest).is_ok());
    data["triggers"][0]["condition"] = json!({"kind":"ownership","target":"region:1","side":0});
    assert!(validate_definition(&data, 2, Mode::Conquest).is_err());
    data["triggers"][0]["condition"]["target"] = json!("ordinary-point");
    assert!(validate_definition(&data, 2, Mode::Conquest).is_ok());
    for text in [json!({"es":"Hold"}), json!(["Hold"])] {
        data["objectives"] = json!([{"id":"hold","text":text}]);
        assert!(validate_definition(&data, 2, Mode::Conquest).is_err());
    }
    data["objectives"] = json!([{"id":"hold","text":{"en":"Hold","es":"Aguanta"}}]);
    assert!(validate_definition(&data, 2, Mode::Conquest).is_ok());
}

#[test]
fn localization_side_key_is_text_and_not_an_authored_side_reference() {
    let data = json!({"version":1.0,"groups":[],"objectives":[],"triggers":[
        {"id":"intro","scope":"match","condition":{"kind":"time","seconds":0.0},
         "actions":[{"kind":"say","text":{"en":"Default","side":"A translation"}}]}]});
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

    let data = json!({"version":1,"areas":[],
    "groups":[],"objectives":[{"id":"mission","text":"Complete the mission"}],
    "triggers":[
        {"id":"complete","scope":"match","condition":{"kind":"time","seconds":0.1},
         "repeat":{"mode":"once"},
         "actions":[{"kind":"objectiveComplete","objective":"mission"}]},
        {"id":"announce","scope":"match","condition":{"kind":"time","seconds":0.25},
         "repeat":{"mode":"once"},
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
    assert!(scenario.facts.areas.is_empty());
    assert_eq!(scenario.last_tick, Some(1));
    assert_eq!(scenario.owners["bridgehead"], -1);
    assert!(scenario.deferred.is_empty());
    state = Match::from_save_json(state.save_json().unwrap()).unwrap();
    assert_eq!(state.authored_scenario, before);

    state.start.points[0].owner = Some(0);
    state.advance_tick().unwrap();
    let scenario = state.authored_scenario.as_ref().unwrap();
    let complete = &scenario.executions["complete@match"];
    assert_eq!(complete.count, 1);
    assert_eq!(complete.next, 0.1);
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
    assert_eq!(scenario.executions["announce@match"].next, 0.25);
    assert_eq!(scenario.next_execution, 3);
    assert_eq!(state.tick_state.fault(), None);
    let before = state.authored_scenario.clone();
    state = Match::from_save_json(state.save_json().unwrap()).unwrap();
    assert_eq!(state.authored_scenario, before);
}

#[test]
fn authored_checkpoint_rejects_missing_clock_legacy_owner_and_tampered_history() {
    let data = json!({"version":1,"objectives":[{"id":"hold","text":"Hold"}],
        "triggers":[{"id":"intro","scope":"match","condition":{"kind":"time","seconds":0.05},
        "actions":[{"kind":"objectiveActivate","objective":"hold"},{"kind":"say","text":"Hold"}]}]});
    let mut state = crate::save_state::tests::ordinary();
    state.authored_scenario = Some(AuthoredScenario::new(data.clone()));
    state.setup_authored_scenario().unwrap();
    let saved = state.save_json().unwrap();
    let mut missing_clock = saved.clone();
    let owner = missing_clock[1]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry[0] == json!(["string", "authored_scenario"]))
        .unwrap();
    owner[1][1][1]
        .as_array_mut()
        .unwrap()
        .retain(|entry| entry[0] != json!(["string", "last_tick"]));
    assert!(Match::from_save_json(missing_clock).is_err());

    let legacy: crate::authored_mission::AuthoredMission = serde_json::from_value(json!({
        "definition":data,"first_tick":null,"last_tick":null,"triggers":[],"objectives":[],"next_execution":1
    })).unwrap();
    state.authored_mission = Some(legacy.clone());
    assert!(state.save_json().is_err());
    let mut legacy_checkpoint = saved.clone();
    let legacy_value = crate::save_codec::to_value(&Some(legacy)).unwrap();
    let fields = legacy_checkpoint[1].as_array_mut().unwrap();
    if let Some(entry) = fields
        .iter_mut()
        .find(|entry| entry[0] == json!(["string", "authored_mission"]))
    {
        entry[1] = legacy_value;
    } else {
        fields.push(json!([["string", "authored_mission"], legacy_value]));
    }
    assert!(
        Match::from_save_json(legacy_checkpoint.clone())
            .err()
            .unwrap()
            .contains("legacy authored mission checkpoints are unsupported")
    );
    legacy_checkpoint[1]
        .as_array_mut()
        .unwrap()
        .retain(|entry| entry[0] != json!(["string", "authored_scenario"]));
    assert!(
        Match::from_save_json(legacy_checkpoint)
            .err()
            .unwrap()
            .contains("legacy authored mission checkpoints are unsupported")
    );
    state = Match::from_save_json(saved).unwrap();
    let report = state.advance_tick().unwrap();
    let say = report
        .events
        .iter()
        .find_map(|event| match event {
            crate::match_tick::TickEvent::AuthoredScenario(say) => Some(say.clone()),
            _ => None,
        })
        .unwrap();
    say.validate(&state).unwrap();
    let legacy_event = crate::match_tick::TickEvent::AuthoredMission(say.shot_json());
    assert!(crate::recipient_snapshot::ShotRef::try_from(&legacy_event).is_err());
    assert!(
        Match::events_from_save_json(Match::save_events_json(&[legacy_event]).unwrap()).is_err()
    );
    let mut unknown_say = serde_json::to_value(&say).unwrap();
    unknown_say["x"] = json!(0);
    assert!(serde_json::from_value::<ScenarioSay>(unknown_say).is_err());
    let mut changed_say = say;
    changed_say.recipients = "side".into();
    assert!(changed_say.validate(&state).is_err());
    let valid = state.save_json().unwrap();
    state.authored_scenario.as_mut().unwrap().objectives[0].state = "complete".into();
    assert!(state.save_json().is_err());
    state = Match::from_save_json(valid).unwrap();
    state.world.game.tick += 1;
    assert!(state.save_json().is_err());
}

#[test]
fn authored_checkpoint_requires_nullable_execution_and_arrival_identity_fields() {
    let data = json!({"version":1,"triggers":[{"id":"future","scope":"match",
        "condition":{"kind":"time","seconds":10},"actions":[{"kind":"say","text":"Later"}]}]});
    let mut state = crate::save_state::tests::ordinary();
    state.authored_scenario = Some(AuthoredScenario::new(data));
    state.setup_authored_scenario().unwrap();
    state.advance_tick().unwrap();
    let saved = state.save_json().unwrap();
    let restored = Match::from_save_json(saved.clone()).unwrap();
    let record = &restored.authored_scenario.as_ref().unwrap().executions["future@match"];
    assert_eq!(record.count, 0);
    assert_eq!(
        record.execution, None,
        "explicit null remains valid before execution"
    );
    let mut missing = saved;
    let owner = missing[1]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry[0] == json!(["string", "authored_scenario"]))
        .unwrap();
    let records = owner[1][1][1]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry[0] == json!(["string", "executions"]))
        .unwrap();
    let record = &mut records[1][1][0][1];
    record[1]
        .as_array_mut()
        .unwrap()
        .retain(|entry| entry[0] != json!(["string", "execution"]));
    assert!(
        Match::from_save_json(missing)
            .err()
            .unwrap()
            .contains("execution")
    );

    for identity in [None, Some("authored-unit".to_owned())] {
        let unit = ArrivalUnit {
            kind: "rifle".into(),
            authored: identity.clone(),
        };
        let saved = crate::save_codec::to_value(&unit).unwrap();
        let restored: ArrivalUnit = crate::save_codec::from_value(saved.clone()).unwrap();
        assert_eq!(restored.authored, identity);
        let mut missing = saved;
        missing[1]
            .as_array_mut()
            .unwrap()
            .retain(|entry| entry[0] != json!(["string", "authored"]));
        assert!(crate::save_codec::from_value::<ArrivalUnit>(missing).is_err());
    }
}
