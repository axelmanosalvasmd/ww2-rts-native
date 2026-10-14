// Three.js object tree (as the reference builders return it) -> a plain intermediate: nodes with their local
// transforms, geometries as typed arrays, and material descriptions. Nothing is re-modelled: positions, normals,
// colors, indices and the custom attributes are copied as they are, in the builders' own model space (three.js:
// +y up, metres; the unit models face +x).
import * as THREE from 'three';

// Attributes the reference shaders read, with the glTF name each is written under (see README.md).
export const ATTRIBUTES = {
  position: 'POSITION', normal: 'NORMAL', color: 'COLOR_0',
  matId: '_MATID', modelUV: '_MODELUV', uv: '_UV', wheelPivot: '_WHEELPIVOT', wheelSide: '_WHEELSIDE', trackData: '_TRACKDATA',
  camo: '_CAMO', hinge: '_HINGE',
};
// per-instance runtime state, not model data (client/wheel-motion.js wheelMesh: the vehicle's wheel travel)
const RUNTIME = new Set(['wheelTravel']);

function floats(attr) {
  const n = attr.count * attr.itemSize;
  if (!attr.isInterleavedBufferAttribute && attr.array instanceof Float32Array && !attr.normalized) return attr.array.slice(0, n);
  const out = new Float32Array(n);
  for (let i = 0; i < attr.count; i++) for (let k = 0; k < attr.itemSize; k++) out[i * attr.itemSize + k] = attr.getComponent(i, k);
  return out;
}

// client/track-motion.js keeps every track loop in one global table and wheelMesh() rewrites trackData.y from the
// geometry's own loop index to that global row (which depends on build order). The exported files keep the local
// index into extras.trackLoops instead. Rows are matched by resampling each local loop exactly as register() does and
// comparing with the table's texels (trackUniform), so the mapping is read from the reference, not guessed.
function sampleLoop(points, samples = 1024) {
  const arc = [0];
  for (let i = 0; i < points.length; i++) { const a = points[i], b = points[(i + 1) % points.length]; arc.push(arc[i] + Math.hypot(b[0] - a[0], b[1] - a[1])); }
  const length = arc.at(-1), out = new Float32Array(samples * 4);
  let segment = 0;
  for (let i = 0; i < samples; i++) {
    const distance = i * length / samples;
    while (segment < points.length - 1 && arc[segment + 1] < distance) segment++;
    const a = points[segment], b = points[(segment + 1) % points.length], fraction = (distance - arc[segment]) / (arc[segment + 1] - arc[segment]);
    out.set([a[0] + (b[0] - a[0]) * fraction, a[1] + (b[1] - a[1]) * fraction, length, 0], i * 4);
  }
  return out;
}
function localTrackData(attr, loops, trackUniform) {
  const data = trackUniform.value.image.data, rowLen = 1024 * 4, rows = data.length / rowLen, rowOf = new Map();
  loops.forEach((loop, j) => {
    const s = sampleLoop(loop.points);
    for (let r = 0; r < rows; r++) {
      let same = true;
      for (let k = 0; k < rowLen; k++) if (data[r * rowLen + k] !== s[k]) { same = false; break; }
      if (same) { if (!rowOf.has(r)) rowOf.set(r, j); return; }
    }
    throw new Error(`track loop ${j} not found in the track table`);
  });
  const out = floats(attr);
  for (let i = 0; i < attr.count; i++) {
    const g = out[i * 4 + 1];
    if (g >= 0) { if (!rowOf.has(g)) throw new Error(`trackData row ${g} has no local loop`); out[i * 4 + 1] = rowOf.get(g); }
  }
  return out;
}

const vec = (v) => [v.x, v.y, v.z];

// Merge every instance of an InstancedMesh into one geometry, as drawn: positions and normals through the instance
// matrices, and the instance color as the vertex color (times the geometry's own colors where the material reads
// them). Used for dressing that is one fixed piece in the reference (the HQ sandbag ring), not for the map pieces.
export function bakeInstances(mesh) {
  const g = mesh.geometry, P = g.attributes.position, N = g.attributes.normal, C = mesh.material.vertexColors ? g.attributes.color : null, I = g.index;
  const n = mesh.count, nv = P.count, ni = I ? I.count : nv, pos = new Float32Array(n * nv * 3), nor = new Float32Array(n * nv * 3), col = new Float32Array(n * nv * 3);
  const idx = new Uint32Array(n * ni), m = new THREE.Matrix4(), nm = new THREE.Matrix3(), t = new THREE.Vector3(), c = new THREE.Color(1, 1, 1);
  for (let k = 0; k < n; k++) {
    mesh.getMatrixAt(k, m); nm.getNormalMatrix(m);
    if (mesh.instanceColor) mesh.getColorAt(k, c);
    const flip = m.determinant() < 0;
    for (let i = 0; i < nv; i++) {
      const o = (k * nv + i) * 3;
      t.fromBufferAttribute(P, i).applyMatrix4(m).toArray(pos, o);
      t.fromBufferAttribute(N, i).applyMatrix3(nm).normalize().toArray(nor, o);
      col[o] = c.r * (C ? C.getX(i) : 1); col[o + 1] = c.g * (C ? C.getY(i) : 1); col[o + 2] = c.b * (C ? C.getZ(i) : 1);
    }
    for (let i = 0; i < ni; i += 3) {
      const a = I ? I.getX(i) : i, b = I ? I.getX(i + 1) : i + 1, d = I ? I.getX(i + 2) : i + 2;
      idx[k * ni + i] = k * nv + a; idx[k * ni + i + 1] = k * nv + (flip ? d : b); idx[k * ni + i + 2] = k * nv + (flip ? b : d);
    }
  }
  const out = new THREE.BufferGeometry();
  out.setAttribute('position', new THREE.BufferAttribute(pos, 3));
  out.setAttribute('normal', new THREE.BufferAttribute(nor, 3));
  out.setAttribute('color', new THREE.BufferAttribute(col, 3));
  out.setIndex(new THREE.BufferAttribute(idx, 1));
  const baked = new THREE.Mesh(out, mesh.material);
  baked.name = mesh.name; baked.position.copy(mesh.position); baked.quaternion.copy(mesh.quaternion); baked.scale.copy(mesh.scale);
  baked.castShadow = mesh.castShadow; baked.receiveShadow = mesh.receiveShadow; baked.visible = mesh.visible;
  baked.userData.bakedInstances = n;
  return baked;
}

