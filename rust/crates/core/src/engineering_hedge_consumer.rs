//! Actual original hedge callback consumer. Core cfg(test) only.
use super::super::Replay;
use serde_json::{Value, json};
use crate::{
    economy::{Faction,Mode}, economy_state::ArmySize, game::Point, grid::SpatialGrid,
    map::AuthoritativeMap, match_abilities::AbilityState, match_combat::CombatState,
    match_commands::{Command,MatchCommands}, match_firing::FiringState,
    match_recovery::RecoveryState, match_runtime::{LivingWeather,Match,MatchOptions,PlayerConfig},
    match_tick::{TickEvent,TickPhase}, navigation::{TerrainView,Versions}, orders::OrderKey,
    pathfinding::{PathRules,PathTerrain,Pathfinding}, random::StreamId,
    unit_publication::{PublicationParts,StagedUnit,prepare_publication},
    weather::{WeatherChoice,WeatherKind,WeatherPlan}, world::CellChange,
};

fn number(v: &Value) -> f64 {
    v.as_f64().expect("finite Source input")
}

fn point(v: &Value) -> Point {
    Point {
        x: number(&v["x"]),
        z: number(&v["z"]),
    }
}

fn p(p: Point) -> Value {
    json!({"x":p.x,"z":p.z})
}

fn compare(actual: &Value, expected: &Value, label: &str) {
    fn visit(actual: &Value, raw: &Value, bits: &Value, path: &str, label: &str) {
        match (actual, raw) {
            (Value::Number(a), Value::Number(_)) => {
                assert_eq!(
                    format!("{:016x}", a.as_f64().unwrap().to_bits()),
                    bits[path].as_str().unwrap(),
                    "{label}:{path}"
                );
            }
            (Value::Array(a), Value::Array(b)) => {
                assert_eq!(a.len(), b.len(), "{label}:{path}");
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    let child = if path.is_empty() {
                        i.to_string()
                    } else {
                        format!("{path}.{i}")
                    };
                    visit(a, b, bits, &child, label);
                }
            }
            (Value::Object(a), Value::Object(b)) => {
                assert_eq!(
                    a.keys().collect::<Vec<_>>(),
                    b.keys().collect::<Vec<_>>(),
                    "{label}:{path}"
                );
                for (key, value) in a {
                    let child = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    visit(value, &b[key], bits, &child, label);
                }
            }
            _ => assert_eq!(actual, raw, "{label}:{path}"),
        }
    }
    visit(actual, &expected["raw"], &expected["floatBits"], "", label);
}

fn environment(m: &Match) -> Value {
    let living = m.start.living.as_ref().unwrap();
    json!({"tick":m.world.game.tick,
        "weather":{"now":m.start.weather.now.name(),"next":m.start.weather.next.map(WeatherKind::name),"at":m.start.weather.at},
        "wx":{"rain":m.world.game.rain,"raining":living.raining,"wet":m.world.game.wet.unwrap(),"next":living.next},
        "seed":m.start.environment_seed,"wind":{"a":m.start.wind.0,"v":m.start.wind.1}})
}

fn restore_environment(m: &mut Match, setup: &Value) {
    // Initial scene facts only. Recorded subsequent frames never enter native state.
    let e = &setup["environment"]["raw"];
    m.start.environment_seed = e["seed"].as_u64().unwrap() as u32;
    m.start.wind = (number(&e["wind"]["a"]), number(&e["wind"]["v"]));
    m.start.weather = WeatherPlan {
        now: WeatherKind::try_from(e["weather"]["now"].as_str().unwrap()).unwrap(),
        next: e["weather"]["next"]
            .as_str()
            .map(|v| WeatherKind::try_from(v).unwrap()),
        at: e["weather"]["at"].as_u64().unwrap() as u32,
    };
    m.world.game.weather = Some(m.start.weather.now as usize);
    m.start.living = Some(LivingWeather {
        raining: e["wx"]["raining"].as_bool().unwrap(),
        next: number(&e["wx"]["next"]),
    });
    m.world.game.rain = number(&e["wx"]["rain"]);
    m.world.game.wet = Some(number(&e["wx"]["wet"]));
    compare(
        &environment(m),
        &setup["environment"],
        "initial environment",
    );
}

