// Preserve complete selected Source checks. Run only with Root's recorder slot.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {readFileSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {gzipSync} from 'node:zlib';
import {encode} from './state-codec.mjs';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], {encoding: 'utf8'}).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const raw = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const fixtures = await import(pathToFileURL(resolve(reference, 'test-fixtures.js')));
const {generateWorldMap} = await import(pathToFileURL(resolve(reference, 'shared/world-conquest.js')));
const source = readFileSync(resolve(reference, 'test.js'), 'utf8');
const transport = readFileSync(resolve(reference, 'test-transport.mjs'), 'utf8');
const hash = text => createHash('sha256').update(text).digest('hex');
const section = (text, start, end) => {
  assert.equal(text.split(start).length, 2, `unique start: ${start}`);
  const a = text.indexOf(start), b = text.indexOf(end, a + start.length);
  assert.ok(b > a, `end: ${end}`);
  return text.slice(a, b);
};
const checkSource = name => section(source, `await check(${JSON.stringify(name)},`, '\n});') + '\n});';
const positionBits = g => [...g.units.values()].map(u => [u.id, bits(u.x), bits(u.z)]);
const data = new DataView(new ArrayBuffer(8));
function bits(n) {
  data.setFloat64(0, n, false);
  return data.getBigUint64(0, false).toString(16).padStart(16, '0');
}
const cases = [];
const rails = [];
let active;
function observed(op, g, input, call) {
  const before = encode(g, {runs: true});
  const result = call();
  active.calls.push({op, input: encode(input), before, after: encode(g, {runs: true}),
    positions: g.units ? positionBits(g) : null, result: encode(result)});
  return result;
}
const sim = {...raw, internals: {...raw.internals}};
sim.step = g => observed('step', g, null, () => raw.step(g));
sim.findPath = (g, from, to) => observed('findPath', g, {from, to}, () => raw.findPath(g, from, to));
sim.internals.separateCrowds = (g, units) => observed('separateCrowds', g, units.map(u => u.id),
  () => raw.internals.separateCrowds(g, units));
const command = (g, owner, cmd) => observed('command', g, {owner, cmd},
  () => fixtures.fixtureCommand(g, owner, cmd));
const common = {...sim, sim, assert, command, fixtureCommand: command,
  clearFixtureUnits: fixtures.clearFixtureUnits,
  readFileSync: path => readFileSync(resolve(reference, path), 'utf8'),
};
// Use the original shared fresh/put definitions below when evaluating test.js.
const helpers = section(source, 'const blank = ', '\nawait check(');
const massive = section(source, 'const massiveFixture = () => {', '\n// Reference before');
const separation = section(source, 'const referenceSeparation = (g) => {', '\n// Twenty passes');
const transportHelpers = section(transport, 'const map = ', "\ntest('a light tank");
const transportCheck = section(transport,
  "test('a light tank carries two squads as riders and a Tiger three',", '\n});') + '\n});';
const selected = [
  {row: 'MOV-43', name: 'Editor reachability uses terrain without a match\'s player or mine state', helpers: '',
    body: checkSource('Editor reachability uses terrain without a match\'s player or mine state')},
  {row: 'MOV-62', name: 'Twenty ticks of a seeded crowded infantry army preserve the old separation coordinates',
    helpers: `const massiveInternals = {...sim, ...sim.internals};\n${massive}\n${separation}`,
    body: checkSource('Twenty ticks of a seeded crowded infantry army preserve the old separation coordinates')},
  {row: 'MOV-62', name: 'Live infantry separation must discover later pairs brought into range by an earlier push',
    helpers, body: checkSource('Live infantry separation must discover later pairs brought into range by an earlier push')},
  {row: 'MOV-72', name: 'a light tank carries two squads as riders and a Tiger three',
    helpers: transportHelpers, body: transportCheck},
];
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const previous = Math.random;
try {
  // Global draws are fixed. The original crowd check retains its own seed 123456.
  Math.random = () => 0.5;
  for (const spec of selected) {
    active = {row: spec.row, name: spec.name, source: spec.helpers + '\n' + spec.body,
      sha256: hash(spec.helpers + '\n' + spec.body), calls: [], assertionsCompleted: false};
    let invoked = 0;
    const check = async (name, fn) => { assert.equal(name, spec.name); invoked++; await fn(); };
    const test = (name, fn) => { assert.equal(name, spec.name); invoked++; fn(); };
    // Source snippets and their helpers are unchanged. Only their imported calls are observed.
    await new AsyncFunction('bindings', `with (bindings) {\n${spec.helpers}\n${spec.body}\n}`)
      ({...common, check, test});
    assert.equal(invoked, 1);
    active.assertionsCompleted = true;
    cases.push(active);
  }
  for (const seed of [0, 1, 42, 20261003, 4294967295]) {
    const options = {size: 'huge', seed, players: 6, teams: [0, 0, 1, 1, 2, 2], scenery: false};
    const map = generateWorldMap(options);
    assert.equal(raw.validateMap(map), null);
    // Retain complete ordered cells, endpoints, region crossings and stations.
    rails.push({options, expected: structuredClone(map.world.rails)});
  }
} finally {
  Math.random = previous;
  clean();
}
const output = {reference: pin, node: process.version, arch: process.arch,
  scope: 'Original complete checks and all assertions. MOV-72 has no new native full-scene consumer.',
  sourceFiles: {'test.js': hash(source), 'test-transport.mjs': hash(transport)}, cases, rails,
  aircraft: {units: Object.fromEntries(['fighter', 'attacker', 'bomber'].map(k => [k, raw.UNITS[k]])), air: raw.CFG.air}};
const target = resolve(root, 'rust/crates/simtest/tests/fixtures/movement-existing-checks.json.gz');
const compressed = gzipSync(JSON.stringify(output) + '\n', {level: 9});
if (process.argv.includes('--check')) assert.deepEqual(readFileSync(target), compressed);
else writeFileSync(target, compressed);
console.log(`${cases.length} complete Source checks; MOV-83 complete aircraft tables; native replay remains separate`);
