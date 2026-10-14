// Record original full ground ticks and the actual room callback's effect batches.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {gzipSync} from 'node:zlib';

assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
assert.ok(process.argv.slice(2).every(a => a === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], {encoding: 'utf8'}).trim();
function clean() {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
}
clean();
const simURL = pathToFileURL(`${reference}/shared/sim.js`).href;
const sim = await import(simURL);
const {rebuildGrid} = await import(pathToFileURL(`${reference}/shared/grid.js`));
const {createRoomTick} = await import(pathToFileURL(`${reference}/server/room-tick.js`));

const mask = (1n << 64n) - 1n, gamma = 0x9e3779b97f4a7c15n;
function mix(x) {
  x = ((x ^ (x >> 30n)) * 0xbf58476d1ce4e5b9n) & mask;
  x = ((x ^ (x >> 27n)) * 0x94d049bb133111ebn) & mask;
  return x ^ (x >> 31n);
}
const data = new DataView(new ArrayBuffer(8));
function bits(n) {data.setFloat64(0, n); return data.getBigUint64(0).toString(16).padStart(16, '0');}
function copy(v) {
  if (v instanceof Map) return [...v].map(([k, value]) => [copy(k), copy(value)]);
  if (v instanceof Set) return [...v].map(copy);
  if (ArrayBuffer.isView(v)) return [...v];
  if (Array.isArray(v)) return v.map(copy);
  if (v && typeof v === 'object') return Object.fromEntries(Object.entries(v)
    .filter(([, value]) => value !== undefined && typeof value !== 'function')
    .map(([k, value]) => [k, copy(value)]));
  return v;
}
function exact(value) {
  const raw = copy(value), floatBits = {};
  function visit(v, path = '') {
    if (typeof v === 'number') {assert.ok(Number.isFinite(v), path); floatBits[path] = bits(v);}
    else if (v && typeof v === 'object') for (const [key, child] of Object.entries(v)) visit(child, path ? `${path}.${key}` : key);
  }
  visit(raw);
  return {raw, floatBits};
}
function runs(values) {
  const original = [...values];
  const out = [];
  for (const value of original) {
    if (out.length && Object.is(out.at(-1)[0], value)) out.at(-1)[1]++;
    else out.push([value, 1]);
  }
  const decoded = out.flatMap(([value, count]) => Array(count).fill(value));
  assert.equal(decoded.length, original.length);
  for (let i = 0; i < original.length; i++) {
    assert.ok(Object.is(decoded[i], original[i]));
    if (typeof original[i] === 'number') assert.equal(bits(decoded[i]), bits(original[i]));
  }
  return out;
}
function random() {
  const streams = new Map([['Combat', {purpose: 3n, state: mix(701n ^ ((3n * gamma) & mask)), count: 0}],
    ['OffMap', {purpose: 4n, state: mix(701n ^ ((4n * gamma) & mask)), count: 0}]]);
  const draws = [];
  return {
    draws,
    next(tick) {
      const stack = new Error().stack;
      const sourceFrames = stack.split('\n').filter(line => line.includes(`${simURL}:`));
      assert.ok(sourceFrames.length);
      const first = sourceFrames[0].trim();
      const purpose = /at (fire|fireOperative) \(/.test(first) ? 'Combat' : /at stepStrikes \(/.test(first) ? 'OffMap' : null;
      assert.ok(purpose, `unexpected source random call: ${stack}`);
      const stream = streams.get(purpose);
      stream.state = (stream.state + gamma) & mask;
      stream.count++;
      const value = Number(mix(stream.state) >> 11n) / 9007199254740992;
      draws.push({tick, purpose, index: stream.count, value, bits: bits(value),
        site: first.replace(`file://${reference}/`, 'reference/ww2-rts/')});
      return value;
    },
    snapshot: () => [...streams].map(([purpose, s]) => ({purpose, count: s.count, state: s.state.toString(16).padStart(16, '0')})),
  };
}
function state(g, rng) {
  return exact({tick: g.tick, visionTick: g.visionTick, nextId: g.nextId, nextFlight: g.nextFlight,
    seed: g.seed, wind: g.wind, wx: g.wx, winner: g.winner, skipFog: !!g.skipFog,
    units: [...g.units.values()], unitOrder: [...g.units.keys()],
    players: g.players.map(p => ({slot: p.slot, name: p.name, team: p.team, faction: p.faction,
      mp: p.mp, inc: p.inc, vp: p.vp, sup: p.sup, spawn: p.spawn, visible: [...p.visible], out: !!p.out, away: !!p.away})),
    operatives: [...(g.operatives ?? [])], points: g.points, story: g.story ?? null, timeline: g.timeline ?? null, fallen: g.fallen ?? null,
    flights: [...g.flights.values()], flightOrder: [...g.flights.keys()], nades: g.nades, salvos: g.salvos,
    structures: [...g.structures], structuralCells: [...g.structuralCells], falling: g.fallingSections,
    shots: g.shots, newCells: g.newCells, cellLog: g.cellLog, strikes: g.strikes, smokes: g.smokes, covers: g.covers ?? [],
    terrain: {chars: runs(g.chars), flags: runs(g.flags), height: g.height && runs(g.height), cellHp: runs(g.cellHp),
      wear: runs(g.wear), scar: runs(g.scar), burnt: runs(g.burnt), cellState: runs(g.cellState),
      versions: [g.terrainVersion ?? 0, g.obstructionVersion ?? 0, g.infantryRegionVersion ?? 0, g.vehicleRegionVersion ?? 0, g.navalRegionVersion ?? 0]},
    environment: {fires: [...g.fires], soak: runs(g.soak), mines: [...g.mines], aid: [...g.aid], wrecks: g.wrecks}, random: rng.snapshot()});
}
const options = {mode: 'conquest', weather: false, tech: false, supply: false, logistics: false};
function scene(spec) {
  const map = {name: 'Recipient ground effects', w: 100, h: 60, rows: Array(60).fill('.'.repeat(100)),
    spawns: [{x: spec.kind === 'companion' ? 17 : 2, y: spec.kind === 'companion' ? 20 : 2}, {x: 97, y: 2}, {x: 2, y: 57}, {x: 97, y: 57}, {x: 50, y: 57}],
    points: [{id: 'quiet', x: 80, y: 50}]};
  if (spec.kind === 'falling') {
    const c = 20 * map.w + 23;
    map.rows[20] = map.rows[20].slice(0, 23) + 'B' + map.rows[20].slice(24);
    map.structures = [{id: 'fragile-house', kind: 'house', sections: [{id: 'wall', c, hp: 12, material: 'wood', anchor: true, supports: []}]}];
  }
  const prior = Math.random, startupDraws = [];
  Math.random = () => {startupDraws.push(.5); return .5;};
  let g;
  try {g = sim.createGame(map, ['Shooter', 'Victim', 'Near west', 'Near east', 'Far'], false, [0, 1, 2, 3, 4], [0, 1, 2, 3, 0], options);}
  finally {Math.random = prior;}
  g.units.clear(); g.nextId = 1; g.seed = 123; g.wind = {a: 0, v: .5};
  const units = spec.kind === 'companion'
    ? [[1, 'tank', 21, 41], [2, 'rifle', 10, 41], [3, 'rifle', 70, 41], [4, 'rifle', 180, 101]]
    : spec.kind === 'support' || spec.kind === 'falling'
      ? [[1, 'rifle', 46.5, 40.75], [0, 'rifle', 8, 8], [2, 'rifle', 10, 41], [3, 'rifle', 70, 41], [4, 'rifle', 180, 101]]
      : spec.kind === 'air'
        ? [[0, 'fighter', 41, 41], [1, 'rifle', 51, 41], [2, 'rifle', 10, 41], [3, 'rifle', 70, 41], [4, 'rifle', 180, 101]]
        : [[0, spec.victimHidden ? 'rifle' : 'sniper', 41, 41], [1, spec.victimHidden ? 'sniper' : 'rifle', 51, 41],
          [2, 'rifle', 10, 41], [3, 'rifle', 70, 41], [4, 'rifle', 180, 101]];
  for (const [owner, kind, x, z] of units) Object.assign(sim.internals.spawnUnit(g, owner, kind),
    {x, z, still: 20, holdPos: true, holdFire: true, auto: false, autoRetreat: false});
  if (spec.kind === 'local') Object.assign(g.units.get(1), {attackId: 2, targetId: 2});
  for (const p of g.players) p.mp = 500;
  if (spec.skipFog) g.skipFog = true;
  rebuildGrid(g); sim.internals.updateVision(g);
  return {g, setup: {map, options, nativeMode: 'Conquest', teams: [0, 1, 2, 3, 4], factions: [0, 1, 2, 3, 0], startupDraws,
    units: [...g.units.values()].map(copy), nextId: g.nextId, environmentSeed: 123, wind: {a: 0, v: .5}, skipFog: !!spec.skipFog}};
}
const specs = [
  {name: 'local-hidden-shooter-and-fog-bound', kind: 'local', ticks: 4, snapEvery: 2},
  {name: 'local-hidden-victim', kind: 'local', victimHidden: true, ticks: 6, snapEvery: 2},
  {name: 'launch-live-flight-refusal', kind: 'local', ticks: 2, snapEvery: 1},
  {name: 'local-fog-rule-ignores-skipFog', kind: 'local', skipFog: true, ticks: 2, snapEvery: 2},
  {name: 'scheduled-empty-recipient-reset', kind: 'local', noRecipient: true, ticks: 4, snapEvery: 2},
  {name: 'artillery-regular-ground-chronology', kind: 'support', ticks: 200, snapEvery: 2},
  {name: 'artillery-collapse-and-falling-refusal', kind: 'falling', ticks: 108, snapEvery: 2},
  {name: 'companion-regular-retired-identity', kind: 'companion', ticks: 4, snapEvery: 2},
  {name: 'companion-regular-hidden-both-after-vision', kind: 'companion', retireVictim: true, ticks: 6, snapEvery: 6},
  {name: 'real-aircraft-refusal', kind: 'air', ticks: 1, snapEvery: 1},
];
function refusals(g) {
  const values = [];
  if (g.flights.size) values.push('live-flights');
  if (g.fallingSections.some(s => !s.settled)) values.push('falling');
  if ([...g.units.values()].some(u => u.air)) values.push('air');
  if ([...g.units.values()].some(u => u.operative)) values.push('operative-actor');
  return values;
}
function run(spec, observe) {
  const {g, setup} = scene(spec), rng = random(), initial = state(g, rng), frames = [], operations = [];
  const room = {game: g, snapEvery: spec.snapEvery, players: g.players.map(() => ({ws: {readyState: 1}, rtt: 12})), spectators: [], operatives: []};
  let current, delivered, projected, snapshotSlot, cacheCalls, actualSent, beforeDelivery, emitted, drawStart;
  const tick = createRoomTick({
    step(game) {
      const prior = Math.random, n = game.shots.length;
      drawStart = rng.draws.length;
      Math.random = () => rng.next(game.tick);
      try {sim.step(game);} finally {Math.random = prior;}
      emitted = exact(game.shots.slice(n));
      beforeDelivery = state(game, rng);
    },
    aiTick(_game, seats, _views, schedule) {assert.deepEqual(seats, []); actualSent = schedule.sent;},
    snapshotCache(game) {cacheCalls++; return observe ? sim.snapshotCache(game) : {};},
    snapshotFor(game, slot, ...args) {snapshotSlot = slot; return observe ? sim.snapshotFor(game, slot, ...args) : {};},
    // Preserve the complete original snapshot. Network delta/version policy is outside this recorder.
    trimmed(_room, _net, fields) {projected.push({slot: snapshotSlot, fields: copy(fields)}); return fields;},
    sendSnapshot(_room, player, json) {delivered.push({slot: room.players.indexOf(player), fields: JSON.parse(json)});},
    sendWatchers() {assert.fail('unexpected watcher');},
    sendOperatives() {assert.fail('unexpected operative recipient');},
    connected: player => player.ws?.readyState === 1,
    backedUp: player => !!player.stalled,
    createTickMeter: () => ({}), recordTick: () => spec.snapEvery, tickStats: () => ({}),
  });
  function command(slot, command) {
    const result = sim.command(g, slot, command) ?? null;
    assert.equal(result, null, `${spec.name}: command admission`);
    operations.push({beforeTick: g.tick + 1, op: 'command', slot, command, result});
  }
  if (spec.kind === 'support' || spec.kind === 'falling') command(0, {t: 'support', kind: 'artillery', x: 41, z: 41, dir: .3});
  let companion;
  if (spec.kind === 'companion') {
    const op = sim.joinOperative(g, 'ray', 0); assert.ok(op); companion = op.unitId;
    operations.push({beforeTick: 1, op: 'join', key: 'ray', owner: 0, result: companion});
    const u = g.units.get(companion), target = g.units.get(1);
    const input = {seq: 0, forward: 0, strafe: 0, yaw: Math.atan2(target.z - u.z, target.x - u.x), pitch: 0,
      weapon: 0, fire: true, ads: true, crouch: false, sprint: false, jump: 0, reload: 0, nade: 0, squad: 0};
    assert.equal(sim.operativeInput(g, 'ray', input), true);
    operations.push({beforeTick: 1, op: 'input', key: 'ray', input, result: true});
  }
  const admitted = state(g, rng);
  for (current = 1; current <= spec.ticks; current++) {
    if (current === 2 && spec.kind === 'local') command(0, {t: 'destroy', ids: [1]});
    if (current === 2 && spec.kind === 'companion') {
      command(0, {t: 'destroy', ids: [companion]});
      if (spec.retireVictim) command(1, {t: 'destroy', ids: [1]});
    }
    room.players.forEach((p, slot) => {
      p.ws.readyState = spec.noRecipient && current === 2 ? (slot === 1 ? 1 : 3) : 1;
      p.stalled = spec.noRecipient && current === 2 && slot === 1;
    });
    const seats = room.players.flatMap((p, slot) => p.ws.readyState === 1 && !p.stalled ? [slot] : []);
    delivered = []; projected = []; cacheCalls = 0; actualSent = false;
    tick(room);
    assert.equal(actualSent, current % spec.snapEvery === 0);
    if (actualSent) {assert.equal(g.shots.length, 0); assert.equal(g.newCells.length, 0);}
    else assert.deepEqual(exact(g.shots), exact(beforeDelivery.raw.shots));
    if (observe) {
      assert.deepEqual(delivered.map(row => row.slot), actualSent ? seats : []);
      assert.equal(cacheCalls, actualSent && seats.length ? 1 : 0);
      frames.push({tick: current, ready: room.players.map(p => p.ws.readyState === 1), stalled: room.players.map(p => p.stalled),
        sent: actualSent, selected: actualSent ? seats : [], cacheCalls,
        generated: emitted, beforeDelivery, afterDelivery: state(g, rng),
        outputs: exact(delivered), sourceOutputs: exact(projected), refusals: refusals(g), draws: rng.draws.slice(drawStart)});
    }
  }
  return {name: spec.name, spec, setup, initial, admitted, operations: exact(operations), frames, final: state(g, rng), draws: rng.draws};
}
const traces = specs.map(spec => {
  const observed = run(spec, true), independent = run(spec, false);
  assert.deepEqual(observed.initial, independent.initial);
  assert.deepEqual(observed.admitted, independent.admitted);
  assert.deepEqual(observed.final, independent.final, `${spec.name}: observers cannot alter represented gameplay`);
  assert.deepEqual(observed.draws, independent.draws);
  return observed;
});
function named(name) {return traces.find(t => t.name === name);}
function output(frame, slot) {return frame.outputs.raw.find(row => row.slot === slot).fields;}
const local = named('local-hidden-shooter-and-fog-bound');
assert.deepEqual(local.frames[1].beforeDelivery.raw.shots.map(s => s.k), ['flight', 'contact']);
assert.deepEqual(local.frames[1].refusals, []);
assert.ok(output(local.frames[1], 2).shots[0].until < .2);
assert.equal(output(local.frames[1], 3).shots[0].until, .2);
assert.ok(!('f' in output(local.frames[1], 3).shots[0]) && !('fo' in output(local.frames[1], 3).shots[0]));
assert.deepEqual(output(local.frames[1], 4).shots, []);
assert.equal(local.draws.length, 1);
const victim = named('local-hidden-victim');
assert.ok(output(victim.frames[1], 3).shots.some(s => s.k === 'contact' && s.f === 1 && s.fo === 0 && !('t' in s) && !('to' in s)));
assert.ok(named('launch-live-flight-refusal').frames[0].refusals.includes('live-flights'));
assert.deepEqual(output(named('local-fog-rule-ignores-skipFog').frames[1], 4).shots, []);
const none = named('scheduled-empty-recipient-reset');
assert.equal(none.frames[0].sent, false); assert.equal(none.frames[0].afterDelivery.raw.shots.length, 1);
assert.equal(none.frames[1].beforeDelivery.raw.shots.length, 2); assert.deepEqual(none.frames[1].selected, []);
assert.equal(none.frames[1].cacheCalls, 0); assert.deepEqual(none.frames[1].outputs.raw, []);
assert.deepEqual(none.frames[3].outputs.raw.flatMap(row => row.fields.shots), []);
const support = named('artillery-regular-ground-chronology');
assert.equal(support.frames.flatMap(f => f.generated.raw).filter(s => s.k === 'shell').length, sim.supShells(0, 'artillery'));
assert.deepEqual(support.frames[100].generated.raw.map(s => s.k), ['shell', 'hurt']);
assert.ok(output(support.frames[101], 0).shots.some(s => s.k === 'hurt' && !('t' in s) && s.fo === 0 && s.to === 1));
assert.ok(output(support.frames[101], 3).shots.some(s => s.k === 'hurt' && s.t === 1 && s.fo === 0 && s.to === 1));
assert.deepEqual(output(support.frames[101], 4).shots.map(s => s.k), ['shell']);
const falling = named('artillery-collapse-and-falling-refusal');
assert.deepEqual(falling.frames[100].generated.raw.map(s => s.k), ['shell', 'hurt', 'collapse']);
assert.ok(falling.frames.some(f => f.refusals.includes('falling') && f.outputs.raw.some(row => row.fields.falling.length)));
for (const trace of traces.filter(t => t.spec.kind === 'companion')) {
  assert.equal(trace.draws.length, 2);
  assert.deepEqual(trace.frames[0].generated.raw.map(s => s.k), ['hurt', 'sniper']);
  assert.ok(trace.frames.filter(f => f.sent).every(f => !f.refusals.includes('operative-actor')));
}
const regular = named('companion-regular-hidden-both-after-vision').frames[5];
assert.ok(output(regular, 2).shots.some(s => s.k === 'sniper' && !('f' in s) && !('t' in s) && s.fo === 0 && s.to === 1));
assert.ok(output(named('companion-regular-retired-identity').frames[1], 2).shots.some(s => s.k === 'sniper' && s.f === 5 && s.t === 1));
assert.ok(named('real-aircraft-refusal').frames[0].refusals.includes('air'));
assert.ok(named('real-aircraft-refusal').frames[0].outputs.raw[0].fields.air.length);
// Same lossless path-array patches as fps-tick. Both raw and numeric-bit trees
// are reconstructed before the fixture is written, including final state.
function changes(a, b, path = [], out = []) {
  if (Object.is(a, b)) return out;
  if (Array.isArray(a) && Array.isArray(b) && a.length === b.length) {
    for (let i = 0; i < a.length; i++) changes(a[i], b[i], [...path, i], out);
    return out;
  }
  if (a && b && typeof a === 'object' && typeof b === 'object' && !Array.isArray(a) && !Array.isArray(b)) {
    for (const key of new Set([...Object.keys(a), ...Object.keys(b)])) {
      if (!Object.hasOwn(b, key)) out.push([[...path, key]]);
      else changes(a[key], b[key], [...path, key], out);
    }
    return out;
  }
  out.push([path, b]); return out;
}
function applyChanges(value, delta) {
  for (const change of delta) {
    const path = change[0];
    if (!path.length) {value = copy(change[1]); continue;}
    let at = value;
    for (const key of path.slice(0, -1)) at = at[key];
    const key = path.at(-1);
    if (change.length === 1) delete at[key];
    else at[key] = copy(change[1]);
  }
  return value;
}
let reconstructed = 0;
function checkedRoundTrip(value) {
  const encoded = JSON.parse(JSON.stringify(value));
  for (const [path, hex] of Object.entries(encoded.floatBits)) {
    const keys = path.split('.'); let at = encoded.raw;
    for (const key of keys.slice(0, -1)) at = at[key];
    data.setBigUint64(0, BigInt(`0x${hex}`));
    at[keys.at(-1)] = data.getFloat64(0);
  }
  assert.deepEqual(encoded, value, 'JSON decoding plus recorded bits must restore original signed zero and finite leaves');
}
function checkedDelta(before, after) {
  const delta = changes(before, after);
  assert.deepEqual(applyChanges(copy(before), delta), after, 'complete raw and floatBits reconstruction');
  checkedRoundTrip(after);
  reconstructed++;
  return delta;
}
const counts = {traces: traces.length, ticks: traces.reduce((n, t) => n + t.spec.ticks, 0),
  recipients: traces.reduce((n, t) => n + t.frames.flatMap(f => f.outputs.raw).length, 0),
  draws: traces.reduce((n, t) => n + t.draws.length, 0), generatedEvents: traces.reduce((n, t) => n + t.frames.flatMap(f => f.generated.raw).length, 0)};
for (const trace of traces) {
  checkedRoundTrip(trace.initial);
  checkedRoundTrip(trace.operations);
  let previous = trace.initial;
  trace.admittedDelta = checkedDelta(previous, trace.admitted);
  previous = trace.admitted; delete trace.admitted;
  for (const frame of trace.frames) {
    for (const value of [frame.generated, frame.sourceOutputs, frame.outputs]) checkedRoundTrip(value);
    frame.beforeDelta = checkedDelta(previous, frame.beforeDelivery);
    frame.afterDelta = checkedDelta(frame.beforeDelivery, frame.afterDelivery);
    previous = frame.afterDelivery;
    delete frame.beforeDelivery; delete frame.afterDelivery;
  }
  trace.finalDelta = checkedDelta(previous, trace.final); delete trace.final;
}
assert.equal(reconstructed, 2 * counts.ticks + 2 * counts.traces);
clean();
const content = JSON.stringify({schema: 1, reference: pin, node: process.version, arch: process.arch, seed: 701,
  scope: 'Original complete Source steps, original snapshotFor, and original createRoomTick reset/selection. No expected events are injected. Live flights, persistent falling and AIR output are recorded refusal witnesses, not native snapshot support.',
  randomness: 'Established source-only SplitMix64 purpose adapters: Combat=3 at fire/fireOperative, OffMap=4 at stepStrikes. Every draw and source callsite is recorded; unexpected draws fail. Native Match keeps its normally seeded RNG. Source environment seed123 and wind are authored initial setup.',
  observers: 'Identity trimmed callback retains the complete original snapshot; transport delta/version policy and watcher/operative delivery are outside scope. Real clock measurements execute but are not recorded or used by the fixed cadence meter. Independent complete steps must match gameplay and draws without snapshot observers.',
  encoding: 'Each exact value contains raw plus every finite numeric floatBits dot path, including IDs/ticks. Original generated/reconstructed state/sourceOutputs facts preserve signed zero. outputs records actual room JSON.stringify payloads parsed as JSON, with Source wire zero normalization. Terrain arrays and environment.soak use lossless Object.is run pairs, independently checked against every original element bit. Ordered Map and Set entries remain arrays.',
  deltas: 'Start with trace.initial exact value. Apply admittedDelta. In frame order apply beforeDelta to reconstruct post-step/pre-reset state, then afterDelta to reconstruct post-room-delivery state. Apply finalDelta after the last frame. A patch is [pathArray,value] for replacement or [pathArray] for deletion; path members are literal object keys or array indexes, so dotted floatBits keys are not split. generated/sourceOutputs/outputs/operations/draws remain complete. Every state reconstruction is checked before serialization.',
  counts: {...counts, reconstructedStates: reconstructed}, traces}) + '\n';
const target = resolve(root, 'rust/crates/simtest/tests/fixtures/recipient-effects.json.gz');
const compressed = gzipSync(content, {level: 9});
if (process.argv.includes('--check')) assert.deepEqual(readFileSync(target), compressed);
else writeFileSync(target, compressed);
console.log(`${counts.traces} original traces, ${counts.ticks} complete ticks, ${counts.recipients} recipient results, ${counts.draws} source draws, ${counts.generatedEvents} generated events, ${reconstructed} reconstructed states, ${Buffer.byteLength(content)} JSON bytes, ${compressed.length} gzip bytes`);
