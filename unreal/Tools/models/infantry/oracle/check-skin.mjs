// End-to-end vertex oracle: skin the exported SK_Infantry.glb (its own bind pose and inverse bind matrices, float32)
// with the C++ port's component-space bone transforms (rig-harness output, field "bones") and compare every vertex
// with the reference's authoredInfantry(rig) for the same case. This checks the port, the bone convention
// (pose = skin matrix * rest) and the exported bind pose together. Tolerance: 2e-5 soldier metres (float32 storage
// of positions and inverse bind matrices; the rest frames themselves are float32 values from Blender).
// Usage: node --import ../register.mjs check-skin.mjs <SK_Infantry.glb> <reference.jsonl> <port.jsonl> <report.json>
import { readFileSync, writeFileSync } from 'node:fs';
import * as THREE from 'three';
import { rigOf } from '../../../../../reference/ww2-rts/client/models/infantry.js';
import { authoredInfantry } from '../../../../../reference/ww2-rts/client/models/infantry-authored.js';

const [glbPath, refPath, portPath, reportPath] = process.argv.slice(2);
const TOLERANCE = 2e-5;
const b = readFileSync(glbPath), len = b.readUInt32LE(12), json = JSON.parse(b.subarray(20, 20 + len).toString());
const bin = b.subarray(20 + len + 8);
const read = (index, Type) => {
  const a = json.accessors[index], v = json.bufferViews[a.bufferView], n = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT4: 16 }[a.type];
  return new Type(bin.buffer.slice(bin.byteOffset + v.byteOffset, bin.byteOffset + v.byteOffset + a.count * n * Type.BYTES_PER_ELEMENT));
};
const attrs = json.meshes[0].primitives[0].attributes;
const P = read(attrs.POSITION, Float32Array), Jn = read(attrs.JOINTS_0, Uint8Array), Wt = read(attrs.WEIGHTS_0, Float32Array);
const ibm = read(json.skins[0].inverseBindMatrices, Float32Array);
const count = P.length / 3, bones = json.skins[0].joints.length;
const ref = readFileSync(refPath, 'utf8').trim().split('\n'), port = readFileSync(portPath, 'utf8').trim().split('\n');
let worst = 0, worstWhere = '', cases = 0;
const F = { 0: 0x5f6046, 1: 0x5d6668, 2: 0x7f7a5c, 3: 0x6e5f42 };
// every 7th case: all kits and poses are spread over it
for (let n = 0; n < ref.length; n += 7) {
  const r = JSON.parse(ref[n]), p = JSON.parse(port[n]);
  const rig = rigOf(r.type, r.fac, r.i, r.k, r.phase);
  const expect = authoredInfantry(rig, { fac: 1, F: { tunic: F[r.fac] } }).attributes.position.array;
  const skin = p.bones.map((m, j) => (m ? new THREE.Matrix4().fromArray(m).multiply(new THREE.Matrix4().fromArray(ibm, j * 16)) : null));
  const q = new THREE.Vector3(), s = new THREE.Vector3();
  for (let i = 0; i < count; i++) {
    s.set(0, 0, 0);
    for (let k = 0; k < 4; k++) {
      const w = Wt[i * 4 + k];
      if (w) s.addScaledVector(q.fromArray(P, i * 3).applyMatrix4(skin[Jn[i * 4 + k]]), w);
    }
    const d = Math.hypot(s.x - expect[i * 3], s.y - expect[i * 3 + 1], s.z - expect[i * 3 + 2]);
    if (d > worst) { worst = d; worstWhere = `${r.type}/${r.fac}/${r.job} k=${r.k} phase=${r.phase} vertex ${i}`; }
  }
  cases++;
}
const report = { tolerance: TOLERANCE, cases, vertices: count, bones, maxErrorMetres: worst, worstAt: worstWhere, pass: worst <= TOLERANCE };
writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report));
if (!report.pass) process.exit(1);
