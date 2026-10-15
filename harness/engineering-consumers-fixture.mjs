// Original engineering acceptance through unchanged Source constructors, ticks and recipients.
// Generation and --check both require an explicit coordinator slot grant.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { gzipSync } from 'node:zlib';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const input = JSON.parse(readFileSync(`${root}/rust/crates/simtest/tests/fixtures/engineering-consumers-input.json`, 'utf8'));
assert.equal(input.reference, pin);
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const fixtures = await import(pathToFileURL(`${reference}/test-fixtures.js`));
const weather = await import(pathToFileURL(`${reference}/shared/weather.js`));
const ai = await import(pathToFileURL(`${reference}/shared/ai-view.js`));
const bits = n => {
  assert.ok(Number.isFinite(n), `nonfinite captured number ${n}`);
  const b = Buffer.alloc(8);
  b.writeDoubleBE(n);
  return b.toString('hex');
};
const f32bits = n => {
  const b = Buffer.alloc(4);
  b.writeFloatBE(n);
  return b.readUInt32BE();
};
const copy = v => JSON.parse(JSON.stringify(v));
const runs = values => {
  const result = [];
  for (const value of values) {
    if (result.length && Object.is(result.at(-1)[0], value)) result.at(-1)[1]++;
    else result.push([value, 1]);
  }
  return result;
};
function exact(raw) {
  const floatBits = {};
  function visit(value, path = '') {
    if (typeof value === 'number') floatBits[path] = bits(value);
    else if (value && typeof value === 'object') {
      for (const [key, child] of Object.entries(value)) visit(child, path ? `${path}.${key}` : key);
    }
  }
  visit(raw);
  return { raw, floatBits };
}
const point = p => p == null ? null : { x: p.x, z: p.z };
function projection(g, id) {
  assert.equal(g.units.size, 1);
  const u = g.units.get(id);
  assert.ok(u && u.hp > 0 && !u.riding && !u.air);
  return {
    tick: g.tick,
    actor: {
      id, type: u.type, owner: u.owner, x: u.x, z: u.z, rot: u.rot, hp: u.hp,
      supp: u.supp, moveSpeed: u.moveSpeed ?? 0, travelDir: u.travelDir ?? null,
      vx: u.vx ?? 0, vz: u.vz ?? 0, drift: !!u.drift,
      path: u.path.map(point), worldGoal: point(u.worldGoal),
      still: u.still ?? null, repath: u.repath, stuck: u.stuck,
      dust: u.dust ?? null, sprint: u.sprint, holdPos: !!u.holdPos,
      autoRetreat: u.autoRetreat, holdFire: !!u.holdFire, auto: u.auto,
    },
    weather: { now: g.weather.now, next: g.weather.next ?? null, at: g.weather.at ?? 0 },
    wx: copy(g.wx), seed: g.seed >>> 0, wind: copy(g.wind),
    terrain: {
      chars: g.chars.join(''), ground: g.ground.join(''), objects: g.objects.join(''),
      flags: runs(g.flags), height: g.height && runs(g.height),
      wear32: runs([...g.wear].map(f32bits)), hp32: runs([...g.cellHp].map(f32bits)),
      state: runs(g.cellState),
    },
  };
}
function environment(g) {
  return { tick: g.tick, weather: copy(g.weather), wx: copy(g.wx), seed: g.seed >>> 0, wind: copy(g.wind) };
}
function terrain(kind, w = 20, h = 20) {
  return Array.from({ length: h }, (_, y) => kind === 'road-lanes'
    ? (y >= 1 && y <= 3 ? 'D' : '.').repeat(w)
    : kind === 'mud-gap' ? '.'.repeat(10) + (y >= 7 && y <= 13 && y !== 11 ? 'M' : '.') + '.'.repeat(w - 11)
      : kind.repeat(w));
}
function mapFor(kind = '.', w = 20, h = 20) {
  return { ...copy(input.map), w, h, rows: terrain(kind, w, h) };
}
let startupDraws = 0, runtimeDraws = 0, runtimeSeed = input.runtimeRandomSeed;
const realRandom = Math.random;
const runtimeRandom = () => {
  runtimeDraws++;
  let t = runtimeSeed = (runtimeSeed + 0x6d2b79f5) | 0;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
function create(map, opts = {}, names = input.names, teams, factions) {
  Math.random = () => { startupDraws++; return input.startupRandom; };
  try { return sim.createGame(map, names, input.shuffle, teams, factions, opts); }
  finally { Math.random = runtimeRandom; }
}
function fresh(map, opts = {}) {
  const g = create(map, opts);
  fixtures.clearFixtureUnits(g);
  g.players.forEach(p => { p.spawn = copy(input.spawnSentinel); });
  return g;
}
function put(g, spec) {
  assert.equal(fixtures.fixtureCommand(g, spec.owner ?? 0, { t: 'buy', unit: spec.type }), undefined);
  const u = [...g.units.values()].at(-1);
  assert.equal(u.type, spec.type);
  Object.assign(u, { x: spec.x, z: spec.z, rot: spec.rot ?? 0, aim: 0, autoRetreat: false });
  if (spec.rot !== undefined) u.rot = spec.rot;
  return u;
}
function setup(g) {
  return {
    teams: g.players.map(p => p.team), factions: g.players.map(p => p.faction),
    players: g.players.map(p => ({ mp: p.mp, spawn: copy(p.spawn) })),
    nextId: g.nextId, environment: exact(environment(g)),
    actors: [...g.units.values()].map(u => ({ id: u.id, type: u.type, owner: u.owner,
      x: u.x, z: u.z, rot: u.rot, aim: u.aim, autoRetreat: u.autoRetreat,
      air: u.air ? copy(u.air) : null, built: u.built ?? null, cells: copy(u.cells ?? []), structureId: u.structureId ?? null })),
  };
}
function planKind(g, kind) {
  g.weather = { now: kind, next: null, at: 0 };
  if (kind === 'rain') Object.assign(g.wx, { raining: true, rain: 1, wet: 1 });
  if (kind === 'mud') g.wx.wet = 1;
}
function travel(spec) {
  const map = mapFor(spec.terrain, spec.w, spec.h), g = fresh(map);
  g.players[0].mp = spec.mp;
  if (spec.depth !== null) g.wear.fill(spec.depth);
  planKind(g, spec.weather);
  if (spec.wet !== null) g.wx.wet = spec.wet;
  if (spec.next !== null) g.wx.next = spec.next;
  const u = put(g, { ...spec, rot: spec.name.startsWith('sink-') ? undefined : 0 });
  const initial = setup(g), beforeMove = exact(projection(g, u.id));
  const request = { t: 'move', orders: [[u.id, spec.targetX, spec.z]] };
  assert.equal(fixtures.fixtureCommand(g, 0, request), undefined);
  // This is the original edge case's authored straight path, never a captured target output.
  if (spec.straight) u.path = [{ x: spec.targetX, z: spec.z }];
  const frames = [exact(projection(g, u.id))];
  let before = u.x;
  for (let tick = 1; tick <= spec.ticks; tick++) {
    sim.step(g);
    frames.push(exact(projection(g, u.id)));
    if (tick === spec.warmup) before = u.x;
  }
  return { name: spec.name, map, setup: initial, beforeMove, request, frames, travel: u.x - before };
}
function assertTravel(traces) {
  const d = Object.fromEntries(traces.map(t => [t.name, t.travel]));
  const near = (value, target, tolerance, name) => assert.ok(Math.abs(value - target) < tolerance, name);
  near(d['tank-road'] / d['tank-grass'], sim.CFG.roadSpeed, .1, 'a tank on a road drives roadSpeed times faster');
  assert.ok(d['tank-edge'] / d['tank-grass'] > 1.03 && d['tank-edge'] / d['tank-grass'] < sim.CFG.roadSpeed - .1, 'a tank half on the road gets part of the bonus');
  near(d['tank-shallow-mud'] / d['tank-grass'], sim.CFG.mudSpeed[0], .1, 'a tank in shallow mud');
  near(d['tank-deep-mud'] / d['tank-grass'], sim.CFG.mudSpeed[1], .1, 'a tank in deep mud');
  near(d['rifle-shallow-mud'] - d['rifle-grass'], 0, .2, 'infantry walk through mud at full speed');
  assert.ok(d['clear-tank'] > 5 && d['clear-rifle'] > 5, 'units move in clear weather');
  for (const [name, baseline, target, tolerance] of [
    ['rain-tank', 'clear-tank', .8, .06], ['rain-tank-road', 'clear-tank-road', 1, .03],
    ['rain-rifle', 'clear-rifle', 1, .03], ['mud-tank', 'clear-tank', .8, .06],
    ['mud-rifle', 'clear-rifle', .9, .03], ['snow-tank', 'clear-tank', .85, .03],
    ['mud-tank-road', 'clear-tank-road', 1, .03], ['sink-mud', 'sink-clear', 1, .03],
    ['snow-tank-road', 'clear-tank-road', .85, .03], ['snow-rifle', 'clear-rifle', .9, .03],
    ['fog-tank', 'clear-tank', 1, .03],
  ]) near(d[name] / d[baseline], target, tolerance, name);
}
function route(spec) {
  const map = mapFor(spec.terrain, spec.terrain === 'road-lanes' ? 80 : 20), g = fresh(map);
  g.chars.forEach((ch, c) => { if (ch === 'M') g.wear[c] = 1; });
  const path = sim.findPath(g, { ...spec.from_, type: spec.type }, spec.to);
  const wet = path.some(p => g.chars[Math.floor(p.z / sim.CELL) * g.w + Math.floor(p.x / sim.CELL)] === 'M');
  const onRoad = path.filter(p => Math.floor(p.z / sim.CELL) >= 1 && Math.floor(p.z / sim.CELL) <= 3).length;
  if (spec.name === 'mud-gap-tank') assert.ok(path.length > 1 && !wet, 'a vehicle routes around mud when dry ground is near');
  if (spec.name === 'mud-gap-rifle') assert.equal(path.length, 1, 'infantry walk straight through mud');
  if (spec.name === 'adjacent-road-tank') assert.ok(onRoad > 5, 'a vehicle takes the road beside it');
  if (spec.name === 'adjacent-road-rifle') assert.equal(onRoad, 0, 'infantry ignore the road');
  return { name: spec.name, map, setup: setup(g), path: exact(path), wet, onRoad };
}
function event(g, e) {
  if (e.weather) g.weather = { now: e.weather, next: null, at: 0 };
  for (const key of ['next', 'raining']) if (e[key] !== undefined) g.wx[key] = e[key];
}
function shower(spec) {
  const map = mapFor(), g = create(map, { weather: spec.weather }, input.names, [0, 1], [0, 1]);
  const initial = setup(g), frames = [];
  if (spec.weather === 'mud') assert.equal(g.wx.wet, 1, 'Mud starts soaked');
  if (spec.weather === 'rain') assert.deepEqual([g.wx.raining, g.wx.rain, g.wx.wet], [true, 1, 1], 'Rain starts raining and soaked');
  for (let tick = 0; tick <= spec.ticks; tick++) {
    if (tick > 0) sim.step(g);
    frames.push(exact(environment(g)));
    for (const e of spec.events.filter(e => e.tick === tick)) event(g, e);
  }
  if (spec.name === 'mud-held') assert.equal(g.wx.wet, 1, 'Mud stays soaked after a shower');
  if (spec.name === 'rain-to-mud') assert.deepEqual([g.wx.raining, g.wx.rain, g.wx.wet], [false, 0, 1], 'Rain turns to mud');
  if (spec.name === 'snow-no-rain') assert.deepEqual([g.wx.raining, g.wx.rain], [false, 0], 'no showers in snow');
  if (spec.name === 'clear-shower') assert.ok(g.wx.raining && g.wx.rain > 0, 'Clear can shower');
  return { name: spec.name, map, setup: initial, frames };
}
function announcements() {
  const map = mapFor(), g = fresh(map), clearRow = sim.snapshotFor(g, 0, []).weather;
  assert.deepEqual(clearRow, ['clear']);
  g.weather = { now: input.announcement.initial, next: input.announcement.next, at: input.announcement.at };
  const initial = setup(g), frames = [];
  for (let tick = 0; tick <= input.announcement.ticks; tick++) {
    if (tick) sim.step(g);
    const rows = g.players.map((_, slot) => sim.snapshotFor(g, slot, []).weather);
    assert.deepEqual(rows[0], rows[1], 'the warning is public');
    if (tick < 400) assert.deepEqual(rows[0], ['fog'], 'no warning too early');
    else if (tick < 600) assert.deepEqual(rows[0], ['fog', 'clear', Math.ceil((600 - tick) / 20)]);
    else assert.deepEqual(rows[0], ['clear'], 'the fog lifts on time');
    frames.push({ environment: exact(environment(g)), rows });
  }
  return { map, setup: initial, clearRow, frames };
}
function sight(kind, expected) {
  const map = mapFor(), g = fresh(map);
  g.players[0].mp = g.players[1].mp = 1000;
  planKind(g, kind);
  const observer = put(g, input.sight.observer);
  const foes = input.sight.distances.map(dx => put(g, { type: 'rifle', owner: 1, x: 4 + dx, z: 20 }));
  const initial = setup(g), frames = [exact(environment(g))], drawsBefore = runtimeDraws;
  for (let tick = 0; tick < input.sight.ticks; tick++) { sim.step(g); frames.push(exact(environment(g))); }
  const rows = sim.snapshotFor(g, 0, []).units;
  const visible = foes.filter(u => rows.some(row => row[0] === u.id)).map(u => Math.round(u.x - 4));
  assert.deepEqual(visible, expected, `${kind}: original sight predicate`);
  return { name: kind, map, setup: initial, frames, observer: observer.id, foes: foes.map(u => u.id), visible,
    rows: rows.length, globalDraws: runtimeDraws - drawsBefore };
}
function plane(kind) {
  const spec = input.planes, map = mapFor('.', spec.w, spec.h), g = fresh(map);
  g.players[0].mp = 5000;
  planKind(g, kind);
  const u = put(g, spec.actor);
  Object.assign(u.air, { state: 'out', mission: copy(spec.mission) });
  const initial = setup(g), frames = [];
  for (let tick = 0; tick <= spec.ticks; tick++) {
    if (tick) sim.step(g);
    frames.push(exact({ environment: environment(g), id: u.id, x: u.x, z: u.z, rot: u.rot,
      air: copy(u.air), sees: sim.teamSees(g, 0, spec.sightProbe) }));
  }
  assert.ok(u.x > spec.actor.x, `${kind}: actual plane travels`);
  assert.equal(weather.weatherSpeed(g, sim.UNITS.fighter), 1, 'planes fly over the weather');
  return { name: kind, map, setup: initial, frames, travel: u.x - spec.actor.x };
}
function constructor(spec) {
  const map = { ...mapFor(), ...spec.meta };
  const opts = { weather: spec.choice, weatherSeed: spec.seed, mapKey: spec.key };
  const g = create(map, opts, input.names, [0, 1], [0, 1]);
  const repeatPlan = /^random-([0-9]|[1-4][0-9])$/.test(spec.name)
    ? copy(create(map, opts, input.names, [0, 1], [0, 1]).weather) : null;
  if (repeatPlan) assert.deepEqual(g.weather, repeatPlan, 'same seed gives same actual constructor plan');
  return { name: spec.name, map, opts, setup: setup(g), plan: copy(g.weather), repeatPlan };
}
function aiCheck(kind) {
  const a = input.ai, map = { w: a.w, h: a.h, rows: terrain('.', a.w, a.h), spawns: a.spawns, points: a.points };
  const g = create(map, { weather: kind }, a.names, a.teams), view = ai.viewFor(g, 0, {}), probes = [];
  assert.equal(view.weather.now, kind);
  assert.equal(weather.aiCaution(view), weather.aiCaution(g));
  for (let x = a.probeStart; x < a.probeLimit; x += a.probeStep) for (let z = a.probeStart; z < a.probeLimit; z += a.probeStep) {
    const seen = view.sees({ x, z });
    assert.equal(seen, sim.teamSees(g, 0, { x, z }));
    probes.push({ x, z, seen });
  }
  return { kind, map, setup: setup(g), weather: copy(view.weather), caution: weather.aiCaution(view), probes };
}
try {
  Math.random = runtimeRandom;
  const traces = input.travel.map(travel);
  assertTravel(traces);
  const routes = input.routes.map(route), showers = input.showers.map(shower), announcement = announcements();
  const sights = input.sight.weathers.map((kind, i) => sight(kind, input.sight.expected[i]));
  assert.ok(sights[1].rows < sights[2].rows && sights[2].rows < sights[0].rows);
  const planes = input.planes.weathers.map(plane);
  for (const trace of planes) assert.equal(bits(trace.travel), bits(planes[0].travel), `${trace.name}: plane weather travel`);
  // Current-position sight is queried from each actual flying scene, not a multiplier table.
  for (const trace of planes) assert.deepEqual(trace.frames.map(f => f.raw.sees), planes[0].frames.map(f => f.raw.sees));
  const plans = input.plans.map(constructor), randomPlans = plans.slice(0, 300).map(p => p.plan);
  for (const kind of weather.WEATHER_KINDS) assert.ok(randomPlans.some(p => p.now === kind));
  assert.ok(randomPlans.some(p => p.next) && randomPlans.some(p => !p.next));
  for (const p of randomPlans.filter(p => p.next)) assert.ok(
    p.now === 'fog' && p.next === 'clear' && p.at >= 4200 && p.at <= 5400 ||
    p.now === 'rain' && p.next === 'mud' && p.at >= 6000 && p.at <= 8400);
  assert.deepEqual(plans[300].plan, plans[301].plan, 'same explicit weather seed in actual constructors');
  const byName = Object.fromEntries(plans.map(p => [p.name, p.plan]));
  for (const [name, expected] of [
    ['host-snow', { now: 'snow', next: null, at: 0 }],
    ['ardennes', { now: 'snow', next: null, at: 0 }],
    ['pegasus', { now: 'fog', next: 'clear', at: 4800 }],
    ['explicit-rain', { now: 'rain', next: null, at: 0 }],
    ['plain', { now: 'clear', next: null, at: 0 }],
    ['unknown-choice', { now: 'clear', next: null, at: 0 }],
    ['unknown-map-weather', { now: 'clear', next: null, at: 0 }],
  ]) assert.deepEqual(byName[name], expected, name);

  const randomCalls = opts => { const before = startupDraws; create(mapFor(), opts, input.names, [0, 1], [0, 1]); return startupDraws - before; };
  const drawCounts = { base: randomCalls({ weather: false }), map: randomCalls({}), fog: randomCalls({ weather: 'fog' }), random: randomCalls({ weather: 'random' }) };
  assert.equal(drawCounts.map, drawCounts.base); assert.equal(drawCounts.fog, drawCounts.base); assert.equal(drawCounts.random, drawCounts.base + 1);
  const aiCases = input.ai.weathers.map(aiCheck);
  const a = input.ai, g = create({ w: a.w, h: a.h, rows: terrain('.', a.w, a.h), spawns: a.spawns, points: a.points }, { weather: 'clear' }, a.names, a.teams);
  Object.assign(g.weather, { next: 'fog', at: g.tick + a.hiddenChangeTicks });
  const hidden = copy(ai.viewFor(g, 0, {}).weather); assert.equal(hidden.next, undefined);
  g.weather.at = g.tick + a.announcedChangeTicks;
  const announced = copy(ai.viewFor(g, 0, {}).weather); assert.equal(announced.next, 'fog');
  clean();
  const result = { reference: pin, node: process.version, arch: process.arch, input, traces, routes, showers, announcement, sights, planes, plans, drawCounts,
    ai: { cases: aiCases, hidden, announced, nativeConsumer: 'missing; no native AI closure claimed' }, startupDraws, runtimeDraws };
  const output = gzipSync(JSON.stringify(result) + '\n', { level: 9 });
  const destination = `${root}/rust/crates/simtest/tests/fixtures/engineering-consumers.json.gz`;
  if (process.argv.includes('--check')) assert.deepEqual(readFileSync(destination), output);
  else writeFileSync(destination, output);
  console.log('22 travel scenes (1980 ticks), 4 paths, 4 live-weather scenes (2000 ticks), 621 warning boundaries, 4 sight scenes (40 ticks), 5 plane scenes (200 ticks), 313 constructor plan cases (363 constructors), 5 Source-only AI cases (845 probes)');
} finally { Math.random = realRandom; }
