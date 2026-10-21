// The browser half of this scope's side by sides: the pinned client in headless Chromium (its own profile), served by
// the isolated reference server. Two modes (after unreal/Tools/compare/tour-reference.mjs; nothing in reference/ changes):
//
//   lobby: joins room <room> first (so it is the host), picks --map in the lobby's Map select, and saves the whole page
//          (lobby card over client/lobby-view.js's backdrop) at each of --at seconds, plus the backdrop canvas alone
//          node reference-shots.mjs lobby --origin http://127.0.0.1:<port> --room <room> --map seelow-heights --out <dir> [--at 6,14]
//   tour:  joins as a spectator, waits for the match, and at each pose of a tour file (the -TCTourFile format: name
//          seconds x z distance|wF yaw) saves the whole page (the paper war map's overlay and the HUD included) and logs
//          the camera, rig.wide, the map view's fade and the HUD's clear band (camera.js band(): #top's bottom, #buy's top)
//          node reference-shots.mjs tour --origin ... --room <room> --tour <file> --out <dir>
import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';

const mode = process.argv[2];
const arg = (name, d) => { const i = process.argv.indexOf(`--${name}`); return i > 0 ? process.argv[i + 1] : d; };
const origin = arg('origin'), room = arg('room'), out = arg('out'), chromium = arg('chromium', '/usr/bin/chromium'), timeout = +arg('timeout', 600);
if (!['lobby', 'tour'].includes(mode) || !origin || !room || !out) { console.error('usage: reference-shots.mjs lobby|tour --origin <url> --room <room> --out <dir> ...'); process.exit(2); }
fs.mkdirSync(out, { recursive: true });
const logFile = path.join(out, `reference-${mode}-log.jsonl`);
const log = (o) => { fs.appendFileSync(logFile, JSON.stringify({ at: new Date().toISOString(), ...o }) + '\n'); console.log(JSON.stringify(o)); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const port = 9300 + Math.floor(Math.random() * 500);
const browser = spawn(chromium, ['--headless=new', `--user-data-dir=${arg('profile', path.join(out, '.chromium-profile'))}`, `--remote-debugging-port=${port}`, '--no-first-run',
  '--no-default-browser-check', '--disable-extensions', '--disable-background-networking', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist',
  '--window-size=1280,720', '--mute-audio', 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
browser.stderr.on('data', () => {});
const kill = () => { try { browser.kill('SIGTERM'); } catch {} };
setTimeout(() => { log({ error: 'timeout' }); kill(); process.exit(3); }, timeout * 1000);
let ws, nextId = 1;
const pending = new Map();
const send = (method, params = {}) => new Promise((resolve, reject) => { const id = nextId++; pending.set(id, { resolve, reject }); ws.send(JSON.stringify({ id, method, params })); });
const evaluate = async (expression) => { const r = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }); if (r.exceptionDetails) throw new Error(r.exceptionDetails.text + ' ' + (r.exceptionDetails.exception?.description ?? '')); return r.result.value; };
for (let i = 0; i < 50 && !ws; i++) { try { const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json(); const page = list.find((t) => t.type === 'page'); if (page) { ws = new WebSocket(page.webSocketDebuggerUrl); break; } } catch {} await sleep(200); }
if (!ws) { log({ error: 'no DevTools target' }); kill(); process.exit(4); }
await new Promise((r) => ws.addEventListener('open', r));
ws.addEventListener('message', (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { const p = pending.get(m.id); pending.delete(m.id); m.error ? p.reject(new Error(m.error.message)) : p.resolve(m.result); return; }
  if (m.method === 'Runtime.exceptionThrown') log({ pageException: m.params.exceptionDetails?.exception?.description ?? m.params.exceptionDetails?.text });
});
await send('Page.enable'); await send('Runtime.enable');
await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 720, deviceScaleFactor: 1, mobile: false });
await send('Page.navigate', { url: origin + '/' }); await sleep(1500);
const who = mode === 'lobby' ? 'RefHost' : 'RefWatch';
await evaluate(`localStorage.setItem('ww2-gfx-auto', '1'); localStorage.setItem('ww2-gfx', 'high'); localStorage.setItem('ww2-controls-seen', '1'); localStorage.setItem('ww2-name', ${JSON.stringify(who)}); true`);
const shot = async (file) => { const s = await send('Page.captureScreenshot', { format: 'png' }); fs.writeFileSync(path.join(out, file), Buffer.from(s.data, 'base64')); return file; };
const band = `(() => { const t = document.getElementById('top').getBoundingClientRect(), b = document.getElementById('buy').getBoundingClientRect(); return { top: t.height ? t.bottom : 0, bottom: b.height ? b.top : innerHeight, innerHeight }; })()`;

