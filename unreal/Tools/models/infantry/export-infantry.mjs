// Exports the reference infantry for Unreal, from the pinned sources only (no remodelling):
//   SK_Infantry.glb     the Blender-authored body (infantry-authored-data.js: the retained infantry-retopo-v3.glb as
//                       fitted and weighted by tools/blender/rig-infantry.py) as a skinned mesh on a flat skeleton:
//                       root, the 15 deform bones at their Blender rest frames, and the rigid kit anchors.
//   Kit/<name>.glb      every attachment authoredParts() adds to a figure (closed grips, scalp and headgear, weapons,
//                       the slung carbine, carried items, flame tanks, engineer packs), built by the reference's own
//                       functions, merged per anchor in the anchor's frame and deduplicated by content.
//   kits.json           which mesh hangs on which anchor bone for each (type, faction, job), plus the checks below.
// Checks made on the way (any failure aborts):
//   - the payload's sourceHash equals the sha256 of the retained glb, and its 7,580 vertices are the glb's under the
//     fit transform rig-infantry.py applies (glTF to Blender axes, centring, 1.40 m height, Blender to game axes);
//   - every kit part keeps a constant transform relative to its anchor across every keyframe the reference bakes,
//     so a socket attachment reproduces the reference figure exactly (the flame hose is the one exception: the
//     reference rebuilds it per pose, here it is two straight segments between the same three points).
// Usage: INFANTRY_DEPS=<dir> node --import ./register.mjs export-infantry.mjs <outdir>
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as THREE from 'three';
import { solve, basis, authoredParts, FACTIONS, tube, HELMETS } from '../../../../reference/ww2-rts/client/models/infantry.js';
import { authoredSource as source, dressedBody, bareHead } from '../../../../reference/ww2-rts/client/models/infantry-authored.js';
import { merge, MATS } from '../../../../reference/ww2-rts/client/models/geom.js';
import { distinctKits, keyframes, kitHeights, BONES, BLENDER_DEFORM } from './figures.mjs';
import { Glb, quaternionOf } from './gltf.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const REF = join(here, '../../../../reference/ww2-rts');
const out = process.argv[2];
if (!out) throw new Error('usage: export-infantry.mjs <outdir>');
mkdirSync(join(out, 'Kit'), { recursive: true });
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const GENERATOR = 'ww2-rts-native unreal/Tools/models/infantry/export-infantry.mjs';
const report = { reference: {}, checks: {}, meshes: {}, kits: [] };

// ---------------------------------------------------------------- provenance of the authored body

