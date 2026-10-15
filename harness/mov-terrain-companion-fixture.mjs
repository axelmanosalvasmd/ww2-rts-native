// Disclosed canonical companion. Do not execute without Root's recorder grant.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync, writeFileSync} from 'node:fs';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {gzipSync} from 'node:zlib';
import {readOriginalCapture} from './mov-original-capture-stream.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const ref = process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`;
const manifest = JSON.parse(readFileSync(`${root}/harness/mov-terrain-scenes.json`, 'utf8'));
const pin = manifest.reference;
assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
const args = process.argv.slice(2), checking = args.includes('--check');
const paths = args.filter(a => a !== '--check');
assert.equal(paths.length, 2, 'separate original capture and /tmp output paths required');
assert.equal(args.length, 2 + Number(checking));
const original = await readOriginalCapture(paths[0]);
assert.equal(original.reference, pin);
assert.equal(original.failure, undefined, 'retain and report the first original failure before companion recording');
const originalChecks = ['Roads and mud', 'Ground that wears', 'Slope', 'Wind, weather, dust and fire'].map(name => {
  const check = original.cases.find(c => c.name === name);
  assert.equal(check?.completed, true, `complete original callback: ${name}`);
  return {name, sha256: check.sha256, completed: check.completed};
});
const target = paths[1];
assert.ok(target.startsWith('/tmp/'));
const clean = () => {
  assert.equal(execFileSync('git', ['-C', ref, 'rev-parse', 'HEAD'], {encoding: 'utf8'}).trim(), pin);
  assert.equal(execFileSync('git', ['-C', ref, 'status', '--porcelain', '--untracked-files=no'], {encoding: 'utf8'}).trim(), '');
};
clean();
const sim = await import(pathToFileURL(`${ref}/shared/sim.js`));
const bytes = new DataView(new ArrayBuffer(8));
const bits = n => { bytes.setFloat64(0, n, false); return bytes.getBigUint64(0, false).toString(16).padStart(16, '0'); };
const point = p => [bits(p.x), bits(p.z)];
const roster = g => [...g.units.values()].map(u => ({id: u.id, kind: u.type, owner: u.owner,
  cells: u.cells ?? null, at: sim.UNITS[u.type].structure ? point(u) : null}));
function sceneMap(spec) {
  const {w, h, spawns, points} = manifest.map;
  const rows = Array.from({length: h}, (_, y) =>
    y < 20 && spec.surface === 'mud' ? 'M'.repeat(w) :
      y >= 1 && y <= 3 && spec.surface === 'road' ? 'D'.repeat(w) : '.'.repeat(w));
  const heights = Array.from({length: h}, (_, y) =>
    y < 20 && spec.surface === 'hill' ? '0'.repeat(10) + '1'.repeat(10) + '0'.repeat(w - 20) : '0'.repeat(w));
  return {name: `Canonical MOV companion ${spec.name}`, w, h, rows, heights, spawns, points, weather: 'clear'};
}
function refresh(g) {
  // Both are real existing providers. This explicit read boundary follows authored placement.
  sim.snapshotCache(g);
  sim.internals.updateVision(g);
}
function frame(g, aliases) {
  return {tick: g.tick, units: Object.fromEntries(Object.entries(aliases).map(([name, id]) => {
    const u = g.units.get(id);
    assert.ok(u, `${name}: actual purchased actor remains present`);
    return [name, {id, x: bits(u.x), z: bits(u.z), rot: bits(u.rot), speed: bits(u.moveSpeed),
      path: u.path.map(point), vx: bits(u.vx), vz: bits(u.vz)}];
  }))};
}
const packet = {reference: pin, node: process.version, arch: process.arch, manifest,
  originalCapture: {sha256: original.sha256Compressed, checks: originalChecks},
  scope: 'Canonical public construction/Buy/Move companion. Original cleared fixtures are captured separately, unchanged.',
  random: 'Global Math.random fixed0.5; native MatchRandom must never be replaced', cases: [], completed: false};
