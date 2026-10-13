// Original FPS fire and grenade launch bodies, with observed mutations and exact
// numeric bits. Full ticks, grenade contacts/blasts and recipient output are excluded.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
assert.equal(process.arch,'x64'); assert.equal(process.version,'v24.21.0');
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim();
assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');
const source=readFileSync(`${reference}/shared/sim.js`,'utf8');
const url=pathToFileURL(`${reference}/shared/sim.js`).href;
for(const marker of ['function fireOperative(g, u, op, input, moving) {','function throwOperative(g, u, input) {','function addFlight(g, u, at, profile, options = {}) {','function hurt(g, t, src, fall, owner) {'])assert.equal(source.split(marker).length,2);
registerHooks({load(path,context,next){const result=next(path,context);return path===url?{...result,source:`${result.source}\nexport { fireOperative, throwOperative, addFlight, placeUnit, heightAt, hurt, onDeck };\n`}:result;}});
const sim=await import(url);
const men=await import(pathToFileURL(`${reference}/shared/squad-men.js`));
const projectile=await import(pathToFileURL(`${reference}/shared/projectiles.js`));
const {rebuildGrid,gridFor}=await import(pathToFileURL(`${reference}/shared/grid.js`));
Math.random=()=>{throw Error('Unseeded reference random call');};
const clone=v=>JSON.parse(JSON.stringify(v));
const copy=v=>Array.isArray(v)?v.map(copy):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,copy(v)])):v;
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
function numericBits(value){const result={};function visit(v,path){if(typeof v==='number'){assert.ok(Number.isFinite(v),`Nonfinite source field ${path}`);result[path]=bits(v);}else if(v&&typeof v==='object')for(const[k,item]of Object.entries(v))visit(item,path?`${path}.${k}`:k);}visit(value,'');return result;}
const exact=value=>({raw:clone(value),floatBits:numericBits(value)});
const runs=items=>{const rows=[];for(const value of items){if(rows.length&&rows.at(-1)[0]===value)rows.at(-1)[1]++;else rows.push([value,1]);}return rows;};
const map={name:'FPS effects fixture',w:64,h:64,rows:Array(64).fill('.'.repeat(64)),spawns:[{x:10,y:32},{x:54,y:32},{x:10,y:54}],points:[{x:32,y:32}]};
const unitKeys=['id','type','owner','x','z','rot','aim','moveSpeed','travelDir','vx','vz','vy','jy','crouch','operative','hp','xp','supp','cooldown','shotAt','weapon','mags','reloadT','nades','nadeT','lastHit','hitBy','hitAt','hitFrom','retarget','targetId','attackId','garrison','riding','air','cargo','retreating','sprint','still','auto','autoRetreat','holdFire','holdPos','path','orders','amove','worldGoal','built','cells','structureId','deathImpulse'];
const row=u=>Object.fromEntries(unitKeys.filter(k=>u[k]!==undefined).map(k=>[k,u[k]]));
const traces=[];
function scene(name,category='fire'){
  let setupDraws=0;const previous=Math.random;Math.random=()=>{setupDraws++;return 0.23456789;};let g;
  try{g=sim.createGame(map,['Commander','Enemy','Ally'],false,[0,1,0],[0,1,2],{supply:false,logistics:false,weather:false,tech:false});}finally{Math.random=previous;}
  const op=sim.joinOperative(g,'gunner',0),raw=g.units.get(op.unitId);
  for(const u of [...g.units.values()])if(u!==raw)g.units.delete(u.id);
  g.flags.fill(sim.TERRAIN['.']);g.chars.fill('.');g.ground.fill('.');g.objects.fill('');g.height=null;
  g.tick=37;g.shots=[];g.nades=[];g.flights.clear();g.nextFlight=1;
  const events=[];let active=false;
  function observed(value,actor){return new Proxy(value,{set(object,key,value){if(active)events.push({actor,op:'set',field:String(key),value:copy(value)});object[key]=value;return true;}});}
  const track=u=>{if(u.mags)u.mags=observed(u.mags,`unit:${u.id}.mags`);return observed(u,`unit:${u.id}`);};
  const shooter=track(raw);g.units.set(shooter.id,shooter);
  const companion=observed(op,'companion:gunner');g.operatives.set(op.key,companion);
  sim.placeUnit(g,shooter,{x:20,z:40,rot:0,aim:0,vx:0,vz:0,jy:0,vy:0});
  let nextFlight=g.nextFlight;Object.defineProperty(g,'nextFlight',{enumerable:true,configurable:true,get:()=>nextFlight,set:value=>{if(active)events.push({actor:'game',op:'set',field:'nextFlight',value});nextFlight=value;}});
  const flightRow=p=>{const {source,suppressed,...fields}=p;return {...fields,sourceId:source.id,sourceMatchesShooter:source===g.units.get(p.shooter),suppressed:[...suppressed]};};
  const emit=(array,actor,convert=v=>v)=>{array.push=function(...values){if(active)for(const value of values)events.push({actor,op:'append',value:copy(convert(value))});return Array.prototype.push.apply(this,values);};};
  emit(g.shots,'shots');emit(g.nades,'nades');
  const insert=g.flights.set.bind(g.flights);g.flights.set=(id,p)=>{if(active)events.push({actor:'flights',op:'insert',id,value:copy(flightRow(p))});return insert(id,p);};
  function snapshot(full=false){
    const units=[...g.units.values()].map(u=>full?u:row(u));
    const raw={game:{w:g.w,h:g.h,tick:g.tick,winner:g.winner,nextId:g.nextId,nextFlight:g.nextFlight,seed:g.seed,mode:g.mode??null,tech:g.tech,logisticsEnabled:g.logisticsEnabled},
      terrain:{flags:runs(g.flags),height:g.height&&runs(g.height),smokes:g.smokes},
      players:g.players.map(p=>({slot:p.slot,team:p.team,faction:p.faction,spawn:p.spawn,mp:p.mp,visible:[...p.visible]})),
      unitOrder:[...g.units.keys()],units,operatives:[...g.operatives.values()],shots:g.shots,nades:g.nades,flightOrder:[...g.flights.keys()],flights:[...g.flights.values()].map(flightRow),
      men:[...g.units.values()].filter(u=>sim.UNITS[u.type].infantry&&!u.operative&&men.SLOTS[u.type]).map(u=>{const drawn=men.menDrawn(u.type,u.hp,sim.UNITS[u.type].models*sim.UNITS[u.type].hpPer);return{id:u.id,drawn,positions:men.menAt(u,drawn)};}),
      grid:{inOrder:gridFor(g).inOrder({x:40,z:40},128,()=>true).map(u=>u.id),unordered:gridFor(g).unordered({x:40,z:40},128).map(u=>u.id),heavy:gridFor(g).heavy({x:40,z:40},128).map(u=>u.id)}};
    return exact(raw);
  }
  const data={name,category,setupDraws,initial:null,operations:[]};
  function start(){if(data.initial===null){g.players[0].visible=new Set([...g.units.keys()]);rebuildGrid(g);data.initial=snapshot(true);traces.push(data);}}
  function action(input,tape,execute){
    assert.ok(tape.every(n=>Number.isFinite(n)&&n>=0&&n<1),'Random fractions must be in [0,1)');start();events.length=0;let used=0;const draws=[];const previous=Math.random;Math.random=()=>{assert.ok(used<tape.length,`${name}/${input.op}: unexpected random draw`);const value=tape[used++];draws.push(value);events.push({actor:'rng',op:'draw',index:used-1,value});return value;};
    active=true;let result;
    try{result=execute();assert.equal(used,tape.length,`${name}/${input.op}: random draw count`);}finally{active=false;Math.random=previous;}
    const log=exact(events);data.operations.push({...clone(input),argumentFloatBits:numericBits(input),rng:exact(draws),events:log,...(result===undefined?{}:{result:exact(result)}),expected:snapshot()});
  }
  const t={g,shooter,op:companion,data,
    target(type='rifle',owner=1,fields={}){assert.equal(data.initial,null,'Targets must be part of initial state');let raw;if(fields.operative){assert.equal(type,'rifle');const op=sim.joinOperative(g,fields.operative,owner);raw=g.units.get(op.unitId);g.operatives.set(op.key,observed(op,`companion:${op.key}`));}else raw=sim.internals.spawnUnit(g,owner,type);const unit=track(raw);g.units.set(unit.id,unit);sim.placeUnit(g,unit,{x:40,z:40,rot:0,vx:0,vz:0,...fields});return unit;},
    setupUnit(unit,fields){assert.equal(data.initial,null);sim.placeUnit(g,unit,fields);},
    terrain(cells){assert.equal(data.initial,null);for(const[c,flags,height]of cells){if(flags!==null)g.flags[c]=flags;if(height!==null){g.height??=new Int8Array(g.w*g.h);g.height[c]=height;}}},
    smoke(smokes){assert.equal(data.initial,null);g.smokes=smokes;},
    reorder(ids){assert.equal(data.initial,null);g.units=new Map(ids.map(id=>[id,g.units.get(id)]));},
    visible(ids){start();g.players[0].visible=new Set(ids);data.initial=snapshot(true);},
    fire(input={},moving=0,tape=[0,0]){assert.equal(tape.length,2);const frame={yaw:0,pitch:0,ads:false,...input};action({op:'fire',key:'gunner',unit:shooter.id,input:frame,moving},tape,()=>sim.fireOperative(g,shooter,companion,frame,moving));},
    hurt(unit,src=sim.UNITS.rifle.ab,fall=1,owner=shooter.owner){action({op:'hurt',target:unit.id,source:src,fall,owner},[],()=>sim.hurt(g,unit,src,fall,owner));},
    throw(input={}){const frame={yaw:0,pitch:0,...input};action({op:'throw',unit:shooter.id,input:frame},[],()=>sim.throwOperative(g,shooter,frame));},
    patch(unit,fields){action({op:'patch_unit',id:unit.id,fields},[],()=>sim.placeUnit(g,unit,fields));},
    tick(tick){action({op:'set_tick',tick},[],()=>{g.tick=tick;});},
    sample(id=g.nextFlight-1){const p=g.flights.get(id),times=[p.stamp-0.1,p.stamp,p.stamp+p.duration*.25,p.stamp+p.duration*.5,p.stamp+p.duration*.75,p.expires,p.expires+.05];action({op:'sample_flight',id,times},[],()=>times.map(time=>({time,position:projectile.flightAt(p,time),velocity:projectile.velocityAt(p,time)})));},
  };return t;
}
const finalShot=t=>t.g.shots.at(-1);
function expectHit(t,target){assert.equal(t.op.hits,1,t.data.name);assert.equal(finalShot(t).t,target.id,t.data.name);assert.ok(target.hp<sim.UNITS[target.type].models*sim.UNITS[target.type].hpPer,t.data.name);}
function expectMiss(t){assert.equal(t.op.hits,0,t.data.name);assert.equal(finalShot(t).hit,undefined,t.data.name);}
{
  const t=scene('visible-rifle-man-hit');const target=t.target();t.fire();expectHit(t,target);assert.equal(finalShot(t).m,12);assert.deepEqual(t.g.shots.map(s=>s.k),['hurt','sniper']);
}
{
  const t=scene('off-axis-ray-miss');t.target();t.fire({yaw:Math.PI/2});expectMiss(t);
}
for(const hidden of [false,true]){
  const t=scene(hidden?'hidden-friendly-body-skipped':'friendly-visible-body-stops-without-damage');const friend=t.target('rifle',2,{x:30,z:40,operative:'friendly',jy:0,crouch:false});const enemy=t.target('rifle',1,{x:40,z:40});
  if(hidden)t.visible([t.shooter.id,enemy.id]);t.fire();assert.equal(friend.hp,20);
  if(hidden)expectHit(t,enemy);else{expectMiss(t);assert.equal(enemy.hp,sim.UNITS.rifle.models*sim.UNITS.rifle.hpPer);assert.ok(finalShot(t).x<31);}
}
{
  const t=scene('hidden-enemy-body-skipped');const target=t.target();t.visible([t.shooter.id]);t.fire();expectMiss(t);assert.equal(target.hp,100);
}
{
  const t=scene('dead-riding-air-bodies-skipped');t.target('rifle',1,{x:25,hp:0});const carrier=t.target('tank',1,{x:80,z:80});const rider=t.target('rifle',1,{x:27,riding:carrier.id});t.setupUnit(carrier,{cargo:[rider.id]});t.target('fighter',1,{x:29});const target=t.target('tank',1,{x:40});t.fire();expectHit(t,target);assert.equal(target.hp,sim.UNITS.tank.hpPer-.5);
}
{
  const t=scene('drawn-outer-man-outside-squad-cylinder');const target=t.target('rifle',1,{z:41.69});t.fire();expectHit(t,target);assert.equal(finalShot(t).m,10);
}
{
  const t=scene('ray-outside-drawn-rank');t.target('rifle',1,{z:42.3});t.fire();expectMiss(t);
}
{
  const t=scene('ray-through-gun-crew-gap');t.target('mg',1,{z:40.55});t.fire();expectMiss(t);
}
{
  const t=scene('partial-health-rotated-moving-squad-slot');const target=t.target('rifle',1,{hp:27,rot:.7,vx:1.8,vz:-.9});
  const first=men.menAt(target,men.menDrawn(target.type,target.hp,100))[0];t.setupUnit(t.shooter,{z:first.z});t.fire();expectHit(t,target);
}
for(const kind of ['trench','garrison']){
  const t=scene(`collapsed-${kind}-body-and-damage`);const target=t.target('rifle',1,{hp:40,z:40.9});
  if(kind==='trench')t.terrain([[Math.floor(target.z/2)*64+Math.floor(target.x/2),sim.TERRAIN['.']|sim.TRENCH,null]]);
  else t.setupUnit(target,{garrison:Math.floor(target.z/2)*64+Math.floor(target.x/2)});
  t.fire();expectHit(t,target);assert.equal(target.hp,36);assert.equal(finalShot(t).m,undefined);
}
for(const crouch of [false,true]){
  const t=scene(crouch?'crouched-operative-below-standing-eye':'standing-operative-body');const target=t.target('rifle',1,{operative:'enemy-op',hp:20,jy:0,crouch});t.fire();if(crouch)expectMiss(t);else expectHit(t,target);
}
{
  const t=scene('crouched-eye-hits-crouched-operative');t.setupUnit(t.shooter,{crouch:true});const target=t.target('rifle',1,{operative:'enemy-op',hp:20,jy:0,crouch:true});t.fire();expectHit(t,target);
}
{
  const t=scene('jump-height-above-ground-operative-body');t.setupUnit(t.shooter,{jy:2});t.target('rifle',1,{operative:'enemy-op',hp:20,jy:0,crouch:false});t.fire();expectMiss(t);
}
{
  const t=scene('jumped-target-floor');t.setupUnit(t.shooter,{jy:1.2});const target=t.target('rifle',1,{operative:'enemy-op',hp:20,jy:1.2,crouch:false});t.fire();expectHit(t,target);
}
{
  const t=scene('structure-tall-body-and-demolition-base');t.setupUnit(t.shooter,{jy:1.8});const target=t.target('hq',1);const hp=target.hp;t.fire();expectHit(t,target);assert.equal(target.hp,hp-.5);
}
{
  const t=scene('killing-hit-counts-without-retirement');const target=t.target('rifle',1,{operative:'enemy-op',hp:3,jy:0,crouch:false});t.fire();assert.equal(target.hp,-5);assert.equal(t.op.kills,1);assert.ok(t.g.units.has(target.id));assert.equal(t.shooter.xp,0);
}
{
  const t=scene('retreat-veterancy-suppression-cap');const target=t.target('rifle',1,{hp:100,xp:500,retreating:true,supp:99});t.fire();expectHit(t,target);assert.equal(target.supp,100);
}
{
  const t=scene('smooth-ground-height-at-shooter-and-target');const target=t.target('rifle',1,{operative:'enemy-op',hp:20,jy:0,crouch:false});
  t.terrain(Array.from({length:64*64},(_,c)=>[c,null,c%64<20?2:3]));t.fire({pitch:Math.atan2(sim.heightAt(t.g,target.x,target.z)-sim.heightAt(t.g,t.shooter.x,t.shooter.z),target.x-t.shooter.x)});expectHit(t,target);
}
{
  const t=scene('ray-stops-below-smooth-rising-ground');t.terrain(Array.from({length:64*64},(_,c)=>[c,null,c%64>=14?4:0]));t.fire();expectMiss(t);assert.ok(finalShot(t).x<31);
}
{
  const t=scene('ray-stops-at-map-edge');t.setupUnit(t.shooter,{x:127.4,z:40});t.fire();expectMiss(t);assert.ok(finalShot(t).x>=128);
}
{
  const t=scene('clear-full-range-ray');t.fire();expectMiss(t);assert.equal(finalShot(t).x,80);
}
{
  const t=scene('sight-wall-periodic-ray-stop');const target=t.target();t.terrain(Array.from({length:64},(_,z)=>[z*64+16,sim.TERRAIN['.']|sim.SIGHT,null]));t.fire();expectMiss(t);assert.equal(target.hp,100);assert.ok(finalShot(t).x<35);
}
{
  const t=scene('last-cell-los-exclusion-at-body-hit');const target=t.target('rifle',1,{operative:'enemy-op',hp:20,jy:0,crouch:false,x:31.6});t.terrain([[20*64+15,sim.TERRAIN['.']|sim.SIGHT,null]]);t.fire();expectHit(t,target);
}
{
  const t=scene('smoke-blocks-long-ray');const target=t.target();t.smoke([{id:1,x:35,z:40,r:4,t:20}]);t.fire();expectMiss(t);assert.equal(target.hp,100);
}
{
  const t=scene('hill-los-check-above-physical-ground');const target=t.target('rifle',1,{operative:'enemy-op',hp:20,jy:10,crouch:false});t.setupUnit(t.shooter,{jy:10});t.terrain(Array.from({length:64},(_,z)=>[z*64+15,null,4]));t.fire();expectMiss(t);assert.equal(target.hp,20);assert.ok(finalShot(t).y>4*sim.CFG.levelHeight);assert.ok(finalShot(t).x<40);
}
for(const friendlyFirst of [false,true]){
  const t=scene(friendlyFirst?'overlapping-body-map-order-friendly-first':'overlapping-body-map-order-enemy-first');const enemy=t.target('rifle',1,{operative:'enemy-op',hp:20,jy:0,crouch:false,x:35});const friend=t.target('rifle',2,{operative:'ally-op',hp:20,jy:0,crouch:false,x:35});
  if(friendlyFirst)t.reorder([t.shooter.id,friend.id,enemy.id]);t.fire();if(friendlyFirst)expectMiss(t);else expectHit(t,enemy);assert.equal(friend.hp,20);
}
for(const [name,fields,input,moving,tape]of [
  ['negative-zero-aim-and-stationary-state',{rot:-0,aim:-0,vx:-0,vz:-0},{yaw:-0,pitch:-0},-0,[0,.25]],
  ['rifle-hip-spread',{weapon:0},{ads:false,yaw:.2,pitch:.1},0,[.36,.125]],
  ['rifle-ads-spread',{weapon:0},{ads:true,yaw:.2,pitch:.1},0,[.36,.125]],
  ['smg-hip-spread',{weapon:1},{ads:false,yaw:-.4,pitch:.2},3.6,[.81,.75]],
  ['smg-ads-crouch-spread',{weapon:1,crouch:true},{ads:true,yaw:-.4,pitch:.2},3.6,[.81,.75]],
  ['airborne-movement-spread-cap',{weapon:1,jy:1.2},{ads:false,yaw:0,pitch:0},6.75,[.64,.25]],
  ['airborne-movement-above-cap',{weapon:1,jy:1.2},{ads:false,yaw:0,pitch:0},30,[.64,.25]],
  ['upper-pitch-spread-clamp',{weapon:1,jy:1.2},{ads:false,yaw:0,pitch:1.45},6.75,[.81,.25]],
  ['lower-pitch-spread-clamp',{weapon:1,jy:1.2},{ads:false,yaw:0,pitch:-1.45},6.75,[.81,.75]],
]){const t=scene(name);t.setupUnit(t.shooter,fields);t.fire(input,moving,tape);expectMiss(t);}
const cap=traces.filter(t=>t.name==='airborne-movement-spread-cap'||t.name==='airborne-movement-above-cap');assert.deepEqual(cap[0].operations[0].expected.raw.shots,cap[1].operations[0].expected.raw.shots);
for(const [name,input,fields]of [
  ['grenade-minimum-range',{yaw:0,pitch:-1.45},{}],
  ['grenade-default-range',{yaw:0,pitch:0},{}],
  ['grenade-maximum-range',{yaw:0,pitch:1.45},{}],
  ['grenade-diagonal-aim',{yaw:.7,pitch:.25},{}],
  ['grenade-upper-map-clamp-duration',{yaw:Math.PI/4,pitch:1.45},{x:127.4,z:127.4}],
  ['grenade-lower-map-clamp-duration',{yaw:-3*Math.PI/4,pitch:1.45},{x:.6,z:.6}],
  ['grenade-crouched-airborne-origin',{yaw:0,pitch:0},{crouch:true,jy:2.5}],
]){const t=scene(name,'grenade');t.setupUnit(t.shooter,fields);t.throw(input);assert.deepEqual(t.g.shots.map(s=>s.k),['flight','throw']);assert.equal(t.shooter.nades,2);assert.equal(t.op.shots,0);t.sample();}
{
  const t=scene('grenade-discrete-elevated-origin-target','grenade');const targetCell=20*64+16;t.terrain([[20*64+10,null,2],[targetCell,null,3]]);t.throw();assert.equal(t.g.flights.get(1).y,2*sim.CFG.levelHeight+1.2);t.sample();
}
{
  const t=scene('grenade-consecutive-flight-registration-and-events','grenade');
  t.throw({yaw:.2,pitch:0});t.sample(1);t.tick(77);t.patch(t.shooter,{nadeT:0});t.throw({yaw:-.7,pitch:.5});t.sample(2);t.tick(117);t.patch(t.shooter,{nadeT:0});t.throw({yaw:Math.PI,pitch:-.5});t.sample(3);
  assert.deepEqual([...t.g.flights.keys()],[1,2,3]);assert.equal(t.g.nextFlight,4);assert.equal(t.shooter.nades,0);assert.deepEqual(t.g.shots.map(s=>s.k),['flight','throw','flight','throw','flight','throw']);
}
{
  const t=scene('hurt-event-precedes-on-deck-rider-disembark','hurt');
  t.setupUnit(t.shooter,{xp:17.3});
  const carrier=t.target('tank',1,{x:40.25,z:40.75});
  const oldImpulse={dir:.2,impulse:.8};
  const rider=t.target('rifle',1,{x:40.25,z:40.75,riding:carrier.id,deathImpulse:oldImpulse,orders:[{t:'move',x:50,z:50}],path:[{x:50,z:50}],targetId:99});
  t.setupUnit(carrier,{cargo:[rider.id]});
  t.terrain([[20*64+20,sim.TERRAIN['.']|sim.MOVE,null]]);
  assert.equal(sim.onDeck(t.g,rider),true);
  const contact={x:rider.x,z:rider.z},hp=rider.hp,xp=t.shooter.xp;
  t.hurt(rider);
  assert.deepEqual(clone(t.g.shots),[{t:rider.id,fo:0,to:1,x:contact.x,z:contact.z,k:'hurt',kill:false}]);
  assert.deepEqual({x:rider.x,z:rider.z},{x:43,z:41});
  assert.notDeepEqual({x:rider.x,z:rider.z},contact);
  assert.equal(rider.hp,hp-sim.UNITS.rifle.ab.inf);assert.equal(rider.supp,sim.CFG.riderShock);
  assert.equal(rider.riding,0);assert.deepEqual(carrier.cargo,[]);
  assert.deepEqual(rider.orders,[{t:'move',x:50,z:50}]);assert.deepEqual(rider.path,[{x:50,z:50}]);assert.equal(rider.targetId,99);
  assert.equal(bits(t.shooter.xp),bits(xp));assert.deepEqual(rider.deathImpulse,oldImpulse);
  assert.equal(t.op.shots,0);assert.equal(t.op.hits,0);assert.equal(t.op.kills,0);
  const log=t.data.operations[0].events.raw;
  const hurt=log.findIndex(e=>e.actor==='shots'&&e.op==='append'&&e.value.k==='hurt');
  const movement=log.findIndex(e=>e.actor===`unit:${rider.id}`&&e.op==='set'&&(e.field==='x'||e.field==='z'));
  assert.ok(hurt>=0&&movement>hurt,'Hurt uses contact position before nearestFree places the rider');
  assert.ok(!log.some(e=>e.op==='set'&&(e.field==='xp'||e.field==='deathImpulse')),'Plain hurt adds neither shooter XP nor a radial impulse');
}
const menCases=[];
for(const type of Object.keys(men.SLOTS)){
  const def=sim.UNITS[type],maxHp=def.models*def.hpPer;
  for(const [label,hp,rot,vx,vz]of [['full',maxHp,0,0,0],['partial-rotated-moving',maxHp*.41,.7,1.8,-.9],['last-man',maxHp*.005,-1.2,-2.7,1.35]]){
    const unit={type,x:40,z:40,hp,rot,vx,vz};const drawn=men.menDrawn(type,hp,maxHp);menCases.push({name:`${type}:${label}`,input:exact({unit,maxHp}),expected:exact({drawn,positions:men.menAt(unit,drawn)})});
  }
}
// Path replacements preserve full snapshots and every deletion, as in sibling recorders.
function changes(a,b,path=[],out=[]){if(Object.is(a,b))return out;if(Array.isArray(a)&&Array.isArray(b)&&a.length===b.length){for(let i=0;i<a.length;i++)changes(a[i],b[i],[...path,i],out);return out;}if(a&&b&&typeof a==='object'&&typeof b==='object'&&!Array.isArray(a)&&!Array.isArray(b)){for(const key of new Set([...Object.keys(a),...Object.keys(b)])){if(!Object.hasOwn(b,key))out.push([[...path,key]]);else changes(a[key],b[key],[...path,key],out);}return out;}out.push([path,b]);return out;}
function applyChanges(value,delta){for(const change of delta){const path=change[0];if(!path.length){value=clone(change[1]);continue;}let at=value;for(const key of path.slice(0,-1))at=at[key];const key=path.at(-1);if(change.length===1)delete at[key];else at[key]=clone(change[1]);}return value;}
for(const t of traces){let previous=t.initial;for(const operation of t.operations){const expected=operation.expected;operation.delta=changes(previous,expected);assert.deepEqual(applyChanges(clone(previous),operation.delta),expected);delete operation.expected;previous=expected;}}
const output=JSON.stringify({schema:1,reference:pin,node:process.version,arch:process.arch,sourceSha256:createHash('sha256').update(source).digest('hex'),map,unitKeys,
  constants:exact({operative:sim.OPERATIVE,weapons:sim.OPERATIVE_WEAPONS,rifleAbility:sim.UNITS.rifle.ab,grenadeProfile:projectile.PROJECTILE_PROFILES.grenade,slots:men.SLOTS,gunSlots:men.GUN_SLOTS,slotScale:men.SLOT_SCALE}),
  floatEncoding:'IEEE754 binary64 big-endian hex, numeric leaf paths separated by dots',
  projection:'Initial units are complete original rows. Runtime units use unitKeys. Flight source aliases use sourceId/sourceMatchesShooter; suppressed Set uses insertion-order values. Events are observed original property writes, registry inserts and internal array appends.',
  unsupported:['tech-enabled damage, Horde modifiers and passenger cases beyond the focused on-deck hurt trace','stepOperative effect admission, cooldown and grenade-priority gates','grenade contact, suppression, blast, terrain damage and retirement settlement','full match phase orchestration','native seeded RNG integration','recipient, network and C representations','adoption of canonical hit-direction policy'],menCases,traces})+'\n';
const destination=`${root}/rust/crates/simtest/tests/fixtures/fps-effects.json`;
if(process.argv.includes('--check'))assert.equal(readFileSync(destination,'utf8'),output);else writeFileSync(destination,output);
const fire=traces.filter(t=>t.category==='fire').length,grenade=traces.filter(t=>t.category==='grenade').length,hurt=traces.filter(t=>t.category==='hurt').length;
console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} ${fire} fire traces, ${grenade} grenade traces, ${hurt} direct hurt trace, ${menCases.length} squad-men cases and ${traces.reduce((n,t)=>n+t.operations.length,0)} operations`);
