// Compares the harness's output with the oracle's for every case and checks the terrain contract
// (docs/contracts/terrain.md, MAP-57) on the game's layout. Prints one line per case and a JSON summary.
//   node compare.mjs <oracle dir> <harness dir> <summary.json>
import { readFileSync, writeFileSync, readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';
const [oracle, harness, summaryPath] = process.argv.slice(2);
const f32 = (p) => { const b = readFileSync(p); return new Float32Array(b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength)); };
const f64 = (p) => { const b = readFileSync(p); return new Float64Array(b.buffer, b.byteOffset, b.byteLength / 8); };
const u32 = (p) => { const b = readFileSync(p); return new Uint32Array(b.buffer, b.byteOffset, b.byteLength / 4); };
const i8 = (p) => { const b = readFileSync(p); return new Int8Array(b.buffer, b.byteOffset, b.byteLength); };
const L = 2.5, CELL = 2;
function maxDiff(a, b) {
  if (a.length !== b.length) return { count: [a.length, b.length], max: Infinity };
  let max = 0, at = -1, nanMismatch = 0;
  for (let i = 0; i < a.length; i++) {
    const x = a[i], y = b[i];
    if (Number.isNaN(x) || Number.isNaN(y)) { if (Number.isNaN(x) !== Number.isNaN(y)) nanMismatch++; continue; }
    const d = Math.abs(x - y);
    if (d > max) { max = d; at = i; }
  }
  return { max, at, nanMismatch };
}
function countOver(a, b, tol) { let n = 0; for (let i = 0; i < a.length; i++) { const x = a[i], y = b[i]; if (Number.isNaN(x) !== Number.isNaN(y) || Math.abs(x - y) > tol) n++; } return n; }
const cases = readdirSync(oracle).filter((f) => f.endsWith('.json') && f !== 'fixture.json').map((f) => f.slice(0, -5)).sort();
const summary = { cases: [] };
for (const name of cases) {
  if (!existsSync(join(harness, `${name}.ref.pos.bin`))) { console.log(`${name}: no harness output`); continue; }
  const meta = JSON.parse(readFileSync(join(oracle, `${name}.json`), 'utf8'));
  const { w, h } = meta, n = w * h;
  const row = { name, w, h, low: meta.low, vertices: meta.stats.vertices, triangles: meta.stats.triangles, cliffEdges: meta.stats.cliffEdges };
  // 1. Reference layout against relief.js, attribute by attribute
  for (const [k, read] of [['pos', f32], ['nrm', f32], ['uv', f32], ['paint', f32], ['scar', f32], ['idx', u32]]) {
    const d = maxDiff(read(join(oracle, `${name}.${k}.bin`)), read(join(harness, `${name}.ref.${k}.bin`)));
    row[k] = d.count ? `count ${d.count[0]} vs ${d.count[1]}` : d.max;
  }
  const pts = f64(join(oracle, `${name}.pts.bin`)), hatRef = f64(join(oracle, `${name}.hat.bin`)), hatNat = f64(join(harness, `${name}.ref.hat.bin`));
  row.hat = maxDiff(hatRef, hatNat).max; row.hatOver1mm = countOver(hatRef, hatNat, 1e-3);
  const hitsRef = f64(join(oracle, `${name}.hits.bin`)), hitsNat = f64(join(harness, `${name}.ref.hits.bin`));
  const hd = maxDiff(hitsRef, hitsNat); row.hits = hd.max; row.hitMissMismatch = hd.nanMismatch / 3; row.hitsOver1cm = countOver(hitsRef, hitsNat, 0.01) / 3;
  row.rays = hitsRef.length / 3;
  const waterRef = f64(join(oracle, `${name}.water.bin`)), waterNat = f64(join(harness, `${name}.ref.water.bin`));
  const wd = maxDiff(waterRef, waterNat); row.water = wd.max; row.waterNaNMismatch = wd.nanMismatch;
  // the sim's queries at the same points (shared/sim.js heightAt and levelAt x 2.5): how far the drawn ground is from them
  const simh = f64(join(oracle, `${name}.simh.bin`)), siml = f64(join(oracle, `${name}.siml.bin`));
  // 2. the game's layout: the contract at every cell centre (the first n points are the cell centres, row by row)
  const game = f64(join(harness, `${name}.game.hat.bin`)), flags = i8(join(harness, `${name}.game.flags.bin`));
  const known = meta.known ?? null, grid = meta.grid, roads = meta.roads, levels = meta.levels, frozen = meta.frozen;
  const wetAt = (x, y) => x >= 0 && y >= 0 && x < w && y < h && (!known || known[y * w + x] === '1') && 'WF='.includes(grid[y * w + x]);
  const c = { plain: 0, plainBad: 0, plainMax: 0, road: 0, roadBad: 0, trench: 0, trenchBad: 0, bank: 0, wet: 0, wetBad: 0, ford: 0, fordBad: 0, bridge: 0, bridgeBad: 0, centreVsSimMax: 0, unknown: 0, unknownNear: 0, shoved: 0, shovedMax: 0, refPlainBad: 0, refPlainMax: 0 };
  // the water line of each wet cell from the frozen levels (water-levels.js), and the depth rows (relief.js readWater)
  const wl = waterRef; // relief.waterAt = wl + WATER_LIFT
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    const i = y * w + x;
    if (known && known[i] !== '1') { c.unknown++; continue; }
    // for the World sample: cells within 2 of an unknown one are shaped differently on purpose (relief.js uses the '.' 0 placeholder)
    let nearUnknown = false;
    if (known) for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) { const xx = x + dx, yy = y + dy; if (xx >= 0 && yy >= 0 && xx < w && yy < h && known[yy * w + xx] !== '1') nearUnknown = true; }
    const ch = grid[i], nominal = levels[i] * L, got = game[i];
    let bank = false;
    for (let dy = -1; dy <= 1; dy++) for (let dx = -1; dx <= 1; dx++) if (wetAt(x + dx, y + dy)) bank = true;
    const dug = ch === 'T', road = roads[i] === '1';
    if ('WF'.includes(ch)) {
      // relief.js sample at a wet centre: min(base, line + 0.06 - carve); ford 0.15, channel 0.42 + 0.38 per row inward
      const isFord = ch === 'F';
      if (isFord) { c.ford++; const target = Math.min(nominal - (road ? 0.1 : 0), wl[i] - 0.15); if (Math.abs(got - target) > 1e-3) c.fordBad++; }
      else c.wet++;
      continue;
    }
    if (ch === '=') { c.bridge++; if (Math.abs(got - (nominal - (road ? 0.1 : 0))) > 0.1) c.bridgeBad++; continue; }
    if (bank) { c.bank++; continue; }
    if (dug) { c.trench++; if (Math.abs(got - (nominal - 0.9)) > 0.1) c.trenchBad++; continue; }
    if (road) { c.road++; if (Math.abs(got - (nominal - 0.1)) > 0.1 + 1e-9) c.roadBad++; continue; }
    if (nearUnknown) c.unknownNear++;
    // the reference's own query on the same cell (relief.js hAt reads shoved lips unshoved)
    const refD = Math.abs(hatRef[i] - nominal);
    if (refD > c.refPlainMax) c.refPlainMax = refD;
    if (refD > 0.1) c.refPlainBad++;
    if (flags[i] & 2) { c.shoved++; c.shovedMax = Math.max(c.shovedMax, Math.abs(got - nominal)); continue; } // a crater lip shoved this cell's drawn vertices
    c.plain++;
    const d = Math.abs(got - nominal);
    if (d > c.plainMax) c.plainMax = d;
    if (d > 0.1) c.plainBad++;
    c.centreVsSimMax = Math.max(c.centreVsSimMax, Math.abs(got - simh[i] + (0)));
  }
  row.contract = c;
  // the exact wet-centre rule (channels) checked against the reference depth rows is in the attribute match above;
  // here: drawn heights vs the queries (game layout)
  const meshHat = f64(join(harness, `${name}.game.mesh-hat.bin`));
  // a point exactly on a cell edge under a vertical cliff face touches both heights; HeightAt answers for the cell it is in
  let qm = 0, qn = 0, qOver = 0, qEdge = 0;
  for (let i = 0; i < meshHat.length; i++) {
    if (Number.isNaN(meshHat[i])) continue;
    const d = Math.abs(meshHat[i] - game[i]); qn++;
    const u = pts[i * 2] / CELL, v = pts[i * 2 + 1] / CELL;
    if (d > 1e-3 && (Math.abs(u - Math.round(u)) < 1e-9 || Math.abs(v - Math.round(v)) < 1e-9)) { qEdge++; continue; }
    if (d > qm) qm = d;
    if (d > 1e-3) qOver++;
  }
  row.heightVsMesh = { points: qn, max: qm, over1mm: qOver, onCliffEdgeLine: qEdge };
  const gh = f64(join(harness, `${name}.game.hits.bin`)), mh = f64(join(harness, `${name}.game.mesh-hits.bin`));
  let rm = 0, rn = 0, rOver = 0, rMiss = 0;
  for (let i = 0; i < mh.length / 3; i++) {
    const a = [gh[i * 3], gh[i * 3 + 1], gh[i * 3 + 2]], b = [mh[i * 3], mh[i * 3 + 1], mh[i * 3 + 2]];
    if (Number.isNaN(a[0]) || Number.isNaN(b[0])) { if (Number.isNaN(a[0]) !== Number.isNaN(b[0])) rMiss++; continue; }
    const d = Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]); rn++;
    if (d > rm) rm = d;
    if (d > 0.01) rOver++;
  }
  row.rayVsMesh = { rays: rn, max: rm, over1cm: rOver, missMismatch: rMiss };
  // the drawn ground against the sim's smooth heightAt and stepped levelAt (information: where they differ and by how much)
  let sh = 0, sl = 0;
  for (let i = n; i < pts.length / 2; i++) { sh = Math.max(sh, Math.abs(hatRef[i] - simh[i])); sl = Math.max(sl, Math.abs(hatRef[i] - siml[i])); }
  row.refVsSim = { heightAt: sh, levelAt: sl };
  // World: the reference draws unknown cells as '.' at level 0 (start.map); the port never reads them. Compare on known
  // cells at least 3 cells from any unknown one (relief.js reads 1 cell for ramps, 2 for its density checks).
  if (known) {
    const far = new Uint8Array(n);
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      let ok = known[y * w + x] === '1';
      for (let dy = -3; dy <= 3 && ok; dy++) for (let dx = -3; dx <= 3; dx++) { const xx = x + dx, yy = y + dy; if (xx >= 0 && yy >= 0 && xx < w && yy < h && known[yy * w + xx] !== '1') { ok = false; break; } }
      far[y * w + x] = ok ? 1 : 0;
    }
    let pn = 0, pm = 0, po = 0, kn = 0;
    for (let i = 0; i < pts.length / 2; i++) {
      const cx = Math.min(w - 1, Math.floor(pts[i * 2] / CELL)), cz = Math.min(h - 1, Math.floor(pts[i * 2 + 1] / CELL));
      if (!far[cz * w + cx]) continue;
      pn++; const d = Math.abs(hatRef[i] - game[i]); if (d > pm) pm = d; if (d > 1e-3) po++;
    }
    for (let i = 0; i < n; i++) if (known[i] === '1') kn++;
    row.worldKnown = { knownCells: kn, farCells: far.reduce((a, b) => a + b, 0), points: pn, max: pm, over1mm: po };
    console.log(`${name.padEnd(24)} World: ${kn} known cells, ${row.worldKnown.farCells} at least 3 from unknown; ${pn} points there: game HeightAt vs relief.js hAt max ${pm.toExponential(1)}, ${po} over 1 mm`);
  }
  summary.cases.push(row);
  const fmt = (v) => (typeof v === 'number' ? (v === 0 ? '0' : v.toExponential(1)) : v);
  console.log(`${name.padEnd(24)} pos ${fmt(row.pos)} nrm ${fmt(row.nrm)} uv ${fmt(row.uv)} paint ${fmt(row.paint)} scar ${fmt(row.scar)} idx ${fmt(row.idx)} | hAt ${fmt(row.hat)} (${row.hatOver1mm} >1mm) rays ${fmt(row.hits)} (${row.hitsOver1cm} >1cm, ${row.hitMissMismatch} miss) water ${fmt(row.water)} | plain ${c.plain} bad ${c.plainBad} max ${fmt(c.plainMax)} (ref ${c.refPlainBad}) shoved ${c.shoved} max ${fmt(c.shovedMax)} road ${c.road}/${c.roadBad} trench ${c.trench}/${c.trenchBad} ford ${c.ford}/${c.fordBad} bridge ${c.bridge}/${c.bridgeBad} | mesh-vs-query ${fmt(qm)} (${qOver}, ${qEdge} on a cliff edge line) ray-vs-mesh ${fmt(rm)} (${rOver}, ${rMiss} miss)`);
}
writeFileSync(summaryPath, JSON.stringify(summary, null, 1));
