// The oracle fixture's inputs as a token stream for atmos-harness.cpp (unreal/Tools/atmosphere/harness/run.sh).
import { readFileSync, writeFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
const [fix, out] = process.argv.slice(2);
const raw = readFileSync(fix);
const f = JSON.parse((fix.endsWith('.gz') ? gunzipSync(raw) : raw).toString());
const t = [];
const R = f.rail;
t.push('RAIL', R.w, R.h, R.steps.length);
for (const s of R.steps) {
  t.push(s.dt, s.centres.length, ...s.centres.flat());
  const rows = s.cells.filter(r => r.length >= 5 && r[4] && (r[4].rail || r[4].station !== undefined));
  t.push(rows.length);
  for (const r of rows) t.push(r[0], r[4].rail ?? 0, r[4].station !== undefined ? 1 : 0, r[4].station ?? 0);
}
t.push('MAPS', f.maps.length);
for (const m of f.maps) { t.push(m.file, m.w, m.h); for (const row of m.rows) t.push(row); }
t.push('WIND', f.wind.length, ...f.wind.map(w => [w[0], w[1]]).flat());
t.push('RUNNING', f.running.length);
for (const r of f.running) {
  const m = f.maps.find(x => x.file === r.file);
  t.push(r.file, r.mood, r.wind ? 1 : 0, ...(r.wind ?? []), r.frames.length);
  for (const fr of r.frames) t.push(fr.t, fr.bankOpacity);
}
writeFileSync(out, t.join('\n') + '\n');