fn make(input: &Value, authored: &Value, setup: &Value, opts: &Value) -> Match {
    let mut map = authored.clone();
    if map.get("name").is_none() {
        map["name"] = json!("");
    }
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&map).unwrap()).unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: input["nativeSeed"].as_u64().unwrap(),
            mode: Mode::Default,
            players: input["names"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(slot, name)| PlayerConfig {
                    name: name.as_str().unwrap().into(),
                    team: setup["teams"][slot].as_i64().unwrap() as i32,
                    faction: Faction::try_from(setup["factions"][slot].as_u64().unwrap() as u8)
                        .unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::from_reference(opts["weather"].as_str()),
            weather_seed: Some(opts["weatherSeed"].as_u64().unwrap_or(1) as u32),
            living_weather: true,
            map_key: opts["mapKey"].as_str().map(str::to_owned),
            tech: false,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}

fn clear(m: &mut Match) {
    // Retain the canonical World and its initial-terrain identity for routing.
    let footprint: Vec<_> = m.building_cells.keys().copied().collect();
    for &cell in &footprint {
        m.world.sections.remove(&cell);
        m.world.game.sections.remove(&(cell as u32));
    }
    m.world.structures.retain(|s| s.kind != "building");
    m.world
        .detached_sections
        .retain(|_, s| !s.metadata.structure_id.starts_with("building:"));
    for cell in footprint {
        let i = m.world.initial();
        let change = CellChange {
            ground: Some(i.ground[cell]),
            object: Some(i.objects[cell]),
            mine: Some(i.mines[cell] != 0),
            height: Some(i.height.as_ref().map_or(0, |h| h[cell])),
            ..Default::default()
        };
        m.world.mutate_cell(cell, change).unwrap();
    }
    for id in m.world.game.units.keys().copied().collect::<Vec<_>>() {
        m.production.remove_building(id);
    }
    m.world.game.units.clear();
    m.building_cells.clear();
    m.building_rallies.clear();
    m.recruit_metadata.clear();
    m.commands = MatchCommands::new(4096);
    m.firing = FiringState::default();
    m.combat = CombatState::default();
    m.abilities = AbilityState::default();
    m.recovery = RecoveryState::default();
    m.grid = SpatialGrid::new(&m.world.game.units, 16.0);
}

fn support_projection(m: &Match) -> Value {
    let g = &m.world.game;
    let off_map = m
        .start
        .random
        .snapshot()
        .streams
        .into_iter()
        .find(|(kind, _)| *kind == StreamId::OffMap)
        .map(|(_, state)| format!("{state:016x}"));
    json!({
        "tick":g.tick,"environment":environment(m),
        "chars":String::from_utf8(g.chars.clone()).unwrap(),
        "ground":String::from_utf8(g.ground.clone().unwrap()).unwrap(),
        "objects":String::from_utf8(g.objects.clone().unwrap()).unwrap(),
        "height":g.height,"scar32":m.world.scar.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
        "wear32":g.wear.as_ref().unwrap().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
        "hp32":g.cell_hp.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),"state":m.world.cell_state,
        "units":g.units.values().map(|u|json!([u.id,m.rules.kinds[u.kind].name,u.owner,u.x,u.z,u.hp,u.supp])).collect::<Vec<_>>(),
        "strikes":g.strikes.iter().map(|s|{
            let t=s.runtime.as_ref().unwrap();
            json!([t.kind.name(),s.owner,s.x,s.z,s.dir,t.delay,t.left,t.next,s.live])
        }).collect::<Vec<_>>(),
        "negativeLog":m.world.cell_log().iter().any(|row|row.height<0),"offMapState":off_map
    })
}

