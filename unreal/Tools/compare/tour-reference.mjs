// The reference half of the slice 13 terrain side by sides: the pinned browser client, served by the isolated reference
// server the native clients play on, joins the same room as a spectator (the server gives it no seat) in headless
// Chromium with its own profile, and saves a frame at every pose of the same tour file the native -TCTourFile switch
// reads (Client/TCSlice13Checks.cpp). Only window.__game's camera state is set (cam.x, cam.z, cam.dist, cam.yaw: what the
// keyboard and wheel move); nothing in reference/ is changed. Only the renderer's canvas is kept in the bare frame. Graphics stay on High: gfx.js's automatic drop to Low is
// switched off through its own localStorage key first. After unreal/Tools/atmosphere/compare/watch-reference.mjs.
//
//   node unreal/Tools/compare/tour-reference.mjs --origin http://127.0.0.1:<port> --room <room> --tour <file> --out <dir>
//        [--chromium /usr/bin/chromium] [--profile <dir>] [--timeout 600] [--name RefWatch]
// Tour lines: "name seconds x z distance yaw" (seconds of match time, the snapshot's tick / 20; distance in metres or wF, the
// share F of rig.wide; yaw the native heading in degrees, the browser's is -(yaw + 90) degrees). "frontier" poses are
// skipped (native only). Writes <out>/<name>.png (only the WebGL canvas) and <out>/<name>-hud.png, and tour-log.jsonl.
import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';

