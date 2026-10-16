use super::*;
use crate::{
    economy::{Faction, Mode, Technology},
    economy_state::ArmySize,
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_commands::UnitControl,
    match_runtime::{MatchOptions, PlayerConfig},
    unit_spawn, vision,
    weather::WeatherChoice,
};
use serde_json::{Value, json};

mod privacy_matrix;

fn point(value: &Value) -> Point {
    Point {
        x: value["x"].as_f64().unwrap(),
        z: value["z"].as_f64().unwrap(),
    }
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/human-observation.json")).unwrap()
}
fn instance(case: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 841,
            mode: if case["mode"] == "default" {
                Mode::Default
            } else {
                Mode::Conquest
            },
            players: [0, 1, 0]
                .into_iter()
                .enumerate()
                .map(|(i, team)| PlayerConfig {
                    name: ["A", "B", "Ally"][i].into(),
                    team,
                    faction: Faction::try_from(i as u8).unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: None,
            living_weather: false,
            map_key: None,
            tech: case["tech"].as_bool().unwrap(),
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    // Import the recorded component's explicit canonical scene through the real initializer.
    m.world.game.units.clear();
    m.commands.units.clear();
    m.firing.units.clear();
    for spec in case["baseUnits"].as_array().unwrap() {
        let id = spec["id"].as_u64().unwrap() as u32;
        let kind = m
            .rules
            .kinds
            .iter()
            .position(|k| k.name == spec["type"].as_str().unwrap())
            .unwrap();
        let u = unit_spawn::initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            kind,
            id,
            spec["owner"].as_u64().unwrap() as usize,
            point(spec),
        )
        .unwrap();
        let control = UnitControl::new(&u, &m.rules).unwrap();
        m.commands.units.insert(id, control);
        m.firing.initialize(id);
        m.combat.initialize(id);
        m.abilities.initialize(id);
        m.recovery.initialize(id);
        m.world.game.units.insert(id, u);
    }
    m.next_unit_id = 8;
    m.grid = SpatialGrid::new(&m.world.game.units, 16.0);
    m
}
fn action(m: &mut Match, value: &Value) {
    for patch in value["units"].as_array().into_iter().flatten() {
        let id = patch["id"].as_u64().unwrap() as u32;
        let u = m.world.game.units.get_mut(&id).unwrap();
        for (name, v) in patch.as_object().unwrap() {
            match name.as_str() {
                "id" => {}
                "x" => u.x = v.as_f64().unwrap(),
                "z" => u.z = v.as_f64().unwrap(),
                "hp" => u.hp = v.as_f64().unwrap(),
                "path" => u.path = v.as_array().unwrap().iter().map(point).collect(),
                "worldGoal" => u.world_goal = (!v.is_null()).then(|| point(v)),
                "amove" => u.amove = (!v.is_null()).then(|| point(v)),
                "board" => u.board = v.as_u64().unwrap() as u32,
                "attackId" => {
                    m.commands.units.get_mut(&id).unwrap().attack_id = v.as_u64().unwrap() as u32
                }
                "targetId" => {
                    m.firing.units.get_mut(&id).unwrap().target_id = v.as_u64().unwrap() as u32
                }
                "nade" => {
                    m.commands.units.get_mut(&id).unwrap().nade = (!v.is_null()).then(|| point(v))
                }
                _ => panic!("unknown recorded input {name}"),
            }
        }
    }
    if let Some(mp) = value["mp"].as_f64() {
        m.start.economy.players[0].mp = mp;
    }
    if value["knownMine"] == true {
        m.world.mine_seen.insert(15 * m.world.game.w + 15, 1);
    }
    m.grid.rebuild(&m.world.game.units);
    let tick = value["tick"].as_i64().unwrap();
    if tick > 0 {
        m.world.game.tick = tick - 1 - (tick - 2) % 4;
        vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    }
    m.world.game.tick = tick;
    if value["finish"] == true {
        m.start.economy.players[0].vp = m.start.win_vp;
        m.apply_conquest_victory().unwrap();
    }
}
fn runs(values: &[u16]) -> Vec<[u16; 2]> {
    let mut out: Vec<[u16; 2]> = Vec::new();
    for &v in values {
        if let Some(last) = out.last_mut().filter(|r| r[0] == v) {
            last[1] += 1;
        } else {
            out.push([v, 1]);
        }
    }
    out
}
fn projection(
    view: &HumanView<Observation>,
    memory: &HumanObservationMemory,
    expected_screen: &Value,
) -> Value {
    let input = view.packet.input();
    let wanted: Vec<u32> = expected_screen["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_u64().unwrap() as u32)
        .collect();
    let sighted: Vec<u32> = expected_screen["sightings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_u64().unwrap() as u32)
        .collect();
    let screen = ScreenView {
        tick: view.tick,
        spawn: view.spawn,
        units: view
            .units
            .iter()
            .filter(|u| wanted.contains(&u.motion.id))
            .collect(),
        visible: expected_screen["visible"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap() as u32)
            .collect(),
        sightings: view
            .sightings
            .iter()
            .filter(|s| sighted.contains(&s.id))
            .collect(),
        packet: &view.packet,
    };
    let filtered = human_screen_observation(&screen).unwrap();
    let units: Vec<_> = view.units.iter().zip(&input.units).map(|(human, u)| json!({
        "id":human.motion.id,"owner":human.owner,"type":human.kind,"x":human.motion.at.x,"z":human.motion.at.z,
        "hp":human.hp,"worldGoal":human.motion.world_goal.map(|p|json!({"x":p.x,"z":p.z})),
        "amove":human.motion.amove.map(|p|json!({"x":p.x,"z":p.z})),"target":human.motion.target_id,
        "retreating":human.motion.retreating,"air":human.air,"build":human.build,"dig":human.dig,
        "entrench":human.entrench,"garrison":human.garrison,"supp":u.supp,"cover":u.cover,
        "path":u.path.iter().map(|p|json!({"x":p.x,"z":p.z})).collect::<Vec<_>>(),
        "attack":u.attack_id,"enter":u.enter,"fire":u.fire_at,"nade":u.nade
    })).collect();
    let research = input.research.as_ref().map(|r| json!({"faction":r.faction as u8,
        "tech":[r.tech.tier(),r.tech.level(Technology::Infantry),r.tech.level(Technology::Guns),r.tech.level(Technology::Armor)],
        "busy":r.busy.iter().map(|t| match t { Technology::Tier=>"tier",Technology::Infantry=>"inf",Technology::Guns=>"guns",Technology::Armor=>"armor" }).collect::<Vec<_>>(),
        "standard":r.standard_army,"stock":r.stock_base,"armory":r.armory_ready }));
    json!({"tick":view.tick,"spawn":view.spawn.map(|p|json!({"x":p.x,"z":p.z})),"units":units,
        "visible":view.visible,"sightings":view.sightings.iter().map(|s|&s.payload).collect::<Vec<_>>(),
        "still":memory.still.iter().map(|s|json!({"id":s.id,"x":s.at.x,"z":s.at.z,"since":s.since})).collect::<Vec<_>>(),
        "screen":{"ids":filtered.input().units.iter().map(|u|u.id).collect::<Vec<_>>(),"visible":screen.visible,
            "sightings":screen.sightings.iter().map(|s|s.id).collect::<Vec<_>>()},
        "packet":{"w":input.width,"h":input.height,"tick":input.tick,"mp":input.manpower,
            "players":input.players.iter().map(|p|json!([p.team,p.out,{"x":p.spawn.x,"z":p.spawn.z}])).collect::<Vec<_>>(),
            "points":input.points.iter().map(|p|json!([p.at.x,p.at.z,p.vp,p.mp,p.owner.map_or(-1,|o|o as i64),p.cut,p.locked])).collect::<Vec<_>>(),
            "chars":String::from_utf8(input.chars.clone()).unwrap(),"flags":runs(&input.flags),"research":research}})
}
fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64().unwrap() == b.as_f64().unwrap(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| equal(a, b)))
        }
        _ => a == b,
    }
}
#[test]
fn original_observations_preserve_own_goals_visibility_sightings_and_complete_packet() {
    let f = fixture();
    assert_eq!(f["cases"].as_array().unwrap().len(), 4);
    for case in f["cases"].as_array().unwrap() {
        let mut m = instance(case);
        let mut memory = HumanObservationMemory::new(0);
        for frame in case["frames"].as_array().unwrap() {
            action(&mut m, &frame["action"]);
            let view = m.human_commander_observation(&mut memory).unwrap();
            let actual = projection(&view, &memory, &frame["expected"]["screen"]);
            assert!(
                equal(&actual, &frame["expected"]),
                "{} tick{}\nactual{actual}\nexpected{}",
                case["name"],
                view.tick,
                frame["expected"]
            );
        }
    }
}
#[test]
fn owner_hurt_projection_preserves_reset_append_and_raw_ordinals() {
    let f = fixture();
    let m = instance(&f["cases"][0]);
    for frame in f["hurtFrames"].as_array().unwrap() {
        let shots: Vec<_> = frame["shots"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                if e["k"] == "hurt" {
                    HumanShot::Hurt {
                        owner: e["to"].as_u64().unwrap() as usize,
                        unit: e["t"].as_u64().unwrap() as u32,
                        at: point(e),
                    }
                } else {
                    HumanShot::Other
                }
            })
            .collect();
        let batch = m
            .human_commander_hurts(0, frame["generation"].as_u64().unwrap(), &shots)
            .unwrap();
        let actual: Vec<_> = batch
            .events
            .iter()
            .map(|h| json!({"ordinal":h.ordinal,"id":h.id,"at":{"x":h.at.x,"z":h.at.z}}))
            .collect();
        assert!(equal(&json!(actual), &frame["expected"]));
        assert_eq!(batch.through_ordinal, shots.len() as u64);
        assert_eq!(batch.generation, frame["generation"].as_u64().unwrap());
    }
    assert!(
        m.human_commander_hurts(
            0,
            41,
            &[HumanShot::Hurt {
                owner: 0,
                unit: 1,
                at: Point { x: 1., z: f64::NAN }
            }]
        )
        .is_err()
    );
    assert!(
        m.human_commander_hurts(
            0,
            41,
            &[HumanShot::Hurt {
                owner: 1,
                unit: 1,
                at: Point { x: 1., z: f64::NAN }
            }]
        )
        .unwrap()
        .events
        .is_empty()
    );
}

