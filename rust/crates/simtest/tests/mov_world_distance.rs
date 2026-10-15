//! Public World constructor, Move and full ticks. No scalar replaces travel.
use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode}, economy_state::ArmySize,
    game::{LOCAL_DEFENDER_OWNER, Point}, map::AuthoritativeMap,
    match_commands::{Command, MoveOrder}, match_runtime::{Match, MatchOptions, PlayerConfig},
    math, random::MatchRandom, weather::{WeatherChoice, WeatherKind},
};
fn bits(n: f64) -> Value { json!(format!("{:016x}",n.to_bits())) }
fn fixture() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mov-world-distance.json.gz");
    let bytes = std::fs::read(path).expect("World fixture absent: Root recorder grant required");
    let mut reader = flate2::read::GzDecoder::new(bytes.as_slice()); let mut text = String::new();
    std::io::Read::read_to_string(&mut reader, &mut text).unwrap();
    let f: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(f["reference"],"e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"],"v24.21.0"); assert_eq!(f["arch"],"x64");
    assert_eq!(f["completed"],true,"retain first Source failure: {}",f["failure"]);
    assert!(f["failure"].is_null() && f["readbackFailure"].is_null());
    assert_eq!(f["originalCapture"]["completed"],true);
    assert_eq!(f["originalCapture"]["name"],"road march: calm units move faster on their own land");
    assert_eq!(f["originalCapture"]["sha256"].as_str().unwrap().len(),64);
    assert_eq!(f["input"],json!({"nativeSeed":29448,"mapSeed":42,"names":["p0","p1"],"teams":[0,1],"factions":[0,1],"weather":"clear"}));
    assert_eq!(f["cases"].as_array().unwrap().len(),2);
    f
}
fn initial(m: &Match) -> Value {
    json!(m.world.game.units.values().map(|u|json!({"id":u.id,
        "owner":if u.owner==LOCAL_DEFENDER_OWNER {-1}else{u.owner as i64},
        "kind":m.rules.kinds[u.kind].name,"cells":u.cells})).collect::<Vec<_>>())
}
fn state(m: &Match, id: u32) -> Value {
    let u=&m.world.game.units[&id];
    json!({"tick":m.world.game.tick,"id":id,"x":bits(u.x),"z":bits(u.z),"rot":bits(u.rot),
        "path":u.path.iter().map(|p|vec![bits(p.x),bits(p.z)]).collect::<Vec<_>>()})
}
struct Run<'a> { m: Match, id: u32, start: Point, c: &'a Value, step: usize, command: usize }
impl<'a> Run<'a> {
    fn new(f: &Value, c: &'a Value) -> Self {
        assert_eq!(c["completed"],true);
        let mut map=f["map"].clone();
        if let Some(runs)=map["world"]["regionMapRuns"].as_array() {
            let cells:Vec<_>=runs.iter().flat_map(|r|std::iter::repeat_n(r[0].clone(),r[1].as_u64().unwrap() as usize)).collect();
            map["world"].as_object_mut().unwrap().remove("regionMapRuns"); map["world"]["regionMap"]=json!(cells);
        }
        assert_eq!(map["world"]["seed"],42,"unaltered original generated geography");
        let map=AuthoritativeMap::from_json(&serde_json::to_vec(&map).unwrap()).expect("actual original World map admission");
        let options=MatchOptions {seed:29448,mode:Mode::World,
            players:vec![PlayerConfig{name:"p0".into(),team:0,faction:Faction::Usa},PlayerConfig{name:"p1".into(),team:1,faction:Faction::Germany}],
            shuffle:false,army:ArmySize::Standard,weather:WeatherChoice::Fixed(WeatherKind::Clear),weather_seed:Some(1),
            living_weather:true,map_key:None,tech:false,supply:true,logistics:false,sandbox:false};
        let m=Match::new_world(&map,options,MatchRandom::new(29448)).expect("authentic World constructor, with every guard intact");
        assert_eq!(initial(&m),c["initial"],"all constructor actors, owners and footprints");
        let id=m.world.game.units.values().find(|u|u.owner==0 && m.rules.kinds[u.kind].name=="rifle").expect("actual constructor rifle").id;
        assert_eq!(json!(id),c["subject"]);
        let u=&m.world.game.units[&id]; let start=u.at();
        assert_eq!(bits(start.x),bits(c["start"]["x"].as_f64().unwrap()));
        assert_eq!(bits(start.z),bits(c["start"]["z"].as_f64().unwrap()));
        // No Source actor or environment result is loaded into this Match.
        m.validate_tick_state().expect("real World full-runner admission before any tick");
        Self{m,id,start,c,step:0,command:0}
    }
    fn step(&mut self, phase: &str, fought: bool) -> f64 {
        if fought {self.m.combat.get_mut(self.id).unwrap().hit_at=self.m.world.game.tick as f64;}
        let expected=&self.c["steps"][self.step];
        assert_eq!(expected["phase"],phase); assert_eq!(expected["fought"],fought);
        assert_eq!(state(&self.m,self.id),expected["before"],"selected World actor before physical tick");
        let before=self.m.world.game.units[&self.id].at();
        self.m.advance_tick().unwrap_or_else(|failure|panic!("{}:{phase}:{} first actual World caller refusal: {failure:?}",self.c["name"],self.m.world.game.tick));
        assert_eq!(state(&self.m,self.id),expected["after"],"selected World actor after full physical tick");
        let after=self.m.world.game.units[&self.id].at(); self.step+=1;
        math::hypot(after.x-before.x,after.z-before.z)
    }
    fn move_leg(&mut self) {
        let u=self.m.world.game.units.get_mut(&self.id).unwrap();
        u.x=self.start.x; u.z=self.start.z; u.path.clear(); u.world_goal=None;
        u.path_gen=u.path_gen.checked_add(1).unwrap();
        self.m.commands.units.get_mut(&self.id).unwrap().orders.clear();
        let cmd=Command::Move{orders:vec![MoveOrder{unit:i64::from(self.id),at:Point{x:self.start.x+20.0,z:self.start.z}}],control_ids:None,attack_move:false,queue:false,together:false,face:None};
        let expected=&self.c["commands"][self.command];
        assert_eq!(json!(self.m.world.game.tick),expected["tick"]);
        assert_eq!(Command::from_json(&expected["command"]).unwrap(),cmd);
        let result=self.m.execute_command(0,&cmd,false);
        assert_eq!(result,Ok(None),"real public World Move: {result:?}"); assert!(expected["result"].is_null());
        self.command+=1;
    }
    fn finish(self) {
        assert_eq!(self.step,self.c["steps"].as_array().unwrap().len());
        assert_eq!(self.command,self.c["commands"].as_array().unwrap().len());
    }
}
#[test]
fn original_public_world_300_plus_40_plus_40_distance() {
    let f=fixture(); let c=&f["cases"][0]; assert_eq!(c["name"],"original-public-300-plus-40-plus-40");
    let mut run=Run::new(&f,c);
    for _ in 0..300 {run.step("warmup",false);}
    run.move_leg(); let mut marching=0.0; for _ in 0..40 {marching+=run.step("marching",false);}
    run.move_leg(); let mut fighting=0.0; for _ in 0..40 {fighting+=run.step("fighting",true);}
    assert!(marching>fighting*1.3,"original physical march {marching}m versus {fighting}m under fire");
    assert_eq!(bits(marching),bits(c["distances"]["marching"].as_f64().unwrap()));
    assert_eq!(bits(fighting),bits(c["distances"]["fighting"].as_f64().unwrap()));
    run.finish();
}
#[test]
fn physical_world_march_calm_boundary_200_and_201() {
    let f=fixture(); let c=&f["cases"][1]; assert_eq!(c["name"],"physical-calm-ticks-200-and-201");
    let mut run=Run::new(&f,c);
    run.m.combat.get_mut(run.id).unwrap().hit_at=0.0;
    run.m.world.game.units.get_mut(&run.id).unwrap().shot_at=Some(0.0);
    for _ in 0..199 {run.step("boundary-warmup",false);}
    run.move_leg(); let normal=run.step("tick-200",false);
    run.move_leg(); let marching=run.step("tick-201",false);
    assert!(normal>0.0,"boundary control actually moves");
    assert!((marching/normal-1.5).abs()<1e-10,"physical1.5 ratio after the strict200-tick boundary");
    assert_eq!(bits(normal),bits(c["distances"]["normal"].as_f64().unwrap()));
    assert_eq!(bits(marching),bits(c["distances"]["marching"].as_f64().unwrap()));
    run.finish();
}
