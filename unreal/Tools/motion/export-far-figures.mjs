// Export the reference's far-away soldier figures, with their morph targets, for the Unreal crowd
// (Client/TCUnitMotion.cpp UTCSoldierCrowd). Runs the pinned client's own builders under Node, unmodified:
// unit-models.js buildModel bakes every squad as the browser does, and each man's far mesh (man.userData.lo) carries
// the standing build, the posture and gait morph targets (poseMorphs: absolute, undone by their pose node) and the
// fallen build (userData.fallen). The near figure's fallen build gives the corpse's resting height too.
//
// Keys follow TCInfantryRig.cpp: 0 standing, 1-3 kneel, prone, retreat, 4-23 gait (8 run, 4 aimed walk, 4 crouched,
// 4 crawl), 24-27 the dig strokes (the standing build again for squads that do not dig), 28 fallen.
// The owner's color shows only on the far figure's arm band (infantry.js farParts: tunic.lerp(owner, 0.4)): each
// figure is built for a black and a white owner, and color = base + tint * owner (linear) is checked on a third.
//
// Output: <out>/FarFigures.bin (layout in TCUnitMotion.cpp) and <out>/FarFigures.json (provenance and the list).
// Usage: INFANTRY_DEPS=<dir with node_modules/three> node --import ./register.mjs export-far-figures.mjs <out>
import { writeFileSync, readFileSync, mkdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { deflateSync } from 'node:zlib';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join, relative } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const REF = join(HERE, '../../../reference/ww2-rts');
const out = process.argv[2];
if (!out) throw new Error('usage: export-far-figures.mjs <outdir>');
mkdirSync(out, { recursive: true });

const THREE = await import('three');
const { buildModel } = await import(join(REF, 'client/unit-models.js'));
const { soldierKit } = await import(join(REF, 'client/models/infantry.js'));
const { UNITS } = await import(join(REF, 'shared/sim.js'));
const { SLOTS } = await import(join(REF, 'shared/squad-men.js'));

const KEYS = 29, DIG = 24, FALLEN = 28;
const LOOK = (color) => ({ uniform: 0x6b7248, vehicle: 0x59623d, color });
const PROBE = 0x3b73d6; // main.js COLORS[0], to check the linear owner tint

function squad(type, fac, color) {
  const root = new THREE.Group(), v = { id: 1, type, root, models: [], turret: null };
  buildModel(v, root, LOOK(color), fac, UNITS[type]);
  return v.models;
}
// all keys of one baked far mesh (positions, normals), three space
function keys(geo) {
  const n = geo.attributes.position.count, targets = geo.morphAttributes.position, normals = geo.morphAttributes.normal;
  if (targets.length !== 23 && targets.length !== 27) throw new Error(`unexpected ${targets.length} morph targets`);
  const P = new Float32Array(KEYS * n * 3), N = new Float32Array(KEYS * n * 3);
  const put = (k, pos, nor) => { for (let i = 0; i < n * 3; i++) { P[k * n * 3 + i] = pos.array[i]; N[k * n * 3 + i] = nor.array[i]; } };
  put(0, geo.attributes.position, geo.attributes.normal);
  for (let j = 0; j < targets.length; j++) put(j + 1, targets[j], normals[j]);
  for (let k = targets.length + 1; k < FALLEN; k++) put(k, geo.attributes.position, geo.attributes.normal); // non-diggers
  const fallen = geo.userData.fallen;
  if (!fallen || fallen.position.count !== n) throw new Error('no fallen build');
  put(FALLEN, fallen.position, fallen.normal);
  return { n, P, N, digger: targets.length === 27 };
}
const minY = (attr) => { let m = Infinity; for (let i = 0; i < attr.count; i++) m = Math.min(m, attr.getY(i)); return m; };

