// Read-only callbacks around the unchanged original benchmark and sim functions.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, chmodSync } from 'node:fs';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const reference = resolve(process.env.WW2_REFERENCE);
const destination = resolve(process.env.WW2_MOV33_CASE);
const simUrl = pathToFileURL(resolve(reference, 'shared/sim.js')).href;
const selfUrl = import.meta.url;
const hash = value => createHash('sha256').update(JSON.stringify(value)).digest('hex');
const buffer = new DataView(new ArrayBuffer(8));
const bits = n => { buffer.setFloat64(0, n); return buffer.getBigUint64(0).toString(16).padStart(16, '0'); };
const point = p => [bits(p.x), bits(p.z)];
const runs = (values, floats = false) => {
  const out = [];
  for (const value of values) {
    const n = floats ? bits(value) : value;
    if (out.length && Object.is(out.at(-1)[0], n)) out.at(-1)[1]++;
    else out.push([n, 1]);
  }
  return out;
};
const plain = value => value === undefined ? { absent: true }
  : typeof value === 'number' ? { bits: bits(value), value: Number.isFinite(value) ? value : String(value) }
  : value instanceof Map ? { map: [...value].map(([k,v]) => [plain(k),plain(v)]) }
  : value instanceof Set ? { set: [...value].map(plain) }
  : ArrayBuffer.isView(value) ? { typed: value.constructor.name, values: [...value].map(plain) }
  : Array.isArray(value) ? value.map(plain)
  : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([k,v]) => [k,plain(v)])) : value;
