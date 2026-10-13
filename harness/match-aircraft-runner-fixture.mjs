// Original complete steps for the sole actual Match runner; observed helpers retain their bodies.
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
registerHooks({load(url,context,next){const r=next(url,context);if(url!==pathToFileURL(resolve(reference,'shared/sim.js')).href)return r;return {...r,source:String(r.source)+`
let aircraftPhaseObserver = null;
const originalAircraftTurn = stepPlane, originalAircraftAA = antiAir;
stepPlane = function(g,u,dt) { aircraftPhaseObserver?.('plane',g,[u.id],dt,'before'); originalAircraftTurn(g,u,dt); aircraftPhaseObserver?.('plane',g,[u.id],dt,'after'); };
antiAir = function(g,list,dt) { aircraftPhaseObserver?.('aa',g,list.map(u=>u.id),dt,'before'); originalAircraftAA(g,list,dt); aircraftPhaseObserver?.('aa',g,list.map(u=>u.id),dt,'after'); };
export function observeAircraftPhases(observer) { aircraftPhaseObserver = observer; }
export {rebuildGrid,updateGrid};
`};}});
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const buf=new DataView(new ArrayBuffer(8)),bits=n=>{buf.setFloat64(0,n);return buf.getBigUint64(0).toString(16).padStart(16,'0');};
const map={name:'Full-step aircraft arena',w:40,h:40,rows:Array(40).fill('.'.repeat(40)),spawns:[{x:3,y:3},{x:36,y:36}],points:[{id:'front',x:20,y:20}]};
const generic=['supp','repath','cd','buff','sprint','react'];
const plane=(kind='fighter',owner=0,extra={})=>({kind,owner,x:38,z:38,holdFire:true,cooldown:.2,retarget:.4,supp:13,repath:7,cd:8,buff:9,sprint:10,react:11,air:{state:'station',mission:{kind:'patrol',id:0,x:40,z:40}},...extra});
const flak=(extra={})=>({kind:'flak',owner:0,x:40,z:40,still:3,holdFire:false,...extra});
function game(specs){const old=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(map,['A','B'],false,[0,1],[0,1],{mode:"conquest",weather:false,logistics:false,tech:false});}finally{Math.random=old;}
 g.units.clear();g.nextId=1000;g.seed=123;g.wind={a:0,v:.5};for(const p of g.players)p.mp=500;for(const spec of specs){const u=sim.internals.spawnUnit(g,spec.owner,spec.kind);const {kind,owner,air,...fields}=spec;Object.assign(u,{auto:false,autoRetreat:false,holdPos:true},structuredClone(fields));if(air)Object.assign(u.air,structuredClone(air));}sim.rebuildGrid(g);for(const p of g.players)p.visible=new Set(g.units.keys());g.shots=[];return g;}
