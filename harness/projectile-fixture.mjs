// Pure helpers only. Regenerate with x64 Node 24.21.0 and a clean pinned reference.
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
if (process.version !== 'v24.21.0' || process.arch !== 'x64') throw new Error('Use x64 Node 24.21.0');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.argv.find(a => a.startsWith('--reference='))?.slice(12) ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
if (execFileSync('git', ['rev-parse', 'HEAD'], { cwd: reference, encoding: 'utf8' }).trim() !== pin) throw new Error('Wrong reference pin');
// Check the full tracked tree, including staged edits, before importing source code.
execFileSync('git', ['diff', '--exit-code', pin, '--'], { cwd: reference, stdio: 'pipe' });
const source = await import(pathToFileURL(`${reference}/shared/projectiles.js`));
const bits = value => { const b = Buffer.alloc(8); b.writeDoubleBE(value); return b.toString('hex'); };
function encoded(value) {
  if (typeof value === 'number') return bits(value);
  if (Array.isArray(value)) return value.map(encoded);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, encoded(v)]));
  return value;
}
const cases = [];
function add(op, label, args) { cases.push({ op, label, args, expected: encoded(source[op](...args)) }); }
const p = (x, y, z) => ({ x, y, z });
const b = (x, z, rot) => rot === undefined ? { x, z } : { x, z, rot };
const box = { x: [-1, 1], y: [-1, 1], z: [-1, 1] };
for (const [name, args] of Object.entries({ inside: [p(0, 0, 0), p(0, 0, 0)], miss: [p(-2, 2, 0), p(2, 2, 0)], face: [p(-2, 0, 0), p(2, 0, 0)], corner: [p(-2, -2, -2), p(2, 2, 2)], tangent: [p(-2, 1, 1), p(2, 1, 1)], reverse: [p(2, 0, 0), p(-2, 0, 0)], end: [p(-2, 0, 0), p(-1, 0, 0)], threshold: [p(1 + 1e-13, 0, 0), p(1, 0, 0)] })) add('sweepBox', name, [...args, box]);
for (const [name, a, z] of [['tangent', p(-2, 0, 1), p(2, 0, 1)], ['vertical', p(0, -2, 0), p(0, 2, 0)], ['inside', p(0, 0, 0), p(0, 0, 0)], ['miss', p(-2, 3, 0), p(2, 3, 0)], ['zero-radius', p(-2, 0, 0), p(2, 0, 0)]]) add('sweepBody', name, [a, z, b(0, 0), b(0, 0), name === 'zero-radius' ? 0 : 1, -1, 1]);
add('sweepBody', 'crossing-moving-body', [p(-2, 0, 0), p(2, 0, 0), b(0, -2), b(0, 2), 0.5, -1, 1]);
for (const [name, a, z] of [['corner', b(0.5, 0.5), b(3.5, 3.5)], ['reverse-corner', b(3.5, 3.5), b(0.5, 0.5)], ['stationary', b(1, 1), b(1, 1)], ['boundary-negative', b(2, 2), b(0, 0)], ['outside-in', b(-2, 0.5), b(2, 0.5)], ['end-corner', b(0.5, 0.5), b(1, 1)], ['outside', b(-4, -4), b(-2, -2)]]) add('crossedCells', name, [a, z, 1, 4, 4]);
for (const duration of [undefined, 0.025, 0.1, 3]) for (const gravity of [0, 9.81, 25]) add('launchSolution', `duration-${duration}-gravity-${gravity}`, [p(1, 2, 3), p(31, 20, -9), 150, gravity, duration]);
add('launchSolution', 'stationary-minimum', [p(0, 0, 0), p(0, 0, 0), 150, 0]);
for (const time of [-1, 0, 3, 3.1, 10]) { const flight = { x: 1, y: 2, z: 3, vx: 20, vy: 30, vz: -10, gravity: 9.81, stamp: 3 }; for (const op of ['flightAt', 'velocityAt']) add(op, `time-${time}`, [flight, time]); }
for (const kind of ['rifle', 'sniper', 'tank']) for (const flame of [false, true]) for (const veh of [0, 19.999999, 20, 80]) for (const shellTerrain of [false, true]) add('projectileProfile', `${kind}-${flame}-${veh}-${shellTerrain}`, [kind, { flame, veh, shellTerrain }]);
const state = { explosive: false, sequence: 0, maxContacts: 2, energy: 1 };
for (const material of Object.values(source.CONTACT_MATERIALS)) for (const normal of [p(0, 1, 0), p(0, -1, 0), p(-1, 0, 0)]) add('contactResponse', material.name, [state, p(100, -10, 0), normal, material]);
for (const [label, change] of Object.entries({ explosive: { explosive: true }, sequence: { sequence: 2 }, energyBelow: { energy: 0.299999999 }, energyAt: { energy: 0.3 } })) add('contactResponse', label, [{ ...state, ...change }, p(100, -10, 0), p(0, 1, 0), source.CONTACT_MATERIALS.steel]);
for (const y of [-0.28 - 1e-12, -0.28, -0.28 + 1e-12]) add('contactResponse', `incidence-${y}`, [state, p(Math.sqrt(1 - y * y) * 100, y * 100, 0), p(0, 1, 0), source.CONTACT_MATERIALS.steel]);
for (const hardness of [0.7 - 1e-12, 0.7, 0.7 + 1e-12]) add('contactResponse', `hardness-${hardness}`, [state, p(100, -10, 0), p(0, 1, 0), { ...source.CONTACT_MATERIALS.steel, hardness }]);
for (const absorption of [-1, 0.3, 0.85, 2]) for (const rebound of [-1, 0.1, 1, 2]) add('contactResponse', `clamps-${absorption}-${rebound}`, [state, p(100, -10, 0), p(0, 1, 0), { ...source.CONTACT_MATERIALS.steel, absorption, rebound }]);
for (const speed of [0, 1, 1 + 1e-10]) add('contactResponse', `speed-${speed}`, [state, p(Math.sqrt(0.99) * speed, -0.1 * speed, 0), p(0, 1, 0), source.CONTACT_MATERIALS.steel]);
for (const [name, before, after] of [['stationary', b(0, 0, 0), b(0, 0, 0)], ['missing', b(0, 0), b(0, 0)], ['fallback', b(0, 0), b(0, 0, 1)], ['missing-after', b(0, 0, 1), b(0, 0)], ['rotate', b(0, 0, -1), b(0, 0, 1)], ['wrap-positive', b(0, 0, 3), b(0, 0, -3)], ['wrap-negative', b(0, 0, -3), b(0, 0, 3)], ['translate', b(-2, -2, 0), b(2, 2, 0.1)], ['pi', b(0, 0, 0), b(0, 0, Math.PI)]]) for (const [a, z] of [[p(-10, 0, 0), p(10, 0, 0)], [p(0, 0, -5), p(0, 0, 5)], [p(-10, 2, 0), p(10, 2, 0)], [p(-10, 0, 1), p(10, 0, 1)]]) add('sweepHull', name, [a, z, before, after, 1, 4, -1, 1]);
for (const args of [[b(1, 2), b(1, 2), b(3, 4)], [b(0, 0), b(10, 0), b(-1, 3)], [b(0, 0), b(10, 0), b(11, 3)], [b(0, 0), b(10, 0), b(5, 3)]]) add('segmentDistance', 'boundary', args);
let seed = 0x51a7b4d3;
const random = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed / 4294967296; };
const number = () => random() * 40 - 20;
const point = () => p(number(), number(), number());
const body = () => b(number(), number());
for (let i = 0; i < 128; i++) {
  const a = point(), z = point(), before = body(), after = body();
  add('sweepBox', `seed-${i}`, [a, z, { x: [-5, 5], y: [-5, 5], z: [-5, 5] }]);
  add('sweepBody', `seed-${i}`, [a, z, before, after, random() * 5, -10, 10]);
  add('sweepHull', `seed-${i}`, [a, z, { ...before, rot: number() }, { ...after, rot: number() }, random() * 3, random() * 10, -10, 10]);
  add('segmentDistance', `seed-${i}`, [before, after, body()]);
  add('crossedCells', `seed-${i}`, [before, after, 2.5, 12, 12]);
  add('launchSolution', `seed-${i}`, [a, z, 40 + random() * 380, random() * 25]);
}
const data = `${JSON.stringify({ format: 1, reference: pin, seed: '0x51a7b4d3', profiles: encoded(source.PROJECTILE_PROFILES), materials: encoded(source.CONTACT_MATERIALS), cases })}\n`;
const path = `${root}/rust/crates/core/tests/fixtures/projectiles.json`;
if (process.argv.includes('--check')) { if (readFileSync(path, 'utf8') !== data) throw new Error('Projectile fixture differs'); } else writeFileSync(path, data);
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} ${cases.length} projectile helper cases`);
