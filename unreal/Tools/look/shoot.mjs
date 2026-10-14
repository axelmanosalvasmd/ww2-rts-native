// Serves look.html with the pinned reference's client/ and shared/ (read only) and three from the pin's package-lock,
// opens it in headless Chromium with a throwaway profile, and saves what the page posts back (the reference pictures,
// masks and plates, env-pmrem.f32 and cameras.json) into the output folder. The model comparison's runner
// (../models/compare/shoot-reference.mjs), pointed at this page.
//
//   node unreal/Tools/look/shoot.mjs --three <dir with node_modules/three> --out <dir> [--assets ...] [--profile <dir>]
//        [--chromium /usr/bin/chromium] [--timeout 900]
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { spawn, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '../../..');
const reference = path.join(repo, 'reference/ww2-rts');
const arg = (name, d) => { const i = process.argv.indexOf(`--${name}`); return i > 0 ? process.argv[i + 1] : d; };
const three = arg('three'), out = arg('out'), profile = arg('profile', path.join(out ?? '.', '.chromium-profile'));
const chromium = arg('chromium', '/usr/bin/chromium'), assets = arg('assets', ''), timeout = +arg('timeout', 900);
if (!three || !out) { console.error('usage: shoot.mjs --three <deps dir> --out <dir> [--assets ...]'); process.exit(2); }
const pin = execFileSync('git', ['-C', reference, 'rev-parse', 'HEAD']).toString().trim();
const dirty = execFileSync('git', ['-C', reference, 'status', '--porcelain']).toString().trim();
if (dirty) { console.error('reference/ww2-rts has local changes; refusing'); process.exit(1); }
const threeVersion = JSON.parse(fs.readFileSync(path.join(three, 'node_modules/three/package.json'), 'utf8')).version;
fs.mkdirSync(out, { recursive: true });
const roots = { '/client/': path.join(reference, 'client'), '/shared/': path.join(reference, 'shared'),
  '/vendor/': path.join(three, 'node_modules/three/build'), '/vendor-jsm/': path.join(three, 'node_modules/three/examples/jsm'), '/look/': here };
const types = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.html': 'text/html', '.json': 'application/json', '.jpg': 'image/jpeg', '.png': 'image/png', '.webp': 'image/webp', '.glb': 'model/gltf-binary', '.bin': 'application/octet-stream' };
const log = [];
let finish, saved = 0;
const done = new Promise((r) => (finish = r));
const server = http.createServer((req, res) => {
  const url = new URL(req.url, 'http://x');
  if (req.method === 'POST') {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => {
      const body = Buffer.concat(chunks);
      if (url.pathname === '/save') {
        const name = path.basename(url.searchParams.get('name'));
        fs.writeFileSync(path.join(out, name), body);
        saved++;
        if (!name.endsWith('.png') || saved % 25 === 0) console.log('saved', name, `(${saved})`);
      } else if (url.pathname === '/log') { log.push(body.toString()); console.log('page:', body.toString()); }
      else if (url.pathname === '/done') finish(body.toString());
      res.end('ok');
    });
    return;
  }
  const root = Object.keys(roots).find((r) => url.pathname.startsWith(r));
  const file = root && path.join(roots[root], decodeURIComponent(url.pathname.slice(root.length)));
  if (!file || !file.startsWith(roots[root]) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) { res.statusCode = 404; res.end(); console.log('404', url.pathname); return; }
  res.setHeader('content-type', types[path.extname(file)] ?? 'application/octet-stream');
  fs.createReadStream(file).pipe(res);
});
server.listen(0, '127.0.0.1', () => {
  const port = server.address().port;
  const page = `http://127.0.0.1:${port}/look/look.html${assets ? `?assets=${assets}` : ''}`;
  console.log('reference', pin, 'three', threeVersion, 'page', page);
  const browser = spawn(chromium, ['--headless=new', `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check', '--disable-extensions', '--disable-background-networking',
    '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--window-size=1280,720', '--hide-scrollbars', page], { stdio: ['ignore', 'ignore', 'pipe'] });
  browser.stderr.on('data', () => {});
  const timer = setTimeout(() => finish('timeout'), timeout * 1000);
  done.then((result) => {
    clearTimeout(timer);
    browser.kill('SIGTERM');
    server.close();
    fs.writeFileSync(path.join(out, 'reference-shoot.json'), JSON.stringify({ reference: pin, three: threeVersion, result, page, saved, chromium: execFileSync(chromium, ['--version']).toString().trim(), log }, null, 1));
    console.log('result', result, 'saved', saved);
    process.exit(result === 'ok' ? 0 : 1);
  });
});
