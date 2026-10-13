// Ground Attack/Retreat commands and full steps from the unchanged pinned simulation.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {gzipSync} from 'node:zlib';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
assert.ok(process.argv.slice(2).every(a=>a==='--check'));
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??join(root,'reference/ww2-rts'));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};
clean();
const sim=await import(pathToFileURL(join(reference,'shared/sim.js')));
const {rebuildGrid,gridFor}=await import(pathToFileURL(join(reference,'shared/grid.js')));
const data=new DataView(new ArrayBuffer(8));
const bits=n=>{data.setFloat64(0,n,false);return data.getBigUint64(0,false).toString(16).padStart(16,'0');};
const encode=v=>v===undefined?null:typeof v==='number'?{f64:bits(v)}:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;
const point=p=>p?{x:bits(p.x),z:bits(p.z)}:null;
const keys=o=>Object.fromEntries(o);
const quiet={holdFire:true,holdPos:true,auto:false,autoRetreat:false};
const controls=['follow','face','garrison','riding','board','retreating','attackId','targetId','enter','amove','fireAt','build','worldGoal','moveOutcome','moveOutcomeTick','moveResult','holdFire','holdPos','auto','autoRetreat','drift','reverse','traffic','trafficWait','pace','routeEnd','motionBefore','motionClearance'];
const timers=['cd','cooldown','retarget','repath','buff','sprint','react','still','stuck','supp','reinf','moveSpeed','vx','vz','travelDir'];
const map=()=>({name:'Ground order checkpoint',w:80,h:60,rows:Array(60).fill('.'.repeat(80)),spawns:[{x:1,y:1},{x:78,y:58}],points:[{x:40,y:58}]});
const wallMap=column=>{const terrain=map();terrain.rows=terrain.rows.map(row=>row.slice(0,column)+'W'+row.slice(column+1));return terrain;};
const spawn=(alias,type,x,z,extra={})=>({alias,type,owner:0,pose:{x,z},...extra});
const step=()=>({op:'step'}),prime=units=>({op:'prime',units});
const command=(t,ids,extra={},sequence)=>({op:'command',cmd:{t,ids,...extra},...(sequence===undefined?{}:{sequence})});
const oldRandom=Math.random,cases=[];let totalSteps=0,totalCommands=0;
try {
 function record(name,spawns,actions,{terrain=map(),budget=4096,retryLimit=1,unitPrime={},boundaryOnly=false,allowEffects=false,cacheKeyFacts=false,check=()=>{}}={}){
  let draws=0;Math.random=()=>{draws++;return .5;};
  const opts={weather:false,tech:false,supply:false,logistics:false},names=['Ground orders','Opposition'],teams=[0,1],factions=[0,1];
  assert.equal(sim.validateMap(terrain),null);
  const g=sim.createGame(terrain,names,false,teams,factions,opts),creationRandomDraws=draws;
  assert.equal(creationRandomDraws,1,`${name}: constructor external draw changed`);
  assert.equal(g.mode,undefined);assert.equal(g.wx,null);assert.equal(g.supply,false);assert.equal(g.logisticsEnabled,false);assert.equal(g.tech,false);
  const creation={seed:g.seed>>>0,nextId:g.nextId,actorOrder:[...g.units.keys()],units:[...g.units.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,...point(u),rot:bits(u.rot),hp:bits(u.hp)}))};
  const aliases={};
  for(const spec of spawns){const u=sim.internals.spawnUnit(g,spec.owner,spec.type);aliases[spec.alias]=u.id;Object.assign(u,spec.pose);}
  const resolveId=id=>typeof id==='string'?(aliases[id]??Number(id)):id;
  const resolveFields=fields=>keys(Object.entries(fields).map(([k,v])=>[k,['board','riding','follow','attackId','targetId'].includes(k)?resolveId(v):k==='cargo'?v.map(resolveId):structuredClone(v)]));
  for(const u of g.units.values())Object.assign(u,quiet);
  for(const [alias,fields]of Object.entries(unitPrime))Object.assign(g.units.get(resolveId(alias)),resolveFields(fields));
  g.pathBudget=budget;g.pathRetryLimit=retryLimit;
  rebuildGrid(g);sim.internals.updateVision(g);
  const setup={map:terrain,names,shuffle:false,teams,factions,opts,creation,initialActors:creation.actorOrder,spawns,aliases,prime:{allUnits:quiet,units:unitPrime,pathBudget:budget,pathRetryLimit:retryLimit},gridRadius:6,observation:{passiveGrid:true,rawFlags:false},creationRandomDraws,setupRandomDraws:draws-creationRandomDraws,scope:{boundaryOnly,allowEffects,damageParity:false,fullStepParity:!boundaryOnly}};
  assert.equal(setup.setupRandomDraws,0,`${name}: setup drew external randomness`);
  const initialWear=g.wear.slice(),initialSurfaceWear=g.surfaceWear.slice();
  const states=[],records=[];
  const observe=()=>{
   // Read the maintained index without rebuilding it or exposing private helpers.
   const candidates=new Map([...g.units.values()].map(u=>[u.id,gridFor(g).candidates(u,setup.gridRadius).map(v=>v.id)]));
   const s={tick:g.tick,actorOrder:[...g.units.keys()],units:[...g.units.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,...point(u),groundFlags:sim.internals.flagsAt(g,u.x,u.z),rot:bits(u.rot),aim:bits(u.aim),hp:bits(u.hp),cargo:[...u.cargo],path:u.path.map(point),orders:encode(u.orders),dig:encode(u.dig?{...u.dig,project:u.dig.project?.id}:null),entrench:u.entrench?.id??null,logistics:encode(u.logistics),controls:encode(keys(controls.map(k=>[k,u[k]]))),timers:encode(keys(timers.map(k=>[k,u[k]]))),grid:candidates.get(u.id)})),players:g.players.map(p=>({slot:p.slot,team:p.team,visible:[...p.visible].sort((a,b)=>a-b),spawn:point(p.spawn),mp:bits(p.mp),inc:bits(p.inc),mun:p.mun===undefined?null:bits(p.mun),fuel:p.fuel===undefined?null:bits(p.fuel),vp:bits(p.vp),out:p.out??null,away:p.away??null,sup:encode(p.sup)})),projects:encode([...(g.projects??[])].map(([id,p])=>[id,p])),projectSeq:g.projectSeq??0,story:encode(g.story),timeline:encode(g.timeline),points:encode(g.points),seed:g.seed>>>0,nextId:g.nextId,winner:g.winner,pathStats:encode(g.pathStats),randomDraws:draws,shots:encode(g.shots),terrain:{journal:encode(g.cellJournal),newCells:encode(g.newCells),cellLog:encode(g.cellLog),versions:encode(keys(['terrainVersion','obstructionVersion','infantryRegionVersion','vehicleRegionVersion','navalRegionVersion'].map(k=>[k,g[k]])))}};
   if(cacheKeyFacts)s.pathViewKey=`${g.tick}:${g.terrainVersion??0}:${g.visionTick??-1}`;
   states.push(s);return s;
  };
  observe();let sequence=0,commandsAtBoundary=false;
  function apply(input){const a=structuredClone(input),before=draws;let result=null;
   if(a.op==='command'){
    if(a.cmd.ids)a.cmd.ids=a.cmd.ids.map(resolveId);if(Object.hasOwn(a.cmd,'id'))a.cmd.id=resolveId(a.cmd.id);if(a.cmd.orders)a.cmd.orders=a.cmd.orders.map(([id,...at])=>[resolveId(id),...at]);if(Object.hasOwn(a.cmd,'target'))a.cmd.target=resolveId(a.cmd.target);
    const issuer=a.issuer??0;a.key={tick:g.tick+1,issuer,sequence:a.sequence??++sequence};delete a.sequence;delete a.issuer;
    result=sim.command(g,issuer,a.cmd)??null;totalCommands++;commandsAtBoundary=true;
   }else if(a.op==='prime'){assert.ok(!commandsAtBoundary,`${name}: prime after queued command`);for(const [alias,fields]of Object.entries(a.units??{})){const resolved=resolveFields(fields);Object.assign(g.units.get(resolveId(alias)),resolved);a.units[alias]=resolved;}rebuildGrid(g);if(a.vision!==false)sim.internals.updateVision(g);}
   else if(a.op==='step'){sim.step(g);totalSteps++;commandsAtBoundary=false;}
   else throw Error(a.op);
   if(!allowEffects)assert.equal(draws,before,`${name}: quiet ${a.op} drew Math.random`);observe();records.push({action:a,result,randomDraws:draws-before,lastSnapshot:states.length-1});
  }
  for(let i=0;i<actions.length;){if(actions[i].op!=='command'){apply(actions[i++]);continue;}const orders=[];while(i<actions.length&&actions[i].op==='command')orders.push(actions[i++]);orders.sort((a,b)=>(a.issuer??0)-(b.issuer??0)||(a.sequence??0)-(b.sequence??0));for(const order of orders)apply(order);}
  assert.ok(creation.actorOrder.every(id=>g.units.has(id)),`${name}: startup actor lost`);
  if(!allowEffects)assert.equal(draws,creationRandomDraws+setup.setupRandomDraws);
  if(!allowEffects)assert.ok(states.every(s=>s.shots.length===0),`${name}: unexpected shot`);
  assert.deepEqual(g.wear,initialWear,`${name}: excluded ground wear changed`);assert.deepEqual(g.surfaceWear,initialSurfaceWear,`${name}: excluded surface wear changed`);
  assert.equal(boundaryOnly,records.every(r=>r.action.op!=='step'),`${name}: boundary-only scope mismatch`);
  check({g,states,records,aliases,unit:(s,a)=>s.units.find(u=>u.id===aliases[a])});
  cases.push({name,setup,records,states});
 }

 const U=(alias,type,x,z,owner=0)=>spawn(alias,type,x,z,{owner});
 const scene=()=>[U('attacker','rifle',30,30),U('target','rifle',100,30,1),U('observer','rifle',98,34)];
 const attack=(ids=['attacker'],target='target',queue=false)=>command('attack',ids,{target,...(queue?{queue:true}:{})});
 const stop=(ids=['attacker'])=>command('stop',ids);
 const retreat=(ids=['attacker'])=>command('retreat',ids);
 const control=(s,a,k,aliases)=>s.units.find(u=>u.id===aliases[a]).controls[k];
 const admission=(reason)=>({records})=>assert.equal(records[0].result,reason);
 record('far-visible-pursuit-and-moving-target',scene(),[attack(),step(),prime({target:{x:106,z:34,path:[],worldGoal:null},observer:{x:104,z:38}}),step()],{check:({states,unit,aliases})=>{assert.equal(control(states[1],'attacker','attackId',aliases).f64,bits(aliases.target));assert.ok(unit(states[2],'attacker').path.length);assert.notEqual(unit(states[2],'attacker').x,unit(states[1],'attacker').x);assert.equal(unit(states[4],'target').x,bits(106));}});
 for(const [name,target]of [['missing',999],['ally','observer']])record('attack-'+name,scene(),[attack(['attacker'],target),step()],{unitPrime:{attacker:{follow:'observer'}},check:({states,records,aliases})=>{assert.equal(records[0].result,'unseen');assert.equal(control(states[1],'attacker','follow',aliases).f64,bits(aliases.observer));}});
 record('attack-hidden',[U('attacker','rifle',30,30),U('target','rifle',100,30,1)],[attack(),step()],{check:admission('unseen')});
 record('attack-visible-dead-before-retirement',scene(),[{op:'prime',units:{target:{hp:0}},vision:false},attack(),stop(),step()],{allowEffects:true,check:({states,records,aliases})=>{assert.equal(records[1].result,null);assert.equal(control(states[2],'attacker','attackId',aliases).f64,bits(aliases.target));assert.equal(states[2].units.find(u=>u.id===aliases.target).hp,bits(0));}});
 record('medic-no-range-queueFull',[U('attacker','medic',30,30),...scene().slice(1)],[attack(),step()],{unitPrime:{attacker:{follow:'observer'}},check:({states,records,aliases})=>{assert.equal(records[0].result,'queueFull');assert.equal(control(states[1],'attacker','follow',aliases).f64,bits(0));}});
 record('empty-selection-needs',scene(),[attack([]),step()],{check:admission('needs')});
 record('in-range-admission-stop-before-step',[U('attacker','rifle',90,30),...scene().slice(1)],[attack(),stop(),step()],{check:admission(null)});
 record('queued-singleton-repeated-ids-cap-eight',scene(),[attack(['attacker','attacker'],'target',true),...Array.from({length:7},()=>attack(['attacker'],'target',true)),stop(),step()],{check:({states,records,unit,aliases})=>{assert.equal(unit(states[1],'attacker').orders.length,2);assert.equal(unit(states[8],'attacker').orders.length,8);assert.equal(records[7].result,'queueFull');const row=unit(states[1],'attacker').orders[0];assert.deepEqual(row.ids,[{f64:bits(aliases.attacker)}]);assert.equal(row.target.f64,bits(aliases.target));assert.equal(row.x.f64,bits(100));assert.equal(row.z.f64,bits(30));}});
 record('queued-anchor-versus-later-target-pose',scene(),[attack(['attacker'],'target',true),step(),prime({target:{x:108,z:34,path:[],worldGoal:null},observer:{x:106,z:38}}),step()],{check:({states,unit})=>{assert.equal(unit(states[1],'attacker').orders[0].x.f64,bits(100));assert.equal(unit(states[4],'target').x,bits(108));}});
 record('mixed-selection-follow-and-admission',[...scene(),U('dead','rifle',36,30),U('rider','rifle',38,30),U('carrier','halftrack',38,30),U('operative','rifle',40,30),U('structure','bunker',42,30),U('rail','train',44,30),U('forced','rifle',46,30),U('stranded','rifle',48,30)],[attack(['dead','rider','operative','structure','rail','forced','stranded','attacker']),stop(['dead','stranded','attacker'])],{boundaryOnly:true,unitPrime:{dead:{hp:0,follow:'observer'},rider:{riding:'carrier',follow:'observer'},carrier:{cargo:['rider']},operative:{operative:true,follow:'observer'},structure:{follow:'observer'},rail:{follow:'observer'},forced:{follow:'observer',logistics:{forced:true,stranded:false}},stranded:{follow:'observer',logistics:{forced:true,stranded:true}},attacker:{follow:'observer'}},check:({states,records,aliases})=>{assert.equal(records[0].result,null);for(const a of ['dead','stranded','attacker'])assert.equal(control(states[1],a,'follow',aliases).f64,bits(0));for(const a of ['rider','operative','structure','rail','forced'])assert.equal(control(states[1],a,'follow',aliases).f64,bits(aliases.observer));}});
 record('ground-attack-air-target-needs',[U('attacker','rifle',30,30),U('target','fighter',50,30,1),U('observer','rifle',48,34)],[attack()],{boundaryOnly:true,unitPrime:{attacker:{follow:'observer'},target:{air:{state:'out',fuel:30,ammo:60,timer:0,mission:null}}},check:({states,records,aliases})=>{assert.equal(records[0].result,'needs');assert.equal(control(states[1],'attacker','follow',aliases).f64,bits(0));}});
 record('paid-dig-attack-cancellation-no-refund',[U('attacker','engineer',30,33),...scene().slice(1)],[command('dig',['attacker'],{kind:'trench',x:30,z:30,dir:0}),attack(),step()],{check:({states,unit})=>{assert.ok(unit(states[1],'attacker').dig);assert.equal(unit(states[2],'attacker').dig,null);assert.equal(states[1].players[0].mp,states[2].players[0].mp);}});
 record('shared-paid-job-attack-cancellation',[U('attacker','engineer',30,33),U('coworker','engineer',34,33),...scene().slice(1)],[command('entrench',['attacker','coworker'],{pattern:'line',fort:'trench',x:30,z:30,x2:38,z2:30}),step(),attack(),step()],{check:({states,unit})=>{assert.ok(unit(states[2],'attacker').dig);assert.equal(unit(states[3],'attacker').dig,null);assert.equal(unit(states[3],'attacker').entrench,null);assert.equal(states[2].players[0].mp,states[3].players[0].mp);}});
 record('retreat-default-home-repeated',[U('attacker','rifle',60,40)],[retreat(),step(),retreat(),step()],{check:({states,unit})=>{assert.equal(unit(states[1],'attacker').controls.retreating,true);assert.ok(unit(states[1],'attacker').path.length);assert.equal(unit(states[3],'attacker').controls.retreating,true);}});
 record('retreat-nearest-finished-own-HQ',[U('attacker','rifle',60,40),U('near','hq',50,40),U('unfinished','hq',59,40),U('enemyHQ','hq',58,40,1)],[retreat(),step()],{unitPrime:{near:{built:1},unfinished:{built:.5},enemyHQ:{built:1}},check:({states,unit})=>assert.deepEqual(unit(states[1],'attacker').controls.worldGoal,{x:{f64:bits(50)},z:{f64:bits(40)}})});
 const startupHQScene=[U('attacker','rifle',60,40)];
 // This explicit setup removes home eligibility by unfinished construction, retaining every actor identity.
 record('retreat-fallback-player-spawn',startupHQScene,[retreat(),step()],{unitPrime:{'7':{built:0}},check:({states,unit})=>assert.deepEqual(unit(states[1],'attacker').controls.worldGoal,states[1].players[0].spawn?{x:{f64:states[1].players[0].spawn.x},z:{f64:states[1].players[0].spawn.z}}:null)});
 const house=map();for(const y of [14,15,16]){const row=[...house.rows[y]];for(const x of [14,15,16])row[x]='B';house.rows[y]=row.join('');}
 record('retreat-garrison-physical-exit',[U('attacker','rifle',31,31)],[retreat(),step()],{terrain:house,unitPrime:{attacker:{garrison:15*80+15}},check:({states,unit})=>{assert.equal(unit(states[1],'attacker').controls.garrison.f64,bits(-1));assert.notEqual(unit(states[1],'attacker').x,bits(31));}});
 record('paid-dig-retreat-no-refund',[U('attacker','engineer',30,33)],[command('dig',['attacker'],{kind:'trench',x:30,z:30,dir:0}),retreat(),step()],{check:({states,unit})=>{assert.ok(unit(states[1],'attacker').dig);assert.equal(unit(states[2],'attacker').dig,null);assert.equal(states[1].players[0].mp,states[2].players[0].mp);}});

 record('deferred-attack-goal-moves',scene(),[attack(),step(),prime({target:{x:108,z:34,path:[],worldGoal:null},observer:{x:106,z:38}}),step()],{terrain:wallMap(25),budget:0,check:({states,unit,aliases})=>{assert.equal(states[2].pathStats.deferred.f64,bits(1));assert.equal(states[4].pathStats.deferred.f64,bits(2));assert.equal(unit(states[4],'attacker').controls.attackId.f64,bits(aliases.target));assert.equal(unit(states[4],'attacker').path.length,0);}});
 record('shared-paid-job-retreat-cancellation',[U('attacker','engineer',30,33),U('coworker','engineer',34,33)],[command('entrench',['attacker','coworker'],{pattern:'line',fort:'trench',x:30,z:30,x2:38,z2:30}),step(),retreat(),step()],{check:({states,unit})=>{assert.ok(unit(states[2],'attacker').dig);assert.equal(unit(states[3],'attacker').dig,null);assert.equal(unit(states[3],'attacker').entrench,null);assert.equal(states[2].players[0].mp,states[3].players[0].mp);}});
 record('garrison-in-range-attack-stays',[U('attacker','rifle',33,31),U('target','rifle',50,31,1),U('observer','rifle',48,35)],[attack(),stop()],{terrain:house,boundaryOnly:true,unitPrime:{attacker:{garrison:15*80+16}},check:({states,unit})=>{assert.equal(unit(states[1],'attacker').controls.garrison.f64,bits(15*80+16));assert.equal(unit(states[1],'attacker').x,bits(33));}});
 record('garrison-far-attack-physical-exit',[U('attacker','rifle',31,31),...scene().slice(1)],[attack(),step()],{terrain:house,unitPrime:{attacker:{garrison:15*80+15}},check:({states,unit})=>{assert.equal(unit(states[1],'attacker').controls.garrison.f64,bits(-1));assert.notEqual(unit(states[1],'attacker').x,bits(31));}});
 record('retreat-selection-repeats-forced-and-riders',[...scene(),U('rider','rifle',38,30),U('carrier','halftrack',38,30),U('operative','rifle',40,30),U('structure','bunker',42,30),U('rail','train',44,30),U('forced','rifle',46,30),U('dead','rifle',52,30)],[retreat(['rider','operative','structure','rail','forced','dead','attacker','attacker'])],{boundaryOnly:true,unitPrime:{rider:{riding:'carrier',follow:'observer'},carrier:{cargo:['rider']},operative:{operative:true,follow:'observer'},structure:{follow:'observer'},rail:{follow:'observer'},forced:{follow:'observer',logistics:{forced:true,stranded:false}},dead:{hp:0,follow:'observer'},attacker:{follow:'observer'}},check:({states,records,aliases})=>{assert.equal(records[0].result,null);for(const a of ['attacker','forced','dead'])assert.equal(control(states[1],a,'follow',aliases).f64,bits(0));for(const a of ['rider','operative','structure','rail'])assert.equal(control(states[1],a,'follow',aliases).f64,bits(aliases.observer));}});

 record('paid-job-unseen-refusal-retains-work',[U('attacker','engineer',30,33)],[command('dig',['attacker'],{kind:'trench',x:30,z:30,dir:0}),attack(['attacker'],999),step()],{check:({states,records,unit})=>{assert.equal(records[1].result,'unseen');assert.ok(unit(states[2],'attacker').dig);assert.equal(states[1].players[0].mp,states[2].players[0].mp);}});
 record('whole-malformed-order-rejection-before-follow',scene(),[command('attack',['attacker'],{target:'target',orders:[[1,'bad',20]]}),step()],{unitPrime:{attacker:{follow:'observer'}},check:({states,records,aliases})=>{assert.equal(records[0].result,'blocked');assert.equal(control(states[1],'attacker','follow',aliases).f64,bits(aliases.observer));}});

 // Move performs real findPath and warms observedPathView. Stop and the following
 // repeated group Retreat stay in that tick/terrain/vision epoch. Its cached unit
 // rows reference the original actors, so physical exits remain live route facts.
 record('warm-owner-cache-group-garrison-retreat',[
  U('warm','rifle',11,21),U('first','rifle',31,31),U('second','rifle',29,31),
 ],[
  command('move',['warm'],{orders:[['warm',13,21]]}),stop(['warm']),
  retreat(['first','second','first']),step(),
 ],{terrain:house,cacheKeyFacts:true,unitPrime:{
  first:{garrison:15*80+15,follow:'warm'},second:{garrison:15*80+14,follow:'warm'},
 },check:({states,records,unit,aliases})=>{
  assert.equal(records[0].result,null);assert.equal(records[1].result,null);assert.equal(records[2].result,null);
  assert.equal(states[1].pathStats.calls.f64,bits(1),'real Move must warm the source route view');
  for(const s of states.slice(1,4))assert.equal(s.pathViewKey,states[0].pathViewKey,'inputs must retain the warmed source epoch');
  assert.equal(states[3].pathStats.calls.f64,bits(4),'warm route plus all three recorded Retreat rows must search');
  assert.deepEqual(records[2].action.cmd.ids,[aliases.first,aliases.second,aliases.first]);
  for(const actor of ['first','second']){
   const before=unit(states[2],actor),exited=unit(states[3],actor),after=unit(states[4],actor);
   assert.notEqual(before.controls.garrison.f64,bits(-1));assert.equal(exited.controls.garrison.f64,bits(-1));
   assert.notDeepEqual([exited.x,exited.z],[before.x,before.z],'actual physical nearest-free exit');
   assert.equal(exited.groundFlags&sim.MOVE,0,'exit must occupy actual open ground');
   assert.ok(exited.path.length,'group must receive real source home routes');
   assert.equal(exited.controls.retreating,true);assert.equal(exited.controls.follow.f64,bits(0));
   assert.equal(after.controls.garrison.f64,bits(-1));assert.equal(after.controls.retreating,true);
   assert.equal(exited.dig,null);assert.equal(exited.entrench,null);
  }
  assert.equal(states[2].players[0].mp,states[3].players[0].mp,'Retreat neither spends nor refunds');
  assert.equal(states[0].actorOrder.length,13,'ten startup actors plus exactly three existing observers');
 }});
 // Source chooses home after the physical exit, which changes the nearest HQ.
 record('retreat-nearest-HQ-after-garrison-exit',[
  U('attacker','rifle',31,31),U('beforeExitHQ','hq',27,31),U('afterExitHQ','hq',36,31),
 ],[retreat(),step()],{terrain:house,unitPrime:{
  attacker:{garrison:15*80+15},beforeExitHQ:{built:1},afterExitHQ:{built:1},
 },check:({states,records,unit})=>{
  assert.equal(records[0].result,null);
  const before=unit(states[0],'attacker'),exited=unit(states[1],'attacker'),after=unit(states[2],'attacker');
  assert.deepEqual([before.x,before.z],[bits(31),bits(31)]);
  assert.deepEqual([exited.x,exited.z],[bits(35),bits(31)],'actual nearest-free physical exit');
  assert.equal(exited.controls.garrison.f64,bits(-1));assert.equal(exited.controls.retreating,true);
  assert.deepEqual(exited.controls.worldGoal,{x:{f64:bits(36)},z:{f64:bits(31)}},'nearest finished own HQ must use the exited pose');
  assert.deepEqual(after.controls.worldGoal,exited.controls.worldGoal);
  assert.equal(after.controls.garrison.f64,bits(-1));assert.equal(after.controls.retreating,true);
  assert.equal(states[0].players[0].mp,states[1].players[0].mp,'Retreat neither spends nor refunds');
  assert.ok(exited.path.length,'actual source home route');
  assert.equal(states[0].actorOrder.length,13,'ten startup actors plus one rifle and two finished HQs');
 }});
 const changes=(a,b,path=[],out=[])=>{if(Object.is(a,b))return out;if(Array.isArray(a)&&Array.isArray(b)&&a.length===b.length){for(let i=0;i<a.length;i++)changes(a[i],b[i],[...path,i],out);return out;}if(a&&b&&typeof a==='object'&&typeof b==='object'&&!Array.isArray(a)&&!Array.isArray(b)){for(const k of new Set([...Object.keys(a),...Object.keys(b)]))if(!Object.hasOwn(b,k))out.push([[...path,k]]);else changes(a[k],b[k],[...path,k],out);return out;}out.push([path,b]);return out;};
 for(const c of cases){c.initial=c.states[0];c.patches=c.states.slice(1).map((s,i)=>changes(c.states[i],s));c.snapshots=c.states.length;delete c.states;}
 clean();const content=JSON.stringify({reference:pin,node:process.version,arch:process.arch,boundary:'unchanged createGame + ordered command + full step, with explicit boundary-only unsupported scenes',scope:{damageParity:false,rawFlags:false,privateScheduler:false},steps:totalSteps,commands:totalCommands,cases})+'\n';
 const target=join(root,'rust/crates/simtest/tests/fixtures/ground-orders.json.gz'),bytes=gzipSync(content,{level:9,mtime:0});assert.equal(bytes.readUInt32LE(4),0);if(process.argv.includes('--check'))assert.deepEqual(readFileSync(target),bytes);else writeFileSync(target,bytes);
 console.error(`${process.argv.includes('--check')?'Verified':'Recorded'} ${cases.length} ground-order cases, ${totalCommands} commands, ${totalSteps} steps, ${cases.reduce((n,c)=>n+c.snapshots,0)} snapshots`);
}finally{Math.random=oldRandom;clean();}
