//! Original Bridges predicates through ordinary paid commands and one public Pathfinder.
//! Full actual inventories are diagnostic records, not aligned whole-match evidence.
use serde_json::{Value, json};
use std::io::{BufWriter, Write};
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    navigation::{TerrainView, Versions},
    pathfinding::{PathRules, PathStatus, PathTerrain, Pathfinding},
    route_knowledge::{InitialTerrain, LiveRouting, RouteKnowledge},
    weather::WeatherChoice,
};

const MAP: &str = include_str!("../../../../harness/scenes/mov28-paid-bridge.json");
const PIN: &str = "e4e579a838aabae3b1d9dadbc17a0a87147bc52d";
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn point(p: Point) -> Value {
    json!([bits(p.x), bits(p.z)])
}
fn write(writer: &mut impl Write, row: Value) {
    serde_json::to_writer(&mut *writer, &row).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
}
fn make() -> Match {
    let map = AuthoritativeMap::from_json(MAP.as_bytes()).unwrap();
    Match::new(&map, MatchOptions {
        // Native startup uses its own unchanged constructor. Source seed/wind
        // are retained in the raw sidecar and are never installed here.
        seed: 60828,
        mode: Mode::Conquest,
        players: vec![
            PlayerConfig { name: "A".into(), team: 0, faction: Faction::Usa },
            PlayerConfig { name: "B".into(), team: 1, faction: Faction::Germany },
        ],
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: None,
        living_weather: false,
        map_key: None,
        tech: false,
        supply: false,
        logistics: false,
        sandbox: false,
    }).unwrap()
}
fn roster(m: &Match) -> Value {
    json!(m.world.game.units.values().map(|u| json!({
        "id":u.id,"owner":u.owner,"kind":m.rules.kinds[u.kind].name,
        "x":bits(u.x),"z":bits(u.z),"hp":bits(u.hp),"rot":bits(u.rot)
    })).collect::<Vec<_>>())
}
fn frame(m: &Match, crew: u32) -> Value {
    let crew_state = m.world.game.units.get(&crew).map(|u| json!({
        "id":crew,"path":u.path.iter().copied().map(point).collect::<Vec<_>>(),"paid":u.dig
    }));
    json!({"tick":m.world.game.tick,"units":roster(m),
        "crew":crew_state,
        "mp":m.start.economy.players.iter().map(|p| bits(p.mp)).collect::<Vec<_>>(),
        "actualUnits":format!("{:?}",m.world.game.units),
        "actualControls":format!("{:?}",m.commands.units),
        "actualJobs":format!("{:?}",m.field_works.jobs())})
}
fn steps(m: &mut Match, crew: u32, count: usize, initial: &[u32], writer: &mut impl Write) {
    for _ in 0..count {
        let result = m.advance_tick();
        write(writer, json!({"advance":format!("{result:?}"),"actual":frame(m,crew)}));
        result.expect("ordinary canonical tick must complete");
        assert_eq!(m.world.game.units.keys().copied().collect::<Vec<_>>(),initial,
            "all actual constructor actors retained");
    }
}
fn issue(m: &mut Match, crew: u32, owner: usize, input: Value, source: &Value,
    writer: &mut impl Write) {
    let parsed = Command::from_json(&input);
    write(writer,json!({"at":m.world.game.tick,"input":input,"parser":format!("{parsed:?}")}));
    let command = parsed.expect("ordinary JSON command parser");
    let before = bits(m.start.economy.players[owner].mp);
    let result = m.execute_command(owner,&command,false);
    write(writer,json!({"dispatch":format!("{result:?}"),"actual":frame(m,crew)}));
    let denial = result.expect("canonical command dispatch");
    let actual = json!({"tick":m.world.game.tick,"owner":owner,"command":input,
        "beforeMp":before,"result":denial.map(|d|d.name()),
        "afterMp":bits(m.start.economy.players[owner].mp)});
    write(writer,json!({"event":actual}));
    assert_eq!(actual,*source,"actual command receipt, including paid or denied outcome");
}
fn probe(m: &Match, paths: &mut Pathfinding, label: &str, source: &Value,
    writer: &mut impl Write) -> usize {
    let g = &m.world.game;
    // Reuse the existing terrain provider for derived wall clearance. The
    // persistent Pathfinder borrows the actual World's stable flags storage
    // and mutation counters, so a paid change must invalidate its typed caches.
    let shoals = m.world.shoal.as_ref().map(|v|v.iter().map(|n|*n!=0).collect::<Vec<_>>());
    let knowledge = RouteKnowledge::observed(InitialTerrain {
        width:g.w,height:g.h,chars:&g.chars,ground:g.ground.as_deref(),
        objects:g.objects.as_deref(),heights:g.height.as_deref(),shoals:shoals.as_deref(),
        house_kinds:Some(&g.house),toughness:g.tough.as_deref(),
        trench_front:g.trench_front.as_deref(),
    },0,m.rules.clone()).unwrap();
    let query = knowledge.query();
    let fires = (0..g.w*g.h).map(|c|m.world.fires.contains_key(&c)).collect::<Vec<_>>();
    let bunkers = g.units.values().filter(|u|m.rules.kinds[u.kind].name=="bunker" && u.hp>0.0)
        .map(|u|u.at()).collect::<Vec<_>>();
    let derived = query.path_terrain(LiveRouting {
        fires:&fires,bunkers:&bunkers,wetness:g.wet.unwrap_or(0.0),roads:m.world.roads,
    }).unwrap();
    let v = m.world.versions;
    let navigation = TerrainView {
        width:g.w,height:g.h,flags:&g.flags,heights:g.height.as_deref(),
        world_known:false,knowledge_id:0,
        versions:Versions { infantry:v.infantry,vehicle:v.vehicle,naval:v.naval,
            obstruction:v.obstruction,knowledge:0 },
        changed:None,changes:&[],
    };
    // The original bare tank query has no owner/team and therefore observes no
    // team's known mines. This is that query's policy, not a Source after-image.
    let known_mines = vec![false;g.w*g.h];
    let terrain = PathTerrain { navigation,wear:g.wear.as_deref(),known_mines:&known_mines,..derived };
    let result = paths.find_path(terrain,PathRules::compiled().unwrap(),
        &m.rules.kinds[m.rules.kind("tank").unwrap()],
        Point { x:5.0,z:21.0 },Point { x:35.0,z:21.0 },None);
    write(writer,json!({"probe":label,"tick":g.tick,"result":format!("{result:?}"),
        "actualFlags":g.flags,"actualChars":String::from_utf8(g.chars.clone()).unwrap(),
        "actualNearWalls":terrain.near_walls,"actualVersions":format!("{v:?}"),
        "actualFlagsStorage":format!("{:p}",g.flags.as_ptr())}));
    let result = result.expect("existing public Pathfinder");
    let status = match result.status {
        PathStatus::Found => "Found",
        PathStatus::Unreachable => "Unreachable",
        PathStatus::ExpansionLimit => "ExpansionLimit",
    };
    let actual = json!({"points":result.waypoints.iter().copied().map(point).collect::<Vec<_>>(),
        "status":status,"expansions":result.expansions});
    write(writer,json!({"probe":label,"actual":actual,"actualRouteEnd":point(result.route_end)}));
    assert_eq!(source["label"],label);
    assert_eq!(source["tick"],g.tick);
    assert_eq!(source["flags"],json!(g.flags));
    assert_eq!(source["chars"],String::from_utf8(g.chars.clone()).unwrap());
    if !source["near"].is_null() {
        assert_eq!(source["near"],json!(terrain.near_walls));
    }
    // A null Source near-wall observation stays unobserved; no zero array or
    // unprojected routeEnd/follow/drift/work state is inferred from absence.
    assert_eq!(actual,source["expected"],"exact actual component routing output");
    result.waypoints.len()
}

