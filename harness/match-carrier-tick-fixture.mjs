// Full carrier ticks from the unchanged pinned simulation.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
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
const map=()=>({name:'Full carrier ticks',w:40,h:40,rows:Array(40).fill('.'.repeat(40)),spawns:[{x:1,y:1},{x:38,y:38}],points:[{x:20,y:38}]});
const roadMap=()=>{const terrain=map();for(const y of [14,15,16,19,20,21])terrain.rows[y]='.'.repeat(14)+'D'.repeat(14)+'.'.repeat(12);return terrain;};
const wallMap=column=>{const terrain=map();terrain.rows=terrain.rows.map(row=>row.slice(0,column)+'W'+row.slice(column+1));return terrain;};
const wallRoadMap=()=>{const terrain=wallMap(15);for(const y of [14,15,16])terrain.rows[y]=terrain.rows[y].slice(0,17)+'D'.repeat(14)+terrain.rows[y].slice(31);return terrain;};
const spawn=(alias,type,x,z,extra={})=>({alias,type,owner:0,pose:{x,z},...extra});
const rider=()=>spawn('rider','rifle',30,30),carrier=type=>spawn('carrier',type,34,30);
const step=()=>({op:'step'}),prime=units=>({op:'prime',units});
const command=(t,ids,extra={},sequence)=>({op:'command',cmd:{t,ids,...extra},...(sequence===undefined?{}:{sequence})});
const board=(target='carrier',ids=['rider'],sequence)=>command('board',ids,target===undefined?{}:{target},sequence);
const oldRandom=Math.random,cases=[];let totalSteps=0,totalCommands=0;
try {
 function record(name,spawns,actions,{terrain=map(),budget=4096,retryLimit=1,unitPrime={},check=()=>{}}={}){
  let draws=0;Math.random=()=>{draws++;return .5;};
  const opts={weather:false,tech:false,supply:false,logistics:false},names=['Carrier crew','Opposition'],teams=[0,1],factions=[0,1];
  assert.equal(sim.validateMap(terrain),null);
  const g=sim.createGame(terrain,names,false,teams,factions,opts),creationRandomDraws=draws;
  assert.equal(g.mode,undefined);assert.equal(g.wx,null);assert.equal(g.supply,false);assert.equal(g.logisticsEnabled,false);assert.equal(g.tech,false);
  const creation={seed:g.seed>>>0,nextId:g.nextId,actorOrder:[...g.units.keys()],units:[...g.units.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,...point(u),rot:bits(u.rot),hp:bits(u.hp)}))};
  const aliases={};
  for(const spec of spawns){const u=sim.internals.spawnUnit(g,spec.owner,spec.type);aliases[spec.alias]=u.id;Object.assign(u,spec.pose);}
  const resolveId=id=>typeof id==='string'?aliases[id]:id;
  const resolveFields=fields=>keys(Object.entries(fields).map(([k,v])=>[k,['board','riding'].includes(k)?resolveId(v):k==='cargo'?v.map(resolveId):structuredClone(v)]));
  for(const u of g.units.values())Object.assign(u,quiet);
  for(const [alias,fields]of Object.entries(unitPrime))Object.assign(g.units.get(resolveId(alias)),resolveFields(fields));
  g.pathBudget=budget;g.pathRetryLimit=retryLimit;
  rebuildGrid(g);sim.internals.updateVision(g);
  const setup={map:terrain,names,shuffle:false,teams,factions,opts,creation,initialActors:creation.actorOrder,spawns,aliases,prime:{allUnits:quiet,units:unitPrime,pathBudget:budget,pathRetryLimit:retryLimit},gridRadius:6,observation:{passiveGrid:true,rawFlags:false},creationRandomDraws,setupRandomDraws:draws-creationRandomDraws};
  const initialWear=g.wear.slice(),initialSurfaceWear=g.surfaceWear.slice();
  const states=[],records=[];
  const observe=()=>{
   // Read the maintained index without rebuilding it or exposing private helpers.
   const candidates=new Map([...g.units.values()].map(u=>[u.id,gridFor(g).candidates(u,setup.gridRadius).map(v=>v.id)]));
   const s={tick:g.tick,actorOrder:[...g.units.keys()],units:[...g.units.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,...point(u),rot:bits(u.rot),aim:bits(u.aim),hp:bits(u.hp),cargo:[...u.cargo],path:u.path.map(point),orders:encode(u.orders),dig:encode(u.dig),entrench:encode(u.entrench),controls:encode(keys(controls.map(k=>[k,u[k]]))),timers:encode(keys(timers.map(k=>[k,u[k]]))),grid:candidates.get(u.id)})),players:g.players.map(p=>({slot:p.slot,team:p.team,mp:bits(p.mp),inc:bits(p.inc),mun:p.mun===undefined?null:bits(p.mun),fuel:p.fuel===undefined?null:bits(p.fuel),vp:bits(p.vp),out:p.out??null,away:p.away??null,sup:encode(p.sup)})),story:encode(g.story),timeline:encode(g.timeline),points:encode(g.points),seed:g.seed>>>0,nextId:g.nextId,winner:g.winner,pathStats:encode(g.pathStats),randomDraws:draws,shots:encode(g.shots),terrain:{journal:encode(g.cellJournal),newCells:encode(g.newCells),cellLog:encode(g.cellLog),versions:encode(keys(['terrainVersion','obstructionVersion','infantryRegionVersion','vehicleRegionVersion','navalRegionVersion'].map(k=>[k,g[k]])))}};
   states.push(s);return s;
  };
  observe();let sequence=0,commandsAtBoundary=false;
  function apply(input){const a=structuredClone(input),before=draws;let result=null;
   if(a.op==='command'){
    a.cmd.ids=a.cmd.ids.map(resolveId);if(a.cmd.orders)a.cmd.orders=a.cmd.orders.map(([id,...at])=>[resolveId(id),...at]);if(Object.hasOwn(a.cmd,'target'))a.cmd.target=resolveId(a.cmd.target);
    const issuer=a.issuer??0;a.key={tick:g.tick+1,issuer,sequence:a.sequence??++sequence};delete a.sequence;delete a.issuer;
    result=sim.command(g,issuer,a.cmd)??null;totalCommands++;commandsAtBoundary=true;
   }else if(a.op==='prime'){assert.ok(!commandsAtBoundary,`${name}: prime after queued command`);for(const [alias,fields]of Object.entries(a.units??{})){const resolved=resolveFields(fields);Object.assign(g.units.get(resolveId(alias)),resolved);a.units[alias]=resolved;}rebuildGrid(g);sim.internals.updateVision(g);}
   else if(a.op==='step'){sim.step(g);totalSteps++;commandsAtBoundary=false;}
   else throw Error(a.op);
   assert.equal(draws,before,`${name}: quiet ${a.op} drew Math.random`);observe();records.push({action:a,result,randomDraws:draws-before,lastSnapshot:states.length-1});
  }
  for(let i=0;i<actions.length;){if(actions[i].op!=='command'){apply(actions[i++]);continue;}const orders=[];while(i<actions.length&&actions[i].op==='command')orders.push(actions[i++]);orders.sort((a,b)=>(a.issuer??0)-(b.issuer??0)||(a.sequence??0)-(b.sequence??0));for(const order of orders)apply(order);}
  assert.ok(creation.actorOrder.every(id=>g.units.has(id)),`${name}: startup actor lost`);
  assert.equal(draws,creationRandomDraws+setup.setupRandomDraws);
  assert.ok(states.every(s=>s.shots.length===0),`${name}: unexpected shot`);
  assert.deepEqual(g.wear,initialWear,`${name}: excluded ground wear changed`);assert.deepEqual(g.surfaceWear,initialSurfaceWear,`${name}: excluded surface wear changed`);
  check({g,states,records,aliases,unit:(s,a)=>s.units.find(u=>u.id===aliases[a])});
  cases.push({name,setup,records,states});
 }
 const seededTimers={cd:5,cooldown:6,retarget:7,repath:8,buff:9,sprint:0,react:11,still:12,supp:9};
 for(const carrierFirst of [false,true])record(`halftrack-${carrierFirst?'carrier':'rider'}-first-board-and-ride`,carrierFirst?[carrier('halftrack'),rider()]:[rider(),carrier('halftrack')],[prime({rider:seededTimers}),board(),step(),command('move',['carrier'],{orders:[['carrier',50,30]]}),step(),prime({carrier:{x:50,z:40,rot:.75,path:[],worldGoal:null,moveSpeed:0}}),step()],{terrain:roadMap(),check:({states,unit,aliases})=>{
  const boarded=states[3],beforeMove=states[4],moved=states[5],r=unit(boarded,'rider');assert.equal(r.controls.riding.f64,bits(aliases.carrier));assert.equal(r.controls.board.f64,bits(0));assert.deepEqual(unit(boarded,'carrier').cargo,[aliases.rider]);assert.equal(r.timers.still.f64,bits(12));assert.equal(r.controls.motionBefore,null);assert.equal(r.timers.cd.f64,bits(4.95));
  assert.notEqual(unit(moved,'carrier').x,unit(beforeMove,'carrier').x,'carrier must really move');
  assert.equal(unit(moved,'rider').x,unit(carrierFirst?moved:beforeMove,'carrier').x);assert.equal(unit(moved,'rider').z,unit(carrierFirst?moved:beforeMove,'carrier').z);
  for(const k of ['cooldown','retarget','repath','buff','sprint','react'])assert.equal(unit(moved,'rider').timers[k].f64,r.timers[k].f64,`riding changed ${k}`);
  const last=states.at(-1);assert.equal(unit(last,'rider').x,bits(50));assert.equal(unit(last,'rider').z,bits(40));assert.equal(unit(last,'rider').rot,bits(.75));assert.equal(unit(last,'rider').timers.supp.f64,bits(0));assert.equal(unit(last,'rider').timers.still.f64,bits(0));
 }});
 record('tank-ordered-two-riders',[carrier('tank'),rider(),spawn('rider2','rifle',34,34)],[board('carrier',['rider2','rider']),step(),command('unload',['carrier']),step()],{check:({states,unit,aliases})=>{assert.deepEqual(unit(states[2],'carrier').cargo,[aliases.rider,aliases.rider2]);assert.equal(unit(states.at(-1),'rider').controls.riding.f64,bits(0));assert.equal(unit(states.at(-1),'rider2').controls.riding.f64,bits(0));}});
 record('truck-ordered-command-boundary',[carrier('lorry'),rider(),spawn('rider2','rifle',34,34),spawn('rider3','rifle',38,30)],[board('carrier',['rider3'],3),board('carrier',['rider2'],2),board('carrier',['rider'],1),step(),command('unload',['carrier']),step()],{check:({states,records,unit,aliases})=>{assert.deepEqual(records.slice(0,3).map(r=>r.action.key.sequence),[1,2,3]);assert.deepEqual(unit(states[4],'carrier').cargo,[aliases.rider,aliases.rider2,aliases.rider3]);assert.equal(unit(states.at(-1),'carrier').cargo.length,0);}});
 record('explicit-target-refusal-and-omission',[rider(),carrier('halftrack'),spawn('enemy','halftrack',50,30,{owner:1})],[board(-1),board(0),board('enemy'),command('board',['rider','carrier']),step()],{check:({records,states,unit,aliases})=>{assert.deepEqual(records.slice(0,4).map(r=>r.result),['unseen','unseen','unseen',null]);assert.ok(!Object.hasOwn(records[3].action.cmd,'target'));assert.equal(unit(states.at(-1),'rider').controls.riding.f64,bits(aliases.carrier));}});
 record('deferred-moving-board-goal',[spawn('rider','rifle',20,30),spawn('carrier','halftrack',40,30)],[board(),command('move',['carrier'],{orders:[['carrier',50,30]]}),step(),step()],{terrain:wallRoadMap(),budget:0,check:({states,unit,aliases})=>{assert.equal(unit(states[1],'rider').path.length,0);assert.equal(states[1].pathStats.failed.f64,bits(1));assert.equal(states[3].pathStats.deferred.f64,bits(1));assert.equal(states[4].pathStats.deferred.f64,bits(2));assert.equal(states[4].pathStats.calls.f64,bits(2));assert.notEqual(unit(states[2],'carrier').x,unit(states[3],'carrier').x);assert.notEqual(unit(states[3],'carrier').x,unit(states[4],'carrier').x);assert.equal(unit(states[4],'rider').controls.board.f64,bits(aliases.carrier));assert.equal(unit(states[4],'rider').path.length,0);}});
 record('near-physically-blocked-deferred-board',[rider(),carrier('halftrack')],[board(),step(),step()],{terrain:wallMap(16),budget:0,check:({states,unit,aliases})=>{const first=states[2],last=states[3];assert.equal(unit(states[1],'rider').path.length,0);assert.equal(states[1].pathStats.failed.f64,bits(1));assert.equal(first.pathStats.deferred.f64,bits(1));assert.equal(last.pathStats.deferred.f64,bits(2));assert.equal(last.pathStats.calls.f64,bits(1));assert.equal(unit(last,'rider').controls.board.f64,bits(aliases.carrier));assert.equal(unit(last,'rider').controls.riding.f64,bits(0));assert.ok(sim.internals.dist({x:30,z:30},{x:34,z:30})<=sim.CFG.aid.board);}});
 {const terrain=map();for(const y of [11,12,13]){const row=[...terrain.rows[y]];for(const x of [11,12,13])if(x!==12||y!==12)row[x]='W';terrain.rows[y]=row.join('');}record('retry-one-failed-board-target-drop',[spawn('rider','rifle',25,25),spawn('carrier','halftrack',40,25)],[board(),step()],{terrain,budget:1,check:({states,unit})=>{assert.equal(states.at(-1).pathStats.dropped.f64,bits(1));assert.equal(unit(states.at(-1),'rider').controls.board.f64,bits(0));assert.equal(unit(states.at(-1),'rider').controls.moveOutcome,'unreachable');}});}
 record('paid-dig-full-refusal-unload-board',[spawn('rider','engineer',30,33),carrier('halftrack'),spawn('occupant','rifle',34,30)],[command('dig',['rider'],{kind:'trench',x:30,z:30,dir:0}),board(),step(),command('unload',['carrier'],{},1),board('carrier',['rider'],2),step()],{unitPrime:{occupant:{riding:'carrier'},carrier:{cargo:['occupant']}},check:({states,records,unit,aliases})=>{assert.equal(records[1].result,'max');assert.ok(unit(states[2],'rider').dig);assert.ok(unit(states[3],'rider').dig);assert.equal(unit(states[5],'rider').dig,null);assert.equal(states[4].players[0].mp,states[5].players[0].mp);assert.equal(unit(states[6],'rider').controls.board.f64,bits(aliases.carrier));assert.equal(states[3].story[0].mpSpent.f64,states[6].story[0].mpSpent.f64);}});
 for(const carrierFirst of [false,true])record(`stale-cargo-${carrierFirst?'carrier':'rider'}-first`,carrierFirst?[carrier('halftrack'),rider()]:[rider(),carrier('halftrack')],[step()],{unitPrime:{carrier:{cargo:[999]},rider:{board:'carrier'}},check:({states,unit,aliases})=>{assert.deepEqual(unit(states[1],'carrier').cargo,carrierFirst?[aliases.rider]:[]);assert.equal(unit(states[1],'rider').controls.riding.f64,bits(carrierFirst?aliases.carrier:0));assert.equal(unit(states[1],'rider').controls.board.f64,bits(0));}});
 const changes=(a,b,path=[],out=[])=>{if(Object.is(a,b))return out;if(Array.isArray(a)&&Array.isArray(b)&&a.length===b.length){for(let i=0;i<a.length;i++)changes(a[i],b[i],[...path,i],out);return out;}if(a&&b&&typeof a==='object'&&typeof b==='object'&&!Array.isArray(a)&&!Array.isArray(b)){for(const k of new Set([...Object.keys(a),...Object.keys(b)]))if(!Object.hasOwn(b,k))out.push([[...path,k]]);else changes(a[k],b[k],[...path,k],out);return out;}out.push([path,b]);return out;};
 for(const c of cases){c.initial=c.states[0];c.patches=c.states.slice(1).map((s,i)=>changes(c.states[i],s));c.snapshots=c.states.length;delete c.states;}
 clean();const content=JSON.stringify({reference:pin,node:process.version,arch:process.arch,boundary:'unmodified createGame + ordered command + full step; native consumer proof separate',random:{value:bits(.5),quietActionDraws:0},steps:totalSteps,commands:totalCommands,cases})+'\n';
 const target=join(root,'rust/crates/simtest/tests/fixtures/match-carrier-tick.json');if(process.argv.includes('--check'))assert.equal(readFileSync(target,'utf8'),content);else writeFileSync(target,content);
 console.error(`${process.argv.includes('--check')?'Verified':'Recorded'} ${cases.length} full carrier cases, ${totalCommands} commands, ${totalSteps} steps, ${cases.reduce((n,c)=>n+c.snapshots,0)} snapshots`);
}finally{Math.random=oldRandom;clean();}
