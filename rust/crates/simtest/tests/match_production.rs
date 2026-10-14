//! Original command/helper production checkpoints on canonical Match methods.
//! Tick checkpoints test only early research and building training. Unrelated
//! source wallet/HP/clock context is imported afterward, not executed natively.
use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    conquest_end::{ConquestEndReason, ConquestFinish},
    economy::{Faction, Mode, Price, Technology},
    economy_state::{ArmySize, ModeContext, WorldContext},
    game::{CELL, LOCAL_DEFENDER_OWNER, Point},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig, ProductionStepError},
    match_story::{StoryChange, StoryKey, prepare_story_updates},
    production::{Context, Denial, Error, JobStatus, UnitView},
    random::MatchRandom,
    unit_publication::{
        PublicationParts, StagedUnit, prepare_publication, prepare_world_publication,
    },
    weather::WeatherChoice,
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/match-production.json")).unwrap()
}
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn pos(p: Option<Point>) -> Value {
    p.map_or(Value::Null, |p| json!([bits(p.x), bits(p.z)]))
}
fn tech(s: &str) -> Technology {
    match s {
        "tier" => Technology::Tier,
        "inf" => Technology::Infantry,
        "guns" => Technology::Guns,
        "armor" => Technology::Armor,
        _ => panic!("technology"),
    }
}
fn tech_name(t: Technology) -> &'static str {
    match t {
        Technology::Tier => "tier",
        Technology::Infantry => "inf",
        Technology::Guns => "guns",
        Technology::Armor => "armor",
    }
}
fn make(c: &Value) -> Match {
    let mut map = c["map"].clone();
    let w = map["w"].as_u64().unwrap() as usize;
    let h = map["h"].as_u64().unwrap() as usize;
    let row = map["fill"].as_str().unwrap().repeat(w);
    map.as_object_mut().unwrap().remove("fill");
    map["rows"] = json!(vec![row; h]);
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&map).unwrap()).unwrap();
    let options = MatchOptions {
        seed: 701,
        mode: if c["map"]["world"].is_null() {
            Mode::Conquest
        } else {
            Mode::World
        },
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
        tech: true,
        supply: true,
        logistics: false,
        sandbox: false,
    };
    let mut m = if options.mode == Mode::World {
        Match::new_world(&map, options, MatchRandom::new(701))
    } else {
        Match::new(&map, options)
    }
    .unwrap();
    for p in &mut m.start.economy.players {
        p.mp = 5000.125;
        p.mun = 500.5;
        p.fuel = 500.25;
    }
    assert_eq!(
        json!(m.world.game.units.keys().collect::<Vec<_>>()),
        c["initialIds"],
        "constructor IDs {}",
        c["name"]
    );
    m
}
fn projection(m: &Match, makers: &[u32], initial_next: u32) -> Value {
    json!({"tick":m.world.game.tick,"next":m.next_unit_id,"nextJob":m.production.next_job_id(),"mode":if m.options.mode==Mode::World{json!("world")}else{Value::Null},"winner":m.finish.map(|f|f.winner),
 "players":m.start.economy.players.iter().enumerate().map(|(slot,p)|{let r=m.production.player(slot).unwrap();let t=r.tech();json!({"slot":slot,"away":p.away,"out":p.out,"mp":bits(p.mp),"mun":bits(p.mun),"fuel":bits(p.fuel),"tier":t.tier(),"armory":{"inf":t.level(Technology::Infantry),"guns":t.level(Technology::Guns),"armor":t.level(Technology::Armor)},"lab":r.jobs().iter().map(|j|json!({"kind":tech_name(j.kind),"progress":bits(j.progress),"time":bits(j.time)})).collect::<Vec<_>>(),"spent":bits(m.story[slot].mp_spent)})}).collect::<Vec<_>>(),
 "buildings":makers.iter().filter_map(|id|m.world.game.units.get(id).map(|u|{let b=m.production.building(*id).unwrap();json!({"id":id,"owner":u.owner,"kind":m.rules.kinds[u.kind].name,"hp":bits(u.hp),"built":bits(u.built.unwrap()),"queue":b.queue().collect::<Vec<_>>(),"jobs":b.rows().iter().map(|r|json!({"id":r.job.id,"type":r.job.kind,"status":if r.status==JobStatus::Active{"active"}else{"waiting"},"mp":bits(r.job.charge.mp),"fuel":bits(r.job.charge.fuel)})).collect::<Vec<_>>(),"progress":bits(b.progress()),"rally":pos(*m.building_rallies.get(id).unwrap())})})).collect::<Vec<_>>(),
 "created":m.world.game.units.values().filter(|u|u.id>=initial_next).map(|u|{let c=&m.commands.units[&u.id];json!({"id":u.id,"owner":u.owner,"kind":m.rules.kinds[u.kind].name,"hp":bits(u.hp),"pose":[bits(u.x),bits(u.z),bits(u.rot),bits(c.aim),bits(u.ghost)],"goal":pos(u.world_goal),"routeEnd":pos(c.route_end),"path":u.path.iter().map(|p|pos(Some(*p))).collect::<Vec<_>>(),"rally":u.drift,"auto":c.auto_cast,"autoRetreat":c.auto_retreat,"grid":m.grid.in_order(&m.world.game.units,(u.x,u.z),12.0,|_,_|true)})}).collect::<Vec<_>>()})
}
fn reason(e: Error) -> Value {
    if let Error::Denied(d) = e {
        json!(match d {
            Denial::Blocked => "blocked",
            Denial::Needs => "needs",
            Denial::Tier => "tier",
            Denial::Mp => "mp",
            Denial::Mun => "mun",
            Denial::Fuel => "fuel",
            Denial::Pop => "pop",
            Denial::Max => "max",
            Denial::QueueFull => "queueFull",
            Denial::SuddenDeath => "suddenDeath",
            _ => panic!("denial"),
        })
    } else {
        panic!("{e:?}")
    }
}
fn resolve(m: &Match, slot: usize, name: &str) -> u32 {
    m.world
        .game
        .units
        .values()
        .find(|u| u.owner == slot && m.rules.kinds[u.kind].name == name)
        .unwrap()
        .id
}
// Explicit paid queue admission. The World command dispatcher is not exercised.
fn buy(m: &mut Match, slot: usize, kind: &str, from: Option<u32>) -> Result<(), Error> {
    let units: Vec<_> = m
        .world
        .game
        .units
        .values()
        .map(|u| UnitView {
            id: u.id,
            owner: (u.owner != LOCAL_DEFENDER_OWNER).then_some(u.owner),
            kind: &m.rules.kinds[u.kind].name,
            hp: u.hp,
            built: u.built.unwrap_or(1.0),
        })
        .collect();
    let regions = m.world_economy_regions().unwrap();
    let c = Context {
        mode: ModeContext {
            kind: Mode::World,
            ..Default::default()
        },
        army: m.options.army,
        tech_enabled: m.options.tech,
        naval: m.world.game.naval,
        sudden_death: false,
        units: &units,
        drops: &[],
        world: Some(WorldContext { regions: &regions }),
    };
    let charge = ww2_core::economy::price_of(Mode::World, kind).unwrap();
    let story = prepare_story_updates(
        &mut m.story,
        false,
        &[StoryChange {
            slot: Some(slot),
            key: StoryKey::MpSpent,
            amount: charge.mp,
        }],
    )
    .unwrap();
    m.production.queue_unit(
        &c,
        &mut m.start.economy,
        slot,
        m.start.players[slot].config.faction,
        kind,
        from,
    )?;
    story.commit();
    Ok(())
}
// Finished armory fixture setup reuses shared source scatter/publication, then
// stamps its authored footprint. This is not a paid native construction command.
fn armory(m: &mut Match, slot: usize, cell: usize) -> u32 {
    finished_building(m, slot, cell, "armory")
}
fn finished_building(m: &mut Match, slot: usize, cell: usize, name: &str) -> u32 {
    let kind = m.rules.kind(name).unwrap();
    let size = Catalog::compiled()
        .unwrap()
        .unit(name)
        .unwrap()
        .get("size")
        .unwrap()
        .number()
        .unwrap() as usize;
    let at = Point {
        x: (cell % m.world.game.w) as f64 * CELL + size as f64 * CELL / 2.0,
        z: (cell / m.world.game.w) as f64 * CELL + size as f64 * CELL / 2.0,
    };
    let id = m.next_unit_id;
    let mut draft = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        slot,
        kind,
        id,
        m.world.game.units.len() as u32,
        m.start.players[slot].spawn,
        Some(at),
    )
    .unwrap();
    let width = m.world.game.w;
    let cells: Vec<_> = (0..size)
        .flat_map(|y| (0..size).map(move |x| cell + y * width + x))
        .collect();
    draft.unit.built = Some(1.0);
    draft.unit.cells = Some(cells.iter().map(|c| *c as u32).collect());
    let parts = PublicationParts {
        units: &mut m.world.game.units,
        grid: &mut m.grid,
        controls: &mut m.commands.units,
        recruitment: &mut m.recruit_metadata,
        next_id: &mut m.next_unit_id,
    };
    let publication = if m.options.mode == Mode::World {
        prepare_world_publication(
            parts,
            &m.rules,
            m.start.players.len(),
            m.world.game.mode,
            vec![draft],
        )
    } else {
        prepare_publication(parts, &m.rules, m.start.players.len(), vec![draft])
    };
    publication.unwrap().publish_with(|u, _| {
        m.combat.initialize(u.id);
        m.firing.initialize(u.id);
        m.abilities.initialize(u.id);
        m.recovery.initialize(u.id);
    });
    for c in cells {
        m.world.set_cell(c, b'K').unwrap();
    }
    m.production.initialize_building(id).unwrap();
    m.building_rallies.insert(id, None);
    id
}
fn phase(v: &Value) -> Value {
    json!({"players":v["players"].as_array().unwrap().iter().map(|p|json!({"tier":p["tier"],"armory":p["armory"],"lab":p["lab"],"spent":p["spent"]})).collect::<Vec<_>>(),"buildings":v["buildings"].as_array().unwrap().iter().map(|b|json!({"id":b["id"],"queue":b["queue"],"jobs":b["jobs"],"progress":b["progress"]})).collect::<Vec<_>>(),"next":v["next"],"nextJob":v["nextJob"]})
}
// Original full ticks additionally run income, combat, section synchronization,
// movement and winner rules. Import only their projected context after checking
// native research/training, so subsequent helper actions have the same inputs.
fn import_tick_context(m: &mut Match, e: &Value) {
    m.world.game.tick = e["tick"].as_i64().unwrap();
    for (slot, p) in e["players"].as_array().unwrap().iter().enumerate() {
        let w = &mut m.start.economy.players[slot];
        w.mp = number(&p["mp"]);
        w.mun = number(&p["mun"]);
        w.fuel = number(&p["fuel"]);
        w.away = p["away"].as_bool().unwrap();
        w.out = p["out"].as_bool().unwrap();
    }
    for b in e["buildings"].as_array().unwrap() {
        let u = m
            .world
            .game
            .units
            .get_mut(&(b["id"].as_u64().unwrap() as u32))
            .unwrap();
        u.hp = number(&b["hp"]);
        u.built = Some(number(&b["built"]));
    }
}
#[test]
fn original_match_research_and_world_training_phases() {
    let f = fixture();
    let mut completions = 0;
    let mut unreachable = 0;
    for c in f["cases"].as_array().unwrap() {
        let mut m = make(c);
        let initial_next = m.next_unit_id;
        let mut makers: Vec<_> = c["start"]["buildings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["id"].as_u64().unwrap() as u32)
            .collect();
        assert_eq!(
            projection(&m, &makers, initial_next),
            c["start"],
            "{} start",
            c["name"]
        );
        for (index, s) in c["stages"].as_array().unwrap().iter().enumerate() {
            let a = &s["action"];
            let slot = a["slot"].as_u64().unwrap_or(0) as usize;
            let mut result = Value::Null;
            match a["op"].as_str().unwrap() {
                "command" => {
                    let cmd = &a["command"];
                    result = if cmd["t"] == "tech" {
                        match m.request_research(slot, tech(cmd["kind"].as_str().unwrap())) {
                            Ok(_) => Value::Null,
                            Err(ProductionStepError::Production(e)) => reason(e),
                            Err(e) => panic!("{e:?}"),
                        }
                    } else {
                        let from = a["fromMaker"].as_str().map(|name| resolve(&m, slot, name));
                        buy(&mut m, slot, cmd["unit"].as_str().unwrap(), from)
                            .map_or_else(reason, |_| Value::Null)
                    };
                }
                "lab-helper" => {
                    m.step_research(a["dt"].as_f64().unwrap()).unwrap();
                }
                "train-helper" => {
                    let done = m
                        .step_training(a["id"].as_u64().unwrap() as u32, a["dt"].as_f64().unwrap())
                        .unwrap();
                    if let Some(done) = done {
                        assert_eq!(m.abilities.get(done.unit), Some(&Default::default()));
                        assert_eq!(m.recovery.get(done.unit), Some(&Default::default()));
                        completions += 1;
                        unreachable += usize::from(
                            done.route_status
                                == Some(ww2_core::pathfinding::PathStatus::Unreachable),
                        );
                        assert_eq!(
                            m.combat.get(done.unit).unwrap(),
                            &ww2_core::match_combat::UnitCombat::default()
                        );
                        assert_eq!(
                            m.firing.get(done.unit).unwrap(),
                            &ww2_core::match_firing::FiringUnit::default()
                        );
                    }
                }
                "tick" => {
                    m.world.game.tick += 1;
                    m.step_research(0.05).unwrap();
                    for id in &makers {
                        m.step_training(*id, 0.05).unwrap();
                    }
                    assert_eq!(
                        phase(&projection(&m, &makers, initial_next)),
                        phase(&s["expected"]),
                        "{} phase tick {index}",
                        c["name"]
                    );
                    import_tick_context(&mut m, &s["expected"]);
                }
                "player" => {
                    let p = &mut m.start.economy.players[slot];
                    let fields = &a["fields"];
                    if let Some(v) = fields["away"].as_bool() {
                        p.away = v;
                    }
                    if let Some(v) = fields["out"].as_bool() {
                        p.out = v;
                    }
                }
                "maker" => {
                    let u = m
                        .world
                        .game
                        .units
                        .get_mut(&(a["id"].as_u64().unwrap() as u32))
                        .unwrap();
                    let fields = &a["fields"];
                    if let Some(v) = fields["hp"].as_f64() {
                        u.hp = v;
                    }
                    if let Some(v) = fields["built"].as_f64() {
                        u.built = Some(v);
                    }
                }
                "winner" => {
                    m.finish = a["value"].as_bool().unwrap().then_some(ConquestFinish {
                        winner: 0,
                        reason: ConquestEndReason::VictoryPoints,
                        end_at: Point { x: 1.0, z: 1.0 },
                        end_tick: m.world.game.tick,
                    });
                }
                "finished-building" => {
                    let id = armory(&mut m, slot, a["cell"].as_u64().unwrap() as usize);
                    assert_eq!(id, a["id"].as_u64().unwrap() as u32);
                    makers.push(id);
                }
                "rally" => {
                    m.building_rallies
                        .insert(a["id"].as_u64().unwrap() as u32, Some(point(&a["to"])));
                }
                "cell" => {
                    m.world
                        .set_cell(
                            a["cell"].as_u64().unwrap() as usize,
                            a["character"].as_str().unwrap().as_bytes()[0],
                        )
                        .unwrap();
                }
                _ => panic!("action"),
            }
            assert_eq!(result, s["result"], "{} denial {index}", c["name"]);
            assert_eq!(
                projection(&m, &makers, initial_next),
                s["expected"],
                "{} checkpoint {index}",
                c["name"]
            );
        }
    }
    assert!(completions >= 4 && unreachable > 0);
}
fn unchanged(m: &Match) -> String {
    format!(
        "{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{}{:?}",
        m.production,
        m.start.economy,
        m.world.game.units,
        m.commands.units,
        m.combat,
        m.firing,
        m.abilities,
        m.recovery,
        m.story,
        m.recruit_metadata,
        m.next_unit_id,
        (
            &m.grid,
            m.start.random.snapshot(),
            m.start.environment_seed,
            m.world.game.tick,
            &m.building_rallies,
            &m.player_rallies
        )
    )
}
#[test]
fn aborted_completion_and_research_publish_nothing() {
    let f = fixture();
    let c = &f["cases"][2];
    for failure in ["combat", "firing", "abilities", "recovery", "id"] {
        let mut m = make(c);
        let hq = resolve(&m, 0, "hq");
        buy(&mut m, 0, "engineer", Some(hq)).unwrap();
        match failure {
            "combat" => {
                m.combat.initialize(m.next_unit_id);
            }
            "firing" => {
                m.firing.initialize(m.next_unit_id);
            }
            "abilities" => {
                m.abilities.initialize(m.next_unit_id);
            }
            "recovery" => {
                m.recovery.initialize(m.next_unit_id);
            }
            "id" => m.next_unit_id = u32::MAX,
            _ => unreachable!(),
        };
        let before = unchanged(&m);
        assert!(m.step_training(hq, 12.0).is_err());
        assert_eq!(unchanged(&m), before);
    }
    let mut m = make(&f["cases"][0]);
    m.story[0].mp_spent = f64::NAN;
    let before = unchanged(&m);
    assert!(m.request_research(0, Technology::Tier).is_err());
    assert_eq!(unchanged(&m), before);
    m.story[0].mp_spent = 0.0;
    let barracks = resolve(&m, 0, "barracks");
    m.production
        .record_paid(barracks, "rifle", Price { mp: 0.0, fuel: 0.0 })
        .unwrap();
    let before = unchanged(&m);
    assert!(matches!(
        m.step_training(barracks, 30.0),
        Err(ProductionStepError::InvalidState(
            "nonempty Conquest training queue"
        ))
    ));
    assert_eq!(unchanged(&m), before);
}

#[test]
fn production_phase_admission_is_read_only_and_rejects_foreign_rows() {
    let f = fixture();
    let mut m = make(&f["cases"][0]);
    m.request_research(0, Technology::Tier).unwrap();
    let before = unchanged(&m);
    m.validate_production_phases(0.05).unwrap();
    assert_eq!(unchanged(&m), before);
    m.production.initialize_building(u32::MAX - 1).unwrap();
    let before = unchanged(&m);
    assert!(m.validate_production_phases(0.05).is_err());
    assert_eq!(unchanged(&m), before);
}

#[test]
fn recorded_world_buy_pays_once_and_publishes_only_at_training_completion() {
    use ww2_core::{
        match_commands::{Command, Denial as CommandDenial},
        orders::OrderKey,
    };
    let f = fixture();
    let mut m = make(&f["cases"][2]);
    let hq = resolve(&m, 0, "hq");
    let foreign = resolve(&m, 1, "hq");
    let next = m.next_unit_id;
    let wallet = m.start.economy.players[0].clone();
    let spent = m.story[0].mp_spent;
    let random = m.start.random.snapshot();
    let engineer = ww2_core::economy::price_of(Mode::World, "engineer").unwrap();
    let rifle = ww2_core::economy::price_of(Mode::World, "rifle").unwrap();
    // Reverse arrival order must still publish the one recorded boundary in key order.
    for (sequence, kind, from) in [
        (3, "engineer", Some(foreign)),
        (2, "rifle", None),
        (1, "engineer", Some(hq)),
    ] {
        m.enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence,
            },
            Command::Buy {
                kind: kind.into(),
                from,
            },
        )
        .unwrap();
    }
    assert_eq!(m.production.next_job_id(), 0);
    assert_eq!(m.start.economy.players[0], wallet);
    let results = m.apply_commands(1, false).unwrap();
    assert_eq!(
        results.iter().map(|r| r.key.sequence).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(
        results.iter().map(|r| r.denial).collect::<Vec<_>>(),
        [None, None, Some(CommandDenial::QueueFull)]
    );
    let jobs = m.production.building(hq).unwrap().rows();
    assert_eq!(
        jobs.iter()
            .map(|r| (r.job.id, r.job.kind.as_str(), r.status))
            .collect::<Vec<_>>(),
        [
            (1, "engineer", JobStatus::Active),
            (2, "rifle", JobStatus::Waiting)
        ]
    );
    assert_eq!(jobs[0].job.charge, engineer);
    assert_eq!(jobs[1].job.charge, rifle);
    assert_eq!(m.production.next_job_id(), 2);
    assert_eq!(m.next_unit_id, next);
    assert!(!m.world.game.units.contains_key(&next));
    assert_eq!(
        m.start.economy.players[0].mp,
        wallet.mp - engineer.mp - rifle.mp
    );
    assert_eq!(
        m.start.economy.players[0].fuel,
        wallet.fuel - engineer.fuel - rifle.fuel
    );
    assert_eq!(m.story[0].mp_spent, spent + engineer.mp + rifle.mp);
    let paid = (m.start.economy.clone(), m.story.clone());
    assert!(m.step_training(hq, 11.0).unwrap().is_none());
    assert_eq!(m.next_unit_id, next);
    let done = m.step_training(hq, 1.0).unwrap().unwrap();
    assert_eq!(done.unit, next);
    assert_eq!(done.completion.job_id, 1);
    assert_eq!(done.completion.building, hq);
    assert_eq!(done.completion.kind, "engineer");
    assert_eq!(
        m.rules.kinds[m.world.game.units[&next].kind].name,
        "engineer"
    );
    assert!(m.commands.units.contains_key(&next));
    assert!(!m.commands.units[&next].hold_fire);
    assert!(!m.commands.units[&next].auto_cast);
    m.validate_production_phases(0.05).unwrap();
    assert_eq!(m.combat.get(next), Some(&Default::default()));
    assert_eq!(
        m.production.building(hq).unwrap().rows()[0].job,
        jobs[1].job
    );
    assert_eq!((m.start.economy.clone(), m.story.clone()), paid);
    assert_eq!(m.start.random.snapshot(), random);

    let buy = Command::Buy {
        kind: "engineer".into(),
        from: Some(hq),
    };
    for fault in [
        "story", "rally", "producer", "mode", "away", "out", "winner",
    ] {
        let story = m.story.clone();
        let rally = m.building_rallies[&hq];
        let production = m.production.clone();
        match fault {
            "story" => m.story[0].mp_spent = f64::NAN,
            "rally" => {
                m.building_rallies.insert(
                    hq,
                    Some(Point {
                        x: f64::NAN,
                        z: 0.0,
                    }),
                );
            }
            "producer" => {
                m.production.remove_building(hq);
            }
            "mode" => m.world.game.mode = None,
            "away" => m.start.economy.players[0].away = true,
            "out" => m.start.economy.players[0].out = true,
            "winner" => {}
            _ => unreachable!(),
        }
        let before = unchanged(&m);
        let result = m.execute_command(0, &buy, fault == "winner");
        if matches!(fault, "away" | "out" | "winner") {
            assert_eq!(result.unwrap(), Some(CommandDenial::Blocked));
        } else {
            assert!(result.is_err(), "{fault}: {result:?}");
        }
        assert_eq!(unchanged(&m), before, "{fault}: partial publication");
        m.story = story;
        m.building_rallies.insert(hq, rally);
        m.production = production;
        m.world.game.mode = Some(ww2_core::game::ModeKind::World);
        m.start.economy.players[0].away = false;
        m.start.economy.players[0].out = false;
    }
    while m.production.building(hq).unwrap().jobs().len() < 5 {
        assert_eq!(m.execute_command(0, &buy, false).unwrap(), None);
    }
    let before = unchanged(&m);
    assert_eq!(
        m.execute_command(0, &buy, false).unwrap(),
        Some(CommandDenial::QueueFull)
    );
    assert_eq!(unchanged(&m), before);
    m.start.economy.players[0].mp = 0.0;
    let before = unchanged(&m);
    assert_eq!(
        m.execute_command(0, &buy, false).unwrap(),
        Some(CommandDenial::Manpower)
    );
    assert_eq!(unchanged(&m), before);
}

