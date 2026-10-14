// Original World movement/contact boundaries, observed without replacing any rule body.
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
assert.ok(process.argv.slice(2).every(v => v === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const ref = resolve(process.env.WW2_REFERENCE ?? resolve(root, 'reference/ww2-rts'));
const target = resolve(root, 'rust/crates/core/tests/fixtures/world-contact.json');
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', ref, ...args], {encoding: 'utf8'}).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const simURL = pathToFileURL(resolve(ref, 'shared/sim.js')).href;
const sourceBytes = readFileSync(resolve(ref, 'shared/sim.js'));
const data = new DataView(new ArrayBuffer(8));
const bits = n => {
  data.setFloat64(0, n);
  return data.getBigUint64(0).toString(16).padStart(16, '0');
};
const real = n => ({value: n, bits: bits(n)});
const copy = v => v instanceof Map ? [...v].map(([k, x]) => [copy(k), copy(x)])
  : v instanceof Set ? [...v].map(copy)
  : ArrayBuffer.isView(v) ? [...v]
  : Array.isArray(v) ? v.map(copy)
  : v && typeof v === 'object' ? Object.fromEntries(Object.entries(v)
    .filter(([k, x]) => !k.startsWith('__contact') && x !== undefined && typeof x !== 'function')
    .map(([k, x]) => [k, copy(x)])) : v;
const canonical = g => copy({tick: g.tick, nextId: g.nextId, seed: g.seed, wind: g.wind, wx: g.wx,
  mode: g.mode, units: [...g.units], players: g.players, local: g.players[-1], regions: g.world.regions,
  nodes: g.nodes, shots: g.shots, flights: g.flights, nades: g.nades, salvos: g.salvos, strikes: g.strikes,
  fires: g.fires, soak: g.soak, wrecks: g.wrecks, smokes: g.smokes, story: g.story, timeline: g.timeline,
  winner: g.winner, structures: g.structures, structuralCells: g.structuralCells, chars: g.chars,
  flags: g.flags, height: g.height, hp: g.cellHp, wear: g.wear, state: g.cellState,
  surfaceWear: g.surfaceWear, ground: g.ground, objects: g.objects, mineLayer: g.mineLayer,
  mines: g.mines, log: g.cellLog, journal: g.cellJournal, newCells: g.newCells,
  terrainVersion: g.terrainVersion, obstructionVersion: g.obstructionVersion,
  infantryRegionVersion: g.infantryRegionVersion, vehicleRegionVersion: g.vehicleRegionVersion});
function observe(g, u, before, phase) {
  if (u.id !== g.__contactActorId) return;
  const c = Math.floor(u.z / 2) * g.w + Math.floor(u.x / 2);
  g.__contactRows.push({phase, tick: g.tick,
    unit: {id: u.id, owner: u.owner, type: u.type, x: real(u.x), z: real(u.z), hp: real(u.hp),
      dust: u.dust ?? null, path: copy(u.path)}, before: copy(before),
    distance: real(Math.sqrt((u.x - before.x) ** 2 + (u.z - before.z) ** 2)), cell: c,
    ch: g.chars[c], flags: g.flags[c], height: g.height?.[c] ?? 0,
    wet: g.wx ? g.wx.wet * (g.height && g.height[c] < 0 ? 2 : 1) : 0, wx: copy(g.wx),
    wear: real(g.wear[c]), wearF32Bits: new Uint32Array(g.wear.buffer)[c],
    surfaceWear: real(g.surfaceWear[c]), surfaceWearF32Bits: new Uint32Array(g.surfaceWear.buffer)[c],
    state: g.cellState[c], journal: copy(g.cellJournal), newCells: copy(g.newCells),
    versions: {terrain: g.terrainVersion, obstruction: g.obstructionVersion,
      infantry: g.infantryRegionVersion, vehicle: g.vehicleRegionVersion},
    shots: copy(g.shots), mines: copy(g.mines), seed: g.seed});
}
globalThis.__worldContactObserve = observe;
registerHooks({load(url, context, next) {
  const result = next(url, context);
  if (url !== simURL) return result;
  const source = String(result.source), start = source.indexOf('export function step(g) {');
  const end = source.indexOf('\nconst isStructure =', start);
  assert.ok(start >= 0 && end > start);
  let body = source.slice(start, end);
  const add = (anchor, addition) => {
    assert.equal(body.split(anchor).length, 2, `Source anchor moved: ${anchor}`);
    body = body.replace(anchor, anchor + addition);
  };
  add('    updateGrid(g, u);', "\n    globalThis.__worldContactObserve(g,u,before,'BeforeContact');");
  add('    u.still = moving ? 0 : u.still + dt;', "\n    globalThis.__worldContactObserve(g,u,before,'AfterContact');");
  return {...result, source: source.slice(0, start) + body + source.slice(end)};
}});
const sim = await import(simURL), pristine = await import(simURL + '?world-contact-pristine');
const {rebuildGrid} = await import(pathToFileURL(resolve(ref, 'shared/grid.js')));
const regions = Array.from({length: 64}, (_, i) => ({id: i, x: i % 8 * 64 + 32,
  y: Math.floor(i / 8) * 64 + 32, kind: i === 1 ? 'industrial' : 'rural',
  bounds: [i % 8 * 64, Math.floor(i / 8) * 64, i % 8 * 64 + 64, Math.floor(i / 8) * 64 + 64],
  ...(i === 0 ? {home: 0} : i === 63 ? {home: 1} : {})}));
const map = {name: 'World contact witness', w: 512, h: 512, rows: Array(512).fill('.'.repeat(512)),
  spawns: [{x: 32, y: 32}, {x: 480, y: 480}], points: [],
  world: {size: 'huge', total: 64, regionCells: 64, regions}};
const scenes = [
  {name: 'commander-infantry-positive-movement', actor: 'rifle', owner: 0, wear: 0},
  {name: 'local-infantry-positive-movement', actor: 'rifle', owner: -1, wear: 0},
  {name: 'commander-heavy-dry-open', actor: 'tank', owner: 0, wear: 0},
  {name: 'commander-heavy-packed-threshold', actor: 'tank', owner: 0, wear: .2495},
  {name: 'commander-heavy-mud-transition', actor: 'tank', owner: 0, wear: .9995},
  {name: 'local-light-dry-open', actor: 'armoredcar', owner: -1, wear: 0},
  {name: 'local-light-mud-depth', actor: 'armoredcar', owner: -1, wear: .5, ch: 'M'},
  {name: 'commander-heavy-wet-below-sea', actor: 'tank', owner: 0, wear: 0, weather: 'mud', height: -1},
  {name: 'local-light-allied-neutral-mine', actor: 'armoredcar', owner: -1, wear: 0, ch: 'N', mineOwner: -1},
  {name: 'local-infantry-allied-neutral-mine', actor: 'rifle', owner: -1, wear: 0, ch: 'N', mineOwner: -1},
  {name: 'commander-heavy-hostile-mine', actor: 'tank', owner: 0, wear: 0, ch: 'N', mineOwner: -1},
  {name: 'commander-heavy-crush', actor: 'tank', owner: 0, wear: 0, ch: 'H'},
];
const cases = [], savedRandom = Math.random;
try {
  for (const input of scenes) {
    const sourceMap = structuredClone(map), x = input.owner === 0 ? 51 : 171;
    const z = input.owner === 0 ? 51 : 85, c = Math.floor(z / 2) * map.w + Math.floor(x / 2);
    sourceMap.world.regionMap = Array.from({length: 512 * 512}, (_, c) =>
      Math.floor(Math.floor(c / 512) / 64) * 8 + Math.floor(c % 512 / 64));
    sourceMap.rows[Math.floor(z / 2)] = sourceMap.rows[Math.floor(z / 2)].slice(0, Math.floor(x / 2))
      + (input.ch ?? '.') + sourceMap.rows[Math.floor(z / 2)].slice(Math.floor(x / 2) + 1);
    if (input.height) {
      sourceMap.heights = Array(512).fill('0'.repeat(512));
      sourceMap.heights[Math.floor(z / 2)] = sourceMap.heights[Math.floor(z / 2)].slice(0, Math.floor(x / 2))
        + 'a' + sourceMap.heights[Math.floor(z / 2)].slice(Math.floor(x / 2) + 1);
    }
    const options = {mode: 'world', tech: true, weather: input.weather ?? false, supply: false, logistics: false};
    const setup = module => {
      const g = module.createGame(sourceMap, ['A', 'B'], false, [0, 1], [0, 0], options);
      let actor = [...g.units.values()].find(u => u.owner === input.owner && u.type === input.actor);
      if (!actor) { assert.equal(input.owner, 0); actor = module.internals.spawnUnit(g, 0, input.actor); }
      for (const u of g.units.values()) Object.assign(u, {holdFire: true, holdPos: true, auto: false, autoRetreat: false});
      Object.assign(actor, {x, z, rot: 0, ghost: 0});
      if (input.owner === -1) actor.guardHome = {x, z};
      g.wear[c] = input.wear;
      g.cellState[c] = Math.min(3, Math.floor(g.wear[c] * 4));
      if (input.mineOwner !== undefined) g.mines.set(c, input.mineOwner);
      g.cellJournal = []; g.newCells = []; g.seed = 123456789;
      rebuildGrid(g); module.internals.updateVision(g);
      const command = {t: 'move', orders: [[actor.id, x + 14, z]]};
      assert.equal(module.command(g, input.owner, command), undefined);
      return {g, actor, command};
    };
    let observedDraws = 0, pristineDraws = 0;
    Math.random = () => { observedDraws++; return .5; };
    const observed = setup(sim);
    Math.random = () => { pristineDraws++; return .5; };
    const plain = setup(pristine), {g, actor, command} = observed;
    assert.deepEqual(canonical(g), canonical(plain.g));
    assert.equal(observedDraws, pristineDraws);
    Object.defineProperty(g, '__contactActorId', {value: actor.id});
    Object.defineProperty(g, '__contactRows', {value: []});
    const initial = {pose: {x, z, rot: 0}, c, wear: real(g.wear[c]), dust: actor.dust ?? null,
      wind: copy(g.wind), seed: g.seed, actorCount: g.units.size, guardHome: copy(actor.guardHome),
      command, randomCalls: observedDraws};
    const fullSteps = [];
    for (let step = 0; step < 2; step++) {
      Math.random = () => { observedDraws++; return .5; }; sim.step(g);
      Math.random = () => { pristineDraws++; return .5; }; pristine.step(plain.g);
      const image = canonical(g);
      assert.deepEqual(image, canonical(plain.g), `${input.name}: passive observer changed Source`);
      assert.equal(observedDraws, pristineDraws, `${input.name}: passive observer changed RNG calls`);
      fullSteps.push({tick: g.tick, seed: g.seed, randomCalls: observedDraws,
        canonicalDigest: createHash('sha256').update(JSON.stringify(image)).digest('hex')});
    }
    const contacts = g.__contactRows;
    assert.equal(contacts.length, 4);
    for (let i = 0; i < contacts.length; i += 2) {
      assert.equal(contacts[i].phase, 'BeforeContact');
      assert.equal(contacts[i + 1].phase, 'AfterContact');
      assert.ok(contacts[i].distance.value > 0, input.name);
    }
    if (input.mineOwner === -1 && input.owner === -1) {
      assert.equal(g.mines.get(c), -1); assert.equal(g.shots.length, 0);
    }
    if (input.name.endsWith('hostile-mine') || input.name.endsWith('crush')) assert.ok(g.shots.length > 0);
    cases.push({name: input.name, inputs: input, initial, contacts, fullSteps,
      final: {tick: g.tick, actor: copy(actor), shots: copy(g.shots)}, pristineSteps: 2});
    console.log(`${input.name}: 2 original/pristine steps and 4 positive-displacement contact boundaries agree`);
  }
} finally { Math.random = savedRandom; clean(); }
assert.deepEqual(readFileSync(resolve(ref, 'shared/sim.js')), sourceBytes);
const output = JSON.stringify({format: 1, reference: pin,
  sourceHash: createHash('sha256').update(sourceBytes).digest('hex'), node: process.version, arch: process.arch,
  scope: 'Original World constructor and full steps with passive contact reads checked against a pristine module and fixed Source random-call counts. Native tests restore only a component boundary. Local Source owner -1 does not imply public issuer admission. Constructor RNG and full World replay remain separate.',
  map, cases}, null, 2) + '\n';
if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), output);
else writeFileSync(target, output);
console.log(`Verified ${cases.length} Source scenes, ${cases.length * 2} full observed/pristine steps, ${cases.length * 4} contact boundaries; fixture SHA256 ${createHash('sha256').update(output).digest('hex')}`);
