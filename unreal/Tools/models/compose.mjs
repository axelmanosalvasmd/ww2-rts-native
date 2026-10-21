// Intermediate asset (extract.mjs) + tint weights -> one .glb. Attribute mapping (README.md has the full table):
//   POSITION, NORMAL           as built (three.js model space: +y up, metres; glTF uses the same axes and units)
//   COLOR_0 (VEC3 float)       linear vertex color of the exported look, clamped to [0, 1]; _COLOR holds the raw
//                              value where a builder wrote a multiplier above 1
//   _MATID, _MODELUV, _UV, _WHEELPIVOT, _WHEELSIDE, _TRACKDATA, _CAMO, _HINGE: the reference attributes, unchanged
//   _OWNERTINT, _VEHICLETINT (VEC3 float): per-channel weights of the owner and vehicle paint (see build-worker.mjs)
//   TEXCOORD_0..n: mirrors for importers that drop custom attributes (Unreal); layout in mesh.extras.ww2.texcoords
// Morph targets carry deltas (three.js stores soldier postures as absolute positions).
import * as THREE from 'three';
import { Gltf } from './gltf-writer.mjs';
import { ATTRIBUTES } from './extract.mjs';

export const FORMAT = 'ww2-reference-models-v1';

// TEXCOORD mirror: two floats per channel, in this fixed order, each channel present only when the primitive has its
// data, so channel numbers differ between primitives but the order never does; mesh.extras.ww2.texcoords lists what
// each TEXCOORD_n holds. Tint weights that are grey (equal r, g, b within 1e-6, true of every tint the exporter keeps
// so far) mirror as one scalar each; otherwise all three channels are mirrored.
const MAX_TEXCOORDS = 8; // Unreal reads TEXCOORD_0..7 (GLTFCore GLTFAccessor.h EMeshAttributeType)
function mirror(src, n) {
  const out = [], push = (label, a, ka, b, kb) => {
    const arr = new Float32Array(n * 2);
    for (let i = 0; i < n; i++) { if (a) arr[i * 2] = a.array[i * a.itemSize + ka]; if (b) arr[i * 2 + 1] = b.array[i * b.itemSize + kb]; }
    out.push({ label, arr });
  };
  const { uv, modelUV, matId, ownerTint: o, vehicleTint: v, rawColor: c, wheelPivot: w, wheelSide, trackData: t, camo, hinge } = src;
  if (uv) push('uv', uv, 0, uv, 1);
  if (modelUV) push('modelUV.xy', modelUV, 0, modelUV, 1);
  if (matId || modelUV) push(`${matId ? 'matId' : '-'},${modelUV ? 'modelUV.z' : '-'}`, matId, 0, modelUV, 2);
  if (o || v) {
    const grey = (T) => { if (!T) return true; for (let i = 0; i < n; i++) if (Math.abs(T.array[i * 3] - T.array[i * 3 + 1]) > 1e-6 || Math.abs(T.array[i * 3] - T.array[i * 3 + 2]) > 1e-6) return false; return true; };
    if (grey(o) && grey(v)) push(`${o ? 'ownerTint' : '-'},${v ? 'vehicleTint' : '-'}`, o, 0, v, 0);
    else { push('ownerTint.rg', o, 0, o, 1); push('ownerTint.b,vehicleTint.r', o, 2, v, 0); push('vehicleTint.gb', v, 1, v, 2); }
  }
  if (c) { push('color.rg', c, 0, c, 1); push('color.b,-', c, 2, null, 0); }
  if (w) { push('wheelPivot.xy', w, 0, w, 1); push('wheelPivot.zw', w, 2, w, 3); }
  if (wheelSide || t) push(`${wheelSide ? 'wheelSide' : '-'},${t ? 'trackData.x' : '-'}`, wheelSide, 0, t, 0);
  if (t) { push('trackData.yz', t, 1, t, 2); push('trackData.w,-', t, 3, null, 0); }
  if (camo) { push('camo.xy', camo, 0, camo, 1); push('camo.zw', camo, 2, camo, 3); }
  if (hinge) { push('hinge.xy', hinge, 0, hinge, 1); push('hinge.z,-', hinge, 2, null, 0); }
  return out;
}

const isDefaultT = (t) => t.every((x) => x === 0), isDefaultR = (r) => r[0] === 0 && r[1] === 0 && r[2] === 0 && r[3] === 1, isDefaultS = (s) => s.every((x) => x === 1);