const arg = (name, d) => { const i = process.argv.indexOf(`--${name}`); return i > 0 ? process.argv[i + 1] : d; };
const origin = arg('origin'), room = arg('room'), out = arg('out'), tourFile = arg('tour');
const chromium = arg('chromium', '/usr/bin/chromium'), timeout = +arg('timeout', 600), who = arg('name', 'RefWatch');
if (!origin || !room || !out || !tourFile) { console.error('usage: tour-reference.mjs --origin <url> --room <room> --tour <file> --out <dir>'); process.exit(2); }
const profile = arg('profile', path.join(out, '.chromium-profile'));
const tour = fs.readFileSync(tourFile, 'utf8').split('\n').map((l) => l.replace(/#.*/, '').trim()).filter(Boolean).map((l) => {
  const [name, t, x, z, dist, yaw] = l.split(/\s+/);
  return { name, t: +t, x: +x, z: +z, frontier: x === 'frontier', wide: dist.startsWith('w') ? +dist.slice(1) : null, dist: dist.startsWith('w') ? null : +dist, yaw: +yaw };
}).filter((p) => !p.frontier).sort((a, b) => a.t - b.t);
fs.mkdirSync(out, { recursive: true });
const logFile = path.join(out, 'tour-log.jsonl');
const log = (o) => { fs.appendFileSync(logFile, JSON.stringify({ at: new Date().toISOString(), ...o }) + '\n'); console.log(JSON.stringify(o)); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const port = 9300 + Math.floor(Math.random() * 500);
const browser = spawn(chromium, ['--headless=new', `--user-data-dir=${profile}`, `--remote-debugging-port=${port}`, '--no-first-run', '--no-default-browser-check',
  '--disable-extensions', '--disable-background-networking', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist',
  '--window-size=1280,720', '--mute-audio', '--autoplay-policy=no-user-gesture-required', 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
browser.stderr.on('data', () => {});
const kill = () => { try { browser.kill('SIGTERM'); } catch {} };
setTimeout(() => { log({ error: 'timeout' }); kill(); process.exit(3); }, timeout * 1000);

let ws, nextId = 1;
const pending = new Map();
const send = (method, params = {}) => new Promise((resolve, reject) => { const id = nextId++; pending.set(id, { resolve, reject }); ws.send(JSON.stringify({ id, method, params })); });
const evaluate = async (expression) => { const r = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }); if (r.exceptionDetails) throw new Error(r.exceptionDetails.text + ' ' + (r.exceptionDetails.exception?.description ?? '')); return r.result.value; };

for (let i = 0; i < 50; i++) { try { const r = await fetch(`http://127.0.0.1:${port}/json/list`); const list = await r.json(); const page = list.find((t) => t.type === 'page'); if (page) { ws = new WebSocket(page.webSocketDebuggerUrl); break; } } catch {} await sleep(200); }
if (!ws) { log({ error: 'no DevTools target' }); kill(); process.exit(4); }
await new Promise((r) => ws.addEventListener('open', r));
// the page's own errors and console warnings go to the log too (a reference module that failed explains a missing layer)
let consoleLines = 0;
ws.addEventListener('message', (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { const p = pending.get(m.id); pending.delete(m.id); m.error ? p.reject(new Error(m.error.message)) : p.resolve(m.result); return; }
  if (m.method === 'Runtime.exceptionThrown' && consoleLines++ < 200) log({ pageException: m.params.exceptionDetails?.exception?.description ?? m.params.exceptionDetails?.text });
  if (m.method === 'Runtime.consoleAPICalled' && ['error', 'warning', 'assert'].includes(m.params.type) && consoleLines++ < 200) log({ console: m.params.type, text: m.params.args.map((a) => a.value ?? a.description ?? '').join(' ').slice(0, 2000) });
});
await send('Page.enable'); await send('Runtime.enable');
await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 720, deviceScaleFactor: 1, mobile: false });
await send('Page.navigate', { url: origin + '/' }); await sleep(1500);
await evaluate(`localStorage.setItem('ww2-gfx-auto', '1'); localStorage.setItem('ww2-gfx', 'high'); localStorage.setItem('ww2-controls-seen', '1'); localStorage.setItem('ww2-name', ${JSON.stringify(who)}); true`);
await send('Page.navigate', { url: `${origin}/play?name=${encodeURIComponent(who)}#${room}` });
let t0 = 0;
for (let i = 0; i < timeout * 4 && !t0; i++) {
  await sleep(250);
  try { if (await evaluate(`!!(window.__game && window.__game.snapshot)`)) t0 = Date.now(); } catch {}
}
if (!t0) { log({ error: 'no snapshot' }); kill(); process.exit(5); }
// the camera's opening sweep (camera.js intro) runs first: the tour starts once it has settled
for (let i = 0; i < 240; i++) { if (await evaluate(`!window.__game.rig.intro`)) break; await sleep(250); }
await evaluate(`(() => { window.__game.renderer.domElement.setAttribute('data-tc-main', '1'); return true; })()`);
log({ watching: true, room, gfx: await evaluate(`localStorage.getItem('ww2-gfx')`), mood: await evaluate(`window.__game.atmos.mood`), weather: await evaluate(`window.__game.atmos.weather`), wide: await evaluate(`window.__game.rig.wide`) });
for (const s of tour) {
  // match time: the snapshot's tick / 20 (shared/sim.js TICK), as the native switch reads it
  for (;;) { const tick = await evaluate(`window.__game.snapshot ? window.__game.snapshot.tick : 0`); if (tick / 20 >= s.t) break; await sleep(200); }
  const yaw = -(s.yaw + 90) * Math.PI / 180;
  const fps0 = await evaluate(`performance.now()`);
  await evaluate(`(() => { const g = window.__game, c = g.cam; c.x = ${s.x}; c.z = ${s.z}; c.yaw = ${yaw}; c.dist = ${s.wide != null ? `g.rig.wide * ${s.wide}` : s.dist}; return true; })()`);
  // frames drawn while the view settles, 4 s (requestAnimationFrame intervals: the software renderer's, not a measure of the game)
  const frames = await evaluate(`new Promise((done) => { const t = []; let last = performance.now(); const end = last + 4000; const f = (now) => { t.push(now - last); last = now; if (now < end) requestAnimationFrame(f); else done(t); }; requestAnimationFrame(f); })`);
  const state = await evaluate(`(() => { const g = window.__game, s = g.snapshot; return { mood: g.atmos.mood, weather: g.atmos.weather, wx: s && s.wx, cam: { x: g.cam.x, z: g.cam.z, dist: g.cam.dist, yaw: g.cam.yaw }, wide: g.rig.wide, paper: g.mapView ? g.mapView.fade : null }; })()`);
  await evaluate(`(() => { let st = document.getElementById('tc-hide'); if (!st) { st = document.createElement('style'); st.id = 'tc-hide'; document.head.appendChild(st); } st.textContent = 'body * { visibility: hidden !important; } canvas[data-tc-main] { visibility: visible !important; }'; return true; })()`);
  await sleep(300);
  const bare = await send('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(path.join(out, `${s.name}.png`), Buffer.from(bare.data, 'base64'));
  await evaluate(`(() => { document.getElementById('tc-hide').textContent = ''; return true; })()`);
  await sleep(300);
  const full = await send('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(path.join(out, `${s.name}-hud.png`), Buffer.from(full.data, 'base64'));
  const ms = frames.slice(1);
  log({ shot: s.name, pose: s, seconds: (Date.now() - t0) / 1000, matchTime: await evaluate(`window.__game.snapshot.tick / 20`), state, frames: ms.length, frameMeanMs: ms.reduce((a, b) => a + b, 0) / Math.max(1, ms.length), frameWorstMs: Math.max(0, ...ms), settledFrom: fps0 });
}
kill();
process.exit(0);
