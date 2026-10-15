//! Selected BASE predicates over authentic constructors and the sole tick runner.
//! The Source recorder preserves original actors and produces the fixture after a slot grant.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::Command,
    match_firing::LaunchEvent,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::{TickEvent, TickPhase},
    random::StreamId,
    vision,
    weather::WeatherChoice,
};

fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}

fn refresh(m: &mut Match) {
    m.grid.rebuild(&m.world.game.units);
    vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    for owner in 0..m.world.game.players.len() {
        m.refresh_recipient_routing(owner).unwrap();
    }
}

fn make(f: &Value, c: &Value) -> Match {
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
            tech: true,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    assert_eq!(
        json!(m.world.game.units.keys().copied().collect::<Vec<_>>()),
        c["originalActors"]
    );
    for u in m.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    for ctl in m.commands.units.values_mut() {
        ctl.hold_fire = true;
        ctl.auto_cast = false;
        ctl.auto_retreat = false;
    }
    m.start.economy.players[0].mp = 10000.0;
    let builder = c["initialAliases"]["builder"].as_u64().unwrap() as u32;
    let u = m.world.game.units.get_mut(&builder).unwrap();
    u.x = 46.0;
    u.z = 80.0;
    refresh(&mut m);
    m
}

fn snapshot(m: &Match, aliases: &BTreeMap<String, u32>, retired: &[(i64, u32)]) -> Value {
    assert_isolated_point(m);
    let g = &m.world.game;
    let actors = g
        .units
        .values()
        .map(|u| {
            let structure = m
                .world
                .structures
                .iter()
                .find(|s| s.entity_id == Some(u.id));
            if m.rules.kinds[u.kind].building {
                let structure =
                    structure.expect("actual building must retain its canonical structure");
                let cells = u.cells.as_ref().expect("actual building footprint");
                assert_eq!(structure.cells, *cells);
                assert_eq!(structure.entity_id, Some(u.id));
                for &cell in cells {
                    assert_eq!(m.building_cells[&cell].unit, u.id);
                }
            }
            json!([
                u.id,
                u.owner,
                m.rules.kinds[u.kind].name,
                bits(u.hp),
                bits(u.x),
                bits(u.z),
                u.built.map(bits),
                u.cells,
                structure.map(|s| s.id.as_str())
            ])
        })
        .collect::<Vec<_>>();
    let lookout = aliases
        .get("site")
        .and_then(|id| g.units.get(id))
        .filter(|u| m.rules.kinds[u.kind].name == "tower")
        .map(|u| bits(vision::vision_of(g, &m.rules, u)));
    let combat = m
        .start
        .random
        .snapshot()
        .streams
        .into_iter()
        .find(|(purpose, _)| *purpose == StreamId::Combat)
        .map(|(_, state)| format!("{state:016x}"));
    json!({
        "tick": g.tick, "nextId": m.next_unit_id, "seed": m.start.environment_seed,
        "actors": actors, "aliases": aliases,
        "players": g.players.iter().enumerate().map(|(i, p)| json!([
            bits(m.start.economy.players[i].mp), m.production.player(i).unwrap().tech().tier(), p.visible,
            bits(m.start.economy.players[i].inc), bits(m.start.economy.players[i].vp)
        ])).collect::<Vec<_>>(),
        "point": m.start.points.iter().map(|p| json!([
            bits(p.at.x), bits(p.at.z), p.owner.map_or(-1, |n| n as i64),
            p.capper.map_or(-1, |n| n as i64), bits(p.progress), p.cut, p.contested, p.on_point
        ])).collect::<Vec<_>>(),
        "buildingCells": m.building_cells.iter().map(|(cell, b)| json!([
            cell, b.unit, b.owner, bits(b.at.x), bits(b.at.z)
        ])).collect::<Vec<_>>(),
        "recovery": g.units.values().map(|u| json!([
            u.id, bits(m.recovery.get(u.id).unwrap().reinforcement)
        ])).collect::<Vec<_>>(),
        "lookout": lookout, "retired": retired, "combatState": combat,
    })
}

