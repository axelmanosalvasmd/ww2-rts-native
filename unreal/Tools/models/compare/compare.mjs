// Builds units with the pinned reference's own model code (client/unit-models.js buildModel and animate, as
// client/viewer.js does), lights them with client/light.js (the default mood), stands them on the viewer's grass and
// 2 m grid, and shoots fixed views. The cameras, light and ground go to cameras.json so the Unreal half
// (Client/TCModelCompare.h, -run=TCModelCompare) renders the same units from the same places.
//
// URL: compare.html?assets=medium:0:0,at:0:0,... (type:faction:owner slot). Default: the pilot set.
// Views per asset, reference metres, unit at the origin facing +x (rot 0, aim 0), standing:
//   front        camera on (1, 0.12, 0) from the model's box centre (client/viewer.js CLOSE 'front')
//   side         camera on (0, 0.12, -1) (the unit's left side; viewer.js 'left side')
//   threequarter camera on (1, 0.7, -1) (viewer.js 'three-quarter front', from above)
//   gameplay     client/camera.js defaults: fov 42, pitch 0.95, distance 40, yaw 0 (viewer.js GAME), looking at the unit
// Close views: vertical fov 30, distance fitted to the box as viewer.js fit() does (margin 1.14), without its crop.
import * as THREE from 'three';
import { UNITS, SUPPORT } from '/shared/sim.js';
import { createAviation } from '/client/aircraft.js';
import { buildModel, animate, setSurfaces, setBuildings } from '/client/unit-models.js';
import { loadModelTextures } from '/client/model-textures.js';
import { ownerRing } from '/client/markers.js';
import { setupLight, sky } from '/client/light.js';
import { MOODS, DEFAULT_MOOD } from '/client/moods.js';
import { surface } from '/client/surfaces.js';
import { buildingModel } from '/client/structures.js';

const W = 1280, H = 720;
// client/main.js FACTIONS (uniform and vehicle paint) and COLORS at the pin, as client/viewer.js copies them
const FACTIONS = [{ uniform: 0x6b7248, vehicle: 0x59623d }, { uniform: 0x5c6266, vehicle: 0x50565a }, { uniform: 0x7d7250, vehicle: 0x4e5a38 }, { uniform: 0x6f6448, vehicle: 0x565640 }];
const COLORS = [0x3b73d6, 0xcc3a2e, 0xece6d6, 0xe2832b, 0x9b5cd4, 0x35b6c0];
const GAME = { fov: 42, pitch: 0.95, dist: 40 };
const AIR_ALT = 20; // main.js: planes fly this high; ATCUnitRenderer::AirAltitude
const SUN_SIDE = 60 * Math.PI / 180, GRASS = 0x6c6c45, GRID = 0x56623a; // viewer.js
const q = new URLSearchParams(location.search);
const assets = (q.get('assets') ?? 'medium:0:0,at:0:0,rifle:0:0,barracks:0:0').split(',').map((s) => { const [type, fac, owner] = s.split(':'); return { type, fac: +fac, owner: +owner }; });

const post = (url, body) => fetch(url, { method: 'POST', body });
window.addEventListener('error', (e) => post('/log', `error: ${e.message}`));
window.addEventListener('unhandledrejection', (e) => post('/log', `rejection: ${e.reason?.stack ?? e.reason}`));

