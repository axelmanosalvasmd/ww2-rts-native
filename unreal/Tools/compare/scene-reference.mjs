// Round 17 (poses): the reference half of a scripted-scene side by side. A native client set up a scene in its own seat
// with -TCOrdersCheck (garrison, Shift queues, a rail ride, a recovery), paused the match with press:pause, took its
// shots ("shot:<name>" steps, each logged with the camera as drawn and the selection) and quit. This tool then sits the
// pinned browser client in the same seat of the same paused match: the seat's token goes into the page's own
// localStorage key (client/room-session.js 'ww2-token:<room>'), so server.js hands it the seat back as to a returning
// player (the match stays paused: a drop does not undo the host's Pause). For each native shot it selects the same unit
// ids (window.__game.selected, the page's own Set; the next paused snapshot, one a second, refreshes the HUD), sets the
// same camera (cam.x, cam.z, cam.dist; yaw -(native yaw + 90) degrees) and saves <out>/<name>.png (the WebGL canvas only)
// and <out>/<name>-hud.png. Nothing in reference/ is changed; the token is never logged.
//
//   node unreal/Tools/compare/scene-reference.mjs --origin http://127.0.0.1:<port> --room <room> --token-file <file>
//        --host-log <native log> --out <dir> [--name Alder] [--only a,b] [--chromium /usr/bin/chromium] [--profile <dir>]
//        [--timeout 300] [--block-maps]
// The host log lines read: "orders check K/N: screenshot <dir>/<name>.png | camera (x, z) yaw Y distance D m | tick T;
// selected [ids]; ..." (Client/TCOrders.cpp with the round 17 lead patch). Writes <out>/scene-log.jsonl.
import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';

const arg = (name, d) => { const i = process.argv.indexOf(`--${name}`); return i > 0 ? process.argv[i + 1] : d; };
const origin = arg('origin'), room = arg('room'), out = arg('out'), tokenFile = arg('token-file'), hostLog = arg('host-log');
const chromium = arg('chromium', '/usr/bin/chromium'), timeout = +arg('timeout', 300), who = arg('name', 'Alder');
const only = arg('only', '') ? new Set(arg('only').split(',')) : null, blockMaps = process.argv.includes('--block-maps');

// The native shots: name, camera and selection, in the order taken.
export function parseShots(text) {
  const shots = [];
  const re = /orders check \d+\/\d+: screenshot (\S+?)\.png \| camera \((-?[\d.]+), (-?[\d.]+)\) yaw (-?[\d.]+) distance ([\d.]+) m \| tick (\d+); selected \[([\d,]*)\]/;
  for (const line of text.split('\n')) {
    const m = re.exec(line);
    if (!m) continue;
    shots.push({ name: path.basename(m[1]), x: +m[2], z: +m[3], yaw: +m[4], dist: +m[5], tick: +m[6], selected: m[7] ? m[7].split(',').map(Number) : [] });
  }
  return shots;
}
// three's camera yaw for a native heading in degrees (tour-reference.mjs: -(yaw + 90) degrees)
export const browserYaw = (nativeYawDeg) => -(nativeYawDeg + 90) * Math.PI / 180;

if (process.argv[1] && import.meta.url.endsWith(path.basename(process.argv[1]))) await main();

