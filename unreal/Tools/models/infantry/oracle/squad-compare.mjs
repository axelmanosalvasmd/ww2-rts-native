// Compares squad-harness output with squad-reference.jsonl. Tolerance 1e-9 (metres, radians, phase) on every value.
// Usage: node squad-compare.mjs squad-reference.jsonl squad-port.jsonl report.json
import { readFileSync, writeFileSync } from 'node:fs';
const [a, b, outPath] = process.argv.slice(2);
const TOLERANCE = 1e-9, FIELDS = ['x', 'z', 'yaw', 'phase', 'blend', 'speed', 'localX', 'localZ', 'localYaw'];
const ref = readFileSync(a, 'utf8').trim().split('\n').map((l) => JSON.parse(l)), port = readFileSync(b, 'utf8').trim().split('\n').map((l) => JSON.parse(l));
const worst = Object.fromEntries(FIELDS.map((f) => [f, 0]));
let failures = 0, first = null;
if (ref.length !== port.length) throw new Error(`steps differ: ${ref.length} vs ${port.length}`);
ref.forEach((men, step) => men.forEach((row, man) => row.forEach((value, k) => {
  const d = Math.abs(value - port[step][man][k]);
  worst[FIELDS[k]] = Math.max(worst[FIELDS[k]], d);
  if (!(d <= TOLERANCE)) { failures++; first ??= `step ${step} man ${man} ${FIELDS[k]}: ${value} vs ${port[step][man][k]}`; }
})));
const report = { tolerance: TOLERANCE, steps: ref.length, men: ref[0].length, worst, failures, first, pass: failures === 0 };
writeFileSync(outPath, JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report));
if (!report.pass) process.exit(1);
