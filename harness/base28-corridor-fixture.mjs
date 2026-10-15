// Paired component experiment. Both constructors, commands and full steps are pinned Source.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync, writeFileSync} from 'node:fs';
import {join, resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {gzipSync, gunzipSync} from 'node:zlib';

assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? join(root, 'reference/ww2-rts'));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], {encoding: 'utf8'}).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const sim = await import(pathToFileURL(join(reference, 'shared/sim.js')));
const {fixtureCommand} = await import(pathToFileURL(join(reference, 'test-fixtures.js')));
const {DEBRIS_LIMITS} = await import(pathToFileURL(join(reference, 'shared/debris-motion.js')));
const view = new DataView(new ArrayBuffer(8));
const bits = value => {
  view.setFloat64(0, value, false);
  return view.getBigUint64(0, false).toString(16).padStart(16, '0');
};
const point = value => ({x: bits(value.x), z: bits(value.z)});
const u64 = value => BigInt.asUintN(64, value), gamma = 0x9e3779b97f4a7c15n;
const mix = value => {
  value = u64((value ^ (value >> 30n)) * 0xbf58476d1ce4e5b9n);
  value = u64((value ^ (value >> 27n)) * 0x94d049bb133111ebn);
  return value ^ (value >> 31n);
};
// The same declared constructor draw used by the retained Match-seed-701 fixtures.
const bootstrap = Number(mix(u64(mix(701n ^ u64(2n * gamma)) + gamma)) & 0xffffffffn);
const originalRandom = Math.random;
const expectedCells = [339, 340, 341, 379, 380, 381, 419, 420, 421];
const localCells = Array.from({length: 9}, (_, y) =>
  Array.from({length: 9}, (_, x) => (y + 5) * 40 + x + 16)).flat();
const from = {x: 5, z: 19}, to = {x: 75, z: 19};
const originalMap = {w: 40, h: 22,
  rows: Array.from({length: 22}, (_, y) => '.'.repeat(20) + (y >= 8 && y <= 10 ? '.' : 'W') + '.'.repeat(19)),
  spawns: [{x: 2, y: 2}, {x: 37, y: 19}], points: []};
const amendedMap = {...structuredClone(originalMap), name: 'Base corridor', points: [{x: 10, y: 20, mp: 0, vp: 0}]};
const settlementSteps = Math.ceil(DEBRIS_LIMITS.lifetime / sim.TICK) + 2;
assert.equal(settlementSteps, 52);
let active = null, activeGame = null;