setSurfaces(surface); setBuildings(buildingModel);
let pending = 0;
const manager = THREE.DefaultLoadingManager, begin = manager.itemStart, end = manager.itemEnd;
manager.itemStart = (u) => { pending++; begin(u); };
manager.itemEnd = (u) => { pending = Math.max(0, pending - 1); end(u); };
loadModelTextures();
const renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
renderer.localClippingEnabled = true;
renderer.setPixelRatio(1);
renderer.setSize(W, H);
document.body.append(renderer.domElement);
const scene = new THREE.Scene();
const cam = new THREE.PerspectiveCamera(30, W / H, 0.05, 3000);
const { sun } = setupLight(renderer, scene, cam);
const mood = MOODS[DEFAULT_MOOD];
scene.fog.near = 1e4; scene.fog.far = 2e4; // no haze over the model in any view (viewer.js close views)
const SUN_DIR = new THREE.Vector3();
{ const az = Math.PI - SUN_SIDE, up = sky.sunUp * Math.PI / 180; SUN_DIR.set(Math.sin(az) * Math.cos(up), Math.sin(up), Math.cos(az) * Math.cos(up)); }
const ground = new THREE.Mesh(new THREE.PlaneGeometry(600, 600), new THREE.MeshLambertMaterial({ color: GRASS }));
ground.rotation.x = -Math.PI / 2; ground.receiveShadow = true;
const grid = new THREE.GridHelper(48, 24, 0x4a5530, GRID);
grid.position.y = 0.02; grid.material.transparent = true; grid.material.opacity = 0.45;
scene.add(ground, grid);

