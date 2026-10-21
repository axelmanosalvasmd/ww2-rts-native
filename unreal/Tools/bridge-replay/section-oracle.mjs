// Generate a compact pose oracle from the pinned Three.js and unmodified debris modules.
// node section-oracle.mjs <reference root> <three root> <output.json>
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
const [ref, three, out] = process.argv.slice(2);
if (!out) throw Error('usage: section-oracle.mjs <reference root> <three root> <output.json>');
const load = p => import(pathToFileURL(path.resolve(p)));
const THREE = await load(path.join(three, 'build/three.module.js'));
const { debrisBody, stepDebris, debrisRow } = await load(path.join(ref, 'shared/debris-motion.js'));
const { createDebrisView } = await load(path.join(ref, 'client/debris.js'));
const poses = [];
for (const dir of [0, Math.PI / 2, -0.73, 2.41]) {
  const body = debrisBody({id: 'section', x:107, y:5, z:93, dir, impulse:4, mass:1200});
  let previous = 0;
  for (const age of [0, 0.1, 0.35, 0.7, 1]) {
    stepDebris(body, age - previous, { ground: () => -10 }); previous = age;
    const row = debrisRow(body), object = new THREE.Object3D();
    object.position.set(row.x, row.y, row.z);
    object.rotation.set(0, -row.dir, -row.tilt, 'YXZ'); object.updateMatrix();
    const corners = [];
    for (const x of [-0.9,0.9]) for (const y of [-1.4,1.4]) for (const z of [-0.1,0.1]) {
      const p = new THREE.Vector3(x,y,z).applyMatrix4(object.matrix);
      corners.push([p.x,p.z,p.y].map(v=>v*100));
    }
    poses.push({row, age, corners});
  }
}
let interpolated;
const view = createDebrisView({pose: (id,row)=>{interpolated=row;}});
view.snapshot({tick:2, falling:[poses[1].row]}, 0);
view.snapshot({tick:4, falling:[poses[2].row]}, 0.1);
view.frame(0.15);
const sources = Object.fromEntries(['client/fx.js','client/debris.js','shared/debris-motion.js','client/structures.js'].map(p=>[p,createHash('sha256').update(fs.readFileSync(path.join(ref,p))).digest('hex')]));
fs.writeFileSync(out,JSON.stringify({pin:'e4e579a', three:THREE.REVISION, sources, poses, interpolated},null,2)+'\n');
console.log(`${poses.length} poses, ${poses.length*8} transformed corners; three r${THREE.REVISION}; ${out}`);
