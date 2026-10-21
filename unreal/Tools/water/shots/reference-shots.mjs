// The reference half of the water and apron side by sides: the pinned browser client itself (unmodified, served by the
// isolated reference server tc-local.sh started) in headless Chromium, hosting a match with one AI on a map in clear
// weather, posed at the same cameras as the native -TCWaterShots switch (Client/TCGameMode.cpp glue, see README), with
// everything but the WebGL canvas hidden, saved through the DevTools protocol. Nothing in the page is changed except
// window.__game's camera (cam.x, cam.z, cam.yaw, cam.dist: the same state the keyboard and wheel move; the distance is
// clamped to rig.wide, the widest view, as the wheel clamps it).
//
//   node unreal/Tools/water/shots/reference-shots.mjs --server http://127.0.0.1:<port> --map river-towns --out <dir>
//        [--room w13ref1] [--chromium /usr/bin/chromium] [--settle 6] [--poses all|name,name]
import fs from 'node:fs';
import path from 'node:path';
import { spawn, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const ref = path.resolve(here, '../../../../reference/ww2-rts');
const arg = (n, d) => { const i = process.argv.indexOf(`--${n}`); return i > 0 ? process.argv[i + 1] : d; };
const server = arg('server'), mapName = arg('map'), out = arg('out'), room = arg('room', 'w13ref' + Math.floor(Math.random() * 1e6));
const chromium = arg('chromium', '/usr/bin/chromium'), settle = +arg('settle', 6), only = arg('poses', 'all');
if (!server || !mapName || !out) { console.error('usage: reference-shots.mjs --server <http url> --map <name> --out <dir>'); process.exit(2); }
fs.mkdirSync(out, { recursive: true });
const sleep = (ms) => new Promise(r => setTimeout(r, ms));

// the same poses as the native switch: nearest known water cell to the centre (first in cell order), edges looking out,
// a corner, the widest zoom on three headings. Native yaw (degrees) -> browser yaw: b = -(yaw + 90) degrees.
const { mapForClient } = await import(path.join(ref, 'shared/world-layers.js'));
const map = mapForClient(JSON.parse(fs.readFileSync(path.join(ref, 'maps', mapName + '.json'), 'utf8')));
const Wm = map.w * 2, Hm = map.h * 2, chars = map.rows.join('');
let water = [Wm / 2, Hm / 2], best = Infinity;
for (let c = 0; c < map.w * map.h; c++) if ('WF='.includes(chars[c])) {
  const p = [(c % map.w + 0.5) * 2, (Math.floor(c / map.w) + 0.5) * 2], d = (p[0] - Wm / 2) ** 2 + (p[1] - Hm / 2) ** 2;
  if (d < best) { best = d; water = p; }
}
const poses = [['water-close', water, -90, 40], ['water-gameplay', water, -90, 85], ['water-wide', water, -60, 200],
  ['edge-north-looking-out', [Wm / 2, 6], -90, 85], ['edge-east-looking-out', [Wm - 6, Hm / 2], 0, 200], ['edge-south-looking-out', [Wm / 2, Hm - 6], 90, 140],
  ['corner-southwest-diagonal', [6, Hm - 6], 135, 160], ['widest-heading-north', [Wm / 2, Hm / 2], -90, 100000],
  ['widest-heading-northeast', [Wm / 2, Hm / 2], -45, 100000], ['widest-heading-west', [Wm / 2, Hm / 2], 180, 100000]]
  .filter(p => only === 'all' || only.split(',').includes(p[0]));

const profile = path.join(out, '.chromium-profile');
fs.rmSync(profile, { recursive: true, force: true });
const url = `${server}/play?name=Alder#${room}`;
const browser = spawn(chromium, ['--headless=new', `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check', '--disable-extensions',
  '--disable-background-networking', '--remote-debugging-port=0', '--window-size=1280,720', '--use-angle=swiftshader', '--enable-unsafe-swiftshader',
  '--autoplay-policy=no-user-gesture-required', '--mute-audio', url], { stdio: ['ignore', 'ignore', 'pipe'] });
let port = 0;
for (let i = 0; i < 100 && !port; i++) { await sleep(200); try { port = +fs.readFileSync(path.join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0]; } catch {} }
if (!port) throw new Error('no DevTools port');
let target = null;
for (let i = 0; i < 50 && !target; i++) { await sleep(200); target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find(t => t.type === 'page'); }
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise(r => ws.addEventListener('open', r, { once: true }));
let id = 0;
const pending = new Map(), console_ = [];
ws.addEventListener('message', (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
  else if (m.method === 'Runtime.consoleAPICalled') console_.push(m.params.args.map(a => a.value ?? a.description).join(' '));
  else if (m.method === 'Runtime.exceptionThrown') console_.push('exception: ' + JSON.stringify(m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text));
});
const cdp = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const run = async (expr) => { const r = await cdp('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true }); if (r.result?.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails)); return r.result?.result?.value; };
await cdp('Runtime.enable');
await cdp('Emulation.setDeviceMetricsOverride', { width: 1280, height: 720, deviceScaleFactor: 1, mobile: false });
const until = async (expr, what, seconds = 90) => { for (let i = 0; i < seconds * 4; i++) { if (await run(expr)) return; await sleep(250); } throw new Error('timed out: ' + what); };
await until(`!!window.__game`, 'the client');
await until(`document.querySelectorAll('#players .slot, #players li, #lobby').length > 0 || true`, 'the lobby');
await sleep(2000);
for (const m of [{ t: 'map', name: mapName }, { t: 'weather', v: 'clear' }, { t: 'addAi' }]) { await run(`window.__game.sendCmd(${JSON.stringify(m)}), true`); await sleep(700); }
await run(`window.__game.sendCmd({ t: 'start' }), true`);
await until(`!!window.__game.relief && !!window.__game.apron`, 'the match', 120);
await sleep(settle * 1000);
// only the WebGL canvas: the HUD, minimap and panels are hidden (the native frames have no HUD either)
await run(`(() => { const s = document.createElement('style'); s.textContent = 'body * { visibility: hidden !important; } canvas.tc-main { visibility: visible !important; }'; document.head.appendChild(s); window.__game.renderer.domElement.classList.add('tc-main'); return true; })()`);
const meta = { reference: execFileSync('git', ['-C', ref, 'rev-parse', 'HEAD']).toString().trim(), chromium: execFileSync(chromium, ['--version']).toString().trim(), map: mapName, room, poses: [] };
for (const [name, [x, z], yaw, dist] of poses) {
  const b = -(yaw + 90) * Math.PI / 180;
  await run(`(() => { const g = window.__game; g.rig.skipIntro?.(); g.cam.x = ${x}; g.cam.z = ${z}; g.cam.yaw = ${b}; g.cam.dist = Math.max(25, Math.min(${dist}, g.rig.wide)); return true; })()`);
  await sleep(2500);
  const pose = await run(`(() => { const g = window.__game; return { x: g.cam.x, z: g.cam.z, yaw: g.cam.yaw, dist: g.cam.dist, water: g.water ? g.water.mesh.geometry.index.count / 3 : 0 }; })()`);
  const shot = await cdp('Page.captureScreenshot', { format: 'png' });
  const file = path.join(out, `${mapName}-${name}.png`);
  fs.writeFileSync(file, Buffer.from(shot.result.data, 'base64'));
  meta.poses.push({ name, asked: { x, z, nativeYaw: yaw, dist }, got: pose, file });
  console.log('saved', file, JSON.stringify(pose));
}
meta.console = console_.slice(-40);
fs.writeFileSync(path.join(out, `${mapName}-reference-shots.json`), JSON.stringify(meta, null, 1));
await run(`window.__game.sendCmd({ t: 'end' }), true`).catch(() => {});
ws.close(); browser.kill('SIGTERM');
await new Promise(r => { browser.once('exit', r); setTimeout(r, 5000); });
fs.rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 300 });