if (mode === 'lobby') {
  await send('Page.navigate', { url: `${origin}/play?name=${who}#${room}` });
  for (let i = 0; i < 80; i++) { await sleep(250); try { if (await evaluate(`!!document.getElementById('mapSel') && document.getElementById('mapSel').options.length > 1`)) break; } catch {} }
  const map = arg('map');
  if (map) await evaluate(`(() => { const s = document.getElementById('mapSel'); s.value = ${JSON.stringify(map)}; s.dispatchEvent(new Event('change', { bubbles: true })); return s.value; })()`);
  const t0 = Date.now();
  log({ lobby: room, map, host: await evaluate(`document.getElementById('mapSel').disabled === false`) });
  for (const at of arg('at', '6,14').split(',').map(Number)) {
    while (Date.now() - t0 < at * 1000) await sleep(100);
    const state = await evaluate(`(() => { const c = document.querySelector('canvas.lobby-view'); return { canvas: !!c, on: !!c && c.classList.contains('on'), buildMs: c?.dataset.buildMs ?? null, mapSel: document.getElementById('mapSel').value }; })()`);
    const full = await shot(`warmap-reference-lobby-${map}-${at}s.png`);
    await evaluate(`(() => { let st = document.getElementById('tc-hide'); if (!st) { st = document.createElement('style'); st.id = 'tc-hide'; document.head.appendChild(st); } st.textContent = '#overlay > .card { visibility: hidden !important; }'; return true; })()`);
    await sleep(200);
    const bare = await shot(`warmap-reference-lobby-${map}-${at}s-backdrop.png`);
    await evaluate(`(() => { document.getElementById('tc-hide').textContent = ''; return true; })()`);
    log({ shot: full, bare, seconds: at, state });
  }
} else {
  const tour = fs.readFileSync(arg('tour'), 'utf8').split('\n').map((l) => l.replace(/#.*/, '').trim()).filter(Boolean).map((l) => {
    const [name, t, x, z, dist, yaw] = l.split(/\s+/);
    return { name, t: +t, x: +x, z: +z, wide: dist.startsWith('w') ? +dist.slice(1) : null, dist: dist.startsWith('w') ? null : +dist, yaw: +yaw };
  }).filter((p) => !Number.isNaN(p.x)).sort((a, b) => a.t - b.t);
  await send('Page.navigate', { url: `${origin}/play?name=${who}#${room}` });
  let ok = false;
  for (let i = 0; i < timeout * 4 && !ok; i++) { await sleep(250); try { ok = await evaluate(`!!(window.__game && window.__game.snapshot)`); } catch {} }
  if (!ok) { log({ error: 'no snapshot' }); kill(); process.exit(5); }
  for (let i = 0; i < 240; i++) { if (await evaluate(`!window.__game.rig.intro`)) break; await sleep(250); }
  log({ watching: room, wide: await evaluate(`window.__game.rig.wide`), band: await evaluate(band) });
  for (const s of tour) {
    for (;;) { const tick = await evaluate(`window.__game.snapshot ? window.__game.snapshot.tick : 0`); if (tick / 20 >= s.t) break; await sleep(200); }
    const yaw = -(s.yaw + 90) * Math.PI / 180;
    await evaluate(`(() => { const g = window.__game, c = g.cam; c.yaw = ${yaw}; c.x = ${s.x}; c.z = ${s.z}; c.dist = ${s.wide != null ? `g.rig.wide * ${s.wide}` : s.dist}; return true; })()`);
    await sleep(4500); // the fade eases in at 8 a second; the software renderer draws about a frame a second
    const state = await evaluate(`(() => { const g = window.__game; return { cam: { x: g.cam.x, z: g.cam.z, dist: g.cam.dist, yaw: g.cam.yaw }, wide: g.rig.wide, fade: g.mapView ? g.mapView.fade : null, matchTime: g.snapshot.tick / 20 }; })()`);
    const file = await shot(`warmap-reference-${s.name}.png`);
    log({ shot: file, pose: s, state, band: await evaluate(band) });
  }
}
kill();
process.exit(0);
