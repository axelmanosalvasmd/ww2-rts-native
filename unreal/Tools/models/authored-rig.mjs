// The Blender-baked infantry payload (client/models/infantry-authored-data.js, format ww2-infantry-skin-v1, written
// by reference tools/blender/rig-infantry.py from art/model-reference-2026-10-03/infantry-rig-v3.blend) as a skinned
// glTF in its rest pose: the payload's own vertices, UVs, joint indices and weights, and its 15 rest frames as the
// joints. Game space as in the payload (+X forward, +Y up, metres). Bone parents follow rig-infantry.py: chest under
// pelvis, head under chest, each side's thigh under pelvis and upper arm under chest, every other bone under the one
// before it. The runtime figures (units/*/...glb) are poses of this mesh baked by client/models/infantry-authored.js,
// which also drops the open-hand triangles at the cuffs and, outside Germany, keeps the bare-head subset.
import * as THREE from 'three';
import { Gltf } from './gltf-writer.mjs';

export function parentOf(i) {
  if (i === 0) return -1;
  if (i === 1) return 0;
  if (i === 2) return 1;
  const offset = (i - 3) % 6;
  return offset === 0 ? 0 : offset === 3 ? 1 : i - 1;
}

export function writeAuthoredRig(d, meta) {
  if (d.format !== 'ww2-infantry-skin-v1') throw new Error(`unexpected infantry payload ${d.format}`);
  const A = d.attributes, n = A.position.length / 3;
  const gltf = new Gltf({ generator: 'ww2-rts-native unreal/Tools/models/export-reference-models.mjs', extras: { format: 'ww2-reference-models-v1', ...meta, space: 'payload game space: +X forward, +Y up, metres (glTF axes and units)' } });
  const rest = d.rest.map((m) => new THREE.Matrix4().fromArray(m));
  const nodes = [], children = d.bones.map(() => []);
  d.bones.forEach((_, i) => { const p = parentOf(i); if (p >= 0) children[p].push(i); });
  // joint node i: local = inverse(parent rest) * rest
  const jointIndex = new Map();
  const add = (i) => {
    const kids = children[i].map(add);
    const local = parentOf(i) < 0 ? rest[i].clone() : rest[parentOf(i)].clone().invert().multiply(rest[i]);
    const t = new THREE.Vector3(), q = new THREE.Quaternion(), s = new THREE.Vector3();
    local.decompose(t, q, s);
    const node = { name: d.bones[i], translation: t.toArray(), rotation: q.toArray(), extras: { restWorld: d.rest[i], length: d.lengths[i] } };
    if (Math.abs(s.x - 1) > 1e-6 || Math.abs(s.y - 1) > 1e-6 || Math.abs(s.z - 1) > 1e-6) node.scale = s.toArray();
    if (kids.length) node.children = kids;
    const idx = gltf.node(node); jointIndex.set(i, idx); nodes.push(idx); return idx;
  };
  const rootJoint = add(0);
  const ibm = new Float32Array(16 * d.bones.length);
  d.bones.forEach((_, i) => ibm.set(rest[i].clone().invert().elements, i * 16));
  gltf.json.skins = [{ name: 'Infantry_Game_Rig', joints: d.bones.map((_, i) => jointIndex.get(i)), skeleton: rootJoint, inverseBindMatrices: gltf.accessor(ibm, 'MAT4', { target: null }) }];
  // the normal map stays a reference: the payload has no tangents (the runtime derives them in its shader)
  gltf.json.images = [{ uri: '../textures/models/infantry-v3-color.jpg' }];
  gltf.json.samplers = [{ magFilter: 9729, minFilter: 9987, wrapS: 33071, wrapT: 33071 }];
  gltf.json.textures = [{ source: 0, sampler: 0 }];
  const material = gltf.material({ name: 'infantry-v3', pbrMetallicRoughness: { baseColorTexture: { index: 0 }, metallicFactor: 0, roughnessFactor: 0.9 }, extras: { ww2: { normal: 'textures/models/infantry-v3-normal.jpg (tangent space, flipY false)', note: 'client/infantry-material.js samples the same atlas (flipY false, as glTF); the runtime tints fabric (_FABRIC = 1) toward the faction tunic color' } } });
  // rig-infantry.py pads unused influence slots with joint indices of bones of zero weight; glTF wants 0 there.
  // Lossless: a zero weight contributes nothing whatever its joint.
  const joints = Uint16Array.from(A.joints), weights = Float32Array.from(A.weights);
  let zeroed = 0;
  for (let i = 0; i < joints.length; i++) if (weights[i] === 0 && joints[i] !== 0) { joints[i] = 0; zeroed++; }
  const attributes = {
    POSITION: gltf.accessor(Float32Array.from(A.position), 'VEC3', { minmax: true }),
    NORMAL: gltf.accessor(Float32Array.from(A.normal), 'VEC3'),
    TEXCOORD_0: gltf.accessor(Float32Array.from(A.uv), 'VEC2'),
    JOINTS_0: gltf.accessor(joints, 'VEC4'),
    WEIGHTS_0: gltf.accessor(weights, 'VEC4'),
    _FABRIC: gltf.accessor(Float32Array.from(A.fabric), 'SCALAR'),
    _SKIN: gltf.accessor(Float32Array.from(A.skin), 'SCALAR'),
  };
  const index = n <= 65535 ? Uint16Array.from(d.indices) : Uint32Array.from(d.indices);
  const mesh = gltf.mesh({ name: 'infantry-authored', primitives: [{ attributes, indices: gltf.indices(index), material, mode: 4 }], extras: { ww2: { fabric: '_FABRIC 1 = uniform cloth (runtime tunic tint), 0 = kit and skin', skin: '_SKIN 1 = bare skin texels', bones: d.bones, blender: d.blender, sourceHash: d.sourceHash } } });
  // the skinned mesh node is a scene root (glTF ignores parent transforms of skinned meshes), next to the joints
  const meshNode = gltf.node({ name: 'infantry-authored', mesh, skin: 0, extras: { zeroWeightJointsZeroed: zeroed } });
  gltf.root(rootJoint); gltf.root(meshNode);
  let sumErr = 0;
  for (let i = 0; i < n; i++) sumErr = Math.max(sumErr, Math.abs(weights[i * 4] + weights[i * 4 + 1] + weights[i * 4 + 2] + weights[i * 4 + 3] - 1));
  return { glb: gltf.toGlb(), stats: { meshes: 1, uniqueVertices: n, uniqueTriangles: d.indices.length / 3, drawnVertices: n, drawnTriangles: d.indices.length / 3, joints: d.bones.length, maxWeightSumError: sumErr, zeroWeightJointsZeroed: zeroed } };
}
