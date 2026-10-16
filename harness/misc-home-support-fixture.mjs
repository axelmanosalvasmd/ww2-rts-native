// Read the pinned home selector, support prices and aircraft retreat branch.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, realpathSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = realpathSync(resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.equal(git(['rev-parse', 'HEAD']), pin);
assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const url = pathToFileURL(`${reference}/shared/sim.js`).href;
registerHooks({ load(path, context, next) {
  const result = next(path, context);
  return path === url ? { ...result, source: `${String(result.source)}\nexport { homeOf as __home, retreatUnit as __retreat };\n` } : result;
} });
const sim = await import(url);
const spawn = { x: 1, z: 1 }, actor = { id: 1, type: 'rifle', owner: 0, x: 40, z: 40 };
const units = [
  [2, 'hq', 0, 10, 100, 1], [3, 'hq', 0, 20, 100, 1],
  [4, 'barracks', 0, 38, 100, 1], [5, 'barracks', 0, 39, 100, 0.5],
  [6, 'barracks', 0, 39.5, 0, 1], [7, 'barracks', 1, 40, 100, 1],
  [8, 'rifle', 0, 40, 100, 1],
].map(([id, type, owner, x, hp, built]) => ({ id, type, owner, x, z: 40, hp, built }));
const g = { players: [{ slot: 0, team: 0, spawn }, { slot: 1, team: 1, spawn }], units: new Map(units.map(u => [u.id, structuredClone(u)])) };
const modes = [null, 'conquest', 'assault', 'annihilation', 'horde', 'classic', 'tutorial', 'other'];
const stages = [];
for (let stage = 0; stage < 3; stage++) {
  if (stage === 1) g.units.get(4).hp = 0;
  if (stage === 2) { g.units.get(2).hp = 0; g.units.get(3).built = 0.99; }
  const homes = modes.map(mode => {
    g.mode = mode ? { kind: mode, slot: 1 } : null;
    const result = sim.__home(g, actor);
    return { mode, home: { x: result.x, z: result.z } };
  });
  stages.push({ units: [...g.units.values()], homes });
  // Preserve the actual inputs independently from the next stage's mutations.
  stages[stage] = structuredClone(stages[stage]);
}
const hordePlayers = [0, 0, 1, 1].map((team, slot) => ({ slot, team, spawn }));
const hordeUnits = [
  [10, 'hq', 0, 10, 100, 1], [11, 'hq', 1, 35, 100, 1],
  [12, 'hq', 1, 39, 100, 0.5], [13, 'hq', 1, 39.5, 0, 1],
  [14, 'hq', 2, 40, 100, 1], [15, 'hq', 3, 39.75, 100, 1],
  [16, 'barracks', 1, 40, 100, 1], [17, 'hq', 1, 45, 100, 1],
].map(([id, type, owner, x, hp, built]) => ({ id, type, owner, x, z: 40, hp, built }));
const hordeHomes = [
  ['nearest-shared-allied-hq', 0, []],
  ['nearer-owned-hq', 0, [[10, { x: 38 }]]],
  ['equal-distance-insertion-order', 0, [[10, { x: 45 }]]],
  ['dead-unfinished-enemy-and-non-hq-refused', 0, [[11, { hp: 0 }], [17, { built: 0.9 }]]],
  ['horde-actor-cannot-share', 2, [[14, { x: 10 }]]],
  ['horde-owned-hq-cannot-be-shared', 3, []],
].map(([name, owner, changes]) => {
  const units = new Map(hordeUnits.map(u => [u.id, structuredClone(u)]));
  for (const [id, change] of changes) Object.assign(units.get(id), change);
  const actor = { id: 1, type: 'rifle', owner, x: 40, z: 40 };
  const game = { players: hordePlayers, units, mode: { kind: 'horde', slot: 2 } };
  const result = sim.__home(game, actor);
  return { name, actor, players: hordePlayers, hordeSlot: 2, units: [...units.values()], home: { x: result.x, z: result.z } };
});
const support = Object.entries(sim.SUPPORT).map(([name, def]) => ({ name, mp: def.cost, mun: def.mun, cd: def.cd, delay: def.delay }));
const aircraft = ['base', 'rearm', 'out', 'station', 'home'].map(prior => {
  const unit = { air: { state: prior } };
  sim.__retreat({ tick: 0 }, unit);
  return { prior, after: unit.air.state };
});
const output = JSON.stringify({ pin, actor, spawn, stages, hordeHomes, support, aircraft }) + '\n';
const destination = `${root}/rust/crates/core/tests/fixtures/misc-home-support.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(destination, 'utf8'), output);
else writeFileSync(destination, output);
assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
console.log('Home/support fixture: 30 home selections, 8 support rows, 5 aircraft states; pin clean.');
