//! Complete pending-drop history through actual ordinary providers and full ticks.
//! The generated oracle is deliberately absent until an authorized Source run.
use serde_json::{Value, json};
use std::collections::BTreeSet;
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::{ArmySize, Context, ModeContext, PendingDrop, Unit as PopulationUnit},
    game::Point,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_support::{SupportEvent, SupportKind},
    match_tick::{TickEvent, TickPhase},
    random::StreamId,
    vision,
    weather::WeatherChoice,
};

fn bits(value: f64) -> Value {
    assert!(value.is_finite());
    json!(format!("{:016x}", value.to_bits()))
}

fn point(value: Option<Point>) -> Value {
    value.map_or(Value::Null, |p| json!([bits(p.x), bits(p.z)]))
}

fn make(f: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&f["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "a".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "b".into(),
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
    // These declared initial controls and wallets are applied on both sides.
    // No pose, roster, footprint, producer, timer, ID or random state is imported.
    for u in m.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    for player in &mut m.start.economy.players {
        player.mp = 10000.0;
    }
    m.grid.rebuild(&m.world.game.units);
    vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    for owner in 0..2 {
        m.refresh_recipient_routing(owner).unwrap();
    }
    m
}

fn identity(m: &Match, original: &BTreeSet<u32>) -> Value {
    json!(original.iter().map(|id| {
        let u = m.world.game.units.get(id).expect("original actor survives");
        let kind = &m.rules.kinds[u.kind];
        let structure = m.world.structures.iter().find(|s| s.entity_id == Some(*id));
        let protected = if kind.structure || u.owner == 1 {
            json!([bits(u.hp), bits(u.x), bits(u.z), u.cells, structure.map(|s| s.id.as_str())])
        } else {
            Value::Null
        };
        json!([id, u.owner, kind.name, protected])
    }).collect::<Vec<_>>())
}

fn population(m: &Match) -> Value {
    let queues = m.world.game.units.values().map(|u| {
        m.production.building(u.id).map_or_else(Vec::new, |b| b.queue().collect::<Vec<_>>())
    }).collect::<Vec<_>>();
    let rows = m.world.game.units.values().zip(&queues).map(|(u, queue)| PopulationUnit {
        id: u64::from(u.id),
        owner: Some(u.owner),
        kind: m.rules.kinds[u.kind].name.as_str(),
        hp: u.hp,
        built: u.built.unwrap_or(0.0),
        queue,
    }).collect::<Vec<_>>();
    let field = rows.iter().zip(m.world.game.units.values())
        .filter(|(_, u)| !m.rules.kinds[u.kind].structure)
        .map(|(row, _)| *row).collect::<Vec<_>>();
    let structures = rows.iter().zip(m.world.game.units.values())
        .filter(|(_, u)| m.rules.kinds[u.kind].structure)
        .map(|(row, _)| *row).collect::<Vec<_>>();
    let drops = m.world.game.strikes.iter().map(|s| PendingDrop {
        owner: s.owner,
        kind: s.runtime.as_ref().expect("actual support timing").kind.name(),
        live: s.live,
    }).collect::<Vec<_>>();
    let context = Context {
        mode: ModeContext { kind: m.options.mode, ..Default::default() },
        army: m.options.army,
        units: &rows,
        drops: &drops,
        points: &[],
        nodes: &[],
        world: None,
    };
    // Query the existing population provider on actual canonical rows.
    // Separate contexts expose each real contribution, without supplying drop facts.
    let wallet = &m.start.economy;
    json!({
        "field": wallet.population(&Context { units: &field, drops: &[], ..context }, 0).unwrap(),
        "queued": wallet.population(&Context { units: &structures, drops: &[], ..context }, 0).unwrap(),
        "reserved": wallet.population(&Context { units: &[], ..context }, 0).unwrap(),
        "total": wallet.population(&context, 0).unwrap(),
        "cap": wallet.population_cap(&context, 0).unwrap(),
        "globalCount": m.world.game.units.len(),
    })
}

fn invariant(m: &Match, original: &BTreeSet<u32>, initial_identity: &Value) {
    assert_eq!(identity(m, original), *initial_identity);
    assert_eq!(m.start.points.len(), 1);
    let p = &m.start.points[0];
    assert_eq!((p.at.x, p.at.z), (81.0, 17.0));
    assert!(p.owner.is_none() && p.capper.is_none() && p.kind.is_none());
    assert_eq!(p.progress, 0.0);
    assert!(!p.cut && !p.contested && p.on_point.is_empty());
    assert!(m.finish.is_none());
    let cell = 8 * m.world.game.w + 40;
    assert_eq!(m.world.game.chars[cell], b'.');
    assert!(!m.building_cells.contains_key(&cell));
    assert!(!m.world.sections.contains_key(&cell));
    let catalog = Catalog::compiled().unwrap();
    let base_income = catalog.table("CFG").unwrap().get("mpBase").unwrap().number().unwrap();
    for player in &m.start.economy.players {
        assert_eq!(player.vp, 0.0);
        assert_eq!(player.inc, base_income);
    }
    for u in m.world.game.units.values() {
        assert!(ww2_core::math::hypot(u.x - p.at.x, u.z - p.at.z) > 40.0);
        assert!(m.commands.units.contains_key(&u.id));
        assert!(m.combat.get(u.id).is_some());
        assert!(m.firing.get(u.id).is_some());
        assert!(m.abilities.get(u.id).is_some());
        assert!(m.recovery.get(u.id).is_some());
        assert!(m.logistics.get(u.id).is_none(), "logistics stays disabled");
        if m.rules.kinds[u.kind].building {
            let s = m.world.structures.iter().find(|s| s.entity_id == Some(u.id))
                .expect("canonical linked building structure");
            let cells = u.cells.as_ref().expect("canonical actor footprint");
            assert_eq!(s.cells, cells.iter().map(|&c| usize::try_from(c).unwrap()).collect::<Vec<_>>());
            for &c in cells {
                assert_eq!(m.building_cells[&(c as usize)].unit, u.id);
            }
            let producer = m.production.building(u.id).expect("canonical producer queue");
            assert!(producer.queue().next().is_none());
            assert!(producer.rows().is_empty());
        }
    }
    for cover in &m.support.covers {
        assert!(ww2_core::math::hypot(cover.at.x - p.at.x, cover.at.z - p.at.z) > cover.radius);
    }
    for &cell in m.building_cells.keys() {
        let x = (cell % m.world.game.w) as f64 * 2.0 + 1.0;
        let z = (cell / m.world.game.w) as f64 * 2.0 + 1.0;
        assert!(ww2_core::math::hypot(x - p.at.x, z - p.at.z) > 40.0);
    }
    let pop = population(m);
    assert_eq!(pop["total"].as_f64().unwrap(), pop["field"].as_f64().unwrap()
        + pop["queued"].as_f64().unwrap() + pop["reserved"].as_f64().unwrap());
    assert_eq!(pop["queued"].as_f64(), Some(0.0));
    assert_eq!(pop["cap"].as_f64(), Some(12.0));
}

fn snapshot(m: &Match, original: &BTreeSet<u32>, initial_identity: &Value) -> Value {
    invariant(m, original, initial_identity);
    let mut random = json!({"weather": null, "combat": null, "offMap": null});
    for (purpose, state) in m.start.random.snapshot().streams {
        let key = match purpose {
            StreamId::Weather => "weather",
            StreamId::Combat => "combat",
            StreamId::OffMap => "offMap",
            other => panic!("undeclared random purpose {other:?}"),
        };
        random[key] = json!(format!("{state:016x}"));
    }
    let actors = m.world.game.units.values().map(|u| {
        let control = &m.commands.units[&u.id];
        let structure = m.world.structures.iter().find(|s| s.entity_id == Some(u.id));
        json!([
            u.id, u.owner, m.rules.kinds[u.kind].name, bits(u.hp), bits(u.x), bits(u.z),
            bits(u.rot), bits(control.aim), u.built.map(bits), u.cells,
            structure.map(|s| s.id.as_str()), u.path.iter().map(|p| point(Some(*p))).collect::<Vec<_>>(),
            bits(u.ghost), u.hold_pos, control.hold_fire, control.auto_cast, control.auto_retreat,
            point(u.world_goal), control.rally_drift, bits(m.recovery.get(u.id).unwrap().reinforcement)
        ])
    }).collect::<Vec<_>>();
    let producers = m.world.game.units.values().filter(|u| m.rules.kinds[u.kind].building).map(|u| {
        let producer = m.production.building(u.id).unwrap();
        json!([u.id, producer.queue().collect::<Vec<_>>(), bits(producer.progress()), point(m.building_rallies[&u.id])])
    }).collect::<Vec<_>>();
    json!({
        "tick": m.world.game.tick, "nextId": m.next_unit_id, "seed": m.start.environment_seed,
        "clearedFixtureEquality": population(m)["total"].as_f64().unwrap() == m.world.game.units.len() as f64,
        "population": population(m).as_object().unwrap().iter().map(|(key, value)| {
            (key.clone(), if key == "globalCount" { value.clone() } else { bits(value.as_f64().unwrap()) })
        }).collect::<serde_json::Map<String, Value>>(), "actors": actors,
        "players": m.start.players.iter().zip(&m.start.economy.players).enumerate().map(|(i, (p, wallet))| json!([
            bits(wallet.mp), bits(wallet.inc), bits(wallet.vp), point(Some(p.spawn)), point(m.player_rallies[i]),
            p.support.iter().map(|(kind, value)| json!([kind, bits(*value)])).collect::<Vec<_>>()
        ])).collect::<Vec<_>>(),
        "producers": producers,
        "buildingCells": m.building_cells.iter().map(|(cell, b)| json!([cell, b.unit, b.owner, bits(b.at.x), bits(b.at.z)])).collect::<Vec<_>>(),
        "strikes": m.world.game.strikes.iter().map(|s| {
            let t = s.runtime.as_ref().unwrap();
            json!([t.kind.name(), s.owner, bits(s.x), bits(s.z), bits(s.dir), bits(t.delay), bits(t.left), bits(t.next), s.live])
        }).collect::<Vec<_>>(),
        "covers": m.support.covers.iter().map(|c| json!([c.owner, c.team, bits(c.at.x), bits(c.at.z), bits(c.radius), bits(c.remaining)])).collect::<Vec<_>>(),
        "random": random,
    })
}

fn event(event: TickEvent) -> Value {
    match event {
        TickEvent::Support(SupportEvent::Plane { kind, at, direction, owner }) => {
            assert_eq!(kind, SupportKind::Para);
            json!(["plane", kind.name(), bits(at.x), bits(at.z), bits(direction), owner])
        }
        TickEvent::Support(SupportEvent::Chutes(at)) => json!(["chutes", bits(at.x), bits(at.z)]),
        TickEvent::Support(SupportEvent::ShotDown { kind, at, direction, fighters }) => {
            json!(["shotdown", kind.name(), bits(at.x), bits(at.z), bits(direction), fighters])
        }
        other => panic!("unexpected event in peaceful pending-drop scene: {other:?}"),
    }
}

type Input = (String, Option<(usize, Value)>);

fn schedule(name: &str, hq: u32) -> Vec<Input> {
    let mut rows = Vec::new();
    let fill = if name == "whole-stick-insufficient" { 7 } else { 6 };
    let buy = json!({"t":"buy","unit":"rifle","from":hq});
    let para = json!({"t":"support","kind":"para","x":45,"z":81});
    for i in 1..=fill {
        rows.push((format!("fill-{i}"), Some((0, buy.clone()))));
    }
    if name == "whole-stick-insufficient" {
        rows.push(("whole-stick-denied".into(), Some((0, para))));
        return rows;
    }
    if name == "pending-then-intercepted" {
        rows.push(("cover-accepted".into(), Some((1, json!({"t":"support","kind":"cover","x":45,"z":81})))));
        for i in 1..=41 {
            rows.push((format!("cover-tick-{i}"), None));
        }
    }
    rows.push(("para-accepted".into(), Some((0, para))));
    rows.push(("reserved-buy-denied".into(), Some((0, buy.clone()))));
    for i in 1..=124 {
        rows.push((format!("para-tick-{i}"), None));
    }
    if name == "pending-then-intercepted" {
        rows.push(("released-buy".into(), Some((0, buy))));
    }
    rows
}

fn resolved(m: &Match, intercepted: bool, before: &BTreeSet<u32>, events: &[Value]) {
    let pop = population(m);
    assert!(m.world.game.strikes.is_empty());
    assert_eq!(pop["reserved"].as_f64(), Some(0.0));
    assert_eq!(pop["total"].as_f64(), pop["field"].as_f64());
    assert_ne!(pop["total"].as_f64().unwrap(), m.world.game.units.len() as f64,
        "The original cleared-scene pop==global-size equality is unequal here");
    let births = m.world.game.units.values().filter(|u| !before.contains(&u.id))
        .map(|u| (u.owner, m.rules.kinds[u.kind].name.as_str())).collect::<Vec<_>>();
    assert_eq!(births, if intercepted { vec![] } else { vec![(0, "rifle"), (0, "rifle"), (0, "mg")] });
    for u in m.world.game.units.values().filter(|u| !before.contains(&u.id)) {
        assert!(!m.recruit_metadata.contains_key(&u.id), "drop publication is not a producer purchase");
    }
    assert_eq!(events.iter().filter(|v| v[0] == "plane").count(), 1);
    assert_eq!(events.iter().filter(|v| v[0] == "chutes").count(), if intercepted { 0 } else { 3 });
    assert_eq!(events.iter().filter(|v| v[0] == "shotdown").count(), usize::from(intercepted));
    if intercepted {
        assert!(m.support.covers.is_empty());
        assert_eq!(events.iter().find(|v| v[0] == "shotdown").unwrap()[5], true);
        assert_eq!(pop["total"].as_f64(), Some(9.0));
    } else {
        assert_eq!(pop["total"].as_f64(), Some(12.0));
    }
}

#[test]
fn original_pending_paratrooper_population_lifecycle() {
    // Runtime input is separate from compile includes. Absent generation must
    // fail explicitly; a guard-only filter cannot qualify this missing replay.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/base-pending-paratroopers.json");
    let f: Value = serde_json::from_str(&std::fs::read_to_string(path)
        .expect("Generate the pinned pending-drop fixture under a future Root slot first")).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(f["seed"], 701);
    assert_eq!(f["scope"], "Complete selected pending-drop predicates on a retained admitted arena; original cleared-fixture pop==global-size is unequal here and requires independent correspondence review");
    assert_eq!(f["opts"], json!({"mode":"conquest","weather":false,"weatherSeed":1,"tech":false,"supply":false,"logistics":false,"army":"standard","sandbox":false}));
    assert_eq!(f["inputs"], json!({"names":["a","b"],"shuffle":false,"teams":[0,1],"factions":[0,1]}));
    assert_eq!(f["setup"], json!({"mpPerSeat":10000,"originalControls":{"holdFire":true,"holdPos":true,"auto":false,"autoRetreat":false}}));
    assert_eq!(f["map"], json!({"name":"BASE pending-drop retained arena","w":80,"h":80,"rows":vec![".".repeat(80);80],"spawns":[{"x":12,"y":40},{"x":68,"y":40}],"points":[{"x":40,"y":8}],"defend":[0]}));
    let cases = f["cases"].as_array().unwrap();
    let expected_names = ["whole-stick-insufficient", "pending-then-delivered", "pending-then-intercepted"];
    assert_eq!(cases.len(), expected_names.len());
    let (mut total_commands, mut total_ticks) = (0, 0);
    for (c, name) in cases.iter().zip(expected_names) {
        assert_eq!(c["name"], name);
        let mut m = make(&f);
        let original = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
        assert_eq!(original.len(), 10);
        assert_eq!(json!(original), c["originalIds"]);
        let initial_identity = identity(&m, &original);
        assert_eq!(initial_identity, c["originalIdentity"]);
        let hq = m.world.game.units.values().find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq").unwrap().id;
        assert_eq!(c["hq"], hq);
        assert_eq!(c["constructorDraws"], 1);
        assert_eq!(population(&m)["field"].as_f64(), Some(3.0));
        assert_eq!(snapshot(&m, &original, &initial_identity), c["initial"], "{name}: canonical constructor");
        let inputs = schedule(name, hq);
        let records = c["records"].as_array().unwrap();
        assert_eq!(records.len(), inputs.len());
        let mut events = Vec::new();
        let mut before_drop = BTreeSet::new();
        let (mut commands, mut ticks) = (0, 0);
        for (r, (label, input)) in records.iter().zip(inputs) {
            assert_eq!(r["label"], label);
            let before = snapshot(&m, &original, &initial_identity);
            let ids = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
            let next = m.next_unit_id;
            let wallets = m.start.economy.players.iter().map(|p| p.mp).collect::<Vec<_>>();
            let actual_events;
            if let Some((slot, raw)) = input {
                assert_eq!(r["op"], "command");
                assert_eq!(r["slot"], slot);
                assert_eq!(r["cmd"], raw);
                let denial = m.execute_command(slot, &Command::from_json(&raw).unwrap(), false).unwrap();
                let expected_denial = if label == "whole-stick-denied" || label == "reserved-buy-denied" { Some("pop") } else { None };
                assert_eq!(denial.map(|d| d.name()), expected_denial, "{name}:{label}");
                assert_eq!(json!(expected_denial), r["result"]);
                assert!(m.support.events.is_empty());
                actual_events = Vec::new();
                commands += 1;
                if expected_denial.is_some() {
                    assert_eq!(snapshot(&m, &original, &initial_identity), before);
                } else if label.starts_with("fill-") || label == "released-buy" {
                    assert_eq!(m.world.game.units.len(), ids.len() + 1);
                    let u = &m.world.game.units[&next];
                    assert_eq!((u.owner, m.rules.kinds[u.kind].name.as_str()), (0, "rifle"));
                    assert_eq!(m.next_unit_id, next + 1);
                    assert!(m.recruit_metadata.contains_key(&next));
                    assert_eq!(m.start.economy.players[0].mp, wallets[0] - 100.0);
                    if label == "released-buy" {
                        assert_eq!(population(&m)["total"].as_f64(), Some(12.0 - 3.0 + 1.0));
                    }
                } else if label == "para-accepted" {
                    before_drop = ids;
                    assert_eq!(m.world.game.units.len(), before_drop.len());
                    assert_eq!(m.next_unit_id, next);
                    assert_eq!(m.start.economy.players[0].mp, wallets[0] - 260.0);
                    assert_eq!(m.world.game.strikes.len(), 1);
                    assert!(!m.world.game.strikes[0].live);
                    assert_eq!(population(&m)["reserved"].as_f64(), Some(3.0));
                    assert_eq!(population(&m)["total"].as_f64(), Some(12.0));
                } else if label == "cover-accepted" {
                    assert_eq!(m.start.economy.players[1].mp, wallets[1] - 120.0);
                    assert_eq!(m.world.game.units.len(), ids.len());
                    assert_eq!(m.next_unit_id, next);
                } else {
                    panic!("undeclared command label {label}");
                }
            } else {
                assert_eq!(r["op"], "tick");
                let report = m.advance_tick().unwrap_or_else(|e| panic!("{name}:{label}: {e}"));
                assert!(report.commands.is_empty());
                assert!(report.retired.is_empty() && report.world_removed.is_empty());
                for phase in [TickPhase::Support, TickPhase::AntiAir, TickPhase::SupportTimers, TickPhase::PointCaptureIncome] {
                    assert!(report.phases.contains(&phase));
                }
                actual_events = report.events.into_iter().map(event).collect::<Vec<_>>();
                assert!(m.support.events.is_empty());
                ticks += 1;
                if label == "cover-tick-41" {
                    assert!(m.world.game.strikes.is_empty());
                    assert_eq!(m.support.covers.len(), 1);
                    let cover = &m.support.covers[0];
                    assert_eq!((cover.owner, cover.team, cover.at.x, cover.at.z), (1, 1, 45.0, 81.0));
                    assert!(cover.remaining > 0.0);
                }
            }
            assert_eq!(json!(actual_events), r["events"], "{name}:{label}: real ordered emissions");
            events.extend(actual_events);
            assert_eq!(snapshot(&m, &original, &initial_identity), r["expected"], "{name}:{label}: actual canonical projection");
            if label == "para-tick-124" {
                resolved(&m, name == "pending-then-intercepted", &before_drop, &events);
            }
        }
        assert_eq!(snapshot(&m, &original, &initial_identity), c["final"]);
        let predicted = match name {
            "whole-stick-insufficient" => (8, 0),
            "pending-then-delivered" => (8, 124),
            "pending-then-intercepted" => (10, 165),
            _ => unreachable!(),
        };
        assert_eq!((commands, ticks), predicted);
        assert_eq!(json!(commands), c["commands"]);
        assert_eq!(json!(ticks), c["ticks"]);
        total_commands += commands;
        total_ticks += ticks;
    }
    assert_eq!((total_commands, total_ticks), (26, 289));
}
