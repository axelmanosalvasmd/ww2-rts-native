// MAP49 only. Root must separately grant execution at the frozen head.
// Original callbacks and helpers are evaluated unchanged; no fixture is written.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.equal(process.argv.length, 2);
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], {
  cwd: root, encoding: 'utf8',
}).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const sim = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const fixtures = await import(pathToFileURL(resolve(reference, 'test-fixtures.js')));
const { segmentCliffClear } = await import(pathToFileURL(resolve(reference, 'shared/vehicle-motion.js')));
const source = readFileSync(resolve(reference, 'test.js'), 'utf8');
const section = (start, end) => {
  assert.equal(source.split(start).length, 2, `unique ${start}`);
  const a = source.indexOf(start), b = source.indexOf(end, a + start.length);
  assert(b > a);
  return source.slice(a, b);
};
const helpers = section('const blank = ', '\nawait check(');
const elevation = section('const ridge = ', '\nawait check("test.js:638"');
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
let original = 0;
for (const name of ['test.js:646', 'Depressions']) {
  const body = section(`await check(${JSON.stringify(name)},`, '\n});') + '\n});';
  const check = async (actual, fn) => {
    assert.equal(actual, name);
    await fn();
    original++;
  };
  await new AsyncFunction('bindings', `with (bindings) {\n${helpers}\n${elevation}\n${body}\n}`)({
    ...sim, sim, assert, check,
    command: fixtures.fixtureCommand, clearFixtureUnits: fixtures.clearFixtureUnits,
  });
}
assert.equal(original, 2);

// Both endpoints are level zero. Each side cell independently decides whether
// the exact diagonal may pass. Negative heights exercise drops as well as rises.
let corners = 0;
for (const [east, south, expected] of [
  [0, 0, true], [1, 1, true], [2, 0, false], [0, 2, false],
  [-2, 0, false], [0, -2, false], [1, -1, true], [2, 2, false],
]) {
  const height = new Int8Array(400);
  height[9 * 20 + 10] = east;
  height[10 * 20 + 9] = south;
  const g = { w: 20, h: 20, height };
  assert.equal(segmentCliffClear(g, { x: 19, z: 19 }, { x: 21, z: 21 }, sim.CELL),
    expected, `diagonal ${east}/${south}`);
  corners++;
}
assert.equal(corners, 8);
clean();
console.log('MAP49: 2 complete original checks (5 assertions), 8 diagonal corner cases');
