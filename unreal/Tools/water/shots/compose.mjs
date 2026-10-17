// Side by side sheets: for each pose, the reference frame (left) and the native frame (right), each halved (2 x 2 box)
// and split by a 4-pixel white bar. Also prints, per frame, how much of it is near black (all channels under 12): a
// void the apron or the background let through would show there.
//   node compose.mjs <reference dir> <native dir> <out dir> <prefix>
import { readdirSync, mkdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { readPng, writePng } from './png.mjs';

const [refDir, natDir, outDir, prefix] = process.argv.slice(2);
mkdirSync(outDir, { recursive: true });
const half = (img) => {
  const w = img.w >> 1, h = img.h >> 1, out = Buffer.alloc(w * h * 3);
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) for (let k = 0; k < 3; k++)
    out[(y * w + x) * 3 + k] = (img.px(2 * x, 2 * y)[k] + img.px(2 * x + 1, 2 * y)[k] + img.px(2 * x, 2 * y + 1)[k] + img.px(2 * x + 1, 2 * y + 1)[k] + 2) >> 2;
  return { w, h, out };
};
const black = (img) => { let n = 0; for (let y = 0; y < img.h; y += 2) for (let x = 0; x < img.w; x += 2) { const [r, g, b] = img.px(x, y); if (r < 12 && g < 12 && b < 12) n++; } return n / ((img.h >> 1) * (img.w >> 1)); };
const report = [];
for (const f of readdirSync(natDir).filter(f => f.endsWith('.png')).sort()) {
  if (!existsSync(join(refDir, f))) continue;
  const r = readPng(join(refDir, f)), n = readPng(join(natDir, f)), a = half(r), b = half(n), bar = 4, W = a.w + bar + b.w, H = Math.max(a.h, b.h);
  const rgb = Buffer.alloc(W * H * 3, 255);
  for (let y = 0; y < a.h; y++) a.out.copy(rgb, y * W * 3, y * a.w * 3, (y + 1) * a.w * 3);
  for (let y = 0; y < b.h; y++) b.out.copy(rgb, (y * W + a.w + bar) * 3, y * b.w * 3, (y + 1) * b.w * 3);
  const name = `${prefix}-${f.replace(/\.png$/, '')}-reference-left-native-right.png`;
  writePng(join(outDir, name), W, H, rgb);
  const row = { pose: f, sheet: name, nearBlackReference: +black(r).toFixed(4), nearBlackNative: +black(n).toFixed(4) };
  report.push(row);
  console.log(JSON.stringify(row));
}