const glbPath = join(REF, 'art/model-reference-2026-10-03/infantry-retopo-v3.glb');
const glbBytes = readFileSync(glbPath);
report.reference.retopoGlbSha256 = sha256(glbBytes);
if (source.sourceHash !== report.reference.retopoGlbSha256) throw new Error('infantry-authored-data.js was not made from the retained glb');
report.reference.payload = { format: source.format, blender: source.blender, bones: source.bones, sourceHash: source.sourceHash };
for (const name of ['infantry-v3-color.jpg', 'infantry-v3-normal.jpg']) report.reference[name] = sha256(readFileSync(join(REF, 'client/textures/models', name)));
report.reference.rigBlendSha256 = sha256(readFileSync(join(REF, 'art/model-reference-2026-10-03/infantry-rig-v3.blend')));
if (source.bones.join() !== BLENDER_DEFORM.join()) throw new Error('bone order changed');
{
  // rig-infantry.py: glTF (y up) imports into Blender as (x, -z, y); centre x, centre y over the 43-58 % height band,
  // feet to z = 0, scale to 1.40; the game point is (by, bz, -bx). Blender then welds seam duplicates (1e-5).
  const len = glbBytes.readUInt32LE(12), json = JSON.parse(glbBytes.subarray(20, 20 + len).toString());
  const bin = glbBytes.subarray(20 + len + 8), acc = json.accessors[json.meshes[0].primitives[0].attributes.POSITION], bv = json.bufferViews[acc.bufferView];
  const raw = new Float32Array(bin.buffer.slice(bin.byteOffset + (bv.byteOffset ?? 0) + (acc.byteOffset ?? 0), bin.byteOffset + (bv.byteOffset ?? 0) + (acc.byteOffset ?? 0) + acc.count * 12));
  const B = []; for (let i = 0; i < acc.count; i++) B.push([raw[i * 3], -raw[i * 3 + 2], raw[i * 3 + 1]]);
  const zs = B.map((p) => p[2]), low = Math.min(...zs), high = Math.max(...zs), scale = 1.40 / (high - low);
  const xs = B.map((p) => p[0]), cx = (Math.min(...xs) + Math.max(...xs)) / 2;
  const mid = B.filter((p) => low + (high - low) * 0.43 < p[2] && p[2] < low + (high - low) * 0.58).map((p) => p[1]), cy = (Math.min(...mid) + Math.max(...mid)) / 2;
  const game = B.map(([x, y, z]) => [(y - cy) * scale, (z - low) * scale, -((x - cx) * scale)]);
  const grid = new Map(), cell = (p) => p.map((c) => Math.floor(c / 0.01)).join();
  game.forEach((p, i) => { const k = cell(p); if (!grid.has(k)) grid.set(k, []); grid.get(k).push(i); });
  const P = source.attributes.position, count = P.length / 3;
  let worst = 0;
  for (let i = 0; i < count; i++) {
    const q = [P[i * 3], P[i * 3 + 1], P[i * 3 + 2]], c = q.map((v) => Math.floor(v / 0.01));
    let best = Infinity;
    for (let dx = -1; dx <= 1; dx++) for (let dy = -1; dy <= 1; dy++) for (let dz = -1; dz <= 1; dz++)
      for (const j of grid.get([c[0] + dx, c[1] + dy, c[2] + dz].join()) ?? []) best = Math.min(best, Math.hypot(game[j][0] - q[0], game[j][1] - q[1], game[j][2] - q[2]));
    worst = Math.max(worst, best);
  }
  report.checks.payloadFromRetopo = { retopoVertices: acc.count, payloadVertices: count, maxDistanceMetres: worst, tolerance: 2e-5 };
  if (!(worst < 2e-5)) throw new Error(`payload vertices are not the retopo's under the documented fit (worst ${worst})`);
}

// ---------------------------------------------------------------- the skinned body

