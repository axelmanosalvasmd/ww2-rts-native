// Oracle comparison: reference.jsonl (motion-reference.mjs) against port.jsonl (motion-harness.cpp), record by record.
// Tolerances (absolute, reference metres, radians or weights):
//   1e-9 for everything computed in doubles on both sides: squads (man place, turn, pose node, every morph weight,
//        muzzle, the follower state), hull settle, wheel travel and turn, wheel angles, track link positions (both
//        read the same float32 path table), the kaiju, planes. Expected differences are the last bits of libm
//        sin, cos, atan2, exp, hypot against V8's.
//   1e-5 for corpse instance matrices and 1e-6 for their fade: the reference stores them in Float32Array instance
//        buffers (InstancedMesh.instanceMatrix, the 'fade' attribute), the port prints doubles.
// Structure must match exactly: record kinds, counts, near/far flags, nulls (no muzzle, no wheels on wheel-less
// hulls are skipped where the reference has none).
// Usage: node compare.mjs reference.jsonl port.jsonl report.json
import { readFileSync, writeFileSync } from 'node:fs';

const [refPath, portPath, reportPath] = process.argv.slice(2);
const ref = readFileSync(refPath, 'utf8').trim().split('\n').map((l) => JSON.parse(l));
const port = readFileSync(portPath, 'utf8').trim().split('\n').map((l) => JSON.parse(l));
const TOL = { squad: 1e-9, vehicle: 1e-9, plane: 1e-9, tv: 1e-9, wheel: 1e-9, bodies: 1e-5, fade: 1e-6 };
const failures = [], worst = {}, counts = {};
let compared = 0;
function num(a, b, field, where, tol) {
  compared++;
  const d = Math.abs(a - b);
  if (!(d <= (worst[field]?.error ?? -1))) worst[field] = { error: d, where };
  if (!(d <= tol)) failures.push(`${where} ${field}: ${a} vs ${b} (|d| ${d})`);
}
function walk(a, b, field, where, tol) {
  if (a === null || b === null || a === undefined || b === undefined) { if (a !== b) failures.push(`${where} ${field}: ${JSON.stringify(a)} vs ${JSON.stringify(b)}`); return; }
  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) { failures.push(`${where} ${field}: length ${a.length} vs ${b?.length}`); return; }
    a.forEach((x, i) => walk(x, b[i], field, where, tol));
    return;
  }
  if (typeof a === 'boolean') { if (a !== b) failures.push(`${where} ${field}: ${a} vs ${b}`); return; }
  num(a, b, field, where, tol);
}
if (ref.length !== port.length) failures.push(`record count ${ref.length} vs ${port.length}`);
for (let n = 0; n < Math.min(ref.length, port.length); n++) {
  const r = ref[n], p = port[n], where = `#${n} ${r.kind}${r.type ? ' ' + r.type : ''}${r.step !== undefined ? ' step ' + r.step : ''}`;
  counts[r.kind] = (counts[r.kind] ?? 0) + 1;
  if (r.kind !== p.kind) { failures.push(`${where}: kind ${p.kind}`); continue; }
  switch (r.kind) {
    case 'trackrows': walk(r.rows, p.rows, 'track rows', where, 0); break;
    case 'tv': walk(r.p, p.p, 'track vertex', where, TOL.tv); break;
    case 'wheel': walk(r.a, p.a, 'wheel angle', where, TOL.wheel); break;
    case 'squad':
      if (r.far !== p.far) failures.push(`${where}: far ${r.far} vs ${p.far}`);
      if (r.men.length !== p.men.length) { failures.push(`${where}: men ${r.men.length} vs ${p.men.length}`); break; }
      r.men.forEach((m, i) => {
        const q = p.men[i], w = `${where} man ${i}`;
        walk(m.p, q.p, 'man position', w, TOL.squad); walk(m.ry, q.ry, 'man turn', w, TOL.squad);
        walk(m.lean, q.lean, 'pose lean', w, TOL.squad); walk(m.sy, q.sy, 'pose height', w, TOL.squad); walk(m.node, q.node, 'pose shift', w, TOL.squad);
        walk(m.w, q.w, 'morph weights', w, TOL.squad); walk(m.muzzle, q.muzzle, 'muzzle', w, TOL.squad);
        if (m.m) walk(m.m, q.m, 'follower', w, TOL.squad);
      });
      break;
    case 'vehicle':
      if (r.kaiju) walk(r.kaiju, p.kaiju, 'kaiju', where, TOL.vehicle);
      if (r.body) walk(r.body, p.body, 'hull body', where, TOL.vehicle);
      if (r.wheels) walk(r.wheels, p.wheels, 'wheel travel', where, TOL.vehicle);
      break;
    case 'plane': walk(r.body, p.body, 'plane body', where, TOL.plane); walk(r.props, p.props, 'propellers', where, TOL.plane); break;
    case 'bodies':
      if (r.count !== p.count || r.list.length !== p.list.length) { failures.push(`${where}: bodies ${r.count}/${r.list.length} vs ${p.count}/${p.list.length}`); break; }
      r.list.forEach((b, i) => { walk(b.m, p.list[i].m, 'corpse matrix', `${where} body ${i}`, TOL.bodies); walk(b.fade, p.list[i].fade, 'corpse fade', `${where} body ${i}`, TOL.fade); });
      break;
    default: failures.push(`${where}: unknown kind`);
  }
}
const report = { tolerances: TOL, records: ref.length, byKind: counts, numbersCompared: compared, worstByField: worst, failureCount: failures.length, failures: failures.slice(0, 60), pass: failures.length === 0 };
writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify({ records: ref.length, numbersCompared: compared, failureCount: failures.length, pass: report.pass }));
if (!report.pass) { console.log(failures.slice(0, 25).join('\n')); process.exit(1); }
