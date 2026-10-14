//! Complete ordinary Source steps compared with the sole native tick runner.
//! Scene inputs are authored. No expected runtime state is imported into Match.
#![recursion_limit = "256"]
use flate2::read::GzDecoder;
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode, Technology},
    economy_state::ArmySize,
    game::{CELL, Point},
    map::AuthoritativeMap,
    match_abilities::AbilityState,
    match_combat::CombatState,
    match_commands::{Command, Denial, MatchCommands, UnitControl},
    match_end::{EndReason, MatchFinish, MatchSample},
    match_firing::FiringState,
    match_recovery::RecoveryState,
    match_runtime::{BuildingCell, Match, MatchOptions, PlayerConfig},
    match_tick::{TickPathConfig, TickPhase},
    orders::OrderKey,
    production::JobStatus,
    unit_spawn,
    weather::WeatherChoice,
};
fn fixture() -> Value {
    let mut text = String::new();
    GzDecoder::new(include_bytes!("fixtures/ordinary-production-tick.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn point(p: Point) -> Value {
    json!({"x":bits(p.x),"z":bits(p.z)})
}
fn optional_point(p: Option<Point>) -> Value {
    p.map_or(Value::Null, point)
}
fn input_point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
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
fn bootstrap(m: &Match) -> Value {
    let game = &m.world.game;
    json!({"chars":String::from_utf8(game.chars.clone()).unwrap(),
      "ground":String::from_utf8(game.ground.as_ref().unwrap().clone()).unwrap(),
      "objects":String::from_utf8(game.objects.as_ref().unwrap().clone()).unwrap(),
      "flags":game.flags,"height":game.height,"cellHp":game.cell_hp.iter().map(|hp|bits(f64::from(*hp))).collect::<Vec<_>>(),
      "footprints":m.building_cells.iter().map(|(cell,b)|json!({"cell":cell,"id":b.unit,"owner":b.owner,"x":bits(b.at.x),"z":bits(b.at.z)})).collect::<Vec<_>>(),
      "structures":m.world.structures.iter().map(|s|json!({"id":s.id,"kind":s.kind,"entityId":s.entity_id,"sections":s.cells.iter().map(|cell|{
        let meta=&m.world.sections[cell];let state=&game.sections[&(*cell as u32)];
        json!({"id":meta.id,"c":cell,"hp":bits(state.hp),"maxHp":bits(state.max_hp.unwrap()),"material":meta.material,"anchor":meta.anchor,"supports":meta.supports,"rubble":meta.rubble,"state":if state.failed{"failed"}else{m.world.cell_row(*cell).unwrap().data.section.unwrap().state}})
      }).collect::<Vec<_>>()})).collect::<Vec<_>>()})
}
fn make(t: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&t["map"]).unwrap()).unwrap();
    let mut m = Match::new(
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
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: true,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    assert_eq!(
        bootstrap(&m),
        t["bootstrap"],
        "retained constructor terrain, footprints and structural sections"
    );
    // Source discards initial actors without clearing their footprints or sections.
    for id in m.world.game.units.keys().copied().collect::<Vec<_>>() {
        m.production.remove_building(id);
    }
    m.world.game.units.clear();
    m.commands = MatchCommands::new(4096);
    m.combat = CombatState::default();
    m.firing = FiringState::default();
    m.abilities = AbilityState::default();
    m.recovery = RecoveryState::default();
    m.recruit_metadata.clear();
    m.building_rallies.clear();
    m.next_unit_id = 1000;
    m.start.environment_seed = 123;
    m.start.wind = (0.0, 0.5);
    for p in &mut m.start.economy.players {
        p.mp = 5000.125;
        p.mun = 500.5;
        p.fuel = 500.25;
    }
    for spec in t["specs"].as_array().unwrap() {
        let owner = spec["owner"].as_u64().unwrap() as usize;
        let name = spec["type"].as_str().unwrap();
        let kind = m.rules.kind(name).unwrap();
        let definition = Catalog::compiled().unwrap().unit(name).unwrap();
        let size = definition.get("size").unwrap().number().unwrap() as usize;
        let max_hp = definition.get("hpPer").unwrap().number().unwrap();
        let cell = spec["cell"].as_u64().unwrap() as usize;
        let width = m.world.game.w;
        let (x, y) = (cell % width, cell / width);
        assert!(x + size <= width && y + size <= m.world.game.h);
        let cells: Vec<_> = (y..y + size)
            .flat_map(|row| (x..x + size).map(move |column| row * width + column))
            .collect();
        let top = cells
            .iter()
            .map(|&c| m.world.game.height.as_ref().map_or(0, |h| h[c]))
            .max()
            .unwrap();
        for &c in &cells {
            if m.world.game.height.as_ref().map_or(0, |h| h[c]) < top {
                m.world.set_level(c, top).unwrap();
            }
            m.world.set_cell(c, b'K').unwrap();
        }
        let at = Point {
            x: (x as f64 + size as f64 / 2.0) * CELL,
            z: (y as f64 + size as f64 / 2.0) * CELL,
        };
        let id = m.next_unit_id;
        let mut u =
            unit_spawn::initial_unit(&m.rules, Catalog::compiled().unwrap(), kind, id, owner, at)
                .unwrap();
        u.cells = Some(cells.iter().map(|&c| c as u32).collect());
        u.built = spec["built"].as_f64();
        u.hold_pos = true;
        if let Some(hp) = spec["hp"].as_f64() {
            u.hp = hp;
        }
        let mut control = UnitControl::new_for_mode(&u, &m.rules, Mode::Conquest).unwrap();
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
        m.commands.insert(id, control);
        m.combat.initialize(id);
        m.firing.initialize(id);
        m.abilities.initialize(id);
        m.recovery.initialize(id);
        m.world.game.units.insert(id, u);
        m.next_unit_id += 1;
        m.production.initialize_building(id).unwrap();
        m.building_rallies.insert(
            id,
            (!spec["rally"].is_null()).then(|| input_point(&spec["rally"])),
        );
        for &c in &cells {
            m.building_cells.insert(
                c,
                BuildingCell {
                    unit: id,
                    owner,
                    at,
                },
            );
        }
        // Source registers the finished building before applying the scene's HP patch.
        m.world
            .register_building_structure(id, &cells, max_hp, max_hp)
            .unwrap();
    }
    assert_eq!(
        bootstrap(&m),
        t["scene"],
        "actual placed-scene terrain, retained footprints and structural sections"
    );
    m.grid.rebuild(&m.world.game.units);
    ww2_core::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    for slot in 0..m.start.players.len() {
        m.refresh_recipient_routing(slot).unwrap();
    }
    m.initialize_tick_scene(
        0,
        TickPathConfig {
            budget: 4096,
            retry_limit: 1,
        },
    )
    .unwrap();
    m
}
fn patch(m: &mut Match, input: &Value) {
    if let Some(rows) = input["units"].as_array() {
        for row in rows {
            let unit = m
                .world
                .game
                .units
                .get_mut(&(row["id"].as_u64().unwrap() as u32))
                .unwrap();
            let fields = &row["fields"];
            if let Some(v) = fields["built"].as_f64() {
                unit.built = Some(v);
            }
            if let Some(v) = fields["owner"].as_u64() {
                unit.owner = v as usize;
            }
            if let Some(v) = fields["hp"].as_f64() {
                unit.hp = v;
            }
        }
    }
    if let Some(rows) = input["players"].as_array() {
        for row in rows {
            let p = &mut m.start.economy.players[row["slot"].as_u64().unwrap() as usize];
            if let Some(v) = row["fields"]["away"].as_bool() {
                p.away = v;
            }
            if let Some(v) = row["fields"]["out"].as_bool() {
                p.out = v;
            }
        }
    }
    if let Some(winner) = input["winner"].as_i64() {
        m.finish = Some(MatchFinish {
            winner: winner as i32,
            reason: EndReason::VictoryPoints,
            end_at: Point { x: 1.0, z: 1.0 },
            end_tick: m.world.game.tick,
        });
    }
}
fn projection(m: &Match) -> Value {
    json!({"tick":m.world.game.tick,"nextId":m.next_unit_id,"nextJob":m.production.next_job_id(),"winner":m.finish.map(|f|f.winner),"pathDropped":m.commands.path_requests().stats().dropped,
      "units":m.world.game.units.values().map(|u| {
        let c=&m.commands.units[&u.id];let f=m.firing.get(u.id).unwrap();let a=m.abilities.get(u.id).unwrap();
        let b=m.production.building(u.id);
        json!({"id":u.id,"type":m.rules.kinds[u.kind].name,"owner":u.owner,"hp":bits(u.hp),"x":bits(u.x),"z":bits(u.z),"rot":bits(u.rot),"aim":bits(c.aim),"built":u.built.map(bits),"cells":u.cells,"structureId":m.world.structures.iter().find(|s|s.entity_id==Some(u.id)).map(|s|&s.id),
          "queue":b.map(|b|b.queue().collect::<Vec<_>>()),"jobs":b.map_or_else(Vec::new,|b|b.rows().iter().map(|r|json!({"id":r.job.id,"type":r.job.kind,"status":if r.status==JobStatus::Active{"active"}else{"waiting"},"mp":bits(r.job.charge.mp),"fuel":bits(r.job.charge.fuel)})).collect::<Vec<_>>()),"prog":b.map(|b|bits(b.progress())),
          "rally":optional_point(m.building_rallies.get(&u.id).copied().flatten()),"path":u.path.iter().copied().map(point).collect::<Vec<_>>(),"goal":optional_point(u.world_goal),"routeEnd":optional_point(c.route_end),"ghost":bits(u.ghost),"auto":c.auto_cast,"autoRetreat":c.auto_retreat,"holdFire":c.hold_fire,"holdPos":u.hold_pos,"drift":u.drift,
          "timers":{"cooldown":bits(f.cooldown),"retarget":bits(f.retarget),"repath":bits(u.repath),"cd":bits(a.cooldown),"buff":bits(a.buff),"sprint":bits(u.sprint),"react":bits(c.react),"still":bits(u.still.expect("Source spawnUnit always authors still")),"stuck":bits(u.stuck),"supp":bits(u.supp),"moveSpeed":bits(u.move_speed),"vx":bits(u.vx.unwrap()),"vz":bits(u.vz.unwrap())}})
      }).collect::<Vec<_>>(),
      "players":m.start.economy.players.iter().enumerate().map(|(slot,p)| {
        let r=m.production.player(slot).unwrap();let t=r.tech();
        json!({"slot":slot,"mp":bits(p.mp),"mun":bits(p.mun),"fuel":bits(p.fuel),"inc":bits(p.inc),"vp":bits(p.vp),"away":p.away,"out":p.out,"tier":t.tier(),"armory":{"inf":t.level(Technology::Infantry),"guns":t.level(Technology::Guns),"armor":t.level(Technology::Armor)},"lab":r.jobs().iter().map(|j|json!({"kind":tech_name(j.kind),"prog":bits(j.progress),"time":bits(j.time)})).collect::<Vec<_>>(),"visible":m.world.game.players[slot].visible})
      }).collect::<Vec<_>>(),
      "story":m.story.iter().map(|s|json!({"kills":bits(s.kills),"losses":bits(s.losses),"built":bits(s.built),"captures":bits(s.captures),"mpSpent":bits(s.mp_spent),"supportCalls":bits(s.support_calls),"planesDowned":bits(s.planes_downed)})).collect::<Vec<_>>(),
      "points":m.start.points.iter().map(|p|json!({"owner":p.owner.map_or(-1,|x|x as i64),"capper":p.capper.map_or(-1,|x|x as i64),"prog":bits(p.progress),"on":p.on_point,"contested":p.contested,"cut":p.cut})).collect::<Vec<_>>(),
      "environment":{"seed":m.start.environment_seed as i32,"wind":[bits(m.start.wind.0),bits(m.start.wind.1)]},"shots":[]})
}
#[test]
fn original_ordinary_research_and_immediate_buy_ticks() {
    let fixture = fixture();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    let mut steps = 0;
    let mut births = 0;
    for trace in fixture["traces"].as_array().unwrap() {
        let name = trace["name"].as_str().unwrap();
        let mut m = make(trace);
        let initial_rng = m.start.random.snapshot();
        assert_eq!(projection(&m), trace["initial"], "{name}: initial scene");
        let mut sequence = [0_u64; 2];
        for frame in trace["frames"].as_array().unwrap() {
            patch(&mut m, &frame["input"]);
            let tick = m.world.game.tick as u64 + 1;
            for row in frame["commands"].as_array().unwrap() {
                let issuer = row["slot"].as_u64().unwrap() as usize;
                sequence[issuer] += 1;
                m.enqueue_command(
                    OrderKey {
                        tick,
                        issuer: issuer as u32,
                        sequence: sequence[issuer],
                    },
                    Command::from_json(&row["command"]).unwrap(),
                )
                .unwrap();
            }
            let previous_next = m.next_unit_id;
            let report = m
                .advance_tick()
                .unwrap_or_else(|e| panic!("{name}:{tick}: {e}"));
            assert_eq!(report.tick, tick as i64);
            assert_eq!(
                report.commands.len(),
                frame["commands"].as_array().unwrap().len()
            );
            for (actual, expected) in report
                .commands
                .iter()
                .zip(frame["commands"].as_array().unwrap())
            {
                assert_eq!(
                    actual.denial.map(|d| d.name()),
                    expected["result"].as_str(),
                    "{name}:{tick}: command result"
                );
            }
            let prior = report
                .prior
                .iter()
                .map(|(id, p)| json!({"id":id,"x":bits(p.x),"z":bits(p.z),"rot":bits(p.rot)}))
                .collect::<Vec<_>>();
            assert_eq!(
                json!(prior),
                frame["prior"],
                "{name}:{tick}: actual prior capture includes command births"
            );
            assert_eq!(
                json!(report.unit_order),
                frame["unitOrder"],
                "{name}:{tick}: late ascending actor membership"
            );
            assert!(
                report.events.is_empty() && report.retired.is_empty(),
                "{name}:{tick}: unexpected effect"
            );
            assert!(
                m.firing.events.is_empty()
                    && m.support.events.is_empty()
                    && m.aviation.events.is_empty()
            );
            let phase = |p| report.phases.iter().position(|v| *v == p).unwrap();
            if tick % 4 == 1 {
                assert!(phase(TickPhase::Vision) < phase(TickPhase::Research));
            }
            assert!(phase(TickPhase::Research) < phase(TickPhase::FieldProjects));
            assert!(phase(TickPhase::FieldProjects) < phase(TickPhase::Paths));
            assert_eq!(
                report
                    .phases
                    .iter()
                    .filter(|p| **p == TickPhase::Research)
                    .count(),
                1
            );
            assert!(!report.unavailable.contains(&TickPhase::Research));
            // Ordinary Buy is immediate boundary publication. Empty building queues
            // do not establish a queued-training phase in this controller scope.
            assert!(report.unavailable.contains(&TickPhase::Production));
            assert!(!report.phases.contains(&TickPhase::Production));
            assert_eq!(
                projection(&m),
                frame["after"],
                "{name}:{tick}: complete original tick projection"
            );
            assert_eq!(
                m.start.random.snapshot(),
                initial_rng,
                "{name}:{tick}: unexpected native random purpose"
            );
            for id in previous_next..m.next_unit_id {
                births += 1;
                assert!(report.prior.contains_key(&id) && report.unit_order.contains(&id));
                assert_eq!(m.combat.get(id), Some(&Default::default()));
                assert_eq!(m.recovery.get(id), Some(&Default::default()));
                // The new actor entered the ordinary timer block once this tick.
                assert_eq!(
                    m.abilities.get(id).unwrap().cooldown.to_bits(),
                    (-m.rules.movement.tick).to_bits()
                );
            }
            // Source keeps running after a winner while its story timeline stops.
            let source_timeline = &frame["sourceWitness"]["timeline"];
            let timeline = m.timeline.iter().map(|row|match row {
                MatchSample::Conquest { t, vp } => json!({"t":{"f64":bits(*t)},"vp":vp.iter().map(|v|json!({"f64":bits(*v)})).collect::<Vec<_>>()}),
                _ => panic!("ordinary scene produced another mode's sample"),
            }).collect::<Vec<_>>();
            assert_eq!(
                json!(timeline),
                *source_timeline,
                "{name}:{tick}: original story sampling"
            );
            steps += 1;
        }
    }
    assert_eq!(steps, 2429);
    assert_eq!(births, 2);
}

#[test]
fn restored_ordinary_training_queue_is_refused_before_the_clock() {
    let f = fixture();
    for source in f["restoredQueueFailures"].as_array().unwrap() {
        assert_eq!(source["failure"]["name"], "TypeError");
        assert!(
            source["failure"]["message"]
                .as_str()
                .unwrap()
                .contains("suddenDeath")
        );
        assert_eq!(source["before"]["tick"], 0);
        assert_eq!(source["after"]["tick"], 1);
        let scene = json!({"map":f["traces"][0]["map"],"specs":source["specs"],"bootstrap":source["bootstrap"],"scene":source["scene"]});
        let mut m = make(&scene);
        m.production.reconcile_queue(1000, &["rifle"]).unwrap();
        let key = OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 37,
        };
        m.enqueue_command(key, Command::Unsupported).unwrap();
        let before = projection(&m);
        let rng = m.start.random.snapshot();
        let queue = (m.commands.pending_commands(), m.commands.next_tick());
        assert_eq!(queue, (1, Some(key.tick)));
        let failure = m.advance_tick().unwrap_err();
        assert_eq!(
            failure.reason,
            "production admission: InvalidState(\"nonempty Conquest training queue\")"
        );
        assert_eq!(failure.tick, 0);
        assert!(failure.events.is_empty());
        assert!(m.tick_state.fault().is_none());
        assert_eq!(projection(&m), before);
        assert_eq!(
            (m.commands.pending_commands(), m.commands.next_tick()),
            queue
        );
        assert_eq!(m.start.random.snapshot(), rng);
        m.production.reconcile_queue(1000, &[]).unwrap();
        let report = m.advance_tick().unwrap();
        assert_eq!(report.tick, key.tick as i64);
        assert_eq!(report.commands.len(), 1);
        assert_eq!(report.commands[0].key, key);
        assert_eq!(report.commands[0].denial, Some(Denial::Unsupported));
        assert_eq!(m.commands.pending_commands(), 0);
        assert_eq!(m.commands.next_tick(), Some(key.tick + 1));
    }
}
