// The reference half of the effects side by side: the pinned client/fx.js (createEffects) itself, fed the scripted
// snapshots of scenes.json the way client/main.js feeds it (fires as cell centres with the cell's character, the seen
// set, removeUnit's wreck for a killed vehicle), around units built by client/unit-models.js, under client/light.js's
// default mood, from client/camera.js's default view. Math.random is seeded before fx.js loads, so the pictures repeat.
// Posts <scene>-<moment>-reference.png and reference-scenes.json (what was alive) back to shoot-fx-reference.mjs.
const W = 1280, H = 720;
const post = (url, body) => fetch(url, { method: 'POST', body });
window.addEventListener('error', (e) => post('/log', `error: ${e.message}`));
window.addEventListener('unhandledrejection', (e) => post('/log', `rejection: ${e.reason?.stack ?? e.reason}`));
// mulberry32: fx.js takes rand = Math.random when it loads
let seed = 0x7c2a;
Math.random = () => { seed |= 0; seed = (seed + 0x6d2b79f5) | 0; let t = Math.imul(seed ^ (seed >>> 15), 1 | seed); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };

const THREE = await import('three');
const { UNITS, CELL } = await import('/shared/sim.js');
const { createEffects } = await import('/client/fx.js');
const { buildModel, animate, setSurfaces, setBuildings } = await import('/client/unit-models.js');
const { loadModelTextures } = await import('/client/model-textures.js');
const { ownerRing } = await import('/client/markers.js');
const { setupLight, sky } = await import('/client/light.js');
const { surface } = await import('/client/surfaces.js');
const { buildingModel } = await import('/client/structures.js');
const { setWind } = await import('/client/wind.js');

const doc = await (await fetch('/fxcompare/scenes.json')).json();
const FACTIONS = [{ uniform: 0x6b7248, vehicle: 0x59623d }, { uniform: 0x5c6266, vehicle: 0x50565a }, { uniform: 0x7d7250, vehicle: 0x4e5a38 }, { uniform: 0x6f6448, vehicle: 0x565640 }];
const COLORS = [0x3b73d6, 0xcc3a2e, 0xece6d6, 0xe2832b, 0x9b5cd4, 0x35b6c0];
const SUN_SIDE = 60 * Math.PI / 180, GRASS = 0x6c6c45;

setSurfaces(surface); setBuildings(buildingModel);
let pending = 0;
const manager = THREE.DefaultLoadingManager, begin = manager.itemStart, end = manager.itemEnd;
manager.itemStart = (u) => { pending++; begin(u); };
manager.itemEnd = (u) => { pending = Math.max(0, pending - 1); end(u); };
loadModelTextures();
const renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
renderer.setPixelRatio(1);
renderer.setSize(W, H);
document.body.append(renderer.domElement);
const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(doc.camera.fov, W / H, 0.5, 3000);
const { sun } = setupLight(renderer, scene, camera);
scene.fog.near = 1e4; scene.fog.far = 2e4; // as the models' side by side: no haze
const SUN_DIR = new THREE.Vector3();
{ const az = Math.PI - SUN_SIDE, up = sky.sunUp * Math.PI / 180; SUN_DIR.set(Math.sin(az) * Math.cos(up), Math.sin(up), Math.cos(az) * Math.cos(up)); }
const ground = new THREE.Mesh(new THREE.PlaneGeometry(600, 600), new THREE.MeshLambertMaterial({ color: GRASS }));
ground.rotation.x = -Math.PI / 2; ground.position.set(60, 0, 60); ground.receiveShadow = true;
scene.add(ground);

// the map: a hedge row and a patch of rubble (client/main.js physicalGrid)
const MW = doc.map.w, grid = Array.from({ length: doc.map.h }, (_, y) => Array.from({ length: MW }, (_, x) =>
  y === doc.map.hedge.row && x >= doc.map.hedge.x0 && x < doc.map.hedge.x1 ? 'H' : doc.map.rubble.rows.includes(y) && x >= doc.map.rubble.x0 && x < doc.map.rubble.x1 ? 'R' : '.'));
const { createRubbleDecals } = await import('/client/rubble-decals.js');
const rubble = createRubbleDecals({ parent: scene, hAt: () => 0, w: MW, grid });
rubble.update(grid.flatMap((row, y) => row.flatMap((ch, x) => ch === 'R' ? [y * MW + x] : [])));