fn world_barracks_scene() -> (Match, u32) {
    let mut m = make(&fixture()["cases"][2]);
    m.request_research(0, Technology::Tier).unwrap();
    assert_eq!(m.step_research(60.0).unwrap(), [(0, Technology::Tier)]);
    assert_eq!(m.production.player(0).unwrap().tech().tier(), 2);
    let cell = 20 * m.world.game.w + 20;
    let barracks = finished_building(&mut m, 0, cell, "barracks");
    m.validate_tick_state().unwrap();
    (m, barracks)
}

#[test]
fn world_buy_refuses_unsupported_newborn_before_payment() {
    use ww2_core::match_commands::{Command, Denial as CommandDenial};
    let (mut m, barracks) = world_barracks_scene();
    let command = Command::Buy {
        kind: "mortar".into(),
        from: Some(barracks),
    };
    // Canonical resource denial still precedes future-birth support.
    m.start.economy.players[0].mp = 0.0;
    let before = unchanged(&m);
    assert_eq!(
        m.execute_command(0, &command, false).unwrap(),
        Some(CommandDenial::Manpower)
    );
    assert_eq!(unchanged(&m), before);
    m.start.economy.players[0].mp = 5000.125;
    let before = unchanged(&m);
    let result = m.execute_command(0, &command, false);
    if result == Ok(None) {
        // Bounded failure witness: actual completion, then the next admission.
        let born = m.step_training(barracks, 20.0).unwrap().unwrap();
        assert!(!m.commands.units[&born.unit].hold_fire);
        eprintln!(
            "unsupported Buy admitted: MP-spent={}, newborn={}, next admission={:?}",
            m.story[0].mp_spent,
            born.unit,
            m.validate_tick_state()
        );
    }
    assert!(
        matches!(&result, Err(error) if error.contains("World automatic salvo tick actuator is not connected")),
        "unsupported future birth must fail its actuator guard: {result:?}"
    );
    assert_eq!(unchanged(&m), before);
    m.enqueue_command(
        ww2_core::orders::OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        },
        command,
    )
    .unwrap();
    let before = unchanged(&m);
    assert!(
        m.validate_tick_state()
            .unwrap_err()
            .contains("World automatic salvo tick actuator is not connected")
    );
    assert_eq!(unchanged(&m), before);
    assert_eq!(m.commands.pending_commands(), 1);
    assert_eq!(m.commands.next_tick(), Some(1));
    drop(m);

    // Restored jobs protect the same future-birth admission boundary.
    let (mut m, barracks) = world_barracks_scene();
    // An existing authored salvo actor remains supported when it holds fire.
    let kind = m.rules.kind("mortar").unwrap();
    let mut actor = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        0,
        kind,
        m.next_unit_id,
        m.world.game.units.len() as u32,
        m.start.players[0].spawn,
        None,
    )
    .unwrap();
    assert!(!actor.control.hold_fire && !actor.control.auto_cast);
    actor.control.hold_fire = true;
    prepare_world_publication(
        PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        m.start.players.len(),
        m.world.game.mode,
        vec![actor],
    )
    .unwrap()
    .publish_with(|u, _| {
        m.combat.initialize(u.id);
        m.firing.initialize(u.id);
        m.abilities.initialize(u.id);
        m.recovery.initialize(u.id);
    });
    m.validate_tick_state().unwrap();
    m.production
        .record_paid(
            barracks,
            "mortar",
            Price {
                mp: 180.0,
                fuel: 0.0,
            },
        )
        .unwrap();
    let before = unchanged(&m);
    let production = m.validate_production_phases(0.05);
    let admission = m.validate_tick_state();
    let training = m.step_training(barracks, 20.0);
    eprintln!(
        "restored mortar: production={production:?}, tick admission={admission:?}, training={training:?}"
    );
    assert!(
        matches!(production, Err(ProductionStepError::Control(ref reason)) if reason == "World automatic salvo tick actuator is not connected"),
        "restored mortar must fail its production actuator guard"
    );
    assert!(
        matches!(admission, Err(ref reason) if reason.contains("World automatic salvo tick actuator is not connected")),
        "restored mortar must fail its whole-tick actuator guard"
    );
    assert!(
        matches!(training, Err(ProductionStepError::Control(ref reason)) if reason == "World automatic salvo tick actuator is not connected"),
        "restored mortar must fail its training actuator guard"
    );
    assert_eq!(unchanged(&m), before);
}