#[test]
fn original_routes_keep_prewarmed_history_across_human_observations() {
    let f = fixture();
    for case in f["cacheCases"].as_array().unwrap() {
        let mut m = instance(case);
        let unit = m.world.game.units[&2].clone();
        let peer = m.world.game.units[&3].clone();
        let goal = point(&case["goal"]);
        let peer_goal = point(&case["peerGoal"]);
        let route = |m: &mut Match, owner, actor: &crate::game::Unit, at| {
            let result = m.command_route(owner, actor, at).unwrap();
            json!(
                result
                    .waypoints
                    .iter()
                    .map(|p| [
                        format!("{:016x}", p.x.to_bits()),
                        format!("{:016x}", p.z.to_bits())
                    ])
                    .collect::<Vec<_>>()
            )
        };
        for read in case["peerReads"].as_array().unwrap() {
            if !read["change"].is_null() {
                m.world
                    .set_cell(
                        read["change"]["cell"].as_u64().unwrap() as usize,
                        read["change"]["ch"].as_str().unwrap().as_bytes()[0],
                    )
                    .unwrap();
            }
            assert_eq!(
                route(&mut m, 1, &peer, peer_goal),
                read["path"],
                "warm peer {}",
                case["mode"]
            );
        }
        let mut initial_version = None;
        for read in case["reads"].as_array().unwrap() {
            if !read["change"].is_null() {
                m.world
                    .set_cell(
                        read["change"]["cell"].as_u64().unwrap() as usize,
                        read["change"]["ch"].as_str().unwrap().as_bytes()[0],
                    )
                    .unwrap();
            }
            assert_eq!(
                route(&mut m, 0, &unit, goal),
                read["path"],
                "warm {}",
                case["mode"]
            );
            initial_version.get_or_insert(
                m.recipient_routing(0)
                    .unwrap()
                    .query()
                    .navigation()
                    .versions
                    .knowledge,
            );
        }
        assert!(
            m.recipient_routing(0)
                .unwrap()
                .query()
                .navigation()
                .versions
                .knowledge
                > initial_version.unwrap(),
            "accepted edits must exercise incremental knowledge history"
        );
        let mut memory = HumanObservationMemory::new(0);
        for expected in case["after"].as_array().unwrap() {
            m.human_commander_observation(&mut memory).unwrap();
            assert_eq!(
                route(&mut m, 0, &unit, goal),
                expected["path"],
                "after observation {}",
                case["mode"]
            );
            assert_eq!(
                route(&mut m, 1, &peer, peer_goal),
                expected["peerPath"],
                "peer after observation {}",
                case["mode"]
            );
        }
        let prior = {
            let query = m.recipient_routing(0).unwrap().query();
            let buffers = query.navigation();
            (buffers.flags.as_ptr(), buffers.heights.unwrap().as_ptr())
        };
        let memory_before = memory.clone();
        m.world.game.units.get_mut(&2).unwrap().amove = Some(Point {
            x: f64::NAN,
            z: 10.,
        });
        assert!(m.human_commander_observation(&mut memory).is_err());
        let after = {
            let query = m.recipient_routing(0).unwrap().query();
            let buffers = query.navigation();
            (buffers.flags.as_ptr(), buffers.heights.unwrap().as_ptr())
        };
        assert_eq!(after, prior);
        assert_eq!(memory, memory_before);
        m.world.game.units.get_mut(&2).unwrap().amove = None;
        let expected = case["after"].as_array().unwrap().last().unwrap();
        assert_eq!(route(&mut m, 0, &unit, goal), expected["path"]);
        assert_eq!(route(&mut m, 1, &peer, peer_goal), expected["peerPath"]);
    }
}

