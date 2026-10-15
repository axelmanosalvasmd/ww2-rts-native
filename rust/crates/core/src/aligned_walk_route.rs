//! Private MOV11 ordinary-command attempts and read-only walkRoute observations.
//! Every original branch remains a completion gate, including branches not yet witnessed.
use super::{Match, Replay, original};
#[path = "aligned_walk_route_support.rs"]
mod support;
use support::{
    approved_currency_only, boolean_counts, count_mappings, goal, number,
    optional_number, point, prefix_alignment, proposed_defaults, proposed_registry, stream_case,
};
use crate::{
    economy::{Faction, Mode}, economy_state::ArmySize, game::{Game, Point, Unit},
    match_commands::Command, match_runtime::{MatchOptions, PlayerConfig}, math,
    traffic::Steer, weather::WeatherChoice,
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::{BTreeMap, BTreeSet}, io::{BufWriter, Write}};

#[derive(Default)]
struct Observation {
    events: Vec<Value>,
    path_calls: usize,
}
thread_local! {
    static OBSERVATION: RefCell<Option<Observation>> = const { RefCell::new(None) };
}
struct Active;
impl Active {
    fn start() -> Self {
        OBSERVATION.with(|v| {
            assert!(v.borrow().is_none(), "no nested observer or second match");
            *v.borrow_mut() = Some(Observation::default());
        });
        Self
    }
}
impl Drop for Active {
    fn drop(&mut self) { OBSERVATION.with(|v| *v.borrow_mut() = None); }
}
pub(crate) fn observe_path_query() {
    OBSERVATION.with(|v| {
        if let Some(v) = v.borrow_mut().as_mut() { v.path_calls += 1; }
    });
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn observe(
    g: &Game, u: &Unit, event: &str, route: &[Point], speed: f64, dt: f64,
    back: bool, steer: &Steer, budget: Option<f64>,
) {
    OBSERVATION.with(|v| {
        if let Some(v) = v.borrow_mut().as_mut() {
            v.events.push(json!({"tick":g.tick,"id":u.id,"event":event,
                "route":route.iter().copied().map(point).collect::<Vec<_>>(),
                "x":number(u.x),"z":number(u.z),"rot":number(u.rot),
                "speed":number(speed),"dt":number(dt),"back":back,"yielding":steer.yielding,
                "budget":budget.map_or(Value::Null,number),"repath":number(u.repath)}));
        }
    });
}
fn calls() -> usize {
    OBSERVATION.with(|v| v.borrow().as_ref().unwrap().path_calls)
}
fn encode(v: Value) -> Value {
    match v {
        Value::Number(v) => number(v.as_f64().unwrap()),
        Value::Array(v) => v.into_iter().map(encode).collect(),
        Value::Object(v) => v.into_iter().map(|(k,v)| (k,encode(v))).collect(),
        v => v,
    }
}
fn queued(c: &Command) -> Result<Value,String> {
    match c {
        Command::Move { orders, control_ids:None, attack_move, queue:false, together:false, face } if orders.len()==1 => {
            let mut v=json!({"t":if *attack_move {"amove"} else {"move"},
                "orders":orders.iter().map(|o|json!([o.unit,o.at.x,o.at.z])).collect::<Vec<_>>(),
                "x":orders[0].at.x,"z":orders[0].at.z});
            if let Some(a)=face {v["face"]=json!(a);}
            Ok(encode(v))
        }
        other => Err(format!("unrepresented actual queue command: {other:?}")),
    }
}
#[allow(clippy::too_many_arguments)]
fn snapshot(m: &Match, replay: &Replay, births: Vec<u32>, deaths: Vec<u32>, inputs: &[Value], roles: &[Option<u32>], arrived: &[u32], retired: &[crate::match_combat::RetiredUnit]) -> Result<Value,String> {
    let units=m.world.game.units.values().map(|u| {
        let c=m.commands.units.get(&u.id).ok_or("missing canonical control")?;
        let f=m.firing.units.get(&u.id).ok_or("missing canonical firing")?;
        let queue=c.orders.iter().map(queued).collect::<Result<Vec<_>,_>>()?;
        Ok(json!({"id":u.id,"owner":u.owner,"kind":m.rules.kinds[u.kind].name,
            "x":number(u.x),"z":number(u.z),"hp":number(u.hp),"supp":number(u.supp),
            "path":u.path.iter().copied().map(point).collect::<Vec<_>>(),"rot":number(u.rot),"aim":number(c.aim),"moveSpeed":number(u.move_speed),
            "vx":optional_number(u.vx),"vz":optional_number(u.vz),"travelDir":optional_number(u.travel_dir),
            "reverse":goal(u.reverse),"repath":number(u.repath),"stuck":number(u.stuck),"still":optional_number(u.still),"orders":queue,
            "targetId":f.target_id,"attackId":c.attack_id,"amove":goal(u.amove),
            "face":c.face.map_or(Value::Null,|v|json!({"a":number(v.angle),"x":number(v.at.x),"z":number(v.at.z)})),
            "holdPos":u.hold_pos,"holdFire":c.hold_fire,"autoRetreat":c.auto_retreat,"retreating":u.retreating,
            "cooldown":number(f.cooldown),"retarget":number(f.retarget),"hasWorldGoal":u.world_goal.is_some(),
            "outcome":c.outcome.map_or_else(||json!({"absent":true}),|v|json!(v.name()))}))
    }).collect::<Result<Vec<_>,String>>()?;
    let walk=OBSERVATION.with(|v| std::mem::take(&mut v.borrow_mut().as_mut().unwrap().events));
    let mut tape=replay.0.lock().unwrap();
    let branches=std::mem::take(&mut tape.scenario_branches);
    let motion=std::mem::take(&mut tape.scenario_motion);
    Ok(json!({"tick":m.world.game.tick,"nextId":m.next_unit_id,"births":births,"deaths":deaths,"inputs":inputs,"roles":roles,"arrived":arrived,
        "units":units,"players":m.start.economy.players.iter().enumerate().map(|(slot,p)|json!({"slot":slot,"team":p.team,"mp":number(p.mp),"mun":number(p.mun),"fuel":number(p.fuel),"vp":number(p.vp),"inc":number(p.inc)})).collect::<Vec<_>>(),
        "points":m.start.points.iter().map(|p|json!({"owner":p.owner.map_or(-1,|v|v as i64),"capper":p.capper.map_or(-1,|v|v as i64),"progress":number(p.progress),"cut":p.cut})).collect::<Vec<_>>(),"winner":m.finish.map(|f|f.winner),
        "footprints":m.world.game.units.values().filter(|u|m.rules.kinds[u.kind].building).map(|u|json!({"id":u.id,"cells":u.cells,"built":number(u.built.expect("canonical building progress"))})).collect::<Vec<_>>(),
        "walk":walk,"branchEvents":branches,"motion":motion,"retirements":retired.iter().map(|r|json!({"id":r.unit.id,"owner":r.unit.owner,"kind":m.rules.kinds[r.unit.kind].name,
            "x":number(r.unit.x),"z":number(r.unit.z),"hp":number(r.unit.hp),"lastHit":r.last_hit.map_or_else(||json!({"absent":true}),|v|number(f64::from(v)))})).collect::<Vec<_>>(),"drawCount":tape.used.len()}))
}
fn issue(m: &mut Match, owner: usize, command: Value, inputs: &mut Vec<Value>) -> Result<(),String> {
    let decoded=Command::from_json(&command).map_err(|e|format!("actual command parser: {e:?}"))?;
    let denial=m.execute_command(owner,&decoded,false)?;
    inputs.push(json!({"owner":owner,"command":command,"denial":denial.map_or_else(||json!({"absent":true}),|d|json!(d.name()))}));
    if let Some(d)=denial {return Err(format!("actual command denial: {}",d.name()));}
    Ok(())
}
fn require(m: &Match, rows: &mut Vec<Value>, label: &str, passed: bool) -> Result<(),String> {
    rows.push(json!({"tick":m.world.game.tick,"label":label,"passed":passed}));
    if passed {Ok(())} else {Err(format!("original predicate/precondition: {label}"))}
}
fn distance(u: &Unit, p: Point) -> f64 { math::hypot(u.x-p.x,u.z-p.z) }
fn target(i: usize, reverse: bool, staged: bool) -> Point {
    if staged {Point {x:57.0+(i%3) as f64*4.0,z:if reverse {47.0-(i/3) as f64*4.0} else {77.0+(i/3) as f64*4.0}}}
    else {Point {x:51.0+(i%6) as f64*4.0,z:if reverse {89.0+(i/6) as f64*4.0} else {31.0+(i/6) as f64*4.0}}}
}
fn move_to(m: &mut Match, ids: &[u32], targets: &[Point], queue: bool, inputs: &mut Vec<Value>) -> Result<(),String> {
    let mut command=json!({"t":"move","orders":ids.iter().zip(targets).map(|(id,p)|json!([id,p.x,p.z])).collect::<Vec<_>>()});
    if queue {command["queue"]=json!(true);}
    issue(m,0,command,inputs)
}
#[allow(clippy::too_many_arguments)]
fn before_tick(m: &mut Match, authored: &Value, scene: &Value, tick: usize, own: &[u32], mobile: &[(u32,usize)], roles: &mut [Option<u32>], destinations: &mut BTreeMap<u32,Point>, assertions: &mut Vec<Value>, inputs: &mut Vec<Value>) -> Result<(),String> {
    if tick==1 {for owner in [0,1] {for key in ["holdPos","holdFire","autoRetreat"] {issue(m,owner,json!({"t":"stance","ids":mobile.iter().filter(|(_,o)|*o==owner).map(|(id,_)|*id).collect::<Vec<_>>(),"key":key,"on":key!="autoRetreat"}),inputs)?;}}}
    match authored["kind"].as_str().unwrap() {
        "movement" => {
            if tick==2 {issue(m,0,json!({"t":"move","orders":[[own[0],5,5]],"face":0}),inputs)?;}
            if tick==201 {let u=&m.world.game.units[&own[0]];require(m,assertions,"Movement original (5,5) start",u.x==5.0&&u.z==5.0&&u.rot==0.0&&u.hp==100.0)?;move_to(m,&own[..1],&[Point{x:30.0,z:30.0}],false,inputs)?;}
        }
        "bridge" => {
            let reverse=authored["variant"]=="reverse";
            if tick==2 {issue(m,0,json!({"t":"tech","kind":"tier"}),inputs)?;}
            if tick==1220 {issue(m,0,json!({"t":"build","ids":own,"kind":"motorpool","x":authored["buildAt"]["x"],"z":authored["buildAt"]["z"]}),inputs)?;}
            if tick==2400 {require(m,assertions,"real completed motorpool",m.world.game.units.values().any(|u|u.owner==0&&m.rules.kinds[u.kind].name=="motorpool"&&u.built.is_some_and(|b|b>=1.0)))?;issue(m,0,json!({"t":"stop","ids":own}),inputs)?;}
            let initial_constructor_slots=[0,2,4];
            let buy_slots=(0..18).filter(|i|!initial_constructor_slots.contains(i)).collect::<Vec<_>>();
            if let Some(&slot)=tick.checked_sub(2401).and_then(|i|buy_slots.get(i)) {
                let before=m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
                issue(m,0,json!({"t":"buy","unit":scene["bridgeTypes"][slot]}),inputs)?;
                let born=m.world.game.units.values().filter(|u|!before.contains(&u.id)).collect::<Vec<_>>();
                require(m,assertions,"one canonical paid recruit",born.len()==1&&m.rules.kinds[born[0].kind].name==scene["bridgeTypes"][slot].as_str().unwrap())?;
                roles[slot]=Some(born[0].id);
            }
            if tick==2420 {
                require(m,assertions,"all18 exact mixed types",roles.iter().enumerate().all(|(i,id)|id.is_some_and(|id|m.rules.kinds[m.world.game.units[&id].kind].name==scene["bridgeTypes"][i].as_str().unwrap())))?;
                let ids=roles.iter().map(|id|id.unwrap()).collect::<Vec<_>>();
                for key in ["holdFire","autoRetreat"] {issue(m,0,json!({"t":"stance","ids":ids,"key":key,"on":key=="holdFire"}),inputs)?;}
                issue(m,0,json!({"t":"stance","ids":buy_slots.iter().map(|i|roles[*i].unwrap()).collect::<Vec<_>>(),"key":"holdPos","on":true}),inputs)?;
                let orders=ids.iter().enumerate().map(|(i,id)|{let mut p=target(i,reverse,true);if m.rules.kinds[m.world.game.units[id].kind].name=="medic" {p.z+=if reverse {-2.0} else {2.0};}json!([id,p.x,p.z])}).collect::<Vec<_>>();
                issue(m,0,json!({"t":"move","orders":orders,"face":if reverse {std::f64::consts::FRAC_PI_2} else {-std::f64::consts::FRAC_PI_2}}),inputs)?;
            }
            if tick==2801 {
                let orders=roles.iter().enumerate().filter_map(|(i,id)|{let id=id.unwrap();if m.rules.kinds[m.world.game.units[&id].kind].name!="medic" {return None;}let p=target(i,reverse,true);Some(json!([id,p.x,p.z]))}).collect::<Vec<_>>();
                issue(m,0,json!({"t":"move","orders":orders,"face":if reverse {std::f64::consts::FRAC_PI_2} else {-std::f64::consts::FRAC_PI_2}}),inputs)?;
            }
            if tick==3001 {
                let ids=roles.iter().map(|id|id.unwrap()).collect::<Vec<_>>();
                require(m,assertions,"all18 original staged positions",ids.iter().enumerate().all(|(i,id)|{let u=&m.world.game.units[id];let p=target(i,reverse,true);u.x==p.x&&u.z==p.z}))?;
                let catalog=crate::catalog::Catalog::compiled().map_err(|e|e.to_string())?;
                let mut full=true;
                for id in &ids {let u=&m.world.game.units[id];let def=catalog.unit(&m.rules.kinds[u.kind].name).map_err(|e|e.to_string())?;let hp=def.get("models").and_then(crate::catalog::Value::number).ok_or("missing models")?*def.get("hpPer").and_then(crate::catalog::Value::number).ok_or("missing hpPer")?;full&=u.hp==hp&&math::cos(u.rot-if reverse {std::f64::consts::FRAC_PI_2} else {-std::f64::consts::FRAC_PI_2})>0.999;}
                require(m,assertions,"all18 original health and facing",full)?;
                issue(m,0,json!({"t":"stance","ids":buy_slots.iter().map(|i|roles[*i].unwrap()).collect::<Vec<_>>(),"key":"holdPos","on":false}),inputs)?;
                issue(m,0,json!({"t":"stop","ids":ids}),inputs)?;
                issue(m,0,json!({"t":if authored["variant"]=="attack-move" {"amove"} else {"move"},"orders":ids.iter().enumerate().map(|(i,id)|{let p=target(i,reverse,false);json!([id,p.x,p.z])}).collect::<Vec<_>>(),"together":authored["variant"]=="together"}),inputs)?;
                for id in &ids {let u=&m.world.game.units[id];let goal=u.world_goal.or(u.amove);require(m,assertions,"real resolved destination exists",goal.is_some())?;let mut p=goal.unwrap();if authored["variant"]=="queued" {p.z-=12.0;}destinations.insert(*id,p);}
                if authored["variant"]=="queued" {move_to(m,&ids,&(0..18).map(|i|{let mut p=target(i,reverse,false);p.z-=12.0;p}).collect::<Vec<_>>(),true,inputs)?;}
            }
            if authored["variant"]=="deferred"&&(tick==3101||tick==3301) {let budget=if tick==3101 {0} else {4096};m.aligned_walk_budget(budget);inputs.push(json!({"op":"pathBudget","value":budget}));}
        }
        "cover" => {
            let ids=roles.iter().map(|id|id.unwrap()).collect::<Vec<_>>();
            if tick==2 {move_to(m,&ids,&[Point{x:21.81,z:20.1894},Point{x:23.1,z:20.1994}],false,inputs)?;}
            if tick==401 {require(m,assertions,"protected cover before true move",crate::terrain::in_cover(&m.world.game,&m.rules,&m.world.game.units[&ids[0]]))?;move_to(m,&ids,&[Point{x:40.0,z:2.0},Point{x:41.29,z:2.01}],false,inputs)?;}
        }
        "naval" => {
            if tick==2 {for owner in [0,1] {issue(m,owner,json!({"t":"tech","kind":"tier"}),inputs)?;}}
            if tick==1220 {for owner in [0,1] {issue(m,owner,json!({"t":"build","ids":mobile.iter().filter(|(_,o)|*o==owner).map(|(id,_)|*id).collect::<Vec<_>>(),"kind":"shipyard","x":45,"z":if owner==0 {17} else {111}}),inputs)?;}}
            if tick==2401 {for owner in [0,1] {
                let before=m.world.game.units.keys().copied().collect::<BTreeSet<_>>();issue(m,owner,json!({"t":"buy","unit":"gunboat"}),inputs)?;
                let born=m.world.game.units.values().filter(|u|!before.contains(&u.id)).collect::<Vec<_>>();require(m,assertions,"real coastal gunboat",born.len()==1&&m.rules.kinds[born[0].kind].name=="gunboat")?;let id=born[0].id;roles[owner]=Some(id);
                issue(m,owner,json!({"t":"stance","ids":[id],"key":"holdFire","on":true}),inputs)?;issue(m,owner,json!({"t":"stance","ids":[id],"key":"autoRetreat","on":false}),inputs)?;issue(m,owner,json!({"t":"move","orders":[[id,75,if owner==0 {57} else {77}]]}),inputs)?;
            }}
            if tick==3001 {require(m,assertions,"both naval staging arrivals",roles.iter().enumerate().all(|(i,id)|distance(&m.world.game.units[&id.unwrap()],Point{x:75.0,z:if i==0 {57.0} else {77.0}})<0.5))?;issue(m,0,json!({"t":"stance","ids":[roles[0]],"key":"holdFire","on":false}),inputs)?;issue(m,0,json!({"t":"attack","ids":[roles[0]],"target":roles[1]}),inputs)?;}
            if tick==3002 {let id=roles[0].unwrap();require(m,assertions,"genuine live target before backing",m.firing.units[&id].target_id==roles[1].unwrap())?;move_to(m,&[id],&[Point{x:75.0,z:49.0}],false,inputs)?;}
        }
        _ => return Err("unknown authored case".into()),
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn after_tick(m: &mut Match, authored: &Value, tick: usize, roles: &[Option<u32>], destinations: &BTreeMap<u32,Point>, arrived: &mut Vec<u32>, assertions: &mut Vec<Value>, inputs: &mut Vec<Value>, call_start: usize) -> Result<(),String> {
    if authored["kind"]=="bridge"&&tick>=3001 {
        if authored["variant"]=="deferred"&&(3101..3301).contains(&tick) {
            require(m,assertions,"zero budget performs no actual findPath calls",calls()==call_start)?;
        }
        for id in roles.iter().map(|id|id.unwrap()) {
            let u=&m.world.game.units[&id];let cell=m.world.game.cell_index(u.x,u.z);
            require(m,assertions,"no river displacement",cell>=0&&m.world.game.chars[cell as usize]!=b'W')?;
            require(m,assertions,"reachable order not abandoned",m.commands.units[&id].outcome!=Some(crate::match_commands::MoveOutcome::Unreachable))?;
            let radius=if authored["variant"]=="attack-move" {2.5} else {0.5};
            if distance(u,destinations[&id])<radius&&u.path.is_empty()&&u.world_goal.is_none()&&m.commands.units[&id].orders.is_empty()&&!arrived.contains(&id) {
                arrived.push(id);issue(m,0,json!({"t":"stance","ids":[id],"key":"holdPos","on":true}),inputs)?;
            }
        }
    }
    if authored["kind"]=="cover"&&tick==401 {
        require(m,assertions,"friendly bridge traffic retains protected cover",crate::terrain::in_cover(&m.world.game,&m.rules,&m.world.game.units[&roles[0].unwrap()]))?;
    }
    Ok(())
}
fn coverage(frame: &Value, counts: &mut BTreeMap<String,Vec<Value>>) {
    let roles=frame["roles"].as_array().unwrap();
    let mut advances=BTreeMap::<u64,usize>::new();
    for row in frame["walk"].as_array().unwrap() {
        if !roles.contains(&row["id"]) {continue;}
        let event=row["event"].as_str().unwrap();
        counts.entry(event.into()).or_default().push(json!({"tick":frame["tick"],"actor":row["id"]}));
        if row["back"]==true&&event=="advance" {counts.entry("backing".into()).or_default().push(json!({"tick":frame["tick"],"actor":row["id"]}));}
        if event=="reached-shift" {*advances.entry(row["id"].as_u64().unwrap()).or_default()+=1;}
    }
    for (id,count) in advances {if count>1 {counts.entry("multi-leg".into()).or_default().push(json!({"tick":frame["tick"],"actor":id,"legs":count}));}}
}
fn run_case(trace: &Value, authored: &Value, source: &Value, writer: &mut impl Write) -> Result<Value,String> {
    let _active=Active::start();
    let ticks=authored["ticks"].as_u64().unwrap() as usize;
    if source["failure"]!=Value::Null||source["completed"]!=ticks||source["frames"].as_array().is_none_or(|v|v.len()!=ticks+1) {return Err("complete authentic Source case required".into());}
    let map=crate::map::AuthoritativeMap::from_json(&serde_json::to_vec(&authored["map"]).unwrap()).map_err(|e|e.to_string())?;
    let replay=Replay::new(&trace["sharedInput"])?;
    let mut m=Match::new_aligned(&map,MatchOptions {
        seed:trace["sharedInput"]["seed"].as_str().ok_or("private seed absent")?.parse().map_err(|_|"private seed invalid")?,
        mode:Mode::Conquest,players:vec![PlayerConfig{name:"A".into(),team:0,faction:Faction::Usa},PlayerConfig{name:"B".into(),team:1,faction:Faction::Germany}],
        shuffle:true,army:ArmySize::Endless,weather:WeatherChoice::from_reference(Some("clear")),weather_seed:None,living_weather:true,
        map_key:Some("aligned-mov11".into()),tech:true,supply:true,logistics:false,sandbox:false,
    },replay.clone())?;
    let own=m.world.game.units.values().filter(|u|u.owner==0&&m.rules.kinds[u.kind].name=="rifle").map(|u|u.id).collect::<Vec<_>>();
    let own_mg=m.world.game.units.values().filter(|u|u.owner==0&&m.rules.kinds[u.kind].name=="mg").map(|u|u.id).collect::<Vec<_>>();
    let mobile=m.world.game.units.values().filter(|u|!m.rules.kinds[u.kind].structure).map(|u|(u.id,u.owner)).collect::<Vec<_>>();
    let initial=snapshot(&m,&replay,m.world.game.units.keys().copied().collect(),Vec::new(),&[],&[],&[],&[])?;
    let report=std::path::PathBuf::from(std::env::var_os("WW2_MOV11_REPORT").ok_or("private report required")?);
    let prefix=std::fs::File::create(report.with_file_name(format!("{}.constructor.json",authored["name"].as_str().unwrap()))).map_err(|e|e.to_string())?;
    serde_json::to_writer(prefix,&json!({"initialRoster":initial["units"],"initialFrame":initial,"startup":replay.receipt()["startup"],"inputReceipt":replay.receipt()})).map_err(|e|e.to_string())?;
    if own.len()!=2||own_mg.len()!=1||mobile.len()!=6 {return Err("ordinary Endless constructor roster changed; no synthetic actors".into());}
    let mut roles=if authored["kind"]=="bridge" {vec![None;18]} else {vec![Some(own[0]),Some(own[1])]};
    if authored["kind"]=="bridge" {roles[0]=Some(own[0]);roles[4]=Some(own[1]);roles[2]=Some(own_mg[0]);}
    let mut native=Vec::new();let mut assertions=Vec::new();let mut destinations=BTreeMap::new();let mut arrived=Vec::new();
    let mut checked=0;let mut failure=None;let mut first_raw=None;let mut first_approved=None;let mut first_proposed=None;
    let mut raw_n=0;let mut approved_n=0;let mut proposed_n=0;let mut ticks_matched=0;
    let mut source_currency=BTreeMap::new();let mut native_currency=BTreeMap::new();let mut source_defaults=BTreeMap::new();let mut native_defaults=BTreeMap::new();
    let mut branch_counts=BTreeMap::new();let mut route_calls=Vec::new();let mut unavailable=BTreeSet::new();
    for tick in 0..=ticks {
        let mut inputs=Vec::new();let call_start=calls();
        let (births,deaths,retired)=if tick==0 {(m.world.game.units.keys().copied().collect(),Vec::new(),Vec::new())} else {
            if let Err(error)=before_tick(&mut m,authored,&trace["scene"],tick,&own,&mobile,&mut roles,&mut destinations,&mut assertions,&mut inputs) {failure=Some(json!({"kind":"inputOrPrecondition","tick":tick,"error":error,"inputs":inputs}));break;}
            let before=m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
            let result=match m.advance_tick() {Ok(v)=>v,Err(e)=>{failure=Some(json!({"kind":"nativeTick","tick":tick,"error":format!("{e:?}"),"inputs":inputs}));break;}};
            unavailable.extend(result.unavailable.iter().map(|v|format!("{v:?}")));
            if let Err(error)=after_tick(&mut m,authored,tick,&roles,&destinations,&mut arrived,&mut assertions,&mut inputs,call_start) {failure=Some(json!({"kind":"originalPredicate","tick":tick,"error":error,"inputs":inputs}));break;}
            let after=m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
            (after.difference(&before).copied().collect(),before.difference(&after).copied().collect(),result.retired)
        };
        let frame=snapshot(&m,&replay,births,deaths,&inputs,&roles,&arrived,&retired)?;
        let expected=&source["frames"][tick];
        coverage(&frame,&mut branch_counts);native.push(frame);
        let observed=&native[tick];
        if tick==0 {let receipt=replay.receipt();if receipt["startup"]!=source["startup"]||receipt["layout"]["input"]!=source["inputReceipt"]["layout"]["input"] {failure=Some(json!({"kind":"startupAlignment","source":source["startup"],"native":receipt["startup"]}));break;}}
        if let Err(error)=prefix_alignment(source,expected,&replay,&mut checked) {failure=Some(json!({"kind":"consumedInputAlignment","tick":tick,"error":error}));break;}
        if let Some(v)=original::compare_projection_for_test(expected,observed) {if first_raw.is_none() {first_raw=Some(json!({"tick":tick,"difference":v}));}} else if tick%10==0 {raw_n+=1;}
        let (a,_)=approved_currency_only(expected);let (b,_)=approved_currency_only(observed);
        if let Some(v)=original::compare_projection_for_test(&a,&b) {if first_approved.is_none() {first_approved=Some(json!({"tick":tick,"difference":v}));}} else if tick%10==0 {approved_n+=1;}
        let (a,c,d)=proposed_defaults(expected);let (b,e,f)=proposed_defaults(observed);
        count_mappings(c,d,&mut source_currency,&mut source_defaults);count_mappings(e,f,&mut native_currency,&mut native_defaults);
        if let Some(v)=original::compare_projection_for_test(&a,&b) {if first_proposed.is_none() {first_proposed=Some(json!({"tick":tick,"difference":v}));}} else {if tick%10==0 {proposed_n+=1;}if tick>0 {ticks_matched+=1;}}
        if tick>0 {route_calls.push(json!({"tick":tick,"actualNativeFindPathCalls":calls()-call_start}));}
    }
    if failure.is_none() {
        let result=match authored["kind"].as_str().unwrap() {
            "movement" => require(&m,&mut assertions,"Movement arrival after full10seconds",distance(&m.world.game.units[&own[0]],Point{x:30.0,z:30.0})<1.0),
            "bridge" => require(&m,&mut assertions,"all18 original120second arrivals",arrived.len()==18),
            "naval" => {let u=&m.world.game.units[&roles[0].unwrap()];require(&m,&mut assertions,"naval backing actual arrival",distance(u,Point{x:75.0,z:49.0})<0.5&&u.path.is_empty())},
            _ => Ok(()),
        };
        if let Err(error)=result {failure=Some(json!({"kind":"finalPredicate","error":error}));}
    }
    let receipt=replay.receipt();let aligned=receipt==source["inputReceipt"]&&checked==receipt["consumed"].as_u64().unwrap() as usize;
    let assertion_match=assertions==*source["assertions"].as_array().unwrap();
    let passed=failure.is_none()&&first_proposed.is_none()&&aligned&&assertion_match&&native.len()==ticks+1;
    let metadata=json!({"name":authored["name"],"mode":"DIAGNOSTIC_PROPOSED_DEFAULTS","ownerApproval":"PENDING","parityEligible":false,"rowStatusChanged":false,
        "diagnosticMatched":passed,"completed":m.world.game.tick,"requestedTicks":ticks,"framesCompared":native.len(),"actualTicksMatched":ticks_matched,
        "rawStrict":{"checkpointsMatched":raw_n,"firstDifference":first_raw},"approvedCurrencyOnly":{"checkpointsMatched":approved_n,"firstDifference":first_approved},
        "proposed":{"checkpointsMatched":proposed_n,"requestedCheckpoints":ticks/10+1,"firstDifference":first_proposed},"failure":failure,
        "fullInputAlignment":aligned,"prefixDrawsCompared":checked,"nativeInputReceipt":receipt,"sourceInputReceipt":source["inputReceipt"],
        "assertionSequenceMatches":assertion_match,"nativeAssertions":assertions,"sourceAssertions":source["assertions"],
        "currencyMappings":{"source":source_currency,"native":native_currency},"booleanMappings":{"source":boolean_counts(&source_defaults),"native":boolean_counts(&native_defaults)},
        "positiveBranches":branch_counts,"actualNativeRouteCalls":route_calls,"actualSourceRouteCalls":source["routeCalls"],"unavailable":unavailable});
    stream_case(writer,&metadata,&native,source)?;
    Ok(metadata)
}

#[test]
#[ignore = "requires complete new private MOV11 Source capture and coordinator slot"]
fn original_movement_and_bridge_matrix_with_all_walk_branches() {
    let manifest=std::path::PathBuf::from(std::env::var_os("WW2_MOV11_TRACE").expect("private manifest required"));
    let trace:Value=serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    let scene:Value=serde_json::from_str(include_str!("../../../../harness/scenarios/aligned-mov11.json")).unwrap();
    assert_eq!(trace["format"],"aligned-mov11-trace-v1");assert_eq!(trace["reference"],scene["reference"]);assert_eq!(trace["scene"],scene);
    assert_eq!(trace["node"], "v24.21.0", "original x64 recorder runtime");
    assert_eq!(trace["sharedInput"]["layout"], scene["layout"]);
    assert_eq!(trace["sharedInput"]["tape"].as_array().unwrap().len(), scene["tapeCapacity"].as_u64().unwrap() as usize);
    assert_eq!(trace["failure"],Value::Null,"Source first failure must be reviewed before native");assert_eq!(trace["instrumentation"]["erasesExactly"],true);
    assert_eq!(trace["cases"].as_array().unwrap().len(),21);
    let mut writer=BufWriter::new(std::fs::File::create(std::env::var_os("WW2_MOV11_REPORT").expect("private report required")).unwrap());
    let header=json!({"format":"aligned-mov11-report-v1","mode":"DIAGNOSTIC_PROPOSED_DEFAULTS","ownerApproval":"PENDING","parityEligible":false,
        "nativeHead":std::env::var("WW2_MOV11_NATIVE_HEAD").expect("frozen head required"),"sourceHead":trace["sourceHead"],"sourceManifestSha256":std::env::var("WW2_MOV11_TRACE_SHA256").expect("private frozen binding required"),"proposedDefaultRegistry":proposed_registry()});
    let mut bytes=serde_json::to_vec(&header).unwrap();assert_eq!(bytes.pop(),Some(b'}'));writer.write_all(&bytes).unwrap();writer.write_all(b",\"cases\":[").unwrap();
    let mut results=Vec::new();let mut branches=BTreeSet::new();
    for (index,authored) in scene["cases"].as_array().unwrap().iter().enumerate() {
        let item=&trace["cases"][index];assert_eq!(item["name"],authored["name"]);
        let filename=item["file"].as_str().unwrap();assert_eq!(filename,format!("{}.json",authored["name"].as_str().unwrap()));
        let source:Value=serde_json::from_slice(&std::fs::read(manifest.parent().unwrap().join(filename)).unwrap()).unwrap();
        assert_eq!(source["name"], authored["name"], "case file belongs to its authored input");
        assert_eq!(item["completed"], source["completed"], "manifest binds actual completed ticks");
        assert_eq!(item["frames"], source["frames"].as_array().unwrap().len(), "manifest binds complete raw frames");
        assert_eq!(item["failure"], source["failure"], "manifest retains actual case failure");
        for (tick, frame) in source["frames"].as_array().unwrap().iter().enumerate() {
            assert_eq!(frame["tick"], tick, "every actual Source frame remains sequential");
        }
        if index>0 {writer.write_all(b",").unwrap();}
        let result=match run_case(&trace,authored,&source,&mut writer) {Ok(v)=>v,Err(e)=>{let v=json!({"name":authored["name"],"admissionFailure":e,"diagnosticMatched":false});serde_json::to_writer(&mut writer,&v).unwrap();v}};
        let matched=result["diagnosticMatched"]==true;
        if let Some(counts)=result["positiveBranches"].as_object() {branches.extend(counts.keys().cloned());}
        println!("{}",json!({"name":authored["name"],"diagnosticMatched":matched,"completed":result["completed"],"framesCompared":result["framesCompared"],"actualTicksMatched":result["actualTicksMatched"],"proposed":result["proposed"],"fullInputAlignment":result["fullInputAlignment"],"failure":result["failure"],"admissionFailure":result["admissionFailure"]}));
        results.push(result);writer.flush().unwrap();if !matched {break;}
    }
    let missing=scene["requiredBranches"].as_array().unwrap().iter().filter(|v|!branches.contains(v.as_str().unwrap())).collect::<Vec<_>>();
    let passed=results.len()==21&&results.iter().all(|v|v["diagnosticMatched"]==true)&&missing.is_empty();
    writer.write_all(b"],\"diagnosticMatched\":").unwrap();serde_json::to_writer(&mut writer,&passed).unwrap();writer.write_all(b",\"missingPositiveBranches\":").unwrap();serde_json::to_writer(&mut writer,&missing).unwrap();writer.write_all(b"}").unwrap();writer.flush().unwrap();
    assert!(passed,"first diagnostic/provider/branch failure retained; MOV11 remains held");
}