#[test]
fn world_unknown_buy_is_a_denial_without_aborting_remaining_boundary_orders() {
    use ww2_core::{
        match_commands::{Command, Denial as CommandDenial},
        orders::OrderKey,
    };
    let mut m = make(&fixture()["cases"][2]);
    let hq = resolve(&m, 0, "hq");
    let unknown = Command::from_json(&json!({"t":"buy", "unit":"bogus"})).unwrap();
    let before = unchanged(&m);
    let direct = m.execute_command(0, &unknown, false);
    assert_eq!(unchanged(&m), before);
    assert!(matches!(
        m.production
            .record_paid(hq, "bogus", Price { mp: 0.0, fuel: 0.0 }),
        Err(Error::Catalog(_))
    ));
    assert_eq!(unchanged(&m), before);
    for (sequence, command) in [
        (
            2,
            Command::Buy {
                kind: "engineer".into(),
                from: Some(hq),
            },
        ),
        (1, unknown),
    ] {
        m.enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence,
            },
            command,
        )
        .unwrap();
    }
    let boundary = m.apply_commands(1, false);
    eprintln!("unknown World Buy: direct={direct:?}, boundary={boundary:?}");
    assert_eq!(direct, Ok(Some(CommandDenial::Blocked)));
    let results = boundary.unwrap();
    assert_eq!(
        results
            .iter()
            .map(|r| (r.key.sequence, r.denial))
            .collect::<Vec<_>>(),
        [(1, Some(CommandDenial::Blocked)), (2, None)]
    );
    assert_eq!(m.commands.pending_commands(), 0);
    assert_eq!(m.commands.next_tick(), Some(2));
    assert_eq!(m.production.next_job_id(), 1);
    let jobs = m.production.building(hq).unwrap().rows();
    assert_eq!(jobs.len(), 1);
    assert_eq!((jobs[0].job.id, jobs[0].job.kind.as_str()), (1, "engineer"));
    assert_eq!(m.start.economy.players[0].mp, 4940.125);
    assert_eq!(m.story[0].mp_spent, 60.0);
}

#[test]
fn ordinary_training_refusal_precedes_research_progress() {
    let f = fixture();
    let mut m = make(&f["cases"][0]);
    m.request_research(0, Technology::Tier).unwrap();
    let barracks = resolve(&m, 0, "barracks");
    m.production
        .record_paid(barracks, "rifle", Price { mp: 0.0, fuel: 0.0 })
        .unwrap();
    let before = unchanged(&m);
    assert_eq!(
        m.validate_production_phases(0.05),
        Err(ProductionStepError::InvalidState(
            "nonempty Conquest training queue"
        ))
    );
    assert_eq!(unchanged(&m), before);
}