fn canonical_projection(m: &Match) -> Value {
    let routing = &m.recipients[0];
    let remembered: Vec<_> = routing
        .remembered()
        .iter()
        .map(|r| json!([r.row.cell, format!("{:?}", r.row)]))
        .collect();
    json!({"rows":remembered,"routes":format!("{:?}",routing.query().navigation()),
        "pending":m.world.pending(crate::world::Recipient::Player(0)),"journal":m.world.journal().iter().map(|r|format!("{r:?}")).collect::<Vec<_>>(),
        "fog":format!("{:?}",m.routing_fog),"queue":format!("{:?}",m.commands.queue.snapshot()),"random":format!("{:?}",m.start.random.snapshot()),
        "wallet":m.start.economy.players[0].mp,"units":format!("{:?}",m.world.game.units)})
}
#[test]
fn detached_views_hide_private_facts_and_refusal_preserves_canonical_observation() {
    let f = fixture();
    let case = &f["cases"][0];
    let mut a = instance(case);
    let mut b = instance(case);
    b.start.economy.players[1].mp = 9999.;
    for id in [5, 6, 7] {
        let u = b.world.game.units.get_mut(&id).unwrap();
        u.hp = 1.;
        u.world_goal = Some(Point { x: 50., z: 50. });
        u.path = vec![Point { x: 60., z: 60. }];
        let c = b.commands.units.get_mut(&id).unwrap();
        c.attack_id = 1;
        c.nade = Some(Point { x: 30., z: 30. });
    }
    b.world.set_cell(47 * 48 + 44, b'O').unwrap();
    let mut ma = HumanObservationMemory::new(0);
    let mut mb = HumanObservationMemory::new(0);
    let va = a.human_commander_observation(&mut ma).unwrap();
    let vb = b.human_commander_observation(&mut mb).unwrap();
    assert_eq!(va, vb);
    let mut bad = va.units[0].clone();
    bad.hp -= 1.;
    let forged = ScreenView {
        tick: va.tick,
        spawn: va.spawn,
        units: vec![&bad],
        visible: vec![],
        sightings: vec![],
        packet: &va.packet,
    };
    assert!(
        human_screen_observation(&forged).is_err(),
        "filtered rows cannot replace delivered facts"
    );
    let saved = va.clone();
    a.world.game.units.get_mut(&1).unwrap().hp = 1.;
    a.commands.units.get_mut(&1).unwrap().nade = Some(Point { x: 40., z: 30. });
    assert_eq!(va, saved, "delivered view owns all values");
    action(&mut b, &json!({"tick":2}));
    b.world.set_cell(10 * 48 + 10, b'O').unwrap();
    b.world.game.units.get_mut(&1).unwrap().amove = Some(Point {
        x: f64::NAN,
        z: 40.,
    });
    let before = canonical_projection(&b);
    let prior = mb.clone();
    assert!(b.human_commander_observation(&mut mb).is_err());
    assert_eq!(canonical_projection(&b), before);
    assert_eq!(mb, prior);
    b.world.game.units.get_mut(&1).unwrap().amove = None;
    let paid = b
        .field_works
        .prepare_start(
            &b.world,
            &mut b.start.economy.players[0],
            crate::field_works::StartRequest {
                control_eligible: true,
                owner: 0,
                unit: 3,
                kind: crate::field_works::FortKind::Trench,
                at: Point { x: 25., z: 25. },
                direction: Some(0.),
            },
            |_| true,
        )
        .unwrap();
    paid.commit(&mut b.field_works);
    assert!(!b.world.game.units[&3].dig);
    let paid_before = canonical_projection(&b);
    assert!(
        b.human_commander_observation(&mut mb).is_err(),
        "reverse paid descriptor parity must fail before publication"
    );
    assert_eq!(canonical_projection(&b), paid_before);
    assert_eq!(mb, prior);
    b.field_works.drop_paid_job(3);
    let repaired = canonical_projection(&b);
    b.options.mode = Mode::World;
    assert!(b.human_commander_observation(&mut mb).is_err());
    assert_eq!(canonical_projection(&b), repaired);
    assert_eq!(mb, prior);
    b.options.mode = Mode::Default;
    assert!(
        b.human_commander_observation(&mut ma).is_err(),
        "another Match cannot reuse seat memory"
    );
    b.start.economy.players[0].out = true;
    let control = b.human_commander_control(0, "hard").unwrap();
    assert!(control.live_owner_out);
    assert_eq!(control.now, 2);
    assert!(!control.winner);
}

