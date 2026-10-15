// Record the original two-starting-rifle command case and its checked route hash.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const reference = resolve(process.env.WW2_REFERENCE ?? fileURLToPath(new URL('../reference/ww2-rts/', import.meta.url)));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const oracle = '8c0b031cc901b56c8dafc8d6ad52a3fed4ae8480457ad5ab3bc86f56cdc6271a';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
function clean() {
  assert.equal(git(['rev-parse', 'HEAD']), pin, 'pinned Source');
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '', 'clean Source');
}
assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
clean();
const original = readFileSync(resolve(reference, 'test-path-performance.mjs'), 'utf8');
const helperEnd = original.indexOf('\n\n{\n  const map = mapWith();');
assert.ok(helperEnd > 0, 'original helper boundary');
const selectedStart = original.indexOf('\n{\n  const g = newGame(mapWith());\n  const [first, second]');
assert.ok(selectedStart > helperEnd, 'original selected command case');
const selectedEnd = original.indexOf('\n}\n', selectedStart) + 2;
assert.ok(selectedEnd > selectedStart, 'original selected case end');
const sourceSelectedBlock = original.slice(selectedStart, selectedEnd);
// Only module specifiers and export visibility change. All original helper bodies stay intact.
const helpers = original.slice(0, helperEnd).replace(/from '(\.\/.+?)'/g,
  (_, relative) => `from '${pathToFileURL(resolve(reference, relative)).href}'`)
  + '\nexport { newGame, mapWith, routeOracle, expectRoute, command, UNITS };\n';
const source = await import(`data:text/javascript;base64,${Buffer.from(helpers).toString('base64')}`);
assert.equal(source.routeOracle.groupMove, oracle);
const map = source.mapWith();
// Source startup consumes ambient randomness. This deterministic adapter is scoped to startup;
// it changes no Source function, terrain fact, command or path search.
const savedRandom = Math.random;
let g;
try {
  Math.random = () => 0.5;
  g = source.newGame(map);
} finally {
  Math.random = savedRandom;
}
const dv = new DataView(new ArrayBuffer(8));
function bits(value) {
  dv.setFloat64(0, value);
  return dv.getBigUint64(0).toString(16).padStart(16, '0');
}
const initial = [...g.units.values()].map(u => ({ id: u.id, type: u.type, owner: u.owner,
  x: bits(u.x), z: bits(u.z), hp: bits(u.hp), rot: bits(u.rot) }));
assert.ok(initial.every(u => !source.UNITS[u.type].building), 'original clearFixtureUnits removed buildings');
assert.equal(g.buildingCells.size, 0, 'original clearFixtureUnits removed footprints');
const terrain = { flags: [...g.flags], chars: g.chars.join(''), ground: g.ground.join(''),
  objects: g.objects.join(''), height: g.height ? [...g.height] : null, near: [...g.worldNearWalls] };
const [first, second] = [...g.units.values()].filter(u => u.owner === 0 && u.type === 'rifle');
assert.ok(first && second, 'original starting rifles');
const input = { t: 'move', orders: [[first.id, 73, 31], [second.id, 73, 35]] };
const result = source.command(g, 0, input);
assert.equal(result, undefined, 'group move should be accepted');
const paths = [first.path.map(p => [p.x, p.z]), second.path.map(p => [p.x, p.z])];
source.expectRoute('group move paths', paths, source.routeOracle.groupMove);
assert.equal(createHash('sha256').update(JSON.stringify(paths)).digest('hex'), oracle);
const output = JSON.stringify({ reference: pin, node: process.version, arch: process.arch,
  sourceTest: 'test-path-performance.mjs:190-195', sourceSelectedBlock,
  startupRandom: 0.5, map, tick: g.tick, initial, terrain, input,
  accepted: result === undefined, oracle, paths,
  pathBits: paths.map(path => path.map(p => p.map(bits))) }) + '\n';
clean();
const target = new URL('../rust/crates/core/tests/fixtures/mov-group-move-selected.json', import.meta.url);
if (process.argv.includes('--check')) {
  assert.equal(readFileSync(target, 'utf8'), output, 'MOV group-move selected fixture differs');
} else {
  writeFileSync(target, output);
}
console.log(JSON.stringify({ checks: 1, commandedUnits: 2, oracle }));
