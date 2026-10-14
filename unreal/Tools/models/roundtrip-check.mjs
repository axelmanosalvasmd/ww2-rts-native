// Reads the exported .glb files back and compares them with a fresh build of the same assets by the reference, in a
// player color the file was not written in (default slot1). Run like build-worker.mjs:
//   WW2_MODEL_DEPS=DIR node --import ./register.mjs roundtrip-check.mjs --reference DIR --out DIR [--look slot1] [--only REGEX]
// Per mesh node, by name: POSITION and indices equal bit for bit; _MATID equal; the color rebuilt from the file
// (COLOR_0 or _COLOR, plus the tint terms for the look's owner color) within 1e-5 of the build; base + morph delta
// within 1e-5 of the built absolute target; node translation, rotation and scale equal.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { installShims, settle } from './shims.mjs';
import { extract } from './extract.mjs';
import { readGlb } from './gltf-writer.mjs';
import { catalog, loadReference, lookFor, readLooks } from './catalog.mjs';

const args = Object.fromEntries(process.argv.slice(2).reduce((a, x, i, all) => (x.startsWith('--') ? [...a, [x.slice(2), all[i + 1]]] : a), []));
installShims();
const R = await loadReference(args.reference), { THREE } = R, looks = readLooks(args.reference);
const look = lookFor(args.look ?? 'slot1', looks), only = args.only ? new RegExp(args.only) : null;
const manifest = JSON.parse(readFileSync(join(args.out, 'manifest.json'), 'utf8'));
const lin = (hex) => { const c = new THREE.Color(hex); return [c.r, c.g, c.b]; };
const TYPES = { 5126: Float32Array, 5123: Uint16Array, 5125: Uint32Array };
const SIZE = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4 };
const { list } = catalog(R, args.reference);
const report = [];
let failures = 0;
for (const entry of list) {
  if (only && !only.test(entry.id)) continue;
  const m = manifest.assets.find((a) => a.id === entry.id);
  if (!m?.files?.length) { report.push({ id: entry.id, error: 'not in manifest' }); failures++; continue; }
  const file = m.files.find((f) => !/-slot\d\.glb$/.test(f.file));
  const { json, bin } = readGlb(readFileSync(join(args.out, file.file)));
  const acc = (i) => { const a = json.accessors[i], v = json.bufferViews[a.bufferView]; return new TYPES[a.componentType](bin.buffer.slice(bin.byteOffset + v.byteOffset, bin.byteOffset + v.byteOffset + a.count * SIZE[a.type] * TYPES[a.componentType].BYTES_PER_ELEMENT)); };
  const built = entry.build(entry.lookDependent ? look : lookFor('base', looks));
  await settle();
  const data = extract(built.root, { names: built.names, instances: built.instances, trackUniform: R.trackMotion.trackUniform, morphNames: (n) => (built.morphNames ? built.morphNames(n) : Array.from({ length: n }, (_, i) => `target_${i}`)), describeMaterial: () => ({}) });
  const owner = entry.lookDependent ? lin(look.owner) : null, def = lin(looks.fallback);
  const problems = [], stats = { meshes: 0, maxColorError: 0, maxMorphError: 0 };
  const byName = new Map(json.nodes.map((n) => [n.name, n]));
  const visit = (node) => {
    const g = byName.get(node.name);
    if (!g) { problems.push(`node ${node.name} missing`); return; }
    if (node !== data.root) {
      const t = g.translation ?? [0, 0, 0], r = g.rotation ?? [0, 0, 0, 1], s = g.scale ?? [1, 1, 1];
      if (node.t.some((x, k) => x !== t[k]) || node.r.some((x, k) => x !== r[k]) || node.s.some((x, k) => x !== s[k])) problems.push(`node ${node.name} transform differs`);
    }
    if (node.mesh) {
      stats.meshes++;
      const geo = data.geometries[node.mesh.geo], p = json.meshes[g.mesh].primitives[0], A = p.attributes;
      const P = acc(A.POSITION), BP = geo.attrs.position.array;
      if (P.length !== BP.length || P.some((x, i) => x !== BP[i])) problems.push(`${node.name}: POSITION differs`);
      if (geo.index) { const I = acc(p.indices); if (I.length !== geo.index.length || I.some((x, i) => x !== geo.index[i])) problems.push(`${node.name}: indices differ`); }
      if (geo.attrs.matId) { const M = acc(A._MATID); if (M.some((x, i) => x !== geo.attrs.matId.array[i])) problems.push(`${node.name}: _MATID differs`); }
      if (geo.attrs.color) {
        const C = acc(A._COLOR ?? A.COLOR_0), OT = A._OWNERTINT !== undefined ? acc(A._OWNERTINT) : null, BC = geo.attrs.color.array;
        for (let i = 0; i < BC.length; i++) {
          const pred = C[i] + (OT ? (owner[i % 3] - def[i % 3]) * OT[i] : 0);
          stats.maxColorError = Math.max(stats.maxColorError, Math.abs(pred - BC[i]));
        }
      }
      if (geo.morph) geo.morph.position.forEach((abs, k) => {
        const D = acc(p.targets[k].POSITION);
        for (let i = 0; i < abs.length; i++) stats.maxMorphError = Math.max(stats.maxMorphError, Math.abs(BP[i] + D[i] - abs[i]));
      });
    }
    node.children.forEach(visit);
  };
  visit(data.root);
  if (stats.maxColorError > 1e-5) problems.push(`color error ${stats.maxColorError}`);
  if (stats.maxMorphError > 1e-5) problems.push(`morph error ${stats.maxMorphError}`);
  if (problems.length) failures++;
  report.push({ id: entry.id, file: file.file, look: entry.lookDependent ? look.name : 'base', ...stats, problems });
  process.stdout.write(`${problems.length ? 'FAIL' : 'ok  '} ${entry.id} meshes ${stats.meshes} color ${stats.maxColorError.toExponential(1)} morph ${stats.maxMorphError.toExponential(1)} ${problems.slice(0, 3).join('; ')}\n`);
}
if (args.report) (await import('node:fs')).writeFileSync(args.report, JSON.stringify({ look: look.name, assets: report.length, failures, report }, null, 1) + '\n');
console.log(`roundtrip: ${report.length} assets, ${failures} failures`);
process.exit(failures ? 1 : 0);