#[test]
fn intent_ai08_delivered_sightings_expire_and_stillness_restarts() {
    let source = fixture();
    let case = &source["cases"][0];
    let mut state = instance(case);
    let mut memory = HumanObservationMemory::new(0);
    let mut saw_expiry = false;
    let mut saw_lost_stillness = false;
    let mut saw_motion_reset = false;
    let mut saw_return = false;
    for frame in case["frames"].as_array().unwrap() {
        let previous = memory.clone();
        action(&mut state, &frame["action"]);
        let view = state.human_commander_observation(&mut memory).unwrap();
        let now = view.tick as f64 / 20.0;
        for sighting in &memory.sightings {
            assert!(now - sighting.time <= 60.0);
            if let Some(unit) = view.units.iter().find(|unit| unit.motion.id == sighting.id) {
                let definition = Catalog::compiled().unwrap().unit(&unit.kind).unwrap();
                let n = |key| definition.get(key).and_then(RuleValue::number).unwrap();
                assert_eq!(
                    sighting.worth,
                    n("cost") * unit.hp / (n("models") * n("hpPer"))
                );
                assert_eq!(sighting.at, unit.motion.at);
                assert_eq!(sighting.time, now);
            }
        }
        for old in &previous.sightings {
            if now - old.time > 60.0 && !view.visible.contains(&old.id) {
                assert!(!memory.sightings.iter().any(|current| current.id == old.id));
                saw_expiry = true;
            }
        }
        for old in &previous.still {
            if !view.visible.contains(&old.id) {
                assert!(!memory.still.iter().any(|current| current.id == old.id));
                saw_lost_stillness = true;
            } else if let Some(current) = memory.still.iter().find(|current| current.id == old.id)
                && math::hypot(old.at.x - current.at.x, old.at.z - current.at.z) > 0.1 + 1e-9
            {
                assert_eq!(current.since, now);
                saw_motion_reset = true;
            }
        }
        for current in &memory.still {
            if !previous.still.iter().any(|old| old.id == current.id)
                && previous.sightings.iter().any(|old| old.id == current.id)
            {
                assert_eq!(current.since, now);
                saw_return = true;
            }
        }
    }
    assert!(saw_expiry && saw_lost_stillness && saw_motion_reset && saw_return);
}
