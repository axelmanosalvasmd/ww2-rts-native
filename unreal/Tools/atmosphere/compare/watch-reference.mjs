// The reference half of a live side by side: the pinned browser client, served by the isolated reference server the
// native client plays on, joins the same room in the middle of the match (a spectator: the server gives it no seat)
// in headless Chromium, and saves frames with its camera where the native shot looked. Driven over the DevTools
// protocol; nothing in reference/ is changed. Graphics stay on High: the automatic drop to Low (gfx.js watchFps) is
// switched off through its own localStorage key first, so clouds and birds stay drawn under the software renderer.
//
//   node unreal/Tools/atmosphere/compare/watch-reference.mjs --origin http://127.0.0.1:<port> --room <room> --out <dir>
//        --shots "<seconds after the first snapshot>:<x>,<z>,<dist>,<yaw radians>[;...]" (an empty field keeps the client's own) [--chromium /usr/bin/chromium]
//        [--profile <dir>] [--timeout 300] [--eval <expression logged with each shot>]
// Writes <out>/ref-<seconds>s-nohud.png (the page's DOM hidden, only the WebGL canvas) and ref-<seconds>s.png, and
// <out>/watch-log.jsonl (the client's mood, weather, last wx row and camera at each shot).
import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';

const arg = (name, d) => { const i = process.argv.indexOf(`--${name}`); return i > 0 ? process.argv[i + 1] : d; };
const origin = arg('origin'), room = arg('room'), out = arg('out'), shotsArg = arg('shots', '');
const chromium = arg('chromium', '/usr/bin/chromium'), timeout = +arg('timeout', 300), probe = arg('eval', '');
const profile = arg('profile', path.join(out ?? '.', '.chromium-profile'));
if (!origin || !room || !out || !shotsArg) { console.error('usage: watch-reference.mjs --origin <url> --room <room> --out <dir> --shots "s:x,z,dist,yaw;..."'); process.exit(2); }
const shots = shotsArg.split(';').filter(Boolean).map((s) => { const [t, v] = s.split(':'); const [x, z, dist, yaw] = v.split(',').map((t) => (t.trim() === '' ? NaN : Number(t))); return { t: +t, x, z, dist, yaw }; }).sort((a, b) => a.t - b.t);
fs.mkdirSync(out, { recursive: true });
const logFile = path.join(out, 'watch-log.jsonl');
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
ws.addEventListener('message', (e) => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { const p = pending.get(m.id); pending.delete(m.id); m.error ? p.reject(new Error(m.error.message)) : p.resolve(m.result); } });
await send('Page.enable'); await send('Runtime.enable');
await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 720, deviceScaleFactor: 1, mobile: false });
// the origin first, to keep Graphics on High (gfx.js: an 'ww2-gfx-auto' entry stops watchFps) and to name the watcher
await send('Page.navigate', { url: origin + '/' }); await sleep(1500);
await evaluate(`localStorage.setItem('ww2-gfx-auto', '1'); localStorage.setItem('ww2-gfx', 'high'); localStorage.setItem('ww2-controls-seen', '1'); localStorage.setItem('ww2-name', 'RefWatch'); true`);
await send('Page.navigate', { url: `${origin}/play?name=RefWatch#${room}` });
let t0 = 0;
for (let i = 0; i < timeout * 4 && !t0; i++) {
  await sleep(250);
  try { if (await evaluate(`!!(window.__game && window.__game.snapshot)`)) t0 = Date.now(); } catch {}
}
if (!t0) { log({ error: 'no snapshot' }); kill(); process.exit(5); }
log({ watching: true, room, gfx: await evaluate(`localStorage.getItem('ww2-gfx')`), mood: await evaluate(`window.__game.atmos.mood`), weather: await evaluate(`window.__game.atmos.weather`) });
for (const s of shots) {
  const wait = t0 + s.t * 1000 - Date.now();
  if (wait > 0) await sleep(wait);
  // an empty field keeps the client's own value (the opening view's target and heading, as the native client's)
  const set = Object.entries({ x: s.x, z: s.z, dist: s.dist, yaw: s.yaw }).filter(([, v]) => Number.isFinite(v)).map(([k, v]) => `c.${k} = ${v};`).join(' ');
  await evaluate(`(() => { const c = window.__game.cam; ${set} return true; })()`);
  await sleep(2300);
  const state = await evaluate(`(() => { const g = window.__game, s = g.snapshot; return { mood: g.atmos.mood, weather: g.atmos.weather, wx: s && s.wx, row: s && s.weather, cam: { x: g.cam.x, z: g.cam.z, dist: g.cam.dist, yaw: g.cam.yaw }, fps: g.fps && g.fps.value }; })()`);
  await evaluate(`(() => { let st = document.getElementById('tc-hide'); if (!st) { st = document.createElement('style'); st.id = 'tc-hide'; document.head.appendChild(st); } st.textContent = 'body * { visibility: hidden !important; } canvas { visibility: visible !important; }'; return true; })()`);
  await sleep(300);
  const bare = await send('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(path.join(out, `ref-${s.t}s-nohud.png`), Buffer.from(bare.data, 'base64'));
  await evaluate(`(() => { document.getElementById('tc-hide').textContent = ''; return true; })()`);
  await sleep(300);
  const full = await send('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(path.join(out, `ref-${s.t}s.png`), Buffer.from(full.data, 'base64'));
  log({ shot: s, state, probe: probe ? await evaluate(probe).catch((e) => String(e)) : undefined });
}
kill();
process.exit(0);
