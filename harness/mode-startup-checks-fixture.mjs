// Bounded MODE-15/19/20 constructor consumers. Unsupported mode rows remain held.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', args, { cwd: reference, encoding: 'utf8' }).trim();
assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
assert.equal(git(['rev-parse', 'HEAD']), pin);
assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
const sourceHash = createHash('sha256').update(readFileSync(`${reference}/shared/sim.js`)).digest('hex');
assert.equal(sourceHash, '7831cff6eb9df90f5c73cc849636c968599dd73f9ad86a2f93b3da17b4ca0207');
// No loader hook, copied implementation, fixture factory, actor patch or alternate scheduler.
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const maps = readdirSync(`${reference}/maps`).filter(f => f.endsWith('.json')).sort().map(name => ({ name, map: JSON.parse(readFileSync(`${reference}/maps/${name}`, 'utf8')) }));
const bits = n => { const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex'); };
const anchors = g => [...g.units.values()].map(u => [u.id, u.owner, u.type, bits(u.x), bits(u.z)]);
const players = g => g.players.map(p => [p.slot, p.name, p.team, p.faction, bits(p.spawn.x), bits(p.spawn.z)]);
const points = g => g.points.map(p => [p.scenarioId, bits(p.x), bits(p.z), bits(p.vp), bits(p.mp), p.kind ?? null, p.needs, p.owner, p.capper, bits(p.progress), p.cut]);
function withRandom(run) {
  const before = Math.random;
  Math.random = () => 0.5;
  try { return run(); } finally { Math.random = before; }
}
function make(map, teams, mode, shuffle) {
  assert.equal(sim.validateMap(map), null);
  return withRandom(() => sim.createGame(map, teams.map((_, i) => `P${i}`), shuffle, teams, teams.map((_, i) => i % 3), { mode, defenderTeam: 1, weather: 'clear', weatherSeed: 31, supply: true, tech: false, logistics: false, sandbox: false, army: 'standard' }));
}
const defendCycles = [];
for (const input of maps.filter(m => m.map.defend?.length)) {
  for (const teams of [[0, 1, 0, 1, 0, 1], [1, 0, 1, 0, 1, 0]]) for (const shuffle of [false, true]) {
    const g = make(input.map, teams, 'assault', shuffle);
    defendCycles.push({ input, teams, shuffle, expected: players(g), constructorActorAnchors: anchors(g) });
  }
}
assert.equal(defendCycles.length, 68);
const hill = maps.find(m => m.name === 'hill-112.json');
const invalidHill = { ...structuredClone(hill.map), defend: hill.map.spawns.map((_, i) => i) };
const invalidHillError = sim.validateMap(invalidHill);
assert.equal(invalidHillError, 'defend must list some (not all) spawn numbers');
const synthetic = [
  [{ x: 15, y: 8, mp: 0 }, { x: 20, y: 12, mp: 2, needs: 0 }, { x: 25, y: 16, needs: 1 }],
  [{ x: 15, y: 8, mp: 0 }, { x: 20, y: 12, mp: 0, needs: 0 }, { x: 25, y: 16, mp: 0, needs: 1 }],
  [{ id: 'kept-a', x: 15, y: 8, mp: 2 }, { id: 'removed', x: 20, y: 12, mp: 0 }, { id: 'kept-b', x: 25, y: 16, needs: 0 }],
  [{ x: 15, y: 8, mp: 2, needs: 2 }, { x: 20, y: 12, mp: 0 }, { x: 25, y: 16, mp: 3 }],
].map((points, i) => ({ name: `declared-point-links-${i}`, map: { name: `Declared point links ${i}`, w: 40, h: 30, rows: Array(30).fill('.'.repeat(40)), spawns: [{ x: 4, y: 4 }, { x: 35, y: 25 }], points } }));
const pointInputs = [...maps.filter(m => m.map.points.some(p => (p.mp ?? 1) === 0)), ...synthetic];
assert.equal(pointInputs.length, 16);
const pointCases = [];
for (const input of pointInputs) for (const mode of ['conquest', 'assault']) {
  const g = make(input.map, [0, 1], mode, false);
  pointCases.push({ input, mode, expected: points(g), constructorActorAnchors: anchors(g) });
}
assert.equal(pointCases.length, 32);
const input = maps.find(m => m.name === 'default.json');
// The entire named real-map check: construct three default players, require nine troops, run five seconds.
const g = withRandom(() => sim.createGame(input.map, ['a', 'b', 'c']));
const startTypes = g.players.map(p => [...g.units.values()].filter(u => u.owner === p.slot && !sim.UNITS[u.type].building).map(u => u.type));
assert.equal(startTypes.flat().length, 9);
const startAnchors = anchors(g);
withRandom(() => { for (let i = 0; i < 5 * 20; i++) sim.step(g); });
assert.equal(g.tick, 100);
const defaultThree = { input, startTypes, startAnchors, endAnchors: anchors(g), tick: g.tick };
const output = JSON.stringify({ format: 1, reference: pin, node: process.version, arch: process.arch, sourceHash, kind: 'bounded-actual-constructors-not-complete-mode-admission', defendCycles, invalidHill, invalidHillError, pointCases, defaultThree, groups: { defendCycles: 68, pointCases: 32, defaultThree: 1, originalConstructors: 101, originalTicks: 100 } }) + '\n';
const target = `${root}/rust/crates/simtest/tests/fixtures/mode-startup-checks.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), output);
else writeFileSync(target, output);
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} startup bounds: 68 defend cycles, 32 point projections, one complete three-player check/100 ticks. All six MODE rows remain held.`);