let active;
function trace(spec) {
  active = {name: spec.name, boundary: 'canonical constructor', completed: false};
  const map = sceneMap(spec), s = manifest.startup;
  const g = sim.createGame(map, s.names, false, s.teams, s.factions,
    {mode: 'conquest', army: s.army, weather: s.weather, weatherSeed: s.weatherSeed,
      tech: s.tech, supply: s.supply, logistics: s.logistics});
  const aliases = {}, retained = [...g.units.values()].map(u => ({u, cells: structuredClone(u.cells), structureId: u.structureId}));
  active = {name: spec.name, map, initial: roster(g), commands: [], steps: [], measurements: {}, completed: false};
  packet.cases.push(active);
  const preserve = () => {
    for (const before of retained) {
      assert.equal(g.units.get(before.u.id), before.u, 'constructor actor identity retained');
      assert.deepEqual(before.u.cells, before.cells, 'constructor footprint identity retained');
      if (before.structureId) assert.ok(g.structures.has(before.structureId), 'constructor structure retained');
    }
  };
  const command = cmd => {
    const before = new Set(g.units.keys()), mp = g.players[0].mp;
    const result = sim.command(g, 0, cmd);
    active.commands.push({command: structuredClone(cmd), result: result ?? null,
      births: [...g.units.keys()].filter(id => !before.has(id)), debit: bits(mp - g.players[0].mp),
      spent: bits(g.story?.[0]?.mpSpent ?? 0)});
    assert.equal(result, undefined, `${spec.name}: real command ${cmd.t}`);
    preserve();
    return active.commands.at(-1);
  };
  const ticks = (phase, count) => {
    for (let i = 0; i < count; i++) {
      sim.step(g);
      preserve();
      active.steps.push({phase, ...frame(g, aliases)});
    }
  };
  g.seed = s.environmentSeed; g.wind = {a: s.wind[0], v: s.wind[1]};
  Object.assign(g.wx, {rain: 0, raining: false, wet: 0, next: 1e9});
  g.players[0].mp = manifest.producer.wallet;
  assert.ok(sim.builderTypes(sim.isConstructionMode(g)).includes(manifest.producer.builderKind));
  const builder = [...g.units.values()].find(u => u.owner === 0 && u.type === manifest.producer.builderKind);
  assert.ok(builder, 'real constructor builder admitted by Source builder policy');
  active.builder = builder.id;
  const built = command({t: 'build', ids: [builder.id], kind: manifest.producer.kind,
    x: manifest.producer.at[0], z: manifest.producer.at[1]});
  assert.equal(built.births.length, 1, 'real construction publishes one producer');
  const maker = g.units.get(built.births[0]);
  assert.equal(maker.type, 'motorpool');
  let constructionTicks = 0;
  while (maker.built < 1 && constructionTicks < manifest.producer.limitTicks) {
    ticks('construction', 1); constructionTicks++;
  }
  assert.ok(maker.built >= 1, 'actual builder completes the Motor Pool before the cap');
  active.constructionTicks = constructionTicks;
  command({t: 'stop', ids: [builder.id]});
  active.producer = roster(g).find(row => row.id === maker.id);
  if (spec.wear !== undefined) {
    // Only the measurement band is an authored input. Every base footprint remains untouched.
    g.wear.fill(spec.wear, 0, manifest.map.w * 20);
  }
  Object.assign(g.wx, {rain: 0, raining: false, wet: 0, next: 1e9});
  if (spec.name === 'wet-ground') {
    g.wind = {a: 0, v: 1}; g.smokes.push({x: 10, z: 10, r: 5, t: 60}); ticks('smoke', 100);
    assert.ok(g.smokes[0].x > 14 && Math.abs(g.smokes[0].z - 10) < 2, 'smoke drifts downwind');
  }
  for (const actor of spec.actors) {
    g.players[0].mp = manifest.producer.wallet;
    const from = actor.kind === 'tank' ? maker : [...g.units.values()].find(u => u.owner === 0 && u.type === manifest.producer.rifleMaker);
    assert.ok(from, 'retained real producer');
    const buy = command({t: 'buy', unit: actor.kind, from: from.id});
    assert.equal(buy.births.length, 1, 'public Buy publishes one actor');
    const u = g.units.get(buy.births[0]); aliases[actor.alias] = u.id;
    assert.equal(u.type, actor.kind);
    Object.assign(u, {x: actor.at[0], z: actor.at[1], rot: actor.rot, aim: 0, autoRetreat: false});
  }
  refresh(g);
  const orders = spec.actors.filter(a => a.goal).map(a => [aliases[a.alias], ...a.goal]);
  command({t: 'move', orders});
  for (const a of spec.actors.filter(a => a.straight)) g.units.get(aliases[a.alias]).path = [{x: a.goal[0], z: a.goal[1]}];
  ticks('warmup', spec.warmup);
  const starts = Object.fromEntries(Object.entries(aliases).map(([name, id]) => [name, g.units.get(id).x]));
  ticks('measure', spec.measure);
  active.measurements = Object.fromEntries(Object.entries(aliases).map(([name, id]) => {
    const u = g.units.get(id); return [name, {distance: u.x - starts[name], x: u.x, z: u.z}];
  }));
  if (spec.rainTicks) {
    const u = g.units.get(aliases.subject);
    const flags = () => sim.snapshotFor(g, 0, []).units.find(row => row[0] === u.id)[12];
    assert.ok(flags() & 32768, 'a tank moving on dry ground trails dust');
    g.wx.next = 0; ticks('rain', spec.rainTicks);
    assert.equal(g.wx.rain, 1, 'the rain sets in');
    assert.ok(g.wx.wet > 0.15 && g.wx.wet < 0.5, 'and the ground soaks slowly');
    assert.deepEqual(sim.snapshotFor(g, 0, []).wx.slice(0, 1), [1], 'clients are told the weather');
    g.wx.wet = 1; g.wx.next = 1e9;
    Object.assign(u, {x: 3, z: 25, rot: 0, moveSpeed: 0, path: []});
    refresh(g); command({t: 'move', orders: [[u.id, 37, 25]]});
    ticks('wet-warmup', 40); const start = u.x; ticks('wet-measure', 20);
    active.measurements.wet = {distance: u.x - start, x: u.x, z: u.z};
    assert.ok(!(flags() & 32768), 'no dust in the wet');
  }
  active.finalRoster = roster(g);
  active.completed = true;
  return active;
}
let failure;
const previousRandom = Math.random;
try {
  Math.random = () => 0.5;
  for (const spec of manifest.scenes) trace(spec);
  const row = name => packet.cases.find(c => c.name === name).measurements;
  const travel = name => row(name).subject.distance;
  const grass = travel('grass'), road = travel('road'), edge = travel('road-edge');
  assert.ok(Math.abs(road / grass - sim.CFG.roadSpeed) < 0.1, 'a tank on a road drives roadSpeed times faster');
  assert.ok(edge / grass > 1.03 && edge / grass < sim.CFG.roadSpeed - 0.1, 'a tank half on the road gets part of the bonus');
  assert.ok(Math.abs(travel('shallow-mud') / grass - sim.CFG.mudSpeed[0]) < 0.1, 'a tank in shallow mud');
  assert.ok(Math.abs(travel('deep-mud') / grass - sim.CFG.mudSpeed[1]) < 0.1, 'a tank in deep mud');
  assert.ok(Math.abs(travel('rifle-mud') - travel('rifle-grass')) < 0.2, 'infantry walk through mud at full speed');
  assert.ok(Math.abs(travel('half-churn') / travel('churn-reference') - (1 - sim.CFG.churn * 0.5)) < 0.06, 'half-churned ground costs half the churn at steady speed');
  assert.ok(Math.abs(row('wet-ground').wet.distance / travel('wet-ground') - (1 - sim.CFG.weather.wetGround)) < 0.06, 'wet ground slows a tank at steady speed');
  assert.ok(row('slope-hill').up.x < row('slope-flat').up.x - 0.5, 'a tank climbs slower than it drives on the flat');
  assert.ok(Math.abs(row('slope-hill').down.x - row('slope-flat').down.x) < 0.3, 'and loses nothing going down');
  packet.completed = true;
} catch (error) {
  failure = error;
  packet.failure = {case: active?.name, name: error.name, message: error.message, stack: error.stack};
} finally {
  Math.random = previousRandom;
  try { clean(); } catch (error) { packet.readbackFailure = {message: error.message, stack: error.stack}; failure ??= error; }
  const compressed = gzipSync(JSON.stringify(packet) + '\n', {level: 9});
  if (failure) writeFileSync(checking ? `${target}.failed.gz` : target, compressed);
  else if (checking) {
    if (!readFileSync(target).equals(compressed)) writeFileSync(`${target}.different.gz`, compressed);
    assert.deepEqual(readFileSync(target), compressed, 'canonical companion recording is reproducible');
  }
  else writeFileSync(target, compressed);
}
if (failure) throw failure;
console.log(`${packet.cases.length} canonical companion scenes; original cleared captures and native proof remain separate`);