#[test]
#[ignore = "requires a complete private MOV28 Source capture and Root execution grant"]
fn original_paid_strips_change_tank_path_availability() {
    let trace: Value = serde_json::from_slice(&std::fs::read(
        std::env::var_os("WW2_MOV28_TRACE").expect("private Source capture required")).unwrap()).unwrap();
    assert_eq!(trace["format"],"mov28-paid-bridge-v1");
    assert_eq!(trace["reference"],PIN);
    assert_eq!(trace["node"],"v24.21.0");
    assert_eq!(trace["arch"],"x64");
    assert_eq!(trace["exit"],0);
    assert_eq!(trace["failure"],Value::Null,"complete authentic Source first");
    assert_eq!(trace["instrumentation"]["erasesExactly"],true);
    assert_eq!(trace["map"],serde_json::from_str::<Value>(MAP).unwrap());
    assert_eq!(trace["constructors"].as_array().unwrap().len(),1);
    assert_eq!(trace["frames"].as_array().unwrap().len(),1202);
    assert_eq!(trace["frames"][1201]["tick"],1201);
    assert!(trace["queries"].as_array().unwrap().iter().all(|q|
        !q["providerCalls"].as_array().unwrap().is_empty()),"actual query callbacks required");
    assert!(trace["assertions"].as_array().unwrap().iter().all(|v|v["passed"]==true));
    let mut assertions = Vec::new();
    for tick in 1..=601 {
        assertions.push(json!({"tick":tick,"label":"all constructor actors retained","passed":true}));
    }
    let at = |tick, label| json!({"tick":tick,"label":label,"passed":true});
    assertions.extend([
        at(601,"original builder start at (13,21)"),
        at(601,"ordinary income can pay all three strips"),
        at(601,"the river stops vehicles"),
        at(601,"invalid bridge does not debit"),
    ]);
    for (index, tick) in [601,801,1001].into_iter().enumerate() {
        assertions.push(at(tick,"each original bridge strip is paid"));
        for step in tick+1..=tick+200 {
            assertions.push(at(step,"all constructor actors retained"));
        }
        assertions.push(at(tick+200,"builders remain on their bank"));
        if index == 0 {
            assertions.push(at(801,"the original three-cell span is built"));
            assertions.push(at(801,"a 2 m span is narrower than the tank hull"));
        }
    }
    assertions.extend([
        at(1201,"the complete 6 m bridge is built"),
        at(1201,"vehicles have a path once physical width fits"),
        at(1201,"all original probes retained"),
        at(1201,"complete original windows retained"),
    ]);
    assert_eq!(trace["assertions"], json!(assertions), "complete ordered original predicate inventory");
    for (tick, frame) in trace["frames"].as_array().unwrap().iter().enumerate() {
        assert_eq!(frame["tick"], tick, "every actual Source tick is retained");
    }
    let events = trace["events"].as_array().unwrap();
    let probes = trace["queries"].as_array().unwrap();
    assert_eq!(events.len(),11);
    assert_eq!(probes.len(),3);
    let file = std::fs::OpenOptions::new().write(true).create_new(true).open(
        std::env::var_os("WW2_MOV28_REPORT").expect("fresh private native output required")).unwrap();
    let mut writer = BufWriter::new(file);
    let mut m = make();
    write(&mut writer,json!({"format":"mov28-paid-native-v1","sourceHead":trace["sourceHead"],
        "nativeHead":std::env::var("WW2_MOV28_NATIVE_HEAD").expect("frozen head required"),
        "actualInitialRoster":roster(&m),"actualInitialUnits":format!("{:?}",m.world.game.units),
        "actualInitialControls":format!("{:?}",m.commands.units),
        "actualInitialEconomy":format!("{:?}",m.start.economy),
        "actualStartup":{"seed":m.options.seed,"nextId":m.next_unit_id,"wind":m.start.wind}}));
    let initial = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let rifles = m.world.game.units.values().filter(|u|u.owner==0 && m.rules.kinds[u.kind].name=="rifle")
        .map(|u|u.id).collect::<Vec<_>>();
    assert_eq!(rifles.len(),2,"actual constructor rifles selected independently");
    let crew = rifles[0];
    assert_eq!(trace["crew"],crew);
    write(&mut writer,json!({"actual":frame(&m,crew)}));
    let mut event = 0;
    for owner in [0,1] {
        let ids = m.world.game.units.values().filter(|u|u.owner==owner && !m.rules.kinds[u.kind].structure)
            .map(|u|u.id).collect::<Vec<_>>();
        for key in ["holdPos","holdFire","autoRetreat"] {
            issue(&mut m,crew,owner,json!({"t":"stance","ids":ids,"key":key,"on":key!="autoRetreat"}),
                &events[event],&mut writer);
            event += 1;
        }
    }
    steps(&mut m,crew,1,&initial,&mut writer);
    issue(&mut m,crew,0,json!({"t":"move","orders":[[crew,13,21]]}),&events[event],&mut writer);
    event += 1;
    steps(&mut m,crew,600,&initial,&mut writer);
    let u = &m.world.game.units[&crew];
    assert_eq!((u.x,u.z),(13.0,21.0),"original builder start");
    let catalog = Catalog::compiled().unwrap();
    let cost = catalog.table("FORTS").unwrap().get("bridge").unwrap().get("cost").unwrap().number().unwrap();
    let dig_time = catalog.table("CFG").unwrap().get("digTime").unwrap().number().unwrap();
    let ticks = (dig_time*3.0+1.0)/m.rules.movement.tick;
    assert_eq!(ticks,200.0,"original work windows");
    assert!(m.start.economy.players[0].mp>=3.0*cost,"ordinary income covers all strips");
    let mut paths = Pathfinding::new();
    assert_eq!(probe(&m,&mut paths,"river",&probes[0],&mut writer),0);
    let before = m.start.economy.players[0].mp;
    issue(&mut m,crew,0,json!({"t":"dig","ids":[crew],"kind":"bridge","x":5,"z":21,"dir":0}),
        &events[event],&mut writer);
    event += 1;
    assert_eq!(m.start.economy.players[0].mp,before,"invalid bridge does not debit");
    for (index,z) in [21,19,23].into_iter().enumerate() {
        let before = m.start.economy.players[0].mp;
        issue(&mut m,crew,0,json!({"t":"dig","ids":[crew],"kind":"bridge","x":21,"z":z,"dir":0}),
            &events[event],&mut writer);
        event += 1;
        assert_eq!(m.start.economy.players[0].mp,before-cost,"each original strip paid");
        steps(&mut m,crew,ticks as usize,&initial,&mut writer);
        assert!(m.world.game.units[&crew].x<18.0,"builders stay on their bank");
        if index==0 {
            assert_eq!((9..=11).map(|x|m.world.game.chars[10*20+x]).collect::<Vec<_>>(),b"===".to_vec());
            assert_eq!(probe(&m,&mut paths,"two-metres",&probes[1],&mut writer),0);
        }
    }
    assert!((9..=11).all(|y|(9..=11).all(|x|m.world.game.chars[y*20+x]==b'=')),
        "complete original 6 m bridge");
    assert!(probe(&m,&mut paths,"six-metres",&probes[2],&mut writer)>0);
    assert_eq!(event,events.len());
    assert_eq!(m.world.game.tick,1201);
    write(&mut writer,json!({"complete":true,"ticks":1201,"frames":1202,"queries":3,"events":11}));
}
