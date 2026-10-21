// The reference half of the slice 13 terrain side by sides: the pinned browser client, served by the isolated reference
// server the native clients play on, joins the same room as a spectator or reconnects a seated token in headless
// Chromium with its own profile, and saves a frame at every pose of the same tour file the native -TCTourFile switch
// reads (Client/TCSlice13Checks.cpp). Only window.__game's camera state is set (cam.x, cam.z, cam.dist, cam.yaw: what the
// keyboard and wheel move); nothing in reference/ is changed. Only the renderer's canvas is kept in the bare frame. Graphics stay on High: gfx.js's automatic drop to Low is
// switched off through its own localStorage key first. After unreal/Tools/atmosphere/compare/watch-reference.mjs.
//
//   node unreal/Tools/compare/tour-reference.mjs --origin http://127.0.0.1:<port> --room <room> --tour <file> --out <dir>
//        [--chromium /usr/bin/chromium] [--profile <dir>] [--timeout 600] [--name RefWatch] [--block-maps] [--query ao=0]
//        [--token-file <file>] [--lang en|es] [--ui 100|150] [--alert-history]
// A token file reconnects the browser to that genuine seat. The token is never logged. Language, UI scale and the
// history switch use the reference's own preferences and alert module.
// --freeze-on-loss holds the browser presentation clock 2.5 s after a genuine own-unit removal, so a slow software
// renderer can capture that alert. It changes no wire payload or match state; the held tick and unit ids are logged.
// --block-maps (round 14): the page's requests for /maps/* fail. Only the lobby's map preview asks for them during play
// (client/main.js previewMap); without them client/lobby-view.js never rebuilds its backdrop in the middle of the match,
// which otherwise (always, for a spectator that joins a running match) calls setFogMap(null) and empties the match's map
// pieces through structures.js's shared state: the browser half then shows the reference's design. --query: more of
// the page's own URL switches (ao=0: no AO composer, the direct path). Each shot logs the lobby backdrop, the map piece
// count and whether the fog map is the match's.
// Tour lines: "name seconds x z distance yaw" (seconds of match time, the snapshot's tick / 20; distance in metres or wF, the
// share F of rig.wide; yaw the native heading in degrees, the browser's is -(yaw + 90) degrees). "frontier" for x (round
// 14): the first cell nobody on this side has seen (fog.at 'dark') on the line from the mean of seat me's ground units
// toward the map's centre, stepped back 6 m, as Client/TCSlice13Checks.cpp picks it. A spectator has no seat: seat 0
// (or --seat N), as the native tour takes it. "unit:<type>" for x (round 14): the living unit of that type with the
// lowest id on that seat (the same id the native tour picks). "step:<n>" for x (round 14, the Tutorial): waits for the
// snapshot's mode.step to reach step n (1-based) and 3 s more, then centres on that step's cell (shared/tutorial.js
// STEPS[n - 1].at, the cell its Hollis lines point to and the native client's Space jumps to). An optional 7th field ("hud") is read by the native tour only.
// "p<seconds>" for seconds (round 17): that match time and the match paused (#pauseBanner shown), the same frozen scene.
// "unit:<type>@<seat>" (round 17): the unit of that seat instead (an AI's squad), as the native tour picks it. Writes <out>/<name>.png (only the WebGL canvas) and <out>/<name>-hud.png, and tour-log.jsonl.
// "unit:vet" (round 17, lead): the living unit with the most veterancy stars of any seat (lowest id among equals), as the native tour picks it.
import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';

