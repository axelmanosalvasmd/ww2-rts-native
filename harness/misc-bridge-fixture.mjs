// Capture bridge failure and railOpen directly from the read-only prototype.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? join(root, 'reference/ww2-rts'));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
assert.equal(git(['rev-parse', 'HEAD']), pin);
assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const temp = mkdtempSync(join(tmpdir(), 'ww2-misc-bridge-'));
const random = Math.random;
try {
  let code = readFileSync(join(reference, 'shared/sim.js'), 'utf8');
  const marker = '  setupStructures(g, map);';
  assert.equal(code.split(marker).length, 2);
  code = code.replace(marker, marker + '\n  return g;')
    .replace(/from '(\.\/[^']+)'/g, (_, path) => `from '${pathToFileURL(join(reference, 'shared', path)).href}'`);
  code += '\nexport { railOpen };\n';
  writeFileSync(join(temp, 'sim.mjs'), code);
  const sim = await import(pathToFileURL(join(temp, 'sim.mjs')));
  Math.random = () => 0.5;
  const w = 60, c = x => 25 * w + x;
  const map = { name: 'Bridge failure and rail connectivity', w, h: w,
    rows: Array.from({ length: w }, (_, y) => '.'.repeat(24) + (y === 25 ? '===W=' : 'WWWWW') + '.'.repeat(31)),
    heights: Array(w).fill('0'.repeat(w)),
    spawns: [{ x: 3, y: 3 }, { x: 56, y: 56 }], points: [{ x: 20, y: 25 }],
    structures: [{ id: 'span', kind: 'bridge', sections: [24, 25, 26, 28].map(x => ({
      id: `deck${x}`, c: c(x), hp: 200, material: 'wood', anchor: x === 24 || x === 28,
      supports: x === 25 || x === 26 ? [`deck${x - 1}`] : [],
    })) }],
  };
  assert.equal(sim.validateMap(map), null);
  const g = sim.createGame(map, [], false, [], [], { weather: false, logistics: false });
  g.tick = 10; g.shots = []; g.seed = 123456789;
  const units = ['rifle', 'tank', 'rifle'].map((type, i) => ({
    id: 901 + i, type, owner: 0, x: [51, 53, 57][i], z: 51,
    hp: sim.UNITS[type].hpPer * sim.UNITS[type].models, path: [], garrison: -1,
  }));
  for (const u of units) g.units.set(u.id, structuredClone(u));
  const lines = [
    { a: 0, b: 1, cells: [c(24), c(25), c(26)], regions: [0, 1], bridges: [c(24), c(25), c(26)] },
    { a: 1, b: 2, cells: [c(28)], regions: [1, 2], bridges: [c(28)] },
    { a: 2, b: 3, cells: [c(10)], regions: [2, 3], bridges: [] },
  ];
  g.world = { regions: [0, 0, 0, 1].map(team => ({ team })) };
  const snapshot = () => ({
    cells: [24, 25, 26, 28].map(x => { const s = g.structuralCells.get(c(x)); return [c(x), g.chars[c(x)], s.hp, s.state, g.cellHp[c(x)]]; }),
    units: units.map(u => g.units.get(u.id).hp),
    open: [0, 1].map(team => lines.map(line => sim.railOpen(g, line, team))),
    collapses: g.shots.filter(s => s.k === 'collapse').map(s => s.section),
  });
  const stages = [snapshot()];
  for (const damage of [199, 1]) { sim.damageWorldSection(g, c(25), damage); stages.push(snapshot()); }
  const output = JSON.stringify({ pin, map, units, lines, teams: [0, 0, 0, 1], stages }) + '\n';
  const path = join(root, 'rust/crates/core/tests/fixtures/misc-bridge.json');
  if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), output);
  else writeFileSync(path, output);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
  process.stdout.write('Bridge fixture: 3 stages, 3 rail lines, 2 teams; pin clean.\n');
} finally { Math.random = random; rmSync(temp, { recursive: true, force: true }); }
