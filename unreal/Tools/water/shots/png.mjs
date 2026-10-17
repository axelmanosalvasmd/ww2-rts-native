// A minimal PNG reader (8-bit RGB or RGBA, non-interlaced: what the engine's screenshots and Chromium write), no
// dependencies. readPng(path) -> { w, h, px(x, y) -> [r, g, b] }.
import { readFileSync } from 'node:fs';
import { inflateSync } from 'node:zlib';

export function readPng(path) {
  const b = readFileSync(path);
  let o = 8, w = 0, h = 0, depth = 0, type = 0;
  const idat = [];
  while (o < b.length) {
    const len = b.readUInt32BE(o), kind = b.toString('ascii', o + 4, o + 8), data = b.subarray(o + 8, o + 8 + len);
    if (kind === 'IHDR') { w = data.readUInt32BE(0); h = data.readUInt32BE(4); depth = data[8]; type = data[9]; if (data[12]) throw new Error('interlaced PNG'); }
    else if (kind === 'IDAT') idat.push(data);
    o += 12 + len;
  }
  if (depth !== 8 || (type !== 2 && type !== 6)) throw new Error(`unsupported PNG: depth ${depth} type ${type}`);
  const bpp = type === 6 ? 4 : 3, stride = w * bpp, raw = inflateSync(Buffer.concat(idat)), out = Buffer.alloc(h * stride);
  for (let y = 0; y < h; y++) {
    const f = raw[y * (stride + 1)], src = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)), row = out.subarray(y * stride, (y + 1) * stride), up = y ? out.subarray((y - 1) * stride, y * stride) : null;
    for (let i = 0; i < stride; i++) {
      const a = i >= bpp ? row[i - bpp] : 0, u = up ? up[i] : 0, c = up && i >= bpp ? up[i - bpp] : 0;
      let v = src[i];
      if (f === 1) v += a; else if (f === 2) v += u; else if (f === 3) v += (a + u) >> 1;
      else if (f === 4) { const p = a + u - c, pa = Math.abs(p - a), pb = Math.abs(p - u), pc = Math.abs(p - c); v += pa <= pb && pa <= pc ? a : pb <= pc ? u : c; }
      row[i] = v & 255;
    }
  }
  return { w, h, px: (x, y) => { const i = y * stride + x * bpp; return [out[i], out[i + 1], out[i + 2]]; } };
}

// writePng(path, w, h, rgb Buffer of w * h * 3): 8-bit RGB, filter 0
import { writeFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';
const crcTable = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
const crc = (buf) => { let c = 0xffffffff; for (const b of buf) c = crcTable[(c ^ b) & 255] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
const chunk = (kind, data) => { const l = Buffer.alloc(4); l.writeUInt32BE(data.length); const kd = Buffer.concat([Buffer.from(kind, 'ascii'), data]); const c = Buffer.alloc(4); c.writeUInt32BE(crc(kd)); return Buffer.concat([l, kd, c]); };
export function writePng(path, w, h, rgb) {
  const raw = Buffer.alloc(h * (w * 3 + 1));
  for (let y = 0; y < h; y++) rgb.copy(raw, y * (w * 3 + 1) + 1, y * w * 3, (y + 1) * w * 3);
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 2;
  writeFileSync(path, Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw, { level: 9 })), chunk('IEND', Buffer.alloc(0))]));
}
