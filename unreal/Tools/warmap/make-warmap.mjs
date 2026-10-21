// The paper war map's assets and oracle, made from the pinned reference's own code (client/symbols.js,
// client/map-view.js), never drawn by hand. Two commands:
//
//   node unreal/Tools/warmap/make-warmap.mjs assets --work <scratch dir> [--chromium /usr/bin/chromium]
//     Writes into <work>:
//       symbols.png       every SYMBOLS entry of client/symbols.js and the "unknown" glyph, drawn by Chromium's own
//                         canvas with drawSymbol(): row block 0 the filled silhouettes (white, even-odd), row block 1
//                         the halos (the stroke drawSymbol puts under them, 18 box units wide, round joins). The
//                         Unreal side tints each with the owner's colour and the paper colour (client/map-view.js draw)
//       WarMapData.json   the atlas layout, the shared/sim.js numbers map-view.js reads (CELL, CFG.pointRadius,
//                         CFG.levelHeight, each building's footprint size for client/fog.js), and the font record
//       font/<file>.ttf   the face Chromium draws map-view.js's "bold ... Courier New" with on this machine
//                         (fontconfig's match), with its licence file when one sits beside it
//
//   node unreal/Tools/warmap/make-warmap.mjs oracle --work <scratch dir> --three <dir with three.module.js>
//        [--maps seelow-heights,crossroads-village] [--chromium ...]
//     Runs client/map-view.js createMapView() unchanged in Chromium for each map (grid = map rows, hAt = the cell's
//     level x 2.5 m), lets one frame() paint, and saves the paper canvas and the desk canvas it made as
//     <work>/oracle/<map>-paper.png and <map>-desk.png, with <map>.json (w, h, PX, W, H). The Unreal port
//     (Client/TCWarMap.h, -run=TCWarMapPaint) paints the same inputs; Tools/warmap/compare-oracle.mjs compares them.
//
// Nothing in reference/ is changed: a loopback static server reads the reference folder and serves a harness page.
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import crypto from 'node:crypto';
import { spawn, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, '..', '..', '..');
const REF = path.join(REPO, 'reference', 'ww2-rts');
const TOOL_VERSION = 1;
const cmd = process.argv[2];
const arg = (name, d) => { const i = process.argv.indexOf(`--${name}`); return i > 0 ? process.argv[i + 1] : d; };
const work = arg('work');
if (!['assets', 'oracle'].includes(cmd) || !work) { console.error('usage: make-warmap.mjs assets|oracle --work <dir> [...]'); process.exit(2); }
const chromium = arg('chromium', '/usr/bin/chromium');
const sha = (buf) => crypto.createHash('sha256').update(buf).digest('hex');
const shaFile = (p) => sha(fs.readFileSync(p));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
fs.mkdirSync(work, { recursive: true });
const refPin = (() => { try { return execFileSync('git', ['-C', REF, 'rev-parse', 'HEAD']).toString().trim(); } catch { return 'unknown'; } })();