const identities = new WeakMap();
let nextIdentity = 1;
const identity = object => {
  if (object === undefined || object === null) return null;
  if (!identities.has(object)) identities.set(object, nextIdentity++);
  return identities.get(object);
};
const terrainIds = new Map();
const trace = { format: 'mov33-original-benchmark-case-v1', constructors: [], terrains: [], commands: [], result: null, failure: null };
let active = null;
const stack = [];
function terrain(g, from) {
  assert(g.worldNearWalls, 'actual observed near-wall view missing; do not synthesize one');
  const team = g.players?.[from.owner]?.team;
  const value = {
    viewIdentity: identity(g), flagsIdentity: identity(g.flags), heightIdentity: identity(g.height),
    width: g.w, height: g.h, worldKnown: !!g.worldKnown,
    flags: runs(g.flags), heights: g.height === null || g.height === undefined ? null : runs(g.height),
    near: runs(g.worldNearWalls), wear: g.wear === null || g.wear === undefined ? null : runs(g.wear, true),
    wetness: bits(g.wx?.wet ?? 0), roads: !!g.roads,
    versions: { infantry: g.infantryRegionVersion ?? 0, vehicle: g.vehicleRegionVersion ?? 0,
      naval: g.navalRegionVersion ?? 0, obstruction: g.obstructionVersion ?? 0, knowledge: g.navigationKnowledgeVersion ?? 0 },
    rawMetadata: plain({ worldKnown: g.worldKnown, navigationObserved: g.navigationObserved,
      navigationKnowledge: g.navigationKnowledge, finePathLimit: g.finePathLimit, roads: g.roads, wx: g.wx,
      infantryRegionVersion: g.infantryRegionVersion, vehicleRegionVersion: g.vehicleRegionVersion,
      navalRegionVersion: g.navalRegionVersion, obstructionVersion: g.obstructionVersion,
      navigationKnowledgeVersion: g.navigationKnowledgeVersion,
      navigationChangedFromVersion: g.navigationChangedFromVersion, navigationChangedCells: g.navigationChangedCells,
      navigationChanges: g.navigationChanges }),
    changed: g.navigationChangedFromVersion === undefined || g.navigationChangedCells === undefined ? null
      : { from: g.navigationChangedFromVersion, to: g.navigationKnowledgeVersion ?? 0, cells: [...g.navigationChangedCells] },
    changes: (g.navigationChanges ?? []).map(v => ({ from: v.from, to: v.to, cells: [...v.cells] })),
    mines: [...(g.mineSeen ?? [])].filter(([,seen]) => team !== undefined && (seen >> team & 1)).map(([c]) => c),
    fires: [...(g.fires?.keys() ?? [])],
    bunkers: [...(g.units?.values() ?? [])].filter(u => u.type === 'bunker' && u.hp > 0).map(point),
  };
  const key = hash(value);
  if (!terrainIds.has(key)) { terrainIds.set(key, trace.terrains.length); trace.terrains.push(value); }
  return terrainIds.get(key);
}
export const capture = {
  constructor(original, args) {
    const map = structuredClone(args[0]);
    const g = original(...args);
    trace.constructors.push({ map, arguments: plain(args.slice(1)), roster: plain([...g.units.values()]),
      nextId: g.nextId, seed: g.seed, players: plain(g.players), layout: plain(g.spawnSlots) });
    return g;
  },
  findPath(original, g, from, to, units, isVehicle) {
    if (!active) return original(g, from, to);
    const known = !(!g.navigationObserved && !g.worldKnown && g.players?.[from.owner]);
    const base = known && !g.vehicleFootprintKnown;
    const leaf = known && !(isVehicle(units[from.type]) && !g.vehicleFootprintKnown);
    const root = stack.length === 0;
    const context = { query: null, leafResults: [] };
    if (base) {
      context.query = { index: active.queries.length, actor: from.id, kind: from.type,
        from: point(from), to: point(to), terrain: terrain(g, from),
        limit: g.finePathLimit === undefined ? null : g.finePathLimit, expected: null };
      active.queries.push(context.query);
    }
    const before = { ...g.pathStats };
    stack.push(context);
    let result;
    try { result = original(g, from, to); }
    finally { assert.equal(stack.pop(), context); }
    if (leaf) {
      const owner = context.query ? context : [...stack].reverse().find(v => v.query);
      assert(owner, 'a leaf must belong to an actual public query');
      owner.leafResults.push({ result, routeEnd: from.routeEnd === undefined ? null : point(from.routeEnd),
        expansions: (g.pathStats?.expansions ?? 0) - (before.expansions ?? 0),
        status: (g.pathStats?.deferred ?? 0) > (before.deferred ?? 0) ? 'ExpansionLimit'
          : (g.pathStats?.failed ?? 0) > (before.failed ?? 0) ? 'Unreachable' : 'Found' });
    }
    if (context.query) {
      assert.equal(context.leafResults.length, 1, 'one actual fine callback per public query required');
      const found = context.leafResults[0];
      assert.equal(found.result, result, 'public query must return its actual fine callback array');
      context.query.expected = { points: result.map(point), routeEnd: found.routeEnd,
        expansions: found.expansions, status: found.status };
      context.returned = result;
    }
    if (root) {
      const query = context.query ? context : context.child;
      assert(query?.query, 'root has no recorded observed public query');
      assert.equal(query.returned, result, 'bank-prefix composition is outside this fixed benchmark component');
      active.roots.push({ actor: from.id, query: query.query.index });
    } else if (context.query) {
      const parent = stack.at(-1);
      assert(!parent.child, 'multiple observed-query descendants require explicit review');
      parent.child = context;
    }
    return result;
  },
  command(original, args) {
    assert.equal(active, null, 'no nested benchmark commands');
    const [g, owner, command] = args;
    assert.equal(command.t, 'move');
    assert.equal(command.orders.length, 60);
    const row = { trial: trace.commands.length, owner, input: structuredClone(command),
      beforeRoster: [...g.units.values()].map(({ id,type,owner,x,z,hp,path }) => ({ id,type,owner,x,z,hp,path: path.map(point) })),
      queries: [], roots: [], accepted: null, routeJson: null, routeHash: null };
    trace.commands.push(row);
    active = row;
    try {
      const result = original(...args);
      row.accepted = result === undefined;
      row.denial = plain(result);
      row.routeJson = JSON.stringify([...g.units.values()].map(u => [u.id,u.path]));
      // Original bench hashes point objects {x,z}, preserving their key order.
      row.routeHash = createHash('sha256').update(row.routeJson).digest('hex');
      assert.equal(row.accepted, true, 'original benchmark command denied');
      assert.equal(row.queries.length, 60, 'retain all actual callback queries');
      assert.equal(row.roots.length, 60, 'retain every original actor route');
      assert.deepEqual(row.roots.map(v => v.actor), command.orders.map(v => v[0]));
      return result;
    } finally { active = null; }
  },
};
registerHooks({ load(url, context, next) {
  const result = next(url, context);
  if (url !== simUrl) return result;
  const original = readFileSync(resolve(reference, 'shared/sim.js'), 'utf8');
  const loaded = typeof result.source === 'string' ? result.source : Buffer.from(result.source).toString('utf8');
  assert.equal(loaded, original, 'observe exact pinned Source');
  const suffix = `\nimport {capture as __mov33} from ${JSON.stringify(selfUrl)};\n`
    + `const __mov33Create=createGame; createGame=(...args)=>__mov33.constructor(__mov33Create,args);\n`
    + `const __mov33Find=findPath; findPath=(g,from,to)=>__mov33.findPath(__mov33Find,g,from,to,UNITS,isGroundVehicle);\n`
    + `const __mov33Command=command; command=(...args)=>__mov33.command(__mov33Command,args);\n`;
  assert.equal((original + suffix).slice(0, -suffix.length), original);
  trace.instrumentation = { erasesExactly: true, appended: suffix };
  return { ...result, source: original + suffix };
} });
const log = console.log;
console.log = (...args) => {
  if (typeof args[0] === 'string' && args[0].startsWith('RESULT ')) trace.result = JSON.parse(args[0].slice(7));
  return log(...args);
};
process.on('uncaughtExceptionMonitor', error => { trace.failure = { message: String(error), stack: error.stack }; });
process.on('exit', code => {
  try {
    assert.equal(code, 0, 'original benchmark process failed');
    assert.equal(trace.constructors.length, 1);
    assert.equal(trace.commands.length, 32);
    assert.equal(trace.result.trials, 32);
    assert.equal(trace.result.warmupExcluded, 4);
    assert.equal(trace.result.packets.length, 32);
    const units = trace.commands[0].beforeRoster.map(({id,type,owner,x,z,hp}) => ({id,type,owner,x,z,hp}));
    const inputs = { map: trace.constructors[0].map, units, commands: trace.commands.slice(0,2).map(v => v.input) };
    assert.equal(hash(inputs), trace.result.inputHash, 'unchanged full original input hash');
    trace.inputJson = JSON.stringify(inputs);
    for (const row of trace.commands) {
      assert.deepEqual(row.input, trace.commands[row.trial % 2].input);
      assert.equal(row.routeHash, trace.result.packets[row.trial].routeHash);
      assert.equal(trace.result.packets[row.trial].result, 'accepted');
      assert.equal(row.queries.reduce((n,q) => n + q.expected.expansions, 0), trace.result.packets[row.trial].expansions);
      assert.equal(row.queries.length, trace.result.packets[row.trial].calls);
      assert.equal(trace.result.packets[row.trial].trial, row.trial);
    }
  } catch (error) { trace.failure ??= { message: String(error), stack: error.stack }; process.exitCode = 2; }
  if (trace.result) {
    const { milliseconds, packets, ...deterministic } = trace.result;
    trace.result = { ...deterministic, packets: packets.map(({ ms, ...packet }) => packet) };
    trace.wallClockExclusions = ['milliseconds', 'packets[].ms'];
  }
  trace.exit = process.exitCode ?? code;
  writeFileSync(destination, JSON.stringify(trace) + '\n', { flag: 'wx', mode: 0o400 });
  chmodSync(destination, 0o400);
});
