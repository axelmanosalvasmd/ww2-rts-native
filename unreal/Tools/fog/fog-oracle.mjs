// Node oracle for the native port of client/fog.js (World/TCFogFrontier). Runs the pinned reference's fog.js itself
// (unmodified, imported from a git-archive copy of e4e579a whose node_modules holds three r186) on scripted play:
// units walking across a map with vision circles (the mask one step behind them, as the 5 Hz vision pass; some
// delivered units give no vision); the server side is imitated only to make the masks (seen now, ever
// seen), which are packed with the reference's own packRuns, exactly as shared/sim.js fogFor sends them. Every step's
// observable output is recorded: the fog texture's alpha per cell (rows turned back to map order) after each frame, the
// minimap alphas and fog.at's three looks.
//   node fog-oracle.mjs <reference copy with node_modules> <out.json>
import fs from 'node:fs';
import path from 'node:path';
const [ref, outFile] = process.argv.slice(2);
const { createFog } = await import(path.join(ref, 'client/fog.js'));
const { packRuns, unpackRuns, UNITS, CELL } = await import(path.join(ref, 'shared/sim.js'));
let seed = 12345;
const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;

function scenario({ name, w, h, opaque, off = false, keyless = false, snaps = 14, units = 6, radius = 5, lift = [] }) {
  const n = w * h, vis = new Uint8Array(n), explored = new Uint8Array(n);
  const walkers = Array.from({ length: units + 3 }, (_, i) => ({ blind: i >= units, x: (2 + rnd() * (w - 4)) * CELL, z: (2 + rnd() * (h - 4)) * CELL, vx: (rnd() - 0.5) * 6, vz: (rnd() - 0.5) * 6,
    type: ['rifle', 'tank', 'hq', 'depot', 'fighter', 'nosuchunit', 'barracks', 'mg', 'tower'][i % 9] }));
  const see = () => {
    vis.fill(0);
    for (const u of walkers) {
      if (u.blind) continue; // delivered but giving this side no vision (an enemy in view of a teammate)
      const cx = Math.floor(u.x / CELL), cz = Math.floor(u.z / CELL), r = UNITS[u.type]?.air ? 0 : radius;
      for (let y = Math.max(0, cz - r); y <= Math.min(h - 1, cz + r); y++) for (let x = Math.max(0, cx - r); x <= Math.min(w - 1, cx + r); x++)
        if ((x - cx) ** 2 + (y - cz) ** 2 <= r * r) vis[y * w + x] = 1;
    }
    for (let c = 0; c < n; c++) explored[c] |= vis[c];
  };
  // a start: some ground explored earlier (a strip), the units' circles seen now
  for (let y = 0; y < h; y++) for (let x = 0; x < Math.floor(w / 3); x++) if ((x + y) % 7) explored[y * w + x] = 1;
  see();
  const key = keyless ? undefined : { v: packRuns(vis), e: packRuns(explored) };
  const fog = createFog(w, h, key, off, opaque);
  const cells = (str) => { const out = []; if (str) unpackRuns(str, (s, c) => { for (let i = s; i < s + c; i++) out.push(i); }); return out; };
  const alphaNow = () => { const a = new Array(n); for (let c = 0; c < n; c++) a[c] = fog.texture.image.data[((h - 1 - Math.floor(c / w)) * w + c % w) * 4 + 3]; return a; };
  const looks = () => { const a = new Array(n); for (let c = 0; c < n; c++) { const t = fog.at((c % w + 0.5) * CELL, (Math.floor(c / w) + 0.5) * CELL); a[c] = t === 'seen' ? 0 : t === 'explored' ? 1 : 2; } return a; };
  const mm = () => { const img = { data: new Uint8ClampedArray(n * 4) }; fog.minimap(img); const a = new Array(n); for (let c = 0; c < n; c++) a[c] = img.data[c * 4 + 3]; return a; };
  const out = { name, w, h, opaque, off, keyless, key: key ? { v: cells(key.v), e: cells(key.e) } : null, initial: { alpha: alphaNow(), looks: looks(), minimap: mm() }, steps: [] };
  let base = vis.slice();
  for (let k = 0; k < snaps; k++) {
    // the server's vision pass lags the units (5 Hz): the mask comes from where they stood before this step
    see();
    for (const u of walkers) {
      u.x += u.vx * CELL; u.z += u.vz * CELL;
      if (u.x < 0 || u.x > w * CELL) { u.vx = -u.vx; u.x = Math.min(w * CELL - 0.01, Math.max(0, u.x)); }
      if (u.z < 0 || u.z > h * CELL) { u.vz = -u.vz; u.z = Math.min(h * CELL - 0.01, Math.max(0, u.z)); }
    }
    const delta = packRuns(vis, base); base = vis.slice();
    // units as snapshot rows [id, type, owner, x, z, ...]; one row off the map
    const rows = walkers.map((u, i) => [i + 1, u.type, 0, u.x, u.z]);
    if (k % 3 === 0) rows.push([99, 'rifle', 1, -3.5, 4]);
    const s = { fog: delta, units: rows };
    fog.snapshot(s);
    const liftNow = lift.includes(k);
    const dts = [0.016, 0.05, 0.1];
    const frames = [];
    for (const dt of dts) { fog.frame(dt, liftNow); frames.push({ dt, alpha: alphaNow() }); }
    fog.frame(1.0, liftNow);
    out.steps.push({ flips: cells(delta), units: rows.map((r) => [r[1], r[3], r[4]]), lift: liftNow, frames, settled: alphaNow(), looks: looks(), minimap: mm(), version: fog.version });
  }
  return out;
}
const scenarios = [
  scenario({ name: 'authored-player', w: 40, h: 34, opaque: false }),
  scenario({ name: 'authored-player-lifted-at-end', w: 30, h: 26, opaque: false, lift: [9, 10, 11, 12, 13] }),
  scenario({ name: 'authored-spectator-no-mask', w: 26, h: 22, opaque: false, keyless: true }),
  scenario({ name: 'world-player-frontier', w: 64, h: 56, opaque: true, snaps: 18, units: 7, radius: 6 }),
  scenario({ name: 'world-lift-ignored', w: 36, h: 30, opaque: true, lift: [4, 5, 6] }),
  scenario({ name: 'editor-off', w: 20, h: 16, opaque: false, off: true }),
];
const meta = { reference: fs.readFileSync(path.join(ref, '..', 'ref.commit'), 'utf8').trim(), fogJsSha256: (await import('node:crypto')).createHash('sha256').update(fs.readFileSync(path.join(ref, 'client/fog.js'))).digest('hex'),
  buildingSizes: Object.fromEntries(Object.entries(UNITS).filter(([, d]) => d.building).map(([k, d]) => [k, d.size])), air: Object.keys(UNITS).filter((k) => UNITS[k].air) };
fs.writeFileSync(outFile, JSON.stringify({ meta, scenarios }));
console.log('scenarios', scenarios.length, 'steps', scenarios.reduce((a, s) => a + s.steps.length, 0), 'cells', scenarios.reduce((a, s) => a + s.w * s.h, 0));
