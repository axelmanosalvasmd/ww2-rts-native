// Record original component calls. This is not a complete game tick comparison.
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
registerHooks({load(url,context,next){const r=next(url,context);if(url!==pathToFileURL(resolve(reference,'shared/sim.js')).href)return r;
 const src=String(r.source),a='  if (g.covers) { for (const c of g.covers)',b='  if (g.winner === null) stepScenario';
 assert.equal(src.split(a).length,2);assert.equal(src.split(b).length,2);
 return {...r,source:src+'\nexport {spawnUnit,placeUnit,stepPlane,updateVision,stepStrikes,antiAir};\nexport function __supportTimers(g,dt){'+src.slice(src.indexOf(a),src.indexOf(b))+'}\n'};
}});
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const grid=await import(pathToFileURL(resolve(reference,'shared/grid.js')));
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n,mix=x=>{x=((x^(x>>30n))*0xbf58476d1ce4e5b9n)&mask;x=((x^(x>>27n))*0x94d049bb133111ebn)&mask;return x^(x>>31n);};
const rng=seed=>{let s=mix(BigInt(seed)^((4n*gamma)&mask)),draws=0;return {next(){draws++;s=(s+gamma)&mask;return Number(mix(s)>>11n)/9007199254740992;},state:()=>s.toString(16).padStart(16,'0'),draws:()=>draws};};
const buf=new DataView(new ArrayBuffer(8)),bits=n=>{buf.setFloat64(0,n);return buf.getBigUint64(0).toString(16).padStart(16,'0');};
const optionalBits=n=>n===undefined||n===null?null:bits(n);
const normalize=v=>JSON.parse(JSON.stringify(v,(_,x)=>x===undefined?null:x));
const floatFields=['x','z','rot','aim','moveSpeed','vx','vz','travelDir','hp','supp','cooldown','still','retarget','repath','stuck','cd','buff','reinf','xp','sprint','hitAt','react','hpWas','woundAt','healAt','aaShot','aaReserve'];
const sourceFields=['path','orders','attackId','targetId','ap','nade','retreating','dig','garrison','enter','board','amove','face','fireAt','riding','cargo','hitBy','hitFrom','drift','reverse','auto','autoRetreat','holdFire','holdPos','follow','lastHit','downed','attackPick','traffic','trafficWait','moveOutcome','moveOutcomeTick','worldGoal','entrench','built','shotAt'];
const represented=new Set(['id','type','owner','logistics','air',...floatFields,...sourceFields]);
const unit=u=>({id:u.id,kind:u.type,owner:u.owner,...Object.fromEntries(floatFields.map(k=>[k,optionalBits(u[k])])),...Object.fromEntries(sourceFields.map(k=>[k,normalize(u[k]??null)])),logistics:u.logistics?normalize(u.logistics):null,air:u.air?{state:u.air.state,fuel:bits(u.air.fuel),ammo:bits(u.air.ammo),timer:bits(u.air.timer),ang:bits(u.air.ang),mission:u.air.mission?{kind:u.air.mission.kind,id:u.air.mission.id??0,x:bits(u.air.mission.x),z:bits(u.air.mission.z)}:null}:null,sourceRest:normalize(Object.fromEntries(Object.entries(u).filter(([k])=>!represented.has(k))))});
const strike=s=>({kind:s.kind,owner:s.owner,x:bits(s.x),z:bits(s.z),dir:bits(s.dir),t:bits(s.t),left:bits(s.left),next:bits(s.next),live:s.live});
const projection=(g,random)=>({tick:g.tick,units:[...g.units.values()].map(unit),players:g.players.map(p=>({slot:p.slot,team:p.team,faction:p.faction,mp:bits(p.mp),vp:bits(p.vp),out:p.out??false,away:p.away??false,cooldowns:Object.fromEntries(Object.entries(p.sup).map(([k,v])=>[k,bits(v)])),visible:[...p.visible],fog:Array.from(sim.teamFog(g,p.team).vis)})),story:g.players.map((_,i)=>Object.fromEntries(['supportCalls','mpSpent','planesDowned'].map(k=>[k,bits(g.story?.[i]?.[k]??0)]))),sourceStory:normalize(g.story),strikes:g.strikes.map(strike),covers:(g.covers??[]).map(c=>({team:c.team,owner:c.owner,x:bits(c.x),z:bits(c.z),r:bits(c.r),t:bits(c.t)})),shots:normalize(g.shots),flights:normalize([...g.flights.values()]),nades:normalize(g.nades),salvos:normalize(g.salvos),smokes:normalize(g.smokes),points:normalize(g.points),environment:[g.seed>>>0,g.nextId],random:random.state(),randomDraws:random.draws()});
const map={name:'Support coherence arena',w:40,h:40,rows:Array(40).fill('.'.repeat(40)),spawns:[{x:2,y:2},{x:37,y:37}],points:[{x:20,y:20,kind:'radio'}]};
const observer={kind:'rifle',owner:0,fields:{x:41,z:41}};
const fighter={kind:'fighter',owner:0,fields:{x:44,z:40}};
const flak={kind:'flak',owner:1,fields:{x:40,z:40,still:3}};
const support=kind=>({t:'support',kind,x:41,z:41,dir:.3});
const patrol=[{owner:0,command:{t:'stance',key:'holdFire',ids:[1001],on:true}},{owner:0,command:{t:'move',orders:[[1001,41,41]]}}];
const cases=[
 {name:'ordinary-para-birth',scene:[observer],steps:122,commands:[{owner:0,command:support('para')}]},
 {name:'para-with-held-on-map-patrol',scene:[observer,fighter],steps:122,commands:[...patrol,{owner:0,command:support('para')}]},
 {name:'recon-arrival-with-on-map-aa',scene:[observer,fighter,flak],steps:66,commands:patrol,timed:[{tick:5,owner:0,command:support('recon')}]},
 {name:'enemy-fighter-cover-denies-para',scene:[observer],steps:122,commands:[{owner:1,command:support('cover')},{owner:0,command:support('para')}]},
];
function make(scene){
 const old=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(map,['A','B'],false,[0,1],[0,1],{weather:false,logistics:false,tech:false});}finally{Math.random=old;}
 g.units.clear();g.nextId=1000;
 for(const spec of scene){const u=sim.spawnUnit(g,spec.owner,spec.kind);sim.placeUnit(g,u,structuredClone(spec.fields));}
 grid.rebuildGrid(g);g.seed=123;g.wind={a:0,v:.5};g.wx={rain:0,wet:0};for(const p of g.players)p.mp=500;
 sim.updateVision(g);g.shots=[];return g;
}
const traces=[];
for(const spec of cases){
 const g=make(spec.scene),random=rng(701),initial=projection(g,random),frames=[];
 const call=(action,fn)=>{const before=projection(g,random),old=Math.random;Math.random=()=>random.next();let denial;try{denial=fn()??null;}finally{Math.random=old;}frames.push({action,denial,before,after:projection(g,random)});};
 const command=row=>call({op:'command',tick:g.tick,owner:row.owner,command:row.command},()=>sim.command(g,row.owner,structuredClone(row.command)));
 for(const row of spec.commands)command(row);
 for(let tick=1;tick<=spec.steps;tick++){
  g.tick=tick;g.shots=[];
  for(const row of spec.timed??[])if(row.tick===tick)command(row);
  // The source refreshes vision before its live-unit turns, on this cadence.
  if(tick%4===1)call({op:'vision',tick},()=>sim.updateVision(g));
  const list=[...g.units.values()];
  for(const u of list)if(u.hp>0&&u.air)call({op:'plane',tick,id:u.id,actors:[u.id]},()=>sim.stepPlane(g,u,.05));
  call({op:'strikes',tick,actors:list.map(u=>u.id)},()=>sim.stepStrikes(g,list,.05));
  call({op:'aa',tick,actors:list.map(u=>u.id)},()=>sim.antiAir(g,list,.05));
  call({op:'timers',tick},()=>sim.__supportTimers(g,.05));
 }
 assert(frames.filter(f=>f.action.op==='command').every(f=>f.denial===null),JSON.stringify({name:spec.name,commands:frames.filter(f=>f.action.op==='command').map(f=>[f.action.command,f.denial])}));
 if(spec.name.includes('birth')||spec.name==='para-with-held-on-map-patrol'){
  const birth=frames.find(f=>f.action.op==='strikes'&&f.after.units.length>f.before.units.length);assert(birth);
  const newborn=birth.after.units.filter(u=>!birth.before.units.some(v=>v.id===u.id));assert.deepEqual(newborn.map(u=>u.kind),['rifle','rifle','mg']);
  for(const u of newborn){for(const k of ['cd','buff','reinf'])assert.equal(u[k],bits(0));assert.equal(u.ap,false);for(const k of ['hpWas','woundAt','healAt'])assert.equal(u[k],null);}
 }
 if(spec.scene.some(s=>s.kind==='fighter')){assert(frames.some(f=>f.action.op==='plane'&&f.after.units.find(u=>u.air)?.air.state==='station'));assert(frames.filter(f=>f.action.op==='plane').every(f=>f.after.units.find(u=>u.air)?.holdFire===true));}
 if(spec.name==='recon-arrival-with-on-map-aa'){
  const arrival=frames.find(f=>f.action.op==='strikes'&&f.after.shots.some(s=>s.k==='recon'));assert(arrival);
  const aa=frames.find(f=>f.action.op==='aa'&&f.action.tick===arrival.action.tick);assert(aa.after.shots.some(s=>s.k==='aa'&&s.f===1002&&s.t===1001));
  assert.notEqual(aa.before.units.find(u=>u.id===1001).hp,aa.after.units.find(u=>u.id===1001).hp);
  assert.equal(arrival.after.story[0].supportCalls,bits(1));assert.equal(aa.after.units.find(u=>u.id===1002).owner,1);
 }
 if(spec.name==='enemy-fighter-cover-denies-para'){const denied=frames.find(f=>f.action.op==='strikes'&&f.after.shots.some(s=>s.k==='shotdown'));assert(denied.after.shots.some(s=>s.k==='shotdown'&&s.by==='fighters'));assert.equal(denied.after.units.length,1);assert.equal(denied.after.story[1].planesDowned,bits(1));}
 assert(frames.every(f=>f.after.flights.length===0&&f.after.nades.length===0&&f.after.salvos.length===0));
 traces.push({...spec,map,initial,frames});
}
const frames=traces.reduce((n,t)=>n+t.frames.length,0),phaseCalls=traces.reduce((n,t)=>n+t.frames.filter(f=>['plane','strikes','aa','timers'].includes(f.action.op)).length,0);
const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,seed:701,environmentSeed:123,dt:.05,traces,frames,phaseCalls})+'\n';
const path=`${root}/rust/crates/simtest/tests/fixtures/support-coherence.json.gz`,compressed=gzipSync(output,{level:9});
if(process.argv.includes('--check'))assert.deepEqual(readFileSync(path),compressed);else writeFileSync(path,compressed);
console.log(`${traces.length} bounded support coherence cases, ${frames} frames, ${phaseCalls} owned calls, ${compressed.length} gzip bytes (${Buffer.byteLength(output)} JSON bytes)`);