fn assert_isolated_point(m: &Match) {
    assert_eq!(m.start.points.len(), 1);
    let p = &m.start.points[0];
    assert_eq!((p.at.x, p.at.z), (81.0, 17.0));
    let point_cell = 8 * m.world.game.w + 40;
    assert_eq!(m.world.game.chars[point_cell], b'.');
    assert!(!m.building_cells.contains_key(&point_cell));
    assert!(!m.world.sections.contains_key(&point_cell));
    assert!(p.owner.is_none() && p.capper.is_none() && p.kind.is_none());
    assert_eq!(p.progress, 0.0);
    assert!(!p.cut && !p.contested && p.on_point.is_empty());
    assert!(m.finish.is_none());
    let cfg = Catalog::compiled().unwrap().table("CFG").unwrap();
    let base = cfg.get("mpBase").unwrap().number().unwrap()
        * cfg
            .get("armies")
            .unwrap()
            .get("standard")
            .unwrap()
            .get("income")
            .unwrap()
            .number()
            .unwrap();
    for player in &m.start.economy.players {
        assert_eq!(player.vp, 0.0);
        assert_eq!(player.inc, base);
    }
    for u in m.world.game.units.values() {
        assert!(ww2_core::math::hypot(u.x - p.at.x, u.z - p.at.z) > 40.0);
    }
    for &cell in m.building_cells.keys() {
        let x = (cell % m.world.game.w) as f64 * 2.0 + 1.0;
        let z = (cell / m.world.game.w) as f64 * 2.0 + 1.0;
        assert!(ww2_core::math::hypot(x - p.at.x, z - p.at.z) > 40.0);
    }
}

fn unit<'a>(m: &'a Match, aliases: &BTreeMap<String, u32>, key: &str) -> &'a ww2_core::game::Unit {
    &m.world.game.units[&aliases[key]]
}

