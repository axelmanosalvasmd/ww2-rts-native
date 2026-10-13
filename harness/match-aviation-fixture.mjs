// Calls unchanged prototype aircraft initialization, mission, movement and AA helpers.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {gzipSync} from 'node:zlib';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`),pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
registerHooks({load(url,context,next){const r=next(url,context);if(url!==pathToFileURL(resolve(reference,'shared/sim.js')).href)return r;return {...r,source:String(r.source)+'\nexport {airBase,sendPlane,stepPlane,antiAir,placeUnit,rebuildGrid,updateGrid,gridFor};\n'};}});
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const buf=new DataView(new ArrayBuffer(8)),bits=n=>{buf.setFloat64(0,n);return buf.getBigUint64(0).toString(16).padStart(16,'0');};
const unit=u=>({id:u.id,owner:u.owner,kind:u.type,x:bits(u.x),z:bits(u.z),rot:bits(u.rot),hp:bits(u.hp),supp:bits(u.supp),air:u.air?.state??null,still:bits(u.still),vx:bits(u.vx),vz:bits(u.vz),moveSpeed:bits(u.moveSpeed),travelDir:bits(u.travelDir),path:u.path,orders:u.orders,auto:u.auto,autoRetreat:u.autoRetreat,aim:bits(u.aim),cooldown:bits(u.cooldown),retarget:bits(u.retarget),targetId:u.targetId,hitBy:u.hitBy,hitAt:bits(u.hitAt)});
const map={name:'Aircraft source arena',w:40,h:40,rows:Array(40).fill('.'.repeat(40)),spawns:[{x:3,y:3},{x:36,y:36}],points:[{id:'front',x:20,y:20}]};
const constructors=[];
for(const kind of ['fighter','attacker','bomber'])for(const owner of [0,1])for(const index of [0,3,19]){
 const old=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(map,['A','B'],false,[0,1],[0,1],{weather:false,logistics:false,tech:false});}finally{Math.random=old;}
 g.units.clear();g.nextId=1000;const u=sim.internals.spawnUnit(g,owner,kind,index);
 constructors.push({kind,owner,index,map,spawn:g.players[owner].spawn,base:{x:u.x,z:u.z},expected:unit(u)});
}

