// Oracle for client/squad-motion.js moveSquad: a scripted squad (a curved advance, a halt, moving fire with an aim,
// suppression and a retreat changing the stride, a man hidden, a refill walk, a trench, a teleport and a hidden spell)
// stepped through the reference, writing the inputs (scenario.txt, read by squad-harness.cpp) and the reference
// outputs per step and man (squad-reference.jsonl).
// Usage: node squad-reference.mjs <outdir>
import { writeFileSync, mkdirSync } from 'node:fs';
import { moveSquad } from '../../../../../reference/ww2-rts/client/squad-motion.js';
import { SLOTS, SLOT_SCALE } from '../../../../../reference/ww2-rts/shared/squad-men.js';

const out = process.argv[2];
mkdirSync(out, { recursive: true });
const slots = SLOTS.rifle.map(([x, z]) => [x * SLOT_SCALE, z * SLOT_SCALE]);
const v = { id: 37, squad: {}, root: { position: { x: 10, z: 20 }, rotation: { y: 0 }, visible: true }, garr: false, trench: false,
  flags: 0, supp: 0, cover: 0, tgt: 0, aim: NaN, models: slots.map((slot, index) => ({ visible: true, scale: { x: 1.1 }, userData: { slot, index } })) };
v.squad.moveFire = true;
const lines = [`${v.id} ${slots.length}`, ...slots.map(([x, z], i) => `${x} ${z} ${i} 1.1`)], rows = [];
let x = 10, z = 20, yaw = 0;
for (let step = 0; step < 400; step++) {
  const dt = step % 37 === 5 ? 0 : 1 / 30 + (step % 7) * 0.004 + (step === 200 ? 0.2 : 0);
  const moving = step < 120 || (step >= 160 && step < 260) || step >= 330;
  if (moving) { yaw += (step < 60 ? 0.012 : -0.008); const sp = step >= 330 ? 6 : 4.5; x += Math.cos(yaw) * sp * dt; z -= Math.sin(yaw) * sp * dt; }
  if (step === 300) { x += 12; z -= 3; } // teleport: more than 8 m in one snapshot
  v.root.position.x = x; v.root.position.z = z; v.root.rotation.y = yaw;
  v.root.visible = !(step >= 280 && step < 290);
  v.trench = step >= 240 && step < 250;
  v.flags = step >= 330 ? 1 : 0;
  v.supp = step < 100 ? 0 : step < 180 ? 60 : step < 230 ? 95 : 10;
  v.cover = step >= 190 && step < 210 ? 2 : 0;
  v.tgt = step >= 160 && step < 220 ? 5 : 0;
  v.aim = step >= 160 && step < 220 ? 0.4 + step * 0.01 : NaN;
  v.models[3].visible = !(step >= 100 && step < 170);
  if (step === 150) v.models[4].userData.refill = true;
  const refill = v.models.map((m) => (m.userData.refill ? 1 : 0));
  lines.push([dt, x, z, yaw, v.root.visible ? 1 : 0, v.garr ? 1 : 0, v.trench ? 1 : 0, v.flags, v.supp, v.cover, v.squad.moveFire ? 1 : 0, v.tgt ? 1 : 0,
    Number.isFinite(v.aim) ? 1 : 0, Number.isFinite(v.aim) ? v.aim : 0, ...v.models.map((m, i) => `${m.visible ? 1 : 0} ${refill[i]}`)].join(' '));
  moveSquad(v, dt);
  rows.push(JSON.stringify(v.models.map((m) => { const q = m.userData.motion; return [q.x, q.z, q.yaw, q.phase, q.blend, q.speed, q.localX, q.localZ, q.localYaw]; })));
}
writeFileSync(`${out}/scenario.txt`, lines.join('\n') + '\n');
writeFileSync(`${out}/squad-reference.jsonl`, rows.join('\n') + '\n');
console.log(JSON.stringify({ steps: rows.length, men: slots.length }));
