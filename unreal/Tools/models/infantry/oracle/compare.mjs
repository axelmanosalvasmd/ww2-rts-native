// Oracle comparison: the reference dump (dump-reference.mjs) against the C++ port (rig-harness.cpp), case by case.
// Tolerance: 1e-9 soldier metres on every joint position and matrix element (skin matrices, frames, things, grips,
// muzzle). Both sides compute in IEEE doubles in the same operation order; the only expected differences are the
// last bits of libm sin/cos/atan2/pow/hypot against V8's, which stay far below 1e-12. Anything above the tolerance,
// a missing field, a different job name or a different list of carried things fails the run.
// Usage: node compare.mjs reference.jsonl port.jsonl report.json
import { readFileSync, writeFileSync } from 'node:fs';

const [refPath, portPath, reportPath] = process.argv.slice(2);
const TOLERANCE = 1e-9;
const ref = readFileSync(refPath, 'utf8').trim().split('\n').map((l) => JSON.parse(l));
const port = readFileSync(portPath, 'utf8').trim().split('\n').map((l) => JSON.parse(l));
if (ref.length !== port.length) throw new Error(`case count differs: ${ref.length} reference, ${port.length} port`);

const worst = {}, failures = [];
let compared = 0;
function walk(a, b, field, where) {
  if (a === null || b === null) {
    if (a !== b) failures.push(`${where} ${field}: ${a === null ? 'reference has none' : 'port has none'}`);
    return;
  }
  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) { failures.push(`${where} ${field}: length ${a.length} vs ${b?.length}`); return; }
    a.forEach((x, i) => walk(x, b[i], field, where));
    return;
  }
  const d = Math.abs(a - b);
  compared++;
  if (!(d <= (worst[field]?.error ?? -1))) worst[field] = { error: d, where };
  if (!(d <= TOLERANCE)) failures.push(`${where} ${field}: ${a} vs ${b} (|d| ${d})`);
}
ref.forEach((r, n) => {
  const p = port[n], where = `${r.type}/${r.fac}/${r.i}/${r.job} k=${r.k} phase=${r.phase}`;
  if (r.job !== p.job) failures.push(`${where}: job ${r.job} vs ${p.job}`);
  for (const field of ['legs', 'arms', 'B', 'U', 'H', 'M0', 'W', 'sling', 'skin', 'grips', 'muzzle']) walk(r[field], p[field], field, where);
  // spine_N and hose_N from the port's full bone list (TCInfantryRig.cpp BoneTransforms, bones 25-30)
  walk(r.spine, p.bones.slice(25, 25 + r.spine.length), 'spine', where);
  if (p.bones.slice(25 + r.spine.length, 29).some((b) => b !== null)) failures.push(`${where}: port poses unused spine anchors`);
  walk(r.hose, r.hose ? p.bones.slice(29, 31) : (p.bones[29] === null && p.bones[30] === null ? null : p.bones.slice(29, 31)), 'hose', where);
  if (r.things.map((t) => t.kind).join() !== p.things.map((t) => t.kind).join()) failures.push(`${where}: things ${r.things.map((t) => t.kind)} vs ${p.things.map((t) => t.kind)}`);
  else r.things.forEach((t, i) => walk(t.m, p.things[i].m, 'things', where));
});
const maxError = Math.max(...Object.values(worst).map((w) => w.error));
const report = { tolerance: TOLERANCE, cases: ref.length, numbersCompared: compared, maxError, worstByField: worst, failures: failures.slice(0, 50), failureCount: failures.length, pass: failures.length === 0 };
writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify({ cases: ref.length, numbersCompared: compared, maxError, failureCount: failures.length, pass: report.pass }));
if (!report.pass) { console.log(failures.slice(0, 15).join('\n')); process.exit(1); }
