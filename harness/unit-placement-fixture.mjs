// Capture immediate placement fields and grid queries from the read-only prototype.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
assert.equal(git(['rev-parse', 'HEAD']), pin);
assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const path = resolve(reference, 'shared/sim.js');
let source = readFileSync(path, 'utf8');
source = source.replace(/from (['"])(\.\/[^'"]+)\1/g,
  (_, quote, relative) => `from ${quote}${new URL(relative, pathToFileURL(path)).href}${quote}`);
source += '\nexport {placeUnit,rebuildGrid};\n';
const sim = await import('data:text/javascript;base64,' + Buffer.from(source).toString('base64'));
const initial = [
  { id: 1, type: 'rifle', owner: 0, x: 15.5, z: 15.5 },
  { id: 2, type: 'tank', owner: 0, x: 33, z: 17 },
  { id: 3, type: 'rifle', owner: 1, x: 35, z: 19 },
  { id: 4, type: 'halftrack', owner: 1, x: 65, z: 65 },
].map(u => ({ ...u, rot: 0, garrison: -1, riding: 0, supp: 0, still: 0 }));
const placements = [
  { id: 1, fields: { x: 16.5, z: 16.5, garrison: 520 } },
  { id: 2, fields: { x: 17, z: 17, rot: 0.75 } },
  { id: 3, fields: { x: 16.75, z: 16.75, riding: 4, supp: 0, still: 0 } },
  { id: 1, fields: { x: 63.5, z: 64.5, garrison: -1 } },
  { id: 4, fields: { x: 17.5, z: 16.5, rot: -1.25 } },
  { id: 3, fields: { x: 17.5, z: 16.5, riding: 0, supp: 70 } },
  { id: 2, fields: { x: -0.5, z: -16.5, owner: 1 } },
  { id: 2, fields: { x: 17, z: 17, owner: 0 } },
];
const queries = [
  { x: 8, z: 8, radius: 0 }, { x: 24, z: 24, radius: 0 },
  { x: 24, z: 24, radius: 18 }, { x: 72, z: 72, radius: 0 },
  { x: -8, z: -24, radius: 0 }, { x: 32, z: 32, radius: 80 },
];
const g = { units: new Map(initial.map(u => [u.id, structuredClone(u)])) };
const grid = sim.rebuildGrid(g);
const ids = rows => rows.map(u => u.id);
const sorted = rows => ids(rows).sort((a, b) => a - b);
const snapshot = () => ({
  units: [...g.units.values()],
  owners: [0, 1].map(owner => ids(grid.ownedBy(owner))),
  queries: queries.map(at => ({
    ordered: ids(grid.inOrder(at, at.radius)),
    unordered: sorted(grid.unordered(at, at.radius)),
    heavy: sorted(grid.heavy(at, at.radius)),
    radius: ids(grid.radius(at, at.radius)),
    nearest: grid.nearest(at, at.radius)?.id ?? null,
    has: grid.has(at, at.radius), hasHeavy: grid.hasHeavy(at, at.radius),
  })),
});
const rows = [];
for (const placement of placements) {
  const unit = g.units.get(placement.id);
  assert.equal(sim.placeUnit(g, unit, placement.fields), unit);
  rows.push(structuredClone(snapshot()));
}
const output = JSON.stringify({ pin, initial, placements, queries, rows }) + '\n';
const destination = resolve(root, 'rust/crates/core/tests/fixtures/unit-placement.json');
if (process.argv.includes('--check')) assert.equal(readFileSync(destination, 'utf8'), output);
else writeFileSync(destination, output);
assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
process.stdout.write('Placement fixture: 8 placements, 6 queries after each placement; pin clean.\n');
