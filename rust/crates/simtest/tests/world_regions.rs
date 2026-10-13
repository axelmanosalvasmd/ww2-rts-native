//! Original World helper transitions on a real native World Match.
use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::{ArmySize, PlayerEconomy},
    game::{LOCAL_DEFENDER_OWNER, Point},
    grid::SpatialGrid,
    map::{AuthoritativeMap, Spawn},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_world::{RegionState, ResourceNode},
    random::MatchRandom,
    weather::WeatherChoice,
    world_map::{WorldDefinition, WorldMapInput, WorldRegion, WorldSize},
};

fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}
fn owner(v: usize) -> i64 {
    if v == LOCAL_DEFENDER_OWNER {
        -1
    } else {
        v as i64
    }
}
fn at(def: &Value) -> Point {
    Point {
        x: (def["x"].as_f64().unwrap() + 0.5) * 2.0,
        z: (def["y"].as_f64().unwrap() + 0.5) * 2.0,
    }
}
fn army(v: &Value) -> ArmySize {
    match v.as_str().unwrap() {
        "standard" => ArmySize::Standard,
        "large" => ArmySize::Large,
        "massive" => ArmySize::Massive,
        "endless" => ArmySize::Endless,
        _ => panic!("unknown army"),
    }
}
fn make_match(f: &Value, teams: &[Value]) -> Match {
    let defs = f["definitions"].as_array().unwrap();
    let homes: Vec<_> = (0..teams.len())
        .map(|i| if i + 1 == teams.len() { 63 } else { i })
        .collect();
    let regions = defs
        .iter()
        .enumerate()
        .map(|(i, r)| WorldRegion {
            id: r["id"].as_i64().unwrap() as i32,
            x: r["x"].as_u64().unwrap() as usize,
            y: r["y"].as_u64().unwrap() as usize,
            bounds: [i % 8 * 64, i / 8 * 64, i % 8 * 64 + 64, i / 8 * 64 + 64],
            home: homes.iter().position(|h| *h == i),
            kind: Some(r["kind"].as_str().unwrap().into()),
            name: None,
            biome: None,
            team: None,
        })
        .collect();
    let map = AuthoritativeMap::from_world(WorldMapInput {
        name: "World region phase oracle".into(),
        width: 512,
        height: 512,
        chars: vec![b'.'; 512 * 512],
        heights: Some(vec![0; 512 * 512]),
        layers: None,
        buildings: Vec::new(),
        spawns: homes
            .iter()
            .map(|i| Spawn {
                x: defs[*i]["x"].as_u64().unwrap() as usize,
                y: defs[*i]["y"].as_u64().unwrap() as usize,
                assault: false,
            })
            .collect(),
        geography: WorldDefinition {
            size: WorldSize::Huge,
            region_cells: 64,
            regions,
            region_map: Some(
                (0..512 * 512)
                    .map(|c| ((c / 512) / 64 * 8 + (c % 512) / 64) as i16)
                    .collect(),
            ),
            biomes: None,
            seed: None,
            generator_version: None,
            metadata: Default::default(),
        },
    })
    .unwrap();
    let options = MatchOptions {
        seed: 9182,
        mode: Mode::World,
        players: teams
            .iter()
            .enumerate()
            .map(|(slot, t)| PlayerConfig {
                name: format!("Commander {slot}"),
                team: t.as_i64().unwrap() as i32,
                faction: Faction::Usa,
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: None,
        living_weather: false,
        map_key: None,
        tech: false,
        supply: true,
        logistics: false,
        sandbox: false,
    };
    Match::new_world(&map, options, MatchRandom::new(9182)).unwrap()
}
fn update_unit(m: &mut Match, raw: &Value, new: bool) {
    let id = raw["id"].as_u64().unwrap() as u32;
    if new {
        let slot = raw["owner"].as_i64().unwrap();
        let u = ww2_core::unit_spawn::initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            m.rules.kind(raw["type"].as_str().unwrap()).unwrap(),
            id,
            if slot < 0 {
                LOCAL_DEFENDER_OWNER
            } else {
                slot as usize
            },
            Point {
                x: raw["x"].as_f64().unwrap(),
                z: raw["z"].as_f64().unwrap(),
            },
        )
        .unwrap();
        m.world.game.units.insert(id, u);
        let control = ww2_core::match_commands::UnitControl::new_for_mode(
            &m.world.game.units[&id],
            &m.rules,
            Mode::World,
        )
        .unwrap();
        m.commands.insert(id, control);
        m.combat.initialize(id);
        m.firing.initialize(id);
        m.next_unit_id = m.next_unit_id.max(id + 1);
    }
    let u = m.world.game.units.get_mut(&id).unwrap();
    if let Some(v) = raw["owner"].as_i64() {
        u.owner = if v < 0 {
            LOCAL_DEFENDER_OWNER
        } else {
            v as usize
        };
    }
    if let Some(v) = raw["x"].as_f64() {
        u.x = v;
    }
    if let Some(v) = raw["z"].as_f64() {
        u.z = v;
    }
    if let Some(v) = raw["hp"].as_f64() {
        u.hp = v;
    }
    if let Some(v) = raw["built"].as_f64() {
        u.built = Some(v);
    }
    if let Some(v) = raw["retreating"].as_bool() {
        u.retreating = v;
    }
    if let Some(v) = raw["riding"].as_u64() {
        u.riding = v != 0;
    }
    m.grid.update(id, u);
    if let Some(forced) = raw["logistics"]["forced"].as_bool() {
        m.logistics.insert(
            id,
            ww2_core::logistics::Reserves {
                ammo: 0.0,
                ammo_max: 0.0,
                ammo_price: 0.0,
                ammo_credit: None,
                provisions: 0.0,
                fuel: None,
                emergency: None,
                warning: None,
                forced,
                stranded: false,
                exhausted: false,
                supply_rate: None,
                cut_for: None,
            },
        );
    }
}
fn update_region(r: &mut RegionState, raw: &Value) {
    if let Some(v) = raw["team"].as_i64() {
        r.team = v as i32;
    }
    if let Some(v) = raw["progress"].as_f64() {
        r.progress = v;
    }
    if let Some(v) = raw["capper"].as_i64() {
        r.capper = (v >= 0).then_some(v as usize);
    }
}
fn reset(m: &mut Match, f: &Value, input: &Value) {
    let ids: Vec<_> = m.world.game.units.keys().copied().collect();
    for id in ids {
        m.production.remove_building(id);
        m.combat.remove(id);
        m.firing.remove(id);
    }
    m.world.game.units.clear();
    m.commands.units.clear();
    m.recruit_metadata.clear();
    m.building_cells.clear();
    m.grid = SpatialGrid::new(&m.world.game.units, 16.0);
    m.logistics = Default::default();
    m.world.game.tick = 40;
    m.options.army = army(&input["army"]);
    m.finish = None;
    m.timeline.clear();
    let teams = input["teams"].as_array().unwrap();
    m.story = vec![Default::default(); teams.len()];
    m.start.economy.players = Vec::new();
    for (slot, team) in teams.iter().enumerate() {
        let team = team.as_i64().unwrap() as i32;
        m.options.players[slot].team = team;
        m.start.players[slot].config.team = team;
        m.world.game.players[slot].team = team;
        m.start.players[slot].spawn =
            at(&f["definitions"][if slot + 1 == teams.len() { 63 } else { slot }]);
        let override_row = &input["players"][slot.to_string()];
        m.start.economy.players.push(PlayerEconomy {
            team,
            mp: 100.0,
            mun: 4.0,
            fuel: 2.0,
            away: override_row["away"].as_bool().unwrap_or(false),
            out: override_row["out"].as_bool().unwrap_or(false),
            ..Default::default()
        });
    }
    let runtime = m.world_runtime.as_mut().unwrap();
    runtime.regions = (0..64)
        .map(|i| {
            let team = if i == 0 {
                teams[0].as_i64().unwrap() as i32
            } else if i == 63 {
                teams.last().unwrap().as_i64().unwrap() as i32
            } else {
                -1
            };
            let mut r = RegionState {
                team,
                progress: if i == 0 || i == 63 { 1.0 } else { 0.0 },
                capper: None,
                contested: false,
                locked: None,
                cap_tick: None,
            };
            update_region(&mut r, &input["regions"][i.to_string()]);
            r
        })
        .collect();
    runtime.nodes = input["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| ResourceNode {
            cell: n["c"].as_u64().unwrap() as usize,
            at: Point {
                x: n["x"].as_f64().unwrap(),
                z: n["z"].as_f64().unwrap(),
            },
            depot: n["depot"].as_u64().map(|v| v as u32),
            rate: n["rate"].as_f64().unwrap(),
            fuel: n["fuel"].as_bool().unwrap(),
        })
        .collect();
    for raw in input["units"].as_array().unwrap() {
        update_unit(m, raw, true);
    }
}
fn projection(m: &Match) -> Value {
    let runtime = m.world_runtime.as_ref().unwrap();
    json!({"regions":runtime.regions.iter().map(|r|json!([r.team,bits(r.progress),r.capper,r.contested,r.locked,r.cap_tick])).collect::<Vec<_>>(),
        "players":m.start.economy.players.iter().zip(&m.start.players).map(|(p,s)|json!([bits(p.mp),bits(p.mun),bits(p.fuel),bits(p.inc),p.fuel_inc.map(bits),p.upkeep.map(bits),p.out,[bits(s.spawn.x),bits(s.spawn.z)]])).collect::<Vec<_>>(),
        "story":m.story.iter().map(|s|bits(s.captures)).collect::<Vec<_>>(),
        "units":m.world.game.units.values().map(|u|json!([u.id,owner(u.owner)])).collect::<Vec<_>>(),
        "winner":null,"reason":null,"endAt":null})
}
fn sorted_units(mut v: Value) -> Value {
    v["units"]
        .as_array_mut()
        .unwrap()
        .sort_by_key(|u| u[0].as_u64().unwrap());
    v
}