fn path(m: &Match, cache: &mut Pathfinding, from: Point, to: Point) -> Vec<Point> {
    let g = &m.world.game;
    assert!(g.flags.iter().all(|flag| flag & 3 != 3));
    assert!(m.world.mines.is_empty());
    let near = vec![0; g.flags.len()];
    let mines = vec![false; g.flags.len()];
    let fires: Vec<_> = (0..g.flags.len())
        .map(|c| m.world.fires.contains_key(&c))
        .collect();
    let v = m.world.versions;
    cache
        .find_path(
            PathTerrain {
                navigation: TerrainView {
                    width: g.w,
                    height: g.h,
                    flags: &g.flags,
                    heights: g.height.as_deref(),
                    world_known: false,
                    knowledge_id: 0,
                    versions: Versions {
                        infantry: v.infantry,
                        vehicle: v.vehicle,
                        naval: v.naval,
                        obstruction: v.obstruction,
                        knowledge: 0,
                    },
                    changed: None,
                    changes: &[],
                },
                near_walls: &near,
                known_mines: &mines,
                fires: &fires,
                bunkers: &[],
                wear: g.wear.as_deref(),
                wetness: g.wet.unwrap(),
                roads: m.world.roads,
            },
            PathRules::compiled().unwrap(),
            &m.rules.kinds[m.rules.kind("rifle").unwrap()],
            from,
            to,
            None,
        )
        .unwrap()
        .waypoints
}

fn original_settled_rubble(m: &Match) {
    let g = &m.world.game;
    let origins = [10 * g.w + 8, 10 * g.w + 9];
    let mut rubble = std::collections::BTreeSet::new();
    for origin in origins {
        let section = &m.world.sections[&origin];
        assert!(g.sections[&(origin as u32)].failed);
        assert!(section.failure.settled_at.is_some_and(f64::is_finite));
        rubble.extend(section.rubble.iter().copied());
    }
    assert!(!rubble.is_empty(), "durable rubble footprint remains");
    for cell in rubble {
        assert_eq!(g.objects.as_ref().unwrap()[cell], b'R');
        assert!(origins.iter().any(|origin| crate::math::hypot(
            (cell % g.w) as f64 - (origin % g.w) as f64,
            (cell / g.w) as f64 - (origin / g.w) as f64,
        ) <= 3.0), "rubble remains within six metres of the collapsed footprint");
    }
}

fn fixture() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/engineering-hedge-original.json");
    let f: Value = serde_json::from_slice(&std::fs::read(path)
        .expect("original hedge capture pending Root-approved recorder patch and Source grant")).unwrap();
    assert_eq!(f["format"],"engineering-hedge-original-v1");
    assert_eq!(f["reference"],"e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"],"v24.21.0");
    assert_eq!(f["arch"],"x64");
    let input: Value = serde_json::from_str(include_str!("../../simtest/tests/fixtures/engineering-support-selected-input.json")).unwrap();
    assert_eq!(f["input"],input);
    assert_eq!(f["check"]["name"],"test.js:701");
    assert_eq!(f["check"]["passed"],true);
    assert_eq!(f["check"]["bodySha256"],"590bb8794b26620f132f866eb2f66194fb9ac11fc13b2cdf32e5aeee3bb5d655");
    f
}

fn replace_expected(value: &mut Value, path: &[Value], replacement: &Value) {
    let Some((key,tail)) = path.split_first() else { *value=replacement.clone(); return; };
    let child = if let Some(index)=key.as_u64() {
        value.as_array_mut().unwrap().get_mut(index as usize).unwrap()
    } else { value.as_object_mut().unwrap().get_mut(key.as_str().unwrap()).unwrap() };
    replace_expected(child,tail,replacement);
}

