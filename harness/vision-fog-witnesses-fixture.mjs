// Supplemental CMB-131 witnesses. Requires a future Root recorder slot.
// Historical vision-rows and vision-wire-batch fixtures are read-only.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { decode, encode } from './state-codec.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = (...args) => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const historical = {
  'rust/crates/simtest/tests/fixtures/vision-rows.json': '00f2bab1491e77f52788d05a8f134ef2a14d1c8c590d64d34bfbca234dfea930',
  'rust/crates/core/tests/fixtures/vision-wire-batch.json': '040f9b187d2bebeb5d399de063601af640330903d51b029f5ab072588bde943a',
};
function preflight() {
  assert.equal(git('rev-parse', 'HEAD'), pin);
  assert.equal(git('status', '--porcelain', '--untracked-files=no'), '');
  for (const [name, hash] of Object.entries(historical)) {
    assert.equal(createHash('sha256').update(readFileSync(resolve(root, name))).digest('hex'), hash, name);
  }
}
assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert(process.argv.slice(2).every(arg => arg === '--check'));
preflight();
const localReference = pathToFileURL(resolve(root, 'reference/ww2-rts') + '/').href;
const actualReference = pathToFileURL(reference + '/').href;
registerHooks({
  resolve(specifier, context, next) {
    const url = new URL(specifier, context.parentURL ?? import.meta.url).href;
    return next(url.startsWith(localReference) ? actualReference + url.slice(localReference.length) : specifier, context);
  },
});
const sim = await import(actualReference + 'shared/sim.js');
const { rebuildGrid } = await import(actualReference + 'shared/grid.js');
const { sightMul } = await import(actualReference + 'shared/weather.js');
const { trim } = await import('./vision-fixture.mjs');
const map = {
  name: 'Isolated supplemental fog witnesses', w: 96, h: 64,
  rows: Array(64).fill('.'.repeat(96)), heights: Array(64).fill('0'.repeat(96)),
  spawns: [{ x: 2, y: 2 }, { x: 2, y: 61 }, { x: 93, y: 61 }],
  points: [{ x: 48, y: 58 }],
};
const cell = (x, z) => Math.floor(z / sim.CELL) * map.w + Math.floor(x / sim.CELL);
function create() {
  const random = Math.random;
  Math.random = () => 0.5;
  let g;
  try {
    g = sim.createGame(map, ['A', 'Ally', 'Enemy'], false, [0, 0, 1], [0, 1, 2],
      { mode: 'conquest', weather: 'clear', tech: true, logistics: false });
  } finally { Math.random = random; }
  g.units.clear(); g.nextId = 1;
  return g;
}
function spawn(g, owner, x, z, extra = {}) {
  const u = sim.internals.spawnUnit(g, owner, 'rifle');
  Object.assign(u, { x, z, rot: 0, still: 0, shotAt: undefined, dust: undefined }, extra);
  return u;
}
function input(g) {
  const state = trim(g); delete state.fog;
  return encode(state, { runs: true });
}
const clone = value => decode(structuredClone(value));
function observe(g) {
  return [0, 1].map(team => {
    const f = sim.teamFog(g, team);
    return { team, vis: [...f.vis], explored: [...f.explored], version: f.version, deltas: [...f.deltas] };
  });
}
function advance(g, name, tick) {
  g.tick = tick;
  const game = input(g);
  rebuildGrid(g); sim.internals.updateVision(g);
  const postVisionInput = input(g), expected = observe(g);
  const coldExpected = observe(clone(postVisionInput));
  for (let team = 0; team < 2; team++) assert.deepEqual(expected[team].vis, coldExpected[team].vis, name);
  return { name, game, postVisionInput, visible: g.players.map(p => [...p.visible]), expected, coldExpected };
}
const differing = (a, b) => a.flatMap((v, c) => v === b[c] ? [] : [c]);
function assertStablePacket(before, after) {
  assert.deepEqual(before.expected, after.expected);
  assert.deepEqual(before.visible, after.visible);
}

