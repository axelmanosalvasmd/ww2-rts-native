// Original lab verdicts over canonical Source births and ordinary ground orders.
// Run only in Root's recorder slot. No Source function body is replaced.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const args = process.argv.slice(2);
const extraAt = args.indexOf('--recovery-output');
const extra = extraAt >= 0;
const referenceAt = args.indexOf('--reference');
const reference = resolve(referenceAt < 0 ? resolve(root, 'reference/ww2-rts') : args[referenceAt + 1]);
const extraTarget = extra ? resolve(args[extraAt + 1]) : null;
const checking = args.includes('--check');
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
if (extra) {
  assert(args[extraAt + 1] && args[extraAt + 1] !== '--check');
  const expected = ['--recovery-output', args[extraAt + 1], ...(referenceAt < 0 ? [] : ['--reference', args[referenceAt + 1]]), ...(checking ? ['--check'] : [])];
  assert.deepEqual(args, expected, 'explicit private recovery selection only');
  assert.equal(execFileSync('git', ['-C', root, 'status', '--porcelain'], { encoding: 'utf8' }).trim(), '', 'freeze the child first');
} else assert.ok(args.every(arg => arg === '--check'));
clean();
const source = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const motion = await import(pathToFileURL(resolve(reference, 'shared/vehicle-motion.js')));
const { MOVEMENT_TYPES } = await import(pathToFileURL(resolve(reference, 'tools/movement-lab-scenarios.js')));
const { runMovementCase } = await import(pathToFileURL(resolve(reference, 'tools/movement-lab-runner.js')));
assert.deepEqual(MOVEMENT_TYPES, ['tank', 'medium', 'tankdestroyer', 'tiger', 'churchill',
  'armoredcar', 'halftrack', 'flaktrack', 'rocket', 'rifle', 'at']);
const scenarios = extra ? ['wall-arrival', 'obstacle', 'wreck', 'closure']
  : ['clearance', 'corner', 'cliff', 'opposing', 'parked', 'road', 'queue'];
const buffer = new DataView(new ArrayBuffer(8));
const bits = value => {
  buffer.setFloat64(0, value);
  return buffer.getBigUint64(0).toString(16).padStart(16, '0');
};
const pt = p => p ? [bits(p.x), bits(p.z)] : null;
const raw = value => value === undefined ? { absent: true }
  : typeof value === 'number' ? { bits: bits(value), value: Number.isFinite(value) ? value : String(value) }
  : value instanceof Map ? { map: [...value].map(([k,v]) => [raw(k),raw(v)]) }
  : value instanceof Set ? { set: [...value].map(raw) }
  : ArrayBuffer.isView(value) ? { typed: value.constructor.name, values: [...value].map(raw) }
  : Array.isArray(value) ? value.map(raw)
  : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([k,v]) => [k,raw(v)])) : value;