fn original_put(m: &mut Match, input: &Value, action: &Value) {
    let id=action["id"].as_u64().unwrap() as u32;
    assert_eq!(m.next_unit_id,id,"real allocator at original authored publication");
    let owner=action["owner"].as_u64().unwrap() as usize;
    assert_eq!(action["type"],"tank");
    assert_eq!(owner,0);
    assert_eq!(point(action),Point{x:25.0,z:5.0});
    let mut staged=StagedUnit::ordinary(&m.world.game,&m.rules,owner,m.rules.kind("tank").unwrap(),
        id,m.world.game.units.len() as u32,point(&input["spawnSentinel"]),Some(point(action))).unwrap();
    staged.unit.rot=number(&action["rot"]);
    staged.control.aim=number(&action["aim"]);
    staged.control.auto_retreat=action["autoRetreat"].as_bool().unwrap();
    prepare_publication(PublicationParts{units:&mut m.world.game.units,grid:&mut m.grid,
        controls:&mut m.commands.units,recruitment:&mut m.recruit_metadata,next_id:&mut m.next_unit_id},
        &m.rules,2,vec![staged]).unwrap().publish_with(|u,_|{
            m.combat.initialize(u.id);m.firing.initialize(u.id);m.abilities.initialize(u.id);m.recovery.initialize(u.id);
        });
    m.grid.rebuild(&m.world.game.units);
}

