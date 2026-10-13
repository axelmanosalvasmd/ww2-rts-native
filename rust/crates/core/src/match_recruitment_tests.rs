use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig, RecruitError},
    weather::WeatherChoice,
};
use serde_json::{Value, json};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn position(p: Option<Point>) -> Value {
    p.map_or(Value::Null, |p| json!([bits(p.x), bits(p.z)]))
}
fn make(map: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(map).unwrap()).unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "B".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(217),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
fn reason(e: RecruitError) -> Value {
    use crate::production::{Denial, Error};
    if let RecruitError::Admission(Error::Denied(d)) = e {
        json!(match d {
            Denial::Blocked => "blocked",
            Denial::Needs => "needs",
            Denial::Tier => "tier",
            Denial::Mp => "mp",
            Denial::Pop => "pop",
            Denial::Max => "max",
            _ => panic!("denial"),
        })
    } else {
        panic!("{e:?}")
    }
}
#[test]
fn recorded_buy_matches_original_commands_with_owned_routing() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../simtest/tests/fixtures/match-recruitment.json"
    ))
    .unwrap();
    let mut failed_paths = 0;
    let mut admitted = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let mut m = make(&case["map"]);
        m.start.economy.players[0].mp = 5000.125;
        m.world.game.reveal = true;
        for (step, stage) in case["stages"].as_array().unwrap().iter().enumerate() {
            m.world.game.tick += 1;
            let action = &stage["action"];
            let slot = action["slot"].as_u64().unwrap_or(0) as usize;
            match action["op"].as_str().unwrap() {
                "player" => {
                    let fields = &action["fields"];
                    if let Some(v) = fields["mp"].as_f64() {
                        m.start.economy.players[slot].mp = v;
                    }
                    if let Some(v) = fields["away"].as_bool() {
                        m.start.economy.players[slot].away = v;
                    }
                    if let Some(v) = fields["out"].as_bool() {
                        m.start.economy.players[slot].out = v;
                    }
                    if !fields["rally"].is_null() {
                        m.player_rallies[slot] = Some(point(&fields["rally"]));
                    }
                }
                "maker" => {
                    let fields = &action["fields"];
                    let index = fields["type"]
                        .as_str()
                        .map(|name| m.rules.kind(name).unwrap());
                    let u = m
                        .world
                        .game
                        .units
                        .get_mut(&(action["id"].as_u64().unwrap() as u32))
                        .unwrap();
                    if let Some(v) = fields["hp"].as_f64() {
                        u.hp = v;
                    }
                    if let Some(v) = fields["built"].as_f64() {
                        u.built = Some(v);
                    }
                    if let Some(v) = index {
                        u.kind = v;
                    }
                }
                "rally" => {
                    m.building_rallies.insert(
                        action["id"].as_u64().unwrap() as u32,
                        Some(point(&action["to"])),
                    );
                }
                "winner" => {
                    m.finish = action["value"].as_bool().unwrap().then_some(
                        crate::conquest_end::ConquestFinish {
                            winner: 0,
                            reason: crate::conquest_end::ConquestEndReason::VictoryPoints,
                            end_at: Point { x: 1.0, z: 1.0 },
                            end_tick: m.world.game.tick,
                        },
                    )
                }
                "buy" => {
                    let result = m.recorded_buy(
                        slot,
                        action["kind"].as_str().unwrap(),
                        action["from"].as_u64().map(|id| id as u32),
                    );
                    let (reason, unit) = match result {
                        Ok(done) => {
                            admitted += 1;
                            assert_eq!(
                                m.combat.get(done.unit),
                                Some(&ww2_core::match_combat::UnitCombat::default()),
                                "recruits must enter combat with canonical default metadata"
                            );
                            assert_eq!(
                                m.firing.get(done.unit),
                                Some(&ww2_core::match_firing::FiringUnit::default()),
                                "recruits must publish canonical target and timer defaults"
                            );
                            let u = &m.world.game.units[&done.unit];
                            let control = &m.commands.units[&done.unit];
                            let failed = done.route_status == Some(PathStatus::Unreachable);
                            failed_paths += usize::from(failed);
                            (
                                Value::Null,
                                json!({"id":u.id,"owner":u.owner,"kind":m.rules.kinds[u.kind].name,"pose":[bits(u.x),bits(u.z),bits(u.rot),bits(control.aim),bits(u.ghost)],"hp":bits(u.hp),"path":u.path.iter().map(|p|position(Some(*p))).collect::<Vec<_>>(),"goal":position(u.world_goal),"routeEnd":position(control.route_end),"drift":u.drift,"auto":control.auto_cast,"autoRetreat":control.auto_retreat,"grid":m.grid.in_order(&m.world.game.units,(u.x,u.z),20.0,|_,_|true),"producer":done.purchase.request.building,"routeFailed":failed}),
                            )
                        }
                        Err(e) => (reason(e), Value::Null),
                    };
                    assert_eq!(
                        json!(m.story.iter().map(|s| bits(s.mp_spent)).collect::<Vec<_>>()),
                        stage["source_spending"]
                    );
                    assert!(m.story.iter().all(|s| s.built == 0.0));
                    assert_eq!(
                        json!({"result":reason,"next":m.next_unit_id,"money":m.start.economy.players.iter().map(|p|bits(p.mp)).collect::<Vec<_>>(),"unit":unit}),
                        stage["expected"],
                        "{} step {step}",
                        case["name"]
                    );
                }
                _ => panic!("action"),
            }
        }
    }
    assert!(admitted > 0 && failed_paths > 0);
}
fn unchanged(m: &Match) -> String {
    format!(
        "{:?}{:?}{:?}{:?}{:?}{:?}{:?}{}",
        m.start.economy,
        m.world.game.units,
        m.commands.units,
        m.recruit_metadata,
        m.combat,
        m.firing,
        m.story,
        m.next_unit_id
    )
}
#[test]
fn recorded_buy_preflight_never_charges_or_publishes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../simtest/tests/fixtures/match-recruitment.json"
    ))
    .unwrap();
    let mut m = make(&fixture["cases"][0]["map"]);
    m.start.economy.players[0].mp = 5000.0;
    // Fresh IDs must be unused in both canonical combat and firing metadata.
    m.combat.initialize(m.next_unit_id);
    let before = unchanged(&m);
    assert!(matches!(
        m.recorded_buy(0, "rifle", None),
        Err(RecruitError::InvalidState(_))
    ));
    assert_eq!(unchanged(&m), before);
    m.combat.remove(m.next_unit_id);
    m.firing.initialize(m.next_unit_id);
    let before = unchanged(&m);
    assert!(matches!(
        m.recorded_buy(0, "rifle", None),
        Err(RecruitError::InvalidState(_))
    ));
    assert_eq!(unchanged(&m), before);
    m.firing.units.remove(&m.next_unit_id);
    m.story[0].mp_spent = f64::NAN;
    let before = unchanged(&m);
    assert!(matches!(
        m.recorded_buy(0, "rifle", None),
        Err(RecruitError::Story(_))
    ));
    assert_eq!(unchanged(&m), before);
    // Blocked seat admission precedes malformed story and recipient state.
    m.player_rallies[0] = Some(Point { x: 35.0, z: 35.0 });
    m.recipients.clear();
    m.start.economy.players[0].away = true;
    let before = unchanged(&m);
    assert!(matches!(
        m.recorded_buy(0, "rifle", None),
        Err(RecruitError::Admission(crate::production::Error::Denied(
            crate::production::Denial::Blocked
        )))
    ));
    assert_eq!(unchanged(&m), before);
}
