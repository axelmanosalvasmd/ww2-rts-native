// Color parity on the unit's own pixels (README.md, "Metrics"). For every asset and view of a look.mjs output folder:
//   - the unit's mean color in the reference (inside its silhouette, look.mjs's mask) and in each Unreal light (inside
//     the Unreal silhouette, from scene depth with the ground hidden); both masks are eroded by 2 px so the silhouette's
//     antialiased edge, which mixes in the ground, is left out
//   - the delta of those means: per channel (0-255, sRGB as displayed), as a Euclidean distance, and as CIE76 Delta E
//     (Lab, D65) of the two means
//   - the ground: the plate (same camera, unit hidden) over the lower half of the frame, outside both masks dilated
//   - the masks' intersection over union (the two renderers agree on the shape; low values flag a pose or framing gap)
//   node unreal/Tools/look/metrics.mjs --dir <look out set> [--dir ...] --lights before,gamebefore,look --out <file.json>
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';

const args = process.argv.slice(2);
const all = (k) => args.flatMap((a, i) => (a === `--${k}` ? [args[i + 1]] : []));
const one = (k, d) => all(k)[0] ?? d;
const dirs = all('dir'), lights = one('lights', 'before,gamebefore,look').split(','), out = one('out');
if (!dirs.length || !out) { console.error('usage: metrics.mjs --dir <set> [--dir ...] --lights a,b --out <json>'); process.exit(2); }

function read(file, W, H) {
  if (!fs.existsSync(file)) return null;
  const b = execFileSync('magick', [file, '-alpha', 'off', '-depth', '8', 'rgb:-'], { maxBuffer: 1 << 28 });
  return b.length === W * H * 3 ? b : null;
}
function mask(file, W, H) {
  const b = read(file, W, H);
  if (!b) return null;
  const m = new Uint8Array(W * H);
  for (let i = 0; i < W * H; i++) m[i] = b[i * 3] > 127 ? 1 : 0;
  return m;
}
function morph(m, W, H, r, grow) {
  // a square erosion (grow false) or dilation (grow true) of radius r, as two separable passes
  const pass = (src, dx, dy) => {
    const dst = new Uint8Array(W * H);
    for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) {
      let v = grow ? 0 : 1;
      for (let k = -r; k <= r; k++) {
        const xx = x + k * dx, yy = y + k * dy;
        const s = xx < 0 || yy < 0 || xx >= W || yy >= H ? 0 : src[yy * W + xx];
        if (grow ? s : !s) { v = grow ? 1 : 0; break; }
      }
      dst[y * W + x] = v;
    }
    return dst;
  };
  return pass(pass(m, 1, 0), 0, 1);
}
function mean(img, m, W, H, where = () => true) {
  let r = 0, g = 0, b = 0, n = 0;
  for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) {
    const i = y * W + x;
    if (!m[i] || !where(x, y)) continue;
    r += img[i * 3]; g += img[i * 3 + 1]; b += img[i * 3 + 2]; n++;
  }
  return n ? { rgb: [r / n, g / n, b / n], n } : { rgb: null, n: 0 };
}
const lin = (c) => { c /= 255; return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4; };
function lab(rgb) {
  const [r, g, b] = rgb.map(lin);
  const X = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047, Y = 0.2126 * r + 0.7152 * g + 0.0722 * b, Z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
  const f = (t) => (t > 216 / 24389 ? Math.cbrt(t) : (24389 / 27 * t + 16) / 116);
  return [116 * f(Y) - 16, 500 * (f(X) - f(Y)), 200 * (f(Y) - f(Z))];
}
const deltaE = (a, b) => { const A = lab(a), B = lab(b); return Math.hypot(A[0] - B[0], A[1] - B[1], A[2] - B[2]); };
const round = (v, k = 2) => (v === null || v === undefined ? null : Array.isArray(v) ? v.map((x) => +x.toFixed(k)) : +v.toFixed(k));

