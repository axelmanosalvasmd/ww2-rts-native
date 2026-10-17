// Run: node harness/if11-source-check.mjs /absolute/path/to/motion_check
// WW2_REFERENCE may select another checkout of the same reference pin.
// --source-only checks the source/client witnesses without claiming C parity.
// No recordings or fixtures are written. Existing recorded flight cases supply
// additional inputs and a decoded binary64 cross-check of the live source.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const reference = resolve(process.env.WW2_REFERENCE ?? resolve(root, 'reference/ww2-rts'));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const args = process.argv.slice(2);
assert.equal(args.length, 1, 'pass the compiled motion_check path or --source-only');
const sourceOnly = args[0] === '--source-only';
const binary = sourceOnly ? null : resolve(args[0]);
const head = spawnSync('git', ['-C', reference, 'rev-parse', 'HEAD'], { encoding: 'utf8' });
assert.equal(head.status, 0, `cannot inspect reference pin: ${head.stderr}`);
assert.equal(head.stdout.trim(), pin, 'reference must be pinned');
const dirty = spawnSync('git', ['-C', reference, 'status', '--porcelain', '--untracked-files=no'], { encoding: 'utf8' });
assert.equal(dirty.status, 0, `cannot inspect reference changes: ${dirty.stderr}`);
assert.equal(dirty.stdout, '', 'reference tracked source must be clean');
const load = path => import(pathToFileURL(resolve(reference, path)).href);
const { UNITS } = await load('shared/sim.js');
const { movementProfile, isGroundVehicle, bodyRadius } = await load('shared/vehicle-motion.js');
const { flightAt, velocityAt } = await load('shared/projectiles.js');
const { createProjectileView } = await load('client/projectiles.js');

// JavaScript JSON serialization normalizes signed zero. Other numbers compare exactly.
function equal(actual, expected, label) {
  if (typeof expected === 'number') {
    assert.equal(typeof actual, 'number', label);
    assert.ok(Number.isFinite(expected) && Number.isFinite(actual), `${label}: finite numbers required`);
    assert.ok(actual === expected, `${label}: ${actual} !== ${expected}`);
  } else if (expected !== null && typeof expected === 'object') {
    assert.ok(actual !== null && typeof actual === 'object', label);
    assert.deepEqual(Object.keys(actual).sort(), Object.keys(expected).sort(), `${label}: keys`);
    for (const key of Object.keys(expected)) equal(actual[key], expected[key], `${label}.${key}`);
  } else {
    assert.strictEqual(actual, expected, label);
  }
}

let cQueries = 0;
function compare(query, input, expected, label) {
  if (sourceOnly) return;
  const child = spawnSync(binary, [query, JSON.stringify(input)], {
    encoding: 'utf8', timeout: 10_000, maxBuffer: 128 * 1024,
  });
  assert.ifError(child.error);
  assert.equal(child.status, 0, `${label}: ${child.stderr}`);
  equal(JSON.parse(child.stdout), expected, `${query}/${label}`);
  cQueries++;
}

const types = Object.keys(UNITS).sort();
assert.equal(types.length, 47, 'all pinned unit types');
for (const type of types) {
  const def = UNITS[type], input = { type };
  compare('movement_profile', input, movementProfile(type, def), type);
  compare('is_ground_vehicle', input, isGroundVehicle(def), type);
  compare('body_radius', input, bodyRadius(def), type);
}

// Exercise the actual client API at tick zero with explicit clocks. Translating
// the row's time to stamp-time preserves the exact elapsed-time subtraction,
// including negative query times that cannot be represented as client ticks.
let clientFlights = 0;
function clientCheck(flight, time, position, velocity, label) {
  const trails = [];
  const view = createProjectileView({ trail: (...values) => trails.push(values) });
  view.snapshot({ tick: 0, shots: [], flights: [{
    flight: 1, seq: 0, x: flight.x, y: flight.y, z: flight.z,
    v: [flight.vx, flight.vy, flight.vz], gravity: flight.gravity,
    time: flight.stamp - time, until: 1, expires: 1, kind: 'gun', effect: 'test',
  }] }, 0);
  view.frame(0);
  assert.equal(view.count, 1, `${label}: client retained flight`);
  assert.equal(trails.length, 1, `${label}: client trail count`);
  equal(trails[0][1], position, `${label}: client position`);
  equal(trails[0][2], velocity, `${label}: client velocity`);
  view.reset();
  assert.equal(view.count, 0, `${label}: client reset`);
  clientFlights++;
}

let generatedFlights = 0;
for (const stamp of [-7.25, 0, 3]) {
  for (const gravity of [0, 9.81, 25]) {
    for (const [vx, vy, vz] of [[0, 0, 0], [20, 30, -10], [-3.125, -17.75, 9.5], [1 / 3, -2 / 7, 123.456789]]) {
      for (const elapsed of [-2, 0, 0.125, 1, 12.5]) {
        const flight = { x: -11.125, y: 42.75, z: 0.3, vx, vy, vz, gravity, stamp };
        const time = stamp + elapsed, input = { flight, time };
        const label = `generated-${generatedFlights}`;
        const position = flightAt(flight, time), velocity = velocityAt(flight, time);
        compare('flight_at', input, position, label);
        compare('velocity_at', input, velocity, label);
        clientCheck(flight, time, position, velocity, label);
        generatedFlights++;
      }
    }
  }
}

function decode(value) {
  if (typeof value === 'string' && /^[0-9a-f]{16}$/i.test(value)) {
    return Buffer.from(value, 'hex').readDoubleBE(0);
  }
  if (Array.isArray(value)) return value.map(decode);
  if (value !== null && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, decode(item)]));
  }
  return value;
}
const recorded = JSON.parse(await readFile(resolve(root, 'rust/crates/core/tests/fixtures/projectiles.json'), 'utf8'));
assert.equal(recorded.reference, pin, 'recorded input pin');
const cases = recorded.cases.filter(row => row.op === 'flightAt' || row.op === 'velocityAt');
assert.equal(cases.length, 10, 'all existing recorded flight/velocity cases');
for (const row of cases) {
  const [flight, time] = decode(row.args);
  const position = flightAt(flight, time), velocity = velocityAt(flight, time);
  const actual = row.op === 'flightAt' ? position : velocity;
  equal(actual, decode(row.expected), `${row.op}/${row.label}: recorded binary64`);
  compare(row.op === 'flightAt' ? 'flight_at' : 'velocity_at', { flight, time }, actual, row.label);
  clientCheck(flight, time, position, velocity, `${row.op}/${row.label}`);
}
console.log(JSON.stringify({ reference: pin, unitTypes: types.length, movementQueries: types.length * 3,
  generatedFlights, recordedQueries: cases.length, clientFlights, cQueries,
  result: sourceOnly ? 'SOURCE ONLY: C parity unrun' : 'PASS' }));