const figures = new Map();
let checked = 0, worstTint = 0;
for (const type of Object.keys(SLOTS)) {
  for (let fac = 0; fac < 4; fac++) {
    const figure = type === 'medic' ? 'engineer' : type;
    const black = squad(type, fac, 0x000000), white = squad(type, fac, 0xffffff), probe = squad(type, fac, PROBE);
    const probeLinear = new THREE.Color(PROBE);
    black.forEach((man, i) => {
      const kit = soldierKit(figure, i, fac), key = `${figure}|${fac}|${kit}`;
      const lo = man.userData.lo[0].geometry, hi = man.userData.hi[0].geometry;
      const loW = white[i].userData.lo[0].geometry, loP = probe[i].userData.lo[0].geometry;
      const n = lo.attributes.position.count;
      const base = new Float32Array(n * 3), tint = new Float32Array(n * 3);
      for (let k = 0; k < n * 3; k++) { base[k] = lo.attributes.color.array[k]; tint[k] = loW.attributes.color.array[k] - lo.attributes.color.array[k]; }
      // the owner tint is linear: base + tint * owner reproduces a third owner's colors
      for (let v = 0; v < n; v++) for (let c = 0; c < 3; c++) {
        const want = loP.attributes.color.array[v * 3 + c], got = base[v * 3 + c] + tint[v * 3 + c] * [probeLinear.r, probeLinear.g, probeLinear.b][c];
        worstTint = Math.max(worstTint, Math.abs(want - got));
      }
      checked++;
      const k = keys(lo);
      if (!lo.attributes.matId || lo.attributes.modelUV) throw new Error(`${key}: expected procedural far material tags without an atlas`);
      const materials = Float32Array.from(lo.attributes.matId.array);
      if (!materials.every(Number.isFinite)) throw new Error(`${key}: non-finite material tag`);
      const rec = { key, type, figure, fac, slot: i, kit, vertices: n, indices: lo.index.count, digger: k.digger, nearMinY: minY(hi.userData.fallen.position), farMinY: minY(lo.userData.fallen.position) };
      if (figures.has(key)) {
        // the same shape from another squad slot or type: it must be the same geometry
        const old = figures.get(key);
        if (old.k.n !== n || Buffer.compare(Buffer.from(old.k.P.buffer), Buffer.from(k.P.buffer)) !== 0 || Buffer.compare(Buffer.from(old.base.buffer), Buffer.from(base.buffer)) !== 0)
          throw new Error(`${key}: two different far figures`);
        return;
      }
      figures.set(key, { rec, k, base, tint, materials, index: lo.index.array });
    });
  }
}
if (worstTint > 1e-6) throw new Error(`owner tint is not linear (worst ${worstTint})`);

// binary: header then the zlib payload
const chunks = [];
const u16 = (x) => { const b = Buffer.alloc(2); b.writeUInt16LE(x); chunks.push(b); };
const u32 = (x) => { const b = Buffer.alloc(4); b.writeUInt32LE(x); chunks.push(b); };
const f32 = (x) => { const b = Buffer.alloc(4); b.writeFloatLE(x); chunks.push(b); };
const arr = (a) => chunks.push(Buffer.from(a.buffer, a.byteOffset, a.byteLength));
const list = [...figures.values()].sort((a, b) => (a.rec.key < b.rec.key ? -1 : a.rec.key > b.rec.key ? 1 : 0));
for (const { rec, k, base, tint, materials, index } of list) {
  const name = Buffer.from(rec.key, 'utf8');
  u16(name.length); chunks.push(name);
  u32(rec.vertices); u32(rec.indices); f32(rec.nearMinY); f32(rec.farMinY);
  arr(k.P); arr(k.N); arr(base); arr(tint); arr(materials); arr(Uint16Array.from(index));
}
const payload = Buffer.concat(chunks), packed = deflateSync(payload, { level: 9 });
const head = Buffer.alloc(8 + 5 * 4);
head.write('TCFARFG1', 0, 'ascii');
head.writeUInt32LE(2, 8); head.writeUInt32LE(list.length, 12); head.writeUInt32LE(KEYS, 16); head.writeUInt32LE(packed.length, 20); head.writeUInt32LE(payload.length, 24);
const bin = Buffer.concat([head, packed]);
writeFileSync(join(out, 'FarFigures.bin'), bin);

const sha = (buf) => createHash('sha256').update(buf).digest('hex');
const git = (args) => { try { return execFileSync('git', ['-C', REF, ...args], { encoding: 'utf8' }).trim(); } catch { return null; } };
const sources = ['client/unit-models.js', 'client/models/infantry.js', 'client/models/infantry-authored.js', 'client/models/infantry-authored-data.js', 'shared/squad-men.js', 'shared/sim.js'];
const manifest = {
  format: 'tc-far-figures-v2', reference: { commit: git(['rev-parse', 'HEAD']), clean: git(['status', '--porcelain']) === '' },
  three: JSON.parse(readFileSync(join(process.env.INFANTRY_DEPS, 'node_modules/three/package.json'), 'utf8')).version, node: process.version,
  tool: { file: relative(join(HERE, '../../..'), fileURLToPath(import.meta.url)), sha256: sha(readFileSync(fileURLToPath(import.meta.url))) },
  sources: Object.fromEntries(sources.map((s) => [s, sha(readFileSync(join(REF, s)))])),
  keys: KEYS, ownerTint: { probe: PROBE, worstError: worstTint, figuresChecked: checked },
  bin: { bytes: bin.length, payload: payload.length, sha256: sha(bin) },
  figures: list.map(({ rec }) => rec),
};
writeFileSync(join(out, 'FarFigures.json'), JSON.stringify(manifest, null, 1) + '\n');
console.log(JSON.stringify({ figures: list.length, bytes: bin.length, payload: payload.length, worstTint, sha256: manifest.bin.sha256 }));
