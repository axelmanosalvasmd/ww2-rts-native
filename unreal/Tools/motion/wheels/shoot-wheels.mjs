// The reference half of the wheel and track comparison: unreal/Tools/look's page (look.html, look.mjs: the pinned
// builders under the game's light) with one addition, made here on a copy in the work folder: after makeUnit has
// built and animated a unit, every wheel mesh's wheelTravel attribute (client/wheel-motion.js wheelMesh) is set to
// ?wheelTravel=<metres>,<radians>, as moveWheels would after that much driving. Nothing in reference/ww2-rts or
// unreal/Tools/look is edited; the reference must be clean (as shoot.mjs requires).
//   node unreal/Tools/motion/wheels/shoot-wheels.mjs --three <deps dir> --out <dir> --work <dir> --assets tank:0:0 --travel 0.37,0.25
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { spawn, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '../../../..');
const reference = path.join(repo, 'reference/ww2-rts');
const lookDir = path.join(repo, 'unreal/Tools/look');
const arg = (name, d) => { const i = process.argv.indexOf(`--${name}`); return i > 0 ? process.argv[i + 1] : d; };
const three = arg('three'), out = arg('out'), work = arg('work'), assets = arg('assets', 'tank:0:0'), travel = arg('travel', '0,0');
const chromium = arg('chromium', '/usr/bin/chromium'), timeout = +arg('timeout', 900);
if (!three || !out || !work) { console.error('usage: shoot-wheels.mjs --three <deps> --out <dir> --work <dir> [--assets ..] [--travel m,rad]'); process.exit(2); }
const pin = execFileSync('git', ['-C', reference, 'rev-parse', 'HEAD']).toString().trim();
if (execFileSync('git', ['-C', reference, 'status', '--porcelain']).toString().trim()) { console.error('reference/ww2-rts has local changes; refusing'); process.exit(1); }
fs.mkdirSync(out, { recursive: true });
const page = path.join(work, 'look');
fs.mkdirSync(page, { recursive: true });
fs.copyFileSync(path.join(lookDir, 'look.html'), path.join(page, 'look.html'));
const source = fs.readFileSync(path.join(lookDir, 'look.mjs'), 'utf8');
const hook = '  animate(v, 10, new THREE.Vector3(0, 30, 30));\n';
if (!source.includes(hook)) { console.error('look.mjs changed: the makeUnit hook is not there'); process.exit(1); }
const patched = source.replace(hook, hook + `  { // shoot-wheels.mjs: wheel-motion.js moveWheels' result after driving, set by hand
    const [t, r] = (new URLSearchParams(location.search).get('wheelTravel') ?? '0,0').split(',').map(Number);
    let n = 0;
    root.traverse((m) => { if (m.userData.wheelTravel) { m.userData.wheelTravel.setXY(0, t, r); m.userData.wheelTravel.needsUpdate = true; n++; } });
    post('/log', \`\${type}: wheelTravel \${t}, \${r} on \${n} meshes\`);
  }
`);
fs.writeFileSync(path.join(page, 'look.mjs'), patched);
const roots = { '/client/': path.join(reference, 'client'), '/shared/': path.join(reference, 'shared'),
  '/vendor/': path.join(three, 'node_modules/three/build'), '/vendor-jsm/': path.join(three, 'node_modules/three/examples/jsm'), '/look/': page };
const types = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.html': 'text/html', '.json': 'application/json', '.jpg': 'image/jpeg', '.png': 'image/png', '.webp': 'image/webp', '.glb': 'model/gltf-binary', '.bin': 'application/octet-stream' };
const log = [];
let finish;
const done = new Promise((r) => (finish = r));
const server = http.createServer((req, res) => {
  const url = new URL(req.url, 'http://x');
  if (req.method === 'POST') {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => {
      const body = Buffer.concat(chunks);
      if (url.pathname === '/save') fs.writeFileSync(path.join(out, path.basename(url.searchParams.get('name'))), body);
      else if (url.pathname === '/log') { log.push(body.toString()); console.log('page:', body.toString()); }
      else if (url.pathname === '/done') finish(body.toString());
      res.end('ok');
    });
    return;
  }
  const root = Object.keys(roots).find((r) => url.pathname.startsWith(r));
  const file = root && path.join(roots[root], decodeURIComponent(url.pathname.slice(root.length)));
  if (!file || !file.startsWith(roots[root]) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) { res.statusCode = 404; res.end(); return; }
  res.setHeader('content-type', types[path.extname(file)] ?? 'application/octet-stream');
  fs.createReadStream(file).pipe(res);
});
server.listen(0, '127.0.0.1', () => {
  const port = server.address().port;
  const url = `http://127.0.0.1:${port}/look/look.html?assets=${assets}&wheelTravel=${travel}`;
  const browser = spawn(chromium, ['--headless=new', `--user-data-dir=${path.join(work, 'profile')}`, '--no-first-run', '--no-default-browser-check', '--disable-extensions',
    '--disable-background-networking', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--window-size=1280,720', '--hide-scrollbars', url], { stdio: ['ignore', 'ignore', 'pipe'] });
  browser.stderr.on('data', () => {});
  const timer = setTimeout(() => finish('timeout'), timeout * 1000);
  done.then((result) => {
    clearTimeout(timer);
    browser.kill('SIGTERM');
    server.close();
    fs.writeFileSync(path.join(out, 'reference-shoot.json'), JSON.stringify({ reference: pin, result, url, travel, log, chromium: execFileSync(chromium, ['--version']).toString().trim() }, null, 1));
    console.log('result', result);
    process.exit(result === 'ok' ? 0 : 1);
  });
});