#[test]
fn original_world_capture_income_transitions_and_explicit_consequences() {
    let f: Value = serde_json::from_str(include_str!("fixtures/world-region.json")).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    let mut transitions = 0;
    for count in [2, 3] {
        let teams: Vec<_> = if count == 2 {
            vec![json!(0), json!(1)]
        } else {
            vec![json!(0), json!(0), json!(1)]
        };
        let mut m = make_match(&f, &teams);
        for case in f["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["input"]["teams"].as_array().unwrap().len() == count)
        {
            reset(&mut m, &f, &case["input"]);
            for step in case["steps"].as_array().unwrap() {
                for id in step["deleteUnits"].as_array().into_iter().flatten() {
                    let id = id.as_u64().unwrap() as u32;
                    m.world.game.units.remove(&id);
                    m.grid.remove_unit(id);
                }
                for raw in step["changeUnits"].as_array().into_iter().flatten() {
                    update_unit(&mut m, raw, false);
                }
                for raw in step["addUnits"].as_array().into_iter().flatten() {
                    update_unit(&mut m, raw, true);
                }
                for raw in step["changeRegions"].as_array().into_iter().flatten() {
                    update_region(
                        &mut m.world_runtime.as_mut().unwrap().regions
                            [raw["index"].as_u64().unwrap() as usize],
                        raw,
                    );
                }
                for raw in step["changePlayers"].as_array().into_iter().flatten() {
                    let p = &mut m.start.economy.players[raw["slot"].as_u64().unwrap() as usize];
                    if let Some(v) = raw["out"].as_bool() {
                        p.out = v;
                    }
                }
                m.world.game.tick = step["tick"].as_i64().unwrap_or(m.world.game.tick + 1);
                let before = projection(&m);
                assert_eq!(
                    before,
                    sorted_units(step["before"].clone()),
                    "{} before",
                    case["name"]
                );
                let order: Vec<_> = step["order"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u32)
                    .collect();
                let result = m.apply_world_capture_income(&order, step["dt"].as_f64().unwrap());
                if case["supported"].as_bool().unwrap() {
                    result.unwrap();
                    assert_eq!(
                        projection(&m),
                        sorted_units(step["after"].clone()),
                        "{} after",
                        case["name"]
                    );
                } else {
                    assert!(result.unwrap_err().starts_with("unsupported World"));
                    assert_eq!(projection(&m), before);
                    assert_ne!(
                        step["before"], step["after"],
                        "unsupported branch must execute in source"
                    );
                }
                transitions += 1;
            }
        }
    }
    assert_eq!(transitions, 39);
}

#[test]
fn malformed_world_phase_is_atomic() {
    let f: Value = serde_json::from_str(include_str!("fixtures/world-region.json")).unwrap();
    let mut m = make_match(&f, &[json!(0), json!(1)]);
    let input = &f["cases"][0]["input"];
    reset(&mut m, &f, input);
    let before = projection(&m);
    assert!(m.apply_world_capture_income(&[1], f64::NAN).is_err());
    assert_eq!(projection(&m), before);
    assert!(m.apply_world_capture_income(&[1, 1], 0.05).is_err());
    assert_eq!(projection(&m), before);
    assert!(m.apply_world_capture_income(&[999], 0.05).is_err());
    assert_eq!(projection(&m), before);
    m.world_runtime.as_mut().unwrap().regions[2].cap_tick = Some(i64::MIN);
    let before = projection(&m);
    assert!(m.apply_world_capture_income(&[1], 0.05).is_err());
    assert_eq!(projection(&m), before);
}

#[test]
fn current_depot_facts_do_not_reuse_retired_tick_rows() {
    use ww2_core::economy_state::{Context, DepotFacts, ModeContext, Node, Unit, WorldContext};
    let f: Value = serde_json::from_str(include_str!("fixtures/world-region.json")).unwrap();
    let case = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "explicit retained depot facts")
        .unwrap();
    let mut m = make_match(&f, &[json!(0), json!(1)]);
    reset(&mut m, &f, &case["input"]);
    let regions = m.world_economy_regions().unwrap();
    let rows = [Unit {
        id: 20,
        owner: Some(0),
        kind: "depot",
        hp: 1.0,
        built: 1.0,
        queue: &[],
    }];
    let nodes = [Node {
        depot: Some(20),
        rate: 3.0,
        fuel: false,
        region: Some(0),
    }];
    let ctx = Context {
        mode: ModeContext {
            kind: Mode::World,
            defender_team: None,
            income_excluded_slot: None,
        },
        army: ArmySize::Standard,
        units: &rows,
        drops: &[],
        points: &[],
        nodes: &nodes,
        world: Some(WorldContext { regions: &regions }),
    };
    let before = m.start.economy.clone();
    let fact = DepotFacts {
        id: 20,
        owner: Some(0),
        hp: 1.0,
        built: 1.0,
    };
    assert!(
        m.start
            .economy
            .advance_income_with_depots(&ctx, &[fact, fact], 0.05, false)
            .is_err()
    );
    assert_eq!(m.start.economy, before);
    let bad = DepotFacts {
        built: f64::NAN,
        ..fact
    };
    assert!(
        m.start
            .economy
            .advance_income_with_depots(&ctx, &[bad], 0.05, false)
            .is_err()
    );
    assert_eq!(m.start.economy, before);
    m.start
        .economy
        .advance_income_with_depots(&ctx, &[], 0.05, false)
        .unwrap();
    assert_eq!(
        projection(&m)["players"],
        case["steps"][0]["after"]["players"]
    );
}