function makeUnit([id, type, owner, x, z, rot]) {
  const def = UNITS[type], root = new THREE.Group();
  const f = { ...FACTIONS[owner], color: COLORS[owner] };
  const v = { id, type, owner, root, models: [], alive: def.models, x, z, rot, aim: rot, turret: null };
  const base = ownerRing(def.radius + 0.4, f.color);
  root.add(base); v.base = base;
  buildModel(v, root, f, owner, def);
  v.supp = 0; v.flags = 0; v.built = 1;
  root.position.set(x, 0, z); root.rotation.y = -rot;
  if (v.turret) v.turret.rotation.y = 0;
  animate(v, 10, new THREE.Vector3(x, 30, z + 30));
  scene.add(root);
  return v;
}
const units = new Map();
const cam = { x: 0, z: 0, dist: doc.camera.dist };
const fx = createEffects({ scene, camera, cam, hAt: () => 0, units, airAlt: 20, mapW: () => MW });
const frame = () => new Promise((r) => requestAnimationFrame(() => r()));

function place(focus) {
  const p = doc.camera.pitch, d = doc.camera.dist, [fx0, fz0] = focus;
  camera.position.set(fx0, d * Math.sin(p), fz0 + d * Math.cos(p)); camera.lookAt(fx0, 0, fz0); camera.updateMatrixWorld();
  cam.x = fx0; cam.z = fz0;
  const c = new THREE.Vector3(fx0, 0, fz0), rad = 45;
  sun.target.position.copy(c); sun.position.copy(c).addScaledVector(SUN_DIR, rad + 120); sun.target.updateMatrixWorld();
  Object.assign(sun.shadow.camera, { left: -rad, right: rad, top: rad, bottom: -rad, near: 1, far: 2 * rad + 240 });
  sun.shadow.camera.updateProjectionMatrix();
}
function timeline(sc) {
  const out = new Map();
  for (const st of sc.steps) for (let i = 0; i < (st.count ?? 1); i++) {
    const at = Math.round((st.at + i * (st.every ?? 0)) * 1000) / 1000, m = out.get(at) ?? { shots: [] };
    if (st.gone) m.gone = [...(m.gone ?? []), ...st.gone];
    for (const [k, v] of Object.entries(st.fields ?? {})) { if (k === 'shots') m.shots.push(...v); else m[k] = v; }
    out.set(at, m);
  }
  return [...out.entries()].sort((a, b) => a[0] - b[0]);
}
async function main() {
  for (let i = 0; i < 400 && pending > 0; i++) await frame();
  const report = [];
  for (const sc of doc.scenes) {
    seed = 0x7c2a;
    fx.reset(); setWind(Math.atan2(0.25, 0.55), (Math.hypot(0.55, 0.25) - 0.25) / 0.6);
    for (const v of units.values()) scene.remove(v.root);
    units.clear();
    for (const row of sc.units) { const v = makeUnit(row); units.set(v.id, v); }
    const killed = new Set(), steps = timeline(sc);
    const rows = () => [...units.values()].map((v) => [v.id, v.type, v.owner, v.x, v.z, v.rot, v.rot, 100, 0, 0, 0, 0, 0, 0, 1]);
    let next = 0, shot = 0, clock = 0;
    const dt = 1 / 30;
    place(sc.pictures[0].focus);
    for (; shot < sc.pictures.length; clock += dt) {
      while (next < steps.length && steps[next][0] <= clock + 1e-9) {
        const m = steps[next++][1];
        for (const sh of m.shots) if (sh.kill && sh.t !== undefined && units.has(sh.t)) killed.add(sh.t);
        // main.js: fires as cell centres with the cell's character
        const fires = (m.fires ?? []).map((c) => [(c % MW + 0.5) * CELL, (Math.floor(c / MW) + 0.5) * CELL, grid[Math.floor(c / MW)][c % MW]]);
        for (const id of m.gone ?? []) {
          const v = units.get(id);
          if (!v) continue;
          // main.js removeUnit: a killed vehicle or structure leaves a wreck
          if (killed.has(id) && !UNITS[v.type].infantry && !UNITS[v.type].air) fx.wreck(v);
          scene.remove(v.root); units.delete(id);
        }
        fx.snapshot({ shots: m.shots, smokes: m.smokes ?? [], fires, units: rows(), cells: [] }, new Set(units.keys()));
      }
      fx.update(dt);
      const p = sc.pictures[shot];
      if (p.at > clock + 1e-6) continue;
      place(p.focus);
      fx.update(0);
      renderer.shadowMap.needsUpdate = true;
      renderer.render(scene, camera);
      await frame();
      renderer.render(scene, camera);
      const blob = await (await fetch(renderer.domElement.toDataURL('image/png'))).blob();
      await post(`/save?name=${encodeURIComponent(`${sc.name}-${p.moment}-reference.png`)}`, blob);
      report.push({ scene: sc.name, moment: p.moment, clock, particles: fx.count, shown: fx.shown });
      shot++;
    }
  }
  await post('/save?name=reference-scenes.json', JSON.stringify({ reference: 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d', shots: report }, null, 1));
  await post('/done', 'ok');
}
main().catch((e) => post('/log', `failed: ${e.stack ?? e}`).then(() => post('/done', 'failed')));