// The geometry as plain arrays. morphNames names the morph targets (they are absolute in three.js,
// morphTargetsRelative false, and become deltas in the writer).
export function geometryData(g, { trackUniform, morphNames } = {}) {
  const out = { count: g.attributes.position.count, attrs: {}, index: null, morph: null, extras: {}, unknown: [] };
  for (const [name, attr] of Object.entries(g.attributes)) {
    if (RUNTIME.has(name) || attr.isInstancedBufferAttribute) continue;
    if (!ATTRIBUTES[name]) out.unknown.push(name);
    out.attrs[name] = { itemSize: attr.itemSize, array: name === 'trackData' && trackUniform && g.userData.trackLoops ? localTrackData(attr, g.userData.trackLoops, trackUniform) : floats(attr) };
  }
  if (g.index) {
    const a = g.index.array, n = g.index.count;
    let max = 0;
    for (let i = 0; i < n; i++) if (a[i] > max) max = a[i];
    out.index = max < 65535 && out.count <= 65535 ? Uint16Array.from(a.subarray(0, n)) : Uint32Array.from(a.subarray(0, n));
  }
  if (g.drawRange && (g.drawRange.start !== 0 || g.drawRange.count !== Infinity)) out.extras.drawRange = { start: g.drawRange.start, count: g.drawRange.count };
  if (g.groups?.length > 1) out.extras.groups = g.groups.map((x) => ({ ...x }));
  const mp = g.morphAttributes?.position;
  if (mp?.length) {
    const names = morphNames ? morphNames(mp.length) : mp.map((_, i) => `target_${i}`);
    out.morph = { relative: !!g.morphTargetsRelative, names, position: mp.map(floats), normal: (g.morphAttributes.normal ?? []).map(floats) };
  }
  const u = g.userData ?? {};
  if (u.muzzles) out.extras.muzzles = u.muzzles.map(vec);
  if (u.fallen) out.fallen = { position: floats(u.fallen.position), normal: floats(u.fallen.normal) };
  if (u.trackLoops) out.extras.trackLoops = u.trackLoops.map((l) => ({ points: l.points.map((p) => [...p]) }));
  if (u.trackSupport) out.extras.trackSupport = [...u.trackSupport];
  return out;
}

// Object tree -> { root, geometries, materials }. names: Map(Object3D -> node name). instances: 'bake' merges an
// InstancedMesh into one mesh; 'prototype' keeps its one shared geometry (the map pieces, props and foliage, whose
// placement is the reference's layout code, not model data).
export function extract(top, { names = new Map(), instances = 'prototype', trackUniform, morphNames, describeMaterial } = {}) {
  const geometries = [], materials = [], geoIndex = new Map(), matIndex = new Map(), warnings = [];
  const geo = (g) => { if (!geoIndex.has(g)) { geoIndex.set(g, geometries.length); geometries.push(geometryData(g, { trackUniform, morphNames })); } return geoIndex.get(g); };
  const mat = (m) => { if (!matIndex.has(m)) { matIndex.set(m, materials.length); materials.push(describeMaterial(m)); } return matIndex.get(m); };
  const used = new Map();
  const nameOf = (o, parentName, k) => {
    let n = names.get(o) ?? (o.name || (o.isMesh ? `${parentName}_mesh${k}` : `${parentName}_${k}`));
    const seen = used.get(n) ?? 0; used.set(n, seen + 1);
    return seen ? `${n}.${seen}` : n;
  };
  const visit = (o, parentName, k) => {
    let src = o;
    if (o.isInstancedMesh && instances === 'bake') src = bakeInstances(o);
    const node = { name: nameOf(o, parentName, k), t: vec(o.position), r: [o.quaternion.x, o.quaternion.y, o.quaternion.z, o.quaternion.w], s: vec(o.scale), children: [], extras: {} };
    if (!o.visible) node.extras.visible = false;
    if (src.isMesh) {
      if (Array.isArray(src.material)) throw new Error(`${node.name}: multi-material meshes are not supported`);
      node.mesh = { geo: geo(src.geometry), mat: mat(src.material) };
      node.extras.castShadow = !!src.castShadow;
      if (src.userData.bakedInstances) node.extras.bakedInstances = src.userData.bakedInstances;
      if (o.isInstancedMesh && instances === 'prototype') node.extras.instanced = { referenceCount: o.count, instanceColor: !!o.instanceColor };
    } else if (o.type !== 'Group' && o.type !== 'Object3D' && o.type !== 'Scene') warnings.push(`${node.name}: ${o.type} exported as an empty node`);
    o.children.forEach((c, i) => node.children.push(visit(c, node.name, i)));
    return node;
  };
  const root = visit(top, 'root', 0);
  return { root, geometries, materials, warnings };
}
