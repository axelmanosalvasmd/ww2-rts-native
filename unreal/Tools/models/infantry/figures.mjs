// The infantry figures the reference builds (client/unit-models.js: one soldier() per formation slot of
// shared/squad-men.js SLOTS, medics borrowing the engineer figure) reduced to distinct (type, faction, job) kits.
import { SLOTS } from '../../../../reference/ww2-rts/shared/squad-men.js';
import { soldierKit } from '../../../../reference/ww2-rts/client/models/infantry.js';

// Unit types limited to one faction (shared/sim.js UNITS faction); the rest are recruitable by all four.
const ONLY = { ranger: 0, conscript: 2, commando: 3 };
export const FIGURE_TYPES = ['rifle', 'conscript', 'commando', 'ranger', 'sniper', 'engineer', 'flamer', 'mg', 'mortar', 'at', 'howitzer', 'flak'];

// [{ type, fac, i, job }] with i the first slot index that wears the kit
export function distinctKits() {
  const out = [], seen = new Set();
  for (const type of FIGURE_TYPES) for (let fac = 0; fac < 4; fac++) {
    if (type in ONLY && ONLY[type] !== fac) continue;
    SLOTS[type].forEach((_, i) => {
      const job = soldierKit(type, i, fac), key = `${type}|${fac}|${job}`;
      if (seen.has(key)) return;
      seen.add(key);
      out.push({ type, fac, i, job });
    });
  }
  return out;
}

// Every keyframe the reference bakes for a figure (infantry.js figure()): postures 0 to 3, the gait frames
// [[3, 8], [0, 4], [1, 4], [2, 4]], the dig strokes for diggers, and the fallen build.
export const DIGGERS = new Set(['rifle', 'conscript', 'engineer']);
export function keyframes(type) {
  const keys = [0, 1, 2, 3].map((k) => ({ k, phase: null }));
  for (const [k, count] of [[3, 8], [0, 4], [1, 4], [2, 4]]) for (let j = 0; j < count; j++) keys.push({ k, phase: j / count });
  if (DIGGERS.has(type)) for (let j = 0; j < 4; j++) keys.push({ k: 'dig', phase: j });
  keys.push({ k: 'fallen', phase: null });
  return keys;
}

// Torso heights where a job's carried kit is fastened (infantry.js authoredParts reads them through onTorso(b, y)):
// the anchors spine_0 to spine_3 sit at body.on(y). Same table as TCInfantryRig.cpp KitHeights; the exporter proves
// each kit part is rigid on one of them across every keyframe, and the oracle compares the C++ frames to these.
export function kitHeights(job, fac) {
  if (job === 'flamer') return fac === 2 ? [0.9, 0.8, 1.0, 0.7] : fac === 1 ? [0.9, 0.93, 0.74, 1.06] : [0.9, 0.92, 1.14];
  if (job.startsWith('engineer')) return fac === 1 ? [0.76, 0.84] : [0.96, 0.86, 1.1];
  return [];
}

// The exported skeleton: root, the 15 Blender deform bones (infantry-authored-data.js order), then the kit anchors.
// TCInfantryRig.cpp EBone/BoneName use the same order.
// Unreal bone names cannot hold '.', so the Blender names (thigh.L) are written with '_' (thigh_L), as the glTF
// importer would sanitize them anyway; BLENDER_DEFORM keeps the payload's own names for the order check.
export const BLENDER_DEFORM = ['pelvis', 'chest', 'head', 'thigh.L', 'shin.L', 'foot.L', 'upper_arm.L', 'forearm.L', 'hand.L',
  'thigh.R', 'shin.R', 'foot.R', 'upper_arm.R', 'forearm.R', 'hand.R'];
export const DEFORM = BLENDER_DEFORM.map((name) => name.replace('.', '_'));
export const ANCHORS = ['head_gear', 'grip_L', 'grip_R', 'weapon', 'sling', 'item_0', 'item_1', 'item_2', 'item_3',
  'spine_0', 'spine_1', 'spine_2', 'spine_3', 'hose_0', 'hose_1'];
export const BONES = ['root', ...DEFORM, ...ANCHORS];