const restOf = (i) => new THREE.Matrix4().fromArray(source.rest[i]);
function writeBody() {
  const glb = new Glb(GENERATOR), J = glb.json;
  const P = Float32Array.from(source.attributes.position), N = Float32Array.from(source.attributes.normal), UV = Float32Array.from(source.attributes.uv);
  const count = P.length / 3, joints = new Uint8Array(count * 4), weights = new Float32Array(count * 4), color = new Float32Array(count * 4);
  for (let i = 0; i < count; i++) {
    let sum = 0;
    for (let k = 0; k < 4; k++) sum += source.attributes.weights[i * 4 + k];
    for (let k = 0; k < 4; k++) {
      const w = source.attributes.weights[i * 4 + k] / sum;
      weights[i * 4 + k] = w;
      joints[i * 4 + k] = w ? source.attributes.joints[i * 4 + k] + 1 : 0; // +1: the root comes first
    }
    // R: the cloth mask the faction tunic tint follows (infantry-authored.js), G: the skin mask, A: 1
    color[i * 4] = source.attributes.fabric[i]; color[i * 4 + 1] = source.attributes.skin[i]; color[i * 4 + 3] = 1;
  }
  const attributes = {
    POSITION: glb.accessor(P, 'VEC3', { target: 34962, minmax: true }), NORMAL: glb.accessor(N, 'VEC3', { target: 34962 }),
    TEXCOORD_0: glb.accessor(UV, 'VEC2', { target: 34962 }), COLOR_0: glb.accessor(color, 'VEC4', { target: 34962 }),
    JOINTS_0: glb.accessor(joints, 'VEC4', { target: 34962 }), WEIGHTS_0: glb.accessor(weights, 'VEC4', { target: 34962 }),
  };
  // Section 0: everything all factions show (bareHead). Section 1: the authored crown and helmet that only the German
  // figure keeps (dressedBody minus bareHead); the others hide it and wear their own headgear on head_gear.
  const bare = new Set(), crown = [];
  for (let i = 0; i < bareHead.length; i += 3) bare.add(bareHead.slice(i, i + 3).join());
  for (let i = 0; i < dressedBody.length; i += 3) { const t = dressedBody.slice(i, i + 3); if (!bare.has(t.join())) crown.push(...t); }
  J.materials.push({ name: 'Infantry_Body', pbrMetallicRoughness: { metallicFactor: 0, roughnessFactor: 0.9 } });
  J.materials.push({ name: 'Infantry_Crown', pbrMetallicRoughness: { metallicFactor: 0, roughnessFactor: 0.9 } });
  J.meshes.push({ name: 'SK_Infantry', primitives: [
    { attributes, indices: glb.accessor(Uint16Array.from(bareHead), 'SCALAR', { target: 34963 }), material: 0 },
    { attributes, indices: glb.accessor(Uint16Array.from(crown), 'SCALAR', { target: 34963 }), material: 1 },
  ] });
  // Joints: a flat hierarchy under the root. The fitted bones carry a per-bone stretch along x that is not a TRS
  // hierarchy, so every bone is posed in component space (TCInfantryRig.cpp) and a flat skeleton keeps that exact.
  const globals = BONES.map((name, b) => (b >= 1 && b <= 15 ? restOf(b - 1) : new THREE.Matrix4()));
  BONES.forEach((name, b) => {
    const node = { name };
    if (b >= 1 && b <= 15) {
      const e = globals[b].elements;
      node.translation = [e[12], e[13], e[14]];
      node.rotation = quaternionOf(e);
      // The bind pose is the TRS written here, so the inverse bind matrices below are built from the same TRS.
      globals[b] = new THREE.Matrix4().compose(new THREE.Vector3(...node.translation), new THREE.Quaternion(...node.rotation), new THREE.Vector3(1, 1, 1));
    }
    if (b === 0) node.children = BONES.map((_, i) => i).slice(1);
    J.nodes.push(node);
  });
  const ibm = new Float32Array(BONES.length * 16);
  globals.forEach((m, b) => ibm.set(m.clone().invert().elements, b * 16));
  J.skins = [{ name: 'SKEL_Infantry', skeleton: 0, joints: BONES.map((_, i) => i), inverseBindMatrices: glb.accessor(ibm, 'MAT4') }];
  J.nodes.push({ name: 'SK_Infantry', mesh: 0, skin: 0 });
  J.scenes[0].nodes = [0, J.nodes.length - 1];
  const bytes = glb.bytes();
  writeFileSync(join(out, 'SK_Infantry.glb'), bytes);
  report.meshes.SK_Infantry = { file: 'SK_Infantry.glb', sha256: sha256(bytes), vertices: count, triangles: (bareHead.length + crown.length) / 3, sections: [bareHead.length / 3, crown.length / 3], bones: BONES };
}
writeBody();