const rows = [];
for (const dir of dirs) {
  const cams = JSON.parse(fs.readFileSync(path.join(dir, 'cameras.json'), 'utf8'));
  const W = cams.width, H = cams.height;
  for (const a of cams.assets) {
    if (a.type === 'calib' || a.type === 'horizon') continue;
    for (const v of a.views) {
      const base = path.join(dir, `${a.name}-${v.label}`);
      const refImg = read(`${base}-reference.png`, W, H), refMask = mask(`${base}-mask-reference.png`, W, H), ueMask = mask(`${base}-mask-unreal.png`, W, H);
      if (!refImg || !refMask || !ueMask) { rows.push({ set: path.basename(dir), asset: a.name, view: v.label, error: 'missing reference, mask or Unreal mask' }); continue; }
      const refIn = morph(refMask, W, H, 2, false), ueIn = morph(ueMask, W, H, 2, false);
      let inter = 0, union = 0;
      for (let i = 0; i < W * H; i++) { inter += refMask[i] & ueMask[i]; union += refMask[i] | ueMask[i]; }
      const outside = morph(Uint8Array.from(refMask, (x, i) => x | ueMask[i]), W, H, 6, true).map((x) => 1 - x);
      const lower = (x, y) => y >= H / 2;
      const both = Uint8Array.from(refIn, (x, i) => x & ueIn[i]); // the pixels both renderers call the unit's
      const ref = mean(refImg, refIn, W, H), refBoth = mean(refImg, both, W, H), refPlate = mean(read(`${base}-plate-reference.png`, W, H) ?? refImg, outside, W, H, lower);
      const row = { set: path.basename(dir), asset: a.name, type: a.type, fac: a.fac, view: v.label, iou: round(union ? inter / union : 0, 3), refPixels: ref.n, ref: round(ref.rgb), refGround: round(refPlate.rgb), lights: {} };
      for (const L of lights) {
        const img = read(`${base}-unreal-${L}.png`, W, H);
        if (!img || !ref.rgb) { row.lights[L] = null; continue; }
        const ue = mean(img, ueIn, W, H), ueBoth = mean(img, both, W, H), plate = read(`${base}-plate-unreal-${L}.png`, W, H);
        const g = plate ? mean(plate, outside, W, H, lower) : { rgb: null };
        // primary: the means over the shared pixels; own: each renderer's own silhouette
        const d = ueBoth.rgb && refBoth.rgb ? ueBoth.rgb.map((c, k) => c - refBoth.rgb[k]) : null;
        row.lights[L] = { mean: round(ueBoth.rgb), refMean: round(refBoth.rgb), pixels: ueBoth.n, delta: round(d), dist: round(d && Math.hypot(...d)), dE: round(d && deltaE(ueBoth.rgb, refBoth.rgb)),
          ownMean: round(ue.rgb), ownDE: round(ue.rgb && deltaE(ue.rgb, ref.rgb)),
          ground: round(g.rgb), groundDE: round(g.rgb && refPlate.rgb && deltaE(g.rgb, refPlate.rgb)) };
      }
      rows.push(row);
    }
  }
}
// per asset: the views averaged
const assets = new Map();
for (const r of rows) {
  if (r.error) continue;
  const key = `${r.set}/${r.asset}`;
  if (!assets.has(key)) assets.set(key, { set: r.set, asset: r.asset, views: 0, iou: 0, lights: Object.fromEntries(lights.map((L) => [L, { dE: 0, dist: 0, n: 0, delta: [0, 0, 0], groundDE: 0, g: 0 }])) });
  const A = assets.get(key);
  A.views++; A.iou += r.iou;
  for (const L of lights) {
    const x = r.lights[L];
    if (!x || x.dE === null) continue;
    const S = A.lights[L];
    S.dE += x.dE; S.dist += x.dist; S.n++; x.delta.forEach((v, k) => { S.delta[k] += v; });
    if (x.groundDE !== null) { S.groundDE += x.groundDE; S.g++; }
  }
}
const summary = [...assets.values()].map((A) => ({ set: A.set, asset: A.asset, views: A.views, iou: round(A.iou / A.views, 3),
  lights: Object.fromEntries(lights.map((L) => { const S = A.lights[L]; return [L, S.n ? { dE: round(S.dE / S.n), dist: round(S.dist / S.n), delta: round(S.delta.map((v) => v / S.n)), groundDE: S.g ? round(S.groundDE / S.g) : null } : null]; })) }));
const totals = Object.fromEntries(lights.map((L) => {
  const xs = summary.map((s) => s.lights[L]).filter(Boolean);
  const avg = (f) => round(xs.reduce((t, x) => t + f(x), 0) / Math.max(1, xs.length));
  return [L, { assets: xs.length, meanDE: avg((x) => x.dE), meanDist: avg((x) => x.dist), meanGroundDE: avg((x) => x.groundDE ?? 0), medianDE: round(xs.map((x) => x.dE).sort((a, b) => a - b)[Math.floor(xs.length / 2)] ?? null) }];
}));
fs.writeFileSync(out, JSON.stringify({ lights, totals, assets: summary, views: rows }, null, 1));
console.log(JSON.stringify(totals, null, 1));
