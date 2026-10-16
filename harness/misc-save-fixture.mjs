// Execute the pinned server's save projection in memory, without starting a server or writing maps.
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
const source = readFileSync(`${reference}/server.js`, 'utf8');
const begin = '  const clean = { name: map.name';
const end = '  await writeFile(join(MAPS, name +';
assert.equal(source.split(begin).length, 2);
assert.equal(source.split(end).length, 2);
const projection = source.slice(source.indexOf(begin), source.indexOf(end));
const project = new Function('map', `${projection}\nreturn clean;`);
const limit = source.match(/const MAX_MAP_BYTES = ([^;]+);/);
assert.ok(limit);
const maxBytes = new Function(`return ${limit[1]};`)();
const { validateMap, CFG } = await import(pathToFileURL(`${reference}/shared/sim.js`));
const { migrateWorldMap } = await import(pathToFileURL(`${reference}/shared/world-layers.js`));
const cases = [];
function record(name, map, synthetic = false) {
  assert.equal(validateMap(map), null, `${name}: valid input`);
  const saved = JSON.parse(JSON.stringify(project(map)));
  assert.equal(validateMap(saved), null, `${name}: valid saved projection`);
  const row = { name, saved };
  if (synthetic) row.map = map;
  cases.push(row);
}
for (const file of readdirSync(`${reference}/maps`).filter(f => f.endsWith('.json')).sort()) {
  record(`authored:${file.slice(0, -5)}`, JSON.parse(readFileSync(`${reference}/maps/${file}`, 'utf8')));
}
let map = { name: 'Save field matrix', w: 20, h: 20, rows: Array(20).fill('.'.repeat(20)),
  heights: Array(20).fill('0'.repeat(20)),
  spawns: [{ x: 1, y: 1, assault: false, extra: 'drop' }, { x: 18, y: 18 }, { x: 18, y: 1, assault: true }],
  points: [{ id: 'target', x: 2, y: 2, vp: 0, mp: null, kind: 'depot', needs: 1, extra: 'drop' }, { x: 3, y: 3 }],
  pointSequence: 2, defend: [0], assaultTime: 600, naval: true, trenchFacing: true,
  scenario: { version: 1, areas: [], triggers: [] },
  triggers: [{ at: 0, say: 'Ready', blow: [10, 10, 10, 10], extra: 'drop' }],
  buildings: [{ x: 10, y: 10, kind: CFG.houses[2].name, extra: 'drop' }],
  structures: [{ id: 'house', kind: 'house', extra: 'drop', sections: [
    { id: 'anchor', c: 210, hp: 321, material: 'steel', anchor: true, supports: [], extra: 'drop' }] }],
  mood: 'snow', weather: 'snow', extra: { private: 'drop' } };
map.rows[10] = '.'.repeat(10) + 'BN' + '.'.repeat(8);
map = migrateWorldMap(map);
map.layers.extra = 'drop';
for (const key of ['materials', 'groundMaterials', 'objectMaterials', 'mineMaterials']) {
  map.layers[key] = [{ c: key === 'mineMaterials' ? 211 : 210, material: 'steel', extra: 'drop' }];
}
record('synthetic:all-fields-and-nested-extras', map, true);
map = { name: 'Save absent fields', w: 20, h: 20, rows: Array(20).fill('.'.repeat(20)),
  spawns: [{ x: 1, y: 1, assault: false }, { x: 18, y: 18 }], points: [{ x: 2, y: 2 }],
  naval: false, trenchFacing: false, buildings: [], triggers: [], structures: [] };
record('synthetic:absent-empty-false-defaults', map, true);
assert.equal(cases.length, 35);
const output = `${JSON.stringify({ reference: pin, maxBytes, cases })}\n`;
const path = `${root}/rust/crates/core/tests/fixtures/misc-map-save.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), output);
else writeFileSync(path, output);
clean();
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} ${cases.length} original in-memory map save projections`);
