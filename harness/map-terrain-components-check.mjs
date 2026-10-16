// Check the MAP ground matrix and unchanged original named checks. Root slot only.
// This writes no fixtures and never replaces the reference's random generator.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.equal(process.argv.length, 2);
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const url = pathToFileURL(resolve(reference, 'shared/sim.js')).href;
registerHooks({ load(path, context, next) {
  const result = next(path, context);
  return path === url
    ? { ...result, source: `${result.source}\nexport { groundMul, speedMul };\n` }
    : result;
} });
const sim = await import(url);
const fixtures = await import(pathToFileURL(resolve(reference, 'test-fixtures.js')));
const bits = n => { const bytes = Buffer.alloc(8); bytes.writeDoubleBE(n); return bytes.toString('hex'); };
const exact = (actual, expected, context) => assert.equal(bits(actual), bits(expected), context);
const ordinary = [1, 0.85, 0.7], ones = [1, 1, 1], road = [1.35, 1.175, 1];
const midpoint = Buffer.from('3fe0cccccccccccc', 'hex').readDoubleBE();
const rows = [
  ['.', ordinary, ones, true], ['B', ordinary, ones, true], ['H', ordinary, ones, true],
  ['#', ordinary, ones, true], ['+', ordinary, ones, true], ['T', ordinary, [1.2, 1.2, 1.2], true],
  ['W', ordinary, ones, true], ['F', [0.75, 0.55, 0.35], [0.75, 0.55, 0.35], false],
  ['=', road, ones, false], ['R', ordinary, ones, true], ['K', ordinary, ones, true],
  ['X', ordinary, [0.35, 0.35, 0.35], true], ['Y', ordinary, ones, true], ['D', road, ones, false],
  ['M', [0.7, midpoint, 0.35], ones, true], ['N', ordinary, ones, true],
  ['O', [0.5, 0.5, 0.5], ones, false], ['A', ordinary, ones, true], ['Q', ordinary, ones, true],
];
assert.deepEqual(rows.map(row => row[0]).sort(), Object.keys(sim.TERRAIN).sort());
let ground = 0;
for (const [ch, vehicle, infantry, wetVehicle] of rows) {
  const g = { flags: [sim.TERRAIN[ch]], wear: new Float32Array(1), height: new Int8Array(1), wx: {} };
  for (const level of [0, -1]) for (const [index, wear] of [0, 0.5, 1].entries()) for (const wet of [0, 1]) {
    g.height[0] = level; g.wear[0] = wear; g.wx.wet = wet;
    const wetFactor = wet === 0 ? 1 : level < 0 ? 0.6 : 0.8;
    for (const [isVehicle, dry] of [[false, infantry[index]], [true, vehicle[index]]]) {
      const factor = ch === 'F' && wet !== 0 ? 0.7 : isVehicle && wetVehicle ? wetFactor : 1;
      exact(sim.groundMul(g, 0, isVehicle), dry * factor, `${ch}/${wear}/${wet}/${level}/${isVehicle}`);
      ground++;
    }
  }
}
assert.equal(ground, 456);
const g = { w: 20, h: 20, flags: new Uint16Array(400).fill(sim.LAND), wear: new Float32Array(400) };
const tank = { type: 'tank', x: 19, z: 19 };
for (const [c, ch] of [[190, 'D'], [188, 'M'], [209, 'F'], [169, 'O']]) g.flags[c] = sim.TERRAIN[ch];
exact(sim.speedMul(g, tank), (1 + 1.35 + 0.7 + 0.75 + 0.5) / 5, 'five cells');
g.flags[190] = sim.TERRAIN.B;
exact(sim.speedMul(g, tank), (1 + 0.7 + 0.75 + 0.5) / 4, 'wall excluded');
g.flags[188] = sim.TERRAIN.W;
exact(sim.speedMul(g, tank), (1 + 0.75 + 0.5) / 3, 'water excluded');
for (const c of [189, 209, 169]) g.flags[c] = sim.TERRAIN.B;
exact(sim.speedMul(g, tank), 1, 'all blocked');
exact(sim.speedMul(g, { ...tank, type: 'gunboat' }), 1, 'naval');
g.flags.fill(sim.LAND);
for (const [c, ch] of [[0, 'D'], [1, 'M'], [20, 'O']]) g.flags[c] = sim.TERRAIN[ch];
exact(sim.speedMul(g, { ...tank, x: 0.5, z: 0.5 }), (1.35 + 0.7 + 0.5) / 3, 'off map');
g.flags.fill(sim.LAND); g.flags[190] = sim.TERRAIN.D;
exact(sim.speedMul(g, { ...tank, x: 19.8 }), (1 + 1.35 + 1 + 1 + 1) / 5, '0.6 radius');
g.flags.fill(sim.LAND); g.wear.fill(0);
for (const [c, ch] of [[189, 'F'], [190, 'X'], [209, 'T']]) g.flags[c] = sim.TERRAIN[ch];
exact(sim.speedMul(g, { type: 'rifle', x: 19.5, z: 19.5 }),
  (0.75 + 0.35 + 0.75 + 1.2 + 0.75) / 5, 'infantry and repeated centre samples');
g.flags.fill(sim.LAND); g.flags[190] = sim.TERRAIN.R; g.wear[190] = 1;
exact(sim.speedMul(g, tank), (1 + 0.7 + 1 + 1 + 1) / 5, 'MOVE-only filter includes VBLOCK');


// Evaluate the original checks and shared helpers verbatim. The water row needs
// only its named sight assertion, so the unrelated destruction check is not run.
const source = readFileSync(resolve(reference, 'test.js'), 'utf8');
const section = (start, end) => {
  assert.equal(source.split(start).length, 2, `unique ${start}`);
  const a = source.indexOf(start), b = source.indexOf(end, a + start.length);
  assert(b > a); return source.slice(a, b);
};
const helpers = section('const blank = ', '\nawait check(');
const elevation = section('const ridge = ', '\nawait check("test.js:638"');
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
let original = 0;
for (const name of ['LOS', 'test.js:638', 'Depressions', 'Slope']) {
  const body = section(`await check(${JSON.stringify(name)},`, '\n});') + '\n});';
  const check = async (actual, fn) => { assert.equal(actual, name); await fn(); original++; };
  await new AsyncFunction('bindings', `with (bindings) {\n${helpers}\n${elevation}\n${body}\n}`)({
    ...sim, sim, assert, check, command: fixtures.fixtureCommand, clearFixtureUnits: fixtures.clearFixtureUnits,
  });
}
const rowsWater = Array.from({ length: 20 }, (_, y) => '.'.repeat(10) + (y === 5 ? '=' : y === 15 ? 'F' : 'W') + '.'.repeat(9));
// Supply the original river layout directly to the pure query. It needs the
// actual terrain flags, flat heights and empty smoke, without a match constructor.
const water = { w: 20, h: 20, flags: Uint16Array.from(rowsWater.join(''), ch => sim.TERRAIN[ch]), smokes: [] };
assert(sim.los(water, { x: 5, z: 25 }, { x: 35, z: 25 }), 'you can see across water');
assert.equal(original, 4);
clean();
console.log(`${ground} ground values, 9 footprint cases, 4 complete original checks and the original water sight assertion`);
