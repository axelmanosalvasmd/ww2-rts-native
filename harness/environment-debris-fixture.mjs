// Calls the original pinned debris helpers without changing their source.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? join(root, 'reference/ww2-rts'));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => { assert.equal(git(['rev-parse', 'HEAD']), pin); assert.equal(git(['status', '--porcelain', '--untracked-files=no']), ''); };
clean();
const { DEBRIS_LIMITS, SECTION_MASS, wreckMass, debrisBody, stepDebris, settleDebris, debrisRow } = await import(pathToFileURL(join(reference, 'shared/debris-motion.js')));
const { UNITS } = await import(pathToFileURL(join(reference, 'shared/sim.js')));
const view = new DataView(new ArrayBuffer(8));
function encode(value) {
  if (typeof value === 'number') { view.setFloat64(0, value, false); return view.getBigUint64(0, false).toString(16).padStart(16, '0'); }
  if (Array.isArray(value)) return value.map(encode);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, encode(item)]));
  return value;
}
function decode(value) {
  if (value === '-0') return -0;
  if (Array.isArray(value)) return value.map(decode);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, decode(item)]));
  return value;
}
const masses = Object.entries(UNITS).map(([kind, definition]) => ({ kind, ...(definition.radius === undefined ? {} : { radius: definition.radius }), expected: encode(wreckMass(kind, definition)) }));
for (const radius of [undefined, 0, -2, 0.7, 1.5, 2.2, 20]) masses.push({ kind: 'unlisted', ...(radius === undefined ? {} : { radius }), expected: encode(wreckMass('unlisted', { radius })) });
const constructors = [];
const base = { id: 'defaults', x: 2, y: 3, z: 4 };
for (const kind of [undefined, 'section', 'wreck']) for (const impulse of [undefined, -1, 0.25, 1, 4, 9]) {
  const input = { ...base, ...(kind === undefined ? {} : { kind }), ...(impulse === undefined ? {} : { impulse }) };
  const body = debrisBody(input); constructors.push({ input, body: encode(body), row: encode(debrisRow(body)) });
}
for (const input of [{ ...base, x: '-0', y: '-0', z: '-0', dir: '-0' }, { ...base, dir: Math.PI, mass: -5, radius: 0, time: 123.4 }, { ...base, dir: -8.4, material: 'wood', mass: 350 }]) {
  const body = debrisBody(decode(input)); constructors.push({ input, body: encode(body), row: encode(debrisRow(body)) });
}
function environment(spec) {
  return {
    ground: (x, z) => (spec.floor ?? 0) + x * (spec.slope ?? 0),
    ...(spec.material ? { material: () => decode(spec.material) } : {}),
    ...(spec.wall ? { contact: (a, b) => a.x <= spec.wall && b.x > spec.wall ? { fraction: (spec.wall - a.x) / (b.x - a.x), normal: { x: -2, y: 0, z: 0 } } : null } : {}),
    ...(spec.hit ? { contact: () => decode(spec.hit) } : {}),
  };
}
const traces = [];
function record(label, input, env, actions, patch = {}) {
  const body = debrisBody(decode(input)); Object.assign(body, decode(patch));
  const snapshots = [{ body: encode(body), row: encode(debrisRow(body)) }], queries = environment(env);
  for (const action of actions) {
    if (action.op === 'step') stepDebris(body, action.dt, queries);
    else if (action.op === 'settle') settleDebris(body, queries);
    else throw Error(action.op);
    snapshots.push({ body: encode(body), row: encode(debrisRow(body)) });
  }
  traces.push({ label, input, patch, env, actions, snapshots });
}
for (const mode of ['flat', 'slope', 'wall']) for (const kind of ['section', 'wreck']) {
  const env = { material: { rebound: 0.8, absorption: -0.3 }, ...(mode === 'slope' ? { slope: 0.1 } : {}), ...(mode === 'wall' ? { wall: 0.3 } : {}) };
  const input = { id: `motion:${mode}:${kind}`, kind, x: '-0', y: 1.6, z: 0, dir: 0.35, impulse: 2.6, mass: kind === 'wreck' ? 57000 : 350, material: 'wood', radius: 0.65, time: 123.4 };
  record(`${mode}:${kind}`, input, env, Array.from({ length: 80 }, (_, i) => ({ op: 'step', dt: i === 10 ? 1.3 : 0.05 })));
}
const step = dt => ({ op: 'step', dt });
const launch = { id: 'boundary', x: 0, y: 2, z: 0 };
for (const fraction of [0, 0.5, 1]) record(`contact-fraction:${fraction}`, launch, { hit: { fraction, normal: { x: -2, y: 1, z: 0 }, material: { rebound: 0.35, absorption: 0.9 } } }, [step(0.05), step(0.1), step(0.5)]);
for (const normal of [{ x: 0, y: 0, z: 0 }, { x: 1, y: 0, z: 0 }]) record(`contact-normal:${normal.x}`, launch, { hit: { fraction: 0.5, normal } }, [step(0.05), step(0.4)]);
for (const vy of [-0.4, -0.4000001]) record(`ground-rebound:${vy}`, { ...launch, y: 0 }, {}, [step(0.01)], { vy, gravity: 0, drag: 0 });
for (const y of [0.015, 0.0150000001]) record(`ground-tolerance:${y}`, { ...launch, y }, {}, [step(0.01)], { vy: 0, gravity: 0 });
for (const vx of [0.119999, 0.12]) record(`rest-speed:${vx}`, { ...launch, y: 0 }, {}, [step(0.01)], { vx, vy: 0, vz: 0, gravity: 0, drag: 0 });
for (const kind of ['section', 'wreck']) {
  record(`travel-limit:${kind}`, { ...launch, kind, y: 10 }, {}, [step(0.05), step(0.05)], { vx: 100, vy: 0, gravity: 0, drag: 0 });
  record(`settle:${kind}`, { ...launch, kind }, { slope: 0.2 }, [{ op: 'settle' }, { op: 'settle' }, step(1)]);
}
record('time-and-subdivision-boundaries', launch, {}, [step(-1), step(0), step(0.05), step(0.0500000001), step(2.5), step(1)]);
record('lifetime-before-and-at', { ...launch, y: 10 }, {}, [step(0.05), step(0.05)], { born: -2.4, vx: 0, vy: 0, gravity: 0 });
clean();
const output = `${JSON.stringify({ reference: pin, node: process.version, arch: process.arch, limits: encode(DEBRIS_LIMITS), sectionMass: encode(SECTION_MASS), masses, constructors, traces })}\n`;
const path = join(root, 'rust/crates/core/tests/fixtures/environment-debris.json');
if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), output);
else writeFileSync(path, output);
console.log(`${process.argv.includes('--check') ? 'Verified' : 'Recorded'} ${masses.length} mass cases, ${constructors.length} constructors, ${traces.length} traces and ${traces.reduce((n, trace) => n + trace.snapshots.length, 0)} body states`);