#[test]
fn original_base_building_acceptance_checks() {
    // Generation must precede Cargo under the coordinator's slot grant. A draft
    // without the generated fixture fails explicitly instead of claiming coverage.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/base-ordinary-checks.json");
    let f: Value = serde_json::from_str(
        &std::fs::read_to_string(path)
            .expect("run the pinned BASE recorder under an authorized slot first"),
    )
    .unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(f["seed"], 701);
    assert_eq!(
        f["scope"],
        "Selected predicates on an admitted ordinary arena; not a literal replay of the zero-point Source maps"
    );
    assert_eq!(
        f["opts"],
        json!({"mode":"conquest","weather":false,"weatherSeed":1,"tech":true,"supply":false,"logistics":false,"army":"standard","sandbox":false})
    );
    assert_eq!(
        f["inputs"],
        json!({"names":["a","b"],"shuffle":false,"teams":[0,1],"factions":[0,1]})
    );
    assert_eq!(
        f["setup"],
        json!({"allUnits":{"holdFire":true,"holdPos":true,"auto":false,"autoRetreat":false},"builder":{"x":46,"z":80},"player0MP":10000})
    );
    assert_eq!(f["cases"].as_array().unwrap().len(), 3);
    let mut names = Vec::new();
    for c in f["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        names.push(name);
        let required: &[&str] = match name {
            "hq-retirement-rebuild-and-engineer-denial" => &[
                "engineer-denied",
                "injured-at-live-hq",
                "live-hq-positive-control",
                "injured-before-scuttle",
                "scuttle-original-hq",
                "hq-retired",
                "fresh-post-retirement-sixty-ticks",
                "hq-rebuilt",
            ],
            "tier-one-tower-twelve-and-sixty-metre-vision" => &[
                "t1-tower-built",
                "pause-tower",
                "enemy-fifty-five-metres",
                "unfinished-vision",
                "resume-tower",
                "tower-completed",
                "finished-vision",
            ],
            "unfinished-and-finished-pillbox-fire" => &[
                "t1-pillbox-denied",
                "research-company-hq",
                "company-hq-completed",
                "pillbox-built",
                "pause-pillbox",
                "enemy-in-front",
                "unfinished-no-fire",
                "resume-pillbox",
                "pillbox-completed",
                "finished-damages-target",
            ],
            other => panic!("unrecognized BASE scene {other}"),
        };
        assert_eq!(
            c["records"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["label"].as_str().unwrap())
                .collect::<Vec<_>>(),
            required
        );
        let mut m = make(&f, c);
        let mut aliases: BTreeMap<String, u32> =
            serde_json::from_value(c["initialAliases"].clone()).unwrap();
        let original_hq_cells = unit(&m, &aliases, "hq").cells.clone().unwrap();
        let mut retired = Vec::new();
        let mut ticks = 0;
        assert_eq!(
            snapshot(&m, &aliases, &retired),
            c["initial"],
            "{name}: original constructor"
        );
        for r in c["records"].as_array().unwrap() {
            let label = r["label"].as_str().unwrap();
            let before_hp = unit(&m, &aliases, "builder").hp;
            let target_hp = unit(&m, &aliases, "target").hp;
            let wallet = m.start.economy.players[0].mp;
            let before_ids = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
            let mut shooters = BTreeSet::new();
            match r["op"].as_str().unwrap() {
                "command" => {
                    let command = Command::from_json(&r["cmd"]).unwrap();
                    let result = m.execute_command(0, &command, false).unwrap();
                    assert_eq!(
                        json!(result.map(|d| d.name())),
                        r["expect"],
                        "{name}:{label}: command result"
                    );
                    if let Some(key) = r["as"].as_str() {
                        let fresh = m
                            .world
                            .game
                            .units
                            .keys()
                            .copied()
                            .filter(|id| !before_ids.contains(id))
                            .collect::<Vec<_>>();
                        assert_eq!(fresh.len(), 1);
                        aliases.insert(key.into(), fresh[0]);
                    }
                }
                "prime" => {
                    let id = r["id"].as_u64().unwrap() as u32;
                    assert!(c["originalActors"].as_array().unwrap().contains(&json!(id)));
                    let u = m.world.game.units.get_mut(&id).unwrap();
                    assert!(!m.rules.kinds[u.kind].building);
                    for (field, value) in r["fields"].as_object().unwrap() {
                        let value = value.as_f64().unwrap();
                        match field.as_str() {
                            "x" => u.x = value,
                            "z" => u.z = value,
                            "hp" => u.hp = value,
                            other => panic!("unauthorized scene field {other}"),
                        }
                    }
                    refresh(&mut m);
                }
                "steps" => {
                    let count = r["count"].as_u64().unwrap();
                    assert!(count <= r["limit"].as_u64().unwrap());
                    match label {
                        "live-hq-positive-control" => assert_eq!(count, 40),
                        "hq-retired" => assert_eq!(count, 1),
                        "unfinished-vision" | "finished-vision" => assert_eq!(count, 20),
                        "unfinished-no-fire" => assert_eq!(count, 80),
                        "finished-damages-target" => assert_eq!(count, 120),
                        _ => {}
                    }
                    if label == "fresh-post-retirement-sixty-ticks" {
                        assert_eq!(count, 60);
                        assert!(!m.world.game.units.contains_key(&aliases["hq"]));
                        let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
                        let fresh = m.prepare_recovery(&order).unwrap();
                        assert_eq!(fresh.units(), order.as_slice());
                        assert!(!fresh.units().contains(&aliases["hq"]));
                    }
                    for _ in 0..count {
                        let old_tick = m.world.game.tick;
                        let report = m
                            .advance_tick()
                            .unwrap_or_else(|e| panic!("{name}:{label}: {e}"));
                        assert_isolated_point(&m);
                        assert_eq!(report.tick, old_tick + 1);
                        for phase in [
                            TickPhase::Firing,
                            TickPhase::Construction,
                            TickPhase::Recovery,
                            TickPhase::Reinforcement,
                            TickPhase::Death,
                            TickPhase::PointCaptureIncome,
                            TickPhase::Victory,
                        ] {
                            assert!(
                                report.phases.contains(&phase),
                                "{name}:{label}: missing {phase:?}"
                            );
                            assert!(!report.unavailable.contains(&phase));
                        }
                        for removed in &report.retired {
                            retired.push((report.tick, removed.unit.id));
                        }
                        for event in &report.events {
                            if let TickEvent::Firing(LaunchEvent::Flight { shooter, .. }) = event {
                                shooters.insert(*shooter);
                            }
                        }
                        if label == "fresh-post-retirement-sixty-ticks" {
                            assert_eq!(unit(&m, &aliases, "builder").hp, before_hp);
                            assert!(!m.world.game.units.contains_key(&aliases["hq"]));
                        }
                        if label == "unfinished-no-fire" {
                            assert_eq!(unit(&m, &aliases, "target").hp, target_hp);
                            assert!(unit(&m, &aliases, "site").built.unwrap() < 1.0);
                            assert!(!shooters.contains(&aliases["site"]));
                        }
                        ticks += 1;
                    }
                    if r["condition"] == "finished" {
                        assert_eq!(unit(&m, &aliases, "site").built, Some(1.0));
                    } else if r["condition"] == "tier2" {
                        assert_eq!(m.production.player(0).unwrap().tech().tier(), 2);
                    }
                }
                op => panic!("unrepresented action {op}"),
            }
            assert_eq!(
                snapshot(&m, &aliases, &retired),
                r["expected"],
                "{name}:{label}"
            );
            match label {
                "engineer-denied" => {
                    assert_eq!(
                        m.world.game.units.keys().copied().collect::<BTreeSet<_>>(),
                        before_ids
                    );
                    assert_eq!(m.start.economy.players[0].mp, wallet);
                }
                "live-hq-positive-control" => assert!(unit(&m, &aliases, "builder").hp > before_hp),
                "hq-retired" => {
                    assert!(!m.world.game.units.contains_key(&aliases["hq"]));
                    assert!(retired.iter().any(|(_, id)| *id == aliases["hq"]));
                    assert!(
                        original_hq_cells
                            .iter()
                            .all(|c| m.world.game.chars[*c] == b'R')
                    );
                }
                "hq-rebuilt" => {
                    let site = unit(&m, &aliases, "site");
                    assert_ne!(site.id, aliases["hq"]);
                    assert_eq!(m.rules.kinds[site.kind].name, "hq");
                    assert_eq!(site.cells.as_ref().unwrap().len(), 9);
                    assert_eq!(site.built, Some(0.0));
                    assert_eq!(site.hp, 750.0);
                    assert_eq!(m.start.economy.players[0].mp, wallet - 200.0);
                    assert_eq!(
                        m.commands.units[&aliases["builder"]].build_target,
                        Some(site.id)
                    );
                }
                "t1-tower-built" => {
                    assert_eq!(m.production.player(0).unwrap().tech().tier(), 1);
                    let tower = unit(&m, &aliases, "site");
                    assert_eq!(tower.cells.as_ref().unwrap().len(), 1);
                    assert!(!m.rules.kinds[tower.kind].weapon);
                }
                "unfinished-vision" | "finished-vision" => {
                    let finished = label == "finished-vision";
                    let tower = unit(&m, &aliases, "site");
                    assert_eq!(
                        vision::vision_of(&m.world.game, &m.rules, tower),
                        if finished { 60.0 } else { 12.0 }
                    );
                    assert_eq!(m.world.game.players[0].sees(aliases["target"]), finished);
                    let target = unit(&m, &aliases, "target");
                    assert_eq!((target.x - tower.x, target.z - tower.z), (55.0, 0.0));
                }
                "finished-damages-target" => {
                    assert!(unit(&m, &aliases, "target").hp < target_hp);
                    assert!(shooters.contains(&aliases["site"]));
                }
                _ => {}
            }
        }
        assert_eq!(ticks, c["ticks"].as_i64().unwrap());
        let missing = c["originalActors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap() as u32)
            .filter(|id| !m.world.game.units.contains_key(id))
            .collect::<Vec<_>>();
        assert_eq!(
            missing,
            if name == "hq-retirement-rebuild-and-engineer-denial" {
                vec![aliases["hq"]]
            } else {
                vec![]
            }
        );
    }
    assert_eq!(
        names,
        [
            "hq-retirement-rebuild-and-engineer-denial",
            "tier-one-tower-twelve-and-sixty-metre-vision",
            "unfinished-and-finished-pillbox-fire"
        ]
    );
}