// Real visibility comes only from fresh dust. The two ordinary allied sources
// are farther than their open-ground 36 m circles from the target cell.
const dustGame = create();
const observer = spawn(dustGame, 0, 41, 41);
const ally = spawn(dustGame, 1, 41, 101);
const target = spawn(dustGame, 2, 83, 41, { dust: 101 });
const targetCell = cell(target.x, target.z), dustFrames = [];
let priorControl;
for (const [name, tick] of [['cold', 101], ['warm', 105], ['stable-cache', 109]]) {
  const frame = advance(dustGame, name, tick);
  assert.equal(dustGame.smokes.length, 0); assert.equal(dustGame.strikes.length, 0);
  assert(dustGame.players[0].visible.has(target.id));
  assert(dustGame.players[1].visible.has(target.id));
  assert(tick - target.dust >= 0 && tick - target.dust < 20);
  assert(sim.los(dustGame, observer, target));
  const own = [...dustGame.units.values()].filter(u => dustGame.players[u.owner].team === 0);
  assert.deepEqual(own.map(u => u.id), [observer.id, ally.id]);
  frame.ordinarySources = own.map(source => {
    // Explicit diagnostic input: one source, no visible enemies. This measures
    // that source's actual teamFog mask, not an admitted match transition.
    const one = clone(frame.postVisionInput);
    one.units = new Map([[source.id, one.units.get(source.id)]]);
    one.players.forEach(p => { p.visible = new Set(); });
    const sourceInput = input(one), expected = observe(one);
    assert.equal(expected[0].vis[targetCell], 0, `ordinary source ${source.id}`);
    return { kind: 'counterfactual-single-source-mask', source: source.id, game: sourceInput, expected };
  });
  // This counterfactual removes only one target's team-visible fact. Physical
  // units, dust, observers, terrain and time remain equal to postVisionInput.
  const control = clone(frame.postVisionInput);
  for (const p of control.players) if (p.team === 0) p.visible.delete(target.id);
  const controlInput = input(control), restored = clone(controlInput);
  for (const p of restored.players) if (p.team === 0) p.visible.add(target.id);
  assert.deepEqual(input(restored), frame.postVisionInput);
  if (priorControl) for (const key of ['fog', 'fogSources', 'fogTerrain']) control[key] = priorControl[key];
  const expected = observe(control), coldExpected = observe(clone(controlInput));
  for (let team = 0; team < 2; team++) assert.deepEqual(expected[team].vis, coldExpected[team].vis);
  assert.equal(expected[0].vis[targetCell], 0);
  assert.equal(frame.expected[0].vis[targetCell], 1);
  const changedCells = differing(expected[0].vis, frame.expected[0].vis);
  assert.deepEqual(changedCells, [targetCell]);
  frame.control = {
    kind: 'counterfactual-remove-only-target-visible-fact', admittedMatch: false,
    game: controlInput, expected, coldExpected, changedCells,
  };
  priorControl = control; dustFrames.push(frame);
}
assertStablePacket(dustFrames[0], dustFrames[1]);
assertStablePacket(dustFrames[1], dustFrames[2]);
for (const id of [observer.id, ally.id]) assert(dustGame.fogSources.get(id).cells.length > 0);

