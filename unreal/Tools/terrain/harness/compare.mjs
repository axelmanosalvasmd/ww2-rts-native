// Compares the harness's overlays and decals (<case>.<k>.native-ops.json, .native-decals.json) with the oracle's
// (<case>.<k>.ops.json, .decals.json): every shell hole, trench cut and contour stroke (geometry, colours, widths,
// clip) and every rubble and crater decal vertex.   node compare.mjs <oracle dir> <harness out dir>
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
const [oracle, mine] = process.argv.slice(2);
const summary = JSON.parse(readFileSync(join(oracle, 'summary.json'), 'utf8'));
const color = (s) => {
  if (Array.isArray(s)) return s;
  let m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(s);
  if (m) return [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16), 1];
  m = /^rgba?\(([^)]*)\)$/.exec(s);
  if (m) { const v = m[1].split(',').map(Number); return [v[0], v[1], v[2], v[3] ?? 1]; }
  throw new Error('colour ' + s);
};
const close = (a, b, tol = 1e-7) => Math.abs(a - b) <= tol * Math.max(1, Math.abs(a), Math.abs(b));
let bad = 0, ops = 0, verts = 0;
const fail = (where, what) => { if (bad++ < 30) console.log(`  ${where}: ${what}`); };
for (const c of summary.cases) {
  const base = `${c.name}.${c.k}`;
  if (!existsSync(join(mine, base + '.native-ops.json'))) { fail(base, 'no harness output'); continue; }
  const A = JSON.parse(readFileSync(join(oracle, base + '.ops.json'), 'utf8')), B = JSON.parse(readFileSync(join(mine, base + '.native-ops.json'), 'utf8'));
  if (A.length !== B.length) { fail(base, `ops ${B.length} vs ${A.length}`); continue; }
  A.forEach((a, i) => {
    const b = B[i]; ops++;
    if (a.op !== b.op) return fail(base, `op ${i} ${b.op} vs ${a.op}`);
    if (a.op === 'fill') {
      const g = a.style;
      if (g.type !== 'radial' || !b.gradient) return fail(base, `op ${i} fill style`);
      [g.x0, g.y0, g.r0, g.x1, g.y1, g.r1].forEach((v, k) => { if (!close(v, b.g[k])) fail(base, `op ${i} gradient ${k} ${b.g[k]} vs ${v}`); });
      g.stops.forEach(([at, col], k) => { const cc = color(col), s = b.stops[k]; if (!s || s[0] !== at || cc.some((v, j) => !close(v, s[j + 1]))) fail(base, `op ${i} stop ${k}`); });
    } else {
      const cc = color(a.style), s = b.stops[0];
      if (cc.some((v, j) => !close(v, s[j + 1]))) fail(base, `op ${i} stroke colour ${s} vs ${cc}`);
      if (!close(a.lineWidth, b.lineWidth)) fail(base, `op ${i} width ${b.lineWidth} vs ${a.lineWidth}`);
      if (a.cap !== 'round' || a.join !== 'round') fail(base, `op ${i} caps ${a.cap}/${a.join}`);
    }
    if (a.path.length !== b.path.length) return fail(base, `op ${i} path ${b.path.length} vs ${a.path.length}`);
    a.path.forEach((p, k) => { const q = b.path[k]; if (p[0] !== q[0] || p.slice(1).some((v, j) => !close(v, q[j + 1]))) fail(base, `op ${i} path ${k} ${JSON.stringify(q)} vs ${JSON.stringify(p)}`); });
    if (JSON.stringify(a.clip) !== JSON.stringify(b.clip)) fail(base, `op ${i} clip ${JSON.stringify(b.clip)} vs ${JSON.stringify(a.clip)}`);
  });
  const DA = JSON.parse(readFileSync(join(oracle, base + '.decals.json'), 'utf8')), DB = JSON.parse(readFileSync(join(mine, base + '.native-decals.json'), 'utf8'));
  for (const kind of ['rubble', 'craters']) {
    const x = DA[kind], y = DB[kind];
    if (!x && !y) continue;
    if (!x || !y || x.count !== y.count) { fail(base, `${kind} count ${y?.count} vs ${x?.count}`); continue; }
    for (const key of ['positions', 'uvs']) {
      if (!x[key]) continue;
      x[key].forEach((v, k) => { verts++; if (Math.abs(v - y[key][k]) > 2e-5) fail(base, `${kind} ${key}[${k}] ${y[key][k]} vs ${v}`); });
    }
  }
}
console.log(`RESULT ${summary.cases.length} paints, ${ops} overlay draws, ${verts} decal numbers: ${bad} mismatches`);
process.exit(bad ? 1 : 0);