const aircraft=u=>({id:u.id,kind:u.type,owner:u.owner,x:bits(u.x),z:bits(u.z),rot:bits(u.rot),hp:bits(u.hp),cooldown:bits(u.cooldown),retarget:bits(u.retarget),targetId:u.targetId,aim:bits(u.aim),holdFire:!!u.holdFire,xp:bits(u.xp),lastHit:u.lastHit??null,downed:!!u.downed,aaShot:u.aaShot===undefined?null:bits(u.aaShot),aaReserve:u.aaReserve===undefined?null:bits(u.aaReserve),logistics:null,air:{state:u.air.state,fuel:bits(u.air.fuel),ammo:bits(u.air.ammo),timer:bits(u.air.timer),ang:bits(u.air.ang),mission:u.air.mission?{kind:u.air.mission.kind,id:u.air.mission.id??0,x:bits(u.air.mission.x),z:bits(u.air.mission.z)}:null}});
const projection=g=>({planes:[...g.units.values()].filter(u=>u.air).map(aircraft),ground:[...g.units.values()].filter(u=>!u.air&&sim.UNITS[u.type].aa).map(u=>({id:u.id,hp:bits(u.hp),xp:bits(u.xp),lastHit:u.lastHit??null,aaShot:u.aaShot===undefined?null:bits(u.aaShot),aaReserve:u.aaReserve===undefined?null:bits(u.aaReserve)})),story:g.players.map((_,i)=>bits(g.story?.[i]?.planesDowned??0)),events:structuredClone(g.shots.filter(s=>s.k==='aa'||s.k==='planedown'))});
const witness=g=>({tick:g.tick,units:structuredClone([...g.units.values()]),players:g.players.map(p=>({slot:p.slot,mp:p.mp,vp:p.vp,out:p.out,away:p.away,sup:structuredClone(p.sup),visible:[...p.visible]})),story:structuredClone(g.story),shots:structuredClone(g.shots),flights:structuredClone([...g.flights.values()]),nades:structuredClone(g.nades),salvos:structuredClone(g.salvos),strikes:structuredClone(g.strikes),covers:structuredClone(g.covers??[]),points:structuredClone(g.points)});
const optional=n=>n===undefined?null:bits(n);
const runner=g=>({tick:g.tick,nextId:g.nextId,aircraft:projection(g),
 generic:[...g.units.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,x:bits(u.x),z:bits(u.z),rot:bits(u.rot),supp:bits(u.supp),repath:bits(u.repath),cd:bits(u.cd),buff:bits(u.buff),sprint:bits(u.sprint),react:bits(u.react),still:optional(u.still),built:optional(u.built),holdPos:!!u.holdPos,auto:!!u.auto,autoRetreat:!!u.autoRetreat,moveSpeed:bits(u.moveSpeed??0),path:structuredClone(u.path),orders:structuredClone(u.orders),attackId:u.attackId,follow:u.follow??0})),
 players:g.players.map(p=>({mp:bits(p.mp),vp:bits(p.vp),inc:bits(p.inc),sup:Object.fromEntries(Object.entries(p.sup).map(([k,v])=>[k,bits(v)])),visible:[...p.visible].sort((a,b)=>a-b)})),
 points:g.points.map(p=>({owner:p.owner,capper:p.capper,progress:bits(p.progress),cut:!!p.cut,onPoint:p.onPoint??[],contested:!!p.contested,capTick:p.capTick??null})),
 story:g.players.map((_,i)=>['kills','losses','built','captures','mpSpent','supportCalls','planesDowned'].map(k=>bits(g.story?.[i]?.[k]??0))),
 shots:structuredClone(g.shots),timeline:structuredClone(g.timeline??[]),seed:g.seed,wind:structuredClone(g.wind),
 flightCount:g.flights.size,nadeCount:g.nades.length,salvoCount:g.salvos.length,strikeCount:g.strikes.length,coverCount:(g.covers??[]).length,smokeCount:g.smokes.length, wreckCount:g.wrecks.length});
