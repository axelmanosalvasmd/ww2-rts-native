// MODE-14 Source observations. This does not pair Source global RNG with native purpose streams.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const sha = text => createHash('sha256').update(text).digest('hex');
const git = args => execFileSync('git', args, { cwd: reference, encoding: 'utf8' }).trim();
assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
assert.equal(git(['rev-parse', 'HEAD']), pin);
assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
const source = readFileSync(`${reference}/shared/sim.js`, 'utf8');
const sourceHash = '7831cff6eb9df90f5c73cc849636c968599dd73f9ad86a2f93b3da17b4ca0207';
assert.equal(sha(source), sourceHash);
const anchor = '  return (shuffle ? good[Math.floor(Math.random() * good.length)] : good[0]).slots;';
assert.equal(source.split(anchor).length, 2);
// The observer copies the complete original local input/candidate state before the unchanged return.
// It neither chooses slots nor consumes randomness. The reference checkout is never written.
const observer = '  __mode14Layouts.push(structuredClone({ nSpawns, teams, shuffle, dist, all, best, close, nearest, good }));\n';
registerHooks({ load(url, context, next) {
  const loaded = next(url, context);
  if (url !== pathToFileURL(`${reference}/shared/sim.js`).href) return loaded;
  assert.equal(sha(String(loaded.source)), sourceHash);
  return { ...loaded, source: 'export const __mode14Layouts = [];\n' + source.replace(anchor, observer + anchor) };
} });
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const bits = n => { const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex'); };
const encode = v => typeof v === 'number' ? bits(v) : Array.isArray(v) ? v.map(encode) : v && typeof v === 'object' ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, encode(x)])) : v;
function packObservation(o) {
  const candidate = c => ({ score: bits(c.score), near: bits(c.near), slots: [...c.slots] });
  return {
    nSpawns: o.nSpawns, teams: [...o.teams], shuffle: o.shuffle,
    dist: o.dist.map(row => row.map(bits)), all: o.all.map(candidate),
    best: bits(o.best), close: o.close.map(candidate), nearest: bits(o.nearest), good: o.good.map(candidate),
  };
}
let observedLayoutCalls = 0, originalConstructors = 0;
function takeObservation() {
  assert.equal(sim.__mode14Layouts.length, 1, 'exactly one original spawnSlots observation');
  observedLayoutCalls++;
  return sim.__mode14Layouts.splice(0, 1)[0];
}
function explicitRandom(fraction, run) {
  const previous = Math.random;
  let draws = 0;
  Math.random = () => { draws++; return fraction; };
  try { return { value: run(), draws }; } finally { Math.random = previous; }
}
function ring(teams, fraction = null) {
  const { value: slots, draws } = explicitRandom(fraction, () => sim.spawnSlots(6, teams, fraction !== null));
  assert.equal(draws, fraction === null ? 0 : 1);
  const observation = takeObservation();
  return { teams, fraction, slots, observation: packObservation(observation) };
}
const ringFixed = [ring([0, 1]), ring([0, 1, 0, 1, 0, 1]), ring([0, 1, 2])];
assert.deepEqual(ringFixed[0].slots, [0, 3]);
const around = (a, b) => Math.min(Math.abs(a - b), 6 - Math.abs(a - b));
const sides = ringFixed[1].slots;
assert.equal(around(sides[0], sides[2]) + around(sides[2], sides[4]) + around(sides[0], sides[4]), 4);
const ffa = ringFixed[2].slots;
assert.ok(around(ffa[0], ffa[1]) === 2 && around(ffa[1], ffa[2]) === 2);
const ringShuffled = Array.from({ length: 20 }, (_, i) => ring([0, 0, 1, 1, 2, 2], i / 20));
for (const c of ringShuffled) assert.equal(new Set(c.slots).size, 6);
const walking = [2, 1].map(level => {
  const map = { w: 10, h: 5, rows: Array(5).fill('.'.repeat(10)), heights: Array(5).fill('00000' + String(level).repeat(5)), spawns: [{ x: 1, y: 2 }, { x: 8, y: 2 }], points: [] };
  const distances = sim.spawnDistances(map, [0, 1]);
  assert.equal(distances[0][1], level === 2 ? 28 : 7);
  return { map, indices: [0, 1], distances: distances.flat().map(bits) };
});
function construct(map, teams, shuffle, fraction) {
  assert.equal(sim.validateMap(map), null);
  const names = teams.map((_, i) => `P${i}`), factions = teams.map((_, i) => i % 4);
  const { value: game, draws } = explicitRandom(fraction, () => sim.createGame(map, names, shuffle, teams, factions, { mode: 'conquest', army: 'standard', weather: 'clear', weatherSeed: 31, tech: true, supply: false, logistics: false, sandbox: false }));
  assert.equal(draws, shuffle ? 2 : 1, 'layout plus initial environment input, or environment only');
  const observation = takeObservation();
  const usable = sim.spawnsFor(map, 'conquest');
  const slots = game.players.map(p => {
    const index = map.spawns.findIndex(s => p.spawn.x === (s.x + 0.5) * sim.CELL && p.spawn.z === (s.y + 0.5) * sim.CELL);
    assert.ok(usable.includes(index));
    return index;
  });
  assert.ok(observation.good.some(c => c.slots.every((slot, i) => usable[slot] === slots[i])));
  originalConstructors++;
  // No constructor actor is cleared, moved, held or otherwise patched.
  return {
    slots, draws, fraction,
    players: game.players.map(p => ({ slot: p.slot, name: p.name, team: p.team, faction: p.faction, spawn: encode(p.spawn) })),
    constructorActorAnchors: [...game.units.values()].map(u => [u.id, u.owner, u.type, bits(u.x), bits(u.z)]),
    observation: packObservation(observation),
  };
}
const maps = [];
for (const file of readdirSync(`${reference}/maps`).filter(n => n.endsWith('.json')).sort()) {
  const text = readFileSync(`${reference}/maps/${file}`, 'utf8'), map = JSON.parse(text);
  assert.ok(!map.world && !map.scenario, 'this batch supports only original ordinary-map Conquest constructors');
  const usable = sim.spawnsFor(map, 'conquest'), distances = sim.spawnDistances(map, usable).flat().map(bits);
  const cases = [];
  for (let count = 2; count <= 6; count++) {
    const rosters = [Array.from({ length: count }, (_, i) => i), Array.from({ length: count }, (_, i) => i % 2)];
    const seen = new Set();
    for (const teams of rosters) {
      if (seen.has(teams.join(','))) continue;
      seen.add(teams.join(','));
      const fixed = construct(map, teams, false, 0.731), shuffled = construct(map, teams, true, 0.731);
      assert.deepEqual(fixed.observation.dist.flat(), distances);
      assert.deepEqual(shuffled.observation.dist.flat(), distances);
      const withoutShuffle = c => { const copy = structuredClone(c); delete copy.shuffle; return copy; };
      assert.deepEqual(withoutShuffle(fixed.observation), withoutShuffle(shuffled.observation));
      assert.deepEqual(fixed.slots, fixed.observation.good[0].slots.map(k => usable[k]));
      cases.push({ teams, fixed, shuffled });
    }
  }
  assert.equal(cases.length, 9);
  maps.push({ name: file, map, sourceMapSha256: sha(text), usable, distances, cases });
}
assert.equal(maps.length, 33);
const pegasus = maps.find(m => m.name === 'pegasus-bridge.json');
const pegasusConstructors = Array.from({ length: 20 }, (_, i) => {
  const result = construct(pegasus.map, [0, 1, 0, 1], true, i / 20);
  const north = i => pegasus.map.spawns[result.slots[i]].y < pegasus.map.h / 2;
  assert.equal(north(0), north(2));
  assert.notEqual(north(0), north(1));
  return result;
});
assert.equal(sim.__mode14Layouts.length, 0);
assert.equal(observedLayoutCalls, 637);
assert.equal(originalConstructors, 614);
const fixture = {
  format: 1, reference: pin, node: process.version, arch: process.arch,
  kind: 'original-source-candidate-sets-not-seeded-whole-match-equality',
  sourceHash, observerAnchorSha256: sha(anchor),
  nativeSeeds: [701, 702], walking, ringFixed, ringShuffled, pegasusConstructors, maps,
  groups: { walking: 2, ringFixed: 3, ringShuffled: 20, pegasusConstructors: 20, maps: 33, mapRosters: 297, mapConstructors: 594, originalConstructors, observedLayoutCalls },
};
const target = `${root}/rust/crates/simtest/tests/fixtures/mode14-spawn-layout.json`;
const data = JSON.stringify(fixture) + '\n';
if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), data);
else writeFileSync(target, data);
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} MODE14: 2 walking inputs, 23 ring layouts, 20 Pegasus constructors, 33 maps/297 rosters/594 map constructors. Seeded whole-Match equality remains held.`);