// ---------- a loopback static server: /ref/... the reference folder (read only), /vendor/three.module.js, /harness.html
const threeDir = arg('three');
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.json': 'application/json', '.html': 'text/html' };
const HARNESS = `<!doctype html><html><head><meta charset="utf-8">
<script type="importmap">{ "imports": { "three": "/vendor/three.module.js", "three/addons/": "/vendor-jsm/" } }</script>
</head><body style="margin:0;background:#000"></body></html>`;
const server = http.createServer((req, res) => {
  const u = new URL(req.url, 'http://x');
  let file = null;
  if (u.pathname === '/harness.html') { res.writeHead(200, { 'content-type': 'text/html' }); return res.end(HARNESS); }
  if (u.pathname.startsWith('/ref/')) file = path.join(REF, decodeURIComponent(u.pathname.slice(5)));
  else if (u.pathname.startsWith('/vendor/') && threeDir) file = path.join(threeDir, decodeURIComponent(u.pathname.slice(8)));
  if (!file || !path.resolve(file).startsWith(path.resolve(file.startsWith(REF) ? REF : threeDir)) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) { res.writeHead(404); return res.end(); }
  res.writeHead(200, { 'content-type': TYPES[path.extname(file)] ?? 'application/octet-stream' });
  res.end(fs.readFileSync(file));
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const origin = `http://127.0.0.1:${server.address().port}`;

// ---------- Chromium over the DevTools protocol (after unreal/Tools/compare/tour-reference.mjs)
const profile = path.join(work, '.chromium-profile');
const port = 9800 + Math.floor(Math.random() * 400);
const browser = spawn(chromium, ['--headless=new', `--user-data-dir=${profile}`, `--remote-debugging-port=${port}`, '--no-first-run', '--no-default-browser-check',
  '--disable-extensions', '--disable-background-networking', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist',
  '--window-size=1280,720', '--mute-audio', 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
browser.stderr.on('data', () => {});
const done = (code) => { try { browser.kill('SIGTERM'); } catch {} server.close(); process.exit(code); };
let ws, nextId = 1;
const pending = new Map();
const send = (method, params = {}) => new Promise((resolve, reject) => { const id = nextId++; pending.set(id, { resolve, reject }); ws.send(JSON.stringify({ id, method, params })); });
const evaluate = async (expression) => { const r = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }); if (r.exceptionDetails) throw new Error(r.exceptionDetails.text + ' ' + (r.exceptionDetails.exception?.description ?? '')); return r.result.value; };
for (let i = 0; i < 50 && !ws; i++) { try { const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json(); const page = list.find((t) => t.type === 'page'); if (page) { ws = new WebSocket(page.webSocketDebuggerUrl); break; } } catch {} await sleep(200); }
if (!ws) { console.error('no DevTools target'); done(4); }
await new Promise((r) => ws.addEventListener('open', r));
ws.addEventListener('message', (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { const p = pending.get(m.id); pending.delete(m.id); m.error ? p.reject(new Error(m.error.message)) : p.resolve(m.result); return; }
  if (m.method === 'Runtime.exceptionThrown') console.error('page exception:', m.params.exceptionDetails?.exception?.description ?? m.params.exceptionDetails?.text);
});
await send('Page.enable'); await send('Runtime.enable');
await send('Page.navigate', { url: origin + '/harness.html' }); await sleep(800);
const savePng = (dataUrl, file) => { const buf = Buffer.from(dataUrl.split(',')[1], 'base64'); fs.writeFileSync(file, buf); return sha(buf); };

try {
  if (cmd === 'assets') await assets();
  else await oracle();
  done(0);
} catch (e) { console.error(e.stack ?? e); done(1); }

async function assets() {
  // the cell holds the 0..100 box plus the halo's 9 units on each side, 128 px a cell, 16 cells a row
  const CELLPX = 128, COLS = 16, BOX = 120;
  const r = await evaluate(`(async () => {
    const S = await import('/ref/client/symbols.js');
    const keys = [...Object.keys(S.SYMBOLS), 'unknown'];
    const rows = Math.ceil(keys.length / ${COLS});
    const cv = document.createElement('canvas'); cv.width = ${COLS * CELLPX}; cv.height = rows * 2 * ${CELLPX};
    const c = cv.getContext('2d');
    const size = ${CELLPX} * 100 / ${BOX};
    keys.forEach((k, i) => {
      const x = (i % ${COLS} + 0.5) * ${CELLPX}, y = (Math.floor(i / ${COLS}) + 0.5) * ${CELLPX};
      const type = k === 'unknown' ? '\\u0000unknown' : k; // a key no table holds: drawSymbol falls back to UNKNOWN
      // fills: drawSymbol with no halo, in white
      S.drawSymbol(c, type, x, y, size, { color: '#ffffff' });
      // halos: drawSymbol's own halo stroke, then the fill cut back out of it is NOT done: the halo stays whole under
      // where the fill goes, as drawSymbol leaves it (the fill is drawn over it at run time)
      c.save(); c.beginPath(); c.rect(x - ${CELLPX} / 2, y + rows * ${CELLPX} - ${CELLPX} / 2, ${CELLPX}, ${CELLPX}); c.clip();
      S.drawSymbol(c, type, x, y + rows * ${CELLPX}, size, { color: 'rgba(0,0,0,0)', halo: '#ffffff', haloWidth: 18 });
      c.restore();
    });
    return { keys, rows, png: cv.toDataURL('image/png'), d: Object.fromEntries(keys.map((k) => [k, k === 'unknown' ? S.GLYPHS.unknown : S.SYMBOLS[k].d])) };
  })()`);
  const atlasSha = savePng(r.png, path.join(work, 'symbols.png'));
  // shared/sim.js numbers, read from the module itself in Node (sim.js runs in Node: the server imports it)
  const sim = await import(path.join(REF, 'shared', 'sim.js'));
  const sizes = Object.fromEntries(Object.entries(sim.UNITS).filter(([, d]) => d.building).map(([t, d]) => [t, d.size ?? 1]));
  const font = findFont();
  const data = {
    format: 'tc-warmap-v1', tool_version: TOOL_VERSION, reference: refPin,
    atlas: { file: 'T_TCWarMapSymbols', cell: CELLPX, columns: COLS, rows: r.rows, box: BOX, halo_rows_offset: r.rows, keys: r.keys },
    sim: { CELL: sim.CELL, pointRadius: sim.CFG.pointRadius, levelHeight: sim.CFG.levelHeight, buildingSize: sizes },
    font,
    sources: Object.fromEntries(['client/symbols.js', 'client/map-view.js', 'client/fog.js', 'shared/sim.js'].map((f) => [f, shaFile(path.join(REF, f))])),
    outputs: { 'symbols.png': atlasSha },
    symbols_sha256: sha(JSON.stringify(r.d)),
  };
  fs.writeFileSync(path.join(work, 'WarMapData.json'), JSON.stringify(data, null, 1) + '\n');
  console.log(`assets: ${r.keys.length} symbols in ${COLS} x ${r.rows * 2} cells, font ${font.family} ${font.style} (${font.file})`);
}

// the face Chromium uses for map-view.js's "bold <n>px "Courier New", monospace" here: fontconfig's match
function findFont() {
  const out = execFileSync('fc-match', ['-f', '%{file}\\n%{family}\\n%{style}\\n', 'Courier New:bold']).toString().trim().split('\n');
  const [file, family, style] = out;
  const dir = path.join(work, 'font');
  fs.mkdirSync(dir, { recursive: true });
  const dst = path.join(dir, path.basename(file));
  fs.copyFileSync(file, dst);
  const licence = ['LICENSE', 'OFL.txt', 'COPYING', 'License.txt'].map((n) => path.join(path.dirname(file), n)).find((p) => fs.existsSync(p))
    ?? ['/usr/share/licenses/ttf-liberation/LICENSE', '/usr/share/licenses/ttf-liberation/OFL.txt', '/usr/share/doc/fonts-liberation/copyright'].find((p) => fs.existsSync(p));
  if (licence) fs.copyFileSync(licence, path.join(dir, 'LICENSE-' + path.basename(file, path.extname(file)) + '.txt'));
  return { query: 'Courier New:bold', file: path.basename(file), family, style: style.split(',')[0], sha256: shaFile(file), licence: licence ? path.basename(licence) : null };
}

async function oracle() {
  if (!threeDir) throw new Error('--three <dir with three.module.js> is needed');
  const maps = arg('maps', 'seelow-heights,crossroads-village').split(',');
  const dir = path.join(work, 'oracle');
  fs.mkdirSync(dir, { recursive: true });
  for (const name of maps) {
    const map = JSON.parse(fs.readFileSync(path.join(REF, 'maps', name + '.json')));
    const r = await evaluate(`(async () => {
      const THREE = await import('three');
      const { createMapView } = await import('/ref/client/map-view.js');
      const sim = await import('/ref/shared/sim.js');
      const map = ${JSON.stringify({ w: map.w, h: map.h, rows: map.rows, heights: map.heights, name: map.name })};
      const grid = map.rows.map((r) => [...r]);
      const hAt = (x, z) => sim.levelOf(map.heights[Math.min(map.h - 1, Math.max(0, Math.floor(z / sim.CELL)))][Math.min(map.w - 1, Math.max(0, Math.floor(x / sim.CELL)))]) * sim.CFG.levelHeight;
      const camera = new THREE.PerspectiveCamera(42, 16 / 9, 1, 3000);
      camera.position.set(map.w, 300, map.h * 2 + 200); camera.lookAt(map.w, 0, map.h); camera.updateMatrixWorld();
      const view = document.createElement('canvas'); document.body.appendChild(view);
      const mv = createMapView({ grid, w: map.w, h: map.h, geometry: new THREE.PlaneGeometry(1, 1), hAt, fog: null, units: new Map(), colorOf: () => '#3b73d6',
        title: map.name, camera, view, state: () => ({ me: 0, selected: new Set(), points: [], strikes: [] }) });
      mv.frame(1, 1); // zoom 1: the fade jumps in, and the first frame paints (dirty, never painted)
      const [desk, paper] = mv.object.children.map((m) => m.material.map?.image);
      return { w: map.w, h: map.h, W: paper.width, H: paper.height, paper: paper.toDataURL('image/png'), desk: desk.toDataURL('image/png'), fade: mv.fade };
    })()`);
    const ps = savePng(r.paper, path.join(dir, `${name}-paper.png`)), ds = savePng(r.desk, path.join(dir, `${name}-desk.png`));
    fs.writeFileSync(path.join(dir, `${name}.json`), JSON.stringify({ map: name, w: r.w, h: r.h, W: r.W, H: r.H, PX: r.W / r.w, fade: r.fade, reference: refPin, paper_sha256: ps, desk_sha256: ds, hAt: 'level x levelHeight of the cell (nearest)' }, null, 1) + '\n');
    console.log(`oracle ${name}: paper ${r.W} x ${r.H} (PX ${r.W / r.w}), desk 2048`);
  }
}
