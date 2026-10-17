// Compares the Unreal port's paper and desk (-run=TCWarMapPaint) with the reference's own canvases
// (make-warmap.mjs oracle), pixel for pixel: mean and percentile absolute differences per channel, the share of pixels
// off by more than 8 and 32 levels, and a sheet (reference | port | difference x 4) as PNG.
//   node unreal/Tools/warmap/compare-oracle.mjs <reference.png> <port.png> <sheet.png> [label]
// PNG reading and writing with node's zlib only (8-bit RGB or RGBA, not interlaced: what Chromium and Unreal write).
import fs from 'node:fs';
import zlib from 'node:zlib';

export function readPng(file) {
  const b = fs.readFileSync(file);
  let p = 8, w = 0, h = 0, ct = 0, depth = 0, inter = 0; const idat = [];
  while (p < b.length) {
    const len = b.readUInt32BE(p), type = b.toString('ascii', p + 4, p + 8), data = b.subarray(p + 8, p + 8 + len);
    if (type === 'IHDR') { w = data.readUInt32BE(0); h = data.readUInt32BE(4); depth = data[8]; ct = data[9]; inter = data[12]; }
    else if (type === 'IDAT') idat.push(data);
    p += 12 + len;
  }
  if (depth !== 8 || inter || (ct !== 6 && ct !== 2)) throw new Error(`${file}: unsupported PNG (depth ${depth}, colour type ${ct}, interlace ${inter})`);
  const bpp = ct === 6 ? 4 : 3, raw = zlib.inflateSync(Buffer.concat(idat)), stride = w * bpp, out = Buffer.alloc(w * h * 4), prev = Buffer.alloc(stride), cur = Buffer.alloc(stride);
  for (let y = 0; y < h; y++) {
    const f = raw[y * (stride + 1)], line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    for (let i = 0; i < stride; i++) {
      const a = i >= bpp ? cur[i - bpp] : 0, up = prev[i], c = i >= bpp ? prev[i - bpp] : 0;
      let v = line[i];
      if (f === 1) v += a; else if (f === 2) v += up; else if (f === 3) v += (a + up) >> 1;
      else if (f === 4) { const pp = a + up - c, pa = Math.abs(pp - a), pb = Math.abs(pp - up), pc = Math.abs(pp - c); v += pa <= pb && pa <= pc ? a : pb <= pc ? up : c; }
      cur[i] = v & 255;
    }
    for (let x = 0; x < w; x++) { out[(y * w + x) * 4] = cur[x * bpp]; out[(y * w + x) * 4 + 1] = cur[x * bpp + 1]; out[(y * w + x) * 4 + 2] = cur[x * bpp + 2]; out[(y * w + x) * 4 + 3] = bpp === 4 ? cur[x * bpp + 3] : 255; }
    cur.copy(prev);
  }
  return { w, h, data: out };
}

export function writePng(file, w, h, rgba) {
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) { raw[y * (w * 4 + 1)] = 0; rgba.copy(raw, y * (w * 4 + 1) + 1, y * w * 4, (y + 1) * w * 4); }
  const crcTable = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
  const crc = (buf) => { let c = 0xffffffff; for (const x of buf) c = crcTable[(c ^ x) & 255] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
  const chunk = (type, data) => { const len = Buffer.alloc(4); len.writeUInt32BE(data.length); const td = Buffer.concat([Buffer.from(type, 'ascii'), data]); const c = Buffer.alloc(4); c.writeUInt32BE(crc(td)); return Buffer.concat([len, td, c]); };
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 6;
  fs.writeFileSync(file, Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(raw, { level: 6 })), chunk('IEND', Buffer.alloc(0))]));
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const [ra, pa, sheet, label = ''] = process.argv.slice(2);
  const A = readPng(ra), B = readPng(pa);
  if (A.w !== B.w || A.h !== B.h) { console.log(JSON.stringify({ label, error: `size ${A.w}x${A.h} vs ${B.w}x${B.h}` })); process.exit(1); }
  const n = A.w * A.h, diffs = new Uint32Array(256); let sum = 0, over8 = 0, over32 = 0;
  const S = Buffer.alloc(A.w * 3 * A.h * 4);
  for (let i = 0; i < n; i++) {
    let m = 0;
    for (let k = 0; k < 3; k++) { const d = Math.abs(A.data[i * 4 + k] - B.data[i * 4 + k]); sum += d; m = Math.max(m, d); }
    diffs[m]++; if (m > 8) over8++; if (m > 32) over32++;
    const y = Math.floor(i / A.w), x = i % A.w;
    for (let k = 0; k < 4; k++) { S[(y * A.w * 3 + x) * 4 + k] = A.data[i * 4 + k]; S[(y * A.w * 3 + A.w + x) * 4 + k] = B.data[i * 4 + k]; }
    const v = Math.min(255, m * 4); S[(y * A.w * 3 + 2 * A.w + x) * 4] = v; S[(y * A.w * 3 + 2 * A.w + x) * 4 + 1] = v; S[(y * A.w * 3 + 2 * A.w + x) * 4 + 2] = v; S[(y * A.w * 3 + 2 * A.w + x) * 4 + 3] = 255;
  }
  const pct = (q) => { let c = 0; for (let v = 0; v < 256; v++) { c += diffs[v]; if (c >= q * n) return v; } return 255; };
  writePng(sheet, A.w * 3, A.h, S);
  console.log(JSON.stringify({ label, size: `${A.w}x${A.h}`, meanAbsPerChannel: +(sum / n / 3).toFixed(3), p50: pct(0.5), p99: pct(0.99), p999: pct(0.999), max: pct(1), over8: +(over8 / n * 100).toFixed(3) + '%', over32: +(over32 / n * 100).toFixed(3) + '%' }));
}