#[test]
fn original_hedge_constant_input_runs_the_real_492_tick_consumer() {
    let f=fixture();let trace=&f["scene"];let setup=&trace["setup"];let input=&f["input"];
    assert_eq!(trace["name"],"test.js:701:0");
    assert_eq!(trace["originalConstantDraws"],20);
    assert_eq!(trace["nativeSamplerDraws"],0);
    assert_eq!(trace["draws"],20);
    assert!(setup["actors"].as_array().unwrap().is_empty());
    let mut authored=input["map"].clone();
    authored["w"]=json!(20);authored["h"]=json!(20);
    let mut rows=vec![".".repeat(20);20];rows[10]=format!("{}BB{}HH{}",".".repeat(8),".".repeat(4),".".repeat(4));
    authored["rows"]=json!(rows);assert_eq!(trace["map"],authored);
    let mut m=make(input,&trace["map"],setup,&json!({}));
    clear(&mut m);
    for (slot,player) in m.start.players.iter_mut().enumerate() {
        player.spawn=point(&setup["players"][slot]["spawn"]);
        m.start.economy.players[slot].mp=number(&setup["players"][slot]["mp"]);
    }
    restore_environment(&mut m,setup);
    m.next_unit_id=setup["nextId"].as_u64().unwrap() as u32;
    compare(&support_projection(&m),&trace["initial"],"initial original hedge");
    let random=m.start.random.snapshot();
    assert!(m.start.random.replay.is_none());
    let replay=Replay::engineering(&trace["originalConstantTape"]).unwrap();
    m.start.random.replay=Some(replay.clone());
    let deltas=trace["frameChanges"].as_array().unwrap();assert_eq!(deltas.len(),492);
    let actions=trace["actions"].as_array().unwrap();assert_eq!(actions.len(),3);
    let queries=trace["queries"].as_array().unwrap();assert_eq!(queries.len(),1);
    let runs=trace["runs"].as_array().unwrap();assert_eq!(runs.len(),54);
    assert_eq!(runs[0]["seconds"],10);assert_eq!(runs[53]["seconds"],12);
    assert!(runs[1..53].iter().all(|run|run["seconds"].as_f64()==Some(0.05)));
    let mut expected=trace["initial"].clone();let mut cache=Pathfinding::new();
    let (mut applied,mut sequence,mut dispatched,mut queried,mut crushed)=(0,0,0,0,0);
    let mut collapses=Vec::new();
    for (index,delta) in deltas.iter().enumerate() {
        let tick=index as i64+1;
        let before=m.world.game.tick;assert_eq!(before,tick-1);
        for query in queries.iter().filter(|q|q["tick"].as_i64()==Some(before)) {
            assert_eq!(before,252);original_settled_rubble(&m);
            let actual=path(&m,&mut cache,point(&query["from"]),point(&query["to"]));
            compare(&json!(actual.iter().copied().map(p).collect::<Vec<_>>()),&query["path"],"original rubble route");
            assert!(actual.iter().all(|p|p.x>0.0),"rubble is walkable");queried+=1;
        }
        for action in actions.iter().filter(|a|a["tick"].as_i64()==Some(before)) {
            if action["kind"]=="put" { assert_eq!(before,252);original_put(&mut m,input,action); }
            else {
                assert_eq!(action["owner"],0);assert_eq!(action["denial"],Value::Null);assert_eq!(action["resetCooldown"],false);
                let request=if before==0 {json!({"t":"support","kind":"artillery","x":18,"z":21,"dir":0})}
                    else {json!({"t":"move","orders":[[m.world.game.units.keys().next().unwrap(),31,36]]})};
                assert_eq!(action["request"],request);
                sequence+=1;
                m.enqueue_command(OrderKey{tick:tick as u64,issuer:0,sequence},Command::from_json(&action["request"]).unwrap()).unwrap();
            }
            applied+=1;
        }
        if tick<=200 { replay.engineering_begin(tick).unwrap(); }
        let chars=m.world.game.chars.clone();
        let report=m.advance_tick().unwrap_or_else(|e|panic!("original hedge tick{tick}: {e}"));
        assert_eq!(report.tick,tick);assert!(m.finish.is_none());
        assert!(report.phases.contains(&TickPhase::Movement));assert!(report.phases.contains(&TickPhase::Support));
        assert!(report.commands.iter().all(|c|c.denial.is_none()));dispatched+=report.commands.len();
        if tick<=200 {replay.engineering_end(tick).unwrap();}
        if tick==200 {
            replay.engineering_finish().unwrap();
            assert_eq!(m.start.random.replay.take().unwrap(),replay);
            assert_eq!(m.start.random.snapshot(),random,"explicit inputs do not advance/reset native streams");
        }
        for effect in report.environment() {
            if let crate::environment::Effect::Collapse(collapse)=effect && collapse.section.is_none() {
                let g=&m.world.game;let cell=g.cell_of(collapse.position.x,collapse.position.z).unwrap();
                assert_eq!(chars[cell],b'H');
                assert!(g.units.values().any(|u|m.rules.kinds[u.kind].name=="tank"&&g.cell_of(u.x,u.z)==Some(cell)));
                crushed+=1;
            }
        }
        let shots:Vec<_>=report.events.iter().filter_map(|event|match event {
            TickEvent::Environment(effect)|TickEvent::Support(crate::match_support::SupportEvent::Environment(effect))=>Some(effect),_=>None,
        }).filter_map(|effect|match effect {crate::environment::Effect::Collapse(c) if c.section.is_none()=>Some(effect.to_json()),_=>None}).collect();
        if !shots.is_empty(){collapses.push(json!({"tick":tick,"shots":shots}));}
        for change in delta.as_array().unwrap(){replace_expected(&mut expected,change[0].as_array().unwrap(),&change[1]);}
        compare(&support_projection(&m),&expected,&format!("original hedge tick{tick}"));
    }
    assert_eq!((applied,dispatched,queried),(3,2,1));assert!(crushed>0);
    assert!(m.start.random.replay.is_none());assert_eq!(m.world.game.tick,492);
    let receipt=replay.receipt();
    assert_eq!(receipt["capacity"],20);assert_eq!(receipt["consumed"],20);
    assert_eq!(receipt["unconsumed"],0);assert_eq!(receipt["exhausted"],true);
    assert_eq!(receipt["engineering"]["closed"],true);assert_eq!(receipt["engineering"]["active"],false);
    assert_eq!(receipt["engineering"]["tick"],200);
    assert_eq!(receipt["engineering"]["sourceCalls"],trace["originalConstantTape"]);
    let source=trace["contactCollapses"].as_array().unwrap();assert_eq!(collapses.len(),source.len());
    for (actual,expected) in collapses.iter().zip(source){compare(actual,expected,"ordered original collapse effects");}
    original_settled_rubble(&m);
    assert!(m.world.game.chars[10*20+14..10*20+16].contains(&b'.'),"tank flattened the hedge");
    compare(&support_projection(&m),&trace["final"],"complete original hedge final");
}
