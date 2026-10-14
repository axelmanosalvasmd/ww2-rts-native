// The reference half of the relief side by side: the pinned client/relief.js createRelief with client/relief-material.js
// on an authored map, under client/light.js's default mood, from client/camera.js cameras (fov 42, pitch 0.95, focus
// on the ground), the same views -run=TCReliefShot renders natively. The paint under the relief is a one-texel-per-cell
// canvas in the minimap colours (client/main.js MM_COLORS, K as B, Q as R), the colours the native relief carries until
// the painted ground (client/ground.js) is wired: both sides compare the relief, not the paint. Roads: ground.js
// cellAttrs prim ROAD, as main.js passes isRoad. The sun is aimed as village.mjs aims it (yaw 0).
import * as THREE from 'three';
import { mapForClient, worldLayers, composeWorldCell } from '/shared/world-layers.js';
import { createRelief } from '/client/relief.js';
import { internals, fieldCells } from '/client/ground.js';
import { setupLight, sky } from '/client/light.js';
import { DEFAULT_MOOD } from '/client/moods.js';

const W = 1280, H = 720, PITCH = 0.95, FOV = 42, SUN_SIDE = 60 * Math.PI / 180;
const q = new URLSearchParams(location.search);
const mapFile = q.get('map') ?? 'default.json', label = q.get('name') ?? mapFile.replace('.json', '');
const views = (q.get('views') ?? '80,80,180,0').split(';').map((s) => s.split(',').map(Number));
const post = (url, body) => fetch(url, { method: 'POST', body });
window.addEventListener('error', (e) => post('/log', `error: ${e.message}`));
window.addEventListener('unhandledrejection', (e) => post('/log', `rejection: ${e.reason?.stack ?? e.reason}`));
const COLORS = { B: [150, 132, 100], K: [150, 132, 100], H: [47, 74, 34], '#': [154, 149, 138], '+': [90, 79, 54], T: [62, 50, 34], W: [60, 93, 112], '=': [122, 90, 58],
  F: [106, 127, 122], R: [122, 114, 102], Q: [122, 114, 102], X: [96, 90, 70], Y: [84, 84, 78], D: [150, 128, 100], M: [92, 80, 58], N: [120, 96, 60], O: [38, 62, 30], A: [226, 220, 204] };
const renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
renderer.setPixelRatio(1);
renderer.setSize(W, H);
document.body.append(renderer.domElement);
const scene = new THREE.Scene();
const cam = new THREE.PerspectiveCamera(FOV, W / H, 0.5, 3000);
const { sun } = setupLight(renderer, scene, cam);
const SUN_DIR = new THREE.Vector3();
{ const az = Math.PI - SUN_SIDE, up = sky.sunUp * Math.PI / 180; SUN_DIR.set(Math.sin(az) * Math.cos(up), Math.sin(up), Math.cos(az) * Math.cos(up)); }
const frame = () => new Promise((r) => requestAnimationFrame(() => r()));

async function main() {
  const map = mapForClient(await (await fetch(`/maps/${mapFile}`)).json());
  const layers = worldLayers(map);
  const grid = Array.from({ length: map.h }, (_, y) => layers.ground.slice(y * map.w, (y + 1) * map.w).map((ch, x) => composeWorldCell(ch, layers.objects[y * map.w + x])));
  internals.buildNoise();
  const attrs = internals.cellAttrs({ w: map.w, h: map.h, map, fields: fieldCells(map), state: null, groundGrid: null, objectGrid: null }, grid);
  const canvas = document.createElement('canvas');
  canvas.width = map.w; canvas.height = map.h;
  const ctx = canvas.getContext('2d'), img = ctx.createImageData(map.w, map.h);
  for (let y = 0; y < map.h; y++) for (let x = 0; x < map.w; x++) {
    const c = COLORS[map.rows[y][x]] ?? [108, 118, 69], k = (y * map.w + x) * 4;
    img.data[k] = c[0]; img.data[k + 1] = c[1]; img.data[k + 2] = c[2]; img.data[k + 3] = 255;
  }
  ctx.putImageData(img, 0, 0);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace; texture.magFilter = THREE.NearestFilter; texture.minFilter = THREE.NearestFilter; texture.generateMipmaps = false;
  const relief = createRelief(map, grid, { texture, isRoad: (x, y) => x >= 0 && y >= 0 && x < map.w && y < map.h && attrs.prim[y * map.w + x] === 4, low: false });
  scene.add(relief.mesh);
  for (let i = 0; i < 5; i++) await frame();
  const out = { reference: 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d', map: mapFile, width: W, height: H, fov: FOV, pitch: PITCH, mood: DEFAULT_MOOD, stats: relief.stats, views: [] };
  for (const [x, z, dist, yaw] of views) {
    const g = relief.hAt(x, z), hd = dist * Math.cos(PITCH);
    cam.position.set(x + Math.sin(yaw) * hd, g + dist * Math.sin(PITCH), z + Math.cos(yaw) * hd);
    cam.lookAt(x, g, z); cam.updateProjectionMatrix(); cam.updateMatrixWorld();
    const c = new THREE.Vector3(x, g, z), rad = dist * 1.2;
    sun.target.position.copy(c); sun.position.copy(c).addScaledVector(SUN_DIR, rad + 120); sun.target.updateMatrixWorld();
    Object.assign(sun.shadow.camera, { left: -rad, right: rad, top: rad, bottom: -rad, near: 1, far: 2 * rad + 240 });
    sun.shadow.camera.updateProjectionMatrix();
    renderer.shadowMap.needsUpdate = true;
    renderer.render(scene, cam);
    await frame();
    renderer.render(scene, cam);
    const name = `${label}-${x}-${z}-${dist}-${yaw}-reference.png`;
    await post(`/save?name=${encodeURIComponent(name)}`, await (await fetch(renderer.domElement.toDataURL('image/png'))).blob());
    out.views.push({ name, focus: [x, g, z], dist, yaw, position: cam.position.toArray() });
  }
  await post(`/save?name=${encodeURIComponent(label)}-views.json`, JSON.stringify(out, null, 1));
  await post('/done', 'ok');
}
main().catch((e) => post('/log', `failed: ${e.stack ?? e}`).then(() => post('/done', 'failed')));