async function main() {
  if (!origin || !room || !out || !tokenFile || !hostLog) { console.error('usage: scene-reference.mjs --origin <url> --room <room> --token-file <file> --host-log <log> --out <dir>'); process.exit(2); }
  const profile = arg('profile', path.join(out, '.chromium-profile'));
  fs.mkdirSync(out, { recursive: true });
  const logFile = path.join(out, 'scene-log.jsonl');
  const log = (o) => { fs.appendFileSync(logFile, JSON.stringify({ at: new Date().toISOString(), ...o }) + '\n'); console.log(JSON.stringify(o)); };
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const shots = parseShots(fs.readFileSync(hostLog, 'utf8')).filter((s) => !only || only.has(s.name));
  log({ shots: shots.length, names: shots.map((s) => s.name) });
  if (!shots.length) { log({ error: 'no native shot with a camera in the host log' }); process.exit(6); }
  const token = fs.readFileSync(tokenFile, 'utf8').trim();
  if (!/^[0-9a-zA-Z_-]{8,40}$/.test(token)) { log({ error: 'token file does not hold a token' }); process.exit(7); }

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
  let consoleLines = 0;
  ws.addEventListener('message', (e) => {
    const m = JSON.parse(e.data);
    if (m.id && pending.has(m.id)) { const p = pending.get(m.id); pending.delete(m.id); m.error ? p.reject(new Error(m.error.message)) : p.resolve(m.result); return; }
    if (m.method === 'Fetch.requestPaused') { send('Fetch.failRequest', { requestId: m.params.requestId, errorReason: 'BlockedByClient' }).catch(() => {}); return; }
    if (m.method === 'Runtime.exceptionThrown' && consoleLines++ < 200) log({ pageException: m.params.exceptionDetails?.exception?.description ?? m.params.exceptionDetails?.text });
  });
  await send('Page.enable'); await send('Runtime.enable');
  await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 720, deviceScaleFactor: 1, mobile: false });
  await send('Page.navigate', { url: origin + '/' }); await sleep(1500);
  // the seat's token under the page's own key, Graphics High, the Controls sheet already seen, the host's name
  await evaluate(`(() => { const k = 'ww2-token:' + ${JSON.stringify(room.toLowerCase())}; localStorage.setItem(k, ${JSON.stringify(token)}); sessionStorage.setItem(k, ${JSON.stringify(token)}); localStorage.setItem('ww2-gfx-auto', '1'); localStorage.setItem('ww2-gfx', 'high'); localStorage.setItem('ww2-controls-seen', '1'); localStorage.setItem('ww2-name', ${JSON.stringify(who)}); return true; })()`);
  if (blockMaps) await send('Fetch.enable', { patterns: [{ urlPattern: '*/maps/*', requestStage: 'Request' }] });
  await send('Page.navigate', { url: `${origin}/play?name=${encodeURIComponent(who)}#${room}` });
  let t0 = 0;
  for (let i = 0; i < timeout * 4 && !t0; i++) { await sleep(250); try { if (await evaluate(`!!(window.__game && window.__game.snapshot)`)) t0 = Date.now(); } catch {} }
  if (!t0) { log({ error: 'no snapshot' }); kill(); process.exit(5); }
  for (let i = 0; i < 240; i++) { if (await evaluate(`!window.__game.rig.intro`)) break; await sleep(250); }
  await evaluate(`(() => { window.__game.renderer.domElement.setAttribute('data-tc-main', '1'); return true; })()`);
  const seat = await evaluate(`window.__game.me`);
  const paused = await evaluate(`(() => { const b = document.getElementById('pauseBanner'); return !!b && !b.classList.contains('hidden'); })()`);
  log({ seated: seat, paused, tick: await evaluate(`window.__game.snapshot.tick`) });
  for (const s of shots) {
    const ids = await evaluate(`(() => { const g = window.__game; g.selected.clear(); const ids = ${JSON.stringify(s.selected)}; for (const id of ids) if (g.units.has(id)) g.selected.add(id); return [...g.selected]; })()`);
    await evaluate(`(() => { const c = window.__game.cam; c.x = ${s.x}; c.z = ${s.z}; c.yaw = ${browserYaw(s.yaw)}; c.dist = ${s.dist}; return true; })()`);
    // two paused snapshots (one a second) refresh the HUD for the selection; frames settle on the software renderer
    await sleep(2500);
    await evaluate(`new Promise((done) => { let n = 0; const f = () => (++n < 3 ? requestAnimationFrame(f) : done(true)); requestAnimationFrame(f); })`);
    const state = await evaluate(`(() => { const g = window.__game, s = g.snapshot; return { tick: s.tick, cam: { x: g.cam.x, z: g.cam.z, dist: g.cam.dist, yaw: g.cam.yaw }, selected: [...g.selected], orders: [...g.selected].map((id) => [id, (g.units.get(id)?.orders ?? []).length]), garrisoned: [...g.units.values()].filter((v) => v.owner === g.me && (v.flags & 32)).length }; })()`);
    await evaluate(`(() => { let st = document.getElementById('tc-hide'); if (!st) { st = document.createElement('style'); st.id = 'tc-hide'; document.head.appendChild(st); } st.textContent = 'body * { visibility: hidden !important; } canvas[data-tc-main] { visibility: visible !important; }'; return true; })()`);
    await sleep(300);
    fs.writeFileSync(path.join(out, `${s.name}.png`), Buffer.from((await send('Page.captureScreenshot', { format: 'png' })).data, 'base64'));
    await evaluate(`(() => { document.getElementById('tc-hide').textContent = ''; return true; })()`);
    await sleep(300);
    fs.writeFileSync(path.join(out, `${s.name}-hud.png`), Buffer.from((await send('Page.captureScreenshot', { format: 'png' })).data, 'base64'));
    log({ shot: s.name, native: s, selectedHere: ids, state });
  }
  kill();
  process.exit(0);
}
