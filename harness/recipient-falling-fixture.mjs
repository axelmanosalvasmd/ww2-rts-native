// Actual legacy scenario damage, debris motion and per-recipient falling delivery.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync} from 'node:fs';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');clean();
assert.ok(process.argv.slice(2).every(a=>a==='--check'));
const url=pathToFileURL(resolve(reference,'shared/sim.js')).href;
registerHooks({load(u,c,next){const r=next(u,c);return u===url?{...r,source:String(r.source)+'\nexport const fallingOracle={stepScenario:g=>stepScenario(g,SCENARIO_TOOLS),stepWorldDebris,damage:SCENARIO_TOOLS.damage};\n'}:r;}});
const sim=await import(url);
const {rebuildGrid}=await import(pathToFileURL(resolve(reference,'shared/grid.js')));
const {createRoomTick}=await import(pathToFileURL(resolve(reference,'server/room-tick.js')));
const copy=x=>JSON.parse(JSON.stringify(x));
const buf=Buffer.alloc(8);
function bits(v){if(typeof v==='number'){buf.writeDoubleLE(v);return buf.toString('hex');}if(Array.isArray(v))return v.map(bits);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,n])=>[k,bits(n)]));return v;}
// Pair only the constructor's original Math.random environment bootstrap with
// Match seed701 Weather streamkey2. Runtime randomness remains Source-owned.
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n;
const mix=n=>{n=((n^(n>>30n))*0xbf58476d1ce4e5b9n)&mask;n=((n^(n>>27n))*0x94d049bb133111ebn)&mask;return n^(n>>31n);};
const bootstrap=Number(mix((mix(701n^(2n*gamma&mask))+gamma)&mask)&0xffffffffn);
assert.equal(bootstrap,2557640466);
function small(){const rows=Array.from({length:60},()=>Array(100).fill('.'));for(let x=40;x<=42;x++)rows[25][x]='=';rows[25][50]='B';rows[25][51]='B';return{name:'Falling recipients',w:100,h:60,rows:rows.map(r=>r.join('')),spawns:[{x:2,y:2},{x:2,y:57},{x:97,y:57}],points:[{x:70,y:30}],triggers:[{at:1,blow:[40,25,40,25]},{at:1,blow:[50,25,50,25]}]};}
const memory=p=>JSON.stringify({terrain:p.terrainMemory?[...p.terrainMemory]:null,pending:p.terrainPending?[...p.terrainPending]:null,key:p.terrainPending?.key,fog:p.fog?{version:p.fog.version,base:[...p.fog.base]}:null,wrecks:p.wreckMemory?[...p.wreckMemory]:null});
function run(name,map,teams,tick){
 const saved=Math.random;let draws=0;Math.random=()=>{draws++;return bootstrap/2**32;};let g;
 try{g=sim.createGame(map,teams.map((_,i)=>`P${i}`),false,teams,teams.map((_,i)=>i),{mode:'conquest',weather:false,tech:false,supply:false,logistics:false});}finally{Math.random=saved;}
 assert.equal(draws,1);
 const initial={units:[...g.units.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,x:u.x,z:u.z,hp:u.hp})),seed:g.seed>>>0,wind:bits(g.wind)};
 const observer=initial.units.find(u=>u.owner===0&&u.type==='rifle').id;
 const actions=[];
 function act(a){
  if(a.tick!==undefined)g.tick=a.tick;
  if(a.move)for(const m of a.move)Object.assign(g.units.get(m.id),{x:m.x,z:m.z});
  if(a.reveal!==undefined)g.reveal=a.reveal;
  if(a.skipFog!==undefined)g.skipFog=a.skipFog;
  rebuildGrid(g);
  const random=Math.random;Math.random=()=>assert.fail('runtime Math.random draw');
  try{if(a.destroy){if(name==='ordinary'){for(const box of [[40,25,40,25],[50,25,50,25]])sim.fallingOracle.damage(g,{kind:'destroy',box},`ordinary:${box[0]}`);}else sim.fallingOracle.stepScenario(g);}for(let i=0;i<(a.steps??0);i++)sim.fallingOracle.stepWorldDebris(g,.05);}finally{Math.random=random;}
  const before=g.players.map(memory),cache=a.warm?sim.snapshotCache(g):undefined;
  if(a.ready){
   const room={game:g,snapEvery:1,players:teams.map((_,slot)=>({ws:{readyState:a.ready[slot]?1:3},stalled:a.stalled?.[slot]??false})),spectators:[],operatives:[]};
   a.outputs=[];let caches=0;
   const tickRoom=createRoomTick({step:()=>{},aiTick:()=>undefined,snapshotCache:game=>{caches++;return sim.snapshotCache(game);},snapshotFor:sim.snapshotFor,trimmed:(_r,_n,msg)=>msg,sendSnapshot:(_r,p,json)=>a.outputs.push({slot:room.players.indexOf(p),falling:bits(JSON.parse(json).falling)}),sendWatchers:()=>assert.fail('unexpected watcher'),connected:p=>p.ws.readyState===1,backedUp:p=>p.stalled,createTickMeter:()=>({}),recordTick:()=>2,tickStats:()=>({})});
   tickRoom(room);assert.equal(caches,a.outputs.length?1:0);
  }else a.outputs=(a.selected??teams.map((_,slot)=>({slot,full:false,omit:false}))).map(({slot,full,omit})=>{
   g.omitFogMask=!!omit;
   const cells=sim.terrainFor(g,slot,!!full),snapshot=sim.snapshotFor(g,slot,g.shots,cells,cache);
   if(a.warm)assert.deepEqual(snapshot.falling,sim.snapshotFor(g,slot,g.shots,cells).falling);
   if(!omit)sim.fogFor(g,slot,!!full);
   return{slot,falling:bits(snapshot.falling)};
  });
  g.players.forEach((p,slot)=>{if(!a.outputs.some(o=>o.slot===slot))assert.equal(memory(p),before[slot]);});
  delete g.omitFogMask;a.seed=g.seed>>>0;a.count=g.fallingSections.length;actions.push(a);
 }
 act({name:'cold-intact',tick:tick-1});
 act({name:'real-legacy-destroy',tick,destroy:true,warm:true});
 if(name!=='great-bridge'){
  act({name:'current-eyes-with-stale-visible-ids',move:[{id:observer,x:83,z:51}],selected:[{slot:1,full:false,omit:false},{slot:0,full:false,omit:false}]});
  act({name:'actual-room-backpressure',ready:[true,true,true],stalled:[false,true,false]});
  act({name:'warm-full-omit-unselected-enemy',warm:true,selected:[{slot:0,full:true,omit:true},{slot:1,full:false,omit:false}]});
  act({name:'live-moving-body',steps:1,warm:true});
  act({name:'eyes-depart-without-vision-refresh',move:[{id:observer,x:5,z:5}],warm:true});
  act({name:'reveal-whole-ordered-list',reveal:true});
  act({name:'source-only-skipFog',reveal:false,skipFog:true,sourceOnly:true});
  act({name:'skipFog-off-omit-does-not-reveal',skipFog:false,selected:[{slot:2,full:true,omit:true}]});
 }
 act({name:'reveal-motion',reveal:true,steps:4,warm:true});
 act({name:'settled-empty',steps:60,warm:true});
 assert.equal(actions.at(-1).count,0);
 assert(actions.some(a=>a.count>0));
 const full=actions.find(a=>a.name==='reveal-motion');assert(full.outputs.every(o=>o.falling.length===full.count));
 assert.notDeepEqual(actions.find(a=>a.name==='reveal-motion').outputs[0].falling,actions.find(a=>a.name==='real-legacy-destroy').outputs[0].falling);
 if(name!=='great-bridge'){
  assert(actions.find(a=>a.name==='current-eyes-with-stale-visible-ids').outputs[0].falling.length>0);
  assert.equal(actions.find(a=>a.name==='skipFog-off-omit-does-not-reveal').outputs[0].falling.length,0);
  assert(actions.find(a=>a.name==='source-only-skipFog').outputs.every(o=>o.falling.length>0));
 }
 return{name,map,teams,tick,initial,actions};
}
const ordinary=small();delete ordinary.triggers;
const traces=[run('small',small(),[0,0,1],20),run('ordinary',ordinary,[0,0,1],20),run('great-bridge',JSON.parse(readFileSync(resolve(reference,'maps/the-great-bridge.json'),'utf8')),[0,1],8400)];
clean();const out={pin,seed:701,bootstrap,scope:'Original complete constructors with every startup actor, actual legacy scenario damage and stepWorldDebris, exact snapshotFor falling rows. These are component phases, not full sim.step. Constructor bootstrap is paired; runtime Math.random throws. Cold/warm selected collectors preserve skipped memory. skipFog is Source-only because Native exposes no matching flag.',traces};
const target=resolve(root,'rust/crates/core/tests/fixtures/recipient-falling.json'),content=JSON.stringify(out)+'\n';
if(process.argv.includes('--check'))assert.equal(readFileSync(target,'utf8'),content);else writeFileSync(target,content);
console.log(`${traces.length} scenes, ${traces.reduce((n,t)=>n+t.actions.length,0)} actions, ${traces.reduce((n,t)=>n+t.actions.reduce((m,a)=>m+a.outputs.length,0),0)} whole recipient lists`);
