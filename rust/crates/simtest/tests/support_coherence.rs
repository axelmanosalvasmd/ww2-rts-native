#![recursion_limit = "256"]
//! Original support/aircraft component calls, with persistent canonical native owners.
//! This does not run or certify a complete Match tick. SourceRest remains in the oracle.
//! The source-only random draw counter is retained; the actual OffMap state is compared.
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    fog::Fog,
    game::{PlaneState, Point},
    map::AuthoritativeMap,
    match_aviation::AviationEvent,
    match_commands::{Command, MoveOutcome},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_support::SupportEvent,
    random::{MatchRandom, StreamId},
    unit_publication::{PublicationParts, SpawnActor, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};

fn fixture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("fixtures/support-coherence.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn bits(value: f64) -> Value {
    json!(format!("{:016x}", value.to_bits()))
}
fn point(at: Point) -> Value {
    json!({"x":at.x,"z":at.z})
}
fn compare(actual: &Value, expected: &Value, path: &str) {
    match expected {
        Value::Number(_) => assert_eq!(
            actual.as_f64().unwrap().to_bits(),
            expected.as_f64().unwrap().to_bits(),
            "{path}"
        ),
        Value::Array(rows) => {
            let actual = actual.as_array().unwrap();
            assert_eq!(actual.len(), rows.len(), "{path}");
            for (i, (a, e)) in actual.iter().zip(rows).enumerate() {
                compare(a, e, &format!("{path}[{i}]"));
            }
        }
        Value::Object(rows) => {
            let actual = actual.as_object().unwrap();
            assert_eq!(actual.len(), rows.len(), "{path}");
            for (key, e) in rows {
                compare(actual.get(key).unwrap(), e, &format!("{path}.{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

fn scene(trace: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&trace["map"]).unwrap()).unwrap();
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
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    // The original scene removes bootstrap actors, while retaining its map and players.
    m.world.game.units.clear();
    m.commands.units.clear();
    m.recruit_metadata.clear();
    m.combat = Default::default();
    m.firing = Default::default();
    m.abilities = Default::default();
    m.recovery = Default::default();
    m.aviation = Default::default();
    m.logistics = Default::default();
    m.next_unit_id = 1000;
    m.grid.rebuild(&m.world.game.units);
    for spec in trace["scene"].as_array().unwrap() {
        let owner = spec["owner"].as_u64().unwrap() as usize;
        let name = spec["kind"].as_str().unwrap();
        let kind = m.rules.kind(name).unwrap();
        let id = if m.rules.kinds[kind].air {
            m.spawn_aircraft(owner, name).unwrap()
        } else {
            let id = m.next_unit_id;
            let draft = StagedUnit::ordinary_with_actor(
                &m.world.game,
                &m.rules,
                SpawnActor::Commander {
                    owner,
                    spawn: m.start.players[owner].spawn,
                },
                kind,
                id,
                m.world.game.units.len() as u32,
                None,
            )
            .unwrap();
            let reserves = m.prepare_new_unit_reserves(&draft.unit).unwrap();
            prepare_publication(
                PublicationParts {
                    units: &mut m.world.game.units,
                    grid: &mut m.grid,
                    controls: &mut m.commands.units,
                    recruitment: &mut m.recruit_metadata,
                    next_id: &mut m.next_unit_id,
                },
                &m.rules,
                2,
                vec![draft],
            )
            .unwrap()
            .publish_with(|u, _| {
                m.combat.initialize(u.id).hit_by = Some(0);
                m.firing.initialize(u.id);
                m.abilities.initialize(u.id);
                m.recovery.initialize(u.id);
                if let Some(r) = reserves.clone() {
                    m.logistics.insert(u.id, r);
                }
            });
            id
        };
        // These are the recorder's explicit initial inputs, never checkpoint outputs.
        let fields = &spec["fields"];
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.x = fields["x"].as_f64().unwrap();
        u.z = fields["z"].as_f64().unwrap();
        if let Some(still) = fields["still"].as_f64() {
            u.still = Some(still);
        }
        m.grid.update(id, u);
    }
    m.start.environment_seed = 123;
    m.start.wind = (0.0, 0.5);
    m.world.game.rain = 0.0;
    m.world.game.wet = Some(0.0);
    for p in &mut m.start.economy.players {
        p.mp = 500.0;
    }
    ww2_core::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    m
}

fn support_event(e: SupportEvent) -> Value {
    match e {
        SupportEvent::Chutes(p) => json!({"k":"chutes","x":p.x,"z":p.z,"pub":true}),
        SupportEvent::Bomb(p) => json!({"k":"bomb","x":p.x,"z":p.z,"pub":true}),
        SupportEvent::Shell(p) => json!({"k":"shell","x":p.x,"z":p.z,"pub":true}),
        SupportEvent::SmokeShells(p) => json!({"k":"smokeshells","x":p.x,"z":p.z,"pub":true}),
        SupportEvent::Plane {
            kind,
            at,
            direction,
            owner,
        } => json!({"k":kind.name(),"x":at.x,"z":at.z,"dir":direction,"fo":owner,"pub":true}),
        SupportEvent::Flak { unit, owner, at } => {
            json!({"f":unit,"fo":owner,"x":at.x,"z":at.z,"k":"flak","pub":true})
        }
        SupportEvent::ShotDown {
            kind,
            at,
            direction,
            fighters,
        } => {
            json!({"k":"shotdown","x":at.x,"z":at.z,"dir":direction,"kind":kind.name(),"by":if fighters {"fighters"} else {"flak"},"pub":true})
        }
        SupportEvent::Environment(e) => e.to_json(),
    }
}
fn aviation_event(e: AviationEvent) -> Value {
    match e {
        AviationEvent::AntiAir {
            shooter,
            target,
            owner,
            target_owner,
            at,
        } => {
            json!({"f":shooter,"t":target,"fo":owner,"to":target_owner,"x":at.x,"z":at.z,"k":"aa"})
        }
        AviationEvent::PlaneDown {
            target,
            owner,
            at,
            direction,
        } => {
            json!({"k":"planedown","t":target,"to":owner,"x":at.x,"z":at.z,"dir":direction,"kill":true})
        }
    }
}
fn state_name(state: PlaneState) -> &'static str {
    match state {
        PlaneState::Base => "base",
        PlaneState::Out => "out",
        PlaneState::Station => "station",
        PlaneState::Home => "home",
        PlaneState::Rearm => "rearm",
    }
}
fn initial_random_state(seed: u64) -> u64 {
    // Ask the actual purpose stream for its first transition, then undo that transition.
    let mut probe = MatchRandom::new(seed);
    probe.next_u64(StreamId::OffMap);
    probe.snapshot().streams[0]
        .1
        .wrapping_sub(0x9e37_79b9_7f4a_7c15)
}
fn projection(m: &Match, fog: &mut Fog, shots: &[Value]) -> Value {
    let units = m.world.game.units.values().map(|u| {
        let c = &m.commands.units[&u.id];
        let combat = m.combat.get(u.id).unwrap();
        let firing = m.firing.get(u.id).unwrap();
        // Missing birth owners must fail; this test does not invent their defaults.
        let ability = m.abilities.get(u.id).expect("published actor Ability row");
        let recovery = m.recovery.get(u.id).expect("published actor Recovery row");
        let a = m.aviation.planes.get(&u.id);
        let aa = m.aviation.anti_air.get(&u.id).copied().unwrap_or_default();
        assert!(c.orders.is_empty(), "this source component scene has no queued commands");
        assert!(u.motion_before.is_none() && u.clearance.is_none() && u.traffic.is_none() && u.lane_scan.is_none());
        assert!(combat.death_impulse.is_none());
        let rest = json!({"motionBefore":null,"motionClearance":null,"trafficTrail":u.traffic_trail.iter().map(|p|json!({"x":p.x,"z":p.z,"rot":p.rot})).collect::<Vec<_>>(),"trafficEntered":u.traffic_entered,"queuedBehind":u.queued_behind,"laneScan":null,"routeEnd":c.route_end.map(point),"pace":if u.pace.is_nan() { Value::Null } else { bits(u.pace) },"ghost":bits(u.ghost),"build":u.build,"guardHome":c.guard_home.map(point),"deathImpulse":null,"atHit":combat.at_hit.map(bits),"dust":u.dust.map(bits),"cells":u.cells,"supplyEscape":c.supply_escape,"supplyHold":c.supply_hold});
        let reserves = m.logistics.get(u.id).map(|r|json!({"ammo":r.ammo,"ammoMax":r.ammo_max,"ammoPrice":r.ammo_price,"ammoCredit":r.ammo_credit,"provisions":r.provisions,"fuel":r.fuel,"emergency":r.emergency,"warning":r.warning,"forced":r.forced,"stranded":r.stranded,"exhausted":r.exhausted,"supplyRate":r.supply_rate,"cutFor":r.cut_for}));
        json!({"id":u.id,"kind":m.rules.kinds[u.kind].name,"owner":u.owner,"x":bits(u.x),"z":bits(u.z),"rot":bits(u.rot),"aim":bits(c.aim),"moveSpeed":bits(u.move_speed),"vx":u.vx.map(bits),"vz":u.vz.map(bits),"travelDir":u.travel_dir.map(bits),"hp":bits(u.hp),"supp":bits(u.supp),"cooldown":bits(firing.cooldown),"still":u.still.map(bits),"retarget":bits(firing.retarget),"repath":bits(u.repath),"stuck":bits(u.stuck),"cd":bits(ability.cooldown),"buff":bits(ability.buff),"ap":ability.ap,"reinf":bits(recovery.reinforcement),"hpWas":recovery.hp_was.map(bits),"woundAt":recovery.wound_at.map(|v|bits(v as f64)),"healAt":recovery.heal_at.map(|v|bits(v as f64)),"xp":bits(combat.xp),"sprint":bits(u.sprint),"hitAt":bits(combat.hit_at),"aaShot":aa.shot.map(bits),"aaReserve":aa.reserve.map(bits),"path":u.path.iter().copied().map(point).collect::<Vec<_>>(),"orders":[],"attackId":c.attack_id,"targetId":firing.target_id,"nade":c.nade.map(point),"retreating":u.retreating,"dig":u.dig,"garrison":u.garrison.map_or(-1,|v|v as i64),"enter":c.enter.map_or(-1,|v|v as i64),"board":u.board,"amove":u.amove.map(point),"face":c.face.map(|f|json!({"a":f.angle,"x":f.at.x,"z":f.at.z})),"fireAt":c.fire_at.map_or(-1,|v|v as i64),"riding":combat.carrier.unwrap_or(0),"cargo":combat.cargo,"hitBy":combat.hit_by,"hitFrom":u.hit_from.map(point),"drift":u.drift,"reverse":u.reverse.map(point),"auto":c.auto_cast,"autoRetreat":c.auto_retreat,"holdFire":c.hold_fire,"holdPos":u.hold_pos,"follow":c.follow,"lastHit":combat.last_hit,"downed":a.is_some_and(|a|a.downed),"traffic":null,"trafficWait":bits(u.traffic_wait),"moveOutcome":c.outcome.map(|o|match o {MoveOutcome::Interrupted=>"interrupted",MoveOutcome::Moving=>"moving",MoveOutcome::Unreachable=>"unreachable",MoveOutcome::Arrived=>"arrived"}),"moveOutcomeTick":c.outcome_tick,"worldGoal":u.world_goal.map(point),"entrench":u.entrench,"built":u.built,"shotAt":u.shot_at.map(bits),"logistics":reserves,"air":a.map(|a|json!({"state":state_name(u.air.unwrap()),"fuel":bits(a.fuel),"ammo":bits(a.ammo),"timer":bits(a.timer),"ang":bits(a.orbit),"mission":a.mission.map(|p|json!({"kind":p.kind.name(),"id":p.target.unwrap_or(0),"x":bits(p.at.x),"z":bits(p.at.z)}))})),"sourceRest":rest})
    }).collect::<Vec<_>>();
    let random = m
        .start
        .random
        .snapshot()
        .streams
        .iter()
        .find(|(k, _)| *k == StreamId::OffMap)
        .map_or_else(|| initial_random_state(m.options.seed), |(_, v)| *v);
    assert!(
        m.firing.flights.is_empty()
            && m.firing.grenades.is_empty()
            && m.firing.salvos.is_empty()
            && m.firing.events.is_empty()
    );
    json!({"tick":m.world.game.tick,"units":units,"players":m.start.players.iter().enumerate().map(|(i,p)|json!({"slot":i,"team":p.config.team,"faction":p.config.faction as u8,"mp":bits(m.start.economy.players[i].mp),"vp":bits(m.start.economy.players[i].vp),"out":m.start.economy.players[i].out,"away":m.start.economy.players[i].away,"cooldowns":p.support.iter().map(|(k,v)|(k.clone(),bits(*v))).collect::<serde_json::Map<String,Value>>(),"visible":m.world.game.players[i].visible,"fog":fog.team(&m.world.game,&m.rules,p.config.team).vis})).collect::<Vec<_>>(),"story":m.story.iter().map(|s|json!({"supportCalls":bits(s.support_calls),"mpSpent":bits(s.mp_spent),"planesDowned":bits(s.planes_downed)})).collect::<Vec<_>>(),"sourceStory":m.story.iter().map(|s|json!({"kills":bits(s.kills),"losses":bits(s.losses),"built":bits(s.built),"captures":bits(s.captures),"mpSpent":bits(s.mp_spent),"supportCalls":bits(s.support_calls),"planesDowned":bits(s.planes_downed)})).collect::<Vec<_>>(),"points":m.start.points.iter().map(|p|json!({"x":p.at.x,"z":p.at.z,"vp":p.vp,"mp":p.mp,"owner":p.owner.map_or(-1,|v|v as i64),"capper":p.capper.map_or(-1,|v|v as i64),"progress":p.progress,"cut":p.cut,"kind":p.kind,"needs":p.needs.map_or(-1,|v|v as i64)})).collect::<Vec<_>>(),"strikes":m.world.game.strikes.iter().map(|s|{let r=s.runtime.as_ref().unwrap();json!({"kind":r.kind.name(),"owner":s.owner,"x":bits(s.x),"z":bits(s.z),"dir":bits(s.dir),"t":bits(r.delay),"left":bits(r.left),"next":bits(r.next),"live":s.live})}).collect::<Vec<_>>(),"covers":m.support.covers.iter().map(|c|json!({"team":c.team,"owner":c.owner,"x":bits(c.at.x),"z":bits(c.at.z),"r":bits(c.radius),"t":bits(c.remaining)})).collect::<Vec<_>>(),"shots":shots,"environment":[m.start.environment_seed,m.next_unit_id],"random":format!("{random:016x}"),"flights":[],"nades":[],"salvos":[],"smokes":m.world.game.smokes.iter().map(|s|json!({"id":s.id,"x":s.x,"z":s.z,"r":s.r,"t":s.remaining})).collect::<Vec<_>>()})
}

fn select(value: &Value, keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .map(|k| ((*k).to_owned(), value[*k].clone()))
            .collect(),
    )
}
fn expected_projection(source: &Value) -> Value {
    let mut value = select(
        source,
        &[
            "tick",
            "units",
            "players",
            "story",
            "sourceStory",
            "points",
            "strikes",
            "covers",
            "shots",
            "environment",
            "random",
            "flights",
            "nades",
            "salvos",
            "smokes",
        ],
    );
    value["sourceStory"] = json!(
        source["players"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, _)| {
                Value::Object(
                    [
                        "kills",
                        "losses",
                        "built",
                        "captures",
                        "mpSpent",
                        "supportCalls",
                        "planesDowned",
                    ]
                    .iter()
                    .map(|k| {
                        (
                            (*k).to_owned(),
                            bits(source["sourceStory"][i][*k].as_f64().unwrap_or(0.0)),
                        )
                    })
                    .collect(),
                )
            })
            .collect::<Vec<_>>()
    );
    value["points"] = json!(
        source["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| select(
                p,
                &[
                    "x", "z", "vp", "mp", "owner", "capper", "progress", "cut", "kind", "needs"
                ]
            ))
            .collect::<Vec<_>>()
    );
    for u in value["units"].as_array_mut().unwrap() {
        // These original systems are outside this consumer and must remain inactive.
        for key in [
            "moveResult",
            "garrisonEntry",
            "queue",
            "productionJobs",
            "prog",
            "rally",
            "structureId",
            "region",
            "gateT",
            "open",
            "convoy",
            "supplyStarted",
            "supplyRetry",
            "supplyRouteReason",
        ] {
            assert!(
                u["sourceRest"][key].is_null(),
                "unsupported source activity {key}"
            );
        }
        assert_eq!(
            u["react"],
            bits(0.0),
            "source reaction owner is not advanced here"
        );
        assert!(
            u["attackPick"].is_null(),
            "automatic aircraft weapon target is outside this consumer"
        );
        for key in ["react", "attackPick"] {
            u.as_object_mut().unwrap().remove(key);
        }
        // Rust represents these absent source policy switches as their false/zero defaults.
        for key in ["holdFire", "holdPos", "downed", "dig", "entrench"] {
            if u[key].is_null() {
                u[key] = json!(false);
            }
        }
        if u["follow"].is_null() {
            u["follow"] = json!(0);
        }
        if u["trafficWait"].is_null() {
            u["trafficWait"] = bits(0.0);
        }
        let mut rest = select(
            &u["sourceRest"],
            &[
                "motionBefore",
                "motionClearance",
                "trafficTrail",
                "trafficEntered",
                "queuedBehind",
                "laneScan",
                "routeEnd",
                "pace",
                "ghost",
                "build",
                "guardHome",
                "deathImpulse",
                "atHit",
                "dust",
                "cells",
                "supplyEscape",
                "supplyHold",
            ],
        );
        if rest["trafficTrail"].is_null() {
            rest["trafficTrail"] = json!([]);
        }
        for key in ["trafficEntered", "build", "supplyEscape", "supplyHold"] {
            if rest[key].is_null() {
                rest[key] = json!(false);
            }
        }
        if rest["queuedBehind"].is_null() {
            rest["queuedBehind"] = json!(0);
        }
        // The shared factory deliberately uses NaN for Source's undefined pace.
        // Preserve absence; a present Source pace still compares its exact floating bits.
        if !rest["pace"].is_null() {
            rest["pace"] = bits(rest["pace"].as_f64().unwrap());
        }
        rest["ghost"] = if rest["ghost"].is_null() {
            bits(0.0)
        } else {
            bits(rest["ghost"].as_f64().unwrap())
        };
        u["sourceRest"] = rest;
    }
    value
}