// The same body as a static mesh for the crowd path (M_InfantryCrowd skins it on the GPU from the keyframe texture):
// UV0 the atlas, UV1 joints 0-1, UV2 joints 2-3, UV3 weights 0-1, UV4 weights 2-3 (joint indices in skeleton order,
// root = 0), vertex color as SK_Infantry. Positions are the bind pose, the same as SK_Infantry's.
function writeCrowdBody() {
  const count = source.attributes.position.length / 3, uv = [1, 2, 3, 4].map(() => new Float32Array(count * 2));
  const color = new Float32Array(count * 4);
  for (let i = 0; i < count; i++) {
    let sum = 0;
    for (let k = 0; k < 4; k++) sum += source.attributes.weights[i * 4 + k];
    for (let k = 0; k < 4; k++) {
      const w = source.attributes.weights[i * 4 + k] / sum;
      uv[k >> 1][i * 2 + (k & 1)] = w ? source.attributes.joints[i * 4 + k] + 1 : 0;
      uv[2 + (k >> 1)][i * 2 + (k & 1)] = w;
    }
    color[i * 4] = source.attributes.fabric[i]; color[i * 4 + 1] = source.attributes.skin[i]; color[i * 4 + 3] = 1;
  }
  const bare = new Set(), crown = [];
  for (let i = 0; i < bareHead.length; i += 3) bare.add(bareHead.slice(i, i + 3).join());
  for (let i = 0; i < dressedBody.length; i += 3) { const t = dressedBody.slice(i, i + 3); if (!bare.has(t.join())) crown.push(...t); }
  // An instanced draw cannot hide a section per instance, so the German crown is its own crowd mesh, drawn for the
  // German figures only (the reference indexes dressedBody for them and bareHead for everyone else).
  for (const [name, indices, slot] of [['SM_InfantryCrowdBody', bareHead, 'InfantryCrowd_Body'], ['SM_InfantryCrowdCrown', crown, 'InfantryCrowd_Crown']]) {
    const glb = new Glb(GENERATOR), J = glb.json;
    const attributes = {
      POSITION: glb.accessor(Float32Array.from(source.attributes.position), 'VEC3', { target: 34962, minmax: true }),
      NORMAL: glb.accessor(Float32Array.from(source.attributes.normal), 'VEC3', { target: 34962 }),
      TEXCOORD_0: glb.accessor(Float32Array.from(source.attributes.uv), 'VEC2', { target: 34962 }),
      TEXCOORD_1: glb.accessor(uv[0], 'VEC2', { target: 34962 }), TEXCOORD_2: glb.accessor(uv[1], 'VEC2', { target: 34962 }),
      TEXCOORD_3: glb.accessor(uv[2], 'VEC2', { target: 34962 }), TEXCOORD_4: glb.accessor(uv[3], 'VEC2', { target: 34962 }),
      COLOR_0: glb.accessor(color, 'VEC4', { target: 34962 }),
    };
    J.materials.push({ name: slot });
    J.meshes.push({ name, primitives: [{ attributes, indices: glb.accessor(Uint16Array.from(indices), 'SCALAR', { target: 34963 }), material: 0 }] });
    J.nodes.push({ name, mesh: 0 });
    J.scenes[0].nodes = [0];
    const bytes = glb.bytes();
    writeFileSync(join(out, `${name}.glb`), bytes);
    report.meshes[name] = { file: `${name}.glb`, sha256: sha256(bytes), vertices: count, triangles: indices.length / 3, uvChannels: ['atlas', 'joints 0-1', 'joints 2-3', 'weights 0-1', 'weights 2-3'] };
  }
}
writeCrowdBody();

// ---------------------------------------------------------------- the kit attachments

const MAT_NAMES = new Map(MATS.map((m, i) => [i, m])); MAT_NAMES.set(-1, 'plain');
function anchorsOf(r, kit) {
  const a = { head_gear: r.H };
  r.arms.forEach((arm, side) => {
    const fore = arm.hand.clone().sub(arm.elbow).normalize();
    const up = r.W ? new THREE.Vector3(...(side === 0 ? [0, -1, 0] : [0, 0, 1])).transformDirection(r.W) : new THREE.Vector3(1, 0, 0).transformDirection(r.body.U);
    a[side ? 'grip_R' : 'grip_L'] = basis(arm.hand, fore, up);
  });
  if (r.W) a.weapon = r.W;
  if (r.sling) a.sling = r.sling;
  r.things.forEach((t, k) => { a[`item_${k}`] = t.m; });
  kitHeights(kit.job, kit.fac).forEach((y, k) => { a[`spine_${k}`] = r.body.on(y); });
  return a;
}
const relative = (anchor, m) => anchor.clone().invert().multiply(m);
const close = (a, b, tol) => a.elements.every((x, i) => Math.abs(x - b.elements[i]) <= tol * Math.max(1, Math.abs(x)));

