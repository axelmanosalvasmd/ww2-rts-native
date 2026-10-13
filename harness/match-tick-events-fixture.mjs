// Record complete original steps and observe shots at their actual append boundaries.
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {gzipSync} from 'node:zlib';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], {encoding:'utf8'}).trim();
const clean = () => {
  assert.equal(git(['rev-parse','HEAD']), pin);
  assert.equal(git(['status','--porcelain','--untracked-files=no']), '');
};
assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
clean();
const simURL = pathToFileURL(resolve(reference, 'shared/sim.js')).href;
const sim = await import(simURL);
const {rebuildGrid} = await import(pathToFileURL(resolve(reference, 'shared/grid.js')));
const mask = (1n << 64n) - 1n, gamma = 0x9e3779b97f4a7c15n;
const mix = x => {
  x = ((x ^ (x >> 30n)) * 0xbf58476d1ce4e5b9n) & mask;
  x = ((x ^ (x >> 27n)) * 0x94d049bb133111ebn) & mask;
  return x ^ (x >> 31n);
};
function stream(purpose) {
  let state = mix(811n ^ ((BigInt(purpose) * gamma) & mask)), count = 0;
  return {next() { state = (state + gamma) & mask; count++; return Number(mix(state) >> 11n) / 9007199254740992; },
    snapshot: () => ({count, state:state.toString(16).padStart(16, '0')})};
}
const buffer = new DataView(new ArrayBuffer(8));
const bits = n => { buffer.setFloat64(0, n); return buffer.getBigUint64(0).toString(16).padStart(16, '0'); };
const map = {name:'Chronological full-step event arena', w:40, h:40, rows:Array(40).fill('.'.repeat(40)),
  spawns:[{x:2,y:2},{x:37,y:37}], points:[{id:'front',x:30,y:15}]};
const options = {mode:'conquest', weather:false, tech:false, supply:false, logistics:false};
const ground = [
  {kind:'rifle',owner:0,x:25,z:25,still:20},
  {kind:'rifle',owner:1,x:37,z:25,still:20},
  {kind:'rifle',owner:0,x:25,z:45,still:20},
  {kind:'rifle',owner:1,x:37,z:45,still:20},
  {kind:'rifle',owner:1,x:13,z:65,hp:44,still:20},
];
const fireCell = 32 * map.w + 6;
const fires = Array.from({length:9}, (_, i) => [fireCell + (Math.floor(i / 3) - 1) * map.w + i % 3 - 1, 11]);
const commands = [
  {tick:1,owner:0,command:{t:'ability',ids:[1002],x:37,z:45}},
  {tick:10,owner:0,command:{t:'support',kind:'artillery',x:41,z:41,dir:.3}},
  {tick:10,owner:0,command:{t:'support',kind:'smoke',x:41,z:55,dir:.3}},
  {tick:110,owner:0,command:{t:'stance',ids:[1000],key:'holdFire',on:false}},
];
function game(specs) {
  const old = Math.random; Math.random = () => .5; let g;
  try { g = sim.createGame(map, ['A','B'], false, [0,1], [0,1], options); } finally { Math.random = old; }
  g.units.clear(); g.nextId = 1000; g.seed = 123; g.wind = {a:0,v:.5};
  for (const p of g.players) p.mp = 500;
  for (const spec of specs) {
    const u = sim.internals.spawnUnit(g, spec.owner, spec.kind);
    const {kind, owner, air, ...fields} = spec;
    Object.assign(u, {holdFire:true,holdPos:true,auto:false,autoRetreat:false}, structuredClone(fields));
    if (air) Object.assign(u.air, structuredClone(air));
  }
  for (const [cell,left] of fires) g.fires.set(cell,left);
  rebuildGrid(g);
  for (const p of g.players) p.visible = new Set(g.units.keys());
  g.shots = [];
  return g;
}
const projection = g => ({tick:g.tick, nextId:g.nextId, seed:g.seed >>> 0,
  units:[...g.units.values()].map(u => [u.id,bits(u.x),bits(u.z),bits(u.rot),bits(u.hp),bits(u.supp)]),
  flights:g.flights.size, grenades:g.nades.length, salvos:g.salvos.length, strikes:g.strikes.length,
  shots:structuredClone(g.shots), story:g.players.map((_,i) => ['kills','losses','built','captures','mpSpent','supportCalls','planesDowned'].map(k => bits(g.story?.[i]?.[k] ?? 0)))});
const witness = g => ({...projection(g), units:structuredClone([...g.units.values()]),
  players:g.players.map(p => ({...structuredClone(p),visible:[...p.visible]})),
  story:structuredClone(g.story), points:structuredClone(g.points), flights:structuredClone([...g.flights]),
  nades:structuredClone(g.nades), salvos:structuredClone(g.salvos), strikes:structuredClone(g.strikes),
  smokes:structuredClone(g.smokes), fires:[...g.fires], chars:g.chars.join(''),
  cellHp:Array.from(g.cellHp), wear:Array.from(g.wear), scar:Array.from(g.scar), burnt:Array.from(g.burnt)});
