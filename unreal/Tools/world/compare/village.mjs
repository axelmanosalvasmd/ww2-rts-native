// The pinned reference's own map pieces (client/structures.js buildStructures) and scenery (client/props.js
// createProps) on a start map (shared/world-layers.js mapForClient), lit by client/light.js in the default mood, seen
// from client/camera.js's camera (fov 42, pitch 0.95, cam.yaw, cam.dist around a focus): the same view the native
// client's verification switches take. Ground: the viewer's flat grass (client/viewer.js GRASS), not the painted ground;
// no fog of war. URL: village.html?map=default.json&views=x,z,dist,yaw;x,z,dist,yaw (reference metres, radians).
import * as THREE from 'three';
import { mapForClient } from '/shared/world-layers.js';
import { buildStructures } from '/client/structures.js';
import { createProps } from '/client/props.js';
import { setupLight, sky } from '/client/light.js';
import { MOODS, DEFAULT_MOOD } from '/client/moods.js';

const W = 1280, H = 720, PITCH = 0.95, FOV = 42, GRASS = 0x6c6c45, SUN_SIDE = 60 * Math.PI / 180;
const q = new URLSearchParams(location.search);
const mapFile = q.get('map') ?? 'default.json';
const views = (q.get('views') ?? '67,119,45,0').split(';').map((s) => s.split(',').map(Number));
const post = (url, body) => fetch(url, { method: 'POST', body });
window.addEventListener('error', (e) => post('/log', `error: ${e.message}`));
window.addEventListener('unhandledrejection', (e) => post('/log', `rejection: ${e.reason?.stack ?? e.reason}`));
let pending = 0;
const manager = THREE.DefaultLoadingManager, begin = manager.itemStart, end = manager.itemEnd;
manager.itemStart = (u) => { pending++; begin(u); };
manager.itemEnd = (u) => { pending = Math.max(0, pending - 1); end(u); };
const renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
renderer.setPixelRatio(1);
renderer.setSize(W, H);
document.body.append(renderer.domElement);
const scene = new THREE.Scene();
const cam = new THREE.PerspectiveCamera(FOV, W / H, 0.5, 3000);
const { sun } = setupLight(renderer, scene, cam);
const mood = MOODS[DEFAULT_MOOD];
const SUN_DIR = new THREE.Vector3();
{ const az = Math.PI - SUN_SIDE, up = sky.sunUp * Math.PI / 180; SUN_DIR.set(Math.sin(az) * Math.cos(up), Math.sin(up), Math.cos(az) * Math.cos(up)); }
const frame = () => new Promise((r) => requestAnimationFrame(() => r()));

async function main() {
  const map = mapForClient(await (await fetch(`/maps/${mapFile}`)).json());
  const hAt = () => 0;
  const ground = new THREE.Mesh(new THREE.PlaneGeometry(map.w * 2 + 400, map.h * 2 + 400), new THREE.MeshLambertMaterial({ color: GRASS }));
  ground.rotation.x = -Math.PI / 2; ground.position.set(map.w, 0, map.h); ground.receiveShadow = true;
  scene.add(ground);
  const pieces = new THREE.Group();
  scene.add(pieces);
  buildStructures(pieces, map.rows.map((r) => [...r]), map.rows, hAt, null, map.buildings ?? [], null);
  createProps({ map, grid: map.rows.map((r) => [...r]), hAt, parent: scene });
  for (let i = 0; i < 400 && pending > 0; i++) await frame();
  for (let i = 0; i < 5; i++) await frame();
  const out = { reference: 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d', map: mapFile, width: W, height: H, fov: FOV, pitch: PITCH, mood: DEFAULT_MOOD, views: [] };
  for (const [x, z, dist, yaw] of views) {
    // client/camera.js: camera.position = focus + (sin yaw, ., cos yaw) * horizontal distance, up dist * sin pitch
    const hd = dist * Math.cos(PITCH);
    cam.position.set(x + Math.sin(yaw) * hd, dist * Math.sin(PITCH), z + Math.cos(yaw) * hd);
    cam.lookAt(x, 0, z); cam.updateProjectionMatrix(); cam.updateMatrixWorld();
    const c = new THREE.Vector3(x, 0, z), rad = dist * 1.2;
    sun.target.position.copy(c); sun.position.copy(c).addScaledVector(SUN_DIR, rad + 120); sun.target.updateMatrixWorld();
    Object.assign(sun.shadow.camera, { left: -rad, right: rad, top: rad, bottom: -rad, near: 1, far: 2 * rad + 240 });
    sun.shadow.camera.updateProjectionMatrix();
    renderer.shadowMap.needsUpdate = true;
    renderer.render(scene, cam);
    await frame();
    renderer.render(scene, cam);
    const name = `${mapFile.replace('.json', '')}-${x}-${z}-${dist}-reference.png`;
    await post(`/save?name=${encodeURIComponent(name)}`, await (await fetch(renderer.domElement.toDataURL('image/png'))).blob());
    out.views.push({ name, focus: [x, z], dist, yaw, position: cam.position.toArray() });
  }
  let n = 0; scene.traverse((o) => { if (o.isInstancedMesh) n += o.count; });
  out.instances = n;
  await post('/save?name=views.json', JSON.stringify(out, null, 1));
  await post('/done', 'ok');
}
main().catch((e) => post('/log', `failed: ${e.stack ?? e}`).then(() => post('/done', 'failed')));
