// Records the frozen workload call by call, for the movement port's parity check (`simtest --replay`):
//   node harness/movement-trace.mjs harness/out/frozen-400.json [--ticks 4] [--out harness/out/trace-400.json]
//     [--area x0,z0,x1,z1]
// Run it with x64 Node 24.21.0: movement goes through Math.atan2, sin, cos and tan, whose last bits differ on arm64
// (DESIGN.md, "The core's math"). --area keeps only the units inside that rectangle (metres), for a smaller recording.
//
// It plays the same game as harness/sim-workload.mjs (harness/workload-patches.mjs) and records, for each tick:
// - start: every unit at the tick's start, right after the spatial index is rebuilt;
// - visible: each player's visible enemies when the unit loop begins (the vision pass has just run), and wet, how wet
//   the ground is then (weather() at the end of each step dries or soaks it);
// - events, in the unit loop's order: 'move' for each movement block (the unit just before the block and just after
//   the stuck count that ends it, with the suppression speed factor taken at the start of the unit's turn and the
//   speed the block worked out), and 'place' for each unit placed outside the block (placeUnit: riders on carriers);
// - separate, separated, progressed: every unit before and after separateCrowds, and after the infantry progress
//   count that follows it.
// Units are recorded as deltas: only the fields that changed since that unit's previous record (a unit's first record
// leaves out fields that are undefined, which read the same when missing). The fields are the
// ones the Rust core's units hold (UNIT_FIELDS), with two flags for what JSON cannot hold: whether laneScan.path and
// motionClearance.path are the unit's path array itself (the reference compares them by identity).
// The game at the start (map, players, units) is written with harness/state-codec.mjs, per-cell arrays as runs.
import { readFileSync, writeFileSync } from 'node:fs';
import { encode, decode } from './state-codec.mjs';
import { loadSim, freeze } from './workload-patches.mjs';

// Hooks into step(), applied after the workload's patches. Each calls a function this script sets on __trace.
const HOOKS = [
  ['export const __timing', 'export const __trace = {};\nexport const __timing'],
  ['  rebuildGrid(g);\n  const flightPrior', '  rebuildGrid(g);\n  __trace.tickStart?.(g);\n  const flightPrior'],
  ['  const infantryProgress = new Map();\n', '  const infantryProgress = new Map();\n  __trace.loopStart?.(g);\n'],
  ['    // Global waypoints, voluntary traffic steering and swept contact are separate steps.\n',
    '    __trace.before?.(g, u, sm.speed);\n    // Global waypoints, voluntary traffic steering and swept contact are separate steps.\n'],
  ['    if (u.logistics?.forced && u.path.length && !u.supplyHold)', '    __trace.after?.(g, u, speed);\n    if (u.logistics?.forced && u.path.length && !u.supplyHold)'],
  ['separateCrowds(g, list);', '__trace.separate?.(g); separateCrowds(g, list); __trace.separated?.(g);'],
  ['  stepFlights(g, flightPrior);\n', '  __trace.progressed?.(g);\n  stepFlights(g, flightPrior);\n'],
  ['function placeUnit(g, u, fields) { Object.assign(u, fields); updateGrid(g, u); return u; }',
    'function placeUnit(g, u, fields) { Object.assign(u, fields); updateGrid(g, u); __trace.place?.(g, u); return u; }'],
];

// The unit fields the Rust core holds (rust/crates/simtest/src/load.rs reads them).
export const UNIT_FIELDS = [
  'id', 'type', 'owner', 'x', 'z', 'rot', 'hp',
  'garrison', 'riding', 'air', 'still', 'shotAt', 'dust', 'built', 'cells',
  'path', 'reverse', 'pace', 'attackId', 'retreating', 'sprint', 'supp', 'hitFrom', 'stuck', 'repath', 'worldGoal', 'amove',
  'moveSpeed', 'travelDir', 'vx', 'vz', 'drift', 'motionBefore', 'motionClearance',
  'traffic', 'trafficWait', 'trafficTrail', 'trafficEntered', 'queuedBehind', 'laneScan', 'ghost',
  'holdPos', 'build', 'dig', 'entrench', 'board', 'rail', 'operative', 'convoy', 'logistics',
];
// read only for their truth: recorded as true or false
const TRUTH = new Set(['holdPos', 'build', 'dig', 'entrench', 'rail', 'operative', 'convoy', 'logistics']);
const value = v => (v === undefined ? { $undefined: true } : encode(v));

// A unit as the Rust core holds it, each field as JSON text.
function record(u) {
  const out = {};
  for (const k of UNIT_FIELDS) {
    let v = u[k];
    if (TRUTH.has(k)) v = !!v;
    else if (k === 'air' && v) v = { state: v.state };
    else if (k === 'laneScan' && v) v = { until: v.until, lane: v.lane };
    else if (k === 'motionClearance' && v) v = { goal: v.goal, reverse: v.reverse, until: v.until };
    out[k] = JSON.stringify(value(v));
  }
  out.samePathLane = String(!!u.laneScan && u.laneScan.path === u.path);
  out.samePathClearance = String(!!u.motionClearance && u.motionClearance.path === u.path);
  return out;
}

const last = new Map(), UNDEFINED = JSON.stringify(value(undefined));
// The fields of u that changed since its previous record (the first time, all of them but those left undefined, which
// read the same when missing).
function delta(u) {
  const rec = record(u), prev = last.get(u.id), out = {};
  for (const [k, s] of Object.entries(rec)) if (prev ? prev[k] !== s : s !== UNDEFINED) out[k] = JSON.parse(s);
  last.set(u.id, rec);
  return out;
}
const deltas = g => [...g.units.values()].map(u => [u.id, delta(u)]).filter(([, d]) => Object.keys(d).length);

