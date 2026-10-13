//! Executable ordinary-Conquest supply phases over canonical Match state.
use crate::{catalog::Catalog,game::{Game,CELL,MOVE,VBLOCK,Point,Pose,Unit},grid::SpatialGrid,
 logistics::{self,LogisticsState,UnitFacts,Definition},match_commands::{MatchCommands,UnitControl},match_combat::CombatState,match_firing::FiringState,match_runtime::Match,
 match_routing::RecipientRouting,math,navigation::{TerrainView,Versions},objectives::{ObjectivePlayer,ObjectiveUnit},pathfinding::PathTerrain,rules::Rules,
 supply_convoys::{self,ConvoyState,DepotPoint,Player,UnitRole,UnitView,UnitOperation},supply_network::{GroundSupplyNetwork,SupplyUnit},supply_routes::{SupplyRouting,Threat},unit_spawn::{self,SpawnPlacer},vehicle,world::World};

fn view(g:&Game)->TerrainView<'_>{TerrainView{width:g.w,height:g.h,flags:&g.flags,heights:g.height.as_deref(),world_known:true,knowledge_id:0,versions:Versions{..Versions::default()},changed:None,changes:&[]}}
fn reserve_context(enabled:bool)->logistics::Context {logistics::Context{enabled_override:Some(enabled),scenario:false,mode:logistics::Mode::Conquest}}
fn facts<'a>(u:&Unit,rules:&'a Rules)->UnitFacts<'a>{UnitFacts{id:u.id,kind:&rules.kinds[u.kind].name,owner:Some(u.owner),owner_exists:true,guard_home:false,riding:u.riding}}
impl Match {
 /// Initialize after troop/base/scenario setup and metadata registration. No runtime rejection is removed by this method alone.
 pub(crate) fn initialize_logistics(&mut self)->Result<(),String>{
  if !self.options.logistics{return Ok(());}
  for u in self.world.game.units.values(){let def=Definition::from_catalog(&self.rules.kinds[u.kind].name).map_err(|e|e.to_string())?;self.logistics.initialize_unit(reserve_context(true),facts(u,&self.rules),Some(&def));}
  self.convoys=ConvoyState::setup(true);
  let ids=self.world.game.units.keys().copied().collect::<Vec<_>>();self.refresh_supply_network(&ids)?;
  self.step_supply_convoys_with_dt(false,0.0)
 }
 /// The source network runs on its own early-phase beat, using the tick's captured ordered list.
 pub fn refresh_supply_network(&mut self,unit_order:&[u32])->Result<(),String>{
  if self.convoys.is_none(){return Ok(());}
  self.validate_supply()?;
  let mut seen=std::collections::BTreeSet::new();if unit_order.iter().any(|id|!seen.insert(*id)){return Err("duplicate supply tick unit".into());}
  let players=self.start.players.iter().zip(&self.start.economy.players).map(|(p,e)|ObjectivePlayer{team:e.team,out:e.out,spawn:p.spawn}).collect::<Vec<_>>();
  let units=unit_order.iter().filter_map(|id|self.world.game.units.get(id)).map(|u|SupplyUnit{fact:ObjectiveUnit::from_unit(u,&self.rules.kinds[u.kind].name,self.logistics.get(u.id).is_some_and(|r|r.forced)),built:u.built}).collect::<Vec<_>>();
  let initial=self.world.initial();
  self.supply_net=GroundSupplyNetwork::compute(view(&self.world.game),&self.world.game.chars,&initial.chars,initial.height.as_deref(),&self.start.points,&units,&players,&mut SpawnPlacer::new())?;Ok(())
 }
 /// Reserve time is charged before movement. Driving fuel is charged separately from actual powered travel.
 pub fn advance_unit_reserves(&mut self,id:u32)->Result<(),String>{
  self.validate_supply()?;let Some(u)=self.world.game.units.get(&id)else{return Ok(())};
  self.logistics.tick_reserves(reserve_context(self.options.logistics),facts(u,&self.rules),self.rules.movement.tick,0.0);Ok(())
 }
 pub fn consume_unit_driving_fuel(&mut self,id:u32,travel_seconds:f64)->Result<(),String>{
  if !travel_seconds.is_finite()||travel_seconds<0.0{return Err("invalid powered travel time".into());}self.validate_supply()?;
  if let Some(u)=self.world.game.units.get(&id){self.logistics.consume_driving_fuel(facts(u,&self.rules),travel_seconds);}Ok(())
 }
 /// Late source phase, after capture and income. The caller owns clock advancement and winner state.
 pub fn step_supply_convoys(&mut self,winner_decided:bool)->Result<(),String>{self.step_supply_convoys_with_dt(winner_decided,self.rules.movement.tick)}
 fn validate_supply(&self)->Result<(),String>{
  let g=&self.world.game;if g.tick<0 || g.tick as u64>9_007_199_254_740_351 || self.start.players.len()!=g.players.len() || g.players.len()!=self.start.economy.players.len() || self.next_unit_id.checked_add(24).is_none(){return Err("invalid supply Match state".into());}
  for (i,p) in g.players.iter().enumerate(){if i!=p.slot || p.team as i32!=self.start.economy.players[i].team{return Err("supply player ownership mismatch".into());}}
  for u in g.units.values(){if u.kind>=self.rules.kinds.len()||u.owner>=g.players.len()||!u.x.is_finite()||!u.z.is_finite()||!u.hp.is_finite()||self.commands.units.get(&u.id).is_none()||self.firing.get(u.id).is_none()||self.combat.get(u.id).is_none(){return Err("invalid canonical supply unit".into());}
   if u.garrison.is_some(){return Err("supply garrison entry is not represented".into());}
   let c=self.combat.get(u.id).unwrap();if u.riding!=c.carrier.is_some() || c.carrier.is_some_and(|id|!g.units.contains_key(&id)){return Err("supply carrier identity mismatch".into());}
  }Ok(())
 }
 fn step_supply_convoys_with_dt(&mut self,winner_decided:bool,dt:f64)->Result<(),String>{
  if self.convoys.is_none()||winner_decided{return Ok(());}
  self.validate_supply()?;
  for owner in 0..self.start.players.len(){self.refresh_recipient_routing(owner)?;}
  let players=self.start.players.iter().zip(&self.start.economy.players).enumerate().map(|(slot,(p,e))|Player{slot,team:e.team,spawn:p.spawn,out:e.out,away:e.away}).collect::<Vec<_>>();
  let points=self.start.points.iter().map(|p|DepotPoint{at:p.at,owner:p.owner,contested:p.contested,depot:p.kind.as_deref()==Some("depot")}).collect::<Vec<_>>();
  let aid=self.world.aid.iter().map(|(&c,&o)|(c,o)).collect::<Vec<_>>();let chars=self.world.game.chars.clone();
  let key=match self.options.army {crate::economy_state::ArmySize::Standard=>"standard",crate::economy_state::ArmySize::Large=>"large",crate::economy_state::ArmySize::Massive=>"massive",crate::economy_state::ArmySize::Endless=>"endless"};
  let pop=Catalog::compiled().map_err(|e|e.to_string())?.table("CFG").map_err(|e|e.to_string())?.get("armies").and_then(|v|v.get(key)).and_then(|v|v.get("pop")).and_then(crate::catalog::Value::number).ok_or("missing army population scale")?;
  let ctx=supply_convoys::Context{enabled:true,construction:false,tick:self.world.game.tick as u64,winner:false,army_pop:pop,players:&players,width:self.world.game.w,chars:&chars,aid:&aid,points:&points};
  let mut adapter=ConvoyWorld{world:&mut self.world,grid:&mut self.grid,commands:&mut self.commands,firing:&mut self.firing,combat:&mut self.combat,recipients:&self.recipients,routing:&mut self.supply_routes,network:&self.supply_net,rules:&self.rules,players:&players,next_id:&mut self.next_unit_id,placer:SpawnPlacer::new()};
  self.convoys.as_mut().unwrap().step(ctx,&mut self.logistics,&mut adapter,dt).map_err(|e|format!("Match convoys: {e:?}"))
 }
}
struct ConvoyWorld<'a>{world:&'a mut World,grid:&'a mut SpatialGrid,commands:&'a mut MatchCommands,firing:&'a mut FiringState,combat:&'a mut CombatState,recipients:&'a [RecipientRouting],routing:&'a mut SupplyRouting,network:&'a GroundSupplyNetwork,rules:&'a Rules,players:&'a [Player],next_id:&'a mut u32,placer:SpawnPlacer}
impl ConvoyWorld<'_>{
 fn unit_view(&self,u:&Unit)->UnitView{let name=self.rules.kinds[u.kind].name.as_str();let c=self.combat.get(u.id).unwrap();UnitView{id:u.id,owner:Some(u.owner),role:match name{"hq"=>UnitRole::Hq,"supplycache"=>UnitRole::Cache,"halftrack"=>UnitRole::Halftrack,"truck"=>UnitRole::Truck,_=>UnitRole::Other},hp:u.hp,built:u.built.unwrap_or(f64::NAN),at:u.at(),still:u.still.unwrap_or(f64::NAN),cargo_len:c.cargo.len(),region_cache:false,riding:c.carrier,path:u.path.clone(),world_goal:u.world_goal,orders:self.commands.units[&u.id].orders.iter().map(|_|u.at()).collect(),amove:u.amove,stuck:u.stuck}}
 fn threats(&self,owner:usize)->Vec<Threat>{let catalog=Catalog::compiled().expect("startup catalog");self.world.game.players[owner].visible.iter().filter_map(|id|self.world.game.units.get(id)).filter(|u|u.hp>0.0&&u.air.is_none()&&self.players[u.owner].team!=self.players[owner].team).filter_map(|u|{let w=catalog.unit(&self.rules.kinds[u.kind].name).ok()?.get("w")?;let range=w.get("range")?.number()?;(range>0.0).then_some(Threat{at:u.at(),range})}).collect()}
}
impl supply_convoys::World for ConvoyWorld<'_>{
 fn units(&self)->Vec<UnitView>{self.world.game.units.values().map(|u|self.unit_view(u)).collect()}
 fn unit(&self,id:u32)->Option<UnitView>{self.world.game.units.get(&id).map(|u|self.unit_view(u))}
 fn route(&mut self,u:&UnitView,at:Point,safe:bool)->supply_convoys::Route {let native=&self.world.game.units[&u.id];let owner=u.owner.unwrap();let recipient=&self.recipients[owner];let query=recipient.query();let known=query.path_terrain(recipient.live()).expect("validated recipient terrain");let threats=self.threats(owner);self.routing.route(owner,self.world.game.tick as u64,known,&self.rules.kinds[native.kind],u.at,at,safe,&threats).expect("validated convoy route inputs")}
 fn continue_route(&mut self,u:&UnitView,_at:Point)->bool{let native=&self.world.game.units[&u.id];let recipient=&self.recipients[u.owner.unwrap()];SupplyRouting::continue_route(recipient.query().navigation(),&self.rules.kinds[native.kind],u.at,&u.path,u.stuck,&self.threats(u.owner.unwrap()))}
 fn handoff(&mut self,u:&UnitView,at:Point,entity:Option<u32>,radius:f64)->bool{
  let g=&self.world.game;let native=&g.units[&u.id];let start=self.combat.get(u.id).and_then(|c|c.carrier).and_then(|id|g.units.get(&id)).unwrap_or(native).at();
  let mut near=vec![0u8;g.flags.len()];for (c,&f) in g.flags.iter().enumerate(){if f&(MOVE|crate::game::SIGHT)==(MOVE|crate::game::SIGHT){let x=c%g.w;let y=c/g.w;for row in y.saturating_sub(1)..=(y+1).min(g.h-1){for col in x.saturating_sub(1)..=(x+1).min(g.w-1){near[row*g.w+col]=1;}}}}
  let fires=(0..g.flags.len()).map(|c|self.world.fires.contains_key(&c)).collect::<Vec<_>>();let owner=u.owner.unwrap();let team=self.players[owner].team;let mines=(0..g.flags.len()).map(|c|self.world.mines.get(&c).is_some_and(|o|*o!=team)&&self.world.mine_seen.get(&c).is_some_and(|mask|mask&(1u32<<team)!=0)).collect::<Vec<_>>();
  let bunkers=g.units.values().filter(|t|t.hp>0.0&&self.rules.kinds[t.kind].name=="bunker").map(Unit::at).collect::<Vec<_>>();
  let terrain=PathTerrain{navigation:TerrainView{versions:Versions{infantry:self.world.versions.infantry,obstruction:self.world.versions.obstruction,..Versions::default()},..view(g)},near_walls:&near,known_mines:&mines,fires:&fires,bunkers:&bunkers,wear:g.wear.as_deref(),wetness:g.wet.unwrap_or(0.0),roads:self.world.roads};
  let cells=entity.and_then(|id|g.units.get(&id)).and_then(|b|b.cells.as_deref()).unwrap_or(&[]);let rifle=&self.rules.kinds[self.rules.kinds.iter().position(|k|k.name=="rifle").unwrap()];
  self.routing.handoff(g.tick as u64,terrain,rifle,start,at,cells,radius).expect("validated physical handoff")
 }
 fn supply_rate(&mut self,owner:usize,at:Point)->Option<f64>{Some(self.network.rate_at(self.players.get(owner).map(|p|p.team),at).expect("validated supply position"))}
 fn spawn(&mut self,owner:usize,at:Point)->u32{
  let game=&self.world.game;let kind=self.rules.kinds.iter().position(|k|k.name=="truck").unwrap();let profile=&self.rules.kinds[kind];
  let nearby=game.units.values().filter(|u|u.air.is_none()&&!u.riding&&u.hp>0.0&&math::hypot(u.x-at.x,u.z-at.z)<30.0).map(|u|(u.at(),self.rules.kinds[u.kind].radius,self.rules.kinds[u.kind].name=="truck")).collect::<Vec<_>>();let n=nearby.iter().filter(|v|v.2).count();
  let placement=self.placer.scatter_position(view(game),profile,self.players[owner].spawn,game.units.len() as u32).expect("validated truck spawn");let id=*self.next_id;let mut unit=unit_spawn::initial_unit(self.rules,Catalog::compiled().unwrap(),kind,id,owner,placement.point).expect("compiled truck definition");
  for k in 0..40{let angle=(n+k) as f64*2.4;let radius=10.0+(k/16) as f64*5.0;let c=self.placer.nearest_free(view(game),Point{x:at.x+math::cos(angle)*radius,z:at.z+math::sin(angle)*radius},MOVE|VBLOCK,None).expect("validated truck scatter");let p=Point{x:(c%game.w) as f64*CELL+CELL/2.0,z:(c/game.w) as f64*CELL+CELL/2.0};if vehicle::vehicle_position_clear(game,Pose{x:p.x,z:p.z,rot:0.0},profile,MOVE|VBLOCK)&&nearby.iter().all(|(q,r,_)|math::hypot(q.x-p.x,q.z-p.z)>(r+profile.radius)*0.8+1.0){unit.x=p.x;unit.z=p.z;break;}}
  let mut control=UnitControl::new(&unit,self.rules).expect("compiled truck control");control.auto_retreat=false;self.commands.insert(id,control);self.firing.initialize(id);self.combat.initialize(id);self.world.game.units.insert(id,unit);self.grid.update(id,&self.world.game.units[&id]);*self.next_id+=1;id
 }
 fn retire(&mut self,id:u32){self.world.game.units.remove(&id);self.commands.units.remove(&id);self.firing.remove(id);self.combat.remove(id);self.grid.rebuild(&self.world.game.units);}
 fn apply(&mut self,id:u32,operation:UnitOperation){let u=self.world.game.units.get_mut(&id).unwrap();let c=self.commands.units.get_mut(&id).unwrap();match operation{UnitOperation::Route{path,world_goal,repath}=>{u.path=path;u.path_gen=u.path_gen.wrapping_add(1);u.world_goal=Some(world_goal);u.repath=repath;u.retreating=false;c.attack_id=0;self.firing.get_mut(id).unwrap().target_id=0;},UnitOperation::ClearRoute=>{u.path.clear();u.world_goal=None;},UnitOperation::ResumeClear=>{u.path.clear();u.world_goal=None;c.orders.clear();},UnitOperation::RepairRoute(path)=>{u.path=path;u.path_gen=u.path_gen.wrapping_add(1);u.stuck=0.0;},UnitOperation::NewTruck=>c.auto_retreat=false}}
}