const arg = (name, d) => { const i = process.argv.indexOf(`--${name}`); return i > 0 ? process.argv[i + 1] : d; };
const origin = arg('origin'), room = arg('room'), out = arg('out'), tourFile = arg('tour');
const chromium = arg('chromium', '/usr/bin/chromium'), timeout = +arg('timeout', 600), who = arg('name', 'RefWatch');
const blockMaps = process.argv.includes('--block-maps'), query = arg('query', '');
const tokenFile = arg('token-file', ''), lang = arg('lang', 'en'), ui = +arg('ui', '100');
const showHistory = process.argv.includes('--alert-history');
const freezeOnLoss = process.argv.includes('--freeze-on-loss');
if (!['en', 'es'].includes(lang) || !Number.isInteger(ui) || ui < 75 || ui > 150) throw new Error('invalid language or UI scale');
const token = tokenFile ? fs.readFileSync(tokenFile, 'utf8').trim() : '';
if (tokenFile && !/^[0-9a-zA-Z_-]{8,40}$/.test(token)) throw new Error('invalid seat token file');
const seatArg = arg('seat', ''), gfxLevel = arg('gfx', 'high'), selectType = arg('select', ''), minFrames = +arg('min-frames', '6'); // round 17 (markers): --select <type> keeps the page's selection on that seat's lowest-id living unit of the type (drawing only, as the native -TCMarkersSelect); the settle waits for at least --min-frames frames (SwiftShader on a loaded machine), at most 90 s // round 14: --gfx low draws the browser on Graphics Low
if (!origin || !room || !out || !tourFile) { console.error('usage: tour-reference.mjs --origin <url> --room <room> --tour <file> --out <dir>'); process.exit(2); }
const profile = arg('profile', path.join(out, '.chromium-profile'));
const tour = fs.readFileSync(tourFile, 'utf8').split('\n').map((l) => l.replace(/#.*/, '').trim()).filter(Boolean).map((l) => {
  const [name, t, x, z, dist, yaw] = l.split(/\s+/);
  // round 17: unit:<type>@<seat> follows that seat's unit (Client/TCModelCompare.h FTCTourPose UnitSeat)
  const [unitType, unitSeat] = x.startsWith('unit:') ? x.slice(5).split('@') : [null, undefined];
  // round 17: p<seconds>, that match time and the match paused (the host's Pause: #pauseBanner shown)
  const whenPaused = t.startsWith('p');
  return { name, t: whenPaused ? +t.slice(1) : +t, whenPaused, x: +x, z: +z, frontier: x === 'frontier', unit: unitType, unitSeat: unitSeat !== undefined && /^\d+$/.test(unitSeat) ? +unitSeat : null, step: x.startsWith('step:') ? +x.slice(5) : null, wide: dist.startsWith('w') ? +dist.slice(1) : null, dist: dist.startsWith('w') ? null : +dist, yaw: +yaw };
}).sort((a, b) => a.t - b.t);
fs.mkdirSync(out, { recursive: true });
const logFile = path.join(out, 'tour-log.jsonl');
const log = (o) => { fs.appendFileSync(logFile, JSON.stringify({ at: new Date().toISOString(), ...o }) + '\n'); console.log(JSON.stringify(o)); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const port = 9300 + Math.floor(Math.random() * 500);
const browser = spawn(chromium, ['--headless=new', `--user-data-dir=${profile}`, `--remote-debugging-port=${port}`, '--no-first-run', '--no-default-browser-check',
  '--disable-extensions', '--disable-background-networking', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist',
  '--window-size=1280,720', '--mute-audio', '--autoplay-policy=no-user-gesture-required', 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
browser.stderr.on('data', (data) => fs.appendFileSync(path.join(out, 'chromium.log'), data));
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
let consoleLines = 0, blocked = 0;
ws.addEventListener('message', (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { const p = pending.get(m.id); pending.delete(m.id); m.error ? p.reject(new Error(m.error.message)) : p.resolve(m.result); return; }
  if (m.method === 'Fetch.requestPaused') { blocked++; send('Fetch.failRequest', { requestId: m.params.requestId, errorReason: 'BlockedByClient' }).catch(() => {}); return; }
  if (m.method === 'Runtime.exceptionThrown' && consoleLines++ < 200) log({ pageException: m.params.exceptionDetails?.exception?.description ?? m.params.exceptionDetails?.text });
  if (m.method === 'Runtime.consoleAPICalled' && ['error', 'warning', 'assert'].includes(m.params.type) && consoleLines++ < 200) log({ console: m.params.type, text: m.params.args.map((a) => a.value ?? a.description ?? '').join(' ').slice(0, 2000) });
});
await send('Page.enable'); await send('Runtime.enable');
if (freezeOnLoss) await send('Page.addScriptToEvaluateOnNewDocument', { source: `(() => {
  const Base = window.WebSocket, realNow = performance.now.bind(performance), rows = new Map(); let me = -1;
  window.WebSocket = class extends Base {
    constructor(...args) {
      super(...args);
      this.addEventListener('message', ({ data }) => {
        let m; try { m = JSON.parse(data); } catch { return; }
        if (m.t === 'lobby' || m.t === 'start') me = m.you ?? me;
        if (m.t !== 's') return;
        const lost = (m.gone ?? []).filter(id => rows.get(id)?.[2] === me);
        if (lost.length && !window.__tcLossClock) {
          let held = realNow();
          Object.defineProperty(performance, 'now', { configurable: true, value: () => held });
          window.__tcLossClock = { tick: m.tick, ids: lost, ageSeconds: 2.5 };
          setTimeout(() => { held += 2500; }, 0);
        }
        if (m.all || !m.held) rows.clear();
        for (const id of m.gone ?? []) rows.delete(id);
        for (const row of m.units ?? []) rows.set(row[0], row);
      });
    }
  };
})()` });
await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 720, deviceScaleFactor: 1, mobile: false });
await send('Page.navigate', { url: origin + '/' }); await sleep(1500);
await evaluate(`localStorage.setItem('ww2-gfx-auto', '1'); localStorage.setItem('ww2-gfx', ${JSON.stringify(gfxLevel)}); localStorage.setItem('ww2-controls-seen', '1'); localStorage.setItem('ww2-name', ${JSON.stringify(who)}); localStorage.setItem('ww2rts-lang', ${JSON.stringify(lang)}); localStorage.setItem('ww2-ui-scale', ${JSON.stringify(String(ui))}); true`);
if (tokenFile) await evaluate(`(() => { const k = ${JSON.stringify('ww2-token:' + room.toLowerCase())}; localStorage.setItem(k, ${JSON.stringify(token)}); sessionStorage.setItem(k, ${JSON.stringify(token)}); return true; })()`);
if (blockMaps) await send('Fetch.enable', { patterns: [{ urlPattern: '*/maps/*', requestStage: 'Request' }] });
await send('Page.navigate', { url: `${origin}/play?name=${encodeURIComponent(who)}${query ? '&' + query : ''}#${room}` });
let t0 = 0;
for (let i = 0; i < timeout * 4 && !t0; i++) {
  await sleep(250);
  try { if (await evaluate(`!!(window.__game && window.__game.snapshot)`)) t0 = Date.now(); } catch {}
}
if (!t0) { log({ error: 'no snapshot' }); kill(); process.exit(5); }
// Use the source camera API to finish the opening sweep before setting an exact diagnostic pose.
// Software rendering can outlast the fixed wall-clock intro wait while game dt remains clamped.
await evaluate(`window.__game.rig.skipIntro()`);
// the camera's opening sweep (camera.js intro) runs first: the tour starts once it has settled
for (let i = 0; i < 240; i++) { if (await evaluate(`!window.__game.rig.intro`)) break; await sleep(250); }
await evaluate(`(() => { window.__game.renderer.domElement.setAttribute('data-tc-main', '1'); return true; })()`);
const identity = await evaluate(`({ me: window.__game.me, watching: document.body.classList.contains('watching'), selected: [...window.__game.selected] })`);
if (tokenFile && !(Number.isInteger(identity.me) && identity.me >= 0)) { log({ error: 'requested seat was not admitted', identity }); kill(); process.exit(7); }
if (showHistory) await evaluate(`import('/client/alerts.js').then(({ alerts }) => { alerts.toggleHistory(); return alerts.history.length; })`);
log({ watching: identity.watching, identity, room, lang, ui, gfx: await evaluate(`localStorage.getItem('ww2-gfx')`), mood: await evaluate(`window.__game.atmos.mood`), weather: await evaluate(`window.__game.atmos.weather`), wide: await evaluate(`window.__game.rig.wide`) });
for (const s of tour) {
  // match time: the snapshot's tick / 20 (shared/sim.js TICK), as the native switch reads it
  for (;;) { const tick = await evaluate(`window.__game.snapshot ? window.__game.snapshot.tick : 0`); if (tick / 20 >= s.t) break; await sleep(200); }
  if (s.whenPaused) {
    for (;;) { if (await evaluate(`(() => { const b = document.getElementById('pauseBanner'); return !!b && !b.classList.contains('hidden'); })()`)) break; await sleep(250); }
    log({ pausedPose: s.name, tick: await evaluate(`window.__game.snapshot.tick`) });
  }
  const yaw = -(s.yaw + 90) * Math.PI / 180;
  const fps0 = await evaluate(`performance.now()`);
  if (s.frontier) {
    // the own army as ATCGameMode::ArmyIds takes it: living ground units, no structures, aircraft, trains or supply trucks
    // (UNITS from the page's own shared/sim.js module)
    const f = await evaluate(`(async () => { const g = window.__game, { UNITS } = await import('/shared/sim.js'); let n = 0, x = 0, z = 0; for (const v of g.units.values()) { const d = UNITS[v.type]; if (v.owner !== (${seatArg === '' ? '(Number.isInteger(g.me) && g.me >= 0 ? g.me : 0)' : +seatArg}) || !d || d.structure || d.air || d.rail || v.type === 'truck' || !(v.hp > 0)) continue; x += v.x; z += v.z; n++; }
      if (!n) return null; x /= n; z /= n; const W = g.fog.w * 2, H = g.fog.h * 2, tx = W / 2, tz = H / 2, len = Math.hypot(tx - x, tz - z), dx = len > 1 ? (tx - x) / len : 1, dz = len > 1 ? (tz - z) / len : 0;
      for (let t = 0; t <= Math.max(len, 1) + 400; t += 1) { const qx = x + dx * t, qz = z + dz * t; const a = g.fog.at(qx, qz); if (a === null) break; if (a === 'dark') { const b = Math.max(0, t - 6); return [x + dx * b, z + dz * b]; } } return [x, z]; })()`);
    log({ frontierPose: s.name, focus: f });
    if (f) { s.x = f[0]; s.z = f[1]; }
  }
  if (s.step) {
    for (;;) { const st = await evaluate(`(() => { const m = window.__game.snapshot && window.__game.snapshot.mode; return m && m.kind === 'tutorial' ? m.step : -1; })()`); if (st >= s.step - 1) break; await sleep(250); }
    await sleep(3000);
    const at = await evaluate(`(async () => { const { STEPS } = await import('/shared/tutorial.js'); const st = STEPS[${s.step - 1}]; return st ? st.at : null; })()`);
    log({ stepPose: s.name, step: s.step, at, goal: await evaluate(`window.__game.snapshot.mode.goal`) });
    if (at) { s.x = at[0] * 2 + 1; s.z = at[1] * 2 + 1; }
  }
  if (s.unit) {
    const u = await evaluate(`(() => { const g = window.__game, seat = ${s.unitSeat != null ? s.unitSeat : seatArg === '' ? '(Number.isInteger(g.me) && g.me >= 0 ? g.me : 0)' : +seatArg}; let best = null; for (const [id, v] of g.units) { if (${s.unit === 'vet' ? "v.vet > 0 && v.hp > 0 && (best === null || v.vet > best.vet || (v.vet === best.vet && +id < best[0]))" : `v.type === ${JSON.stringify(s.unit)} && v.owner === seat && v.hp > 0 && (best === null || +id < best[0])`}) { best = [+id, v.x, v.z]; best.vet = v.vet; } } return best; })()`);
    log({ unitPose: s.name, unit: u });
    if (u) { s.x = u[1]; s.z = u[2]; }
  }
  await evaluate(`(() => { const g = window.__game, c = g.cam; c.x = ${s.x}; c.z = ${s.z}; c.yaw = ${yaw}; c.dist = ${s.wide != null ? `g.rig.wide * ${s.wide}` : s.dist}; return true; })()`);
  if (selectType) {
    const sel = await evaluate(`(() => { const g = window.__game, seat = ${seatArg === '' ? '(Number.isInteger(g.me) && g.me >= 0 ? g.me : 0)' : +seatArg}; let best = null; for (const [id, v] of g.units) { if (v.type === ${JSON.stringify(selectType)} && v.owner === seat && v.hp > 0 && (best === null || +id < best)) best = +id; } g.selected.clear(); if (best !== null) g.selected.add(best); return best; })()`);
    log({ selected: sel, type: selectType, pose: s.name });
    // Paused snapshots do not redraw the selection DOM after a direct camera-tool selection.
    // Use the reference Idle control to refresh it, and require the requested type to be visible.
    const drawn = await evaluate(`(() => {
      const g = window.__game, type = ${JSON.stringify(selectType)};
      const hasRow = () => [...document.querySelectorAll('#selection .srow')].some(e => e.dataset.type === type);
      if (hasRow()) return true;
      for (let i = 0; i < 32; i++) {
        const idle = document.querySelector('#selection [data-idle]');
        if (!idle) break;
        idle.click();
        if ([...g.selected].some(id => g.units.get(id)?.type === type) && hasRow()) return true;
      }
      return false;
    })()`);
    if (!drawn) { log({ error: 'requested selection has no visible HUD row', type: selectType }); kill(); process.exit(8); }
    await evaluate(`(() => { const c = window.__game.cam; c.x = ${s.x}; c.z = ${s.z}; c.yaw = ${yaw}; c.dist = ${s.wide != null ? `window.__game.rig.wide * ${s.wide}` : s.dist}; return true; })()`);
  }
  // frames drawn while the view settles, 4 s (requestAnimationFrame intervals: the software renderer's, not a measure of the game)
  const frames = await evaluate(`new Promise((done) => { const t = []; let last = performance.now(); const t0 = last, end = last + 4000; const f = (now) => { t.push(now - last); last = now; if ((now < end || t.length < ${minFrames}) && now < t0 + 90000) requestAnimationFrame(f); else done(t); }; requestAnimationFrame(f); })`);
  const state = await evaluate(`(() => { const g = window.__game, s = g.snapshot; return { mood: g.atmos.mood, weather: g.atmos.weather, wx: s && s.wx, cam: { x: g.cam.x, z: g.cam.z, dist: g.cam.dist, yaw: g.cam.yaw }, wide: g.rig.wide, paper: g.mapView ? g.mapView.fade : null, fog: [g.scene.fog.near, g.scene.fog.far, g.scene.fog.color.getHexString()], composer: g.renderer.info.programs.some((p) => p.name === 'OutputShader') }; })()`);
  if (!s.unit && !s.frontier && !s.step && (Math.abs(state.cam.x - s.x) > 0.05 || Math.abs(state.cam.z - s.z) > 0.05 || (s.dist != null && Math.abs(state.cam.dist - s.dist) > 0.05) || Math.abs(state.cam.yaw - yaw) > 1e-7)) {
    log({ error: 'camera did not reach the requested pose', requested: s, actual: state.cam }); kill(); process.exit(9);
  }
  // the lobby backdrop's side effects (round 14): its canvas, the match's map pieces, the fog map the apron overlay reads
  const sideEffects = await evaluate(`(() => { const g = window.__game, K = new Set(['brick','stone','gableBrick','roof','paint','pane','wall','gable','window','earth','shrub','leaves','wood','dark','rail','duck']); let pieces = 0; g.scene.traverse((o) => { if (o.isInstancedMesh && K.has(o.name)) pieces += o.count; }); const u = g.renderer.properties.get(g.apron.fogMesh.material).uniforms || {}; return { lobbyBackdrop: document.querySelectorAll('canvas.lobby-view').length, pieces, fowSize: u.fowSize?.value?.toArray(), fogMapIsMatch: u.fowMap?.value === g.fog?.texture }; })()`);
  if (s.unit) {
    // round 14: follow the unit to the last moment (a moving vehicle stays in the middle of the shot)
    const u2 = await evaluate(`(() => { const g = window.__game, seat = ${s.unitSeat != null ? s.unitSeat : seatArg === '' ? '(Number.isInteger(g.me) && g.me >= 0 ? g.me : 0)' : +seatArg}; let best = null; for (const [id, v] of g.units) { if (${s.unit === 'vet' ? "v.vet > 0 && v.hp > 0 && (best === null || v.vet > best.vet || (v.vet === best.vet && +id < best[0]))" : `v.type === ${JSON.stringify(s.unit)} && v.owner === seat && v.hp > 0 && (best === null || +id < best[0])`}) { best = [+id, v.x, v.z, v.speed ?? null]; best.vet = v.vet; } } if (best) { g.cam.x = best[1]; g.cam.z = best[2]; } return best; })()`);
    log({ unitFollow: s.name, unit: u2 });
    await sleep(1500);
  }
  await evaluate(`(() => { let st = document.getElementById('tc-hide'); if (!st) { st = document.createElement('style'); st.id = 'tc-hide'; document.head.appendChild(st); } st.textContent = 'body * { visibility: hidden !important; } canvas[data-tc-main] { visibility: visible !important; }'; return true; })()`);
  await sleep(300);
  const bare = await send('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(path.join(out, `${s.name}.png`), Buffer.from(bare.data, 'base64'));
  await evaluate(`(() => { document.getElementById('tc-hide').textContent = ''; return true; })()`);
  await sleep(300);
  const full = await send('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(path.join(out, `${s.name}-hud.png`), Buffer.from(full.data, 'base64'));
  const ms = frames.slice(1);
  log({ shot: s.name, pose: s, seconds: (Date.now() - t0) / 1000, matchTime: await evaluate(`window.__game.snapshot.tick / 20`), lossClock: await evaluate(`window.__tcLossClock ?? null`), state, sideEffects, mapsBlocked: blocked, frames: ms.length, frameMeanMs: ms.reduce((a, b) => a + b, 0) / Math.max(1, ms.length), frameWorstMs: Math.max(0, ...ms), settledFrom: fps0 });
}
kill();
process.exit(0);
