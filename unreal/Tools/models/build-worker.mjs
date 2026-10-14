// One look of the reference models, built under Node (run by export-reference-models.mjs with --import register.mjs).
//   --mode colors --look NAME --out FILE: per geometry of every look-dependent asset, its vertex colors and digests
//   --mode write --work DIR --out-dir DIR [--only REGEX]: builds the 'base' look, derives the tint attributes from
//      the colors dumps in DIR, writes one .glb per asset and DIR/assets.json for the manifest
import { writeFileSync, readFileSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { createHash } from 'node:crypto';
import { serialize, deserialize } from 'node:v8';
import { installShims, settle } from './shims.mjs';
import { extract } from './extract.mjs';
import { writeAsset } from './compose.mjs';
import { catalog, loadReference, lookFor, readLooks, PROBE } from './catalog.mjs';
import { writeAuthoredRig } from './authored-rig.mjs';

const args = Object.fromEntries(process.argv.slice(2).reduce((a, x, i, all) => (x.startsWith('--') ? [...a, [x.slice(2), all[i + 1]]] : a), []));
const reference = args.reference;
installShims();
const R = await loadReference(reference);
const { THREE } = R;
const looks = readLooks(reference);
const { list } = catalog(R, reference);
const only = args.only ? new RegExp(args.only) : null;
const digest = (a) => (a ? createHash('sha256').update(new Uint8Array(a.buffer, a.byteOffset, a.byteLength)).digest('hex') : null);

// material descriptions: what the reference shaders do with each material, in plain data (README.md)
const labels = new Map([[R.um.PAINT, 'unit-models.js PAINT'], [R.um.VEHICLE_PAINT, 'unit-models.js VEHICLE_PAINT'], [R.um.INFANTRY_PAINT, 'unit-models.js INFANTRY_PAINT'], [R.foliage.leafMaterial(), 'foliage.js leafMaterial()'], [R.foliage.barkMaterial(), 'foliage.js barkMaterial()']]);
const DEFAULT_MAT = { 'unit-models.js PAINT': 'armor-paint', 'unit-models.js VEHICLE_PAINT': 'armor-paint', 'unit-models.js INFANTRY_PAINT': 'wool', 'aircraft.js BODY_MAT': 'aircraft-paint', 'aircraft.js METAL_MAT': 'aluminum', 'aircraft.js FIELD_MAT': 'aircraft-paint', 'aircraft.js BLADE_MAT': 'aircraft-paint' };
const SURF = (() => {
  const src = readFileSync(join(reference, 'client/surfaces.js'), 'utf8'), body = src.match(/const SURF = \{([\s\S]*?)\n\};/)[1];
  return Object.fromEntries([...body.matchAll(/^\s{2}(\w+): \{([^}]*)\}/gm)].map(([, key, spec]) => [key, { tex: spec.match(/tex: '([^']+)'/)?.[1] ?? null, draw: spec.match(/draw: (\w+)/)?.[1] ?? null, size: Number(spec.match(/size: ([\d.]+)/)[1]) }]));
})();
labels.set(R.structures.buildingModel('wall', looks.factions[0]).children[0].material, 'structures.js MODEL_MAT');
for (const key of Object.keys(SURF)) { labels.set(R.surfaces.surface(key), `surfaces.js surface('${key}')`); labels.set(R.surfaces.surface(key, true), `surfaces.js surface('${key}', true)`); }
const textureRef = (tex) => {
  if (!tex) return null;
  const src = tex.image?.src ?? tex.image?._src;
  if (src) return { file: src.replace(/^\//, ''), flipY: tex.flipY, colorSpace: tex.colorSpace, wrap: tex.wrapS === THREE.RepeatWrapping ? 'repeat' : tex.wrapS === THREE.ClampToEdgeWrapping ? 'clamp' : 'mirror' };
  return { procedural: true, colorSpace: tex.colorSpace };
};
function describeMaterial(m) {
  const label = labels.get(m) ?? null, d = m.defaultAttributeValues ?? {}, surf = label?.match(/surface\('(\w+)'/)?.[1];
  const out = { label, type: m.type, color: m.color ? [m.color.r, m.color.g, m.color.b] : null, vertexColors: !!m.vertexColors, doubleSided: m.side === THREE.DoubleSide, transparent: !!m.transparent, opacity: m.opacity, alphaTest: m.alphaTest, depthWrite: m.depthWrite };
  if (m.isMeshStandardMaterial) { out.roughness = m.roughness; out.metalness = m.metalness; }
  out.map = textureRef(m.map);
  const patches = [];
  if ('matId' in d) patches.push({ patch: 'model-textures.js modelMaterial', unsetMatId: DEFAULT_MAT[label] ?? null, layers: 'texture array of client/textures/models (manifest.matLayers), triplanar in model space; matId fraction = baked grime' });
  if ('modelUV' in d) patches.push({ patch: 'infantry-material.js infantryMaterial', color: 'client/textures/models/infantry-v3-color.jpg', normal: 'client/textures/models/infantry-v3-normal.jpg', flipY: false, uv: '_MODELUV.xy; _MODELUV.z 0 = no atlas (weapons), 1 = atlas, 2 = atlas with uniform tint' });
  if ('wheelPivot' in d) patches.push({ patch: 'wheel-motion.js wheelMaterial', note: 'wheels turn about _WHEELPIVOT (local z axle); tracks follow extras.trackLoops (_TRACKDATA)' });
  if ('camo' in d) patches.push({ patch: 'planes.js paintMaterial', note: 'camouflage and panel or hinge lines drawn per pixel from _CAMO and _HINGE' });
  if (label === 'structures.js MODEL_MAT') patches.push({ patch: 'structures.js MODEL_MAT', texture: 'client/textures/detail.jpg', uv: '_UV.x is a surface id, not a coordinate: 0 none, 1 canvas (R), 2 timber (G), 3 concrete (B), 4 corrugated sheet ridged along x (B), 5 burlap (R), 6 corrugated sheet ridged along z (B); the detail texture is mapped from model-space axes', metresPerRepeat: { 1: 1 / 0.42, 2: 1 / 0.8, 3: 1 / 0.45, 4: 1 / 0.5, 5: 1 / 2, 6: 1 / 0.5 } });
  if (surf) patches.push({ patch: 'surfaces.js planar', surface: surf, texture: SURF[surf].tex ? `client/textures/${SURF[surf].tex.includes('.') ? SURF[surf].tex : SURF[surf].tex + '.jpg'}` : `procedural (surfaces.js ${SURF[surf].draw})`, metresPerRepeat: SURF[surf].size, uv: 'world-planar: top faces from x/z, walls from the side (no uv attribute)', topTint: m.userData.topTint ? [m.userData.topTint.value.r, m.userData.topTint.value.g, m.userData.topTint.value.b] : null });
  if (patches.length) out.shader = patches;
  return out;
}

const morphFor = (b) => b.morphNames;
async function buildAsset(entry, look) {
  const built = entry.build(look);
  await settle();
  for (const [m, l] of built.labels ?? []) if (m) labels.set(m, l);
  built.root.updateMatrixWorld(true);
  const data = extract(built.root, { names: built.names, instances: built.instances, trackUniform: R.trackMotion.trackUniform, morphNames: (n) => (built.morphNames ? built.morphNames(n) : Array.from({ length: n }, (_, i) => `target_${i}`)), describeMaterial });
  return { built, data };
}

if (args.mode === 'colors') {
  const look = lookFor(args.look, looks), out = {};
  for (const entry of list) {
    if (!entry.lookDependent || (only && !only.test(entry.id))) continue;
    const { data } = await buildAsset(entry, look);
    out[entry.id] = data.geometries.map((g) => ({ count: g.count, color: g.attrs.color?.array ?? null, position: digest(g.attrs.position.array), index: digest(g.index), matId: digest(g.attrs.matId?.array) }));
    out[`${entry.id}#materials`] = data.materials.map((m) => m.color ?? null); // a material colored by the look (unit-models.js cloth(f.color))
  }
  mkdirSync(dirname(args.out), { recursive: true });
  writeFileSync(args.out, serialize(out));
  process.exit(0);
}

if (args.mode !== 'write') throw new Error('--mode colors|write');
const work = args.work, outDir = args['out-dir'];
const dumps = Object.fromEntries(['slot0', 'slot1', 'slot2', 'slot3', 'slot4', 'slot5', 'o1', 'o2', 'v2', 'v3'].map((n) => [n, deserialize(readFileSync(join(work, `colors-${n}.bin`)))]));
const lin = (hex) => { const c = new THREE.Color(hex); return [c.r, c.g, c.b]; };
const base = lookFor('base', looks), results = [];

// Per-vertex owner and vehicle tint weights by linearity: color = COLOR_0 + (owner - ownerDefault) * _OWNERTINT
// + (vehicle - vehicleDefault) * _VEHICLETINT per linear channel. Derived from the probes, then checked against every
// real player color (slot0..5) and a third vehicle paint (v3). Returns the tints and the largest errors seen.
function tints(id, gi, g, fac) {
  const n = g.count, C = g.attrs.color?.array;
  const rec = { checks: {} };
  for (const [name, d] of Object.entries(dumps)) {
    const o = d[id]?.[gi];
    if (!o) throw new Error(`${id} geometry ${gi}: missing from look ${name}`);
    if (o.count !== n || o.position !== digest(g.attrs.position.array) || o.index !== digest(g.index)) rec.geometryVaries = [...(rec.geometryVaries ?? []), name];
    if (o.matId !== digest(g.attrs.matId?.array)) rec.matIdVaries = [...(rec.matIdVaries ?? []), name];
  }
  if (!C || rec.geometryVaries) return rec;
  const get = (name) => dumps[name][id][gi].color;
  const lo = lin(PROBE.lo), hi = lin(PROBE.hi), mid = lin(PROBE.mid), veh = lin(looks.factions[fac].vehicle), def = lin(looks.fallback);
  const o1 = get('o1'), o2 = get('o2'), v2 = get('v2'), v3 = get('v3');
  const OT = new Float32Array(n * 3), VT = new Float32Array(n * 3);
  let anyO = 0, anyV = 0;
  for (let i = 0; i < n * 3; i++) {
    const c = i % 3;
    OT[i] = (o2[i] - o1[i]) / (hi[c] - lo[c]);
    VT[i] = (v2[i] - o1[i]) / (hi[c] - veh[c]);
    anyO = Math.max(anyO, Math.abs(OT[i])); anyV = Math.max(anyV, Math.abs(VT[i]));
  }
  const err = (actual, owner, vehicle) => { let e = 0; for (let i = 0; i < n * 3; i++) { const c = i % 3; const p = C[i] + (owner[c] - def[c]) * OT[i] + (vehicle[c] - veh[c]) * VT[i]; e = Math.max(e, Math.abs(p - actual[i])); } return e; };
  for (let k = 0; k < 6; k++) rec.checks[`slot${k}`] = err(get(`slot${k}`), lin(looks.colors[k]), veh);
  rec.checks.o1 = err(o1, lo, veh); rec.checks.v3 = err(v3, lo, mid);
  let gray = 0;
  for (let i = 0; i < n; i++) for (const T of [OT, VT]) gray = Math.max(gray, Math.abs(T[i * 3] - T[i * 3 + 1]), Math.abs(T[i * 3] - T[i * 3 + 2]));
  rec.maxOwnerTint = anyO; rec.maxVehicleTint = anyV; rec.grayDeviation = gray;
  rec.owner = anyO > 0 ? OT : null; rec.vehicle = anyV > 0 ? VT : null;
  return rec;
}

const TOL = 1e-5;
// Materials whose color is the owner's color in every look (unit-models.js cloth(f.color): the inline structures' flag):
// the vertex tint above cannot see them. Marked ownerColor in the material extras; the importer colors them per instance.
function ownerMaterials(id, data) {
  const near = (a, b) => a && b && a.length >= 3 && [0, 1, 2].every((c) => Math.abs(a[c] - b[c]) <= 1e-6);
  const looksOwner = { o1: lin(PROBE.lo), o2: lin(PROBE.hi), ...Object.fromEntries([0, 1, 2, 3, 4, 5].map((k) => [`slot${k}`, lin(looks.colors[k])])) };
  return data.materials.map((m, mi) => near(m.color, lin(looks.fallback)) && Object.entries(looksOwner).every(([name, c]) => near(dumps[name][`${id}#materials`]?.[mi], c)));
}
// named role nodes (turret, mounts, legs, gun, fx_tip, ...) and where they sit in their parent: the pivots
const ROLE = /^(turret|gun|hull_\d+|gun_\d+|mount_\d+|leg_\d+|tail|hull|body|door|crew|fx_tip|gun_near|gun_far|prop_\d+|bomb|airframe|riders_\d+)$/;
function pivotsOf(root) {
  const out = [];
  const visit = (n, parent) => { if (ROLE.test(n.name)) out.push({ node: n.name, parent, translation: n.t, rotation: n.r, scale: n.s }); n.children.forEach((c) => visit(c, n.name)); };
  visit(root, null);
  return out;
}
for (const entry of list) {
  if (only && !only.test(entry.id)) continue;
  let result;
  try {
    const { built, data } = await buildAsset(entry, base);
    const tintInfo = data.geometries.map((g, gi) => (entry.lookDependent ? tints(entry.id, gi, g, entry.faction) : null));
    const ownerMats = entry.lookDependent ? ownerMaterials(entry.id, data) : [];
    ownerMats.forEach((on, mi) => { if (on) data.materials[mi] = { ...data.materials[mi], ownerColor: true }; });
    const ownerOk = tintInfo.every((t) => !t || (!t.geometryVaries && Object.entries(t.checks).every(([k, e]) => !k.startsWith('slot') || e <= TOL)));
    const vehicleOk = tintInfo.every((t) => !t || (!t.geometryVaries && (t.checks.v3 ?? 0) <= TOL && (t.checks.o1 ?? 0) <= TOL));
    const variants = [{ suffix: '', colors: null }];
    if (!ownerOk) for (let k = 0; k < 6; k++) variants.push({ suffix: `-slot${k}`, colors: data.geometries.map((_, gi) => dumps[`slot${k}`][entry.id][gi].color), slot: k });
    const files = [];
    for (const v of variants) {
      const file = `${entry.id}${v.suffix}.glb`;
      const meta = { id: entry.id + v.suffix, category: entry.category, faction: entry.faction, source: entry.source, extras: built.extras, look: !entry.lookDependent ? null : v.slot === undefined ? { owner: looks.fallback, vehicle: entry.faction == null ? null : looks.factions[entry.faction].vehicle, name: 'faction paint, no-slot owner color' } : { owner: looks.colors[v.slot], vehicle: looks.factions[entry.faction].vehicle, name: `player slot ${v.slot}` } };
      const t = data.geometries.map((g, gi) => ({ owner: ownerOk && v.slot === undefined ? tintInfo[gi]?.owner ?? null : null, vehicle: vehicleOk && ownerOk && v.slot === undefined ? tintInfo[gi]?.vehicle ?? null : null, colors: v.colors?.[gi] ?? null }));
      const { glb, stats } = writeAsset(data, meta, t);
      mkdirSync(dirname(join(outDir, file)), { recursive: true });
      writeFileSync(join(outDir, file), glb);
      files.push({ file, sha256: createHash('sha256').update(glb).digest('hex'), bytes: glb.length, ...stats, look: meta.look });
    }
    const checks = {};
    for (const t of tintInfo) if (t) for (const [k, e] of Object.entries(t.checks)) checks[k] = Math.max(checks[k] ?? 0, e);
    result = { id: entry.id, category: entry.category, type: entry.type ?? null, kind: entry.kind ?? null, faction: entry.faction, source: entry.source, extras: built.extras, files, warnings: data.warnings, unknownAttributes: [...new Set(data.geometries.flatMap((g) => g.unknown))],
      pivots: pivotsOf(data.root),
      tint: entry.lookDependent ? { owner: !ownerOk ? 'nonlinear: per-slot variant files written' : tintInfo.some((t) => t?.owner) ? 'linear: _OWNERTINT' : 'unused', vehicle: !tintInfo.some((t) => t?.vehicle) ? 'unused' : vehicleOk && ownerOk ? 'linear: _VEHICLETINT' : 'nonlinear: no _VEHICLETINT (COLOR_0 is exact for the faction paint, the only vehicle paint the reference uses)', perSlotVariants: !ownerOk, ownerColorMaterials: ownerMats.filter(Boolean).length, maxReconstructionError: checks, grayDeviation: Math.max(0, ...tintInfo.map((t) => t?.grayDeviation ?? 0)), geometryVaries: [...new Set(tintInfo.flatMap((t) => t?.geometryVaries ?? []))], matIdVaries: [...new Set(tintInfo.flatMap((t) => t?.matIdVaries ?? []))] } : null,
      geometryIds: data.geometries.map((g) => digest(g.attrs.position.array)) };
  } catch (e) {
    result = { id: entry.id, category: entry.category, type: entry.type ?? null, faction: entry.faction, source: entry.source, error: String(e?.stack ?? e).split('\n').slice(0, 4).join(' | ') };
  }
  results.push(result);
  process.stdout.write(`${result.error ? 'FAIL' : 'ok  '} ${entry.id}${result.error ? ' ' + result.error : ''}\n`);
}
// the Blender-baked infantry rig payload, as a skinned rest-pose mesh
if (!only || only.test('source/infantry-authored-rig')) {
  const id = 'source/infantry-authored-rig', source = { module: 'client/models/infantry-authored-data.js', call: 'default export (format ww2-infantry-skin-v1, inflated by blender-baked.js inflateModelArrays)', modules: ['client/models/infantry-authored-data.js', 'client/models/infantry-authored.js', 'client/models/blender-baked.js', 'tools/blender/rig-infantry.py'] };
  try {
    const { glb, stats } = writeAuthoredRig(R.authored, { id, category: 'source', source });
    mkdirSync(join(outDir, 'source'), { recursive: true });
    writeFileSync(join(outDir, `${id}.glb`), glb);
    results.push({ id, category: 'source', type: null, kind: null, faction: null, source, extras: { bones: R.authored.bones, blender: R.authored.blender, retopoSha256: R.authored.sourceHash }, files: [{ file: `${id}.glb`, sha256: createHash('sha256').update(glb).digest('hex'), bytes: glb.length, ...stats }], warnings: [], unknownAttributes: [], tint: null });
    process.stdout.write(`ok   ${id}\n`);
  } catch (e) { results.push({ id, category: 'source', source, error: String(e?.stack ?? e).split('\n').slice(0, 4).join(' | ') }); process.stdout.write(`FAIL ${id} ${e}\n`); }
}
// props kinds that draw foliage.js geometry, by identity (props.js meshesFor)
const aliases = [];
if (!only) {
  const { collectProps, collectPieces } = catalog(R, reference);
  const fol = new Map();
  for (const k of ['broadleaf', 'pine', 'poplar']) { const t = R.foliage.treeGeometry(k); fol.set(t.bark, `foliage/${k} bark`); fol.set(t.leaves, `foliage/${k} leaves`); }
  for (const k of ['bush', 'hedge', 'grass', 'crop']) fol.set(R.foliage.leafGeometry(k), `foliage/${k} leaves`);
  for (const [name, p] of collectProps()) aliases.push({ reference: `client/props.js InstancedMesh '${name}'`, geometry: fol.get(p.mesh.geometry) ?? `props/${name}`, sameObject: fol.has(p.mesh.geometry) });
  const leaves = collectPieces().get('leaves');
  if (leaves) aliases.push({ reference: "client/structures.js KINDS.leaves", geometry: fol.get(leaves.mesh.geometry) ?? 'pieces/leaves', sameObject: fol.has(leaves.mesh.geometry) });
}
writeFileSync(join(work, 'assets.json'), JSON.stringify({ results, aliases }));
