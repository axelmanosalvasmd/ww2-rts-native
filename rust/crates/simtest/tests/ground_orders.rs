//! Original ground-order inputs replayed through canonical Match owners.
//!
//! Component observations execute actual command prefixes before the first source
//! step. Separate fresh scenes enqueue exact OrderKeys and call one advance_tick
//! per full source step, in forward and reverse arrival order. Oracle patches only
//! reconstruct read-only JSON. Constructor RNG, wallets and startup actors remain
//! native; declared scene primes are the sole setup overrides.
//!
//! Two mixed rail/partial-reserve scenes lack represented factory metadata and
//! remain explicit exclusions. Garrison Retreat uses the routing owner's actual
//! prepared existing exits, including the warm-cache group with repeated IDs.
//! Dead-target retirement with allowEffects contributes its command prefix but is
//! excluded from exact quiet full-step parity. AIR Needs uses actual spawn_aircraft
//! and its canonical sortie metadata.
//!
//! moveResult retention, auxiliary motionClearance/lane objects, full terrain
//! journals/versions, detailed capture-point state and private scheduler totals
//! are outside the projection. Source pathViewKey remains read-only evidence; no
//! native private-cache key is synthesized. Deferred/dropped public counters,
//! visible IDs, physical groundFlags, ordered paths/queues, paid work/project membership,
//! timers, wallets and story are compared. No complete damage, ground-effects,
//! rail, naval or operative tick result is claimed. Full-step checks require the
//! sole runner's planned GroundAttack receipt and fail while it is absent.
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, Denial, MoveOutcome},
    match_end::MatchSample,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::{TickPathConfig, TickPhase},
    orders::OrderKey,
    random::RandomSnapshot,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};

fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn number(v: &Value) -> f64 {
    let text = v
        .as_str()
        .or_else(|| v["f64"].as_str())
        .expect("source IEEE 754 value");
    f64::from_bits(u64::from_str_radix(text, 16).unwrap())
}
fn point(p: Point) -> Value {
    json!({"x":bits(p.x),"z":bits(p.z)})
}
fn tagged(v: Value) -> Value {
    match v {
        Value::Number(n) => json!({"f64":bits(n.as_f64().unwrap())}),
        Value::Array(a) => Value::Array(a.into_iter().map(tagged).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, tagged(v))).collect()),
        v => v,
    }
}
fn tagged_point(p: Option<Point>) -> Value {
    p.map_or(Value::Null, |p| tagged(json!({"x":p.x,"z":p.z})))
}
fn fixture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(include_bytes!("fixtures/ground-orders.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    let f: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(f["cases"].as_array().unwrap().len(), 28);
    assert_eq!(f["commands"], 48);
    assert_eq!(f["steps"], 30);
    assert_eq!(
        f["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["snapshots"].as_u64().unwrap())
            .sum::<u64>(),
        110
    );
    f
}
fn actor(c: &Value, value: &Value) -> u32 {
    if let Some(name) = value.as_str() {
        c["setup"]["aliases"][name]
            .as_u64()
            .map(|id| u32::try_from(id).unwrap())
            .unwrap_or_else(|| name.parse().expect("declared alias or startup actor ID"))
    } else {
        u32::try_from(value.as_u64().expect("declared actor ID")).unwrap()
    }
}
fn refresh(m: &mut Match, vision: bool) {
    m.grid.rebuild(&m.world.game.units);
    if vision {
        ww2_core::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
        for owner in 0..m.world.game.players.len() {
            m.refresh_recipient_routing(owner).unwrap();
        }
    }
}
fn prime_units(m: &mut Match, c: &Value, rows: &Value) {
    for (name, fields) in rows.as_object().expect("declared prime unit fields") {
        let id = actor(c, &json!(name));
        assert!(
            m.world.game.units.contains_key(&id),
            "prime actor must already exist"
        );
        for (key, v) in fields.as_object().unwrap() {
            match key.as_str() {
                "hp" => m.world.game.units.get_mut(&id).unwrap().hp = v.as_f64().unwrap(),
                "built" => {
                    m.world.game.units.get_mut(&id).unwrap().built = Some(v.as_f64().unwrap())
                }
                "garrison" => {
                    m.world.game.units.get_mut(&id).unwrap().garrison =
                        Some(usize::try_from(v.as_u64().unwrap()).unwrap())
                }
                "follow" => m.commands.units.get_mut(&id).unwrap().follow = actor(c, v),
                "x" => m.world.game.units.get_mut(&id).unwrap().x = v.as_f64().unwrap(),
                "z" => m.world.game.units.get_mut(&id).unwrap().z = v.as_f64().unwrap(),
                "worldGoal" => {
                    assert!(
                        v.is_null(),
                        "only declared null worldGoal prime is represented"
                    );
                    m.world.game.units.get_mut(&id).unwrap().world_goal = None;
                }
                "path" => {
                    assert!(
                        v.as_array().unwrap().is_empty(),
                        "only declared empty path prime is represented"
                    );
                    let u = m.world.game.units.get_mut(&id).unwrap();
                    u.path.clear();
                    u.path_gen = u.path_gen.checked_add(1).unwrap();
                }
                "air" => {
                    // Declared source setup after an actual aircraft factory birth.
                    assert_eq!(v.as_object().unwrap().len(), 5);
                    assert_eq!(v["state"], "out");
                    assert!(v["mission"].is_null());
                    let u = m.world.game.units.get_mut(&id).unwrap();
                    assert!(u.air.is_some());
                    u.air = Some(ww2_core::game::PlaneState::Out);
                    let air = m
                        .aviation
                        .planes
                        .get_mut(&id)
                        .expect("canonical aircraft factory row");
                    air.fuel = v["fuel"].as_f64().unwrap();
                    air.ammo = v["ammo"].as_f64().unwrap();
                    air.timer = v["timer"].as_f64().unwrap();
                    air.mission = None;
                }
                other => panic!("undeclared/unrepresented ground-order prime {name}/{other}"),
            }
        }
    }
}
fn make(c: &Value) -> Match {
    let s = &c["setup"];
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&s["map"]).unwrap()).unwrap();
    let players = s["names"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(owner, name)| PlayerConfig {
            name: name.as_str().unwrap().into(),
            team: s["teams"][owner].as_i64().unwrap() as i32,
            faction: Faction::try_from(s["factions"][owner].as_u64().unwrap() as u8).unwrap(),
        })
        .collect();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Default,
            players,
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
    assert_eq!(
        json!(m.world.game.units.keys().copied().collect::<Vec<_>>()),
        s["initialActors"],
        "all source constructor actors retained"
    );
    assert_eq!(
        m.next_unit_id,
        s["creation"]["nextId"].as_u64().unwrap() as u32
    );
    for spec in s["spawns"].as_array().unwrap() {
        let id = m.next_unit_id;
        assert_eq!(id, actor(c, &spec["alias"]));
        assert!(!m.world.game.units.contains_key(&id) && !m.commands.units.contains_key(&id));
        assert!(
            m.combat.get(id).is_none()
                && m.firing.get(id).is_none()
                && m.abilities.get(id).is_none()
                && m.recovery.get(id).is_none()
        );
        let name = spec["type"].as_str().unwrap();
        let kind = m.rules.kind(name).unwrap();
        let owner = spec["owner"].as_u64().unwrap() as usize;
        let at = Point {
            x: spec["pose"]["x"].as_f64().unwrap(),
            z: spec["pose"]["z"].as_f64().unwrap(),
        };
        assert_eq!(
            spec["pose"].as_object().unwrap().len(),
            2,
            "declared birth pose schema"
        );
        if m.rules.kinds[kind].air {
            assert_eq!(m.spawn_aircraft(owner, name).unwrap(), id);
            let unit = m.world.game.units.get_mut(&id).unwrap();
            unit.x = at.x;
            unit.z = at.z;
            assert!(m.aviation.planes.contains_key(&id));
        } else {
            assert!(
                !m.rules.kinds[kind].rail,
                "rail factory metadata is excluded explicitly"
            );
            let staged = StagedUnit::ordinary(
                &m.world.game,
                &m.rules,
                owner,
                kind,
                id,
                u32::try_from(m.world.game.units.len()).unwrap(),
                m.start.players[owner].spawn,
                Some(at),
            )
            .unwrap();
            prepare_publication(
                PublicationParts {
                    units: &mut m.world.game.units,
                    grid: &mut m.grid,
                    controls: &mut m.commands.units,
                    recruitment: &mut m.recruit_metadata,
                    next_id: &mut m.next_unit_id,
                },
                &m.rules,
                m.start.players.len(),
                vec![staged],
            )
            .unwrap()
            .publish();
            m.combat.initialize(id).hit_by = Some(0);
            m.firing.initialize(id);
            m.abilities.initialize(id);
            m.recovery.initialize(id);
        }
        assert_eq!(m.next_unit_id, id.checked_add(1).unwrap());
    }
    let quiet = &s["prime"]["allUnits"];
    assert_eq!(
        *quiet,
        json!({"holdFire":true,"holdPos":true,"auto":false,"autoRetreat":false})
    );
    for u in m.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    prime_units(&mut m, c, &s["prime"]["units"]);
    refresh(&mut m, true);
    m.initialize_tick_scene(
        0,
        TickPathConfig {
            budget: s["prime"]["pathBudget"].as_u64().unwrap() as usize,
            retry_limit: s["prime"]["pathRetryLimit"].as_u64().unwrap() as u32,
        },
    )
    .unwrap();
    m
}
// This is an oracle-only JSON update. It cannot borrow or change Native Match.
fn oracle_patch(state: &mut Value, patch: &Value) {
    for change in patch.as_array().unwrap() {
        let path = change[0].as_array().unwrap();
        if path.is_empty() {
            *state = change[1].clone();
            continue;
        }
        let mut at = &mut *state;
        for key in &path[..path.len() - 1] {
            at = if let Some(i) = key.as_u64() {
                &mut at[i as usize]
            } else {
                &mut at[key.as_str().unwrap()]
            };
        }
        let key = path.last().unwrap();
        if let Some(i) = key.as_u64() {
            at[i as usize] = change[1].clone();
        } else if change.as_array().unwrap().len() == 1 {
            at.as_object_mut().unwrap().remove(key.as_str().unwrap());
        } else {
            at[key.as_str().unwrap()] = change[1].clone();
        }
    }
}
fn waiting(c: &Command) -> Value {
    match c {
        Command::Attack {
            ids,
            target,
            queue,
            at: Some(at),
        } => {
            assert!(!queue, "normalized waiting Attack queue=false");
            assert_eq!(ids.len(), 1, "singleton waiting Attack");
            tagged(json!({"t":"attack", "ids":ids, "target":target, "x":at.x, "z":at.z}))
        }
        other => panic!("unrepresented waiting ground order {other:?}"),
    }
}
fn paid(m: &Match, id: u32) -> Value {
    let Some(job) = m.field_works.jobs().get(&id) else {
        return Value::Null;
    };
    let mut row = tagged(json!({"x":job.at.x, "z":job.at.z, "dir":job.direction,
        "t":job.elapsed, "kind":job.kind.as_str(), "reach":job.reach, "project":job.project,
        "cells":job.cells.iter().map(|c| json!([c.cell, char::from(c.ch).to_string()])).collect::<Vec<_>>() }));
    // Source adds n lazily and front only for shared pattern jobs.
    if let Some(n) = job.original_cells {
        row["n"] = tagged(json!(n));
    }
    if let Some(front) = job.front {
        row["front"] = tagged(json!(front));
    }
    assert!(job.from.is_none(), "unrepresented distant paid-work origin");
    row
}
fn projects(m: &Match) -> Value {
    tagged(json!(m.field_works.projects().map(|p| json!([p.id, {
        "id":p.id, "owner":p.owner, "fort":p.fort.as_str(), "crew":p.crew, "active":p.active,
        "jobs":p.jobs.iter().map(|j| json!({"kind":j.kind.as_str(),"x":j.at.x,"z":j.at.z,
            "dir":j.direction,"front":j.front})).collect::<Vec<_>>()
    }])).collect::<Vec<_>>()))
}
fn compare(
    m: &Match,
    expected: &Value,
    rng: &RandomSnapshot,
    env_start: u32,
    source_start: u32,
    radius: f64,
    context: &str,
) {
    macro_rules! same {
        ($a:expr,$s:expr,$name:expr) => {
            assert_eq!($a, $s, "{context}: {}", $name);
        };
    }
    same!(
        m.world.game.tick,
        expected["tick"].as_i64().unwrap(),
        "tick"
    );
    same!(
        json!(m.world.game.units.keys().copied().collect::<Vec<_>>()),
        expected["actorOrder"],
        "actor order"
    );
    same!(
        m.next_unit_id,
        expected["nextId"].as_u64().unwrap() as u32,
        "next ID"
    );
    same!(
        m.start.random.snapshot(),
        *rng,
        "quiet native random streams"
    );
    same!(
        m.start.environment_seed.wrapping_sub(env_start),
        (expected["seed"].as_u64().unwrap() as u32).wrapping_sub(source_start),
        "environment advancement from own constructor states"
    );
    for source in expected["units"].as_array().unwrap() {
        let id = source["id"].as_u64().unwrap() as u32;
        let u = &m.world.game.units[&id];
        let c = &m.commands.units[&id];
        let combat = m.combat.get(id).unwrap();
        let fire = m.firing.get(id).unwrap();
        let ability = m.abilities.get(id).unwrap();
        same!(json!(u.owner), source["owner"], "owner");
        same!(json!(m.rules.kinds[u.kind].name), source["type"], "kind");
        for (k, n) in [
            ("x", u.x),
            ("z", u.z),
            ("rot", u.rot),
            ("hp", u.hp),
            ("aim", c.aim),
        ] {
            same!(bits(n), source[k], format!("actor {id} {k}"));
        }
        same!(
            json!(combat.cargo),
            source["cargo"],
            format!("actor {id} cargo order")
        );
        same!(
            u.riding,
            combat.carrier.is_some(),
            format!("actor {id} riding query flag")
        );
        if let Some(carrier) = combat.carrier {
            assert!(m.world.game.units.contains_key(&carrier));
            assert!(
                m.combat.get(carrier).unwrap().cargo.contains(&id),
                "{context}: missing reciprocal live carrier link"
            );
        }
        for cargo in &combat.cargo {
            if let Some(row) = m.combat.get(*cargo) {
                assert_eq!(row.carrier, Some(id), "{context}: cargo reciprocal link");
                assert!(m.world.game.units[cargo].riding);
            }
        }
        same!(
            json!(u.path.iter().copied().map(point).collect::<Vec<_>>()),
            source["path"],
            format!("actor {id} path")
        );
        same!(
            json!(c.orders.iter().map(waiting).collect::<Vec<_>>()),
            source["orders"],
            format!("actor {id} normalized waiting queue")
        );
        same!(
            paid(m, id),
            source["dig"],
            format!("actor {id} canonical paid work")
        );
        assert!(
            source["logistics"].is_null() && m.logistics.get(id).is_none(),
            "{context}: unsupported reserve setup"
        );
        same!(
            u.attack,
            c.attack_id != 0,
            "canonical attack activity projection"
        );
        same!(
            json!(m.world.game.flags[m.world.game.cell_of(u.x, u.z).unwrap()]),
            source["groundFlags"],
            "physical ground flags at actor pose"
        );
        same!(
            u.dig,
            m.field_works.is_active(id),
            "Dig owner/query projection"
        );
        same!(
            json!(m.field_works.membership(id)),
            source["entrench"],
            "Entrench owner projection"
        );
        same!(
            u.entrench,
            m.field_works.membership(id).is_some(),
            "Entrench query projection"
        );
        let controls = &source["controls"];
        let fields = [
            ("board", tagged(json!(u.board))),
            ("riding", tagged(json!(combat.carrier.unwrap_or(0)))),
            (
                "garrison",
                tagged(json!(u.garrison.map_or(-1, |v| v as i64))),
            ),
            ("attackId", tagged(json!(c.attack_id))),
            ("targetId", tagged(json!(fire.target_id))),
            ("enter", tagged(json!(c.enter.map_or(-1, |v| v as i64)))),
            ("fireAt", tagged(json!(c.fire_at.map_or(-1, |v| v as i64)))),
            ("retreating", json!(u.retreating)),
            ("holdFire", json!(c.hold_fire)),
            ("holdPos", json!(u.hold_pos)),
            ("auto", json!(c.auto_cast)),
            ("autoRetreat", json!(c.auto_retreat)),
            ("worldGoal", tagged_point(u.world_goal)),
            ("amove", tagged_point(u.amove)),
            ("reverse", tagged_point(u.reverse)),
            ("routeEnd", tagged_point(c.route_end)),
            ("drift", json!(u.drift)),
        ];
        for (k, v) in fields {
            same!(v, controls[k], format!("actor {id} control {k}"));
        }
        if controls["follow"].is_null() {
            same!(c.follow, 0, "absent source follow");
        } else {
            same!(tagged(json!(c.follow)), controls["follow"], "follow");
        }
        if controls["pace"].is_null() {
            assert!(u.pace.is_nan(), "{context}: absent source pace sentinel");
        } else {
            same!(tagged(json!(u.pace)), controls["pace"], "pace");
        }
        if controls["trafficWait"].is_null() {
            same!(u.traffic_wait, 0., "absent source trafficWait");
        } else {
            same!(
                tagged(json!(u.traffic_wait)),
                controls["trafficWait"],
                "trafficWait"
            );
        }
        same!(
            c.face.is_some(),
            !controls["face"].is_null(),
            "facing presence"
        );
        assert!(
            controls["face"].is_null(),
            "{context}: facing body outside ground-order fixture"
        );
        same!(
            u.traffic.is_some(),
            !controls["traffic"].is_null(),
            "traffic presence"
        );
        assert!(
            controls["traffic"].is_null(),
            "{context}: traffic body outside ground-order fixture"
        );
        assert!(controls["build"].is_null() || controls["build"] == tagged(json!(0)));
        same!(u.build, false, "quiet construction");
        let outcome = c.outcome.map(|o| match o {
            MoveOutcome::Interrupted => "interrupted",
            MoveOutcome::Moving => "moving",
            MoveOutcome::Unreachable => "unreachable",
            MoveOutcome::Arrived => "completed",
        });
        same!(json!(outcome), controls["moveOutcome"], "move outcome");
        same!(
            tagged(json!(c.outcome_tick)),
            controls["moveOutcomeTick"],
            "move outcome clock"
        );
        // Auxiliary source motionClearance/lane object shape has no receipt here.
        let actual_before = u
            .motion_before
            .map(|p| tagged(json!({"x":p.x,"z":p.z,"rot":p.rot})));
        same!(
            actual_before.unwrap_or(Value::Null),
            controls["motionBefore"],
            "motionBefore"
        );
        for (k, n) in [
            ("cd", ability.cooldown),
            ("cooldown", fire.cooldown),
            ("retarget", fire.retarget),
            ("repath", u.repath),
            ("buff", ability.buff),
            ("sprint", u.sprint),
            ("react", c.react),
            ("still", u.still.expect("canonical still timer")),
            ("stuck", u.stuck),
            ("supp", u.supp),
            ("reinf", m.recovery.get(id).unwrap().reinforcement),
            ("moveSpeed", u.move_speed),
            ("vx", u.vx.expect("canonical vx")),
            ("vz", u.vz.expect("canonical vz")),
        ] {
            same!(
                tagged(json!(n)),
                source["timers"][k],
                format!("actor {id} timer {k}")
            );
        }
        let travel = u
            .travel_dir
            .expect("source factory travelDir is represented");
        same!(
            tagged(json!(travel)),
            source["timers"]["travelDir"],
            "travelDir"
        );
        // JS candidates defaults to ordered bucket candidates, not exact-radius filtering.
        same!(
            json!(
                m.grid
                    .in_order(&m.world.game.units, (u.x, u.z), radius, |_, _| true)
            ),
            source["grid"],
            format!("actor {id} passive candidates")
        );
    }
    same!(
        m.field_works.project_sequence(),
        expected["projectSeq"].as_u64().unwrap(),
        "project identity sequence"
    );
    same!(
        projects(m),
        expected["projects"],
        "listed shared projects and retained counters"
    );
    same!(
        expected["randomDraws"],
        json!(1),
        "quiet source external draw count after its constructor"
    );
    for (slot, source) in expected["players"].as_array().unwrap().iter().enumerate() {
        let p = &m.start.economy.players[slot];
        same!(json!(slot), source["slot"], "player slot");
        same!(json!(p.team), source["team"], "player team");
        same!(
            point(m.start.players[slot].spawn),
            source["spawn"],
            "source player spawn"
        );
        let mut visible = m.world.game.players[slot].visible.clone();
        visible.sort_unstable();
        same!(
            json!(visible),
            source["visible"],
            "visible target membership"
        );
        for (k, n) in [("mp", p.mp), ("inc", p.inc), ("vp", p.vp)] {
            same!(bits(n), source[k], format!("wallet {slot}/{k}"));
        }
        for (k, n) in [("mun", p.mun), ("fuel", p.fuel)] {
            if source[k].is_null() {
                same!(n, 0., format!("absent source {k}"));
            } else {
                same!(bits(n), source[k], k);
            }
        }
        for (k, v) in [("out", p.out), ("away", p.away)] {
            if source[k].is_null() {
                same!(v, false, format!("absent source {k}"));
            } else {
                same!(json!(v), source[k], k);
            }
        }
        same!(
            tagged(json!(m.start.players[slot].support)),
            source["sup"],
            "support cooldowns"
        );
    }
    for (slot, s) in m.story.iter().enumerate() {
        for (k, n) in [
            ("kills", s.kills),
            ("losses", s.losses),
            ("built", s.built),
            ("captures", s.captures),
            ("mpSpent", s.mp_spent),
            ("supportCalls", s.support_calls),
            ("planesDowned", s.planes_downed),
        ] {
            if expected["story"].is_null() {
                same!(n, 0., "lazy source story default");
            } else {
                same!(tagged(json!(n)), expected["story"][slot][k], k);
            }
        }
    }
    if expected["timeline"].is_null() {
        assert!(m.timeline.is_empty());
    } else {
        same!(
            tagged(json!(
                m.timeline
                    .iter()
                    .map(|s| match s {
                        MatchSample::Conquest { t, vp } => json!({"t":t,"vp":vp}),
                        MatchSample::Territories { .. } => panic!(
                            "{context}: Territories timeline is outside the ground-order fixture"
                        ),
                    })
                    .collect::<Vec<_>>()
            )),
            expected["timeline"],
            "story sample"
        );
    }
    let stats = m.commands.path_requests().stats();
    if !expected["pathStats"].is_null() {
        same!(
            stats.deferred as f64,
            number(&expected["pathStats"]["deferred"]),
            "public scheduler deferred"
        );
        same!(
            stats.dropped as f64,
            number(&expected["pathStats"]["dropped"]),
            "public scheduler dropped"
        );
    } else {
        same!(stats.deferred, 0, "absent source deferred counter");
        same!(stats.dropped, 0, "absent source dropped counter");
    }
    same!(expected["shots"], json!([]), "quiet source shots");
    assert!(m.firing.events.is_empty() && m.support.events.is_empty());
    assert!(expected["winner"].is_null() && m.finish.is_none());
}
fn order_key(action: &Value) -> OrderKey {
    OrderKey {
        tick: action["key"]["tick"].as_u64().unwrap(),
        issuer: action["key"]["issuer"].as_u64().unwrap() as u32,
        sequence: action["key"]["sequence"].as_u64().unwrap(),
    }
}
/// Exclusions describe the absent canonical input provider, not a runtime pass.
fn component_exclusion(name: &str) -> Option<&'static str> {
    match name {
        "mixed-selection-follow-and-admission" | "retreat-selection-repeats-forced-and-riders" => {
            Some("rail factory and partial reserve setup have no represented scene provider")
        }
        _ => None,
    }
}
fn full_step_exclusion(c: &Value) -> Option<&'static str> {
    component_exclusion(c["name"].as_str().unwrap()).or_else(|| {
        c["setup"]["scope"]["allowEffects"]
            .as_bool()
            .unwrap()
            .then_some("dead-target retirement draws effects outside exact quiet damage parity")
    })
}
fn assert_inventory(f: &Value) {
    let cases = f["cases"].as_array().unwrap();
    let excluded = cases
        .iter()
        .filter_map(|c| {
            component_exclusion(c["name"].as_str().unwrap())
                .map(|why| (c["name"].as_str().unwrap(), why))
        })
        .collect::<Vec<_>>();
    assert_eq!(excluded.len(), 2, "explicit component exclusion inventory");
    assert_eq!(
        cases
            .iter()
            .filter(|c| c["setup"]["scope"]["allowEffects"] == true)
            .count(),
        1
    );
    assert_eq!(
        cases
            .iter()
            .filter(|c| c["setup"]["scope"]["boundaryOnly"] == true)
            .count(),
        4
    );
}
#[test]
fn actual_dispatch_matches_ground_order_command_prefixes() {
    let f = fixture();
    assert_inventory(&f);
    let mut compared_cases = 0;
    let mut command_records = 0;
    for c in f["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        if component_exclusion(name).is_some() {
            continue;
        }
        let mut m = make(c);
        let rng = m.start.random.snapshot();
        let env = m.start.environment_seed;
        let source_env = c["initial"]["seed"].as_u64().unwrap() as u32;
        let radius = c["setup"]["gridRadius"].as_f64().unwrap();
        let mut expected = c["initial"].clone();
        compare(&m, &expected, &rng, env, source_env, radius, name);
        let mut commands_started = false;
        for (index, record) in c["records"].as_array().unwrap().iter().enumerate() {
            let action = &record["action"];
            if action["op"] == "step" {
                break;
            }
            match action["op"].as_str().unwrap() {
                "prime" => {
                    assert!(
                        !commands_started,
                        "declared primes precede boundary commands"
                    );
                    prime_units(&mut m, c, &action["units"]);
                    refresh(&mut m, action["vision"] != false);
                }
                "command" => {
                    let key = order_key(action);
                    assert_eq!(key.tick, m.world.game.tick as u64 + 1);
                    let denial = match Command::from_json(&action["cmd"]) {
                        Ok(command) => m
                            .execute_command(key.issuer as usize, &command, false)
                            .unwrap_or_else(|error| {
                                panic!("{name}: actual command failed: {error}")
                            }),
                        Err(denial) => Some(denial),
                    };
                    assert_eq!(
                        denial.map_or(Value::Null, |d| json!(d.name())),
                        record["result"],
                        "{name}: actual command denial"
                    );
                    command_records += 1;
                    commands_started = true;
                }
                other => panic!("unsupported source operation {other}"),
            }
            oracle_patch(&mut expected, &c["patches"][index]);
            assert_eq!(record["lastSnapshot"].as_u64().unwrap(), index as u64 + 1);
            compare(
                &m,
                &expected,
                &rng,
                env,
                source_env,
                radius,
                &format!("{name} command prefix {index}"),
            );
        }
        assert_eq!(
            m.world.game.tick, 0,
            "prefix test performs no simulation step"
        );
        assert_eq!(
            m.commands.pending_commands(),
            0,
            "prefix test does not bypass an enqueued boundary"
        );
        compared_cases += 1;
    }
    assert_eq!(compared_cases, 26);
    assert_eq!(command_records, 42);
}
#[test]
fn queued_ground_orders_match_original_full_steps_and_arrival_order() {
    let f = fixture();
    assert_inventory(&f);
    let mut steps = 0;
    let mut command_records = 0;
    let mut receipt_count = 0;
    let mut compared_cases = 0;
    for reverse_arrival in [false, true] {
        for c in f["cases"].as_array().unwrap() {
            let name = c["name"].as_str().unwrap();
            if c["setup"]["scope"]["boundaryOnly"] == true || full_step_exclusion(c).is_some() {
                continue;
            }
            let mut m = make(c);
            let rng = m.start.random.snapshot();
            let env = m.start.environment_seed;
            let source_env = c["initial"]["seed"].as_u64().unwrap() as u32;
            let wear = m.world.game.wear.clone();
            let surface_wear = m.world.surface_wear.clone();
            let mut expected = c["initial"].clone();
            let radius = c["setup"]["gridRadius"].as_f64().unwrap();
            let mut pending: Vec<(OrderKey, Result<Command, Denial>, Value)> = Vec::new();
            compare(&m, &expected, &rng, env, source_env, radius, name);
            for (index, record) in c["records"].as_array().unwrap().iter().enumerate() {
                let action = &record["action"];
                match action["op"].as_str().unwrap() {
                    "prime" => {
                        assert!(pending.is_empty(), "{name}: prime after a boundary command");
                        prime_units(&mut m, c, &action["units"]);
                        refresh(&mut m, action["vision"] != false);
                    }
                    "command" => {
                        let key = order_key(action);
                        assert_eq!(key.tick, m.world.game.tick as u64 + 1);
                        let parsed = Command::from_json(&action["cmd"]);
                        if let Err(denial) = &parsed {
                            assert_eq!(
                                json!(denial.name()),
                                record["result"],
                                "source whole-payload parser denial"
                            );
                        }
                        pending.push((key, parsed, record["result"].clone()));
                        command_records += 1;
                    }
                    "step" => {
                        pending.sort_by_key(|r| r.0);
                        let oracle = pending
                            .iter()
                            .filter(|(_, c, _)| c.is_ok())
                            .map(|(key, _, denial)| (*key, denial.clone()))
                            .collect::<Vec<_>>();
                        if reverse_arrival {
                            pending.reverse();
                        }
                        for (key, command, _) in pending.drain(..) {
                            if let Ok(command) = command {
                                m.enqueue_command(key, command).unwrap();
                            }
                        }
                        // One unchanged sim.step is consumed by one real canonical runner call.
                        let report = m.advance_tick().unwrap_or_else(|error| {
                            panic!("{name}: actual full ground tick failed: {error}")
                        });
                        // Pending public receipt name agreed with the sole tick owner. This
                        // assertion fails clearly until its actual enum/runner hook lands.
                        assert!(
                            report
                                .phases
                                .iter()
                                .any(|p| format!("{p:?}") == "GroundAttack"),
                            "{name}: actual GroundAttack tick hook/receipt is unavailable"
                        );
                        assert!(
                            !report
                                .unavailable
                                .iter()
                                .any(|p| format!("{p:?}") == "GroundAttack"),
                            "{name}: GroundAttack remains unavailable"
                        );
                        assert!(
                            report.unavailable.contains(&TickPhase::GroundEffects),
                            "excluded ground-effects scope changed"
                        );
                        assert_eq!(
                            report.commands.len(),
                            oracle.len(),
                            "{name}: real boundary receipt count"
                        );
                        for (actual, (key, source)) in report.commands.iter().zip(&oracle) {
                            assert_eq!(actual.key, *key, "{name}: exact source command key/order");
                            assert_eq!(
                                actual.denial.map_or(Value::Null, |d| json!(d.name())),
                                *source,
                                "{name}: actual boundary denial {key:?}"
                            );
                        }
                        receipt_count += oracle.len();
                        steps += 1;
                    }
                    other => panic!("unknown source ground-order operation {other}"),
                }
                oracle_patch(&mut expected, &c["patches"][index]);
                assert_eq!(record["lastSnapshot"].as_u64().unwrap(), index as u64 + 1);
                if action["op"] == "step" {
                    compare(
                        &m,
                        &expected,
                        &rng,
                        env,
                        source_env,
                        radius,
                        &format!("{name} post-step {index} reverse={reverse_arrival}"),
                    );
                    assert_eq!(
                        m.world.game.wear, wear,
                        "{name}: excluded ground wear remains unchanged"
                    );
                    assert_eq!(
                        m.world.surface_wear, surface_wear,
                        "{name}: excluded surface wear remains unchanged"
                    );
                }
            }
            assert!(
                pending.is_empty(),
                "{name}: trailing source commands have no step"
            );
            assert_eq!(
                c["patches"].as_array().unwrap().len() + 1,
                c["snapshots"].as_u64().unwrap() as usize
            );
            compared_cases += 1;
        }
    }
    assert_eq!(compared_cases, 46);
    assert_eq!(steps, 58);
    assert_eq!(command_records, 80);
    assert_eq!(
        receipt_count, 78,
        "two parser-rejected payloads never enter the queue"
    );
}