const snapshot = g => ({ tick: g.tick, units: [...g.units.values()].map(u => ({
  id: u.id, x: bits(u.x), z: bits(u.z), rot: bits(u.rot), speed: bits(u.moveSpeed ?? 0),
  path: u.path.map(pt), goal: pt(u.worldGoal), orders: u.orders.length,
  reverse: pt(u.reverse), clearance: !!u.motionClearance,
  outcome: u.moveOutcome ?? null,
  ...(extra && { kind: u.type, hp: bits(u.hp), routeEnd: pt(u.routeEnd) }),
})) });
let current;
let currentGame;
const cases = [];
let captureFailure = null;
if (extra) {
  process.on('uncaughtExceptionMonitor', error => { captureFailure = { message: String(error), stack: error.stack }; });
  process.on('exit', code => {
    const output = { format: 'mov24-27-original-lab-v1', reference: pin, node: process.version, arch: process.arch,
      sourceHead: execFileSync('git', ['-C', root, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(),
      types: MOVEMENT_TYPES, scenarios, cases, incomplete: current && !cases.includes(current) ? current : null,
      finalActualPrefix: currentGame ? { tick: currentGame.tick, nextId: currentGame.nextId,
        roster: raw([...currentGame.units.values()]), pathStats: raw(currentGame.pathStats) } : null,
      failure: captureFailure, exit: process.exitCode ?? code,
      scope: 'Existing canonical-birth lab adapter; original authored poses, events, commands, windows and all runner verdicts. No production-roster or whole-match equivalence claim.' };
    const text = JSON.stringify(output) + '\n';
    writeFileSync(checking ? extraTarget + '.check-actual.json' : extraTarget, text, { flag: 'wx', mode: 0o400 });
    if (checking) assert.equal(readFileSync(extraTarget, 'utf8'), text);
  });
}

function clearStartingForces(g) {
  for (const [id, row] of g.units) if (!source.UNITS[row.type].structure) g.units.delete(id);
}

// The original lab copies a rifle template into every type. At its insertion
// boundary, replace those defaults with a real spawnUnit of the declared type.
// Keep the object identity used by the lab's event closures. Only the lab's
// declared pose and hold controls survive; paths and timers come from spawnUnit.
function canonicalBirths(g) {
  const units = g.units, insert = units.set.bind(units);
  // Keep constructor bases with their actual footprint and production owners.
  // The original lab's empty-army setup otherwise leaves orphaned base cells.
  units.clear = () => clearStartingForces(g);
  let spawning = false;
  units.set = (id, row) => {
    if (spawning) return insert(id, row);
    assert.equal(row.id, id);
    assert.equal(id, g.nextId - 1);
    const declared = { id, x: row.x, z: row.z, rot: row.rot,
      holdFire: true, holdPos: true, auto: false, autoRetreat: false };
    const nextId = g.nextId;
    let born;
    spawning = true;
    try {
      g.nextId = id;
      born = source.internals.spawnUnit(g, row.owner, row.type);
    } finally {
      spawning = false;
      g.nextId = nextId;
    }
    for (const key of Object.keys(row)) delete row[key];
    Object.assign(row, born, declared);
    current.subject ??= id;
    return insert(id, row);
  };
}
const sim = {
  ...source,
  createGame(map, ...args) {
    // Native map parsing requires an objective. The original factory clears
    // objectives before stepping; record that same explicit isolation input.
    const binding = { ...map, points: [{ id: 'lab-binding', x: 4, y: 59 }] };
    assert.equal(source.validateMap(binding), null);
    const saved = Math.random;
    let g;
    try {
      Math.random = () => 0.5;
      g = source.createGame(binding, ...args);
    } finally { Math.random = saved; }
    current.map = binding;
    if (extra) {
      currentGame = g;
      current.constructorRoster = raw([...g.units.values()]);
      current.constructorStartup = raw({ seed: g.seed, nextId: g.nextId, wind: g.wind, players: g.players, army: g.army });
    }
    canonicalBirths(g);
    return g;
  },
  command(g, owner, command) {
    const input = structuredClone(command);
    const row = { tick: g.tick + 1, owner, sequence: current.commands.length + 1, command: input };
    if (extra) current.commands.push(row);
    const result = source.command(g, owner, command);
    if (extra) row.result = result === undefined ? { absent: true } : result;
    assert.equal(result, undefined, 'selected lab input must be accepted');
    if (!extra) current.commands.push(row);
    return result;
  },
  mutateWorldCell(g, cell, change) {
    if (extra) current.mutations.push({ tick: g.tick + 1, cell, change: structuredClone(change) });
    return source.mutateWorldCell(g, cell, change);
  },
  step(g) {
    if (!current.initial) {
      assert.equal(g.tick, 0);
      assert.equal(g.points.length, 0);
      assert.equal(g.mode ?? null, null);
      current.initial = [...g.units.values()].map(u => ({ id: u.id, kind: u.type, owner: u.owner,
        x: u.x, z: u.z, rot: u.rot, aim: u.aim,
        structure: !!source.UNITS[u.type].structure, hp: u.hp, cells: u.cells ?? null,
        holdFire: !!u.holdFire, holdPos: !!u.holdPos, auto: !!u.auto, autoRetreat: !!u.autoRetreat }));
      current.environment = { seed: g.seed, wind: [g.wind.a, g.wind.v], wet: g.wx?.wet ?? null,
        pathBudget: g.pathBudget ?? 4096, pathRetryLimit: g.pathRetryLimit ?? 20 };
      current.checkpoints.push(snapshot(g));
      if (extra) current.frames.push(snapshot(g));
    }
    source.step(g);
    current.ticks = g.tick;
    if (current.type === 'tank' && current.scenario === 'corner' && g.tick > 1) {
      const u = g.units.get(current.subject);
      if (!u.path.length && !u.worldGoal && Math.abs(u.moveSpeed ?? 0) < 0.1
        && Math.hypot(u.x - 64, u.z - 48) < 0.5) current.cornerFinishTick ??= g.tick;
    }
    if (g.tick === 1 || g.tick % 20 === 0) current.checkpoints.push(snapshot(g));
    current.last = snapshot(g);
    if (extra) current.frames.push(current.last);
  },
};
const random = Math.random;
for (const type of MOVEMENT_TYPES) for (const scenario of scenarios) {
  current = { type, scenario, commands: [], checkpoints: [] };
  if (extra) Object.assign(current, { authoredOriginalLab: true, frames: [], mutations: [] });
  const verdict = runMovementCase(sim, motion, scenario, { type, seed: 7301 });
  if (extra) current.verdict = verdict;
  assert.deepEqual(verdict.failures, [], `${type}/${scenario}: original selected verdict`);
  if (type === 'tank' && scenario === 'corner') assert.ok(current.cornerFinishTick <= 181, 'light tank corner within nine seconds');
  if (current.checkpoints.at(-1).tick !== current.last.tick) current.checkpoints.push(current.last);
  delete current.last;
  if (extra) cases.push(current);
  else cases.push({ ...current, verdict });
}
if (extra) {
  current = null;
  clean();
  assert.equal(cases.length, 44);
  console.log(`Verified ${cases.length} original recovery/wall-arrival lab journeys, ${cases.reduce((sum, c) => sum + c.ticks, 0)} full steps`);
} else {

// Reproduce the selected engine acceleration and Stop timing using authentic
// Source actors, public commands and actual recipient rows. No server timer is
// required for these deterministic ground predicates.
current = { type: 'tank', scenario: 'engine-braking', commands: [], checkpoints: [], milestones: [] };
const engineMap = { name: 'Engine braking predicates', w: 64, h: 64, rows: Array(64).fill('.'.repeat(64)),
  spawns: [{ x: 8, y: 8 }, { x: 55, y: 55 }], points: [{ id: 'lab-binding', x: 4, y: 59 }] };
assert.equal(source.validateMap(engineMap), null);
let engine;
try {
  Math.random = () => 0.5;
  engine = source.createGame(engineMap, ['A', 'B'], false, [0, 1], [0, 1], { weather: false, supply: false });
} finally { Math.random = random; }
current.map = engineMap;
clearStartingForces(engine); engine.points = []; engine.seed = 7301;
engine.weather = { now: 'clear', next: null, at: 0 };
const tank = source.internals.spawnUnit(engine, 0, 'tank');
const foot = source.internals.spawnUnit(engine, 0, 'rifle');
current.subject = tank.id; current.engineActors = [tank.id, foot.id];
Object.assign(tank, { x: 30, z: 30, holdFire: true, holdPos: false, auto: false, autoRetreat: false });
Object.assign(foot, { x: 30, z: 42, holdFire: true, holdPos: false, auto: false, autoRetreat: false });
const steps = count => { for (let n = 0; n < count; n++) sim.step(engine); };
const delivered = label => {
  const packet = source.snapshotFor(engine, 0, [], [], source.snapshotCache(engine));
  const rows = [tank, foot].map(u => {
    const row = packet.units.find(row => row[0] === u.id);
    assert.ok(row, 'recipient must receive its own actor');
    return [row[0], row[3], row[4], row[5], row[15] ?? 0];
  });
  current.milestones.push({ label, tick: engine.tick, rows,
    plan: packet.plans.find(plan => plan[0] === tank.id)?.[1] ?? null });
  return rows;
};
steps(4);
sim.command(engine, 0, { t: 'move', orders: [[tank.id, 80, 30]] });
sim.command(engine, 0, { t: 'move', orders: [[foot.id, 80, 42]] });
steps(2);
const accelerated = delivered('accelerated');
assert.ok(accelerated[0][4] > 0 && accelerated[0][4] < source.UNITS.tank.speed);
assert.ok(accelerated[1][1] - 30 > 0.3);
steps(35);
const beforeStop = delivered('before-stop');
sim.command(engine, 0, { t: 'stop', ids: [tank.id] });
steps(2);
const braking = delivered('braking');
assert.ok(braking[0][1] > beforeStop[0][1] && braking[0][4] > 0 && braking[0][4] < beforeStop[0][4]);
steps(50);
const stopped = delivered('stopped');
assert.equal(stopped[0][4], 0);
assert.ok(stopped[0][1] - braking[0][1] <= beforeStop[0][4] ** 2 / (2 * motion.VEHICLE_PROFILES.tank.braking) + 1);
assert.equal(current.milestones.at(-1).plan, 0, 'stopped recipient plan has no resumed travel');
assert.equal(tank.path.length, 0); assert.equal(tank.orders.length, 0); assert.equal(tank.worldGoal, null);
if (current.checkpoints.at(-1).tick !== current.last.tick) current.checkpoints.push(current.last);
delete current.last;
cases.push(current);

// The selected diagonal regression is separate from the lab's journey reader.
const cliffs = [];
for (const heights of [[0, 2, 0, 0], [0, 0, 2, 0], [0, 1, 1, 2]]) {
  const g = { w: 2, h: 2, height: Int8Array.from(heights) };
  for (const [from, to] of [[{ x: 1, z: 1 }, { x: 3, z: 3 }], [{ x: 3, z: 3 }, { x: 1, z: 1 }]]) {
    const clear = motion.segmentCliffClear(g, from, to);
    assert.equal(clear, false, 'selected diagonal must fail in both directions');
    cliffs.push({ heights, from, to, clear });
  }
}
clean();
const output = JSON.stringify({ reference: pin, node: process.version, arch: process.arch,
  scope: 'Original clearance/corner/cliff/opposing/parked/road/queue journeys across the original eleven lab types, canonical spawnUnit births, unchanged lab verdicts, light-tank nine-second corner and engine acceleration/Stop recipient-row predicates. Native admission and continuous verdicts remain required. MOV-15 delivered corner, MOV-20 reverse set and MOV-21 Horde remain separate.',
  sourceFiles: ['test-movement-lab.js', 'tools/movement-lab-scenarios.js', 'tools/movement-lab-runner.js', 'shared/vehicle-motion.js', 'shared/sim.js'],
  types: MOVEMENT_TYPES, scenarios, cases, cliffs }) + '\n';
const target = resolve(root, 'rust/crates/simtest/tests/fixtures/mov-vehicle-predicates.json');
if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), output);
else writeFileSync(target, output);
console.log(`Verified ${cases.length - 1} original lab journeys and one engine sequence, ${cases.reduce((sum, c) => sum + c.ticks, 0)} full steps and ${cliffs.length} diagonal checks`);
}
