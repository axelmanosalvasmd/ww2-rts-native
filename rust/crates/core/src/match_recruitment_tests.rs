use super::RecruitParts;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig, RecruitError},
    pathfinding::PathStatus,
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
                                Some(&crate::match_combat::UnitCombat::default()),
                                "recruits must enter combat with canonical default metadata"
                            );
                            assert_eq!(
                                m.firing.get(done.unit),
                                Some(&crate::match_firing::FiringUnit::default()),
                                "recruits must publish canonical target and timer defaults"
                            );
                            assert_eq!(
                                m.abilities.get(done.unit),
                                Some(&crate::match_abilities::AbilityUnit::default())
                            );
                            assert_eq!(
                                m.recovery.get(done.unit),
                                Some(&crate::match_recovery::RecoveryUnit::default())
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
        "{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{}",
        m.start.economy,
        m.world.game.units,
        m.commands.units,
        m.recruit_metadata,
        m.combat,
        m.firing,
        m.abilities,
        m.recovery,
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

#[test]
fn denied_rally_buy_preserves_moved_gameplay_bucket_order() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../simtest/tests/fixtures/match-recruitment.json"
    ))
    .unwrap();
    let mut m = make(&fixture["cases"][0]["map"]);
    let at = Point { x: 49.0, z: 33.0 };
    let kind = m.rules.kind("tank").unwrap();
    // Two canonical heavy drafts provide a valid current-position visibility
    // fixture. Only the low ID leaves and returns, like a maintained movement update.
    for id in [1, 2] {
        let unit = crate::unit_spawn::initial_unit(
            &m.rules,
            crate::catalog::Catalog::compiled().unwrap(),
            kind,
            id,
            0,
            at,
        )
        .unwrap();
        let control = crate::match_commands::UnitControl::new(&unit, &m.rules).unwrap();
        m.world.game.units.insert(id, unit);
        m.commands.insert(id, control);
    }
    m.grid = crate::grid::SpatialGrid::new(&m.world.game.units, 16.0);
    let low = m.world.game.units.get_mut(&1).unwrap();
    low.x = 65.0;
    m.grid.update(1, low);
    low.x = at.x;
    m.grid.update(1, low);
    let probe = |m: &Match| {
        (
            m.grid
                .unordered(&m.world.game.units, (at.x, at.z), 1.0, |_, _| true),
            m.grid
                .heavy(&m.world.game.units, (at.x, at.z), 1.0, |_, _| true),
        )
    };
    let before = probe(&m);
    assert_eq!(before, (vec![2, 1], vec![2, 1]));
    m.player_rallies[0] = Some(Point { x: 35.0, z: 35.0 });
    m.start.economy.players[0].mp = 0.0;
    m.world.game.tick += 1; // Force recipient live refresh and remembered row replay.
    m.world.set_cell(700, b'O').unwrap();
    let state = unchanged(&m);
    assert!(matches!(
        m.recorded_buy(0, "rifle", None),
        Err(RecruitError::Admission(crate::production::Error::Denied(
            crate::production::Denial::Mp
        )))
    ));
    assert_eq!(unchanged(&m), state);
    assert_eq!(probe(&m), before);
    // A subsequent legitimate routing read also preserves maintained bucket order.
    m.refresh_recipient_routing(0).unwrap();
    assert_eq!(probe(&m), before);
}

