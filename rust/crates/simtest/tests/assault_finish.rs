use serde_json::{Value,json};
use ww2_core::{
    economy::{Faction,Mode}, economy_state::ArmySize,
    game::{Point,LOCAL_DEFENDER_OWNER}, map::AuthoritativeMap,
    match_runtime::{Match,MatchOptions,PlayerConfig}, match_end::{EndReason,MatchSample},
    match_story::{DeathRecord,PlayerStory,StoryKey}, weather::WeatherChoice,
};
fn fixture() -> Value { serde_json::from_str(include_str!("fixtures/assault-finish.json")).unwrap() }
fn make(c: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&c["map"]).unwrap()).unwrap();
    Match::new_assault(&map,MatchOptions {
        seed:701,mode:Mode::Assault,players:c["teams"].as_array().unwrap().iter().enumerate().map(|(i,t)|PlayerConfig {
            name:format!("P{i}"),team:t.as_i64().unwrap() as i32,faction:Faction::try_from((i%4) as u8).unwrap(),
        }).collect(),shuffle:false,army:ArmySize::Standard,weather:WeatherChoice::Map,weather_seed:Some(1),living_weather:false,map_key:None,
        tech:true,supply:false,logistics:false,sandbox:false,
    },c["defender"].as_i64().unwrap() as i32).unwrap()
}
fn encode(v: Value) -> Value {
    match v {
        Value::Number(n)=>json!(format!("{:016x}",n.as_f64().unwrap().to_bits())),
        Value::Array(a)=>Value::Array(a.into_iter().map(encode).collect()),
        Value::Object(o)=>Value::Object(o.into_iter().map(|(k,v)|(k,encode(v))).collect()),v=>v,
    }
}
fn samples(rows: &[MatchSample]) -> Vec<Value> {
    rows.iter().map(|s|match s {
        MatchSample::Conquest{t,vp}=>json!({"t":t,"vp":vp}),
        MatchSample::Territories{t,points,structures}=>json!({"t":t,"points":points,"structures":structures}),
    }).collect()
}
fn story(rows: &[PlayerStory]) -> Vec<Value> {
    rows.iter().map(|s|Value::Object(StoryKey::ALL.into_iter().map(|k|(k.as_str().to_owned(),json!(s.value(k)))).collect())).collect()
}
fn snapshot(m: &Match) -> Value {
    let mut fallen=serde_json::Map::new();
    for (name,at) in [("structure",m.fallen.structure),("bunker",m.fallen.bunker),("hq",m.fallen.hq)] {
        if let Some(p)=at { fallen.insert(name.into(),json!({"x":p.x,"z":p.z})); }
    }
    let report=m.match_result().unwrap().map(|r|json!({"reason":r.reason.as_str(),"at":{"x":r.at.x,"z":r.at.z},"endTick":r.end_tick,"story":story(&r.story),"timeline":samples(&r.timeline),"winVp":r.win_vp,"mode":r.mode}));
    encode(json!({"timeLeft":m.assault.unwrap().time_left,"winner":m.finish.map(|f|f.winner),"reason":m.finish.map(|f|f.reason.as_str()),"endAt":m.finish.map(|f|json!({"x":f.end_at.x,"z":f.end_at.z})),"endTick":m.finish.map(|f|f.end_tick),"reveal":m.world.game.reveal,"timeline":samples(&m.timeline),"story":story(&m.story),"fallen":fallen,"report":report}))
}
fn at(v: &Value)->Point { Point{x:v["x"].as_f64().unwrap(),z:v["z"].as_f64().unwrap()} }
#[test]
fn canonical_assault_phase_matches_original_finish_story_and_samples() {
    let f=fixture();
    assert_eq!(f["reference"],"e4e579a838aabae3b1d9dadbc17a0a87147bc52d");assert_eq!(f["node"],"v24.21.0");assert_eq!(f["arch"],"x64");
    let mut phases=0;
    for c in f["cases"].as_array().unwrap() {
        let mut m=make(c);
        for step in c["steps"].as_array().unwrap() {
            let a=&step["action"];m.world.game.tick=a["tick"].as_i64().unwrap();
            if let Some(t)=a["timeLeft"].as_f64(){m.assault.as_mut().unwrap().time_left=t;}
            for p in a["points"].as_array().into_iter().flatten(){let owner=p[1].as_i64().unwrap();m.start.points[p[0].as_u64().unwrap() as usize].owner=(owner>=0).then_some(owner as usize);}
            for p in a["out"].as_array().into_iter().flatten(){m.start.economy.players[p[0].as_u64().unwrap() as usize].out=p[1].as_bool().unwrap();}
            for p in a["bunkers"].as_array().into_iter().flatten(){for u in m.world.game.units.values_mut(){if m.rules.kinds[u.kind].name=="bunker"&&(p[0].is_null()||u.owner==p[0].as_u64().unwrap() as usize){u.hp=p[1].as_f64().unwrap();}}}
            if a["neutral"]==true{let u=m.world.game.units.values_mut().find(|u|m.rules.kinds[u.kind].name=="bunker").unwrap();u.owner=LOCAL_DEFENDER_OWNER;}
            if a["fallen"].is_object(){m.fallen.bunker=Some(at(&a["fallen"]));}
            if a["death"].is_object(){let d=&a["death"];m.record_death(DeathRecord{owner:Some(d["owner"].as_u64().unwrap() as usize),kind:"bunker",at:at(&d["at"])},Some(d["by"].as_u64().unwrap() as usize)).unwrap();}
            if a["spend"].is_array(){m.record_spending(a["spend"][0].as_u64().unwrap() as usize,a["spend"][1].as_f64().unwrap()).unwrap();}
            if a["periodic"]==true{m.sample_match_tick().unwrap();}
            m.apply_assault_victory(a["dt"].as_f64().unwrap_or(m.rules.movement.tick)).unwrap();
            assert_eq!(snapshot(&m),step["expected"],"{} phase{}",c["name"],phases);
            if let Some(report)=m.match_result().unwrap(){assert_eq!(report.mode,"assault");assert_eq!(report.win_vp,None);assert_eq!(report.names.len(),m.start.players.len());}
            assert!(m.apply_conquest_victory().is_err());assert!(m.sample_conquest_tick().is_err());assert!(m.conquest_result().is_err());
            phases+=1;
        }
    }
    assert_eq!(f["cases"].as_array().unwrap().len(),11);assert_eq!(phases,22);
}
fn commit_state(m: &Match)->String { format!("{:?}{:?}{:?}{:?}{:?}{}",m.assault,m.finish,m.timeline,m.story,m.fallen,m.world.game.reveal) }
#[test]
fn invalid_phase_inputs_leave_timer_finish_story_and_timeline_atomic() {
    let f=fixture();
    let mutations:[fn(&mut Match);9]=[
        |m|m.options.players[1].team=5,
        |m|m.world.game.players[0].slot=1,
        |m|m.assault.as_mut().unwrap().time_left=f64::NAN,
        |m|m.story[1].kills=f64::INFINITY,
        |m|m.start.points[0].owner=Some(6),
        |m|m.world.game.units.values_mut().next().unwrap().hp=f64::NAN,
        |m|m.world.game.units.values_mut().next().unwrap().kind=usize::MAX,
        |m|m.timeline.push(MatchSample::Conquest{t:0.0,vp:vec![0.0,0.0]}),
        |m|{m.assault.as_mut().unwrap().time_left=0.0;m.fallen.bunker=Some(Point{x:f64::MAX,z:0.0});for u in m.world.game.units.values_mut(){u.hp=0.0;}},
    ];
    for mutate in mutations {let mut m=make(&f["cases"][0]);mutate(&mut m);let before=commit_state(&m);assert!(m.apply_assault_victory(0.05).is_err());assert_eq!(commit_state(&m),before);}
    for dt in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY] {let mut m=make(&f["cases"][0]);let before=commit_state(&m);assert!(m.apply_assault_victory(dt).is_err());assert_eq!(commit_state(&m),before);}
    let mut m=make(&f["cases"][0]);m.assault.as_mut().unwrap().time_left=f64::MAX;let before=commit_state(&m);assert!(m.apply_assault_victory(-f64::MAX).is_err());assert_eq!(commit_state(&m),before);
}
#[test]
fn first_finish_freezes_clock_reveal_story_and_source_reason() {
    let f=fixture();let mut m=make(&f["cases"][0]);m.assault.as_mut().unwrap().time_left=0.0;
    let finish=m.apply_assault_victory(0.0).unwrap().unwrap();assert_eq!(finish.reason,EndReason::Timer);
    let before=commit_state(&m);assert_eq!(m.apply_assault_victory(f64::NAN).unwrap(),None);assert!(!m.sample_match_tick().unwrap());m.record_spending(0,30.0).unwrap();assert_eq!(commit_state(&m),before);
    assert_eq!([EndReason::VictoryPoints,EndReason::Headquarters,EndReason::Bunkers,EndReason::Structures,EndReason::Timer,EndReason::Draw,EndReason::World,EndReason::Nations,EndReason::Tutorial].map(EndReason::as_str),["vp","hq","bunkers","structures","timer","draw","world","nations","tutorial"]);
    m.options.mode=Mode::World;assert!(m.apply_assault_victory(0.05).is_err());assert!(m.sample_match_tick().is_err());assert!(m.match_result().is_err());
}
