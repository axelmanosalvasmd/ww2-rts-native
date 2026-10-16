// Execute complete original acceptance bodies. Requires a future Root Source slot.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { gzipSync } from 'node:zlib';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const input = JSON.parse(readFileSync(`${root}/rust/crates/simtest/tests/fixtures/engineering-support-selected-input.json`, 'utf8'));
assert.equal(input.reference, pin);
const simURL = pathToFileURL(`${reference}/shared/sim.js`).href;
const sim = await import(simURL);
const fixtures = await import(pathToFileURL(`${reference}/test-fixtures.js`));
const { DEBRIS_LIMITS } = await import(pathToFileURL(`${reference}/shared/debris-motion.js`));
const original = readFileSync(`${reference}/test.js`, 'utf8');
const copy = value => JSON.parse(JSON.stringify(value));
const bitBuffer = Buffer.alloc(8);
const bits = value => { bitBuffer.writeDoubleBE(value); return bitBuffer.toString('hex'); };
const f32 = value => { const b = Buffer.alloc(4); b.writeFloatBE(value); return b.readUInt32BE(); };
function exact(raw) {
  const floatBits = {};
  function visit(value, path = '') {
    if (typeof value === 'number') {
      assert.ok(Number.isFinite(value), path);
      floatBits[path] = bits(value);
    } else if (value && typeof value === 'object') {
      for (const [key, child] of Object.entries(value)) visit(child, path ? `${path}.${key}` : key);
    }
  }
  visit(raw);
  return { raw, floatBits };
}
// Existing native OffMap purpose sampler, already used by ground-support fixtures.
const mask = (1n << 64n) - 1n, gamma = 0x9e3779b97f4a7c15n;
const mix = x => {
  x = ((x ^ (x >> 30n)) * 0xbf58476d1ce4e5b9n) & mask;
  x = ((x ^ (x >> 27n)) * 0x94d049bb133111ebn) & mask;
  return x ^ (x >> 31n);
};
function random() {
  let state = mix(BigInt(input.nativeSeed) ^ ((4n * gamma) & mask)), count = 0;
  return {
    next() { state = (state + gamma) & mask; count++; return Number(mix(state) >> 11n) / 9007199254740992; },
    state: () => count ? state.toString(16).padStart(16, '0') : null,
    count: () => count,
  };
}
const games = new WeakMap(), scenes = [], checks = [];
let checkName, sceneIndex;
function createGame(...args) {
  const prior = Math.random;
  Math.random = () => input.startupRandom;
  let g;
  try { g = sim.createGame(...args); } finally { Math.random = prior; }
  const scene = { name: `${checkName}:${sceneIndex++}`, map: copy(args[0]), actions: [], runs: [], queries: [], frames: [], contactCollapses: [], drawSites: new Map(), rng: random() };
  games.set(g, scene);
  scenes.push(scene);
  return g;
}
const blank = rows => ({ ...copy(input.map), w: rows[0].length, h: rows.length, rows });
const empty = Array(20).fill('.'.repeat(20));
const fresh = (rows = empty, n = 2) => {
  const g = createGame(blank(rows), ['a', 'b', 'c'].slice(0, n), false);
  fixtures.clearFixtureUnits(g);
  g.players.forEach(p => { p.spawn = copy(input.spawnSentinel); });
  return g;
};
function projection(g) {
  return {
    tick: g.tick,
    environment: { tick: g.tick, weather: copy(g.weather), wx: copy(g.wx), seed: g.seed >>> 0, wind: copy(g.wind) },
    chars: g.chars.join(''), ground: g.ground.join(''), objects: g.objects.join(''),
    height: g.height ? [...g.height] : null,
    scar32: [...g.scar].map(f32), wear32: [...g.wear].map(f32), hp32: [...g.cellHp].map(f32), state: [...g.cellState],
    units: [...g.units.values()].map(u => [u.id, u.type, u.owner, u.x, u.z, u.hp, u.supp]),
    strikes: g.strikes.map(s => [s.kind, s.owner, s.x, s.z, s.dir, s.t, s.left, s.next, !!s.live]),
    negativeLog: g.cellLog.some(row => row[2] < 0),
    offMapState: games.get(g).rng.state(),
  };
}
function capture(g) {
  const scene = games.get(g);
  if (!scene.setup) {
    scene.setup = {
      teams: g.players.map(p => p.team), factions: g.players.map(p => p.faction),
      players: g.players.map(p => ({ mp: p.mp, spawn: copy(p.spawn) })), nextId: g.nextId,
      environment: exact({ tick: g.tick, weather: copy(g.weather), wx: copy(g.wx), seed: g.seed >>> 0, wind: copy(g.wind) }),
      actors: [...g.units.values()].map(u => ({ id: u.id, type: u.type, owner: u.owner, x: u.x, z: u.z,
        rot: u.rot, aim: u.aim, autoRetreat: u.autoRetreat, air: u.air ? copy(u.air) : null,
        built: u.built ?? null, cells: copy(u.cells ?? []), structureId: u.structureId ?? null })),
    };
    scene.initial = exact(projection(g));
  }
  return scene;
}
function command(g, owner, request) {
  const scene = capture(g);
  const resetCooldown = request.t === 'support' && scene.actions.some(a => a.request?.t === 'support');
  if (resetCooldown) assert.equal(g.players[owner].sup[request.kind], 0, 'original loop reset its support cooldown');
  const action = { tick: g.tick, owner, request: copy(request), resetCooldown };
  const denial = fixtures.fixtureCommand(g, owner, request);
  action.denial = denial ?? null;
  scene.actions.push(action);
  return denial;
}
function put(g, owner, type, x, z) {
  const scene = games.get(g);
  const action = scene.setup ? { tick: g.tick, kind: 'put', owner, type, x, z, rot: 0, aim: 0, autoRetreat: false } : null;
  assert.equal(fixtures.fixtureCommand(g, owner, { t: 'buy', unit: type }), undefined);
  const u = [...g.units.values()].at(-1);
  Object.assign(u, { x, z, rot: 0, aim: 0, autoRetreat: false });
  if (action) { action.id = u.id; scene.actions.push(action); }
  return u;
}
function run(g, secs) {
  const scene = capture(g), before = g.tick, prior = Math.random;
  // The original hedge callback pins only its first artillery run to this function.
  const originalConstant = scene.name === 'test.js:701:0' && before === 0;
  if (originalConstant) assert.equal(secs, sim.SUPPORT.artillery.delay + 5);
  Math.random = () => {
    const stack = new Error().stack;
    assert.ok(stack.includes('stepStrikes'), `${scene.name}: unexpected random purpose ${stack}`);
    const site = stack.split('\n').find(line => line.includes(`${simURL}:`));
    assert.ok(site);
    scene.drawSites.set(site.trim(), (scene.drawSites.get(site.trim()) ?? 0) + 1);
    if (originalConstant) {
      const value = prior(), index = scene.originalConstantDraws ?? 0;
      assert.equal(value, 0.5, 'original hedge callback constant');
      assert.ok(index < 20 && site.includes('shared/sim.js:5287:'), 'original artillery call site/count');
      scene.originalConstantTape ??= [];
      scene.originalConstantTape.push({ index, tick: g.tick, axis: index % 2 === 0 ? 'along' : 'side',
        bits: bits(value), site: site.trim().replace(reference, 'reference/ww2-rts') });
      scene.originalConstantDraws = index + 1;
      return value;
    }
    return scene.rng.next();
  };
  try {
    for (let i = 0; i < secs * 20; i++) {
      const before = g.shots.length;
      sim.step(g);
      const shots = g.shots.slice(before).filter(s => s.k === 'collapse' && s.section === undefined && s.id !== undefined);
      if (shots.length) scene.contactCollapses.push(exact({ tick: g.tick, shots }));
      scene.frames.push(exact(projection(g)));
    }
  } finally { Math.random = prior; }
  scene.runs.push({ before, seconds: secs, after: g.tick });
}
function findPath(g, from, to) {
  const scene = capture(g), path = sim.findPath(g, from, to);
  scene.queries.push({ tick: g.tick, from: copy(from), to: copy(to), path: exact(path) });
  return path;
}
const exposed = { ...sim, damageCells(g, ...args) {
  const scene = capture(g);
  scene.componentInput = copy(args);
  const result = sim.damageCells(g, ...args);
  scene.componentFinal = exact(projection(g));
  return result;
} };
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const prior = Math.random;
try {
  Math.random = () => input.startupRandom;
  for (const name of input.checks) {
    checkName = name; sceneIndex = 0;
    const sourceName = name === 'Fortifications wire' ? 'Fortifications' : name;
    const marker = `await check(${JSON.stringify(sourceName)}, async () => {`;
    assert.equal(original.split(marker).length, 2, `one original check ${sourceName}`);
    const start = original.indexOf(marker) + marker.length;
    const end = original.indexOf('\n});', start);
    assert.ok(end > start);
    let body = original.slice(start, end);
    if (name === 'Fortifications wire') {
      const begin = body.indexOf('  // wire: infantry crawls through it, a tank flattens it');
      const stop = body.indexOf('  // tank traps:', begin);
      assert.ok(begin >= 0 && stop > begin);
      const at = body.slice(body.indexOf('  const at ='), body.indexOf('\n', body.indexOf('  const at =')));
      body = at + '\n' + body.slice(begin, stop);
    }
    const helperStart = original.indexOf('const settleDebris =');
    const helperEnd = original.indexOf('\n};', helperStart) + 3;
    assert.ok(helperStart >= 0 && helperEnd > helperStart);
    const helpers = new Function('assert', 'DEBRIS_LIMITS', 'sim', 'step', original.slice(helperStart, helperEnd) + '\nreturn {settleDebris, settledRubble};')(
      assert, DEBRIS_LIMITS, sim, g => run(g, sim.TICK));
    await new AsyncFunction('assert', 'sim', 'SUPPORT', 'UNITS', 'CFG', 'CELL', 'fresh', 'empty', 'command', 'run', 'put', 'blank', 'createGame', 'clearFixtureUnits', 'findPath', 'settleDebris', 'settledRubble', body)(
      assert, exposed, sim.SUPPORT, sim.UNITS, sim.CFG, sim.CELL, fresh, empty, command, run, put, blank, createGame, fixtures.clearFixtureUnits, findPath, helpers.settleDebris, helpers.settledRubble);
    checks.push({ name, scope: name === 'Fortifications wire' ? 'complete original wire block' : 'complete original callback', bodySha256: createHash('sha256').update(body).digest('hex'), passed: true });
  }
} finally { Math.random = prior; }
assert.equal(scenes.length, 7);
for (const scene of scenes) {
  assert.ok(scene.setup && scene.initial);
  scene.final = scene.frames.at(-1) ?? scene.componentFinal ?? scene.initial;
  scene.nativeSamplerDraws = scene.rng.count();
  scene.draws = scene.nativeSamplerDraws + (scene.originalConstantDraws ?? 0);
  scene.drawSites = [...scene.drawSites].map(([site, count]) => ({ site: site.replace(reference, 'reference/ww2-rts'), count }));
  assert.equal(scene.drawSites.reduce((n, row) => n + row.count, 0), scene.draws);
  delete scene.rng;
}
for (const name of input.nativeScenes) assert.equal(scenes.filter(scene => scene.name === name).length, 1);
clean();
const bytes = gzipSync(JSON.stringify({ reference: pin, node: process.version, arch: process.arch, input, checks, scenes }) + '\n', { level: 9 });
const output = `${root}/rust/crates/simtest/tests/fixtures/engineering-support-selected.json.gz`;
// Reversible expected-value deltas avoid a test compression dependency in the rules core.
function differences(a, b, path = [], out = []) {
  if (Object.is(a, b)) return out;
  if (Array.isArray(a) && Array.isArray(b) && a.length === b.length) {
    for (let i = 0; i < a.length; i++) differences(a[i], b[i], [...path, i], out);
  } else if (a && b && typeof a === 'object' && typeof b === 'object' &&
      !Array.isArray(a) && !Array.isArray(b) && Object.keys(a).length === Object.keys(b).length &&
      Object.keys(a).every(key => Object.hasOwn(b, key))) {
    for (const key of Object.keys(a)) differences(a[key], b[key], [...path, key], out);
  } else out.push([path, b]);
  return out;
}
const hedge = scenes.find(scene => scene.name === 'test.js:701:0');
assert.equal(hedge.originalConstantDraws, 20);
assert.equal(hedge.nativeSamplerDraws, 0);
const { frames, ...hedgeScene } = hedge;
let previous = hedge.initial;
hedgeScene.frameChanges = frames.map(frame => { const diff = differences(previous, frame); previous = frame; return diff; });
let rebuilt = structuredClone(hedge.initial);
for (let i = 0; i < frames.length; i++) {
  for (const [path, value] of hedgeScene.frameChanges[i]) {
    if (!path.length) { rebuilt = structuredClone(value); continue; }
    let parent = rebuilt;
    for (const key of path.slice(0, -1)) { assert.ok(Object.hasOwn(parent, key)); parent = parent[key]; }
    assert.ok(Object.hasOwn(parent, path.at(-1)));
    parent[path.at(-1)] = structuredClone(value);
  }
  assert.deepEqual(rebuilt, frames[i], `entire original hedge frame ${i + 1} survives delta encoding`);
}
const coreBytes = Buffer.from(JSON.stringify({ format: 'engineering-hedge-original-v1', reference: pin,
  node: process.version, arch: process.arch, input, check: checks.find(check => check.name === 'test.js:701'), scene: hedgeScene }) + '\n');
const coreOutput = `${root}/rust/crates/core/tests/fixtures/engineering-hedge-original.json`;
if (process.argv.includes('--check')) assert.ok(readFileSync(output).equals(bytes), 'authentic capture bytes unchanged');
else writeFileSync(output, bytes);
if (process.argv.includes('--check')) assert.ok(readFileSync(coreOutput).equals(coreBytes), 'original core hedge capture bytes unchanged');
else writeFileSync(coreOutput, coreBytes);
console.error(`${process.argv.includes('--check') ? 'Verified' : 'Recorded'} ${checks.length} original acceptance selections, ${scenes.length} scenes, ${scenes.reduce((n, scene) => n + scene.frames.length, 0)} full Source ticks`);
