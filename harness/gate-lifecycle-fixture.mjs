// Exact isolated Gate components from the unchanged pinned prototype functions.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync, writeFileSync, realpathSync} from 'node:fs';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert(process.argv.slice(2).every(arg => arg === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = realpathSync(resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], {encoding: 'utf8'}).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const url = pathToFileURL(`${reference}/shared/sim.js`).href;
// Export private functions without replacing their bodies or dependencies.
registerHooks({load(u, context, next) {
  const result = next(u, context);
  return u === url ? {...result, source: String(result.source) + '\nexport {stepGate, unitRow, squareDir};\n'} : result;
}});
const sim = await import(url);
const {rebuildGrid} = await import(pathToFileURL(`${reference}/shared/grid.js`));
const {TIER} = await import(pathToFileURL(`${reference}/shared/tech.js`));
const bits = n => { const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex'); };
const map = {name: 'Gate components', w: 60, h: 60, rows: Array(60).fill('.'.repeat(60)),
  heights: Array(60).fill('0'.repeat(60)), spawns: [{x:4,y:4},{x:55,y:55}], points:[{x:30,y:30}]};
function make() {
  const old = Math.random;
  Math.random = () => .5;
  let g;
  try { g = sim.createGame(map, ['A','B'], false, [0,1], [0,1], {weather:false,tech:false,supply:false,logistics:false}); }
  finally { Math.random = old; }
  g.units.clear();
  g.nextId = 1000;
  return g;
}
const placements = [];
for (const [x,z] of [[30,25],[1,1],[120,120]]) {
  for (const dir of [0, .7, Math.PI/4, -Math.PI/4, Math.PI/2, -1.2, 7.5]) {
    for (const terrain of ['open','blocked','hidden','road','path','mud']) {
      const g = make();
      const cell = 12*60+15;
      if (terrain === 'blocked' || terrain === 'hidden') sim.internals.setCell(g, cell, 'H');
      if (['road','path','mud'].includes(terrain)) sim.internals.setCell(g, cell, {road:'D',path:'+',mud:'M'}[terrain]);
      const result = sim.placementCheck(g, {kind:'gate',x,z,dir,team:0}, at => terrain !== 'hidden' || Math.floor(at.z/2)*60+Math.floor(at.x/2) !== cell);
      placements.push({x,z,dir:bits(dir),terrain,square:bits(sim.squareDir(dir)),
        result:{ok:result.ok,reason:result.reason ?? null,cells:(result.cells ?? []).map(([c]) => c)}});
    }
  }
}
const gateCell = 15*60+15;
const traces = [];
function trace(name, actions, surface = '.') {
  const g = make();
  sim.internals.setCell(g,gateCell,surface);
  const gate = sim.internals.placeBuilding(g, 0, 'gate', gateCell, true);
  const troop = sim.internals.spawnUnit(g, 1, 'rifle');
  const friend = sim.internals.spawnUnit(g, 0, 'rifle');
  Object.assign(troop,{x:90,z:90}); Object.assign(friend,{x:90,z:94});
  const frames = [];
  for (const action of actions) {
    for (const [actor, fields] of Object.entries(action.patch ?? {})) Object.assign({troop,friend,gate}[actor], fields);
    rebuildGrid(g);
    sim.stepGate(g, gate, action.dt);
    frames.push({action,open:gate.open ?? null,timer:bits(gate.gateT),ch:g.chars[gateCell],flags:sim.unitRow(g,gate)[12]});
  }
  traces.push({name,surface,frames});
}
const step = (patch = {}, dt = .5) => ({patch,dt});
trace('bucket-threat-hysteresis-and-timer', [step({},.05),
  step({troop:{x:47,z:47}},.25),step({},.25),
  step({troop:{x:48,z:48}}),step({troop:{x:64,z:64}}),
  step({troop:{x:48,z:48}}),step({troop:{x:47,z:47}})]);
trace('occupied-passage-and-eligibility', [step(),
  step({troop:{x:41,z:31},friend:{x:31,z:31}}),
  step({friend:{type:'tank',x:34.1,z:31}}),step({friend:{x:40,z:31}}),step({troop:{hp:0}}),
  step({troop:{hp:100,riding:7}}),step({troop:{riding:0,air:{state:'out'}}}),
  step({troop:{air:null,type:'hq'}}),step({troop:{type:'rifle',owner:0}}),
  step({troop:{owner:1}}),step({troop:{x:90,z:90}})]);
trace('twenty-hertz-clock', Array.from({length:35}, (_,i) => step(i === 1 ? {troop:{x:41,z:31}} : {}, .05)));
// Diagnostic of the prototype's passable terrain still being blocked by Gate bodies.
// Native passage tests retain this terrain/body distinction; this is not a whole-match comparison.
function passage(withGates) {
  const g = make();
  const unit = sim.internals.spawnUnit(g,0,'rifle');
  Object.assign(unit,{x:30,z:27,autoRetreat:false,holdFire:true,holdPos:false});
  for (let x=0;x<60;x++) if (x<14 || x>16) sim.internals.setCell(g,15*60+x,'K');
  const gates = [];
  if (withGates) for (let x=14;x<=16;x++) gates.push(sim.internals.placeBuilding(g,0,'gate',15*60+x,true));
  rebuildGrid(g);
  sim.step(g);
  const denial = sim.command(g,0,{t:'move',orders:[[unit.id,30,37]]}) ?? null;
  for (let i=0;i<160;i++) sim.step(g);
  return {withGates,denial,steps:160,x:bits(unit.x),z:bits(unit.z),
    gates:gates.map(gate=>({open:gate.open,ch:g.chars[gate.cells[0]]}))};
}
const passageRuns = [passage(false),passage(true)];
assert.equal(passageRuns[0].z,bits(37));
assert.equal(passageRuns[1].z,bits(29.24890270493851));
assert(passageRuns[1].gates.every(gate=>gate.open && gate.ch === '.'));
for (const surface of ['D','+','M']) trace(`surface:${surface}`, [step(),step({troop:{x:41,z:31}}),step({troop:{x:90,z:90}})], surface);
const fixture = {reference:pin,TIER,gate:sim.FORTS.gate,placements,traces,passageRuns};
const output = JSON.stringify(fixture) + '\n';
const file = `${root}/rust/crates/core/tests/fixtures/gate-lifecycle.json`;
clean();
if (process.argv.includes('--check')) assert.equal(readFileSync(file,'utf8'),output);
else writeFileSync(file,output);
console.log(`${placements.length} placements, ${traces.reduce((n,t)=>n+t.frames.length,0)} Gate steps`);
