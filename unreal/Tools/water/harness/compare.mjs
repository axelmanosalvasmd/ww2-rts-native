// Compares the harness's port.*.bin with the oracle's *.bin in every case directory and prints one line per case and
// a verdict. Tolerances: positions 1e-4 m, flow and normals 1e-5 (the reference stores Float32; both sides compute in
// doubles), indices and every mask, cell and noise texel exact (noise: at most 1 step on a byte, see README).
import { readFileSync, existsSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = process.argv[2];
const out = process.argv[3];
const f32 = (p) => { const b = readFileSync(p); return new Float32Array(b.buffer, b.byteOffset, b.byteLength / 4); };
const u32 = (p) => { const b = readFileSync(p); return new Uint32Array(b.buffer, b.byteOffset, b.byteLength / 4); };
const u8 = (p) => new Uint8Array(readFileSync(p));
let fail = 0;
const report = { cases: [], noise: {} };
function maxDiff(a, b) { if (a.length !== b.length) return Infinity; let m = 0; for (let i = 0; i < a.length; i++) m = Math.max(m, Math.abs(a[i] - b[i])); return m; }
function bytes(a, b) { if (a.length !== b.length) return { diff: Infinity, max: Infinity }; let d = 0, m = 0; for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) { d++; m = Math.max(m, Math.abs(a[i] - b[i])); } return { diff: d, max: m }; }
for (const k of ['water-noise', 'ground-noise']) {
  const r = bytes(u8(join(root, `${k}.bin`)), u8(join(root, `port.${k}.bin`)));
  report.noise[k] = r;
  if (r.max > 1) fail++;
  console.log(`${k}: ${r.diff} of ${u8(join(root, `${k}.bin`)).length} bytes differ, by at most ${r.max}`);
}
const dirs = readdirSync(root, { withFileTypes: true }).filter(d => d.isDirectory() && existsSync(join(root, d.name, 'input.txt'))).map(d => d.name).sort();
for (const name of dirs) {
  const d = join(root, name), row = { name, water: [], apron: null };
  for (let i = 0; existsSync(join(d, `water${i}.pos.bin`)) || existsSync(join(d, `water${i}.none`)); i++) {
    const ref = existsSync(join(d, `water${i}.pos.bin`)), port = existsSync(join(d, `port.water${i}.pos.bin`));
    if (!ref || !port) { const same = ref === port; row.water.push({ step: i, none: !ref, same }); if (!same) fail++; continue; }
    const r = {
      step: i,
      verts: f32(join(d, `water${i}.pos.bin`)).length / 3,
      pos: maxDiff(f32(join(d, `water${i}.pos.bin`)), f32(join(d, `port.water${i}.pos.bin`))),
      flow: maxDiff(f32(join(d, `water${i}.flow.bin`)), f32(join(d, `port.water${i}.flow.bin`))),
      nor: maxDiff(f32(join(d, `water${i}.nor.bin`)), f32(join(d, `port.water${i}.nor.bin`))),
      index: bytes(u32(join(d, `water${i}.index.bin`)), u32(join(d, `port.water${i}.index.bin`))).diff,
      tris: u32(join(d, `water${i}.index.bin`)).length / 3,
      mask: bytes(u8(join(d, `water${i}.mask.bin`)), u8(join(d, `port.water${i}.mask.bin`))).diff,
    };
    r.ok = r.pos <= 1e-4 && r.flow <= 1e-5 && r.nor <= 1e-5 && r.index === 0 && r.mask === 0;
    if (!r.ok) fail++;
    row.water.push(r);
  }
  const a = {
    pos: maxDiff(f32(join(d, 'apron0.pos.bin')), f32(join(d, 'port.apron0.pos.bin'))),
    nor: maxDiff(f32(join(d, 'apron0.nor.bin')), f32(join(d, 'port.apron0.nor.bin'))),
    index: bytes(u32(join(d, 'apron0.index.bin')), u32(join(d, 'port.apron0.index.bin'))).diff,
    cells: bytes(u8(join(d, 'apron0.cells.bin')), u8(join(d, 'port.apron0.cells.bin'))).diff,
    cloud: u32(join(d, 'apron0.cloud.bin'))[0] === u32(join(d, 'port.apron0.cloud.bin'))[0],
    verts: f32(join(d, 'apron0.pos.bin')).length / 3,
  };
  a.ok = a.pos <= 1e-4 && a.nor <= 1e-5 && a.index === 0 && a.cells === 0 && a.cloud;
  if (!a.ok) fail++;
  row.apron = a;
  report.cases.push(row);
  const w = row.water.map(r => r.none ? `${r.step}:none${r.same ? '' : ' MISMATCH'}` : `${r.step}:${r.ok ? 'ok' : 'FAIL'} pos ${r.pos.toExponential(1)} flow ${r.flow.toExponential(1)} idx ${r.index} mask ${r.mask}`).join(', ');
  console.log(`${name}: apron ${a.ok ? 'ok' : 'FAIL'} (${a.verts} verts, pos ${a.pos.toExponential(1)}, nor ${a.nor.toExponential(1)}, idx ${a.index}, cells ${a.cells}); water ${w}`);
}
report.failures = fail;
if (out) writeFileSync(out, JSON.stringify(report, null, 1));
console.log(fail ? `FAIL: ${fail} mismatches` : `PASS: ${report.cases.length} cases`);
process.exit(fail ? 1 : 0);
