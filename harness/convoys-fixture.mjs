// Execute original store/convoy functions with deterministic caller route and handoff hooks.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url)),ref=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');assert.equal(execFileSync('git',['rev-parse','HEAD'],{cwd:ref,encoding:'utf8'}).trim(),'e4e579a838aabae3b1d9dadbc17a0a87147bc52d');execFileSync('git',['diff','--exit-code','HEAD','--','shared'],{cwd:ref});
const source=readFileSync(`${ref}/shared/supply-convoys.js`,'utf8').replace("'./logistics.js'",JSON.stringify(pathToFileURL(`${ref}/shared/logistics.js`).href))+'\nexport {syncStores,availableStock,consumeStock,stockTotal,storeSpace};\n';
const S=await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);const {initializeUnit}=await import(pathToFileURL(`${ref}/shared/logistics.js`));const {UNITS}=await import(pathToFileURL(`${ref}/shared/sim.js`));
const empty=()=>({ammo:0,provisions:0,fuel:0});
const unit=(id,type,owner,x,z)=>({id,type,owner,x,z,hp:100,built:1,still:3,cargo:[],path:[],worldGoal:null,orders:[],amove:null,stuck:0,attackId:0,targetId:0,retreating:false,repath:0,autoRetreat:true});
function projection(g){const snapshots=g.players.map(p=>S.logisticsSnapshot(g,p.slot));const s=g.convoys;return {snapshots,stores:[...s.stores.values()].map(v=>({id:v.id,owner:v.owner,x:v.x,z:v.z,source:v.source,entity:v.entity,capacity:v.capacity,buckets:[...v.buckets],donations:[...(v.donations??[])].map(([p,r])=>[p,[...r]]),active:v.active,support:!!v.support,point:v.point??null})),nextSpawn:[...s.nextSpawn].sort((a,b)=>a[0]-b[0]),losses:[...s.losses].sort((a,b)=>a[0]-b[0]),events:s.events.map(v=>({...v,x:v.x??null,z:v.z??null})),alerts:[...s.alerts].sort((a,b)=>a[0].localeCompare(b[0])),nextEvent:s.nextEvent,nextSchedule:s.nextSchedule,refreshQueue:s.refreshQueue,scheduleQueue:s.scheduleQueue,scheduled:[...s.scheduled].sort((a,b)=>a-b),assignments:[...s.assignments].sort((a,b)=>a[0]-b[0]),routed:s.routed,jobs:[...s.jobs].sort((a,b)=>a[0]-b[0]),units:[...g.units.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,x:u.x,z:u.z,hp:u.hp,built:u.built,still:u.still,cargo:u.cargo,path:u.path,worldGoal:u.worldGoal,orders:u.orders,amove:u.amove,stuck:u.stuck,attackId:u.attackId,targetId:u.targetId,retreating:u.retreating,repath:u.repath,autoRetreat:u.autoRetreat,riding:u.riding??null,regionCache:u.regionCache??null,logistics:u.logistics??null,convoy:u.convoy?{...u.convoy,routeFailure:u.convoy.routeFailure??null,retry:u.convoy.retry??null}:null}))};}
const scenarios=[];
for(const mode of ['conquest','classic','world'])for(const pop of [1,2.5])for(const many of [false,true]){
 const g={logisticsEnabled:true,mode:{kind:mode},army:{pop},tick:0,w:12,chars:Array(144).fill('.'),points:[{kind:'depot',owner:0,contested:false,x:9,z:11}],aid:new Map([[14,0]]),players:[{slot:0,team:0,spawn:{x:1,z:1},out:false,away:false},{slot:1,team:0,spawn:{x:21,z:21},out:false,away:false}],units:new Map(),winner:null};g.chars[14]='A';
 const list=[unit(1,'rifle',0,4,4),unit(2,'tank',0,90,90),unit(3,'rifle',1,22,22),unit(4,'supplycache',1,18,18),unit(5,'halftrack',0,5,5)];if(mode!=='conquest')list.push(unit(6,'hq',0,1,1),unit(7,'hq',1,21,21));for(const u of list){g.units.set(u.id,u);initializeUnit(g,u,UNITS[u.type]);}
 if(many)for(let id=8;id<14;id++){const u=unit(id,'tank',0,id<11?50+(id-8)*2:50+(id-8)*25,50);initializeUnit(g,u,UNITS.tank);Object.assign(u.logistics,{cutFor:10,supplyRate:0,provisions:14-id});g.units.set(id,u);}
 let nextId=20,config={rate:1,route:'good',handoff:true,move:false};const hooks={supplyRate:()=>config.rate,handoff:()=>config.handoff,route:(u,at)=>{if(config.route==='bad')return[];return[{x:at.x,z:at.z}];},routeReason:()=>config.route==='bad'?'blocked':null,continueRoute:()=>config.route==='continue',spawn:(owner,type,at)=>{const u=unit(nextId++,type,owner,at.x,at.z);g.units.set(u.id,u);return u;},retire:u=>g.units.delete(u.id)};
 S.setupConvoys(g);const initial=structuredClone(projection(g)),steps=[];
 const actions=[{tick:0,dt:0},{tick:1,dt:1,rate:0},{tick:40,dt:10,rate:0,reserve:{id:2,provisions:20,ammo:0,fuel:0,emergency:0}},{tick:80,dt:1,rate:0},{tick:81,dt:4,rate:0},{tick:82,dt:1,rate:0,move:true},{tick:83,dt:1,rate:0,move:true},{tick:84,dt:8,rate:0,move:true},{tick:120,dt:1,rate:0,move:true},{tick:160,dt:0,rate:1,route:'continue'},{tick:200,dt:10,rate:1},{tick:240,dt:0,rate:0,reserve:{id:2,provisions:0,ammo:0,cutFor:10},route:'bad'},{tick:280,dt:1,rate:0,route:'bad'},{tick:320,dt:1,rate:0,route:'bad'},{tick:360,dt:1,rate:0,route:'good',stuckTrucks:true},{tick:400,dt:0,rate:1,patch:{id:5,still:0}},{tick:440,dt:0,rate:1,patch:{id:5,still:3}},{tick:480,dt:0,rate:1,point:{contested:true}},{tick:520,dt:0,rate:1,point:{contested:false,owner:1}},{tick:560,dt:0,rate:1,patch:{id:4,owner:0}},{tick:600,dt:0,rate:1,patch:{id:5,hp:0}},{tick:601,dt:0,addTruck:true,owner:0,patch:{id:4,owner:1}},{tick:602,dt:0,seedCargo:{ammo:2,provisions:80,fuel:10}},{tick:603,dt:0,supply:4,owner:0},{tick:604,dt:0,move:true},{tick:605,dt:4},{tick:606,dt:4},{tick:607,dt:0,resume:true,owner:0},{tick:608,dt:0,death:true},{tick:609,dt:0,addTruck:true,owner:0},{tick:610,dt:0,command:'stop',owner:0},{tick:611,dt:0,command:'move',owner:0,x:55,z:55,queue:false},{tick:612,dt:0,command:'retreat',owner:0},{tick:613,dt:0,seedCargo:{ammo:3,provisions:100,fuel:0}},{tick:614,dt:0,supply:3,owner:0},{tick:615,dt:0,move:true},{tick:616,dt:4,teamChange:true},{tick:617,dt:0,addTruck:true,restoreTeam:true,supply:3,owner:0,rate:0,seedUnload:true},{tick:618,dt:1,teamChange:true,seedUnload:true},{tick:620,dt:0,support:{id:1,owner:0,amount:5}},{tick:621,dt:0,support:{id:2,owner:0,amount:5}},{tick:622,dt:0,support:{id:3,owner:0,amount:5}},{tick:623,dt:0,support:{id:4,owner:1,amount:5}},{tick:640,dt:0,addTruck:true,owner:0,seedTrip:'reuse',route:'continue'},{tick:680,dt:0,seedTrip:'repair',route:'good'}];
 for(const a of actions){g.tick=a.tick;config={...config,...a};if(a.reserve){const{id,...v}=a.reserve;Object.assign(g.units.get(id).logistics,v);}if(a.patch){const{id,...v}=a.patch;Object.assign(g.units.get(id),v);}if(a.restoreTeam)g.players[1].team=0;if(a.teamChange)g.players[1].team=1;
  if(a.point)Object.assign(g.points[0],a.point);if(a.stuckTrucks)for(const u of g.units.values())if(u.type==='truck'&&u.path.length)u.stuck=1;
  if(a.move)for(const u of g.units.values())if(u.type==='truck'&&u.worldGoal){Object.assign(u,u.worldGoal);u.path=[];u.worldGoal=null;}
  if(a.addTruck)S.addTruck(g,a.owner,[...g.convoys.stores.values()].find(s=>s.source&&s.owner===a.owner),hooks);
  const truck=[...g.units.values()].filter(u=>u.type==='truck').at(-1);
  if(a.seedCargo&&truck)truck.convoy.cargo={...a.seedCargo};
  if(a.seedTrip&&truck){truck.convoy.state='returning';const origin=[...g.convoys.stores.values()].find(s=>s.source&&s.owner===truck.owner);truck.convoy.origin=origin.id;truck.convoy.destination={x:origin.x,z:origin.z};truck.convoy.route=[{x:70,z:70},{x:origin.x,z:origin.z}];truck.path=structuredClone(truck.convoy.route);truck.worldGoal={x:origin.x,z:origin.z};truck.convoy.manual=false;truck.convoy.hold=false;truck.x=100;truck.z=100;truck.stuck=a.seedTrip==='repair'?1:0;}
  if(a.supply&&truck)S.commandConvoy(g,a.owner,{t:'supply',ids:[truck.id],target:a.supply},hooks);
  if(a.seedUnload&&truck){truck.convoy.state='unloading';truck.convoy.timer=8;truck.convoy.cargo={ammo:3,provisions:100,fuel:0};truck.x=22;truck.z=22;truck.path=[];truck.worldGoal=null;g.units.get(3).logistics.provisions=0;g.units.get(3).logistics.cutFor=10;}
  if(a.command&&truck)S.commandConvoy(g,a.owner,{t:a.command,ids:[truck.id],x:a.x,z:a.z,queue:a.queue},hooks);
  if(a.resume&&truck)S.commandConvoy(g,a.owner,{t:'logisticsResume',ids:[truck.id]},hooks);
  if(a.death&&truck){S.convoyDeath(g,truck);g.units.delete(truck.id);}
  const supportResult=a.support?S.supportProvision(g,g.units.get(a.support.id),a.support.owner,a.support.amount,hooks):null;
  S.stepConvoys(g,a.dt,hooks);steps.push({action:a,supportResult,expected:structuredClone(projection(g))});}
 scenarios.push({mode,pop,many,initial,steps});
}
// Ownership pools and grants come from original private functions, exposed only in the external recorder.
const pools=[];
for(const relief of [true,false])for(const payer of [0,1,2]){const store={id:'cache:1',owner:1,x:0,z:0,capacity:{ammo:20,provisions:200,fuel:300},buckets:new Map([[0,{ammo:10,provisions:100,fuel:90}],[1,{ammo:2,provisions:10,fuel:20}],[2,{ammo:5,provisions:50,fuel:30}]]),donations:new Map([[0,new Map([[1,{ammo:4,provisions:40,fuel:45}],[2,{ammo:2,provisions:20,fuel:15}]])],[2,new Map([[1,{ammo:3,provisions:30,fuel:10}]])]])};const before=S.availableStock(store,payer,relief);S.consumeStock(store,payer,{ammo:7,provisions:65,fuel:80},relief);pools.push({relief,payer,before,after:S.availableStock(store,payer,relief),buckets:[...store.buckets],donations:[...store.donations].map(([p,r])=>[p,[...r]]),total:S.stockTotal(store),space:S.storeSpace(store)});}
// Isolated support spending reaches source bypass, donation consumption and canonical fallback.
const support=[];
for(const kind of ['source','grant','fallback','captured','dead']) {
 const u=unit(2,'tank',kind==='captured'?1:0,90,90); if(kind==='dead')u.hp=0;
 const g={logisticsEnabled:true,mode:{kind:'conquest'},players:[{slot:0,team:0},{slot:1,team:0}],units:new Map([[2,u]])}; initializeUnit(g,u,UNITS.tank); S.setupConvoys(g);
 const owner=kind==='grant'?1:0,amount=5;
 const store={id:'cache:4',owner:1,x:90,z:90,source:kind==='source',active:true,entity:4,capacity:{ammo:20,provisions:200,fuel:300},buckets:new Map([[0,{ammo:0,provisions:30,fuel:0}]]),donations:new Map([[0,new Map([[1,{ammo:0,provisions:20,fuel:0}]])]])};
 if(kind==='source'||kind==='grant')g.convoys.stores.set(store.id,store);
 const result=S.supportProvision(g,u,owner,amount,{handoff:()=>true});
 support.push({kind,owner,amount,result,provisions:u.logistics.provisions,buckets:[...store.buckets],donations:[...store.donations].map(([p,r])=>[p,[...r]])});
}
// Record field deltas instead of duplicating the complete map of inventories on every tick.
function differences(a,b,path=[],out=[]) {
 if(Object.is(a,b))return out;
 if(Array.isArray(a)&&Array.isArray(b)&&a.length===b.length){for(let i=0;i<b.length;i++)differences(a[i],b[i],[...path,i],out);return out;}
 if(a&&b&&typeof a==='object'&&typeof b==='object'&&!Array.isArray(a)&&!Array.isArray(b)&&Object.keys(a).sort().join('|')===Object.keys(b).sort().join('|')){for(const k of Object.keys(b))differences(a[k],b[k],[...path,k],out);return out;}
 out.push([path,b]);return out;
}
for(const scenario of scenarios){let previous=scenario.initial;for(const step of scenario.steps){const current=step.expected;step.delta=differences(previous,current);delete step.expected;previous=current;}}
const output=JSON.stringify({reference:'e4e579a',scenarios,pools,support})+'\n',path=`${root}/rust/crates/core/tests/fixtures/convoys.json`;if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),output);else writeFileSync(path,output);console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} ${scenarios.length} startup/runtime traces, ${scenarios.reduce((n,s)=>n+s.steps.length,0)} steps, ${pools.length} payer-pool cases`);
