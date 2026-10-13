// Actual pinned sim.step calls, including operative entry, input and full tick effects.
// The Combat seed convention matches the existing firing/ability source recorders.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {createHash} from 'node:crypto';
import {gzipSync} from 'node:zlib';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};
clean();const source=readFileSync(`${reference}/shared/sim.js`,'utf8');
const simURL=pathToFileURL(`${reference}/shared/sim.js`).href;
for(const anchor of ['export function step(g) {','function stepOperative(g, u) {','function spawnOperative(g, op, from = null) {'])assert.equal(source.split(anchor).length,2);
registerHooks({load(url,context,next){const result=next(url,context);return url===simURL?{...result,source:`${result.source}\nexport { placeUnit };\n`}:result;}});
const sim=await import(simURL);
const {gridFor,rebuildGrid}=await import(pathToFileURL(`${reference}/shared/grid.js`));
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n;
const mix=x=>{x=((x^(x>>30n))*0xbf58476d1ce4e5b9n)&mask;x=((x^(x>>27n))*0x94d049bb133111ebn)&mask;return x^(x>>31n);};
function combatRandom(seed){let state=mix(BigInt(seed)^((3n*gamma)&mask)),count=0;return {next(){state=(state+gamma)&mask;count++;return Number(mix(state)>>11n)/9007199254740992;},snapshot(){return {seed,count,state:state.toString(16).padStart(16,'0')};}};}
const copy=v=>v instanceof Map?[...v].map(([k,v])=>[copy(k),copy(v)]):v instanceof Set?[...v].map(copy):ArrayBuffer.isView(v)?[...v]:Array.isArray(v)?v.map(copy):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).filter(([,v])=>v!==undefined&&typeof v!=='function').map(([k,v])=>[k,copy(v)])):v;
const clone=v=>JSON.parse(JSON.stringify(v));
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
function numericBits(value){const out={};function visit(v,path){if(typeof v==='number'){assert.ok(Number.isFinite(v),`nonfinite source field ${path}`);out[path]=bits(v);}else if(v&&typeof v==='object')for(const[k,item]of Object.entries(v))visit(item,path?`${path}.${k}`:k);}visit(value,'');return out;}
const exact=value=>{value=copy(value);return {raw:clone(value),floatBits:numericBits(value)};};
const runs=items=>{const out=[];for(const v of items){if(out.length&&Object.is(out.at(-1)[0],v))out.at(-1)[1]++;else out.push([v,1]);}return out;};
const map={name:'Full operative tick fixture',w:64,h:64,rows:Array(64).fill('.'.repeat(64)),spawns:[{x:10,y:32},{x:54,y:32}],points:[{id:'inert-point',x:32,y:32}]};
const options={supply:false,logistics:false,weather:false,tech:false};
const combatSeed=701,traces=[],snapshots=new WeakMap();
function scene(name){
  const setupDraws=[];const previous=Math.random;Math.random=()=>{setupDraws.push(.23456789);return .23456789;};let g;
  try{g=sim.createGame(map,['Commander','Enemy'],false,[0,1],[0,1],options);}finally{Math.random=previous;}
  g.units.clear();g.tick=0;g.shots=[];g.flights.clear();g.nades=[];g.salvos=[];g.strikes=[];g.smokes=[];
  const random=combatRandom(combatSeed),events=[],known=new WeakMap();let active=false;
  const record=e=>{if(active)events.push(copy(e));};
  // Preserve object identity: original spawnUnit publishes and indexes the same
  // object it returns. A replacement Proxy would invalidate grid identity checks.
  function observed(object,actor,extra=[]){if(known.has(object))return object;known.set(object,object);
    for(const key of new Set([...Object.keys(object),...extra])){let value=object[key];Object.defineProperty(object,key,{enumerable:true,configurable:true,get:()=>value,set:v=>{record({actor,op:'set',field:key,value:v});value=key==='mags'&&v?observed(v,`${actor}.mags`):v;}});}return object;
  }
  const trackUnit=u=>{if(u.mags)u.mags=observed(u.mags,`unit:${u.id}.mags`);return observed(u,`unit:${u.id}`,['operative','vy','jy','crouch','weapon','mags','reloadT','nades','nadeT','shotAt','lastHit','follow','hpWas','woundAt','moveOutcome','moveOutcomeTick','moveResult','worldGoal']);};
  const trackCompanion=op=>observed(op,`companion:${op.key}`);
  function scalar(key){let value=g[key];Object.defineProperty(g,key,{enumerable:true,configurable:true,get:()=>value,set:v=>{record({actor:'game',op:'set',field:key,value:v});value=v;}});}
  for(const key of ['tick','nextId','nextFlight','seed','visionTick','winner'])scalar(key);
  g.wind=observed(g.wind,'wind');
  g.players=g.players.map((p,i)=>observed(p,`player:${i}`));
  const insert=g.units.set.bind(g.units),remove=g.units.delete.bind(g.units);
  g.units.set=(id,u)=>{const tracked=trackUnit(u);record({actor:'units',op:'insert',id,value:tracked});return insert(id,tracked);};
  g.units.delete=id=>{record({actor:'units',op:'delete',id});return remove(id);};
  g.operatives=new Map();const put=g.operatives.set.bind(g.operatives);
  g.operatives.set=(key,op)=>{const tracked=trackCompanion(op);record({actor:'operatives',op:'insert',key,value:tracked});return put(key,tracked);};
  const flightRow=(p,world=g)=>{const {source,suppressed,...fields}=p;return {...fields,sourceId:source.id,sourceMatchesShooter:source===world.units.get(p.shooter),suppressed:[...suppressed]};};
  const putFlight=g.flights.set.bind(g.flights),deleteFlight=g.flights.delete.bind(g.flights);
  g.flights.set=(id,p)=>{const tracked=observed(p,`flight:${id}`,['sent']);record({actor:'flights',op:'insert',id,value:flightRow(tracked)});return putFlight(id,tracked);};
  g.flights.delete=id=>{record({actor:'flights',op:'delete',id});return deleteFlight(id);};
  function arrayField(key,transform=v=>v){let array=g[key]??[];
    const wrap=a=>{a.push=function(...values){for(const value of values)record({actor:key,op:'append',value:transform(value)});return Array.prototype.push.apply(this,values);};return a;};
    array=wrap(array);Object.defineProperty(g,key,{enumerable:true,configurable:true,get:()=>array,set:a=>{record({actor:'game',op:'set',field:key,value:a});array=wrap(a);}});
  }
  arrayField('shots');arrayField('nades');
  let story=g.story;Object.defineProperty(g,'story',{enumerable:true,configurable:true,get:()=>story,set:s=>{record({actor:'game',op:'set',field:'story',value:s});story=s.map((row,i)=>observed(row,`story:${i}`));}});
  const data={name,combatSeed,setupRng:exact(setupDraws),initial:null,operations:[]};
  function snapshot(game=g,stream=random){const grid=gridFor(game);const probes=[];
    for(const [x,z]of [[16,16],[32,32],[80,80]])for(const radius of [0,15,80])probes.push({x,z,radius,inOrder:grid.inOrder({x,z},radius).map(u=>u.id),unordered:grid.unordered({x,z},radius).map(u=>u.id),heavy:grid.heavy({x,z},radius).map(u=>u.id)});
    return exact({game:{w:game.w,h:game.h,tick:game.tick,nextId:game.nextId,nextFlight:game.nextFlight,winner:game.winner,mode:game.mode??null,seed:game.seed,wind:game.wind,visionTick:game.visionTick??null,weather:game.weather,wx:game.wx,winVp:game.winVp,tech:game.tech,supply:game.supply,logisticsEnabled:game.logisticsEnabled},
      players:game.players.map(p=>({...p,visible:[...p.visible]})),points:game.points,units:[...game.units.values()],unitOrder:[...game.units.keys()],operatives:[...game.operatives.values()],
      shots:game.shots,nades:game.nades,flights:[...game.flights.values()].map(p=>flightRow(p,game)),flightOrder:[...game.flights.keys()],salvos:game.salvos,strikes:game.strikes,smokes:game.smokes,story:game.story??null,timeline:game.timeline??null,fallen:game.fallen??null,
      terrain:{flags:runs(game.flags),height:game.height&&runs(game.height),chars:runs(game.chars),ground:runs(game.ground),objects:runs(game.objects),cellHp:runs(game.cellHp),wear:runs(game.wear),scar:runs(game.scar),burnt:runs(game.burnt),cellState:runs(game.cellState),versions:[game.terrainVersion??0,game.obstructionVersion??0,game.infantryRegionVersion??0,game.vehicleRegionVersion??0]},
      environment:{wrecks:game.wrecks,fires:[...game.fires],soak:[...game.soak],newCells:game.newCells,cellLog:game.cellLog,claims:[...(game.claims??[])],aid:[...game.aid],mines:[...game.mines]},
      grid:{probes,ownedBy:game.players.map(p=>grid.ownedBy(p.slot).map(u=>u.id))},combatRandom:stream.snapshot()});
  }
  function start(){if(!data.initial){rebuildGrid(g);data.initial=snapshot();traces.push(data);}}
  function action(args,expectedDraws,execute){start();events.length=0;const draws=[],sourceSites=[];const saved=Math.random;
    Math.random=()=>{const stack=new Error().stack;const line=stack.split('\n').find(s=>s.includes(`${simURL}:`));assert.ok(line&&line.includes('fireOperative'),`${name}: unexpected source random call ${line}`);assert.ok(draws.length<expectedDraws,`${name}: excess Combat draw`);const value=random.next();draws.push(value);const site=line.match(/sim\.js:(\d+):(\d+)/);assert.ok(site);sourceSites.push(`shared/sim.js:${site[1]}:${site[2]}`);record({actor:'rng',op:'draw',index:draws.length-1,value,sourceSite:sourceSites.at(-1)});return value;};
    active=true;let result;try{result=execute();assert.equal(draws.length,expectedDraws,`${name}: Combat draw count`);}finally{active=false;Math.random=saved;}
    data.operations.push({...clone(args),argumentFloatBits:numericBits(args),rng:{...exact(draws),sourceSites},events:exact(events),...(result===undefined?{}:{result:exact(result)}),expected:snapshot()});
  }
  const t={g,data,get op(){return g.operatives.get('gunner');},get unit(){return g.units.get(t.op.unitId);},
    ordinary(type='rifle',owner=0,fields={}){assert.equal(data.initial,null);const u=sim.internals.spawnUnit(g,owner,type);sim.placeUnit(g,g.units.get(u.id),{x:82,z:80,holdPos:true,holdFire:true,auto:false,autoRetreat:false,...fields});return g.units.get(u.id);},
    join(key='gunner',owner=0){action({op:'join',key,owner},0,()=>{const op=sim.joinOperative(g,key,owner);return {key:op.key,unitId:op.unitId};});return t;},
    input(fields={}){const frame={seq:(t.op.lastSeq??-1)+1,forward:0,strafe:0,yaw:0,pitch:0,fire:false,ads:false,crouch:false,sprint:false,weapon:0,jump:0,reload:0,nade:0,squad:0,...fields};action({op:'input',key:'gunner',input:frame},0,()=>sim.operativeInput(g,'gunner',frame));return t;},
    patch(fields,id=t.op.unitId){action({op:'patch_unit',id,fields},0,()=>sim.placeUnit(g,g.units.get(id),fields));return t;},
    player(owner,fields){action({op:'patch_player',owner,fields},0,()=>{Object.assign(g.players[owner],fields);});return t;},
    game(fields){action({op:'patch_game',fields},0,()=>{Object.assign(g,fields);});return t;},
    step(draws=0){action({op:'step'},draws,()=>sim.step(g));return t;},
  };snapshots.set(data,snapshot);return t;
}
function joinAt(t){return t.join().patch({x:80,z:80});}
{
  const t=joinAt(scene('fresh-five-ticks-then-stale-want-clearing'));t.input({forward:1}).input({forward:1,jump:1});
  for(let i=0;i<7;i++){t.step();assert.deepEqual(copy(t.op.want),{});}assert.equal(t.g.tick-t.op.lastInput,7);
}
{
  const t=joinAt(scene('moving-fire-and-single-cooldown-decrement'));t.input({forward:1,fire:true}).step(2);assert.equal(t.op.shots,1);assert.equal(t.unit.mags[0],7);const cd=t.unit.cooldown;t.step();assert.equal(bits(t.unit.cooldown),bits(Math.max(0,cd-sim.TICK)));assert.equal(t.op.shots,1);
}
{
  const t=joinAt(scene('fire-kill-retires-target-and-starts-delay-in-same-full-tick'));t.join('enemy',1);
  const enemy=t.g.operatives.get('enemy'),id=enemy.unitId;t.patch({x:92,z:80,hp:3},id).input({fire:true,ads:true}).step(2);
  assert.deepEqual(t.g.shots.map(s=>s.k),['hurt','sniper']);assert.equal(t.op.shots,1);assert.equal(t.op.hits,1);assert.equal(t.op.kills,1);
  assert.equal(enemy.unitId,0);assert.equal(enemy.respawnAt,t.g.tick+160);assert.ok(!t.g.units.has(id));assert.equal(t.g.story[0].kills,1);assert.equal(t.g.story[1].losses,1);assert.equal(t.unit.xp,0);assert.equal(t.g.story[0].mpSpent,0);
}
{
  const t=joinAt(scene('operative-skips-ordinary-timers-and-stale-reload'));t.patch({cooldown:.32,nadeT:.12,reloadT:.7,supp:8,retarget:.9,repath:.8,cd:.7,buff:.6,sprint:.5,react:.4});t.step();assert.equal(t.unit.reloadT,.7);assert.equal(t.unit.retarget,.9);assert.equal(t.unit.cd,.7);assert.equal(t.unit.buff,.6);assert.equal(t.unit.sprint,.5);assert.equal(t.unit.react,.4);
}
{
  const t=joinAt(scene('weapon-swap-cancels-reload-and-blocks-fire'));t.patch({reloadT:.7}).input({fire:true,weapon:1}).step();assert.equal(t.unit.weapon,1);assert.equal(t.unit.reloadT,0);assert.equal(t.unit.cooldown,sim.OPERATIVE.swap);assert.equal(t.op.shots,0);
}
{
  const t=joinAt(scene('empty-trigger-starts-reload-before-fire'));t.patch({mags:[0,32]}).input({fire:true}).step();assert.equal(t.unit.reloadT,sim.OPERATIVE_WEAPONS[0].reload);t.step();assert.equal(bits(t.unit.reloadT),bits(sim.OPERATIVE_WEAPONS[0].reload-sim.TICK));assert.equal(t.op.shots,0);
}
{
  const t=joinAt(scene('grenade-press-beats-fire-on-full-launch-tick'));t.input({fire:true}).input({fire:true,nade:1}).step();assert.deepEqual(t.g.shots.map(s=>s.k),['flight','throw']);assert.equal(t.op.shots,0);const p=t.g.flights.get(1);assert.equal(p.last,p.launched);assert.equal(bits(t.g.nades[0].t),bits(p.duration-sim.TICK));assert.equal(p.sequence,0);
}
for(const [label,fields]of [['empty',{nades:0}],['timer',{nadeT:.5}]]){
  const t=joinAt(scene(`blocked-grenade-${label}-falls-through-to-fire`));t.patch(fields).input({fire:true}).input({fire:true,nade:1}).step(2);assert.equal(t.op.shots,1);assert.equal(t.g.flights.size,0);
}
{
  const t=joinAt(scene('sprint-consumes-grenade-want-and-blocks-both-effects'));t.input({forward:1,sprint:true,fire:true}).input({forward:1,sprint:true,fire:true,nade:1}).step();assert.equal(t.g.shots.length,0);assert.deepEqual(copy(t.op.want),{});t.input({sprint:true,fire:true,nade:1}).step(2);assert.equal(t.op.shots,1);assert.equal(t.g.flights.size,0);
}
for(const [label,fields]of [['winner',{winner:0}],['out',null]]){
  const t=joinAt(scene(`${label}-clears-want-and-blocks-effects`));t.input({fire:true}).input({fire:true,jump:1,nade:1});if(fields)t.game(fields);else t.player(0,{out:true});t.step();assert.equal(t.g.shots.length,0);assert.deepEqual(copy(t.op.want),{});
}
for(const funded of ['wallet','waiting-wallet','squad']){
  const t=scene(`death-delay-and-${funded}-respawn`);const squad=funded==='squad'?t.ordinary():null;
  joinAt(t);t.player(0,{away:true,mp:funded==='wallet'?10:funded==='waiting-wallet'?5:0});
  if(squad){t.input().input({squad:1}).step();assert.equal(t.op.squadId,squad.id);assert.equal(squad.follow,'gunner');}
  const dead=t.op.unitId;t.patch({hp:0}).step();const died=t.g.tick,due=died+160;assert.equal(t.op.unitId,0);assert.equal(t.op.input,null);assert.equal(t.op.respawnAt,due);assert.ok(!t.g.units.has(dead));
  t.input({weapon:1});while(t.g.tick<due-1)t.step();assert.equal(t.op.unitId,0);t.step();
  if(funded==='waiting-wallet'){assert.equal(t.op.unitId,0);t.player(0,{mp:10}).step();}
  assert.ok(t.op.unitId>dead);assert.equal(t.unit.weapon,1);assert.equal(t.op.input,null);assert.equal(t.g.players[0].mp,0);assert.equal(t.g.story[0].mpSpent,0);
  if(squad){assert.equal(bits(squad.hp),bits(sim.UNITS.rifle.models*sim.UNITS.rifle.hpPer-sim.UNITS.rifle.hpPer));assert.equal(squad.follow,'gunner');}
}
// Repeat every operation without observation descriptors. Complete snapshots
// must agree, so recording cannot alter source object identity or tick behavior.
for(const trace of traces){
  const saved=Math.random;Math.random=()=>.23456789;let bare;
  try{bare=sim.createGame(map,['Commander','Enemy'],false,[0,1],[0,1],options);}finally{Math.random=saved;}
  bare.units=new Map(trace.initial.raw.units.map(u=>[u.id,copy(u)]));bare.operatives=new Map();
  Object.assign(bare,{tick:trace.initial.raw.game.tick,nextId:trace.initial.raw.game.nextId,nextFlight:trace.initial.raw.game.nextFlight});
  rebuildGrid(bare);const stream=combatRandom(combatSeed),snapshot=snapshots.get(trace);
  assert.deepEqual(snapshot(bare,stream),trace.initial,`${trace.name}: observer changed initial source state`);
  for(const op of trace.operations){const prior=Math.random,draws=[];Math.random=()=>{const value=stream.next();draws.push(value);return value;};
    try{switch(op.op){
      case 'join':sim.joinOperative(bare,op.key,op.owner);break;
      case 'input':assert.equal(sim.operativeInput(bare,op.key,op.input),op.result.raw);break;
      case 'patch_unit':sim.placeUnit(bare,bare.units.get(op.id),op.fields);break;
      case 'patch_player':Object.assign(bare.players[op.owner],op.fields);break;
      case 'patch_game':Object.assign(bare,op.fields);break;
      case 'step':sim.step(bare);break;
      default:assert.fail(`Unexpected replay operation ${op.op}`);
    }}finally{Math.random=prior;}
    assert.deepEqual(exact(draws),{raw:op.rng.raw,floatBits:op.rng.floatBits},`${trace.name}: unobserved source draw order`);
    assert.deepEqual(snapshot(bare,stream),op.expected,`${trace.name}/${op.op}: observation altered original full step`);
  }
}
function changes(a,b,path=[],out=[]){if(Object.is(a,b))return out;if(Array.isArray(a)&&Array.isArray(b)&&a.length===b.length){for(let i=0;i<a.length;i++)changes(a[i],b[i],[...path,i],out);return out;}if(a&&b&&typeof a==='object'&&typeof b==='object'&&!Array.isArray(a)&&!Array.isArray(b)){for(const key of new Set([...Object.keys(a),...Object.keys(b)])){if(!Object.hasOwn(b,key))out.push([[...path,key]]);else changes(a[key],b[key],[...path,key],out);}return out;}out.push([path,b]);return out;}
function applyChanges(value,delta){for(const change of delta){const path=change[0];if(!path.length){value=clone(change[1]);continue;}let at=value;for(const key of path.slice(0,-1))at=at[key];const key=path.at(-1);if(change.length===1)delete at[key];else at[key]=clone(change[1]);}return value;}
for(const t of traces){let previous=t.initial;for(const op of t.operations){const expected=op.expected;op.delta=changes(previous,expected);assert.deepEqual(applyChanges(clone(previous),op.delta),expected);delete op.expected;previous=expected;}}
clean();const output=JSON.stringify({schema:1,reference:pin,node:process.version,arch:process.arch,sourceSha256:createHash('sha256').update(source).digest('hex'),map,options,combatSeed,
  floatEncoding:'IEEE754 binary64 big-endian hex at numeric leaf paths separated by dots',
  projection:'Complete original unit/player/companion rows, ordered registries and events. Terrain arrays use runs. Map/Set values use ordered arrays. Flight source pointers use sourceId/sourceMatchesShooter. Observed source property writes, appends, inserts/deletions and random call sites retain their order without replacing source bodies.',
  observationVerification:'Every operation and complete numeric-bit snapshot is independently replayed with ordinary unobserved source objects; observer descriptors must not change original behavior or object identity.',
  randomConvention:'Source Math.random uses the native seed 701 Combat SplitMix64 fractions, as in match-firing/ability-flights recorders. Native runtime must start MatchRandom normally; no native draw injection. Source wind/environment seed remains separate.',
  sourcePhases:['clock then due respawns before debris','vision before operative input freshness and weapon priority','ordinary-unit timer/reinforcement skips for operatives','projectiles and linked grenade ticket decrement on launch tick','death bookkeeping before point capture and income'],
  unsupported:['grenade contact, bounce, blast and settlement beyond its first launch tick','AI, tech/research, logistics/supply, production/construction, support, scenarios, air/rail, moving debris, fire/flood and reinforcement are inert in these authored scenes','recipient, network and C representations','hit-direction policy adoption','native full-match parity claim'],traces})+'\n';
const destination=`${root}/rust/crates/simtest/tests/fixtures/fps-tick.json.gz`,compressed=gzipSync(output,{level:9});
if(process.argv.includes('--check'))assert.deepEqual(readFileSync(destination),compressed);else writeFileSync(destination,compressed);
const counts={};for(const t of traces)for(const op of t.operations)counts[op.op]=(counts[op.op]??0)+1;
console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} ${traces.length} actual-step traces: ${JSON.stringify(counts)}, ${Buffer.byteLength(output)} JSON bytes, ${compressed.length} gzip bytes`);
