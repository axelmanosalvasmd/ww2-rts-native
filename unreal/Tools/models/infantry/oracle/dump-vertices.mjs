// Reference vertex positions of the authored body for a few motion states, computed the way the reference draws them:
// infantry-authored.js authoredInfantry() for each keyframe rig, each posture keyframe stored undone by its pose
// (unit-models.js poseMorphs), the keyframes blended as three.js absolute morph targets with the weights
// unit-models.js animate() gives them, then the pose node (lean, height scale, shift). The Unreal check commandlet
// (ue-check/TCInfantryRigCheck.cpp) poses SK_Infantry through UTCInfantryAnimInstance and the crowd path and compares.
// Output JSON: { bind: payload rest positions, cases: [{ name, type, fac, slot, motion, positions: [x, y, z, ...] }] },
// reference metres.
import { writeFileSync } from 'node:fs';
import { rigOf } from '../../../../../reference/ww2-rts/client/models/infantry.js';
import { authoredInfantry, authoredSource } from '../../../../../reference/ww2-rts/client/models/infantry-authored.js';
import { AIM_SHIFT } from '../../../../../reference/ww2-rts/client/models/infantry.js';
import { gaitWeights } from '../../../../../reference/ww2-rts/client/squad-motion.js';
import { DIGGERS } from '../figures.mjs';