const traces=[];
function trace(name,specs,steps,commands=[]){
 const g=game(specs),baseline=game(specs),initial=projection(g),initialRunner=runner(g),frames=[];
 sim.observeAircraftPhases(null);
 for(let i=0;i<steps;i++){
  g.shots=[];baseline.shots=[];const inputs=commands.filter(c=>c.tick===i).map(c=>({owner:c.owner??0,command:c.command}));
  for(const row of inputs){assert.equal(sim.command(g,row.owner,structuredClone(row.command)),undefined);assert.equal(sim.command(baseline,row.owner,structuredClone(row.command)),undefined);}
  const prior=[...g.units.values()].map(u=>[u.id,bits(u.x),bits(u.z),bits(u.rot)]);
  const phases=[];let active=null;
  sim.observeAircraftPhases((op,g,actors,dt,edge)=>{if(edge==='before'){assert.equal(active,null);active={op,actors,dt,before:projection(g),visible:g.players.map(p=>[...p.visible]),groundInputs:[...g.units.values()].filter(u=>!u.air).map(u=>({id:u.id,still:u.still,built:u.built}))};}else{assert.equal(active.op,op);active.after=projection(g);phases.push(active);active=null;}});
  const old=Math.random;Math.random=()=>{throw new Error(`${name}: unexpected automatic/ground weapon or random phase`);};
  try{sim.step(g);sim.observeAircraftPhases(null);sim.step(baseline);}finally{Math.random=old;sim.observeAircraftPhases(null);}
  const fullStep=witness(g);assert.deepEqual(fullStep,witness(baseline),`${name} observer changed step ${i}`);
  for(const u of g.units.values())if(u.air){const spec=specs[u.id-1000];for(const k of generic)assert.equal(bits(u[k]),bits(spec[k]),`${name} AIR entered ground ${k}`);}
  assert.equal(g.flights.size,0);assert.equal(g.salvos.length,0);assert.equal(g.nades.length,0);
  frames.push({inputs,prior,phases,after:projection(g),expectedRunner:runner(g),fullStep});
 }
 if(name.endsWith('offmap-base-patrol-return-rearm'))assert.deepEqual(new Set(frames.flatMap(f=>f.phases.flatMap(p=>p.after.planes.map(u=>u.air.state)))),new Set(['base','out','station','home','rearm']));
 if(name==='held-fighter-air-attack-and-dead-later-shooter'){
  const first=frames[0].phases.at(-1).after;assert(first.events.some(e=>e.k==='planedown'&&e.t===1001));assert(!first.events.some(e=>e.k==='aa'&&e.f===1001));assert(!frames[0].after.planes.some(u=>u.id===1001));
 }
 if(name==='full-step-equal-distance-current-grid-aa')assert.equal(frames[0].phases.at(-1).after.events.find(e=>e.k==='aa').t,1002);
 traces.push({name,map,specs,visible:true,initial,initialRunner,frames});
}
for(const kind of ['fighter','attacker','bomber']){
 trace(`${kind}-offmap-base-patrol-return-rearm`,[plane(kind,0,{air:{state:'base'}})],850,[{tick:1,command:{t:'move',orders:[[1000,25,25]]}},{tick:70,command:{t:'stop',ids:[1000]}},{tick:100,command:{t:'retreat',ids:[1000]}}]);
 trace(`${kind}-held-station-fuel-home`,[plane(kind,0,{air:{state:'station',fuel:.1,mission:{kind:'patrol',id:0,x:40,z:40}}})],750);
}
trace('held-fighter-air-attack-and-dead-later-shooter',[plane('fighter',0,{x:40,z:40}),plane('fighter',1,{x:42,z:40,hp:1})],12,[{tick:0,command:{t:'attack',ids:[1000],target:1001}},{tick:0,owner:1,command:{t:'attack',ids:[1001],target:1000}}]);
trace('ground-flak-continuous-aa',[flak(),plane('bomber',1,{x:44,z:40,air:{state:'station',mission:{kind:'patrol',id:0,x:44,z:40}}})],100);
trace('full-step-equal-distance-current-grid-aa',[flak(),plane('bomber',1,{x:44,z:40}),plane('bomber',1,{x:44,z:40})],20);
trace('held-escort-ground-friend',[plane('attacker',0,{air:{state:'base'}}),{kind:'rifle',owner:0,x:45,z:45,holdFire:true,auto:false,autoRetreat:false}],40,[{tick:0,command:{t:'escort',ids:[1000],target:1001}}]);
const phaseCalls=traces.reduce((n,t)=>n+t.frames.reduce((a,f)=>a+f.phases.length,0),0),steps=traces.reduce((n,t)=>n+t.frames.length,0);
const fixture={reference:pin,node:process.version,arch:process.arch,scope:"unchanged complete Source step; Native must call actual advance_tick, never replay observed phases or seed their outputs",traces,steps,phaseCalls};
const path=`${root}/rust/crates/simtest/tests/fixtures/match-aircraft-runner.json.gz`,compressed=gzipSync(JSON.stringify(fixture)+'\n',{level:9});
if(process.argv.includes('--check'))assert.deepEqual(readFileSync(path),compressed);else writeFileSync(path,compressed);
console.log(`${traces.length} original full-step traces, ${steps} steps, ${phaseCalls} observed aircraft/AA calls for actual runner consumption; unobserved replay agrees`);