#[test]
fn refused_buy_does_not_freeze_same_tick_garrison_move_memory() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../simtest/tests/fixtures/match-recruitment.json"
    ))
    .unwrap();
    let r = &fixture["deniedRoutingRegression"];
    let mut m = make(&r["map"]);
    let slot = r["slot"].as_u64().unwrap() as usize;
    let scout = r["scout"].as_u64().unwrap() as u32;
    m.world.game.units.retain(|id, _| *id == scout);
    let unit = m.world.game.units.get_mut(&scout).unwrap();
    let at = point(&r["actorSetup"]["at"]);
    unit.x = at.x;
    unit.z = at.z;
    unit.garrison = Some(r["actorSetup"]["garrison"].as_u64().unwrap() as usize);
    unit.hold_pos = r["actorSetup"]["holdPos"].as_bool().unwrap();
    m.grid = crate::grid::SpatialGrid::new(&m.world.game.units, 16.0);
    m.world.game.tick = r["tick"].as_i64().unwrap();
    m.start.economy.players[slot].mp = r["money"].as_f64().unwrap();
    m.player_rallies[slot] = Some(point(&r["rally"]));
    let cell = r["terrainEdit"]["cell"].as_u64().unwrap() as usize;
    m.world
        .set_cell(
            cell,
            r["terrainEdit"]["character"].as_str().unwrap().as_bytes()[0],
        )
        .unwrap();
    let before = (
        m.recipients[slot].remembered().to_vec(),
        m.recipients[slot].query().navigation().versions,
        m.world
            .pending(crate::world::Recipient::Player(slot))
            .map(|v| v.to_vec()),
    );
    let state = unchanged(&m);
    let result = m.recorded_buy(slot, r["denied"]["command"]["unit"].as_str().unwrap(), None);
    assert_eq!(reason(result.unwrap_err()), r["denied"]["result"]);
    assert_eq!(unchanged(&m), state);
    assert_eq!(
        (
            m.recipients[slot].remembered().to_vec(),
            m.recipients[slot].query().navigation().versions,
            m.world
                .pending(crate::world::Recipient::Player(slot))
                .map(|v| v.to_vec())
        ),
        before
    );
    let command = crate::match_commands::Command::from_json(&r["move"]["command"]).unwrap();
    assert_eq!(m.execute_command(slot, &command, false).unwrap(), None);
    let unit = &m.world.game.units[&scout];
    assert_eq!(unit.at(), point(&r["move"]["at"]));
    assert_eq!(unit.garrison, None);
    let remembered = m.recipients[slot]
        .remembered()
        .iter()
        .find(|row| row.row.cell == cell)
        .unwrap();
    assert_eq!(
        char::from(remembered.row.ch).to_string(),
        r["move"]["rememberedCell"].as_str().unwrap()
    );
    assert_eq!(
        json!(
            unit.path
                .iter()
                .map(|p| position(Some(*p)))
                .collect::<Vec<_>>()
        ),
        r["move"]["path"]
    );
    assert_eq!(position(unit.world_goal), r["move"]["goal"]);
    assert_eq!(
        position(m.commands.units[&scout].route_end),
        r["move"]["routeEnd"]
    );
    assert_eq!(
        m.recipients[slot].remembered().len(),
        r["move"]["after"]["rows"].as_array().unwrap().len()
    );
    assert_eq!(r["denied"]["routeReads"], 0);
    assert_eq!(r["move"]["routeReads"], 1);
    assert_eq!(r["prematureRead"]["rememberedCell"], ".");
}

#[test]
fn rejected_final_preflight_discards_staged_sight_and_purchase() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../simtest/tests/fixtures/match-recruitment.json"
    ))
    .unwrap();
    let setup = || {
        let mut m = make(&fixture["cases"][0]["map"]);
        m.start.economy.players[0].mp = 5000.0;
        m.player_rallies[0] = Some(Point { x: 35.0, z: 35.0 });
        m.world.game.tick += 1;
        let warm =
            crate::match_commands::Command::from_json(&json!({"t":"move","ids":[1],"x":61,"z":41}))
                .unwrap();
        assert_eq!(m.execute_command(0, &warm, false).unwrap(), None);
        assert!(!m.world.game.units[&1].path.is_empty());
        m.world.set_cell(700, b'O').unwrap();
        m
    };
    let mut m = setup();
    let mut control = setup();
    let staged = recruit_parts!(m).stage(0, "rifle", None, false).unwrap();
    // Exercise a final story rejection after admission and before publication.
    m.story[0].mp_spent = f64::NAN;
    let before = unchanged(&m);
    let identity = {
        let query = m.recipients[0].query();
        let view = query.navigation();
        (view.flags.as_ptr(), view.versions.knowledge)
    };
    let rows = m.recipients[0].remembered().to_vec();
    let pending = m.world.pending(crate::world::Recipient::Player(0)).cloned();
    let fog = m.routing_fog.clone();
    assert!(matches!(
        m.finish_recorded_buy(staged),
        Err(RecruitError::Story(_))
    ));
    assert_eq!(unchanged(&m), before);
    let after_identity = {
        let query = m.recipients[0].query();
        let view = query.navigation();
        (view.flags.as_ptr(), view.versions.knowledge)
    };
    assert_eq!(after_identity, identity);
    assert_eq!(m.recipients[0].remembered(), rows);
    assert_eq!(
        m.world.pending(crate::world::Recipient::Player(0)),
        pending.as_ref()
    );
    assert_eq!(m.routing_fog, fog);
    // A second actual command at the same clock sees the original warmed graph,
    // as the control branch that never attempted a preview does.
    m.story[0].mp_spent = 0.0;
    let reroute =
        crate::match_commands::Command::from_json(&json!({"t":"move","ids":[1],"x":65,"z":45}))
            .unwrap();
    assert_eq!(m.execute_command(0, &reroute, false).unwrap(), None);
    assert_eq!(control.execute_command(0, &reroute, false).unwrap(), None);
    assert_eq!(
        format!("{:?}", m.world.game.units[&1]),
        format!("{:?}", control.world.game.units[&1])
    );
    assert_eq!(m.commands.units[&1], control.commands.units[&1]);
    assert_eq!(
        m.recipients[0].remembered(),
        control.recipients[0].remembered()
    );
}