const out = process.argv[2];
const HAND = [0.7, 1.05], PRONE_LEAN = -1.49, PRONE_X = -0.95, PRONE_Y = 0.12;
const POSES = [[0, 1, 0, 0], [0, 1, 0, 0], [PRONE_LEAN, 1, PRONE_X, PRONE_Y], [0, 1, 0, 0]].map((p, k) => [...p, HAND[0] + AIM_SHIFT[k][0], HAND[1] + AIM_SHIFT[k][1]]);
// keyframe k -> [posture, phase] in TCInfantryRig.cpp SolveKey order (infantry.js figure() order)
function keyRig(type, fac, slot, key) {
  if (key === 0) return [0, rigOf(type, fac, slot, 0)];
  if (key < 4) return [key, rigOf(type, fac, slot, key)];
  if (key < 24) {
    let g = key - 4;
    if (g < 8) return [3, rigOf(type, fac, slot, 3, g / 8)];
    g -= 8;
    return [Math.floor(g / 4), rigOf(type, fac, slot, Math.floor(g / 4), (g % 4) / 4)];
  }
  if (key < 28) return [0, DIGGERS.has(type) ? rigOf(type, fac, slot, 'dig', key - 24) : rigOf(type, fac, slot, 0)];
  return [-1, rigOf(type, fac, slot, 'fallen')];
}
function positions(type, fac, slot, motion) {
  const F = { tunic: 0x5d6668 };
  if (motion.fallen) return Array.from(authoredInfantry(rigOf(type, fac, slot, 'fallen'), { fac: 1, F }).attributes.position.array);
  // unit-models.js animate(), for one man
  const w = motion.w, sum = w.reduce((a, b) => a + b, 0), blend = motion.moving;
  const aim = motion.aim * blend * w[0] / sum;
  const upright = blend * (w[0] + w[3]) / sum - aim, crouch = blend * w[1] / sum, crawl = blend * w[2] / sum;
  const dig = DIGGERS.has(type) ? motion.work * (1 - blend) : 0;
  const gait = new Float64Array(24), pw = new Float64Array(4), mp = new Float64Array(6);
  gaitWeights(motion.phase, upright, 8, gait, 0); gaitWeights(motion.phase, aim, 4, gait, 8);
  gaitWeights(motion.phase, crouch, 4, gait, 12); gaitWeights(motion.phase, crawl, 4, gait, 16);
  if (dig) gaitWeights(motion.workPhase, dig, 4, gait, 20);
  for (let i = 0; i < 4; i++) pw[i] = w[i] / sum * (1 - blend) * (1 - dig);
  for (let i = 0; i < 4; i++) for (let j = 0; j < 6; j++) mp[j] += pw[i] * POSES[i][j];
  for (let j = 0; j < 6; j++) mp[j] += upright * POSES[3][j] + (aim + dig) * POSES[0][j] + crouch * POSES[1][j] + crawl * POSES[2][j];
  const weights = new Float64Array(29);
  for (let i = 1; i < 4; i++) weights[i] = pw[i];
  for (let i = 0; i < 24; i++) weights[4 + i] = gait[i];
  const others = weights.reduce((a, b) => a + b, 0);
  weights[0] = 1 - others;
  let acc = null;
  for (let key = 0; key < 29; key++) {
    if (!weights[key]) continue;
    const [k, rig] = keyRig(type, fac, slot, key);
    const P = authoredInfantry(rig, { fac: 1, F }).attributes.position.array;
    acc ??= new Float64Array(P.length);
    // poseMorphs undo for posture frames (keys 1 to 23); stand and dig frames are stored as built
    const undo = key >= 1 && key < 24 && k > 0;
    const [lean, sy, tx, ty] = POSES[Math.max(0, k)], c = Math.cos(lean), s = Math.sin(lean);
    for (let i = 0; i < P.length; i += 3) {
      let x = P[i], y = P[i + 1];
      if (undo) { const a = x - tx, b = y - ty; x = a * c + b * s; y = (b * c - a * s) / sy; }
      acc[i] += weights[key] * x; acc[i + 1] += weights[key] * y; acc[i + 2] += weights[key] * P[i + 2];
    }
  }
  // the pose node: position (mp2 - ox, mp3 - oy), rotation z mp0, scale y mp1 (three.js T * R * S)
  const ox = mp[4] - HAND[0], oy = mp[5] - HAND[1], c = Math.cos(mp[0]), s = Math.sin(mp[0]);
  for (let i = 0; i < acc.length; i += 3) {
    const x = acc[i], y = acc[i + 1] * mp[1];
    acc[i] = x * c - y * s + mp[2] - ox; acc[i + 1] = x * s + y * c + mp[3] - oy;
  }
  return Array.from(acc);
}
const base = { w: [1, 0, 0, 0], moving: 0, aim: 0, phase: 0, work: 0, workPhase: 0, fallen: false };
const CASES = [
  { name: 'rifleman standing aim', type: 'rifle', fac: 0, slot: 1, motion: { ...base } },
  { name: 'leader running, phase 0.25', type: 'rifle', fac: 1, slot: 0, motion: { ...base, w: [0, 0, 0, 1], moving: 1, phase: 0.25 } },
  { name: 'MG gunner prone', type: 'mg', fac: 2, slot: 0, motion: { ...base, w: [0, 0, 1, 0] } },
  { name: 'engineer dig stroke 2', type: 'engineer', fac: 3, slot: 1, motion: { ...base, work: 1, workPhase: 0.5 } },
  { name: 'sniper fallen', type: 'sniper', fac: 0, slot: 0, motion: { ...base, fallen: true } },
  { name: 'flamethrower crouched walk, phase 0.5', type: 'flamer', fac: 1, slot: 0, motion: { ...base, w: [0, 1, 0, 0], moving: 1, phase: 0.5 } },
  { name: 'bazooka half way to prone', type: 'ranger', fac: 0, slot: 1, motion: { ...base, w: [0.5, 0, 0.5, 0] } },
  { name: 'rifleman walking with aim, phase 0.4', type: 'conscript', fac: 2, slot: 2, motion: { ...base, moving: 0.8, aim: 1, phase: 0.4 } },
];
const cases = CASES.map((c) => ({ ...c, positions: positions(c.type, c.fac, c.slot, c.motion) }));
writeFileSync(out, JSON.stringify({ bind: Array.from(authoredSource.attributes.position), uv: Array.from(authoredSource.attributes.uv), cases }));
console.log(JSON.stringify({ cases: cases.length, vertices: cases[0].positions.length / 3 }));
