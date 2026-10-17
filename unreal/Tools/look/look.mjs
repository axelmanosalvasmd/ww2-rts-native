// The reference half of the light and color comparison (README.md). It builds units exactly as the model comparison
// does (unit-models.js buildModel and animate, planes through aircraft.js, the viewer's grass and 2 m grid) under the
// game's own light: light.js setupLight with the default mood, the PMREM outdoor environment, ACES filmic tone mapping
// and the mood's exposure. On top of the model comparison's four views it shoots, per asset:
//   - a silhouette mask of each view (every visible part of the unit white on black, ground hidden), so color can be
//     compared on the unit's own pixels in both renderers;
//   - a plate of each view (the same camera with the unit hidden): the ground and its light alone;
//   - with ':man' (type:fac:owner:man), one soldier: alive = 1, as main.js shows a squad at one man's hit points, and
//     close views of that man (man-front, man-threequarter, man-side) instead of the four.
// Two special assets: 'calib', a grid of flat MeshBasicMaterial patches of known linear color (the tone curve and
// output encoding), and 'horizon', a low view over the ground with the game's distance haze (light.js followView at
// the default 40 m camera distance) and the haze as background (fog and sky).
// It also writes three's own prefiltered environment (the PMREM atlas of light.js outdoorEnvironment, as float32) so
// the Unreal materials sample the very texels three samples.
//
// 'rings-<type>:fac:owner': the unit's owner ring and its selection ring alone (markers.js ownerRing and
// selectionRing, radius + 0.4 and + 1, client/overlay.js on the GPU), no unit, shot from a close, the gameplay and a far
// camera through the game's own graphics High pipeline (light.js postFor: an EffectComposer with GTAO and OutputPass,
// so the overlay is tone mapped with the frame) as <name>-<view>-reference.png, and straight to the canvas as graphics
// Low draws it (toneMapped: false keeps the overlay out of the tone curve) as <name>-<view>-low-reference.png, with the
// rings hidden as the plate.
//
// URL: look.html?assets=medium:0:0,rifle:1:1:man,calib,horizon,rings-rifle:0:0
import * as THREE from 'three';
import { UNITS, SUPPORT } from '/shared/sim.js';
import { createAviation } from '/client/aircraft.js';
import { buildModel, animate, setSurfaces, setBuildings } from '/client/unit-models.js';
import { loadModelTextures } from '/client/model-textures.js';
import { ownerRing, selectionRing } from '/client/markers.js';
import { setupLight, sky, outdoorEnvironment } from '/client/light.js';
import { MOODS, DEFAULT_MOOD, TONE } from '/client/moods.js';
import { surface } from '/client/surfaces.js';
import { buildingModel } from '/client/structures.js';

const W = 1280, H = 720;
// client/main.js FACTIONS (uniform and vehicle paint) and COLORS at the pin, as client/viewer.js copies them
const FACTIONS = [{ uniform: 0x6b7248, vehicle: 0x59623d }, { uniform: 0x5c6266, vehicle: 0x50565a }, { uniform: 0x7d7250, vehicle: 0x4e5a38 }, { uniform: 0x6f6448, vehicle: 0x565640 }];
const COLORS = [0x3b73d6, 0xcc3a2e, 0xece6d6, 0xe2832b, 0x9b5cd4, 0x35b6c0];
const GAME = { fov: 42, pitch: 0.95, dist: 40 };
const AIR_ALT = 20;
const SUN_SIDE = 60 * Math.PI / 180, GRASS = 0x6c6c45, GRID = 0x56623a, GRID_CENTRE = 0x4a5530; // viewer.js
const q = new URLSearchParams(location.search);
const assets = (q.get('assets') ?? 'medium:0:0,rifle:1:1:man,calib,horizon').split(',').map((s) => {
  const [type, fac, owner, man] = s.split(':');
  return { type, fac: +(fac ?? 0), owner: +(owner ?? 0), man: man === 'man' };
});

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
const HAZE = scene.background.clone();
const NO_FOG = { near: 1e4, far: 2e4 }; // the model views keep haze off the model (viewer.js close views)
const SUN_DIR = new THREE.Vector3();
{ const az = Math.PI - SUN_SIDE, up = sky.sunUp * Math.PI / 180; SUN_DIR.set(Math.sin(az) * Math.cos(up), Math.sin(up), Math.cos(az) * Math.cos(up)); }
const ground = new THREE.Mesh(new THREE.PlaneGeometry(600, 600), new THREE.MeshLambertMaterial({ color: GRASS }));
ground.rotation.x = -Math.PI / 2; ground.receiveShadow = true;
const grid = new THREE.GridHelper(48, 24, GRID_CENTRE, GRID);
grid.position.y = 0.02; grid.material.transparent = true; grid.material.opacity = 0.45;
scene.add(ground, grid);

