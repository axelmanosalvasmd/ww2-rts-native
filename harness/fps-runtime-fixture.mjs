// Runs original private FPS phases at the pinned reference. The loader adds exports
// and exact lifecycle blocks; it does not change function bodies or run full ticks.
// Fire rays and grenade effects are outside this fixture.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { compileFunction } from 'node:vm';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
assert.equal(process.arch, 'x64'); assert.equal(process.version, 'v24.21.0');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
assert.equal(git(['rev-parse', 'HEAD']), pin);
assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
const source = readFileSync(`${reference}/shared/sim.js`, 'utf8');
const server = readFileSync(`${reference}/server.js`, 'utf8');
const between = (text, start, end) => {
  const a = text.indexOf(start), b = text.indexOf(end, a);
  assert.ok(a >= 0 && b > a, `Missing source seam ${start}`);
  assert.equal(text.indexOf(start, a + start.length), -1, `Ambiguous source seam ${start}`);
  return text.slice(a, b);
};
const respawn = between(source, '  for (const op of g.operatives?.values() ?? []) {', '  stepWorldDebris(g, TICK);');
const death = between(source, '    if (u.operative) {\n      const op = g.operatives.get(u.operative);', '    g.units.delete(u.id);');
const reset = between(server, '  op.online = connected(person); op.lastSeq = -1;', '  person.inputAt = 0;');
const url = pathToFileURL(`${reference}/shared/sim.js`).href;
registerHooks({ load(path, context, next) {
  const result = next(path, context);
  return path === url ? { ...result, source: `${result.source}\nexport { stepOperative, pickSquad, ledSquad, leadSquad, spawnOperative, placeUnit };\nexport function fpsRespawn(g) {\n${respawn}\n}\nexport function fpsDeath(g,u) {\n${death}\n}\nexport function fpsReset(op,online) { const connected=()=>online,person=null;\n${reset}\n}\n` } : result;
} });
const sim = await import(url);
const { rebuildGrid, gridFor } = await import(pathToFileURL(`${reference}/shared/grid.js`));
const map = { name: 'FPS phase fixture', w: 32, h: 32, rows: Array(32).fill('.'.repeat(32)), spawns: [{ x: 7, y: 16 }, { x: 24, y: 16 }], points: [{ x: 16, y: 16 }] };
const unitKeys = ['id','type','owner','x','z','rot','aim','moveSpeed','vx','vz','vy','jy','hp','supp','cooldown','crouch','operative','weapon','mags','reloadT','nades','nadeT','auto','autoRetreat','holdFire','holdPos','follow','riding','garrison','retreating','path','orders','amove','attackId','targetId','retarget','worldGoal','pace','moveOutcome','moveOutcomeTick','traffic','trafficWait','entrench','face'];
const clone = value => JSON.parse(JSON.stringify(value));
const projection = unit => Object.fromEntries(unitKeys.filter(k => unit[k] !== undefined).map(k => [k, unit[k]]));
const players = g => g.players.map(p => ({slot:p.slot,team:p.team,faction:p.faction,mp:p.mp,out:p.out ?? false,away:p.away ?? false,spawn:p.spawn}));
const bits = n => { const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex'); };
const finiteBits = value => {
  const fields = {};
  function visit(v, path) {
    if (typeof v === 'number' && Number.isFinite(v)) fields[path] = bits(v);
    else if (v && typeof v === 'object') for (const [k, nested] of Object.entries(v)) visit(nested, path ? `${path}.${k}` : k);
  }
  visit(value, ''); return fields;
};
const gridProbes = [{x:16,z:16},{x:32,z:32}].flatMap(at=>[0,15,80].map(radius=>({...at,radius})));
const gridSnapshot = g => {
  const grid=gridFor(g),ids=units=>units.map(u=>u.id);
  return {queries:gridProbes.map(({x,z,radius})=>({x,z,radius,inOrder:ids(grid.inOrder({x,z},radius,()=>true)),unordered:ids(grid.unordered({x,z},radius)),heavy:ids(grid.heavy({x,z},radius))})),owned:g.players.map(p=>({owner:p.slot,ids:ids(grid.ownedBy(p.slot))}))};
};
const snapshot = (g, full = false) => {
  const playerRows=players(g),unitRows=[...g.units.values()].map(u=>full ? u : projection(u)),ops=[...(g.operatives?.values() ?? [])];
  return clone({grid:gridSnapshot(g),floatBits:{game:finiteBits({tick:g.tick,winner:g.winner,nextId:g.nextId}),players:playerRows.map(p=>({slot:p.slot,fields:finiteBits(p)})),units:unitRows.map(u=>({id:u.id,fields:finiteBits(u)})),operatives:ops.map(op=>({key:op.key,fields:finiteBits(op)}))},tick:g.tick,winner:g.winner,nextId:g.nextId,players:playerRows,unitOrder:[...g.units.keys()],units:unitRows,operatives:ops});
};
const traces = [];
let terrain;
function trace(name) {
  const old = Math.random; Math.random = () => 0.23456789;
  let g; try { g = sim.createGame(map,['Commander','Enemy'],false,[0,1],[0,1],{supply:false,logistics:false,weather:false,tech:false}); } finally { Math.random = old; }
  terrain ??= {flags:[...g.flags],height:g.height&&[...g.height]};
  const data = {name,initial:snapshot(g,true),operations:[]}; traces.push(data);
  function record(action, execute) {
    const previous = Math.random; Math.random = () => { throw Error(`Random effect in ${name}/${action.op}`); };
    let result; try { result = execute(); } finally { Math.random = previous; }
    data.operations.push(clone({...action,...(result === undefined ? {} : {result}),expected:snapshot(g)})); return result;
  }
  const api = {
    g,data,
    join:(key='friend',owner=0)=>record({op:'join',key,owner},()=>sim.joinOperative(g,key,owner)?.unitId ?? null),
    input:(input,key='friend')=>record({op:'input',key,input},()=>sim.operativeInput(g,key,input)),
    patchUnit:(id,fields)=>record({op:'patch_unit',id,fields},()=>{const u=g.units.get(id);sim.placeUnit(g,u,fields);}),
    patchOp:(fields,key='friend')=>record({op:'patch_op',key,fields},()=>Object.assign(g.operatives.get(key),fields)&&undefined),
    patchPlayer:(owner,fields)=>record({op:'patch_player',owner,fields},()=>Object.assign(g.players[owner],fields)&&undefined),
    tick:tick=>record({op:'set_tick',tick},()=>{g.tick=tick;}),
    winner:winner=>record({op:'set_winner',winner},()=>{g.winner=winner;}),
    step:(key='friend')=>record({op:'step',key},()=>{g.tick++;rebuildGrid(g);const op=g.operatives.get(key);if(op.unitId)sim.stepOperative(g,g.units.get(op.unitId));}),
    respawn:()=>record({op:'pre_respawn'},()=>sim.fpsRespawn(g)),
    death:(key='friend')=>record({op:'death',key},()=>{const op=g.operatives.get(key),u=g.units.get(op.unitId);assert.ok(u.hp<=0);sim.fpsDeath(g,u);g.units.delete(u.id);}),
    reset:(online,key='friend')=>record({op:'reset',key,online},()=>sim.fpsReset(g.operatives.get(key),online)),
    pick:(key='friend')=>record({op:'pick',key},()=>{const op=g.operatives.get(key);sim.pickSquad(g,g.units.get(op.unitId),op);}),
    lead:(key='friend')=>record({op:'lead',key},()=>{const op=g.operatives.get(key);sim.leadSquad(g,g.units.get(op.unitId),op);}),
    command:(owner,command)=>record({op:'command',owner,command},()=>sim.command(g,owner,command)??null),
    terrain:(cells,height=false)=>record({op:'terrain',cells,height},()=>{if(height&&!g.height)g.height=new Int8Array(g.w*g.h);for(const[c,flag,level]of cells){g.flags[c]=flag;if(g.height&&level!==null)g.height[c]=level;}g.infantryRegionVersion=(g.infantryRegionVersion??0)+1;g.vehicleRegionVersion=(g.vehicleRegionVersion??0)+1;}),
    hold:(extra={},ticks=1)=>{for(let i=0;i<ticks;i++){const op=g.operatives.get('friend');api.input({seq:op.lastSeq+1,forward:0,strafe:0,yaw:0,pitch:0,fire:false,jump:0,reload:0,nade:0,squad:0,...extra});api.step();}},
  };
  return api;
}
{
  const t=trace('join-input-baseline-reset'); const id=t.join();
  t.join();t.join('invalid',99);t.join('friend',1);
  t.input(null);t.input({seq:0,forward:0,strafe:0,yaw:0,pitch:0,fire:'true',ads:1,crouch:1,sprint:1,weapon:1,jump:90,reload:90,nade:90,squad:90});
  const frame={seq:1,forward:1,strafe:0,yaw:0,pitch:0};
  for(const bad of [{seq:0},{seq:-1},{seq:1.5},{seq:9007199254740992},{forward:1.01},{strafe:-1.01},{yaw:10000.1},{pitch:1.45001},{forward:null},{yaw:'0'}])t.input({...frame,...bad});
  t.input({...frame,weapon:9,jump:91,reload:91,nade:91,squad:91,fire:true});
  // Clear the fire and grenade edges through the source reconnect reset before stepping.
  t.reset(true);t.input({...frame,seq:0,jump:100,reload:100,nade:100,squad:100});t.step();
  t.input({...frame,seq:1,jump:101,reload:100,nade:100,squad:100});t.step();
  t.input({...frame,seq:2,jump:4,reload:-1,nade:null,squad:2,weapon:1.5});t.step();
  t.reset(false);t.input({...frame,seq:100});t.step();t.reset(true);t.input({...frame,seq:0,jump:7});t.step();
  assert.equal(t.g.units.get(id).nades,3);assert.equal(t.g.operatives.get('friend').shots,0);
}
{
  const t=trace('walk-sprint-crouch-ads-diagonal-stale-offline'); const id=t.join();t.patchUnit(id,{x:24,z:24,supp:3,nadeT:0.31,cooldown:0.31});
  t.hold({forward:1},6);t.hold({forward:1,sprint:true},6);t.hold({forward:1,sprint:true,ads:true},5);t.hold({forward:1,crouch:true},5);t.hold({forward:1,strafe:1,yaw:0.37},6);t.hold({forward:-1,sprint:true},4);
  for(let i=0;i<12;i++)t.step();t.hold({forward:1},5);t.patchOp({online:false});for(let i=0;i<5;i++)t.step();
  t.patchOp({online:true});t.hold({},3);
}
{
  const t=trace('jump-air-control-crouch-landing');const id=t.join();t.patchUnit(id,{x:24,z:24});t.hold({crouch:true});t.hold({forward:1,jump:1},2);t.hold({forward:0,strafe:1,yaw:0.7,jump:1,crouch:true},6);t.hold({forward:-1,jump:1},8);t.hold({jump:1},6);t.hold({jump:1},2);assert.equal(t.g.units.get(id).jy,0);
}
for(const mode of ['wall-slide','cliff-slide','map-edges']){
  const t=trace(mode);const id=t.join();
  if(mode==='map-edges'){
    for(const [x,z,yaw]of[[0.41,20,Math.PI],[63.59,20,0],[20,0.41,-Math.PI/2],[20,63.59,Math.PI/2]]){t.patchUnit(id,{x,z,vx:0,vz:0});t.hold({forward:1,yaw},3);}
  }else{
    t.patchUnit(id,{x:31.7,z:28,vx:0,vz:0});t.terrain(Array.from({length:32},(_,z)=>[z*32+16,mode==='wall-slide'?sim.MOVE:0,mode==='cliff-slide'?4:null]),mode==='cliff-slide');t.hold({forward:1,strafe:0.6},10);
  }
}
{
  const t=trace('weapon-swap-reload-cancel-complete');const id=t.join();t.patchUnit(id,{x:24,z:24,mags:[3,4]});t.hold({});t.hold({reload:1});t.hold({reload:1},3);t.hold({weapon:1,reload:1});t.hold({weapon:1,reload:2});t.hold({weapon:1,reload:2},44);t.hold({weapon:1,reload:3});t.hold({weapon:0,reload:3});
}
{
  const t=trace('squad-nearest-strict-range-ties-release-retreat-follow');const id=t.join();t.patchUnit(id,{x:24,z:24,rot:0});
  const own=[...t.g.units.values()].filter(u=>u.owner===0&&!u.operative), enemy=[...t.g.units.values()].find(u=>u.owner===1);
  for(let i=0;i<own.length;i++)t.patchUnit(own[i].id,{x:32+i,z:24});t.patchUnit(enemy.id,{x:24,z:24});t.pick();assert.equal(t.g.operatives.get('friend').squadId,0);
  t.patchUnit(own[0].id,{x:24,z:30});t.patchUnit(own[1].id,{x:24,z:18});t.pick();assert.equal(t.g.operatives.get('friend').squadId,own[0].id);t.pick();
  t.patchUnit(own[0].id,{riding:99});t.pick();assert.equal(t.g.operatives.get('friend').squadId,own[1].id);
  t.tick(5);t.lead();t.patchUnit(id,{x:36,z:24});t.tick(6);t.lead();t.patchUnit(own[1].id,{retreating:true});t.tick(10);t.lead();t.patchUnit(own[1].id,{retreating:false});t.lead();
  t.command(0,{t:'move',orders:[[own[1].id,10,10]]});t.lead();t.command(0,{t:'move',orders:[[id,10,10]]});
}
{
  const t=trace('death-delay-paid-respawn-gates');const id=t.join();t.hold({forward:1});t.patchUnit(id,{hp:0});t.death();const at=t.g.operatives.get('friend').respawnAt;assert.equal(at,t.g.tick+160);
  t.patchPlayer(0,{mp:9});t.tick(at-1);t.respawn();t.tick(at);t.respawn();t.patchPlayer(0,{mp:10});t.patchOp({online:false});t.respawn();t.patchOp({online:true});t.winner(1);t.respawn();t.winner(null);t.patchPlayer(0,{out:true});t.respawn();t.patchPlayer(0,{out:false});t.respawn();assert.equal(t.g.players[0].mp,0);assert.notEqual(t.g.operatives.get('friend').unitId,id);t.join();
}
for(const eligibility of ['eligible','one-man','garrisoned','riding','dead','taken-back']){
  const t=trace(`squad-respawn-${eligibility}`);const id=t.join();const s=[...t.g.units.values()].find(u=>u.owner===0&&!u.operative);
  t.patchUnit(id,{x:24,z:24});t.patchUnit(s.id,{x:24,z:27,hp:40});t.pick();assert.equal(t.g.operatives.get('friend').squadId,s.id);t.patchUnit(id,{hp:0});t.death();
  if(eligibility==='one-man')t.patchUnit(s.id,{hp:20});if(eligibility==='dead')t.patchUnit(s.id,{hp:0});if(eligibility==='garrisoned')t.patchUnit(s.id,{garrison:1});if(eligibility==='riding')t.patchUnit(s.id,{riding:99});if(eligibility==='taken-back')t.patchUnit(s.id,{follow:0});
  t.patchPlayer(0,{mp:0});t.tick(t.g.operatives.get('friend').respawnAt);t.respawn();
  if(eligibility==='eligible'){assert.equal(t.g.units.get(s.id).hp,20);assert.ok(t.g.operatives.get('friend').unitId);t.patchUnit(t.g.operatives.get('friend').unitId,{hp:0});t.death();t.tick(t.g.operatives.get('friend').respawnAt);t.respawn();t.patchPlayer(0,{mp:10});t.respawn();}
  else {assert.equal(t.g.operatives.get('friend').unitId,0);t.patchPlayer(0,{mp:10});t.respawn();}
}
{
  const t=trace('join-and-live-input-out-winner-gates');t.patchPlayer(0,{out:true});t.join();t.patchPlayer(0,{out:false});t.winner(1);t.join();t.winner(null);const id=t.join();t.hold({forward:1},4);t.patchPlayer(0,{out:true});t.input({seq:100,forward:1,strafe:0,yaw:0,pitch:0});t.step();t.patchPlayer(0,{out:false});t.winner(1);t.input({seq:100,forward:1,strafe:0,yaw:0,pitch:0});t.step();t.winner(null);t.patchUnit(id,{hp:0});t.death();
}
{
  const t=trace('multiple-companions-insertion-order-wallet-delay-loadout');
  const keys=['zulu','alpha','beta'],wallet=t.g.players[0].mp,original=keys.map(key=>t.join(key,0));
  assert.equal(t.g.players[0].mp,wallet);
  assert.deepEqual([...t.g.operatives.keys()],keys);
  t.tick(7);
  for(const unit of original)t.patchUnit(unit,{hp:0});
  for(const key of keys)t.death(key);
  const due=t.g.operatives.get('zulu').respawnAt;
  assert.ok(keys.every(key=>t.g.operatives.get(key).respawnAt===due));
  t.input({seq:0,forward:0,strafe:0,yaw:0,pitch:0,fire:false,weapon:1,jump:0,reload:0,nade:0,squad:0},'beta');
  t.patchPlayer(0,{mp:20});t.tick(due-1);t.respawn();
  assert.ok(keys.every(key=>t.g.operatives.get(key).unitId===0));
  t.tick(due);t.respawn();
  assert.ok(t.g.operatives.get('zulu').unitId>0&&t.g.operatives.get('alpha').unitId>0);
  assert.equal(t.g.operatives.get('beta').unitId,0);assert.equal(t.g.players[0].mp,0);
  t.tick(due+1);t.respawn();t.patchPlayer(0,{mp:10});t.respawn();
  assert.equal(t.g.players[0].mp,0);assert.equal(t.g.units.get(t.g.operatives.get('beta').unitId).weapon,1);
  assert.equal(t.g.operatives.get('beta').input,null);
}
// These server statements run with explicit clock and send hooks. They check
// admission and message permissions independently of the match rules.
const admitBlock = between(server, '        const commander = Number.isInteger(msg.commander) && room.players[msg.commander];', '\n      }\n      else if (room.state');
const admit = compileFunction(`let me;\n${admitBlock}\nreturn me;`, ['room','msg','token','name','ws','send']);
const whitelistBlock = between(server, "    if (me.role === 'operative') {\n      if (msg.t === 'fps' && room.state === 'play' && room.game && !room.pause) {", "    if (msg.t === 'name' && typeof msg.name === 'string') { me.name = cleanName(msg.name); lobby(room); }\n    else if");
const cleanName = server.split('\n').find(line=>line.startsWith('const cleanName = '));
assert.ok(cleanName);
const whitelist = compileFunction(`${cleanName}\n${whitelistBlock}`, ['room','me','msg','clock','operativeInput','lobby']);
const permissions = {admission:[],whitelist:[]};
for(const [name,commander,ai,count,token,mode] of [
 ['human-commander',0,false,0,'friend','conquest'],['invalid-commander',99,false,0,'friend','conquest'],
 ['fractional-commander',0.5,false,0,'friend','conquest'],['ai-commander',0,true,0,'friend','conquest'],
 ['four-operative-limit',0,false,4,'friend','conquest'],['empty-identity',0,false,0,'','conquest'],
 ['world-mode',0,false,0,'friend','world'],['fourth-operative',0,false,3,'friend','conquest'],
]) {
 const room={players:[{ai}],operatives:Array.from({length:count},(_,i)=>({token:`existing-${i}`})),mode};
 const calls=[],ws={close:(...args)=>{calls.push(['close',...args]);}};
 const me=admit(room,{commander},token,'Friend',ws,(_,msg)=>calls.push(['send',msg]));
 permissions.admission.push({name,commander,ai,count,token,mode,accepted:!!me,roster:room.operatives.map(p=>p.token),calls});
}
for(const [name,t,state,game,pause,now,inputAt] of [
 ['fps-live','fps','play',true,false,100,0],['fps-under-20ms','fps','play',true,false,119,100],
 ['fps-at-20ms','fps','play',true,false,120,100],['fps-lobby','fps','lobby',true,false,100,0],
 ['fps-no-game','fps','play',false,false,100,0],['fps-paused','fps','play',true,true,100,0],
 ['army-command','cmd','play',true,false,100,0],['host-command','addAi','lobby',true,false,100,0],
 ['seat-change','team','lobby',true,false,100,0],['resync','resync','play',true,false,100,0],
 ['name','name','play',true,false,100,0],
]) {
 const room={state,game:game?{}:null,pause:pause?{}:null},me={role:'operative',token:'friend',inputAt,net:{full:false},name:'Friend'},msg={t,name:'  <Alice>  '},calls=[];
 whitelist(room,me,msg,{now:()=>now},(_g,key,frame)=>calls.push(['input',key,frame]),()=>calls.push(['lobby']));
 permissions.whitelist.push({name,t,state,game,pause,now,inputAt,expected:{inputAt:me.inputAt,full:me.net.full,name:me.name,calls}});
}
// Match the sibling environment and convoy recorders: path replacements replay
// each complete snapshot, including field deletions and every exact float bit.
function changes(a,b,path=[],out=[]) {
  if(Object.is(a,b))return out;
  if(Array.isArray(a)&&Array.isArray(b)&&a.length===b.length){for(let i=0;i<a.length;i++)changes(a[i],b[i],[...path,i],out);return out;}
  if(a&&b&&typeof a==='object'&&typeof b==='object'&&!Array.isArray(a)&&!Array.isArray(b)){
    for(const key of new Set([...Object.keys(a),...Object.keys(b)])){
      if(!Object.hasOwn(b,key))out.push([[...path,key]]);else changes(a[key],b[key],[...path,key],out);
    }return out;
  }
  out.push([path,b]);return out;
}
function applyChanges(value,delta) {
  for(const[path,replacement]of delta){let at=value;for(const key of path.slice(0,-1))at=at[key];if(delta.length===0)continue;
    const key=path.at(-1);if(replacement===undefined)delete at[key];else at[key]=clone(replacement);
  }return value;
}
for(const t of traces){let previous=t.initial;for(const operation of t.operations){const expected=operation.expected,delta=changes(previous,expected);assert.deepEqual(applyChanges(clone(previous),delta),expected);operation.delta=delta;delete operation.expected;previous=expected;}}
const output = JSON.stringify({schema:2,reference:pin,node:process.version,arch:process.arch,map,terrain,options:{supply:false,logistics:false,weather:false,tech:false},unitKeys,permissions,floatEncoding:"IEEE754 binary64 big-endian hex, finite numeric leaf paths separated by dots",constants:{tick:sim.TICK,cell:sim.CELL,operative:sim.OPERATIVE,weapons:sim.OPERATIVE_WEAPONS},unsupported:['operative fire rays, hit selection and damage','grenade flight, blast and projectile effects','full match phase orchestration','squad movement after the real source command is issued'],traces})+'\n';
const destination = `${root}/rust/crates/simtest/tests/fixtures/fps-runtime.json`;
if(process.argv.includes('--check'))assert.equal(readFileSync(destination,'utf8'),output);
else writeFileSync(destination,output);
console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} ${traces.length} FPS traces and ${traces.reduce((n,t)=>n+t.operations.length,0)} operations`);