const letterCodes = letters => Uint8Array.from(letters, ch => ch.charCodeAt(0));
// The game without its units, cut down to what the movement block, separation and the vision pass read.
function trim(g) {
  const pick = (o, keys) => Object.fromEntries(keys.filter(k => o[k] !== undefined).map(k => [k, o[k]]));
  return {
    w: g.w, h: g.h, flags: g.flags, height: g.height, house: g.house,
    // terrain letters as character codes, so they are written as runs
    chars: letterCodes(g.chars), ground: g.ground && letterCodes(g.ground), objects: g.objects && letterCodes(g.objects),
    wear: g.wear, cellHp: g.cellHp, trenchFront: g.trenchFront, tough: g.tough,
    tick: g.tick, visionTick: g.visionTick, terrainVersion: g.terrainVersion, mode: g.mode, naval: g.naval, reveal: g.reveal,
    weather: { now: g.weather?.now }, wx: g.wx && { rain: g.wx.rain, wet: g.wx.wet },
    players: g.players.map(p => ({ slot: p.slot, team: p.team, visible: p.visible })),
    smokes: g.smokes.map(s => ({ x: s.x, z: s.z, r: s.r })), strikes: g.strikes.map(s => pick(s, ['live', 'kind', 'owner', 'x', 'z', 'dir'])),
    structuralCells: new Map([...g.structuralCells].map(([c, s]) => [c, pick(s, ['state', 'hp', 'maxHp'])])),
    wrecks: g.wrecks.map(w => ({ x: w.x, z: w.z, motion: !!w.motion })),
  };
}

// shared/sim.js with the workload's patches and the hooks above. Call it once, before anything else imports sim.js.
export async function traceSim() {
  const sim = await loadSim(HOOKS);
  freeze(sim);
  return sim;
}

// Keeps only the units inside a rectangle (metres): [x0, z0, x1, z1].
export function keepArea(g, area) {
  for (const u of [...g.units.values()]) if (!(u.x >= area[0] && u.z >= area[1] && u.x < area[2] && u.z < area[3])) g.units.delete(u.id);
}

// Steps g for `ticks` ticks with sim from traceSim(), recording as described at the top.
export async function trace(sim, g, ticks) {
  // tables.mjs imports shared/sim.js too: only after the hooks are in place, so both get the patched module
  const { tables } = await import('./tables.mjs');
  last.clear();
  const game = encode(trim(g), { runs: true });
  game.units = { $map: [...g.units.values()].map(u => [u.id, delta(u)]) };

  const steps = [];
  let now = null;
  Object.assign(sim.__trace, {
    tickStart: g => { now = { start: deltas(g), events: [] }; steps.push(now); },
    loopStart: g => { now.tick = g.tick; now.visible = g.players.map(p => [...p.visible]); now.wet = g.wx?.wet ?? null; },
    before: (g, u, smSpeed) => { now.events.push(['move', u.id, delta(u), smSpeed]); },
    after: (g, u, speed) => { now.events.at(-1).push(delta(u), speed); },
    place: (g, u) => { if (now?.visible) now.events.push(['place', u.id, delta(u)]); },
    separate: g => { now.separate = deltas(g); },
    separated: g => { now.separated = deltas(g); },
    progressed: g => { now.progressed = deltas(g); },
  });

  let draws = 0;
  const random = Math.random;
  Math.random = () => { draws++; return random(); };
  const terrainBefore = g.terrainVersion, cellsBefore = g.newCells.length;
  try {
    for (let t = 0; t < ticks; t++) sim.step(g);
  } finally {
    Math.random = random;
    for (const k of Object.keys(sim.__trace)) delete sim.__trace[k];
  }
  if (draws) throw new Error(`the frozen workload drew ${draws} random numbers`);
  if (g.terrainVersion !== terrainBefore) throw new Error('the frozen workload changed the terrain');
  // touch() after a vehicle's step logs a cell only when its look changed; the port leaves touch out
  if (g.newCells.length !== cellsBefore) throw new Error('a cell changed its look during the run');
  return {
    reference: 'e4e579a', node: process.version, v8: process.versions.v8, arch: process.arch, ticks,
    tables: tables(), game, steps,
  };
}

if (import.meta.main) {
  const args = process.argv.slice(2), opt = (name, fallback) => (args.includes(name) ? args[args.indexOf(name) + 1] : fallback);
  const file = args[0];
  if (!file || file.startsWith('--')) throw new Error('usage: node harness/movement-trace.mjs <state.json> [--ticks N] [--out file] [--area x0,z0,x1,z1]');
  if (process.arch !== 'x64') console.warn(`warning: ${process.arch} Node computes other last bits in V8's math than x64 Node; use x64 for recordings that become fixtures`);
  const area = opt('--area', null)?.split(',').map(Number) ?? null;
  const out = opt('--out', file.replace(/(frozen-)?([^/]*)\.json$/, 'trace-$2.json'));
  const sim = await traceSim();
  const g = decode(JSON.parse(readFileSync(file, 'utf8')).game);
  if (area) keepArea(g, area);
  const result = { state: file, area, ...await trace(sim, g, Number(opt('--ticks', 4))) };
  writeFileSync(out, JSON.stringify(result));
  const calls = result.steps.reduce((n, s) => n + s.events.filter(e => e[0] === 'move').length, 0);
  console.log(`wrote ${out}: ${result.steps.length} ticks, ${result.game.units.$map.length} units, ${calls} movement calls`);
}
