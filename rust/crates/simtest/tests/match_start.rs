//! Original createGame startup, with troop capture before starting bases are stamped.
use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode, Technology},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig, StartPlan},
    rules::Rules,
    weather::WeatherChoice,
};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn runs(values: impl IntoIterator<Item = Value>) -> Value {
    let mut out: Vec<[Value; 2]> = Vec::new();
    for value in values {
        if let Some(last) = out.last_mut().filter(|r| r[0] == value) {
            last[1] = json!(last[1].as_u64().unwrap() + 1);
        } else {
            out.push([value, json!(1)]);
        }
    }
    json!(out)
}
fn equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, a)| b.get(k).is_some_and(|b| equivalent(a, b)))
        }
        _ => a == b,
    }
}
fn terrain(instance: &Match) -> Value {
    use ww2_core::world::Recipient;
    let w = &instance.world;
    let g = &w.game;
    let row = |r: &ww2_core::world::CellRow| {
        let d = &r.data;
        let mut data = json!({"ground":char::from(d.ground).to_string(),"object":char::from(d.object).to_string(),"material":d.material,"groundMaterial":d.ground_material,"groundWear":d.ground_wear});
        if let Some(front) = d.trench_front {
            data["trenchFront"] = json!(front);
        }
        if d.mine {
            data["mine"] = json!(true);
            if let Some(m) = &d.mine_material {
                data["mineMaterial"] = json!(m);
            }
        }
        if let Some(s) = &d.section {
            data["section"] = json!({"id":s.id,"structureId":s.structure_id,"state":s.state,"hp":s.hp,"maxHp":s.max_hp,"material":s.material,"anchor":s.anchor});
        }
        json!([
            r.cell,
            char::from(r.ch).to_string(),
            r.height,
            r.state,
            data
        ])
    };
    let buildings: Vec<_> = w
        .structures
        .iter()
        .filter(|s| s.kind == "building")
        .map(|s| {
            let sections: Vec<_> = s
                .section_ids
                .iter()
                .zip(&s.cells)
                .map(|(id, c)| {
                    let (m, h) = w.section_by_id(id).unwrap();
                    json!([
                        m.id,
                        m.structure_id,
                        c,
                        bits(h.hp),
                        bits(h.max_hp.unwrap()),
                        m.material,
                        m.anchor,
                        m.supports,
                        "intact",
                        m.rubble
                    ])
                })
                .collect();
            json!([s.id, s.entity_id, s.cells, sections])
        })
        .collect();
    json!({"chars":runs(g.chars.iter().map(|c|json!(char::from(*c).to_string()))),
        "ground":runs(g.ground.as_ref().unwrap().iter().map(|c|json!(char::from(*c).to_string()))),
        "objects":runs(g.objects.as_ref().unwrap().iter().map(|c|json!(char::from(*c).to_string()))),
        "flags":runs(g.flags.iter().map(|v|json!(v))),"height":g.height.as_ref().map(|h|runs(h.iter().map(|v|json!(v)))),
        "hp":runs(g.cell_hp.iter().map(|v|json!(v.to_bits()))),"wear":w.cell_log().iter().map(|r|json!([r.cell,g.wear.as_ref().unwrap()[r.cell].to_bits()])).collect::<Vec<_>>(),
        "state":runs(w.cell_state.iter().map(|v|json!(v))),
        "versions":[g.terrain_version as u64,w.versions.obstruction,w.versions.infantry,w.versions.vehicle,w.versions.naval],
        "log":w.cell_log().iter().map(row).collect::<Vec<_>>(),"newCells":w.new_cells().iter().map(row).collect::<Vec<_>>(),"journal":w.journal(),
        "buildingCells":instance.building_cells.iter().map(|(c,b)|json!([c,b.unit,b.owner,bits(b.at.x),bits(b.at.z)])).collect::<Vec<_>>(),
        "pending":(0..instance.start.players.len()).map(|i|w.pending(Recipient::Player(i)).unwrap()).collect::<Vec<_>>(),"buildings":buildings})
}
fn options(c: &Value, map_key: &str) -> MatchOptions {
    MatchOptions {
        seed: c["seed"].as_str().unwrap().parse().unwrap(),
        mode: Mode::Conquest,
        players: c["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| PlayerConfig {
                name: p["name"].as_str().unwrap().to_owned(),
                team: p["team"].as_i64().unwrap() as i32,
                faction: Faction::try_from(p["faction"].as_u64().unwrap() as u8).unwrap(),
            })
            .collect(),
        shuffle: c["shuffle"].as_bool().unwrap(),
        army: match c["army"].as_str().unwrap() {
            "standard" => ArmySize::Standard,
            "large" => ArmySize::Large,
            "massive" => ArmySize::Massive,
            "endless" => ArmySize::Endless,
            _ => unreachable!(),
        },
        weather: WeatherChoice::from_reference(c["weather"].as_str()),
        weather_seed: c["weatherSeed"].as_u64().map(|v| v as u32),
        living_weather: c["living"].as_bool().unwrap(),
        map_key: Some(map_key.to_owned()),
        tech: c["tech"].as_bool().unwrap(),
        supply: true,
        logistics: false,
        sandbox: false,
    }
}
#[test]
fn startup_plan_and_initial_force_match_original_create_game() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-start.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let rules = Rules::compiled().unwrap();
    let mut troop_count = 0;
    for (i, c) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &fixture["maps"][c["map"].as_u64().unwrap() as usize];
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&input["map"]).unwrap()).unwrap();
        let o = options(c, input["name"].as_str().unwrap());
        let plan = StartPlan::prepare(&map, &o, &rules).unwrap();
        let expected = &c["expected"];
        let players: Vec<_> = plan.players.iter().zip(&plan.economy.players).map(|(p,e)| json!({
            "name": p.config.name, "team": p.config.team, "faction": p.config.faction as u8,
            "spawn": [bits(p.spawn.x), bits(p.spawn.z)], "mp": bits(e.mp), "inc": bits(e.inc), "vp": bits(e.vp),
            "support": p.support.iter().map(|(k,v)| (k.clone(), bits(*v))).collect::<std::collections::BTreeMap<_,_>>(), "tier": o.tech.then_some(1),
        })).collect();
        assert_eq!(json!(players), expected["players"], "case {i} players");
        let points: Vec<_> = plan.points.iter().map(|p| json!({ "id":p.id, "at":[bits(p.at.x),bits(p.at.z)], "vp":bits(p.vp), "mp":bits(p.mp), "kind":p.kind, "needs":p.needs.map_or(-1, |n| n as i64) })).collect();
        assert_eq!(json!(points), expected["points"], "case {i} points");
        assert_eq!(bits(plan.win_vp), expected["winVp"], "case {i} VP goal");
        assert_eq!(
            json!({"now":plan.weather.now.name(), "next":plan.weather.next.map(|n|n.name()), "at":plan.weather.at}),
            expected["weather"],
            "case {i} weather"
        );
        assert_eq!(
            json!([bits(plan.wind.0), bits(plan.wind.1)]),
            expected["wind"],
            "case {i} wind"
        );
        assert_eq!(bits(plan.initial_rain), expected["rain"]);
        assert_eq!(
            plan.initial_wet.map(bits).unwrap_or(Value::Null),
            expected["wet"]
        );
        assert_eq!(
            plan.living
                .map(|l| json!({ "seed":plan.environment_seed, "raining":l.raining, "next":bits(l.next) }))
                .unwrap_or(Value::Null),
            expected["living"],
            "case {i} living weather"
        );
        let mut game = map.initialize_structures().unwrap().query_game();
        let next_id = plan.spawn_initial_force(&mut game, &rules).unwrap();
        let units = expected["startingUnits"].as_array().unwrap();
        assert_eq!(game.units.len(), units.len());
        assert_eq!(next_id as usize, units.len() + 1);
        for raw in units {
            let unit =
                simtest::load::unit(raw, &rules, plan.players.len(), game.w * game.h).unwrap();
            assert_eq!(
                simtest::replay::diff(&game.units[&unit.id], &unit),
                None,
                "case {i} unit {}",
                unit.id
            );
            troop_count += 1;
        }
        let initial_wear = game.wear.as_ref().unwrap().clone();
        let instance = Match::new(&map, o).unwrap();
        let game = &instance.world.game;
        let units = expected["finalUnits"].as_array().unwrap();
        assert_eq!(game.units.len(), units.len(), "case {i} final count");
        assert_eq!(instance.next_unit_id as usize, units.len() + 1);
        for raw in units {
            let unit =
                simtest::load::unit(raw, &rules, plan.players.len(), game.w * game.h).unwrap();
            assert_eq!(
                simtest::replay::diff(&game.units[&unit.id], &unit),
                None,
                "case {i} final unit {}",
                unit.id
            );
        }
        assert_eq!(
            instance.start.environment_seed,
            expected["environmentSeed"].as_u64().unwrap() as u32,
            "case {i} environment RNG"
        );
        let production: Vec<_> = game
            .units
            .values()
            .filter(|u| u.cells.is_some())
            .map(|u| {
                let p = instance.production.building(u.id).unwrap();
                assert!(p.jobs().is_empty());
                json!([
                    u.id,
                    p.queue().collect::<Vec<_>>(),
                    [],
                    bits(p.progress()),
                    instance.building_rallies[&u.id].map(|p| [p.x, p.z])
                ])
            })
            .collect();
        assert_eq!(
            json!(production),
            expected["production"],
            "case {i} production"
        );
        let tech:Vec<_>=(0..instance.start.players.len()).map(|slot| {
            let p=instance.production.player(slot).unwrap();
            assert!(p.jobs().is_empty());
            let t=p.tech();
            if !instance.options.tech {Value::Null} else {json!([t.tier(),{"inf":t.level(Technology::Infantry),"guns":t.level(Technology::Guns),"armor":t.level(Technology::Armor)},[]])}
        }).collect();
        assert_eq!(json!(tech), expected["tech"], "case {i} tech");
        let scenario = instance
            .scenario
            .as_ref()
            .map_or(Value::Null, |s| s.state_json());
        assert!(
            equivalent(&scenario, &expected["scenario"]),
            "case {i} scenario"
        );
        let actual = terrain(&instance);
        for (key, value) in expected["finalTerrain"].as_object().unwrap() {
            assert!(equivalent(&actual[key], value), "case {i} terrain {key}");
        }
        let changed: std::collections::BTreeSet<_> =
            instance.world.cell_log().iter().map(|r| r.cell).collect();
        for (cell, initial) in initial_wear.iter().enumerate() {
            if !changed.contains(&cell) {
                assert_eq!(
                    initial.to_bits(),
                    game.wear.as_ref().unwrap()[cell].to_bits(),
                    "case {i} untouched wear {cell}"
                );
            }
        }
    }
    assert_eq!(fixture["maps"].as_array().unwrap().len(), 36);
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 63);
    assert!(troop_count > 600);
}

#[test]
fn startup_rejects_incomplete_features_and_keeps_failed_force_setup_atomic() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-start.json")).unwrap();
    let input = &fixture["maps"][0];
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&input["map"]).unwrap()).unwrap();
    let base = options(&fixture["cases"][0], input["name"].as_str().unwrap());
    let mut invalid = Vec::new();
    let mut o = base.clone();
    o.mode = Mode::Classic;
    invalid.push(o);
    let mut o = base.clone();
    o.players.clear();
    invalid.push(o);
    let mut o = base.clone();
    o.players[0].team = 6;
    invalid.push(o);
    let mut o = base.clone();
    o.living_weather = false;
    o.weather = WeatherChoice::from_reference(Some("rain"));
    invalid.push(o);
    for o in invalid {
        assert!(Match::new(&map, o).is_err());
    }
    let rules = Rules::compiled().unwrap();
    let plan = StartPlan::prepare(&map, &base, &rules).unwrap();
    let mut game = map.initialize_structures().unwrap().query_game();
    game.w += 1;
    let before = format!("{game:?}");
    assert!(plan.spawn_initial_force(&mut game, &rules).is_err());
    assert_eq!(format!("{game:?}"), before);
}