const meshes = new Map(); // content key -> { hint, facs, geo }
// Merge parts (reference geom.js merge: linear vertex colors and material tags) and key them by content; files are
// written once every kit is known, so each name says which factions share the mesh.
function addKitMesh(parts, hint, fac) {
  const geo = merge(parts);
  const P = geo.attributes.position.array, N = geo.attributes.normal.array, C = geo.attributes.color.array, M = geo.attributes.matId.array, I = geo.index.array;
  // Content key at 1 micrometre (normals and colors at 1e-5): the same reference part reached through different
  // rigs differs only in the last float bits of anchor^-1 * m, which must not split one mesh into many.
  const q = (a, step) => Array.from(a, (x) => Math.round(x / step)).join();
  const key = sha256([q(P, 1e-6), q(N, 1e-5), q(C, 1e-5), q(M, 1), q(I, 1)].join('|')).slice(0, 16);
  if (!meshes.has(key)) meshes.set(key, { hint, facs: new Set(), geo });
  meshes.get(key).facs.add(fac);
  return key;
}
const FAC_TAG = ['us', 'de', 'su', 'uk'];
function writeKitMeshes() {
  const used = new Set(), names = new Map();
  for (const [key, m] of meshes) {
    const shared = m.facs.size === 4 || /^(grip|hose)/.test(m.hint);
    let name = shared ? m.hint : `${m.hint}-${[...m.facs].sort().map((f) => FAC_TAG[f]).join('-')}`, n = 2;
    const base = name;
    while (used.has(name)) name = `${base}-v${n++}`;
    used.add(name); names.set(key, name);
    const geo = m.geo, P = geo.attributes.position.array, N = geo.attributes.normal.array, C = geo.attributes.color.array, M = geo.attributes.matId.array, I = geo.index.array;
    // one primitive per material tag: geom.js MATS index, or 'plain' (skin, faces)
    const byMat = new Map();
    for (let t = 0; t < I.length; t += 3) {
      const id = Math.floor(M[I[t]] + 0.25);
      if (!byMat.has(id)) byMat.set(id, []);
      byMat.get(id).push(I[t], I[t + 1], I[t + 2]);
    }
    const glb = new Glb(GENERATOR), J = glb.json;
    const attributes = { POSITION: glb.accessor(Float32Array.from(P), 'VEC3', { target: 34962, minmax: true }), NORMAL: glb.accessor(Float32Array.from(N), 'VEC3', { target: 34962 }),
      COLOR_0: glb.accessor(Float32Array.from(C), 'VEC3', { target: 34962 }) };
    const primitives = [], materials = [];
    for (const id of [...byMat.keys()].sort((a, b) => a - b)) {
      const mat = MAT_NAMES.get(id) ?? 'plain';
      J.materials.push({ name: `Kit_${mat}` });
      materials.push(mat);
      primitives.push({ attributes, indices: glb.accessor(Uint32Array.from(byMat.get(id)), 'SCALAR', { target: 34963 }), material: J.materials.length - 1 });
    }
    J.meshes.push({ name: `SM_InfantryKit_${name}`, primitives });
    J.nodes.push({ name: `SM_InfantryKit_${name}`, mesh: 0 });
    J.scenes[0].nodes = [0];
    const bytes = glb.bytes();
    writeFileSync(join(out, 'Kit', `${name}.glb`), bytes);
    report.meshes[name] = { name, file: `Kit/${name}.glb`, sha256: sha256(bytes), vertices: P.length / 3, triangles: I.length / 3, materials, factions: [...m.facs].sort() };
  }
  return names;
}

function headName(kit, F) {
  if (kit.fac === 1) return null;
  const kind = kit.type === 'conscript' && kit.job !== 'leader' ? 'pilotka' : kit.type === 'commando' ? 'beret' : kit.type === 'sniper' && kit.fac === 2 ? 'hood' : F.helmet;
  return `head-${kind}`;
}
function hintFor(anchor, kit, F, r0) {
  if (anchor === 'grip_L' || anchor === 'grip_R') return anchor === 'grip_L' ? 'grip-l' : 'grip-r';
  if (anchor === 'head_gear') return headName(kit, F);
  if (anchor === 'weapon') return `weapon-${r0.s.gun.kind}-${F[r0.s.gun.kind] ?? r0.s.gun.kind}`;
  if (anchor === 'sling') return `sling-${F.carbine}`;
  if (anchor.startsWith('item_')) return `item-${r0.things[+anchor.slice(5)].kind}`;
  if (anchor.startsWith('spine_')) return `kit-${kit.job.replace(/B$/, '')}-${anchor.slice(6)}`;
  return anchor;
}

