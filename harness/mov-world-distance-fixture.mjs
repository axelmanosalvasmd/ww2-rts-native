// Original World public movement plus an explicit physical calm-boundary companion.
// Execute only after Root grants a recorder slot. No Source bodies are patched.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync, writeFileSync} from 'node:fs';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {gzipSync} from 'node:zlib';
import {readOriginalCapture} from './mov-original-capture-stream.mjs';
const root = fileURLToPath(new URL('../', import.meta.url));
const ref = process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`;
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
assert.equal(process.version, 'v24.21.0'); assert.equal(process.arch, 'x64');
const args = process.argv.slice(2), checking = args.includes('--check');
const paths = args.filter(a => a !== '--check');
assert.equal(paths.length, 2); assert.equal(args.length, 2 + Number(checking));
const [originalPath, target] = paths; assert.ok(target.startsWith('/tmp/'));
const original = await readOriginalCapture(originalPath);
assert.equal(original.reference, pin); assert.equal(original.failure, undefined);
const originalCheck = original.cases.find(c => c.name === 'road march: calm units move faster on their own land');
assert.equal(originalCheck?.completed, true, 'retain the complete original World callback separately');
const clean = () => {
  assert.equal(execFileSync('git', ['-C', ref, 'rev-parse', 'HEAD'], {encoding: 'utf8'}).trim(), pin);
  assert.equal(execFileSync('git', ['-C', ref, 'status', '--porcelain', '--untracked-files=no'], {encoding: 'utf8'}).trim(), '');
};
clean();
const sim = await import(pathToFileURL(`${ref}/shared/sim.js`));
const {generateWorldMap} = await import(pathToFileURL(`${ref}/shared/world-conquest.js`));
const bytes = new DataView(new ArrayBuffer(8));
const bits = n => { bytes.setFloat64(0, n, false); return bytes.getBigUint64(0, false).toString(16).padStart(16, '0'); };
const runs = values => { const out = []; for (const value of values) { if (out.at(-1)?.[0] === value) out.at(-1)[1]++; else out.push([value, 1]); } return out; };
const map = generateWorldMap({seed: 42, players: 2, teams: [0, 1]});
const recordedMap = structuredClone(map);
if (recordedMap.world.regionMap) {
  recordedMap.world.regionMapRuns = runs(recordedMap.world.regionMap);
  delete recordedMap.world.regionMap;
}
const packet = {reference: pin, node: process.version, arch: process.arch, map: recordedMap,
  input: {nativeSeed: 29448, mapSeed: 42, names: ['p0', 'p1'], teams: [0, 1], factions: [0, 1], weather: 'clear'},
  random: 'Source global Math.random fixed0.5; native MatchRandom stays native and independent',
  originalCapture: {sha256: original.sha256Compressed,
    name: originalCheck.name, sha256Callback: originalCheck.sha256, completed: true},
  cases: [], completed: false};
let active;
function trace(name, boundary) {
  active = {name, boundary: 'authentic World constructor', completed: false};
  const g = sim.createGame(map, ['p0', 'p1'], false, [0, 1], [0, 1], {mode: 'world', weather: 'clear'});
  const rifle = [...g.units.values()].find(u => u.owner === 0 && u.type === 'rifle');
  assert.ok(rifle, 'authentic constructor rifle');
  const start = {x: rifle.x, z: rifle.z};
  active = {name, subject: rifle.id, start, initial: [...g.units.values()].map(u =>
    ({id: u.id, owner: u.owner, kind: u.type, cells: u.cells ?? null})), steps: [], commands: [], distances: {}, completed: false};
  packet.cases.push(active);
  const state = () => ({tick: g.tick, id: rifle.id, x: bits(rifle.x), z: bits(rifle.z),
    rot: bits(rifle.rot), path: rifle.path.map(p => [bits(p.x), bits(p.z)])});
  const step = (phase, fought = false) => {
    if (fought) rifle.hitAt = g.tick; // Exact original leg input, not generated hurt output.
    const before = {x: rifle.x, z: rifle.z};
    const row = {phase, fought, before: state()}; active.steps.push(row);
    sim.step(g); row.after = state();
    return Math.hypot(rifle.x - before.x, rifle.z - before.z);
  };
  const move = () => {
    Object.assign(rifle, {...start, path: [], orders: [], worldGoal: null});
    const command = {t: 'move', orders: [[rifle.id, start.x + 20, start.z]]};
    const result = sim.command(g, 0, command);
    active.commands.push({tick: g.tick, command, result: result ?? null});
    assert.equal(result, undefined, 'actual public World Move admission');
  };
  if (boundary) {
    // Separate authored last-combat inputs expose the strict >200 rule physically.
    rifle.hitAt = 0; rifle.shotAt = 0;
    for (let i = 0; i < 199; i++) step('boundary-warmup');
    move(); const normal = step('tick-200');
    move(); const marching = step('tick-201');
    assert.ok(normal > 0, 'boundary control actually moves');
    // New diagnostic bound for coordinate subtraction, not a replacement for the original >1.3 assertion.
    assert.ok(Math.abs(marching / normal - sim.CFG.march.speed) < 1e-10, 'physical march ratio at201 is1.5 after the nonmarch tick200');
    active.distances = {normal, marching};
  } else {
    for (let i = 0; i < 300; i++) step('warmup');
    const leg = fought => { move(); let d = 0; for (let i = 0; i < 40; i++) d += step(fought ? 'fighting' : 'marching', fought); return d; };
    const marching = leg(false), fighting = leg(true);
    assert.ok(marching > fighting * 1.3, `march ${marching.toFixed(1)} m against ${fighting.toFixed(1)} m under fire`);
    active.distances = {marching, fighting};
  }
  active.completed = true;
}
let failure; const previousRandom = Math.random;
try {
  Math.random = () => 0.5;
  trace('original-public-300-plus-40-plus-40', false);
  trace('physical-calm-ticks-200-and-201', true);
  packet.completed = true;
} catch (error) {
  failure = error;
  packet.failure = {case: active?.name, name: error.name, message: error.message, stack: error.stack};
} finally {
  Math.random = previousRandom;
  try { clean(); } catch (error) { packet.readbackFailure = {message: error.message}; failure ??= error; }
  const compressed = gzipSync(JSON.stringify(packet) + '\n', {level: 9});
  if (failure) writeFileSync(checking ? `${target}.failed.gz` : target, compressed);
  else if (checking) {
    if (!readFileSync(target).equals(compressed)) writeFileSync(`${target}.different.gz`, compressed);
    assert.deepEqual(readFileSync(target), compressed, 'World physical recording is reproducible');
  }
  else writeFileSync(target, compressed);
}
if (failure) throw failure;
console.log('Two authentic World physical branches,581 full ticks; native proof remains pending');