// A separate open, flat scene warms the same source cache before one height edit.
// Canonical base footprints remain outside the observer's complete sight domain.
// No smoke, recon or enemy cell contribution masks the height edit.
const heightGame = create(), heightObserver = spawn(heightGame, 0, 41, 41);
const heightFrames = [];
for (const [name, tick] of [['cold', 201], ['warm', 205], ['stable-cache', 209]]) {
  heightFrames.push(advance(heightGame, name, tick));
}
assertStablePacket(heightFrames[0], heightFrames[1]);
assertStablePacket(heightFrames[1], heightFrames[2]);
assert(heightGame.fogSources.get(heightObserver.id).cells.length > 0);
const editCell = cell(49, 41), probeCell = cell(65, 41);
const beforeInput = input(heightGame);
assert.equal(heightGame.height[editCell], 0);
assert.equal(heightGame.flags[editCell] & sim.SIGHT, 0);
// Source's ordinary rifle fogReach reduces to max(6, vision * sightMul) under
// these checked clear-ground observer facts. Keep every distant base intact.
assert.equal(heightObserver.type, 'rifle');
assert.equal(heightObserver.garrison, -1);
assert.equal(heightGame.height[cell(heightObserver.x, heightObserver.z)], 0);
assert.equal(heightGame.weather.now, 'clear');
assert.equal(heightGame.wx.rain, 0);
assert.equal(sightMul(heightGame), 1);
const sightReach = Math.max(6, sim.UNITS[heightObserver.type].vision * sightMul(heightGame));
assert.equal(sightReach, 36);
const bases = [
  { id: 10, owner: 0, x: 5, z: 5, c: 97 },
  { id: 11, owner: 0, x: 17, z: 5, c: 103 },
  { id: 12, owner: 1, x: 5, z: 123, c: 5761 },
  { id: 13, owner: 1, x: 17, z: 123, c: 5767 },
  { id: 14, owner: 2, x: 187, z: 123, c: 5852 },
  { id: 15, owner: 2, x: 175, z: 123, c: 5846 },
];
const canonicalCells = bases.flatMap(base => Array.from({ length: 9 }, (_, i) => base.c + i % 3 + Math.floor(i / 3) * heightGame.w));
const flaggedCells = [...heightGame.flags].flatMap((flags, c) => flags & (sim.SIGHT | sim.WOOD) ? [c] : []);
assert.deepEqual(flaggedCells, canonicalCells.toSorted((a, b) => a - b));
assert.equal(flaggedCells.length, 54);
assert.equal(heightGame.structures.size, 6);
assert.equal(heightGame.structuralCells.size, 54);
assert.equal(heightGame.buildingCells.size, 54);
let minimumRectangleDistance = Infinity;
const exceptions = flaggedCells.map(c => {
  const x = c % heightGame.w, y = Math.floor(c / heightGame.w);
  const dx = Math.max(x * sim.CELL - heightObserver.x, 0, heightObserver.x - (x + 1) * sim.CELL);
  const dz = Math.max(y * sim.CELL - heightObserver.z, 0, heightObserver.z - (y + 1) * sim.CELL);
  const distance = Math.hypot(dx, dz);
  assert(distance > sightReach, `entire flagged cell ${c} outside sight domain`);
  minimumRectangleDistance = Math.min(minimumRectangleDistance, distance);
  assert.equal(heightGame.flags[c], 1027); assert.equal(heightGame.flags[c] & sim.WOOD, 0);
  assert.equal(heightGame.chars[c], 'K'); assert.equal(heightGame.ground[c], '.'); assert.equal(heightGame.objects[c], 'K');
  const section = heightGame.structuralCells.get(c), building = heightGame.buildingCells.get(c);
  const base = bases.find(b => b.id === building?.id);
  assert(base); assert.equal(building.owner, base.owner); assert.equal(building.x, base.x); assert.equal(building.z, base.z);
  assert.equal(section.id, `building:${base.id}:${c}`); assert.equal(section.entityId, base.id);
  assert.equal(section.structureId, `building:${base.id}`); assert.equal(section.c, c);
  assert.equal(section.state, 'intact'); assert(section.hp > 0);
  const structure = heightGame.structures.get(section.structureId);
  assert.equal(structure.entityId, base.id); assert.equal(structure.kind, 'building');
  assert.equal(structure.sections.length, 9); assert(structure.sections.includes(section));
  assert.equal(heightGame.units.has(base.id), false);
  return { cell: c, distance, flags: heightGame.flags[c], chars: heightGame.chars[c],
    ground: heightGame.ground[c], object: heightGame.objects[c], section: structuredClone(section), building: structuredClone(building) };
});
for (const c of [editCell, probeCell]) assert.equal(heightGame.flags[c] & (sim.SIGHT | sim.WOOD), 0);
const sightDomain = { reach: sightReach, minimumRectangleDistance, bases, exceptions, structures: structuredClone([...heightGame.structures.values()]) };
heightGame.height[editCell] = 2;
heightGame.terrainVersion = (heightGame.terrainVersion ?? 0) + 1;
const after = advance(heightGame, 'height-only-edit', 213);
const restoredHeight = clone(after.game), previous = clone(beforeInput);
restoredHeight.height[editCell] = previous.height[editCell];
restoredHeight.tick = previous.tick;
restoredHeight.visionTick = previous.visionTick;
restoredHeight.terrainVersion = previous.terrainVersion;
assert.deepEqual(input(restoredHeight), beforeInput, 'no other scene input changes');
const before = heightFrames.at(-1).expected[0], changed = after.expected[0];
assert.equal(before.vis[probeCell], 1); assert.equal(changed.vis[probeCell], 0);
const changedCells = differing(before.vis, changed.vis);
assert(changedCells.length > 0); assert(changedCells.includes(probeCell));
assert.equal(changed.version, before.version + 1);
assert(changed.deltas.some(([version, delta]) => version === before.version && delta.length > 0));
heightFrames.push(after);

preflight();
const content = JSON.stringify({
  pin, sourceBlob: git('rev-parse', `${pin}:shared/sim.js`), node: process.version, arch: process.arch,
  historical, scope: 'isolated-component-witnesses-with-explicit-counterfactuals',
  seenGround: { target: target.id, targetCell, frames: dustFrames },
  heightOnly: { observer: heightObserver.id, editCell, oldHeight: 0, newHeight: 2, probeCell, changedCells, sightDomain, frames: heightFrames },
}) + '\n';
const output = resolve(root, 'rust/crates/simtest/tests/fixtures/vision-fog-witnesses.json');
if (process.argv.includes('--check')) assert.equal(readFileSync(output, 'utf8'), content);
else writeFileSync(output, content);
console.log('supplemental CMB-131: 3 dust/control frames, 2 per-source masks per frame, 4 height frames');
