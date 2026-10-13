//! Replay the frozen full-step source oracle through the single Match runner.
//! Constructor randomness remains native. Quiet runtime streams must not draw;
//! environment seed advancement compares deltas from each constructor's state.
use serde_json::{Value, json};
use std::collections::BTreeSet;
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    field_works::{Project, ProjectOrder},
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, MoveOutcome, UnitControl},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::TickPathConfig,
    random::RandomSnapshot,
    unit_spawn,
    weather::WeatherChoice,
};
#[path = "common/startup.rs"]
mod startup;

fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn number(v: &Value) -> f64 {
    let s = v.as_str().or_else(|| v["f64"].as_str()).unwrap();
    f64::from_bits(u64::from_str_radix(s, 16).unwrap())
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
fn project(p: &Project) -> Value {
    json!({"id":p.id,"owner":p.owner,"fort":p.fort.as_str(),"crew":p.crew,"active":p.active,
        "jobs":p.jobs.iter().map(|j|json!({"kind":j.kind.as_str(),"x":bits(j.at.x),"z":bits(j.at.z),"dir":bits(j.direction),"front":bits(j.front)})).collect::<Vec<_>>()})
}
fn order(c: &Command) -> Value {
    match c {
        Command::Entrench {
            ids,
            order:
                ProjectOrder::Join {
                    project,
                    at: Some(p),
                },
            ..
        } => tagged(json!({"t":"entrench","ids":ids,"join":project,"x":p.x,"z":p.z})),
        _ => panic!("unrepresented waiting command in frozen mass fixture: {c:?}"),
    }
}
fn paid(m: &Match, id: u32) -> Value {
    let Some(d) = m.field_works.jobs().get(&id) else {
        return Value::Null;
    };
    json!({"x":bits(d.at.x),"z":bits(d.at.z),"cells":d.cells.iter().map(|c|json!([c.cell,char::from(c.ch).to_string()])).collect::<Vec<_>>(),
        "t":bits(d.elapsed),"dir":bits(d.direction),"front":d.front.map(bits),"kind":d.kind.as_str(),"reach":d.reach.map(bits),"n":d.original_cells,"waits":null,"project":d.project})
}
fn alias(c: &Value, name: &str) -> u32 {
    c["setup"]["aliases"][name].as_u64().unwrap() as u32
}
fn refresh(m: &mut Match) {
    m.grid.rebuild(&m.world.game.units);
    ww2_core::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    for owner in 0..m.world.game.players.len() {
        m.refresh_recipient_routing(owner).unwrap();
    }
}
fn make(c: &Value) -> Match {
    let setup = &c["setup"];
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&setup["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "Crew".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "Opposition".into(),
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
    assert_eq!(
        json!(m.world.game.units.keys().copied().collect::<Vec<_>>()),
        setup["initialActors"]
    );
    // Source births are unpaid setup factories, not recorded Buy commands.
    let kind = m.rules.kind("engineer").unwrap();
    for (index, pose) in setup["spawnCrew"]["positions"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let id = m.next_unit_id;
        assert_eq!(id, alias(c, &format!("worker{}", index + 1)));
        assert!(!m.world.game.units.contains_key(&id));
        assert!(!m.commands.units.contains_key(&id));
        assert!(m.combat.get(id).is_none() && m.firing.get(id).is_none());
        assert!(m.abilities.get(id).is_none() && m.recovery.get(id).is_none());
        let next = id.checked_add(1).unwrap();
        let unit = unit_spawn::initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            kind,
            id,
            0,
            Point {
                x: pose["x"].as_f64().unwrap(),
                z: pose["z"].as_f64().unwrap(),
            },
        )
        .unwrap();
        let control = UnitControl::new(&unit, &m.rules).unwrap();
        m.commands.insert(id, control);
        m.combat.initialize(id);
        m.firing.initialize(id);
        m.abilities.initialize(id);
        m.recovery.initialize(id);
        m.world.game.units.insert(id, unit);
        m.next_unit_id = next;
    }
    let observer = m.world.game.units.get_mut(&alias(c, "observer")).unwrap();
    observer.x = setup["prime"]["observer"]["x"].as_f64().unwrap();
    observer.z = setup["prime"]["observer"]["z"].as_f64().unwrap();
    for unit in m.world.game.units.values_mut() {
        unit.hold_pos = true;
    }
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    m.start.economy.players[0].mp = 1000.;
    m.world.take_journal();
    refresh(&mut m);
    if c["records"][0]["action"]["tick"].is_null() {
        m.initialize_tick_scene(
            0,
            TickPathConfig {
                budget: 4096,
                retry_limit: setup["prime"]["pathRetryLimit"].as_u64().unwrap() as u32,
            },
        )
        .unwrap();
    }
    m
}
fn prime(m: &mut Match, c: &Value, action: &Value) {
    if let Some(tick) = action["tick"].as_u64() {
        m.initialize_tick_scene(
            tick,
            TickPathConfig {
                budget: 4096,
                retry_limit: c["setup"]["prime"]["pathRetryLimit"].as_u64().unwrap() as u32,
            },
        )
        .unwrap();
    }
    if let Some(units) = action["units"].as_object() {
        for (name, changes) in units {
            let id = alias(c, name);
            let unit = m.world.game.units.get_mut(&id).unwrap();
            assert_eq!(changes.as_object().unwrap().len(), 1);
            assert!(changes.as_object().unwrap().contains_key("hp"));
            unit.hp = changes["hp"].as_f64().unwrap();
        }
    }
    refresh(m);
}
fn apply_patch(state: &mut Value, patch: &Value) {
    for change in patch.as_array().unwrap() {
        let path = change[0].as_array().unwrap();
        if path.is_empty() {
            *state = change[1].clone();
            continue;
        }
        let mut at = &mut *state;
        for key in &path[..path.len() - 1] {
            at = if let Some(n) = key.as_u64() {
                &mut at[n as usize]
            } else {
                &mut at[key.as_str().unwrap()]
            };
        }
        let key = path.last().unwrap();
        if let Some(n) = key.as_u64() {
            at[n as usize] = change[1].clone();
        } else if change.as_array().unwrap().len() == 1 {
            at.as_object_mut().unwrap().remove(key.as_str().unwrap());
        } else {
            at[key.as_str().unwrap()] = change[1].clone();
        }
    }
}
fn compare(
    m: &Match,
    expected: &Value,
    rng: &RandomSnapshot,
    env_start: u32,
    source_start: u32,
    context: &str,
) {
    macro_rules! same {
        ($actual:expr,$expected:expr,$field:expr) => {
            assert_eq!($actual, $expected, "{context}: {}", $field)
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
        "environment draw advancement"
    );
    same!(
        m.field_works.project_sequence(),
        expected["projectSeq"].as_u64().unwrap(),
        "project sequence"
    );
    same!(
        json!(m.field_works.projects().map(project).collect::<Vec<_>>()),
        expected["listed"],
        "listed projects"
    );
    let refs = expected["retained"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| {
            p["listed"] == true
                || !p["memberIds"].as_array().unwrap().is_empty()
                || !p["paidIds"].as_array().unwrap().is_empty()
        })
        .collect::<Vec<_>>();
    let ids = refs
        .iter()
        .map(|p| p["id"].as_u64().unwrap())
        .collect::<BTreeSet<_>>();
    let actual_refs = m
        .field_works
        .projects()
        .map(|p| p.id)
        .chain(
            m.world
                .game
                .units
                .keys()
                .filter_map(|id| m.field_works.membership(*id)),
        )
        .chain(m.field_works.jobs().values().filter_map(|d| d.project))
        .collect::<BTreeSet<_>>();
    same!(actual_refs, ids, "operational project identities");
    for source in refs {
        let id = source["id"].as_u64().unwrap();
        let p = m
            .field_works
            .retained_projects()
            .find(|p| p.id == id)
            .unwrap();
        let mut source_row = source.clone();
        for key in ["listed", "memberIds", "paidIds", "queuedJoinIds"] {
            source_row.as_object_mut().unwrap().remove(key);
        }
        same!(project(p), source_row, "operational project body");
        same!(
            m.field_works.project(id).is_some(),
            source["listed"].as_bool().unwrap(),
            "project listed flag"
        );
        let members = m
            .world
            .game
            .units
            .keys()
            .copied()
            .filter(|unit| m.field_works.membership(*unit) == Some(id))
            .collect::<Vec<_>>();
        let paid = m
            .field_works
            .jobs()
            .iter()
            .filter(|(_, d)| d.project == Some(id))
            .map(|(unit, _)| *unit)
            .collect::<Vec<_>>();
        let queued=m.commands.units.iter().filter(|(unit,c)|m.world.game.units.contains_key(unit)&&c.orders.iter().any(|o|matches!(o,Command::Entrench{order:ProjectOrder::Join{project,..},..} if *project==id))).map(|(unit,_)|*unit).collect::<Vec<_>>();
        same!(json!(members), source["memberIds"], "member refs");
        same!(json!(paid), source["paidIds"], "paid refs");
        same!(json!(queued), source["queuedJoinIds"], "queued join refs");
    }
    for source in expected["units"].as_array().unwrap() {
        let id = source["id"].as_u64().unwrap() as u32;
        let unit = &m.world.game.units[&id];
        let control = &m.commands.units[&id];
        let firing = m.firing.get(id).unwrap();
        let ability = m.abilities.get(id).unwrap();
        same!(json!(unit.owner), source["owner"], "unit owner");
        same!(
            json!(m.rules.kinds[unit.kind].name),
            source["type"],
            "unit kind"
        );
        for (key, value) in [
            ("hp", unit.hp),
            ("x", unit.x),
            ("z", unit.z),
            ("rot", unit.rot),
            ("aim", control.aim),
        ] {
            same!(bits(value), source[key], format!("unit {id} {key}"));
        }
        same!(paid(m, id), source["dig"], format!("unit {id} paid work"));
        same!(
            json!(m.field_works.membership(id)),
            source["entrench"],
            format!("unit {id} membership")
        );
        same!(
            unit.dig,
            m.field_works.is_active(id),
            "synchronous Dig projection"
        );
        same!(
            unit.entrench,
            m.field_works.membership(id).is_some(),
            "synchronous Entrench projection"
        );
        same!(
            json!(unit.path.iter().copied().map(point).collect::<Vec<_>>()),
            source["path"],
            "path"
        );
        same!(
            json!(control.orders.iter().map(order).collect::<Vec<_>>()),
            source["orders"],
            "waiting orders"
        );
        let fields = [
            ("attackId", tagged(json!(control.attack_id))),
            ("targetId", tagged(json!(firing.target_id))),
            (
                "garrison",
                tagged(json!(unit.garrison.map_or(-1, |v| v as i64))),
            ),
            (
                "enter",
                tagged(json!(control.enter.map_or(-1, |v| v as i64))),
            ),
            ("board", tagged(json!(unit.board))),
            ("retreating", json!(unit.retreating)),
            ("holdFire", json!(control.hold_fire)),
            ("holdPos", json!(unit.hold_pos)),
            ("auto", json!(control.auto_cast)),
            ("autoRetreat", json!(control.auto_retreat)),
            (
                "worldGoal",
                unit.world_goal
                    .map(|p| tagged(json!({"x":p.x,"z":p.z})))
                    .unwrap_or(Value::Null),
            ),
        ];
        for (key, value) in fields {
            same!(
                value,
                source["controls"][key],
                format!("unit {id} control {key}")
            );
        }
        let fields = [
            (
                "face",
                control
                    .face
                    .map(|f| tagged(json!({"angle":f.angle,"at":{"x":f.at.x,"z":f.at.z}})))
                    .unwrap_or(Value::Null),
            ),
            (
                "riding",
                tagged(json!(m.combat.get(id).unwrap().carrier.unwrap_or(0))),
            ),
            (
                "amove",
                unit.amove
                    .map(|p| tagged(json!({"x":p.x,"z":p.z})))
                    .unwrap_or(Value::Null),
            ),
            (
                "fireAt",
                tagged(json!(control.fire_at.map_or(-1, |v| v as i64))),
            ),
        ];
        for (key, value) in fields {
            same!(
                value,
                source["controls"][key],
                format!("unit {id} control {key}")
            );
        }
        let follow = &source["controls"]["follow"];
        if follow.is_null() {
            same!(control.follow, 0, "source-absent initial follow");
        } else {
            same!(tagged(json!(control.follow)), *follow, "follow");
        }
        let build = &source["controls"]["build"];
        assert!(
            build.is_null() || build == &tagged(json!(0)),
            "{context}: unexpected construction activity"
        );
        assert!(!unit.build, "{context}: native construction activity");
        let outcome = control.outcome.map(|v| match v {
            MoveOutcome::Interrupted => "interrupted",
            MoveOutcome::Moving => "moving",
            MoveOutcome::Unreachable => "unreachable",
            MoveOutcome::Arrived => "completed",
        });
        same!(
            json!(outcome),
            source["controls"]["moveOutcome"],
            "move outcome"
        );
        same!(
            tagged(json!(control.outcome_tick)),
            source["controls"]["moveOutcomeTick"],
            "move outcome clock"
        );
        if !source["controls"]["moveResult"].is_null() {
            let result = match control.outcome {
                Some(MoveOutcome::Unreachable) => "unreachable",
                Some(MoveOutcome::Arrived) => "completed",
                other => panic!("{context}: source result has no canonical outcome {other:?}"),
            };
            same!(
                tagged(json!([result, control.outcome_tick.unwrap()])),
                source["controls"]["moveResult"],
                "move result facts"
            );
        }
        for (key, value) in [
            ("repath", unit.repath),
            ("cooldown", firing.cooldown),
            ("retarget", firing.retarget),
            ("cd", ability.cooldown),
            ("buff", ability.buff),
            ("sprint", unit.sprint),
            ("react", control.react),
            ("still", unit.still.unwrap()),
            ("stuck", unit.stuck),
            ("supp", unit.supp),
            ("moveSpeed", unit.move_speed),
            ("vx", unit.vx.unwrap()),
            ("vz", unit.vz.unwrap()),
        ] {
            same!(
                tagged(json!(value)),
                source["timers"][key],
                format!("unit {id} timer {key}")
            );
        }
        // The original factory omits trafficWait until interruption initializes it.
        let wait = &source["timers"]["trafficWait"];
        if wait.is_null() {
            same!(unit.traffic_wait, 0., "absent initial traffic wait");
        } else {
            same!(tagged(json!(unit.traffic_wait)), *wait, "traffic wait");
        }
    }
    for (slot, source) in expected["players"].as_array().unwrap().iter().enumerate() {
        let p = &m.start.economy.players[slot];
        same!(json!(slot), source["slot"], "player slot");
        same!(json!(p.team), source["team"], "player team");
        same!(json!(p.out), source["out"], "player out");
        same!(json!(p.away), source["away"], "player away");
        for (key, value) in [("mun", p.mun), ("fuel", p.fuel)] {
            if source[key].is_null() {
                same!(value, 0., format!("source-absent initial {key}"));
            } else {
                same!(bits(value), source[key], key);
            }
        }
        same!(bits(p.mp), source["mp"], "wallet including income");
        same!(bits(p.inc), source["inc"], "income rate");
        same!(bits(p.vp), source["vp"], "VP");
    }
    for (slot, row) in m.story.iter().enumerate() {
        for (key, n) in [
            ("kills", row.kills),
            ("losses", row.losses),
            ("built", row.built),
            ("captures", row.captures),
            ("mpSpent", row.mp_spent),
            ("supportCalls", row.support_calls),
            ("planesDowned", row.planes_downed),
        ] {
            let source = &expected["story"][slot][key];
            if expected["story"].is_null() {
                same!(n, 0., "source-absent eager story default");
            } else {
                same!(tagged(json!(n)), *source, key);
            }
        }
    }
    if expected["timeline"].is_null() {
        assert!(m.timeline.is_empty(), "{context}: source-absent timeline");
    } else {
        same!(
            tagged(json!(
                m.timeline
                    .iter()
                    .map(|s| json!({"t":s.t,"vp":s.vp}))
                    .collect::<Vec<_>>()
            )),
            expected["timeline"],
            "story samples"
        );
    }
    same!(expected["shots"], json!([]), "quiet source shots");
    assert!(
        m.firing.events.is_empty() && m.support.events.is_empty(),
        "{context}: unexpected native shot producer event"
    );
    same!(
        json!(m.world.journal()),
        expected["terrain"]["journal"],
        "terrain journal"
    );
    same!(
        json!([
            m.world.game.terrain_version,
            m.world.versions.obstruction,
            m.world.versions.infantry,
            m.world.versions.vehicle,
            m.world.versions.naval
        ]),
        expected["terrain"]["versions"],
        "terrain versions"
    );
    for cell in expected["terrain"]["cells"].as_array().unwrap() {
        let c = cell["c"].as_u64().unwrap() as usize;
        let g = &m.world.game;
        for (key, ch) in [
            ("ch", g.chars[c]),
            ("ground", g.ground.as_ref().unwrap()[c]),
            ("object", g.objects.as_ref().unwrap()[c]),
        ] {
            same!(json!(char::from(ch).to_string()), cell[key], key);
        }
        same!(json!(m.world.mine_layer[c]), cell["mine"], "mine layer");
        same!(json!(g.flags[c]), cell["flags"], "flags");
        same!(
            json!(g.height.as_ref().map_or(0, |h| h[c])),
            cell["height"],
            "height"
        );
        for (key, value) in [
            ("hp", g.cell_hp[c]),
            ("wear", g.wear.as_ref().unwrap()[c]),
            ("surfaceWear", m.world.surface_wear[c]),
            ("scar", m.world.scar[c]),
        ] {
            same!(bits(f64::from(value)), cell[key], key);
        }
        same!(json!(m.world.burnt[c]), cell["burnt"], "burnt");
        same!(json!(m.world.cell_state[c]), cell["state"], "state");
        same!(
            json!(g.trench_front.as_ref().map(|v| v[c].to_bits())),
            cell["frontBits"],
            "trench front bits"
        );
    }
    same!(
        tagged(json!(
            m.world
                .cell_log()
                .iter()
                .map(startup::row)
                .collect::<Vec<_>>()
        )),
        expected["terrain"]["cellLog"],
        "terrain cell log"
    );
    same!(
        tagged(json!(
            m.world
                .new_cells()
                .iter()
                .map(startup::row)
                .collect::<Vec<_>>()
        )),
        expected["terrain"]["newCells"],
        "new cells"
    );
    same!(
        json!(
            m.world
                .mines
                .iter()
                .map(|(c, p)| json!([c, p]))
                .collect::<Vec<_>>()
        ),
        expected["terrain"]["mines"],
        "mine owners"
    );
    same!(
        json!(
            m.world
                .mine_seen
                .iter()
                .map(|(c, p)| json!([c, p]))
                .collect::<Vec<_>>()
        ),
        expected["terrain"]["mineSeen"],
        "seen mines"
    );
    same!(
        json!(
            m.world
                .aid
                .iter()
                .map(|(c, p)| json!([c, p]))
                .collect::<Vec<_>>()
        ),
        expected["terrain"]["aid"],
        "aid"
    );
    if !expected["pathStats"].is_null() {
        same!(
            m.path_requests().stats().dropped as f64,
            number(&expected["pathStats"]["dropped"]),
            "actual scheduler drops"
        );
    }
    assert!(
        expected["winner"].is_null() && m.finish.is_none(),
        "{context}: quiet winner"
    );
}

#[test]
fn canonical_mass_projects_replay_original_full_steps() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../core/tests/fixtures/mass-project-tick.json"
    ))
    .unwrap();
    let mut steps = 0;
    let mut snapshots = 0;
    for c in fixture["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut m = make(c);
        let rng = m.start.random.snapshot();
        let env = m.start.environment_seed;
        let source_env = c["initial"]["seed"].as_u64().unwrap() as u32;
        let mut expected = c["initial"].clone();
        let mut next_patch = 0;
        compare(&m, &expected, &rng, env, source_env, name);
        snapshots += 1;
        for record in c["records"].as_array().unwrap() {
            let action = &record["action"];
            let count = match action["op"].as_str().unwrap() {
                "command" => {
                    let command = Command::from_json(&action["cmd"]).unwrap();
                    let denial = m
                        .execute_command(
                            action["slot"].as_u64().unwrap_or(0) as usize,
                            &command,
                            false,
                        )
                        .unwrap();
                    assert!(denial.is_none(), "{name}: {denial:?}");
                    1
                }
                "prime" => {
                    prime(&mut m, c, action);
                    1
                }
                "steps" => action["count"].as_u64().unwrap() as usize,
                other => panic!("unknown frozen source action {other}"),
            };
            for _ in 0..count {
                if action["op"] == "steps" {
                    let report = m.advance_tick().unwrap_or_else(|e| {
                        panic!("{name} actual tick {}: {e}", m.world.game.tick)
                    });
                    assert!(report.commands.is_empty());
                    assert!(
                        report
                            .phases
                            .contains(&ww2_core::match_tick::TickPhase::FieldProjects)
                    );
                    assert!(
                        !report
                            .unavailable
                            .contains(&ww2_core::match_tick::TickPhase::FieldProjects)
                    );
                    steps += 1;
                }
                apply_patch(&mut expected, &c["patches"][next_patch]);
                next_patch += 1;
                snapshots += 1;
                compare(
                    &m,
                    &expected,
                    &rng,
                    env,
                    source_env,
                    &format!("{name} snapshot {next_patch}"),
                );
            }
            assert_eq!(
                next_patch,
                record["lastSnapshot"].as_u64().unwrap() as usize
            );
        }
        assert_eq!(next_patch + 1, c["snapshots"].as_u64().unwrap() as usize);
    }
    assert_eq!((steps, snapshots), (193, 215));
}
