use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{LOCAL_DEFENDER_OWNER, Point},
    map::AuthoritativeMap,
    match_end::{EndReason, MatchSample},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_story::DeathRecord,
    match_timeline::{self, TimelinePlayer, TimelineUnit},
    rules::Rules,
    weather::WeatherChoice,
};
fn fixture() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/mode-history-components.json"
    );
    let bytes = std::fs::read(path)
        .expect("Generate the authentic mode-history component fixture after a Source slot grant");
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        value["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    value
}
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn encode(value: Value) -> Value {
    match value {
        Value::Number(n) => bits(n.as_f64().unwrap()),
        Value::Array(values) => Value::Array(values.into_iter().map(encode).collect()),
        Value::Object(object) => {
            Value::Object(object.into_iter().map(|(k, v)| (k, encode(v))).collect())
        }
        other => other,
    }
}
fn point(value: &Value) -> Point {
    Point {
        x: value["x"].as_f64().unwrap(),
        z: value["z"].as_f64().unwrap(),
    }
}
fn sample(input: &Value, rules: &Rules) -> MatchSample {
    let teams = input["teams"].as_array().unwrap();
    let players = teams
        .iter()
        .enumerate()
        .map(|(index, team)| TimelinePlayer {
            team: team.as_i64().unwrap() as i32,
            vp: input["scores"][index].as_f64().unwrap(),
        })
        .collect::<Vec<_>>();
    let points = input["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|slot| usize::try_from(slot.as_i64().unwrap()).ok())
        .collect::<Vec<_>>();
    let units = input["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| TimelineUnit {
            owner: usize::try_from(u["owner"].as_i64().unwrap()).unwrap_or(LOCAL_DEFENDER_OWNER),
            kind: rules.kind(u["type"].as_str().unwrap()).unwrap(),
            hp: u["hp"].as_f64().unwrap(),
        });
    match_timeline::sample(
        Mode::from_kind(input["mode"].as_str()),
        input["tick"].as_i64().unwrap(),
        &players,
        &points,
        units,
        rules,
    )
    .unwrap()
}
fn sample_json(sample: &MatchSample) -> Value {
    match sample {
        MatchSample::Conquest { t, vp } => json!({"t":t,"vp":vp}),
        MatchSample::Territories {
            t,
            points,
            structures,
        } => json!({"t":t,"points":points,"structures":structures}),
    }
}
#[test]
fn mode93_original_sampler_covers_every_structure_policy() {
    let f = fixture();
    let rules = Rules::compiled().unwrap();
    let cases = f["samples"].as_array().unwrap();
    assert_eq!(cases.len(), 21);
    for case in cases {
        assert_eq!(
            encode(json!([sample_json(&sample(&case["input"], &rules))])),
            case["expected"],
            "{}",
            case["name"]
        );
    }
}
fn callback_owner(input: &Value) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"History callbacks", "w":40,"h":40,"rows":vec![".".repeat(40);40],
            "spawns":[{"x":3,"y":3},{"x":36,"y":36},{"x":3,"y":36}],
            "points":[{"x":18,"y":18}]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 1851,
            mode: Mode::Conquest,
            players: input["teams"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(slot, team)| PlayerConfig {
                    name: format!("P{slot}"),
                    team: team.as_i64().unwrap() as i32,
                    faction: Faction::Usa,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: None,
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
#[test]
fn mode92_retained_death_spots_feed_original_finish_metadata() {
    let f = fixture();
    let rules = Rules::compiled().unwrap();
    let cases = f["finishes"].as_array().unwrap();
    assert_eq!(cases.len(), 12);
    for case in cases {
        let input = &case["input"];
        // Callback events are authored inputs shared with Source. No expected
        // fallen position, finish record or constructor state initializes Match.
        let mut m = callback_owner(input);
        for death in input["deaths"].as_array().unwrap() {
            m.record_death(
                DeathRecord {
                    owner: death["owner"].as_u64().map(|s| s as usize),
                    kind: death["kind"].as_str().unwrap(),
                    at: point(&death["at"]),
                },
                death["by"].as_u64().map(|s| s as usize),
            )
            .unwrap();
        }
        let finish = m
            .fallen
            .finish(
                Mode::from_kind(input["mode"].as_str()),
                input["winner"].as_i64().unwrap() as i32,
                (
                    input["width"].as_u64().unwrap() as usize,
                    input["height"].as_u64().unwrap() as usize,
                ),
                input["tick"].as_i64().unwrap(),
            )
            .unwrap();
        let mut fallen = json!({});
        for (key, at) in [
            ("structure", m.fallen.structure),
            ("bunker", m.fallen.bunker),
            ("hq", m.fallen.hq),
        ] {
            if let Some(at) = at {
                fallen[key] = json!({"x":at.x,"z":at.z});
            }
        }
        let result = json!({"winner":finish.winner,"reason":finish.reason.as_str(),
            "at":{"x":finish.end_at.x,"z":finish.end_at.z},"endTick":finish.end_tick,
            "fallen":fallen,"timeline":[sample_json(&sample(input,&rules))]});
        assert_eq!(encode(result), case["expected"], "{}", case["name"]);
    }
}

#[test]
fn mode69_complete_initial_force_comes_from_actual_tutorial_constructor() {
    use ww2_core::tutorial_steps::TutorialStep;
    let f = fixture();
    let cases = f["tutorialStarts"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&input["map"]).unwrap()).unwrap();
        let factions = input["factions"].as_array().unwrap();
        let m = Match::new_tutorial(
            &map,
            MatchOptions {
                seed: 1851,
                mode: Mode::Tutorial,
                players: factions
                    .iter()
                    .enumerate()
                    .map(|(slot, f)| PlayerConfig {
                        name: format!("Human{slot}"),
                        team: 0,
                        faction: Faction::try_from(f.as_u64().unwrap() as u8).unwrap(),
                    })
                    .collect(),
                shuffle: false,
                army: ArmySize::Standard,
                weather: WeatherChoice::Map,
                weather_seed: None,
                living_weather: false,
                map_key: None,
                tech: false,
                supply: false,
                logistics: false,
                sandbox: false,
            },
        )
        .unwrap();
        let runtime = m.tutorial_state().unwrap();
        assert_eq!(m.world.game.units.len(), factions.len() * 3);
        assert!(
            m.world
                .game
                .units
                .values()
                .all(|u| u.owner < factions.len())
        );
        let players = m
            .start
            .players
            .iter()
            .zip(&m.start.economy.players)
            .map(|(p, w)| {
                json!({
                    "name":p.config.name,"team":p.config.team,"faction":p.config.faction as u8,
                    "spawn":{"x":p.spawn.x,"z":p.spawn.z},"mp":w.mp,"support":p.support,
                })
            })
            .collect::<Vec<_>>();
        let units = m
            .world
            .game
            .units
            .values()
            .map(|u| {
                let control = &m.commands.units[&u.id];
                json!({"id":u.id,"type":m.rules.kinds[u.kind].name,"owner":u.owner,
                    "x":u.x,"z":u.z,"hp":u.hp,"rot":u.rot,"autoRetreat":control.auto_retreat,
                    "retreating":u.retreating,"holdPos":u.hold_pos,
                    "path":u.path.iter().map(|p|json!({"x":p.x,"z":p.z})).collect::<Vec<_>>(),
                    "amove":u.amove.map(|p|json!({"x":p.x,"z":p.z})),
                })
            })
            .collect::<Vec<_>>();
        let points = m
            .start
            .points
            .iter()
            .map(|p| {
                json!({"x":p.at.x,"z":p.at.z,"vp":p.vp,"mp":p.mp,
            "owner":p.owner.map_or(-1,|owner|owner as i64)})
            })
            .collect::<Vec<_>>();
        let actual = json!({"players":players,"units":units,"points":points,"mode":{
            "enemy":runtime.enemy(),"step":TutorialStep::ALL.iter().position(|step|*step==runtime.step()).unwrap(),
            "steps":TutorialStep::ALL.len(),"goal":runtime.goal(),"spawned":runtime.spawned(),
        }});
        assert_eq!(
            encode(actual),
            case["expected"],
            "actual Tutorial constructor case{index}"
        );
        assert!(m.validate_tick_state().is_ok());
    }
}