// Preserve the complete own-state graph, insertion order, typed arrays and undefined values.
// This is separate from the fixed measured projection; no raw state is installed into native state.
function rawState(value) {
  const seen = new Map();
  function encode(item, path) {
    if (item === undefined) return {$undefined: true};
    if (typeof item === 'number') return {$f64: bits(item)};
    if (typeof item === 'bigint') return {$bigint: String(item)};
    if (!item || typeof item !== 'object') {
      assert.notEqual(typeof item, 'function');
      return item;
    }
    if (seen.has(item)) return {$ref: seen.get(item)};
    seen.set(item, path);
    if (item instanceof Map) return {$map: [...item].map(([key, entry], i) => [encode(key, `${path}/key${i}`), encode(entry, `${path}/value${i}`)])};
    if (item instanceof Set) return {$set: [...item].map((entry, i) => encode(entry, `${path}/${i}`))};
    if (ArrayBuffer.isView(item)) return {$typed: item.constructor.name, values: [...item].map((entry, i) => encode(entry, `${path}/${i}`))};
    if (Array.isArray(item)) return item.map((entry, i) => encode(entry, `${path}/${i}`));
    return Object.fromEntries(Object.keys(item).map(key => [key, encode(item[key], `${path}/${key}`)]));
  }
  return encode(value, '$');
}
function cell(g, c) {
  return {c, ch: g.chars[c], ground: g.ground[c], object: g.objects[c], flags: g.flags[c],
    level: g.height?.[c] ?? 0, hp32: new Uint32Array(g.cellHp.buffer)[c],
    wear32: new Uint32Array(g.wear.buffer)[c], scar32: new Uint32Array(g.scar.buffer)[c],
    state: g.cellState[c], burnt: g.burnt[c]};
}
function scene(kind) {
  const map = kind === 'original' ? structuredClone(originalMap) : structuredClone(amendedMap);
  const opts = {mode: kind === 'original' ? 'classic' : 'conquest'};
  const measured = [], raw = [], witness = [];
  active = {kind, map, opts, measured, raw, witness};
  activeGame = null;
  let draws = 0, steps = 0, site = null, debit = null;
  Math.random = () => { draws++; return bootstrap / 2 ** 32; };
  if (kind === 'amended') assert.equal(sim.validateMap(map), null);
  const validation = sim.validateMap(map);
  const g = sim.createGame(map, ['a', 'b'], false, [0, 1], [0, 1], opts);
  activeGame = g;
  assert.equal(draws, 1, 'declared constructor draw only');
  Math.random = () => { throw Error(`${kind}: runtime Math.random interference`); };
  const constructor = rawState(g);
  const crew = [...g.units.values()].find(u => u.owner === 0 && u.type === (kind === 'original' ? 'engineer' : 'rifle'));
  assert.ok(crew);
  const retained = [...g.units.values()].map(u => ({id: u.id, hp: u.hp, cells: structuredClone(u.cells ?? [])}));
  const initialOrder = [...g.units.keys()];
  g.nodes = []; g.players[0].mp = 5000;
  crew.x = 37; crew.z = 19;
  function save(label, route = null) {
    for (const actor of retained) {
      const u = g.units.get(actor.id);
      assert.ok(u, `${kind}: retained natural actor`);
      assert.equal(u.hp, actor.hp, `${kind}: unrelated actor damage is interference`);
      assert.deepEqual(u.cells ?? [], actor.cells, `${kind}: retained natural footprint`);
      for (const c of actor.cells) assert.equal(g.chars[c], 'K');
    }
    if (kind === 'amended') {
      assert.equal(g.points.length, 1);
      const p = g.points[0];
      assert.deepEqual([p.owner, p.capper, p.progress, p.mp, p.vp, p.cut], [-1, -1, 0, 0, 0, false]);
    }
    const sections = (site?.cells ?? []).map(c => {
      const s = g.structuralCells.get(c);
      assert.ok(s && s.entityId === site.id && s.structureId === site.structureId);
      return {c, state: s.state, hp: bits(s.hp), maxHp: bits(s.maxHp), material: s.material,
        failedAt: s.failedAt === undefined ? null : bits(s.failedAt),
        settledAt: s.settledAt === undefined ? null : bits(s.settledAt), rubble: structuredClone(s.rubble)};
    });
    // This field list is fixed before either run. Global IDs, mode, roster, seed,
    // absolute versions and income remain in raw/witness, never normalized into equality.
    measured.push({label, tick: g.tick, route: route?.map(point) ?? null, debit,
      site: site ? {type: site.type, owner: site.owner, hp: bits(site.hp), built: bits(site.built),
        cells: [...site.cells], at: point(site), live: g.units.get(site.id) === site,
        cost: bits(sim.UNITS.barracks.cost), maxHp: bits(sim.UNITS.barracks.hpPer)} : null,
      terrain: localCells.map(c => cell(g, c)), sections});
    witness.push({label, tick: g.tick, seed: g.seed >>> 0,
      versions: [g.terrainVersion ?? 0, g.infantryRegionVersion ?? 0, g.vehicleRegionVersion ?? 0, g.navalRegionVersion ?? 0, g.obstructionVersion ?? 0],
      wallet: g.players.map(p => bits(p.mp)),
      actors: [...g.units.values()].map(u => ({id: u.id, type: u.type, owner: u.owner, hp: bits(u.hp),
        at: point(u), cells: u.cells ?? null, built: u.built === undefined ? null : bits(u.built), structureId: u.structureId ?? null}))});
    raw.push({label, state: rawState(g)});
  }
  function step(label) { sim.step(g); steps++; active.steps = steps; save(label); }
  save('initial');
  const open = sim.findPath(g, from, to);
  assert.ok(open.length, 'corridor begins open'); save('open', open);
  step('preparation');
  const beforeIds = new Set(g.units.keys()), beforeMoney = g.players[0].mp;
  assert.equal(fixtureCommand(g, 0, {t: 'build', ids: [crew.id], kind: 'barracks', x: 41, z: 19}), undefined);
  const born = [...g.units.values()].filter(u => !beforeIds.has(u.id));
  assert.equal(born.length, 1); site = born[0];
  assert.equal(site.type, 'barracks');
  if (kind === 'original') assert.equal([...g.units.values()].find(u => u.type === 'barracks'), site);
  assert.ok(site, 'a building site is stamped in the corridor');
  debit = bits(beforeMoney - g.players[0].mp);
  assert.equal(debit, bits(150)); assert.equal(site.hp, 375); assert.equal(site.built, 0);
  assert.deepEqual(site.cells, expectedCells); assert.deepEqual([site.x, site.z], [41, 19]);
  assert.equal(g.structures.get(site.structureId).entityId, site.id);
  for (const c of site.cells) assert.equal(g.buildingCells.get(c).id, site.id);
  save('stamp');
  const closed = sim.findPath(g, from, to);
  assert.deepEqual(closed, [], 'footprint closes the only corridor'); save('closed', closed);
  assert.equal(fixtureCommand(g, 0, {t: 'stop', ids: [crew.id]}), undefined); save('stop');
  site.hp = 0; save('destroy-input'); step('destroyed');
  assert.equal(g.units.has(site.id), false, 'actual retirement removed only the destroyed site');
  for (let i = 0; i < settlementSteps; i++) step(`settle:${i + 1}`);
  const sections = site.cells.map(c => g.structuralCells.get(c));
  assert.ok(sections.every(s => s?.state === 'failed' && Number.isFinite(s.settledAt)), 'destroyed construction site: all sections failed and settled');
  const rubble = [...new Set(sections.flatMap(s => s.rubble))];
  assert.ok(rubble.length > 0, 'destroyed construction site: a durable rubble footprint remains');
  for (const c of rubble) {
    assert.equal(g.objects[c], 'R', 'destroyed construction site: recorded footprint contains rubble');
    assert.ok(site.cells.some(origin => Math.hypot(c % g.w - origin % g.w, Math.floor(c / g.w) - Math.floor(origin / g.w)) <= 3), 'destroyed construction site: rubble remains within 6 m of the collapsed footprint');
  }
  const reopened = sim.findPath(g, from, to);
  assert.ok(reopened.length, 'collapsed footprint reopens the corridor'); save('reopened', reopened);
  assert.equal(steps, 54);
  console.error(JSON.stringify({completed: kind, steps, snapshots: measured.length, queries: 3}));
  return {kind, map, opts, validation, constructor, initialOrder, builder: crew.id, newborn: site.id,
    draws, steps, snapshots: measured.length, measured, witness, raw};
}
const result = {reference: pin, node: process.version, arch: process.arch, constructor: {nativeSeed: 701, bootstrap},
  projection: {localCells, query: {from, to}, expectedCells, settlementSteps,
    measuredFields: ['label', 'tick', 'route', 'debit', 'site', 'terrain', 'sections'],
    separateRawDifferences: ['mode', 'roster', 'IDs', 'starting footprints', 'absolute versions', 'income', 'RNG', 'map name and point']},
  original: null, amended: null};
try {
  result.original = scene('original');
  result.amended = scene('amended');
  assert.equal(JSON.stringify(result.original.measured), JSON.stringify(result.amended.measured), 'exact original/amended measured component outputs');
  const content = JSON.stringify(result) + '\n';
  const target = join(root, 'rust/crates/simtest/tests/fixtures/base28-corridor.json.gz');
  if (process.argv.includes('--check')) assert.equal(gunzipSync(readFileSync(target)).toString(), content);
  else writeFileSync(target, gzipSync(content, {level: 9}));
  console.error(JSON.stringify({result: process.argv.includes('--check') ? 'checked' : 'generated',
    framePairs: result.original.snapshots, stepsPerPath: 54, actualSteps: 108, queriesPerPath: 3}));
} catch (error) {
  if (process.env.BASE28_FAILURE_PATH) writeFileSync(process.env.BASE28_FAILURE_PATH,
    JSON.stringify({error: String(error), result, active, currentRaw: activeGame ? rawState(activeGame) : null}) + '\n', {flag: 'wx'});
  throw error;
} finally { Math.random = originalRandom; clean(); }