// the rings of a unit alone (main.js makeUnit: ownerRing(def.radius + 0.4, color), selectionRing(def.radius + 1))
function makeRings(type, fac, owner) {
  const def = UNITS[type], root = new THREE.Group();
  const base = ownerRing(def.radius + 0.4, COLORS[owner]);
  const { sel } = selectionRing(def.radius + 1);
  sel.visible = true;
  root.add(base, sel);
  scene.add(root);
  return { id: 1, type, owner, root, models: [], rings: true };
}
// light.js postFor: graphics High draws the frame through this composer (4x multisampled half floats, GTAO radius 1.6
// mixed at 0.85 with see-through things left out of its depth, then OutputPass: tone mapping and sRGB for the whole frame)
let composer = null;
async function highComposer() {
  if (composer) return composer;
  const mods = await Promise.all(['EffectComposer', 'RenderPass', 'GTAOPass', 'OutputPass'].map((n) => import(`three/addons/postprocessing/${n}.js`)));
  const { EffectComposer, RenderPass, GTAOPass, OutputPass } = Object.assign({}, ...mods);
  composer = new EffectComposer(renderer, new THREE.WebGLRenderTarget(1, 1, { type: THREE.HalfFloatType, samples: 4 }));
  const ao = new GTAOPass(scene, cam, 1, 1);
  ao.updateGtaoMaterial({ radius: 1.6, distanceFallOff: 1, thickness: 1, samples: 12 });
  ao.blendIntensity = 0.85;
  const seeThrough = (m) => m.transparent || !m.depthWrite;
  ao._overrideVisibility = function () {
    const cache = this._visibilityCache;
    this.scene.traverseVisible((o) => {
      const m = o.material;
      if (o.isPoints || o.isLine || o.isLine2 || o.isSprite || (m && (Array.isArray(m) ? m.every(seeThrough) : seeThrough(m)))) { o.visible = false; cache.push(o); }
    });
  };
  composer.addPass(new RenderPass(scene, cam)); composer.addPass(ao); composer.addPass(new OutputPass());
  composer.setPixelRatio(1); composer.setSize(W, H);
  return composer;
}
function shootHigh(view) {
  cam.fov = view.fov; cam.aspect = W / H; cam.up.set(0, 1, 0); cam.near = 0.05; cam.far = 3000;
  cam.position.fromArray(view.pos); cam.lookAt(new THREE.Vector3().fromArray(view.target)); cam.updateProjectionMatrix(); cam.updateMatrixWorld();
  scene.fog.near = view.fog.near; scene.fog.far = view.fog.far;
  aimShadow(view);
  renderer.shadowMap.needsUpdate = true;
  composer.render(1 / 60);
  return renderer.domElement.toDataURL('image/png');
}
function ringView(label, dist) {
  const hd = dist * Math.cos(GAME.pitch), h = dist * Math.sin(GAME.pitch);
  return { label, pos: [0, h, hd], target: [0, 0, 0], fov: GAME.fov, rad: Math.max(20, dist), fog: NO_FOG };
}