fn raw_story(rows: &Value) -> Vec<ww2_core::match_story::PlayerStory> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let n = |key: &str| row[key].as_f64().unwrap_or(0.0);
            ww2_core::match_story::PlayerStory {
                kills: n("kills"),
                losses: n("losses"),
                built: n("built"),
                captures: n("captures"),
                mp_spent: n("mpSpent"),
                support_calls: n("supportCalls"),
                planes_downed: n("planesDowned"),
            }
        })
        .collect()
}
fn strings(values: &Value) -> Vec<String> {
    values
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}
fn horde_run_json(run: &ww2_core::match_result::HordeRun) -> Value {
    run.fields()
}
fn horde_result_json(result: &ww2_core::match_result::HordeResult) -> Value {
    result.fields()
}
fn outcome_json(outcome: Option<ww2_core::match_result::LastMatch>) -> Value {
    outcome.map(|o| o.fields()).unwrap_or(Value::Null)
}
fn result_json(report: &ww2_core::match_result::CompletedReport) -> Value {
    report.fields()
}
#[test]
fn mode90_result_composition_matches_original_server_expressions() {
    use ww2_core::{
        match_result::{
            CompletedReport, HordeRecordKey, HordeRecords, HordeRun, PersonHistory, RoomReport,
        },
        match_story::MatchResult,
    };
    let f = fixture();
    let cases = f["reports"].as_array().unwrap();
    assert_eq!(cases.len(), 24);
    for case in cases {
        let i = &case["input"];
        let names = strings(&i["roomNames"]);
        let raw = raw_story(&i["story"]);
        let mode = match i["mode"].as_str() {
            None => "conquest",
            Some("classic") => "classic",
            Some("world") => "world",
            Some("annihilation") => "annihilation",
            Some("assault") => "assault",
            Some("horde") => "horde",
            Some("tutorial") => "tutorial",
            _ => unreachable!(),
        };
        let reason = match i["reason"].as_str().unwrap() {
            "draw" => EndReason::Draw,
            "vp" => EndReason::VictoryPoints,
            "tutorial" => EndReason::Tutorial,
            "structures" => EndReason::Structures,
            _ => unreachable!(),
        };
        let sandbox = i["sandbox"].as_bool().unwrap();
        let mut records = HordeRecords::default();
        let horde = if mode == "horde" && !sandbox {
            Some(
                records
                    .finish(
                        &HordeRecordKey {
                            map: "Crossroads".into(),
                            humans: names.len(),
                            army: ArmySize::Standard,
                        },
                        HordeRun::from_counters(
                            i["wave"].as_u64().unwrap(),
                            i["tick"].as_i64().unwrap(),
                            &raw,
                            &names,
                        )
                        .unwrap(),
                    )
                    .unwrap(),
            )
        } else {
            None
        };
        let result = MatchResult {
            winner: i["winner"].as_i64().unwrap() as i32,
            teams: i["teams"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap() as i32)
                .collect(),
            names: strings(&i["canonicalNames"]),
            reason,
            at: point(&i["at"]),
            end_tick: i["endTick"].as_i64().unwrap(),
            story: raw.into_iter().map(|row| row.rounded()).collect(),
            timeline: i["timeline"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| {
                    let numbers = |key: &str| {
                        s[key]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_f64().unwrap())
                            .collect()
                    };
                    if mode == "conquest" {
                        MatchSample::Conquest {
                            t: s["t"].as_f64().unwrap(),
                            vp: numbers("vp"),
                        }
                    } else {
                        MatchSample::Territories {
                            t: s["t"].as_f64().unwrap(),
                            points: numbers("points"),
                            structures: numbers("structures"),
                        }
                    }
                })
                .collect(),
            win_vp: if mode == "conquest" {
                i["winVp"].as_f64()
            } else {
                None
            },
            mode,
        };
        let completed = CompletedReport::compose(result, &names, sandbox, horde).unwrap();
        let report = result_json(&completed);
        let mut people = vec![PersonHistory::default(); names.len()];
        completed.remember(&mut people).unwrap();
        let room = RoomReport::Completed(Box::new(completed));
        let last = people
            .iter()
            .map(|p| outcome_json(p.last_match()))
            .collect::<Vec<_>>();
        let seated = people
            .iter()
            .map(|p| room.recipient_fields(p, true))
            .collect::<Vec<_>>();
        let actual = json!({"report":report,"lastMatch":last,"seated":seated,
            "moved":room.recipient_fields(&people[1],true),
            "observer":room.recipient_fields(&people[1],false),
            "newcomer":room.recipient_fields(&PersonHistory::default(),true),
            "hostEnded":RoomReport::HostEnded.recipient_fields(&people[1],true)});
        assert_eq!(
            encode(actual),
            case["expected"],
            "original result composition {mode} winner{}",
            i["winner"]
        );
    }
    let runs = f["hordeRuns"].as_array().unwrap();
    assert_eq!(runs.len(), 7);
    let mut records = HordeRecords::default();
    for case in runs {
        let i = &case["input"];
        let names = strings(&i["names"]);
        let story = i["kills"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| ww2_core::match_story::PlayerStory {
                kills: v.as_f64().unwrap(),
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let key = HordeRecordKey {
            map: i["map"].as_str().unwrap().to_owned(),
            humans: names.len(),
            army: match i["army"].as_str().unwrap() {
                "standard" => ArmySize::Standard,
                "large" => ArmySize::Large,
                _ => unreachable!(),
            },
        };
        let run = HordeRun::from_counters(
            i["wave"].as_u64().unwrap(),
            i["tick"].as_i64().unwrap(),
            &story,
            &names,
        )
        .unwrap();
        let result = records.finish(&key, run).unwrap();
        assert_eq!(
            encode(json!({"result":horde_result_json(&result),
            "stored":records.best(&key).unwrap().map(horde_run_json)})),
            case["expected"]
        );
    }
}