function game(){const old=Math.random;Math.random=()=>.5;try{const g=sim.createGame(map,['A','B'],false,[0,1],[0,1],{weather:false,logistics:false,tech:false});g.units.clear();g.nextId=1000;sim.rebuildGrid(g);return g;}finally{Math.random=old;}}
function patch(g,u,p){
 const plain={};for(const k of ['x','z','hp','rot','still','built','xp','lastHit','holdFire','aaShot','aaReserve'])if(k in p)plain[k]=p[k];
 Object.assign(u,plain);
 if(p.air)Object.assign(u.air,structuredClone(p.air));
 if('logistics' in p)u.logistics=structuredClone(p.logistics);
 sim.updateGrid(g,u);
}
const reserves=(ammo,extra={})=>({ammo,ammoMax:100,ammoPrice:.01,ammoCredit:0,provisions:120,fuel:180,emergency:60,warning:null,forced:false,stranded:false,exhausted:false,...extra});
const patrol={kind:'patrol',id:0,x:40,z:40};
const plane=(kind='fighter',owner=0,extra={})=>({kind,owner,x:38,z:38,holdFire:true,air:{state:'out',mission:patrol},...extra});
const flak=(extra={})=>({kind:'flak',owner:0,x:40,z:40,still:3,holdFire:false,...extra});
const snapshot=g=>({units:[...g.units.values()].map(u=>({id:u.id,kind:u.type,owner:u.owner,x:bits(u.x),z:bits(u.z),rot:bits(u.rot),hp:bits(u.hp),cooldown:bits(u.cooldown),retarget:bits(u.retarget),targetId:u.targetId,aim:bits(u.aim),holdFire:!!u.holdFire,xp:bits(u.xp),lastHit:u.lastHit??null,downed:!!u.downed,aaShot:u.aaShot===undefined?null:bits(u.aaShot),aaReserve:u.aaReserve===undefined?null:bits(u.aaReserve),logistics:u.logistics?{ammo:bits(u.logistics.ammo),forced:u.logistics.forced,stranded:u.logistics.stranded,exhausted:u.logistics.exhausted}:null,air:u.air?{state:u.air.state,fuel:bits(u.air.fuel),ammo:bits(u.air.ammo),timer:bits(u.air.timer),ang:bits(u.air.ang),mission:u.air.mission?{kind:u.air.mission.kind,id:u.air.mission.id??0,x:bits(u.air.mission.x),z:bits(u.air.mission.z)}:null}:null})),story:g.players.map((_,i)=>bits(g.story?.[i]?.planesDowned??0)),events:structuredClone(g.shots)});
const traces=[];
function trace(name,specs,actions,visible=true){
 const g=game();for(const spec of specs){const u=sim.internals.spawnUnit(g,spec.owner,spec.kind);patch(g,u,spec);}
 for(const p of g.players)p.visible=new Set(visible?[...g.units.keys()]:[]);
 g.shots=[];const initial=snapshot(g),frames=[];
 for(const action of actions){g.shots=[];let admitted=null;
  if(action.op==='step'){const u=g.units.get(action.id);if(u.hp>0)sim.stepPlane(g,u,action.dt);}
  else if(action.op==='aa')sim.antiAir(g,action.actors.map(id=>g.units.get(id)),action.dt);
  else if(action.op==='mission'){const u=g.units.get(action.id),a=u.air;admitted=a.state!=='rearm'&&a.fuel>0&&a.ammo>0;sim.sendPlane(u,structuredClone(action.mission));}
  else if(action.op==='home'){const u=g.units.get(action.id);admitted=sim.airborne(u);if(admitted)u.air.state='home';}
  else if(action.op==='patch')patch(g,g.units.get(action.id),action.value);
  else if(action.op==='remove'){g.units.delete(action.id);sim.rebuildGrid(g);}
  else if(action.op==='visible')g.players[action.owner].visible=new Set(action.ids);
  else if(action.op==='rebuild')sim.rebuildGrid(g);
  else if(action.op==='reregister'){const grid=sim.gridFor(g);grid.drop(action.id,grid.records.get(action.id));grid.update(g.units.get(action.id));}
  else throw new Error(action.op);
  frames.push({action,admitted,expected:snapshot(g)});
 }
 traces.push({name,map,specs,visible,initial,frames});
}
const step=(id,n,dt=.05)=>Array.from({length:n},()=>({op:'step',id,dt}));
const aa=(actors,n,dt=.05)=>Array.from({length:n},()=>({op:'aa',actors,dt}));
for(const kind of ['fighter','attacker','bomber']){
 trace(`${kind}-patrol-orbit`,[plane(kind,0,{air:{state:'base'}})],[{op:'mission',id:1000,mission:patrol},...step(1000,180)]);
 trace(`${kind}-fuel-home-rearm`,[plane(kind,0,{hp:sim.UNITS[kind].hpPer*.6,air:{state:'station',fuel:.1,mission:patrol}})],step(1000,750));
 trace(`${kind}-ammo-home`,[plane(kind,0,{air:{state:'station',ammo:0,mission:patrol}})],step(1000,35));
 trace(`${kind}-wounded-home`,[plane(kind,0,{hp:sim.UNITS[kind].hpPer*.3,air:{state:'station',mission:patrol}})],step(1000,35));
 trace(`${kind}-bail-equality`,[plane(kind,0,{hp:sim.UNITS[kind].hpPer*sim.CFG.air.bail,air:{state:'station',mission:patrol}})],step(1000,4));
 trace(`${kind}-rearm-denial`,[plane(kind,0,{hp:sim.UNITS[kind].hpPer*.5,air:{state:'rearm',timer:.1,ammo:0,mission:null}})],[{op:'mission',id:1000,mission:patrol},...step(1000,4)]);
}
trace('escort-moving-lost-target',[plane('attacker',0,{air:{state:'base'}}),{kind:'rifle',owner:0,x:44,z:45}],[{op:'mission',id:1000,mission:{kind:'escort',id:1001,x:44,z:45}},...step(1000,6),{op:'patch',id:1001,value:{x:47,z:48}},...step(1000,6),{op:'remove',id:1001},...step(1000,6),{op:'home',id:1000},...step(1000,12)]);
trace('held-fighter-attack-air-target-loss',[plane('fighter',0,{air:{state:'base'}}),plane('bomber',1,{x:44,z:45,air:{state:'station',mission:patrol}})],[{op:'mission',id:1000,mission:{kind:'attack',id:1001,x:44,z:45}},...step(1000,6),{op:'patch',id:1001,value:{x:47,z:48}},...step(1000,6),{op:'visible',owner:0,ids:[]},...step(1000,6)]);
trace('nearest-own-finished-airfield',[plane('fighter',0,{air:{state:'base'}}),{kind:'airfield',owner:0,x:16,z:18,built:1},{kind:'airfield',owner:0,x:60,z:60,built:1},{kind:'airfield',owner:1,x:37,z:37,built:1},{kind:'airfield',owner:0,x:38,z:39,built:.9}],step(1000,2));
for(const kind of ['flak','flaktrack','mg','flakpos','fighter']){
 const shooter=kind==='fighter'?plane(kind,0,{holdFire:false,air:{state:'station',ammo:0,mission:patrol}}):flak({kind});
 trace(`${kind}-continuous-aa`,[shooter,plane('bomber',1,{x:46,z:40,hp:250,air:{state:'home',mission:patrol}})],aa([1000],18));
}
trace('aa-killed-later-fighter-must-skip',[plane('fighter',0,{holdFire:false,x:40,z:40}),plane('fighter',1,{holdFire:false,x:42,z:40,hp:1})],aa([1000,1001],3));
trace('aa-nearest-and-range',[flak(),plane('bomber',1,{x:52,z:40,hp:300}),plane('fighter',1,{x:44,z:40,hp:3})],aa([1000],10));
trace('aa-exact-range',[flak(),plane('bomber',1,{x:40+sim.UNITS.flak.aa.range,z:40,hp:300})],aa([1000],2));
trace('aa-setup',[flak({still:1.99}),plane('bomber',1,{x:44,z:40})],[...aa([1000],2),{op:'patch',id:1000,value:{still:2}},...aa([1000],2)]);
trace('aa-held-ground',[flak({holdFire:true}),plane('bomber',1,{x:44,z:40})],aa([1000],2));
trace('aa-unseen',[flak(),plane('bomber',1,{x:44,z:40})],aa([1000],2),false);
trace('aa-unfinished-emplacement',[flak({kind:'flakpos',built:.9}),plane('bomber',1,{x:44,z:40})],[...aa([1000],2),{op:'patch',id:1000,value:{built:1}},...aa([1000],2)]);
trace('aa-parked-fighter',[plane('fighter',0,{holdFire:false,air:{state:'base'}}),plane('bomber',1,{x:44,z:40})],aa([1000],2));
trace('aa-held-fighter-target',[plane('fighter',0,{x:40,z:40,air:{state:'station',mission:{kind:'attack',id:1002,x:48,z:40}}}),plane('fighter',1,{x:42,z:40}),plane('bomber',1,{x:48,z:40,hp:200})],aa([1000],8));
for(const [name,r] of [['empty',reserves(0)],['last-round',reserves(1)],['low',reserves(20)],['forced',reserves(100,{forced:true})],['stranded',reserves(100,{forced:true,stranded:true})],['exhausted',reserves(100,{exhausted:true})]])trace(`aa-reserves-${name}`,[flak({logistics:r,aaReserve:.95}),plane('bomber',1,{x:44,z:40,hp:300})],aa([1000],24));
// Original <= selection follows maintained grid registration order at equal distances.
trace('aa-equal-distance-grid-history',[flak(),plane('bomber',1,{x:42,z:40,hp:300}),plane('bomber',1,{x:38,z:40,hp:300})],[...aa([1000],1),{op:'patch',id:1001,value:{x:66,z:66}},{op:'patch',id:1001,value:{x:42,z:40}},...aa([1000],1),{op:'reregister',id:1001},...aa([1000],1),{op:'rebuild'},...aa([1000],1)]);
const checkpoints=traces.reduce((n,t)=>n+t.frames.length+1,0);
const fixture={reference:pin,node:process.version,arch:process.arch,constructors,traces,checkpoints};
const path=`${root}/rust/crates/simtest/tests/fixtures/match-aviation.json.gz`,compressed=gzipSync(JSON.stringify(fixture)+'\n',{level:9});
if(process.argv.includes('--check'))assert.deepEqual(readFileSync(path),compressed);else writeFileSync(path,compressed);
console.log(`${constructors.length} actual aircraft constructors, ${traces.length} component traces, ${checkpoints} checkpoints`);
