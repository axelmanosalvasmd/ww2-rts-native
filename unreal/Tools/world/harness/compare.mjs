// Compares the harness's (or the engine's) output with the oracle fixture: every piece instance of every kind in the
// reference's order, every candidate and every placement. Tolerance: 2e-4 absolute plus 1e-5 relative (the reference
// stores Float32 matrices; the fixture rounds to 1e-6).
import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
const [fixture, got] = process.argv.slice(2);
const raw = readFileSync(fixture);
const f = JSON.parse((fixture.endsWith('.gz') ? gunzipSync(raw) : raw).toString('utf8'));
const cases = [];
let cur = null;
for (const line of readFileSync(got, 'utf8').split('\n')) {
  if (!line) continue;
  const t = line.split(' ');
  if (t[0] === 'CASE') { cur = { pieces: new Map(), cand: [], placed: [] }; cases.push(cur); continue; }
  if (t[0] === 'P') { const k = t[1]; if (!cur.pieces.has(k)) cur.pieces.set(k, []); cur.pieces.get(k).push(t.slice(2).map(Number)); }
  else if (t[0] === 'C') cur.cand.push([t[1], ...t.slice(2).map(Number)]);
  else if (t[0] === 'L') cur.placed.push(t[1] === '-' ? null : [t[1], ...t.slice(2).map(Number)]);
}
const near = (a, b) => Math.abs(a - b) <= 2e-4 + 1e-5 * Math.abs(b);
let bad = 0, checked = 0;
const report = [];
const fail = (msg) => { if (bad++ < 40) report.push(msg); };
f.cases.forEach((c, i) => {
  const g = cases[i];
  if (!g) { fail(`case ${i} ${c.name}: missing`); return; }
  if (c.pieces) {
    const want = new Map(c.pieces.map(k => [k.kind, k.instances]));
    for (const k of new Set([...want.keys(), ...g.pieces.keys()])) {
      const a = want.get(k) ?? [], b = g.pieces.get(k) ?? [];
      if (a.length !== b.length) { fail(`${c.name}: ${k} count ${b.length}, reference ${a.length}`); continue; }
      for (let j = 0; j < a.length; j++) {
        checked++;
        const d = a[j].findIndex((v, q) => !near(b[j][q], v));
        if (d >= 0) { fail(`${c.name}: ${k}[${j}] value ${d}: ${b[j][d]} vs reference ${a[j][d]} (row ${b[j].join(',')} / ${a[j].join(',')})`); }
      }
    }
  }
  if (c.props) {
    const a = c.props.candidates, b = g.cand;
    if (a.length !== b.length) fail(`${c.name}: ${b.length} candidates, reference ${a.length}`);
    for (let j = 0; j < Math.min(a.length, b.length); j++) {
      checked++;
      if (a[j][0] !== b[j][0] || a[j].slice(1).some((v, q) => !near(b[j][q + 1], v))) { fail(`${c.name}: candidate ${j}: ${b[j].join(',')} vs reference ${a[j].join(',')}`); }
    }
    const pa = c.props.placed, pb = g.placed;
    for (let j = 0; j < Math.min(pa.length, pb.length); j++) {
      checked++;
      const x = pa[j], y = pb[j];
      if (!x !== !y) { fail(`${c.name}: placement ${j}: ${y ? y[0] : 'hidden'} vs reference ${x ? x[0] : 'hidden'}`); continue; }
      if (!x) continue;
      if (x[0] !== y[0] || x.slice(1).some((v, q) => !near(y[q + 1], v))) fail(`${c.name}: placement ${j}: ${y.join(',')} vs reference ${x.join(',')}`);
    }
  }
});
console.log(report.join('\n'));
console.log(`${checked} rows compared, ${bad} differ`);
process.exit(bad ? 1 : 0);
