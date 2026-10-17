// Oracle side A: the pinned reference's motion code, run under Node on scripted scenarios, unmodified on disk:
//   unit-models.js animate() for squads (posture, aim, dig, LOD, suspension, moveSquad, pose node, muzzle),
//   animate() for vehicles and boats (model-motion.js moveModel, wheel-motion.js moveWheels) and the kaiju (stride),
//   track-motion.js trackVertex on the Sherman's own track links, wheel-motion.js wheelAngle on its wheels,
//   aircraft.js tick (bank, wobble, propellers), unit-models.js createBodies (corpses: cap, soft cap, life, fade,
//   sink, the toss beside a blast).
// Writes scenario.txt (the inputs, for motion-harness.cpp) and reference.jsonl (what the reference drew).
// Math.random and performance.now are replaced by recorded sequences so both sides see the same values.
// Usage: INFANTRY_DEPS=<dir with node_modules/three> node --import ../register.mjs motion-reference.mjs <outdir>
import { writeFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const REF = join(HERE, '../../../../reference/ww2-rts');
const out = process.argv[2];
if (!out) throw new Error('usage: motion-reference.mjs <outdir>');
mkdirSync(out, { recursive: true });

// recorded randomness: every Math.random() the reference draws is written to the scenario in order
let seed = 0x2545f491;
const draws = [];
const rng = () => { seed |= 0; seed = (seed + 0x6d2b79f5) | 0; let t = Math.imul(seed ^ (seed >>> 15), 1 | seed); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
Math.random = () => { const r = rng(); draws.push(r); return r; };
let clock = 1000;
globalThis.performance = { now: () => clock * 1000 };

const THREE = await import('three');
const um = await import(join(REF, 'client/unit-models.js'));
const { wheelAngle } = await import(join(REF, 'client/wheel-motion.js'));
const { trackVertex } = await import(join(REF, 'client/track-motion.js'));
const { createAviation } = await import(join(REF, 'client/aircraft.js'));
const { UNITS } = await import(join(REF, 'shared/sim.js'));
const { soldierKit } = await import(join(REF, 'client/models/infantry.js'));

const scen = [], lines = [];
const S = (...a) => scen.push(a.map(String).join(' '));
const O = (o) => lines.push(JSON.stringify(o));
// scenario values from a fixed generator (not Math.random, which the reference itself draws from)
let gs = 12345;
const gen = () => { gs = (Math.imul(gs, 1103515245) + 12345) >>> 0; return gs / 4294967296; };
const G = (x, z) => 0.4 * Math.sin(0.13 * x) + 0.3 * Math.cos(0.11 * z) + 2; // the ground both sides use
const LOOK = { uniform: 0x6b7248, vehicle: 0x59623d, color: 0x3b73d6 };
const v3 = (p) => [p.x, p.y, p.z];

// ------------------------------------------------------------------ the tracks first (a fresh path table)
{
  const root = new THREE.Group(), v = { id: 4, type: 'medium', root, models: [], turret: null };
  um.buildModel(v, root, LOOK, 0, UNITS.medium);
  let mesh = null;
  root.traverse((o) => { if (o.userData.wheelTravel && o.geometry.attributes.trackData) mesh = o; });
  const geo = mesh.geometry, T = geo.attributes.trackData, P = geo.attributes.position, W = geo.attributes.wheelPivot, side = geo.attributes.wheelSide;
  const loops = geo.userData.trackLoops;
  S('tracks', loops.length);
  for (const loop of loops) S('loop', loop.points.length, ...loop.points.flat());
  const rows = new Set();
  const picks = [];
  for (let i = 0; i < T.count; i++) if (T.getY(i) >= 0) { rows.add(T.getY(i)); if (i % 37 === 0) picks.push(i); }
  O({ kind: 'trackrows', rows: [...rows].sort((a, b) => a - b) });
  const motions = [[0, 0], [0.37, 0], [-2.9, 0.4], [15.3, -1.7], [123.456, 3.2]];
  for (const i of picks) for (const [travel, turn] of motions) {
    S('tv', P.getX(i), P.getY(i), P.getZ(i), T.getX(i), T.getY(i), T.getZ(i), T.getW(i), travel, turn);
    O({ kind: 'tv', p: v3(trackVertex(geo, i, travel, turn)) });
  }
  const pivots = new Map();
  for (let i = 0; i < W.count; i++) if (W.getW(i) > 0) pivots.set([W.getX(i), W.getY(i), W.getZ(i), W.getW(i), side.getX(i)].join(), [W.getX(i), W.getY(i), W.getZ(i), W.getW(i), side.getX(i)]);
  for (const p of pivots.values()) for (const [travel, turn] of motions) {
    S('wheel', p[3], p[4], travel, turn);
    O({ kind: 'wheel', a: wheelAngle(p, travel, turn) });
  }
}

// ------------------------------------------------------------------ squads
const SQUADS = [['rifle', 0], ['rifle', 1], ['mg', 1], ['mortar', 2], ['at', 3], ['sniper', 0], ['medic', 1], ['flamer', 2], ['engineer', 0],
  ['conscript', 2], ['commando', 3], ['ranger', 1], ['howitzer', 0], ['flak', 2]];
let id = 40;
for (const [type, fac] of SQUADS) {
  const root = new THREE.Group(), v = { id: id++, type, root, models: [], turret: null, x: 0, z: 0 };
  um.buildModel(v, root, LOOK, fac, UNITS[type]);
  const figure = type === 'medic' ? 'engineer' : type, men = v.models.length;
  const digger = v.models[0].userData.hi[0].morphTargetInfluences.length > 23;
  S('squad', type, figure, fac, v.id, men, digger ? 1 : 0, v.squad.moveFire ? 1 : 0);
  v.models.forEach((man, i) => S('man', i, man.userData.slot[0], man.userData.slot[1], man.scale.x, soldierKit(figure, i, fac)));
  // a path: march, wheel about, halt under fire, go to ground, retreat, dig, hide, teleport, garrison, lose men
  let x = 10 + gen() * 20, z = -5 + gen() * 10, rot = gen() * 6, speed = 0, alive = men;
  let flags = 0, supp = 0, cover = 0, tgt = 0, aim = rot, garr = 0, visible = 1, detailed = 1;
  for (let step = 0; step < 420; step++) {
    const dt = step % 53 === 7 ? 0 : 0.012 + gen() * 0.03;
    const phase = Math.floor(step / 30) % 14;
    speed = [1.4, 1.4, 2.2, 0.6, 0, 0, 1.0, 3.0, 0, 0.2, 1.4, 0.8, 0, 2.5][phase];
    rot += [0, 0.9, -0.6, 0, 0, 0, 0.4, 0, 0, 0, -1.2, 0, 0, 0.3][phase] * dt;
    flags = phase === 7 ? 1 : phase === 8 ? 16 : phase === 9 ? 128 : 0;
    supp = phase === 4 ? 60 : phase === 5 ? 95 : phase === 6 ? 70 : phase === 12 ? 92 : 0;
    cover = phase === 12 ? 2 : 0;
    tgt = phase === 2 || phase === 3 || phase === 10 ? 99 : 0;
    aim = rot + Math.sin(step * 0.05) * 0.8;
    visible = phase === 11 && step % 30 < 12 ? 0 : 1;
    detailed = step % 97 > 90 ? 0 : 1;
    garr = phase === 13 && step % 30 > 20 ? 1 : 0;
    if (step === 250 || step === 330) alive = Math.max(1, alive - 1 - (step === 330 ? 2 : 0));
    if (step === 300) alive = Math.min(men, alive + 1);
    x += Math.cos(rot) * speed * dt; z += Math.sin(rot) * speed * dt;
    if (step === 200) { x += 12; z -= 3; } // a jump: the men snap to their slots
    const eyeD = 60 + 80 * (0.5 + 0.5 * Math.sin(step * 0.031));
    const eye = { x: x + eyeD * 0.6, y: G(x, z) + eyeD * 0.7, z: z + eyeD * 0.38 };
    S('step', dt, x, G(x, z), z, -rot, visible, detailed, eye.x, eye.y, eye.z, flags, supp, cover, tgt ? 1 : 0, aim, garr, alive);
    // main.js snapshot and frame loop, as far as animate() reads them
    v.x = x; v.z = z; v.rot = rot; v.flags = flags; v.supp = supp; v.cover = cover; v.tgt = tgt; v.aim = aim; v.garr = !!garr;
    v.models.forEach((man, i) => { man.visible = i < alive && !garr; });
    root.visible = !!visible;
    root.position.set(x, G(x, z), z); root.rotation.y = -rot;
    um.animate(v, dt, new THREE.Vector3(eye.x, eye.y, eye.z), G, !!detailed);
    const sq = v.squad;
    O({ kind: 'squad', type, step, far: sq.far, men: v.models.map((man) => {
      const u = man.userData, m = u.motion, lod = sq.far ? u.lo : u.hi;
      return { p: v3(man.position), ry: man.rotation.y, lean: u.pose.rotation.z, sy: u.pose.scale.y, node: [u.pose.position.x, u.pose.position.y],
        w: [...lod[0].morphTargetInfluences], muzzle: u.muzzle ? v3(u.muzzle) : null, m: m ? [m.x, m.z, m.yaw, m.phase, m.blend] : null };
    }) });
  }
  S('end');
}

// ------------------------------------------------------------------ vehicles, boats and the kaiju
const VEHICLES = [['medium', 0], ['armoredcar', 0], ['tiger', 1], ['halftrack', 2], ['lcvp', 0], ['gunboat', 3], ['destroyer', 1], ['kaiju', 0], ['truck', 2]];
for (const [type, fac] of VEHICLES) {
  const root = new THREE.Group(), v = { id: id++, type, root, models: [], turret: null, x: 0, z: 0 };
  um.buildModel(v, root, LOOK, fac, UNITS[type]);
  S('vehicle', type, v.id);
  let x = gen() * 30, z = gen() * 30, rot = gen() * 6, speed = 0;
  for (let step = 0; step < 360; step++) {
    const dt = step % 41 === 3 ? 0 : 0.01 + gen() * 0.04;
    const phase = Math.floor(step / 30) % 12;
    speed = [0, 2, 5, 5, 0, -2, 0, 9, 3, 0, 4, 1][phase];
    rot += [0, 0, 0.5, -0.8, 0, 0, 0, 0.2, 1.5, 0, -0.4, 0][phase] * dt;
    x += Math.cos(rot) * speed * dt; z += Math.sin(rot) * speed * dt;
    if (step === 150) x += 20; // a snapshot correction past 8 m
    const visible = phase === 9 && step % 30 < 10 ? 0 : 1;
    const y = G(x, z) + (type === 'lcvp' || type === 'gunboat' || type === 'destroyer' ? 0 : 0);
    S('vstep', dt, x, y, z, -rot, visible);
    v.x = x; v.z = z; root.visible = !!visible;
    root.position.set(x, y, z); root.rotation.y = -rot;
    um.animate(v, dt, new THREE.Vector3(0, 50, 0), G, true);
    const o = { kind: 'vehicle', type, step };
    if (v.legs) o.kaiju = [v.legs[0].rotation.z, v.legs[1].rotation.z, v.legs[0].position.y, v.legs[1].position.y, v.hull.position.y, v.hull.rotation.x, v.turret.position.y, v.tail.position.y, v.tail.rotation.y];
    else {
      const b = v.visualBody;
      o.body = b ? [b.rotation.x, b.rotation.z, b.position.y] : null;
      o.wheels = v.wheelMotion && v.wheelMotion.meshes.length ? [v.wheelMotion.travel, v.wheelMotion.turn] : null;
    }
    O(o);
  }
  S('end');
}

// ------------------------------------------------------------------ planes
{
  const world = new THREE.Group();
  const aviation = createAviation({ hAt: G, UNITS, units: new Map(), altitude: 20, world: () => world, facOf: () => 0, colorOf: () => 0x3b73d6, vehicleOf: () => 0x59623d, me: () => 0, air: () => {} });
  for (const type of ['fighter', 'bomber']) {
    const root = new THREE.Group(), v = { id: id++, type, root, models: [], turret: null, x: 5, z: 5, rot: 0.3, hp: 1000 };
    aviation.buildUnit(v, root, type, 0);
    // instance(): each propeller starts at rotation.x = random * TAU and spins at 22 + random * 4 rad/s
    const props = v.props.map((p) => [p.rotation.x, p.userData.spin]);
    S('plane', v.id, props.length, ...props.flat());
    let rot = 0.3;
    for (let step = 0; step < 240; step++) {
      const dt = step % 31 === 5 ? 0 : 0.01 + gen() * 0.03;
      rot += [0, 1.2, -2, 0.4][Math.floor(step / 40) % 4] * dt;
      clock += dt;
      const visible = Math.floor(step / 40) % 6 === 4 && step % 40 < 15 ? 0 : 1;
      v.rot = rot; v.x += Math.cos(rot) * 30 * dt; v.z += Math.sin(rot) * 30 * dt; v.hp = 1000;
      root.visible = !!visible;
      S('pstep', dt, rot, visible, clock);
      aviation.tick(v, dt);
      O({ kind: 'plane', step, body: [v.body.rotation.x, v.body.rotation.z, v.bank], props: v.props.map((p) => p.rotation.x) });
    }
    S('end');
  }
}

// ------------------------------------------------------------------ corpses
{
  const scene = new THREE.Scene(), world = new THREE.Group();
  scene.add(world);
  const root = new THREE.Group(), v = { id: 7, type: 'rifle', root, models: [], turret: null };
  um.buildModel(v, root, LOOK, 0, UNITS.rifle);
  const man = v.models[0];
  const bodies = um.createBodies();
  let time = 0;
  const fx = { blastNear: (x, z) => (time > 3 && time < 3.5 && Math.hypot(x - 20, z - 20) < 6 ? { x: 20, z: 20, s: 3 } : null), gore: () => {} };
  S('bodies');
  const rec = (r0) => { const r = draws.slice(r0); return r; };
  for (let step = 0; step < 2400; step++) {
    const dt = step % 61 === 1 ? 0 : 0.01 + gen() * 0.03;
    const adds = step < 1500 && step % 4 === 0 ? 1 + (step % 18 === 0 ? 1 : 0) : 0;
    for (let a = 0; a < adds; a++) {
      const bx = 14 + gen() * 12, bz = 14 + gen() * 12, yaw = gen() * 6.28;
      bodies.add(world, bx, G(bx, bz), bz, man, yaw);
      S('add', bx, G(bx, bz), bz, yaw);
    }
    const r0 = draws.length;
    time += dt;
    bodies.update(dt, fx, G, null);
    S('update', dt, time, ...rec(r0).length ? ['r', ...rec(r0)] : []);
    if (step % 10 === 0 || step > 2390) {
      const list = [];
      for (const mesh of world.children) {
        if (!mesh.isInstancedMesh) continue;
        const f = mesh.geometry.attributes.fade, m = new THREE.Matrix4();
        for (let i = 0; i < mesh.count; i++) { mesh.getMatrixAt(i, m); list.push({ m: [...m.elements], fade: f.array[i] }); }
      }
      O({ kind: 'bodies', step, count: bodies.count, list });
      S('snap');
    }
  }
  S('end');
}

writeFileSync(join(out, 'scenario.txt'), scen.join('\n') + '\n');
writeFileSync(join(out, 'reference.jsonl'), lines.join('\n') + '\n');
console.log(JSON.stringify({ scenarioLines: scen.length, records: lines.length, randomDraws: draws.length }));