function makeUnit(type, fac, owner) {
  const def = UNITS[type], root = new THREE.Group();
  const f = { ...FACTIONS[fac], color: COLORS[owner] };
  const v = { id: 1, type, owner, root, models: [], alive: def.models, x: 0, z: 0, rot: 0, aim: 0, turret: null };
  const base = ownerRing(def.radius + 0.4, f.color);
  root.add(base); v.base = base;
  const aviation = createAviation({ hAt: () => 0, UNITS, SUPPORT, units: new Map(), altitude: AIR_ALT, world: () => scene, facOf: () => fac,
    colorOf: () => f.color, vehicleOf: () => f.vehicle, me: () => fac, explode() {}, play() {}, flakTypes: new Set(['flak', 'flaktrack', 'flakpos']) });
  if (type === 'airfield') { aviation.buildAirfield(v, root, owner); scene.add(root); return v; }
  if (def.air) {
    aviation.buildUnit(v, root, type, owner);
    base.visible = false; root.position.y = AIR_ALT;
    if (v.shadow) { v.shadow.visible = true; v.shadow.position.set(0, 0.3, 0); }
    scene.add(root);
    return v;
  }
  buildModel(v, root, f, fac, def);
  v.supp = 0; v.flags = 0; v.built = 1;
  if (v.turret) v.turret.rotation.y = 0;
  animate(v, 10, new THREE.Vector3(0, 30, 30));
  scene.add(root);
  return v;
}
// main.js applySnapshot at one man's hit points: alive = 1, men past it hidden (the first man stays)
function oneMan(v) {
  if (UNITS[v.type].air || !v.models.length || v.models[0] === v.root) return false;
  v.alive = 1;
  v.models.forEach((m, i) => { m.visible = i < 1; });
  animate(v, 10, new THREE.Vector3(0, 30, 30));
  v.models.forEach((m, i) => { m.visible = i < 1; });
  return true;
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
function closeView(label, box, dir, margin = 1.14) {
  const c = box.getCenter(new THREE.Vector3()), d = new THREE.Vector3(...dir).normalize();
  cam.fov = 30; cam.aspect = W / H; cam.up.set(0, 1, 0);
  cam.position.copy(c).add(d); cam.lookAt(c); cam.updateMatrixWorld();
  const r = new THREE.Vector3().setFromMatrixColumn(cam.matrixWorld, 0), u = new THREE.Vector3().setFromMatrixColumn(cam.matrixWorld, 1);
  const ty = Math.tan(cam.fov * Math.PI / 360), tx = ty * cam.aspect;
  let dist = 0;
  for (let i = 0; i < 8; i++) {
    const p = new THREE.Vector3(i & 1 ? box.max.x : box.min.x, i & 2 ? box.max.y : box.min.y, i & 4 ? box.max.z : box.min.z).sub(c), z = p.dot(d);
    dist = Math.max(dist, Math.abs(p.dot(r)) * margin / tx + z, Math.abs(p.dot(u)) * margin / ty + z);
  }
  const pos = c.clone().addScaledVector(d, dist);
  return { label, pos: pos.toArray(), target: c.toArray(), fov: 30, rad: box.getSize(new THREE.Vector3()).length() / 2 + 1, fog: NO_FOG };
}
function gameView(alt = 0) {
  const hd = GAME.dist * Math.cos(GAME.pitch), h = GAME.dist * Math.sin(GAME.pitch), tz = -hd * alt / h;
  return { label: 'gameplay', pos: [0, h, tz + hd], target: [0, 0, tz], fov: GAME.fov, rad: 40, fog: NO_FOG };
}
// light.js followView: haze from 0.6 to 3.8 camera distances (the mood's hazeK = 1)
const gameFog = (dist) => ({ near: dist * 0.6 / sky.haze, far: dist * 3.8 / sky.haze });
function aimShadow(view) {
  const c = new THREE.Vector3().fromArray(view.target), rad = view.rad;
  sun.target.position.copy(c); sun.position.copy(c).addScaledVector(SUN_DIR, rad + 120); sun.target.updateMatrixWorld();
  Object.assign(sun.shadow.camera, { left: -rad, right: rad, top: rad, bottom: -rad, near: 1, far: 2 * rad + 240 });
  sun.shadow.camera.updateProjectionMatrix();
  sun.shadow.normalBias = (2 * rad) / sun.shadow.mapSize.x * 0.8;
}
function shoot(view) {
  cam.fov = view.fov; cam.aspect = W / H; cam.up.set(0, 1, 0); cam.near = 0.05; cam.far = 3000;
  cam.position.fromArray(view.pos); cam.lookAt(new THREE.Vector3().fromArray(view.target)); cam.updateProjectionMatrix(); cam.updateMatrixWorld();
  scene.fog.near = view.fog.near; scene.fog.far = view.fog.far;
  aimShadow(view);
  renderer.shadowMap.needsUpdate = true;
  renderer.info.reset();
  renderer.render(scene, cam);
  return renderer.domElement.toDataURL('image/png');
}
// the silhouette: every visible part of the unit white, ground and grid hidden, black behind
const WHITE = new THREE.MeshBasicMaterial({ color: 0xffffff, fog: false, toneMapped: false, side: THREE.DoubleSide });
function shootMask(view, v) {
  const shown = [ground.visible, grid.visible, v.shadow?.visible];
  ground.visible = grid.visible = false; if (v.shadow) v.shadow.visible = false;
  const bg = scene.background; scene.background = new THREE.Color(0, 0, 0);
  scene.overrideMaterial = WHITE;
  const url = shoot(view);
  scene.overrideMaterial = null; scene.background = bg;
  [ground.visible, grid.visible] = shown; if (v.shadow) v.shadow.visible = shown[2];
  return url;
}
const save = async (name, url) => post(`/save?name=${encodeURIComponent(name)}`, await (await fetch(url)).blob());
const frame = () => new Promise((r) => requestAnimationFrame(() => r()));

// three's PMREM atlas of the outdoor environment, read back from its own render target (half floats, as three keeps it)
function pmremTexels() {
  const tex = outdoorEnvironment(renderer), target = tex.renderTarget, w = tex.image.width, h = tex.image.height;
  if (!target || tex.type !== THREE.HalfFloatType) throw new Error(`the PMREM atlas is not a half float render target (type ${tex.type})`);
  const half = new Uint16Array(w * h * 4);
  renderer.readRenderTargetPixels(target, 0, 0, w, h, half);
  const px = Float32Array.from(half, THREE.DataUtils.fromHalfFloat);
  let peak = 0;
  for (let i = 0; i < px.length; i++) if (i % 4 !== 3) peak = Math.max(peak, px[i]);
  post('/log', `pmrem ${w}x${h} read back from its render target, peak ${peak}`);
  if (!(peak > 0)) throw new Error('the PMREM atlas read back empty');
  const maxMip = Math.log2(h) - 2; // WebGLProgram.js generateCubeUVSize
  return { px, meta: { width: w, height: h, maxMip, texelWidth: 1 / (3 * Math.max(2 ** maxMip, 7 * 16)), texelHeight: 1 / h, rowOrder: 'row 0 is texture v = 0 (GL)', type: tex.type, mapping: tex.mapping, peak } };
}

// 'calib': flat unlit patches (MeshBasicMaterial, tone mapped like every game material) facing +x, 2 m apart
const CALIB = [0, 0.01, 0.02, 0.05, 0.1, 0.18, 0.3, 0.5, 0.75, 1, 1.5, 2.5, 4, 8].map((g) => [g, g, g])
  .concat([[1, 0.5, 0.2], [0.2, 0.5, 1], [0.1, 0.3, 0.05], [0.6, 0.05, 0.05], [0.05, 0.25, 0.6], [0.4, 0.4, 0.1]]);
function buildCalib() {
  const root = new THREE.Group(), patches = [];
  CALIB.forEach((c, i) => {
    const col = i % 5, row = Math.floor(i / 5);
    const m = new THREE.Mesh(new THREE.PlaneGeometry(1.6, 1.6), new THREE.MeshBasicMaterial({ color: new THREE.Color().setRGB(c[0], c[1], c[2], THREE.LinearSRGBColorSpace), fog: true }));
    m.rotation.y = Math.PI / 2; // facing +x
    m.position.set(0, 7 - row * 2, (col - 2) * 2);
    root.add(m); patches.push({ linear: c, centre: m.position.toArray(), size: 1.6 });
  });
  root.position.y = 0; scene.add(root);
  return { root, patches };
}
const project = (p) => { const v = new THREE.Vector3(...p).project(cam); return [(v.x + 1) / 2 * W, (1 - v.y) / 2 * H]; };

async function main() {
  const env = pmremTexels();
  await post('/save?name=env-pmrem.f32', env.px.buffer);
  const out = { reference: 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d', three: THREE.REVISION, width: W, height: H,
    light: { mood: DEFAULT_MOOD, row: mood, tone: TONE, exposure: renderer.toneMappingExposure, toneMapping: renderer.toneMapping, outputColorSpace: renderer.outputColorSpace,
      sunDir: SUN_DIR.toArray(), sun: mood.sun, sunI: sun.intensity, shadowIntensity: sun.shadow.intensity, hemiTop: mood.top, hemiBottom: mood.bottom, hemiI: mood.hemiI,
      environmentIntensity: scene.environmentIntensity, environmentRotationY: scene.environmentRotation.y, haze: mood.haze, hazeLinear: HAZE.toArray(), ground: GRASS, grid: GRID, gridCentre: GRID_CENTRE,
      // the model comparison's stand-ins, kept so TCModelCompare renders the same "before" from this file
      sunLux: 2.0, skyIntensity: 5.0, exposureBias: 0.4 },
    env: env.meta, assets: [] };
  const built = [];
  for (const a of assets) {
    if (a.type === 'calib') { built.push({ ...a, calib: buildCalib() }); continue; }
    if (a.type === 'horizon') { built.push({ ...a }); continue; }
    if (a.type.startsWith('rings-')) { built.push({ ...a, v: makeRings(a.type.slice(6), a.fac, a.owner) }); continue; }
    const v = makeUnit(a.type, a.fac, a.owner);
    if (a.man && !oneMan(v)) { post('/log', `${a.type}: no soldiers to isolate`); }
    built.push({ ...a, v });
  }
  for (let i = 0; i < 400 && pending > 0; i++) await frame();
  for (let i = 0; i < 5; i++) await frame();
  const roots = built.flatMap((b) => b.v ? [b.v.root, b.v.shadow].filter(Boolean) : b.calib ? [b.calib.root] : []);
  for (const a of built) {
    for (const r of roots) r.visible = false;
    let views, name;
    const entry = { type: a.type, fac: a.fac, owner: a.owner };
    if (a.calib) {
      a.calib.root.visible = true;
      ground.visible = grid.visible = false;
      name = 'calib';
      views = [{ label: 'patches', pos: [30, 3, 0], target: [0, 3, 0], fov: 30, rad: 20, fog: NO_FOG }];
      shoot(views[0]);
      entry.patches = a.calib.patches.map((p) => ({ ...p, pixel: project(p.centre) }));
    } else if (a.type === 'horizon') {
      name = 'horizon';
      views = [{ label: 'fog', pos: [0, 6, 60], target: [0, 2, -40], fov: 42, rad: 60, fog: gameFog(GAME.dist) }];
    } else if (a.v.rings) {
      a.v.root.visible = true;
      name = `${a.type}-f${a.fac}-s${a.owner}`;
      views = [ringView('ring-close', 14), ringView('ring-gameplay', GAME.dist), ringView('ring-far', 120)];
      await highComposer();
      for (const view of views) {
        shootHigh(view); await frame();
        await save(`${name}-${view.label}-reference.png`, shootHigh(view));
        shoot(view); await frame();
        await save(`${name}-${view.label}-low-reference.png`, shoot(view));
        a.v.root.visible = false;
        shootHigh(view); await frame();
        await save(`${name}-${view.label}-plate-reference.png`, shootHigh(view));
        a.v.root.visible = true;
      }
      out.assets.push({ name, ...entry, rings: true, views: views.map(({ rad, ...v }) => v) });
      continue;
    } else {
      const v = a.v;
      v.root.visible = true; if (v.shadow) v.shadow.visible = true;
      name = `${a.type}-f${a.fac}-s${a.owner}${a.man ? '-man' : ''}`;
      if (a.man) {
        const box = bounds(v.models[0]);
        views = [closeView('man-front', box, [1, 0.1, 0], 1.25), closeView('man-threequarter', box, [1, 0.45, -0.8], 1.25), closeView('man-side', box, [0, 0.1, -1], 1.25)];
        entry.man = 0; entry.alive = 1;
      } else {
        const box = bounds(v.root);
        views = [closeView('front', box, [1, 0.12, 0]), closeView('side', box, [0, 0.12, -1]), closeView('threequarter', box, [1, 0.7, -1]), gameView(UNITS[a.type].air ? AIR_ALT : 0)];
      }
      entry.box = (() => { const b = bounds(a.man ? v.models[0] : v.root); return { min: b.min.toArray(), max: b.max.toArray() }; })();
    }
    for (const view of views) {
      shoot(view); await frame();
      await save(`${name}-${view.label}-reference.png`, shoot(view));
      if (a.v) {
        await save(`${name}-${view.label}-mask-reference.png`, shootMask(view, a.v));
        a.v.root.visible = false; if (a.v.shadow) a.v.shadow.visible = false;
        await save(`${name}-${view.label}-plate-reference.png`, shoot(view));
        a.v.root.visible = true; if (a.v.shadow) a.v.shadow.visible = true;
      }
    }
    ground.visible = grid.visible = true;
    out.assets.push({ name, ...entry, views: views.map(({ rad, ...v }) => v) });
  }
  await post('/save?name=cameras.json', JSON.stringify(out, null, 1));
  await post('/done', 'ok');
}
main().catch((e) => post('/log', `failed: ${e.stack ?? e}`).then(() => post('/done', 'failed')));