function run(name, specs, observed) {
  const g = game(specs), random = {Combat:stream(3),OffMap:stream(4)}, frames = [], drawSites = new Map();
  for (let tick = 1; tick <= 125; tick++) {
    g.shots = [];
    const inputs = commands.filter(c => c.tick === tick);
    const denials = inputs.map(c => sim.command(g, c.owner, structuredClone(c.command)) ?? null);
    assert(denials.every(d => d === null), `${name}:${tick}: original command denial`);
    const prior = [...g.units.values()].map(u => [u.id,bits(u.x),bits(u.z),bits(u.rot)]);
    const appended = [], emissions = [];
    if (observed) Object.defineProperty(g.shots, 'push', {value:function(...events) {
      const site = new Error().stack.split('\n').find(s => s.includes(`${simURL}:`));
      assert(site, 'shot append must identify an original source site');
      for (const event of events) { appended.push(event); emissions.push({index:emissions.length,site:site.trim().replace(reference,'reference/ww2-rts')}); }
      return Array.prototype.push.apply(this, events);
    }});
    const old = Math.random;
    Math.random = () => {
      const stack = new Error().stack, source = stack.split('\n').find(s => s.includes(`${simURL}:`));
      assert(source, `${name}: unknown source randomness`);
      const purpose = stack.includes('stepStrikes') ? 'OffMap' : 'Combat';
      assert(purpose === 'OffMap' || /\b(fire|addFlight|launchSalvo)\b/.test(stack), `${name}: unexpected purpose ${stack}`);
      const site = `${purpose}:${source.trim().replace(reference,'reference/ww2-rts')}`;
      drawSites.set(site, (drawSites.get(site) ?? 0) + 1);
      return random[purpose].next();
    };
    try { sim.step(g); } finally { Math.random = old; }
    if (observed) assert.deepEqual(appended, structuredClone(g.shots), `${name}:${tick}: append order differs from shots`);
    const retired = prior.map(r => r[0]).filter(id => !g.units.has(id));
    frames.push({inputs:inputs.map(({owner,command}) => ({owner,command})),denials,prior,retired,
      emissions, expected:projection(g), random:Object.fromEntries(Object.entries(random).map(([k,v]) => [k,v.snapshot()])), witness:witness(g)});
  }
  return {frames,drawSites:[...drawSites].map(([site,count]) => ({site,count}))};
}
const traces = [];
for (const variant of ['ground','air','air-death']) {
  const air = variant !== 'ground';
  const name = variant === 'air-death' ? 'launch-aa-casualty-then-support-fire' : air ? 'launch-projectile-support-aa-fire-death' : 'launch-projectile-support-fire-death';
  const specs = [...ground.map(s => structuredClone(s))];
  if (air) specs.push({kind:'flak',owner:0,x:40,z:40,still:20,holdFire:false,cooldown:100,aaShot:variant === 'air-death' ? 0 : .25},
    {kind:'bomber',owner:1,x:44,z:40,hp:variant === 'air-death' ? 1 : 420,air:{state:'station',mission:{kind:'patrol',id:0,x:44,z:40}}});
  const a = run(name,specs,true), b = run(name,specs,false);
  for (let i = 0; i < a.frames.length; i++) {
    assert.deepEqual(a.frames[i].witness,b.frames[i].witness,`${name}:${i+1}: observer changed original full step`);
    assert.deepEqual(a.frames[i].random,b.frames[i].random,`${name}:${i+1}: observer changed random draws`);
  }
  assert.deepEqual(a.drawSites,b.drawSites);
  const events = a.frames.flatMap(f => f.expected.shots);
  assert(events.some(e => e.k === 'flight'));
  assert(events.some(e => e.k === 'throw'));
  assert(events.some(e => e.k === 'contact' && e.explosive));
  assert(events.some(e => e.k === 'hurt' && e.local));
  assert(events.some(e => e.k === 'shell'));
  assert(events.some(e => e.k === 'smokeshells'));
  const mixed = a.frames[109].expected.shots;
  const launch = mixed.findIndex(e => e.k === 'flight');
  const support = mixed.findIndex(e => e.k === 'shell');
  const late = mixed.findIndex(e => e.k === 'hurt' && e.t === 1004 && e.kill);
  assert(launch >= 0 && support > launch && late > support, `${name}: missing tick-110 launch/support/late-fire order`);
  assert.deepEqual(a.frames[109].retired.filter(id => id === 1004),[1004]);
  if (variant === 'air') {
    const aa = mixed.findIndex(e => e.k === 'aa');
    assert(aa > support && late > aa, `${name}: missing support/AA/fire order`);
  }
  if (variant === 'air-death') {
    const first = a.frames[0].expected.shots;
    const aa = first.findIndex(e => e.k === 'aa'), down = first.findIndex(e => e.k === 'planedown');
    assert(aa > first.findIndex(e => e.k === 'flight') && down > aa);
    assert(a.frames[0].retired.includes(1006));
    assert.equal(a.frames[1].expected.shots.length,0,'AA casualty must not emit again next tick');
  }
  traces.push({name,map,specs,visible:true,fires,seed:811,environmentSeed:123,wind:{a:0,v:.5},...a});
}
clean();
const steps = traces.reduce((n,t) => n+t.frames.length,0);
const events = traces.reduce((n,t) => n+t.frames.reduce((n,f) => n+f.expected.shots.length,0),0);
const output = {reference:pin,node:process.version,arch:process.arch,seed:811,steps,events,
  scope:'Original complete commands and sim.step bodies. Shots observed at actual append boundaries and verified against every unobserved full-step witness. Native consumes one advance_tick per Source step with naturally seeded MatchRandom.',traces};
const path = resolve(root,'rust/crates/simtest/tests/fixtures/match-tick-events.json.gz');
const compressed = gzipSync(JSON.stringify(output)+'\n',{level:9});
if (process.argv.includes('--check')) assert.deepEqual(readFileSync(path),compressed);
else writeFileSync(path,compressed);
console.log(`${traces.length} original full-step traces, ${steps} steps, ${events} chronological shots; unobserved replay agrees after every step`);