function makeUnit(type, fac, owner) {
  const def = UNITS[type], root = new THREE.Group();
  const f = { ...FACTIONS[fac], color: COLORS[owner] }; // main.js look(slot)
  const v = { id: 1, type, owner, root, models: [], alive: def.models, x: 0, z: 0, rot: 0, aim: 0, turret: null };
  const base = ownerRing(def.radius + 0.4, f.color);
  root.add(base); v.base = base;
  // main.js makeUnit: planes through client/aircraft.js (the game hides their ring and flies them AIR_ALT up)
  const aviation = createAviation({ hAt: () => 0, UNITS, SUPPORT, units: new Map(), altitude: AIR_ALT, world: () => scene, facOf: () => fac,
    colorOf: () => f.color, vehicleOf: () => f.vehicle, me: () => fac, explode() {}, play() {}, flakTypes: new Set(['flak', 'flaktrack', 'flakpos']) });
  if (type === 'airfield') { aviation.buildAirfield(v, root, owner); scene.add(root); return v; } // main.js makeUnit
  if (def.air) {
    aviation.buildUnit(v, root, type, owner);
    base.visible = false; root.position.y = AIR_ALT;
    if (v.shadow) { v.shadow.visible = true; v.shadow.position.set(0, 0.3, 0); }
    scene.add(root);
    return v;
  }
  buildModel(v, root, f, fac, def);
  v.supp = 0; v.flags = 0; v.built = 1;
  if (v.turret) v.turret.rotation.y = 0; // aim = rot
  animate(v, 10, new THREE.Vector3(0, 30, 30)); // viewer.js pose(): a long dt snaps the posture, nearEye keeps the near model
  scene.add(root);
  return v;
}
function bounds(root) {
  root.updateMatrixWorld(true);
  const box = new THREE.Box3(), p = new THREE.Vector3();
  root.traverseVisible((o) => {
    if (!o.isMesh || o.geometry?.type === 'BufferGeometry' && !o.geometry.getAttribute('position')) return;
    const pos = o.geometry.getAttribute('position');
    if (!pos || o.material?.isShaderMaterial) return;
    for (let i = 0; i < pos.count; i++) box.expandByPoint(o.getVertexPosition(i, p).applyMatrix4(o.matrixWorld));
  });
  return box;
}
function closeView(label, box, dir) {
  const c = box.getCenter(new THREE.Vector3()), d = new THREE.Vector3(...dir).normalize();
  cam.fov = 30; cam.aspect = W / H; cam.up.set(0, 1, 0);
  cam.position.copy(c).add(d); cam.lookAt(c); cam.updateMatrixWorld();
  const r = new THREE.Vector3().setFromMatrixColumn(cam.matrixWorld, 0), u = new THREE.Vector3().setFromMatrixColumn(cam.matrixWorld, 1);
  const ty = Math.tan(cam.fov * Math.PI / 360), tx = ty * cam.aspect, margin = 1.14;
  let dist = 0;
  for (let i = 0; i < 8; i++) {
    const p = new THREE.Vector3(i & 1 ? box.max.x : box.min.x, i & 2 ? box.max.y : box.min.y, i & 4 ? box.max.z : box.min.z).sub(c), z = p.dot(d);
    dist = Math.max(dist, Math.abs(p.dot(r)) * margin / tx + z, Math.abs(p.dot(u)) * margin / ty + z);
  }
  const pos = c.clone().addScaledVector(d, dist);
  return { label, pos: pos.toArray(), target: c.toArray(), fov: 30, rad: box.getSize(new THREE.Vector3()).length() / 2 + 1 };
}
function gameView(alt = 0) {
  // viewer.js gameView: alt lifts the point it centres on, so a plane AIR_ALT up sits mid-frame
  const hd = GAME.dist * Math.cos(GAME.pitch), h = GAME.dist * Math.sin(GAME.pitch), tz = -hd * alt / h;
  return { label: 'gameplay', pos: [0, h, tz + hd], target: [0, 0, tz], fov: GAME.fov, rad: 40 };
}
function shoot(view) {
  cam.fov = view.fov; cam.aspect = W / H; cam.up.set(0, 1, 0); cam.near = 0.05; cam.far = 3000;
  cam.position.fromArray(view.pos); cam.lookAt(new THREE.Vector3().fromArray(view.target)); cam.updateProjectionMatrix(); cam.updateMatrixWorld();
  // viewer.js sunOn / aimSunBox: a shadow box around what the view shows
  const c = new THREE.Vector3().fromArray(view.target), rad = view.rad;
  sun.target.position.copy(c); sun.position.copy(c).addScaledVector(SUN_DIR, rad + 120); sun.target.updateMatrixWorld();
  Object.assign(sun.shadow.camera, { left: -rad, right: rad, top: rad, bottom: -rad, near: 1, far: 2 * rad + 240 });
  sun.shadow.camera.updateProjectionMatrix();
  sun.shadow.normalBias = (2 * rad) / sun.shadow.mapSize.x * 0.8;
  renderer.shadowMap.needsUpdate = true;
  renderer.info.reset();
  renderer.render(scene, cam);
  return renderer.domElement.toDataURL('image/png');
}
const frame = () => new Promise((r) => requestAnimationFrame(() => r()));
async function main() {
  const out = { reference: 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d', width: W, height: H,
    light: { sunDir: SUN_DIR.toArray(), sun: mood.sun, sunI: mood.sunI, hemiTop: mood.top, hemiBottom: mood.bottom, hemiI: mood.hemiI, haze: mood.haze, exposure: mood.exposure, ground: GRASS, grid: GRID,
      // the Unreal half's stand-ins for three's lights (README.md: not a calibrated match)
      sunLux: 2.0, skyIntensity: 5.0, exposureBias: 0.4 },
    assets: [] };
  const built = assets.map((a) => ({ ...a, v: makeUnit(a.type, a.fac, a.owner) }));
  for (let i = 0; i < 400 && pending > 0; i++) await frame(); // textures (model layers, infantry atlas, surfaces)
  for (let i = 0; i < 5; i++) await frame();
  for (const a of built) {
    for (const b of built) { b.v.root.visible = b === a; if (b.v.shadow) b.v.shadow.visible = b === a; }
    const box = bounds(a.v.root);
    const views = [closeView('front', box, [1, 0.12, 0]), closeView('side', box, [0, 0.12, -1]), closeView('threequarter', box, [1, 0.7, -1]), gameView(UNITS[a.type].air ? AIR_ALT : 0)];
    const name = `${a.type}-f${a.fac}-s${a.owner}`;
    for (const view of views) {
      shoot(view); await frame();
      await post(`/save?name=${encodeURIComponent(`${name}-${view.label}-reference.png`)}`, await (await fetch(shoot(view))).blob());
    }
    out.assets.push({ name, type: a.type, fac: a.fac, owner: a.owner, box: { min: box.min.toArray(), max: box.max.toArray() }, views: views.map(({ rad, ...v }) => v) });
  }
  await post('/save?name=cameras.json', JSON.stringify(out, null, 1));
  await post('/done', 'ok');
}
main().catch((e) => post('/log', `failed: ${e.stack ?? e}`).then(() => post('/done', 'failed')));
