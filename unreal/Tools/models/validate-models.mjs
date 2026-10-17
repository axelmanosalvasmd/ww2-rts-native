// Checks every .glb under the export directory against manifest.json:
//   1. the Khronos glTF-Validator (npm gltf-validator) when --validator DIR points at a directory where it is
//      installed (npm i gltf-validator), else that step is reported as skipped
//   2. own structural checks: GLB header and chunks, accessor ranges inside the BIN chunk, index values below the
//      vertex count, finite floats, POSITION min/max equal to the data, attribute counts equal per primitive, the
//      file sha256 and vertex and triangle counts equal to the manifest entry
// Usage: node unreal/Tools/models/validate-models.mjs [--out DIR] [--validator DIR] [--report FILE]
import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { readGlb } from './gltf-writer.mjs';

const argv = process.argv.slice(2), opt = (k, d) => { const i = argv.indexOf(`--${k}`); return i >= 0 ? argv[i + 1] : d; };
const out = resolve(opt('out', join(import.meta.dirname, '../../Saved/ModelExport')));
const validatorDir = opt('validator', null), reportFile = opt('report', null);
const manifest = JSON.parse(readFileSync(join(out, 'manifest.json'), 'utf8'));
const validator = validatorDir ? createRequire(join(resolve(validatorDir), 'package.json'))('gltf-validator') : null;

const TYPES = { 5126: Float32Array, 5123: Uint16Array, 5125: Uint32Array, 5121: Uint8Array };
const N = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT4: 16 };
function structural(buf, entry) {
  const errors = [], { json, bin } = readGlb(buf);
  if (json.asset?.version !== '2.0') errors.push('asset.version is not 2.0');
  const read = (ai) => {
    const a = json.accessors[ai], v = json.bufferViews[a.bufferView], T = TYPES[a.componentType], n = a.count * N[a.type];
    if (v.byteOffset + v.byteLength > bin.length) throw new Error(`bufferView ${a.bufferView} past BIN`);
    if (n * T.BYTES_PER_ELEMENT > v.byteLength) throw new Error(`accessor ${ai} longer than its view`);
    if ((v.byteOffset + (a.byteOffset ?? 0)) % T.BYTES_PER_ELEMENT) throw new Error(`accessor ${ai} misaligned`);
    return new T(bin.buffer.slice(bin.byteOffset + v.byteOffset, bin.byteOffset + v.byteOffset + n * T.BYTES_PER_ELEMENT));
  };
  let vertices = 0, triangles = 0;
  for (const mesh of json.meshes ?? []) for (const p of mesh.primitives) {
    const count = json.accessors[p.attributes.POSITION].count;
    vertices += count;
    for (const [name, ai] of [...Object.entries(p.attributes), ...(p.targets ?? []).flatMap((t) => Object.entries(t))]) {
      const a = json.accessors[ai];
      if (a.count !== count) errors.push(`${mesh.name} ${name}: ${a.count} != ${count}`);
      const data = read(ai);
      if (data instanceof Float32Array) for (const x of data) if (!Number.isFinite(x)) { errors.push(`${mesh.name} ${name}: non-finite value`); break; }
      if (a.min) {
        const n = N[a.type], min = new Array(n).fill(Infinity), max = new Array(n).fill(-Infinity);
        for (let i = 0; i < data.length; i++) { const k = i % n; if (data[i] < min[k]) min[k] = data[i]; if (data[i] > max[k]) max[k] = data[i]; }
        if (min.some((x, k) => x !== a.min[k]) || max.some((x, k) => x !== a.max[k])) errors.push(`${mesh.name} ${name}: min/max differ from data`);
      }
    }
    if (p.indices !== undefined) {
      const idx = read(p.indices);
      triangles += idx.length / 3;
      if (idx.length % 3) errors.push(`${mesh.name}: index count not a multiple of 3`);
      for (const i of idx) if (i >= count) { errors.push(`${mesh.name}: index ${i} >= ${count}`); break; }
    } else triangles += count / 3;
  }
  if (entry) {
    if (entry.uniqueVertices !== vertices) errors.push(`manifest vertices ${entry.uniqueVertices} != ${vertices}`);
    if (entry.uniqueTriangles !== triangles) errors.push(`manifest triangles ${entry.uniqueTriangles} != ${triangles}`);
  }
  return errors;
}

const files = manifest.assets.flatMap((a) => (a.files ?? []).map((f) => ({ asset: a.id, ...f })));
files.push(...manifest.copied.filter((c) => c.file.endsWith('.glb')).map((c) => ({ asset: c.file, file: c.file, sha256: c.sha256, copied: true })));
const report = { out, validator: validator ? { name: 'gltf-validator', version: validator.version() } : 'skipped (no --validator)', files: [] };
let bad = 0;
for (const f of files) {
  const buf = readFileSync(join(out, f.file)), r = { file: f.file };
  r.sha256 = createHash('sha256').update(buf).digest('hex') === f.sha256 ? 'match' : 'MISMATCH';
  try { r.structural = structural(buf, f.copied ? null : f); } catch (e) { r.structural = [String(e.message)]; }
  if (validator) {
    const externalResourceFunction = (uri) => new Promise((ok, fail) => { try { ok(new Uint8Array(readFileSync(join(out, dirname(f.file), decodeURIComponent(uri))))); } catch (e) { fail(String(e)); } });
    const v = await validator.validateBytes(new Uint8Array(buf), { uri: f.file, maxIssues: 50, writeTimestamp: false, externalResourceFunction });
    r.validator = { errors: v.issues.numErrors, warnings: v.issues.numWarnings, infos: v.issues.numInfos, hints: v.issues.numHints, messages: v.issues.messages.filter((m) => m.severity <= 1).slice(0, 10).map((m) => `${m.severity === 0 ? 'E' : 'W'} ${m.code} ${m.pointer ?? ''} ${m.message}`) };
  }
  const failed = r.sha256 !== 'match' || r.structural.length || (r.validator?.errors ?? 0) > 0;
  if (failed) bad++;
  report.files.push(r);
  if (failed || r.validator?.warnings) console.log(`${failed ? 'FAIL' : 'warn'} ${f.file} ${r.sha256} ${r.structural.join('; ')} ${r.validator ? `E${r.validator.errors} W${r.validator.warnings} ${r.validator.messages.join(' | ')}` : ''}`);
}
report.summary = { files: files.length, failed: bad, validatorErrors: report.files.reduce((n, r) => n + (r.validator?.errors ?? 0), 0), validatorWarnings: report.files.reduce((n, r) => n + (r.validator?.warnings ?? 0), 0), validatorInfos: report.files.reduce((n, r) => n + (r.validator?.infos ?? 0), 0) };
if (reportFile) writeFileSync(reportFile, JSON.stringify(report, null, 1) + '\n');
console.log(JSON.stringify(report.summary));
process.exit(bad ? 1 : 0);
