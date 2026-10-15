// Execute the original five-second wind check with unchanged full Source steps.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
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
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const { clearFixtureUnits } = await import(pathToFileURL(`${reference}/test-fixtures.js`));
const source = readFileSync(`${reference}/test.js`, 'utf8');
const line = prefix => {
  const found = source.split('\n').filter(text => text.startsWith(prefix));
  assert.equal(found.length, 1, prefix);
  return found[0];
};
const helpers = ['const blank = ', 'const empty = ', 'const run = ', 'const fresh = '].map(line).join('\n');
const begin = source.indexOf('  const g = fresh(); g.players[0].mp = 5000;\n  g.wind = { a: 0, v: 1 }; g.smokes.push({ x: 10, z: 10, r: 5, t: 60 });');
assert.ok(begin >= 0);
const end = source.indexOf('  // rain: comes on', begin);
assert.ok(end > begin);
const selected = source.slice(begin, end);
assert.ok(selected.includes('run(g, 5);'));
assert.ok(selected.includes("assert.ok(g.smokes[0].x > 14 && Math.abs(g.smokes[0].z - 10) < 2, 'smoke drifts downwind');"));
const bits = value => { const buffer = Buffer.alloc(8); buffer.writeDoubleBE(value); return buffer.toString('hex'); };
const snapshot = g => ({
  seed: g.seed >>> 0,
  wind: [bits(g.wind.a), bits(g.wind.v)],
  wx: g.wx ? [g.wx.raining, ...[g.wx.next, g.wx.rain, g.wx.wet].map(bits)] : null,
  plan: structuredClone(g.weather ?? null),
  // The original cloud has no ID. Zero is an observation ordinal, not a Source ID.
  smoke: g.smokes.map((s, ordinal) => [ordinal, bits(s.x), bits(s.z), bits(s.t)]),
});
let input;
const steps = [], expected = [], startupDraws = [];
const createGame = (...args) => {
  const prior = Math.random;
  Math.random = () => { startupDraws.push(0.5); return 0.5; };
  try { return sim.createGame(...args); } finally { Math.random = prior; }
};
const observeStep = g => {
  if (!input) {
    assert.equal(g.units.size, 0);
    assert.equal(g.smokes.length, 1);
    assert.equal(Object.hasOwn(g.smokes[0], 'id'), false);
    input = {
      seed: g.seed >>> 0, wind: structuredClone(g.wind), wx: structuredClone(g.wx ?? null),
      weather: structuredClone(g.weather ?? null),
      smokes: g.smokes.map((s, ordinal) => ({ ...s, id: ordinal })),
      sourceCloudHasId: false,
      sourceActors: [...g.units.values()], sourceStructures: [...g.structures],
      sourceStructuralCells: [...g.structuralCells],
      sourcePlayers: g.players.map(p => ({ slot: p.slot, spawn: p.spawn, team: p.team })),
    };
  }
  sim.step(g);
  steps.push([g.tick, sim.TICK]);
  expected.push(snapshot(g));
};
// These are original test helper/check statements. Only the step observation is wrapped.
const execute = new Function('createGame', 'clearFixtureUnits', 'step', 'assert', `${helpers}\n${selected}\nreturn g;`);
const game = execute(createGame, clearFixtureUnits, observeStep, assert);
assert.equal(steps.length, 100);
assert.equal(game.tick, 100);
assert.equal(input.wind.a, 0);
assert.equal(input.wind.v, 1);
assert.deepEqual(input.smokes.map(({ x, z, r, t }) => ({ x, z, r, t })), [{ x: 10, z: 10, r: 5, t: 60 }]);
const output = JSON.stringify({ reference: pin, node: process.version, arch: process.arch,
  provenance: 'Original test.js blank/empty/fresh/run helpers and quantitative wind check; unchanged full Source step; constant 0.5 startup Math.random only. Native compares the existing weather component, not constructor or whole-Match RNG equivalence.',
  originalHelpers: helpers, originalCheck: selected, startupDraws,
  input, steps, expected, final: { x: game.smokes[0].x, z: game.smokes[0].z },
}) + '\n';
clean();
const destination = `${root}/rust/crates/core/tests/fixtures/smoke-wind-selected.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(destination, 'utf8'), output);
else writeFileSync(destination, output);
console.log(JSON.stringify({ checks: 1, actualSourceTicks: steps.length, radius: input.smokes[0].r, initialWind: input.wind.v, finalX: game.smokes[0].x, finalZ: game.smokes[0].z }));
