// Oracle side A: the pinned reference solver. For every distinct kit and every keyframe it bakes (plus a few phases
// between keyframes, since the native client may sample the gait continuously) write the solved rig: legs, arms,
// body frames, head, weapon, sling, carried things, the 15 skin matrices of infantry-authored.js transforms(), the
// grip frames authoredParts() hangs the closed hands on, and the muzzle. Output: JSON lines, one per case, plus the
// case list the C++ harness reads.
// Usage: INFANTRY_DEPS=<dir with node_modules/three> node --import ../register.mjs dump-reference.mjs <outdir>
import { writeFileSync, mkdirSync } from 'node:fs';
import * as THREE from 'three';
import { rigOf, basis, onTorso, torsoAt, chain, T } from '../../../../../reference/ww2-rts/client/models/infantry.js';
import { transforms } from '../../../../../reference/ww2-rts/client/models/infantry-authored.js';
import { distinctKits, keyframes, kitHeights } from '../figures.mjs';

const out = process.argv[2];
if (!out) throw new Error('usage: dump-reference.mjs <outdir>');
mkdirSync(out, { recursive: true });
const v = (p) => [p.x, p.y, p.z];
const m = (x) => [...x.elements];
const BETWEEN = [0.13, 0.37, 0.61, 0.89]; // off-keyframe gait phases

const cases = [], lines = [];
for (const kit of distinctKits()) {
  const keys = keyframes(kit.type);
  for (const k of [0, 1, 2, 3]) for (const phase of BETWEEN) keys.push({ k, phase });
  for (const { k, phase } of keys) {
    const r = rigOf(kit.type, kit.fac, kit.i, k, phase);
    const grips = r.arms.map((arm, side) => {
      const fore = arm.hand.clone().sub(arm.elbow).normalize();
      const up = r.W ? new THREE.Vector3(...(side === 0 ? [0, -1, 0] : [0, 0, 1])).transformDirection(r.W) : new THREE.Vector3(1, 0, 0).transformDirection(r.body.U);
      return m(basis(arm.hand, fore, up));
    });
    const rec = {
      type: kit.type, fac: kit.fac, i: kit.i, job: kit.job, k, phase,
      legs: r.legs.map((L) => [v(L.hip), v(L.knee), v(L.ankle), v(L.toe), v(L.pole)]),
      arms: r.arms.map((a) => [v(a.sho), v(a.elbow), v(a.hand), [a.err, 0, 0]]),
      B: m(r.body.B), U: m(r.body.U), H: m(r.H), M0: m(r.M0),
      W: r.W ? m(r.W) : null, sling: r.sling ? m(r.sling) : null,
      things: r.things.map((t) => ({ kind: t.kind, m: m(t.m) })),
      skin: transforms(r).map(m), grips,
      muzzle: r.W ? v(new THREE.Vector3(r.info.L, 0, 0).applyMatrix4(r.W)) : null,
      // the kit anchors: body.on(y) at each fastening height, and the two straight hose segments through the
      // three points authoredParts() runs its hose tube through
      spine: kitHeights(kit.job, kit.fac).map((y) => m(r.body.on(y))),
      hose: null,
    };
    if (kit.job === 'flamer' && r.W) {
      const back = chain(onTorso(r.body, 0.9, Math.PI, 0.08), T(0, 0, 0.05));
      const from = new THREE.Vector3(0, -0.22, 0).applyMatrix4(back), hip = r.body.at(torsoAt(0.68, -1.7, 0.08)), to = new THREE.Vector3().applyMatrix4(r.W);
      const seg = (a, b) => m(basis(a, b.clone().sub(a), new THREE.Vector3(0, 1, 0)).multiply(new THREE.Matrix4().makeScale(a.distanceTo(b), 1, 1)));
      rec.hose = [seg(from, hip), seg(hip, to)];
    }
    lines.push(JSON.stringify(rec));
    cases.push(`${kit.type} ${kit.fac} ${kit.i} ${k} ${phase === null ? 'none' : phase}`);
  }
}
writeFileSync(`${out}/reference.jsonl`, lines.join('\n') + '\n');
writeFileSync(`${out}/cases.txt`, cases.join('\n') + '\n');
console.log(JSON.stringify({ cases: cases.length, kits: distinctKits().length }));