export function writeAsset(data, meta, tints) {
  const gltf = new Gltf({ generator: 'ww2-rts-native unreal/Tools/models/export-reference-models.mjs', extras: {
    format: FORMAT, id: meta.id, category: meta.category, faction: meta.faction, source: meta.source, look: meta.look, ...meta.extras && { asset: meta.extras },
    space: 'three.js model space as built: +Y up, metres, right-handed; unit models face +X with +Z on their right. Unreal Interchange maps glTF (x, y, z) to UE (x, z, y) x 100 cm, so +X forward, +Z right becomes UE +X forward, +Y right, +Z up.',
  } });
  const meshOf = new Map(), stats = { meshes: 0, uniqueVertices: 0, uniqueTriangles: 0, drawnVertices: 0, drawnTriangles: 0, morphTargets: 0, clampedColors: 0, texcoords: 0 };
  const matIndex = new Map();
  const material = (mi) => {
    if (matIndex.has(mi)) return matIndex.get(mi);
    const d = data.materials[mi], c = d.color ?? [1, 1, 1], clamp = (x) => Math.min(1, Math.max(0, x));
    const m = { name: d.label ?? d.type, pbrMetallicRoughness: { baseColorFactor: [clamp(c[0]), clamp(c[1]), clamp(c[2]), clamp(d.opacity ?? 1)], metallicFactor: d.metalness ?? 0, roughnessFactor: d.roughness ?? 1 } };
    if (d.doubleSided) m.doubleSided = true;
    if (d.alphaTest > 0) { m.alphaMode = 'MASK'; m.alphaCutoff = d.alphaTest; } else if (d.transparent) m.alphaMode = 'BLEND';
    if (d.type === 'MeshBasicMaterial') { m.extensions = { KHR_materials_unlit: {} }; gltf.use('KHR_materials_unlit'); }
    m.extras = { ww2: d };
    const i = gltf.material(m); matIndex.set(mi, i); return i;
  };
  const meshFor = (gi, mi, name) => {
    const key = `${gi}|${mi}`;
    if (meshOf.has(key)) return meshOf.get(key);
    const g = data.geometries[gi], t = tints[gi] ?? {}, n = g.count, attributes = {};
    const add = (name, arr, type, opts) => { attributes[name] = gltf.accessor(arr, type, opts); };
    const TYPE = { 1: 'SCALAR', 2: 'VEC2', 3: 'VEC3', 4: 'VEC4' };
    add('POSITION', g.attrs.position.array, 'VEC3', { minmax: true });
    // glTF requires unit normals. A zero normal (three.js computeVertexNormals on a vertex no triangle uses, such
    // as SphereGeometry's spare pole vertices) becomes +Y; the count and whether any triangle uses those vertices
    // go in mesh.extras.ww2.zeroNormals and the manifest.
    let zeroNormals = null;
    if (g.attrs.normal) {
      const N = g.attrs.normal.array, fixed = [];
      for (let i = 0; i < n; i++) if (!(Math.hypot(N[i * 3], N[i * 3 + 1], N[i * 3 + 2]) > 0)) fixed.push(i);
      let nor = N;
      if (fixed.length) {
        nor = N.slice();
        for (const i of fixed) nor.set([0, 1, 0], i * 3);
        const used = new Set(g.index ?? []), referenced = fixed.filter((i) => (g.index ? used.has(i) : true)).length;
        zeroNormals = { replaced: fixed.length, referencedByTriangles: referenced };
        stats.zeroNormalsReplaced = (stats.zeroNormalsReplaced ?? 0) + fixed.length;
        stats.zeroNormalsReferenced = (stats.zeroNormalsReferenced ?? 0) + referenced;
      }
      add('NORMAL', nor, 'VEC3');
    }
    const color = t.colors ?? g.attrs.color?.array, src = {};
    if (color) {
      let clamped = 0;
      const c0 = Float32Array.from(color, (x) => { const y = Math.min(1, Math.max(0, x)); if (y !== x) clamped++; return y; });
      add('COLOR_0', c0, 'VEC3');
      if (clamped) { add('_COLOR', Float32Array.from(color), 'VEC3'); stats.clampedColors += clamped; }
      src.rawColor = clamped ? { array: Float32Array.from(color), itemSize: 3 } : null;
    }
    for (const [name, a] of Object.entries(g.attrs)) {
      if (name === 'position' || name === 'normal' || name === 'color') continue;
      src[name] = a;
      add(ATTRIBUTES[name] ?? `_${name.toUpperCase()}`, a.array, TYPE[a.itemSize]);
    }
    if (t.owner) { add('_OWNERTINT', t.owner, 'VEC3'); src.ownerTint = { array: t.owner, itemSize: 3 }; }
    if (t.vehicle) { add('_VEHICLETINT', t.vehicle, 'VEC3'); src.vehicleTint = { array: t.vehicle, itemSize: 3 }; }
    const channels = mirror(src, n);
    if (channels.length > MAX_TEXCOORDS) throw new Error(`${meta.id}: ${channels.length} texcoord channels (${channels.map((c) => c.label)}) exceed ${MAX_TEXCOORDS}`);
    channels.forEach((c, k) => add(`TEXCOORD_${k}`, c.arr, 'VEC2'));
    stats.texcoords = Math.max(stats.texcoords, channels.length);
    const prim = { attributes, mode: 4, material: material(mi) };
    if (g.index) prim.indices = gltf.indices(g.index);
    const targetNames = [];
    const targets = [];
    const delta = (abs, base) => { const d = new Float32Array(abs.length); for (let i = 0; i < abs.length; i++) d[i] = abs[i] - base[i]; return d; };
    if (g.morph) g.morph.position.forEach((p, k) => {
      const tgt = { POSITION: gltf.accessor(g.morph.relative ? p : delta(p, g.attrs.position.array), 'VEC3', { minmax: true }) };
      if (g.morph.normal[k] && g.attrs.normal) tgt.NORMAL = gltf.accessor(g.morph.relative ? g.morph.normal[k] : delta(g.morph.normal[k], g.attrs.normal.array), 'VEC3');
      targets.push(tgt); targetNames.push(g.morph.names[k]);
    });
    if (g.fallen) {
      const tgt = { POSITION: gltf.accessor(delta(g.fallen.position, g.attrs.position.array), 'VEC3', { minmax: true }) };
      if (g.attrs.normal) tgt.NORMAL = gltf.accessor(delta(g.fallen.normal, g.attrs.normal.array), 'VEC3');
      targets.push(tgt); targetNames.push('fallen');
    }
    if (targets.length) { prim.targets = targets; stats.morphTargets = Math.max(stats.morphTargets, targets.length); }
    const extras = { ww2: { attributes: Object.keys(attributes), texcoords: channels.map((c) => c.label), ...g.extras, ...(zeroNormals ? { zeroNormals } : {}) } };
    if (targets.length) { extras.targetNames = targetNames; extras.ww2.morph = 'deltas from the standing build; kneel, prone and retreat are stored undone by the pose node transform (client/unit-models.js poseMorphs and POSES); fallen is the corpse build (unit-models.js fallenGeometry), not undone'; }
    const mesh = { name, primitives: [prim], extras };
    if (targets.length) mesh.weights = new Array(targets.length).fill(0);
    const idx = gltf.mesh(mesh);
    stats.meshes++; stats.uniqueVertices += n; stats.uniqueTriangles += (g.index ? g.index.length : n) / 3;
    meshOf.set(key, idx);
    return idx;
  };
  // bounds over every mesh in the asset's root frame
  const min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity], v = new THREE.Vector3();
  const visit = (node, parentMatrix, shown) => {
    const m = new THREE.Matrix4().compose(new THREE.Vector3(...node.t), new THREE.Quaternion(...node.r), new THREE.Vector3(...node.s));
    const world = parentMatrix ? parentMatrix.clone().multiply(m) : new THREE.Matrix4();
    const out = { name: node.name };
    if (parentMatrix) { if (!isDefaultT(node.t)) out.translation = node.t; if (!isDefaultR(node.r)) out.rotation = node.r; if (!isDefaultS(node.s)) out.scale = node.s; }
    else out.extras = { rootTransform: { translation: node.t, rotation: node.r, scale: node.s } };
    const visible = shown && node.extras.visible !== false;
    if (node.mesh) {
      out.mesh = meshFor(node.mesh.geo, node.mesh.mat, node.name);
      const g = data.geometries[node.mesh.geo], P = g.attrs.position.array;
      for (let i = 0; i < g.count; i++) { v.set(P[i * 3], P[i * 3 + 1], P[i * 3 + 2]).applyMatrix4(world); for (let k = 0; k < 3; k++) { const x = v.getComponent(k); if (x < min[k]) min[k] = x; if (x > max[k]) max[k] = x; } }
      if (visible) { stats.drawnVertices += g.count; stats.drawnTriangles += (g.index ? g.index.length : g.count) / 3; }
    }
    const extras = { ...node.extras };
    if (Object.keys(extras).length) out.extras = { ...(out.extras ?? {}), ...extras };
    const children = node.children.map((c) => visit(c, world, visible));
    if (children.length) out.children = children;
    return gltf.node(out);
  };
  // the root keeps identity (the reference builds every model at the origin); its own transform, if any, is in extras
  gltf.root(visit(data.root, null, true));
  stats.bounds = { min, max };
  return { glb: gltf.toGlb(), stats };
}