#[test]
fn support_and_on_map_aircraft_replay_original_owned_phase_boundaries() {
    let f = fixture();
    assert_eq!(f["traces"].as_array().unwrap().len(), 4);
    let mut count = 0;
    let mut clocks = 0;
    let mut owned_calls = 0;
    for trace in f["traces"].as_array().unwrap() {
        let mut m = scene(trace);
        let mut fog = Fog::default();
        let mut shots = Vec::new();
        let mut captured = m.world.game.units.keys().copied().collect::<Vec<_>>();
        assert_eq!(captured.len(), trace["scene"].as_array().unwrap().len());
        compare(
            &projection(&m, &mut fog, &shots),
            &expected_projection(&trace["initial"]),
            trace["name"].as_str().unwrap(),
        );
        for (index, frame) in trace["frames"].as_array().unwrap().iter().enumerate() {
            let action = &frame["action"];
            let tick = action["tick"].as_i64().unwrap();
            if tick != m.world.game.tick {
                assert_eq!(tick, m.world.game.tick + 1);
                captured = m.world.game.units.keys().copied().collect();
                let guard_before = projection(&m, &mut fog, &shots);
                m.validate_support_tick_phases(&captured, 0.05, tick as u64)
                    .unwrap();
                clocks += 1;
                assert_eq!(
                    projection(&m, &mut fog, &shots),
                    guard_before,
                    "pure before-clock support admission"
                );
                m.world.game.tick = tick;
                shots.clear();
                assert!(m.support.events.is_empty() && m.aviation.events.is_empty());
            }
            let path = format!(
                "{} frame {index} {}",
                trace["name"].as_str().unwrap(),
                action["op"].as_str().unwrap()
            );
            compare(
                &projection(&m, &mut fog, &shots),
                &expected_projection(&frame["before"]),
                &format!("{path} before"),
            );
            let actors = action["actors"]
                .as_array()
                .map(|ids| {
                    ids.iter()
                        .map(|v| v.as_u64().unwrap() as u32)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let denial = match action["op"].as_str().unwrap() {
                "command" => m
                    .execute_command(
                        action["owner"].as_u64().unwrap() as usize,
                        &Command::from_json(&action["command"]).unwrap(),
                        false,
                    )
                    .unwrap()
                    .map(|d| d.name()),
                "vision" => {
                    assert_eq!(tick % 4, 1);
                    ww2_core::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
                    None
                }
                "plane" => {
                    m.step_aircraft(action["id"].as_u64().unwrap() as u32, 0.05)
                        .unwrap();
                    None
                }
                "strikes" => {
                    assert_eq!(
                        actors, captured,
                        "{path}: captured native source membership"
                    );
                    m.step_support_strikes(&actors, 0.05).unwrap();
                    None
                }
                "aa" => {
                    assert_eq!(actors, captured, "{path}: retained pre-birth AA membership");
                    m.step_aircraft_anti_air(&actors, 0.05).unwrap();
                    None
                }
                "timers" => {
                    m.advance_support_timers(0.05).unwrap();
                    None
                }
                _ => panic!("unknown source call"),
            };
            if matches!(
                action["op"].as_str().unwrap(),
                "plane" | "strikes" | "aa" | "timers"
            ) {
                owned_calls += 1;
            }
            assert_eq!(json!(denial), frame["denial"], "{path} admission");
            shots.extend(m.support.take_events().into_iter().map(support_event));
            shots.extend(m.aviation.take_events().into_iter().map(aviation_event));
            compare(
                &projection(&m, &mut fog, &shots),
                &expected_projection(&frame["after"]),
                &format!("{path} after"),
            );
            count += 1;
        }
    }
    assert_eq!(count, 1603);
    assert_eq!(clocks, 432);
    assert_eq!(owned_calls, 1484);
}
