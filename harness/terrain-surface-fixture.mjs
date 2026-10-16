// Record original rendered cell centres and frozen water lines, without changing the reference.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
assert.ok(process.argv.slice(2).every(a => a === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const { createRelief } = await import(pathToFileURL(`${reference}/client/relief.js`));
const { roadMask } = await import(pathToFileURL(`${reference}/shared/weather.js`));
const runs = values => {
  const out = [];
  for (const value of values) {
    if (out.length && out.at(-1)[0] === value) out.at(-1)[1]++;
    else out.push([value, 1]);
  }
  return out;
};
const f32bits = value => {
  const bytes = Buffer.alloc(4); bytes.writeFloatLE(value); return bytes.readUInt32LE();
};
const cases = [];
function record(name, map, grid = map.rows, liveHeights = map.heights) {
  const mask = roadMask({ ...map, rows: grid });
  const relief = createRelief(map, grid, { low: false, isRoad: (x, y) => mask[y * map.w + x] });
  if (liveHeights !== map.heights) {
    map.heights = liveHeights;
    relief.update(Array.from({ length: map.w * map.h }, (_, c) => c));
  }
  const heights = [], water = [];
  for (let y = 0; y < map.h; y++) for (let x = 0; x < map.w; x++) {
    heights.push(f32bits(relief.hAt((x + 0.5) * 2, (y + 0.5) * 2)));
    const line = relief.waterAt((x + 0.5) * 2, (y + 0.5) * 2);
    water.push(Number.isNaN(line) ? null : line);
  }
  const row = { name, height: runs(heights), water: runs(water) };
  if (!name.startsWith('authored:')) row.map = map, row.grid = grid, row.liveHeights = liveHeights;
  cases.push(row);
  relief.dispose();
}
for (const file of readdirSync(`${reference}/maps`).filter(f => f.endsWith('.json')).sort()) {
  record(`authored:${file.slice(0, -5)}`, JSON.parse(readFileSync(`${reference}/maps/${file}`, 'utf8')));
}
function synthetic(name, cell, level, edit = null) {
  const map = { name: name.replace(/^synthetic:/, ''), w: 20, h: 20,
    rows: Array.from({ length: 20 }, (_, y) => Array.from({ length: 20 }, (_, x) => cell(x, y)).join('')),
    heights: Array.from({ length: 20 }, (_, y) => Array.from({ length: 20 }, (_, x) => String(level(x, y))).join('')),
    spawns: [{ x: 1, y: 1 }, { x: 18, y: 18 }], points: [{ x: 2, y: 2 }] };
  const original = structuredClone(map);
  const liveHeights = edit ? map.heights.map(row => row.replaceAll('1', 'b')) : map.heights;
  record(name, map, map.rows, liveHeights);
  cases.at(-1).map = original;
}
synthetic('synthetic:trenches-roads-cliffs', (x, y) => y === 6 ? 'T' : y === 9 ? 'D' : '.', (x, y) => y < 12 ? 0 : 3);
synthetic('synthetic:channels-fords-bridge', (x, y) => x >= 5 && x <= 14 ? y === 9 ? '=' : y === 10 ? 'F' : 'W' : '.', () => 0);
synthetic('synthetic:bridge-only-and-separated-lines', (x, y) => y === 5 && x >= 5 && x <= 8 ? '=' : y === 8 && x === 5 || y === 10 && x === 7 ? 'W' : '.', (x, y) => y === 5 ? 2 : y < 10 ? 1 : 0);
synthetic('synthetic:frozen-lines-after-dig', (x, y) => x >= 5 && x <= 14 ? 'W' : '.', () => 1, true);
assert.equal(cases.filter(row => row.name.startsWith('authored:')).length, 33);
const result = `${JSON.stringify({ reference: pin, cases })}\n`;
const output = `${root}/rust/crates/core/tests/fixtures/terrain-surface.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(output, 'utf8'), result);
else writeFileSync(output, result);
clean();
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} ${cases.length} original terrain centre/water cases`);