let checkedParts = 0, checkedPoses = 0;
const kitsOut = [];
for (const kit of distinctKits()) {
  const F = FACTIONS[kit.fac] ?? FACTIONS[0];
  const ctx = { type: kit.type, fac: kit.fac, job: kit.job, F, owner: 0x3b73d6 };
  const keys = keyframes(kit.type);
  const rigs = keys.map(({ k, phase }) => solve(ctx, k, phase));
  const partLists = rigs.map((r) => authoredParts(r, ctx));
  const anchorLists = rigs.map((r) => anchorsOf(r, kit));
  const count = partLists[0].length;
  if (partLists.some((p) => p.length !== count)) throw new Error(`${kit.type}/${kit.job}: part count changes between keyframes`);
  const groups = new Map(); // anchor -> [{ geo, color, m, mat }]
  const order = ['head_gear', 'grip_L', 'grip_R', 'weapon', 'sling', 'item_0', 'item_1', 'item_2', 'item_3', 'spine_0', 'spine_1', 'spine_2', 'spine_3'];
  let hose = false;
  for (let j = 1; j < count; j++) {
    const part0 = partLists[0][j];
    let found = null;
    for (const name of order) {
      if (!anchorLists.every((a) => a[name])) continue;
      const rel0 = relative(anchorLists[0][name], part0.m);
      if (anchorLists.every((a, p) => close(relative(a[name], partLists[p][j].m), rel0, 1e-8))) { found = { name, rel0 }; break; }
    }
    if (!found) {
      // the flame hose: built from world points through the hip (identity matrix, geometry changes per pose)
      if (kit.job === 'flamer' && part0.mat === 'rubber' && part0.m.equals(new THREE.Matrix4())) { hose = true; continue; }
      throw new Error(`${kit.type}/${kit.fac}/${kit.job}: part ${j} (${part0.mat}) is not rigid on any anchor`);
    }
    if (!groups.has(found.name)) groups.set(found.name, []);
    groups.get(found.name).push({ geo: part0.geo, color: part0.color, m: found.rel0, mat: part0.mat });
    checkedParts++; checkedPoses += rigs.length;
  }
  const attachments = [];
  for (const [anchor, parts] of groups) attachments.push({ bone: anchor, mesh: addKitMesh(parts, hintFor(anchor, kit, F, rigs[0]), kit.fac) });
  if (hose) {
    // the reference tube() with the hose's radius, sides and color, one metre long on +x; hose_N scales it to length
    const segment = [{ geo: tube([new THREE.Vector3(0, 0, 0), new THREE.Vector3(1, 0, 0)], [0.016, 0.016], 5, new THREE.Vector3(0, 1, 0)), color: 0x2a2724, m: new THREE.Matrix4(), mat: 'rubber' }];
    const name = addKitMesh(segment, 'hose', kit.fac);
    attachments.push({ bone: 'hose_0', mesh: name }, { bone: 'hose_1', mesh: name });
  }
  kitsOut.push({ type: kit.type, fac: kit.fac, slot: kit.i, job: kit.job, sections: kit.fac === 1 ? [0, 1] : [0], attachments, kitHeights: kitHeights(kit.job, kit.fac) });
}
report.checks.kitRigidity = { kits: kitsOut.length, partsChecked: checkedParts, partPoseChecks: checkedPoses, tolerance: 'relative 1e-8 per matrix element' };
const names = writeKitMeshes();
for (const kit of kitsOut) for (const a of kit.attachments) a.mesh = names.get(a.mesh);

const manifest = {
  format: 'ww2-infantry-unreal-v1',
  generator: GENERATOR,
  reference: report.reference,
  axes: 'glTF files keep the reference axes (+x forward, +y up, +z right, metres); the Unreal glTF importer maps (x, y, z) to (x, z, y), which is the project convention ToWorld(x, z, y) * 100',
  bones: BONES,
  body: report.meshes.SK_Infantry,
  crowdBody: report.meshes.SM_InfantryCrowdBody,
  crowdCrown: report.meshes.SM_InfantryCrowdCrown,
  meshes: Object.values(report.meshes).filter((m) => m.name),
  kits: kitsOut,
  checks: report.checks,
};
writeFileSync(join(out, 'kits.json'), JSON.stringify(manifest, null, 1) + '\n');
console.log(JSON.stringify({ kits: kitsOut.length, kitMeshes: meshes.size, checks: report.checks }, null, 1));
