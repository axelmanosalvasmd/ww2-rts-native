//! Actual reference steps over friendly matches and the connected ordered phases.
use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn make(c: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&c["map"]).unwrap()).unwrap();
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
                    team: if c["enemy"] == true { 1 } else { 0 },
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: match c["army"].as_str().unwrap() {
                "standard" => ArmySize::Standard,
                "large" => ArmySize::Large,
                "massive" => ArmySize::Massive,
                "endless" => ArmySize::Endless,
                _ => unreachable!(),
            },
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: true,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
fn snapshot(m: &Match) -> Value {
    let g = &m.world.game;
    json!({"tick":g.tick,"visionTick":g.vision_tick,"seed":m.start.environment_seed,"wind":[bits(m.start.wind.0),bits(m.start.wind.1)],
        "units":g.units.values().map(|u|json!([u.id,bits(u.x),bits(u.z),bits(u.rot),bits(u.hp),bits(u.supp),bits(u.stuck),bits(u.repath),bits(u.sprint),u.path.iter().map(|p|json!([bits(p.x),bits(p.z)])).collect::<Vec<_>>()])).collect::<Vec<_>>(),
        "weapons":g.units.values().map(|u|{let w=m.firing.get(u.id).expect("canonical weapon row");json!([u.id,w.target_id,bits(w.cooldown),bits(w.retarget),bits(m.commands.units[&u.id].aim)])}).collect::<Vec<_>>(),
        "players":m.start.economy.players.iter().map(|p|json!([bits(p.mp),bits(p.vp),bits(p.inc)])).collect::<Vec<_>>(),
        "points":m.start.points.iter().enumerate().map(|(i,p)|json!([p.owner.map_or(-1,|v|v as i64),p.capper.map_or(-1,|v|v as i64),bits(p.progress),p.cut,p.contested,p.on_point,m.capture_ticks[i]])).collect::<Vec<_>>()})
}
#[test]
fn connected_tick_phases_match_actual_reference_steps() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    for c in fixture["cases"].as_array().unwrap() {
        let mut m = make(c);
        let input = &c["input"];
        for unit in m.world.game.units.values_mut() {
            unit.hold_pos = true;
        }
        if let Some(keep) = input["keep"].as_array() {
            m.world
                .game
                .units
                .retain(|id, _| keep.iter().any(|v| v.as_u64() == Some(u64::from(*id))));
        }
        if let Some(owners) = input["owners"].as_array() {
            for row in owners {
                m.world
                    .game
                    .units
                    .get_mut(&(row[0].as_u64().unwrap() as u32))
                    .unwrap()
                    .owner = row[1].as_u64().unwrap() as usize;
            }
        }
        if input["disableAuto"] == true {
            for control in m.commands.units.values_mut() {
                control.auto_cast = false;
            }
        }
        if input["disableRetreat"] == true {
            for control in m.commands.units.values_mut() {
                control.auto_retreat = false;
            }
        }
        m.start.environment_seed = input["seed"].as_u64().unwrap() as u32;
        m.start.wind = (
            input["wind"][0].as_f64().unwrap(),
            input["wind"][1].as_f64().unwrap(),
        );
        for row in input["positions"].as_array().unwrap() {
            let u = m
                .world
                .game
                .units
                .get_mut(&(row[0].as_u64().unwrap() as u32))
                .unwrap();
            u.x = row[1].as_f64().unwrap();
            u.z = row[2].as_f64().unwrap();
        }
        if let Some(rows) = input["hp"].as_array() {
            for row in rows {
                m.world
                    .game
                    .units
                    .get_mut(&(row[0].as_u64().unwrap() as u32))
                    .unwrap()
                    .hp = row[1].as_f64().unwrap();
            }
        }
        for row in input["paths"].as_array().unwrap() {
            let u = m
                .world
                .game
                .units
                .get_mut(&(row[0].as_u64().unwrap() as u32))
                .unwrap();
            u.path = vec![Point {
                x: row[1].as_f64().unwrap(),
                z: row[2].as_f64().unwrap(),
            }];
            u.path_gen += 1;
        }
        if let Some(changes) = input["changes"].as_array() {
            for row in changes {
                m.world
                    .set_cell(
                        row[0].as_u64().unwrap() as usize,
                        row[1].as_str().unwrap().as_bytes()[0],
                    )
                    .unwrap();
            }
        }
        if let Some(fires) = input["fires"].as_array() {
            for row in fires {
                m.world
                    .fires
                    .insert(row[0].as_u64().unwrap() as usize, row[1].as_f64().unwrap());
            }
        }
        if let Some(smokes) = input["smokes"].as_array() {
            for cloud in smokes {
                m.world.game.smokes.push(ww2_core::game::Smoke {
                    id: cloud["id"].as_u64().unwrap() as u32,
                    x: cloud["x"].as_f64().unwrap(),
                    z: cloud["z"].as_f64().unwrap(),
                    r: cloud["r"].as_f64().unwrap(),
                    remaining: Some(cloud["t"].as_f64().unwrap()),
                });
            }
        }
        if let Some(commands) = input["commands"].as_array() {
            for row in commands {
                m.enqueue_command(
                    ww2_core::orders::OrderKey {
                        tick: row["tick"].as_u64().unwrap(),
                        issuer: row["issuer"].as_u64().unwrap() as u32,
                        sequence: row["sequence"].as_u64().unwrap(),
                    },
                    ww2_core::match_commands::Command::from_json(&row["command"]).unwrap(),
                )
                .unwrap();
            }
        }
        let mut expected = c["checkpoints"].as_array().unwrap().iter().peekable();
        let mut deaths = Vec::new();
        for _ in 0..c["ticks"].as_u64().unwrap_or(200) {
            let report = m.advance_tick().unwrap();
            for retired in &report.retired {
                deaths.push(json!([
                    report.tick,
                    retired.unit.id,
                    retired.unit.owner,
                    m.rules.kinds[retired.unit.kind].name,
                    retired.last_hit
                ]));
            }
            assert!(!report.unavailable.is_empty());
            assert!(
                report
                    .phases
                    .contains(&ww2_core::match_tick::TickPhase::Firing)
            );
            assert!(
                report
                    .phases
                    .contains(&ww2_core::match_tick::TickPhase::Projectiles)
            );
            if expected
                .peek()
                .is_some_and(|e| e["tick"].as_i64() == Some(report.tick))
            {
                let mut state = snapshot(&m);
                if c["enemy"] == true {
                    let random = m.start.random.snapshot();
                    let value = random
                        .streams
                        .iter()
                        .find(|(stream, _)| *stream == ww2_core::random::StreamId::Combat)
                        .unwrap()
                        .1;
                    state["combatState"] = json!(format!("{value:016x}"));
                }
                if c["retirement"] == true {
                    state["deaths"] = json!(deaths);
                    state["story"] = json!(
                        m.story
                            .iter()
                            .map(|row| ww2_core::match_story::StoryKey::ALL
                                .into_iter()
                                .map(|key| bits(row.value(key)))
                                .collect::<Vec<_>>())
                            .collect::<Vec<_>>()
                    );
                }
                if c["abilityRecovery"] == true {
                    state["abilityRecovery"] = json!(
                        m.world
                            .game
                            .units
                            .values()
                            .map(|unit| {
                                let ability = m.abilities.get(unit.id).unwrap();
                                let recovery = m.recovery.get(unit.id).unwrap();
                                let control = &m.commands.units[&unit.id];
                                json!([
                                    unit.id,
                                    bits(ability.cooldown),
                                    bits(ability.buff),
                                    ability.ap,
                                    control.nade.map(|at| [bits(at.x), bits(at.z)]),
                                    ability.aimed_auto,
                                    ability.aimed_walked,
                                    recovery.hp_was.map(bits),
                                    recovery.wound_at,
                                    recovery.heal_at,
                                    bits(recovery.reinforcement)
                                ])
                            })
                            .collect::<Vec<_>>()
                    );
                    state["grenades"] = json!(
                        m.firing
                            .grenades
                            .iter()
                            .map(|ticket| {
                                json!([
                                    ticket.flight,
                                    bits(ticket.at.x),
                                    bits(ticket.at.z),
                                    bits(ticket.remaining),
                                    ticket.owner
                                ])
                            })
                            .collect::<Vec<_>>()
                    );
                }
                if c["field"] == true {
                    let g = &m.world.game;
                    state["field"] = json!({
                        "jobs":m.field_works.jobs().iter().map(|(id,job)|json!([
                            id,
                            job.kind.as_str(),
                            bits(job.at.x),
                            bits(job.at.z),
                            bits(job.direction),
                            bits(job.elapsed),
                            job.original_cells,
                            job.cells.iter().map(|cell|json!([cell.cell,char::from(cell.ch).to_string()])).collect::<Vec<_>>(),
                            job.front.map(bits),
                            job.reach.map(bits),
                            job.from.map(|at|[bits(at.x),bits(at.z)])
                        ])).collect::<Vec<_>>(),
                        "chars":String::from_utf8(g.chars.clone()).unwrap(),
                        "front":g.chars.iter().enumerate().filter(|(_,ch)|**ch==b'T').map(|(cell,_)|json!([
                            cell,
                            bits(f64::from(g.trench_front.as_ref().unwrap()[cell]))
                        ])).collect::<Vec<_>>()
                    });
                }
                if expected
                    .peek()
                    .is_some_and(|row| row.get("control").is_some())
                {
                    use ww2_core::match_commands::MoveOutcome;
                    state["control"] = json!(
                        m.world
                            .game
                            .units
                            .values()
                            .map(|u| {
                                let c = &m.commands.units[&u.id];
                                json!([
                                    u.id,
                                    c.outcome.map(|o| match o {
                                        MoveOutcome::Interrupted => "interrupted",
                                        MoveOutcome::Moving => "moving",
                                        MoveOutcome::Unreachable => "unreachable",
                                        MoveOutcome::Arrived => "completed",
                                    }),
                                    c.outcome_tick,
                                    c.orders.len(),
                                    u.world_goal.map(|p| [bits(p.x), bits(p.z)]),
                                    u.amove.map(|p| [bits(p.x), bits(p.z)])
                                ])
                            })
                            .collect::<Vec<_>>()
                    );
                }
                if input["fires"].is_array() {
                    state["environment"] = json!({
                        "fires":m.world.fires.iter().map(|(c,left)|json!([c,bits(*left)])).collect::<Vec<_>>(),
                        "smokes":m.world.game.smokes.iter().map(|s|json!([s.id,bits(s.x),bits(s.z),bits(s.r),bits(s.remaining.unwrap())])).collect::<Vec<_>>(),
                        "chars":String::from_utf8(m.world.game.chars.clone()).unwrap(),"burnt":m.world.burnt
                    });
                }
                assert_eq!(
                    state,
                    *expected.next().unwrap(),
                    "army {} tick {}",
                    c["army"],
                    report.tick
                );
            }
        }
        assert!(expected.next().is_none());
    }
}
#[test]
fn rejected_clock_does_not_consume_a_tick() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    let mut m = make(&fixture["cases"][0]);
    m.world.game.tick = i64::MAX;
    let before = snapshot(&m);
    assert!(m.advance_tick().is_err());
    assert_eq!(snapshot(&m), before);
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn queued_companion_join_enters_prior_and_skips_ordinary_timers() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    let mut m = make(&fixture["cases"][0]);
    let count = m.world.game.units.len();
    m.enqueue_command(
        ww2_core::orders::OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        },
        ww2_core::match_commands::Command::CompanionJoin {
            identity: ww2_core::recorded_input::CompanionJournalId::new(1).unwrap(),
        },
    )
    .unwrap();
    let report = m.advance_tick().unwrap();
    assert_eq!(m.world.game.tick, 1);
    assert_eq!(m.world.game.units.len(), count + 1);
    let id = *m
        .commands
        .units
        .iter()
        .find(|(_, control)| control.operative)
        .unwrap()
        .0;
    assert!(report.prior.contains_key(&id));
    assert!(
        report
            .phases
            .contains(&ww2_core::match_tick::TickPhase::Operative)
    );
    assert!(
        !report
            .unavailable
            .contains(&ww2_core::match_tick::TickPhase::Operative)
    );
    assert_eq!(m.world.game.units[&id].repath, 0.0);
    assert_eq!(m.world.game.units[&id].sprint, 0.0);
    assert_eq!(m.firing.get(id).unwrap().retarget, 0.0);
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn unconnected_support_boundary_preserves_all_pending_input_and_timers() {
    use ww2_core::{match_commands::Command, orders::OrderKey};
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    let mut m = make(&fixture["cases"][0]);
    let first = *m.world.game.units.keys().next().unwrap();
    m.enqueue_command(
        OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        },
        Command::Stop {
            ids: vec![i64::from(first)],
        },
    )
    .unwrap();
    m.enqueue_command(
        OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 2,
        },
        Command::Support {
            kind: "artillery".into(),
            at: Point { x: 8.0, z: 8.0 },
            direction: None,
        },
    )
    .unwrap();
    let before = snapshot(&m);
    let timers = m
        .start
        .players
        .iter()
        .map(|p| p.support.clone())
        .collect::<Vec<_>>();
    let controls = m.commands.units.clone();
    assert!(
        m.advance_tick()
            .unwrap_err()
            .contains("support strike tick actuator")
    );
    assert_eq!(snapshot(&m), before);
    assert_eq!(m.commands.units, controls);
    assert_eq!(m.commands.next_tick(), Some(1));
    assert_eq!(m.commands.pending_commands(), 2);
    assert_eq!(
        m.start
            .players
            .iter()
            .map(|p| p.support.clone())
            .collect::<Vec<_>>(),
        timers
    );
    assert!(m.support.strikes.is_empty());
    assert!(m.tick_state.fault().is_none());

    let mut future = make(&fixture["cases"][0]);
    let timers = future
        .start
        .players
        .iter()
        .map(|p| p.support.clone())
        .collect::<Vec<_>>();
    future
        .enqueue_command(
            OrderKey {
                tick: 2,
                issuer: 0,
                sequence: 1,
            },
            Command::Support {
                kind: "artillery".into(),
                at: Point { x: 8.0, z: 8.0 },
                direction: None,
            },
        )
        .unwrap();
    let report = future.advance_tick().unwrap();
    assert!(
        report
            .phases
            .contains(&ww2_core::match_tick::TickPhase::SupportTimers)
    );
    for (slot, row) in timers.iter().enumerate() {
        for (name, before) in row {
            assert_eq!(
                future.start.players[slot].support[name],
                before - future.rules.movement.tick
            );
        }
    }
    let before = snapshot(&future);
    let timers = future
        .start
        .players
        .iter()
        .map(|p| p.support.clone())
        .collect::<Vec<_>>();
    assert!(
        future
            .advance_tick()
            .unwrap_err()
            .contains("support strike tick actuator")
    );
    assert_eq!(snapshot(&future), before);
    assert_eq!(
        future
            .start
            .players
            .iter()
            .map(|p| p.support.clone())
            .collect::<Vec<_>>(),
        timers
    );
    assert_eq!(future.commands.next_tick(), Some(2));
    assert_eq!(future.commands.pending_commands(), 1);
    assert!(future.tick_state.fault().is_none());
}

#[test]
fn late_terrain_change_does_not_recompute_the_earlier_supply_beat() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    let mut c = fixture["ordering"].clone();
    c["enemy"] = json!(true);
    let mut m = make(&c);
    m.start.points[0].owner = Some(0);
    m.start.points[0].capper = Some(0);
    m.start.points[0].progress = 1.0;
    for step in c["steps"].as_array().unwrap() {
        m.world.game.tick = step["tick"].as_i64().unwrap();
        let ids: Vec<_> = m.world.game.units.keys().copied().collect();
        m.apply_point_supply(&ids).unwrap();
        for change in step["changes"].as_array().unwrap() {
            m.world
                .set_cell(
                    change[0].as_u64().unwrap() as usize,
                    change[1].as_str().unwrap().as_bytes()[0],
                )
                .unwrap();
        }
        m.apply_point_capture_income(&ids).unwrap();
        let state = snapshot(&m);
        assert_eq!(
            json!({"players":state["players"],"points":state["points"]}),
            step["expected"]
        );
    }
    assert!(m.start.points[0].cut);
}
