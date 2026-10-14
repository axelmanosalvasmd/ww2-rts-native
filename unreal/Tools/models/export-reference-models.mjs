// Exports every model the pinned reference client renders to glTF 2.0 binaries, by running the reference's own
// builders under Node (no re-modelling). Output: unreal/Saved/ModelExport/{units,planes,structures,pieces,foliage,
// props,source,textures}/... and manifest.json. See README.md in this folder for the attribute mapping and axes.
//
// Usage: node unreal/Tools/models/export-reference-models.mjs --deps DIR [--out DIR] [--reference DIR] [--jobs N]
//   --deps: a scratch directory for the pin's own three.js; filled with `npm ci --ignore-scripts` from the pin's
//           package.json and package-lock.json when node_modules/three is missing (env WW2_MODEL_DEPS also works)
//   --only REGEX: export only matching asset ids (debugging; the manifest then covers only those)
import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync, rmSync, readdirSync, copyFileSync, statSync } from 'node:fs';
import { join, resolve, relative, dirname } from 'node:path';
import { SHIMS } from './shims.mjs';

const here = import.meta.dirname, repo = resolve(here, '../../..');
// this exporter's own files (other tools share the folder), hashed into manifest.generator
const TOOL_FILES = ['authored-rig.mjs', 'build-worker.mjs', 'catalog.mjs', 'compose.mjs', 'export-reference-models.mjs', 'extract.mjs', 'gltf-writer.mjs', 'layers.mjs', 'register.mjs', 'roundtrip-check.mjs', 'shims.mjs', 'three-resolve.mjs', 'validate-models.mjs', 'README.md'];
const argv = process.argv.slice(2), opt = (k, d) => { const i = argv.indexOf(`--${k}`); return i >= 0 ? argv[i + 1] : d; };
const reference = resolve(opt('reference', join(repo, 'reference/ww2-rts')));
const out = resolve(opt('out', join(repo, 'unreal/Saved/ModelExport')));
const deps = resolve(opt('deps', process.env.WW2_MODEL_DEPS ?? ''));
const jobs = Number(opt('jobs', 4)), only = opt('only', null);
if (!process.env.WW2_MODEL_DEPS && !argv.includes('--deps')) throw new Error('pass --deps DIR (a scratch directory for the pin\'s three.js) or set WW2_MODEL_DEPS');
const sha = (buf) => createHash('sha256').update(buf).digest('hex');
const git = (...a) => execFileSync('git', ['-C', reference, ...a], { encoding: 'utf8', maxBuffer: 1 << 30 }).trim();

// 1. the pin: its commit, a clean tree, and the sha256 of every file read
const pin = git('rev-parse', 'HEAD');
const dirty = git('status', '--porcelain');
if (dirty) throw new Error(`reference/ww2-rts has local changes; refusing to export from an unpinned tree:\n${dirty}`);
// 2. the pin's three.js, in a scratch directory (reference/ is read only and has no node_modules)
mkdirSync(deps, { recursive: true });
for (const f of ['package.json', 'package-lock.json']) {
  const want = git('show', `${pin}:${f}`);
  if (!existsSync(join(deps, f)) || readFileSync(join(deps, f), 'utf8').trim() !== want) writeFileSync(join(deps, f), want + '\n');
}
if (!existsSync(join(deps, 'node_modules/three/package.json'))) execFileSync('npm', ['ci', '--ignore-scripts', '--no-audit', '--no-fund'], { cwd: deps, stdio: 'inherit' });
const threeVersion = JSON.parse(readFileSync(join(deps, 'node_modules/three/package.json'), 'utf8')).version;
const lockThree = JSON.parse(readFileSync(join(deps, 'package-lock.json'), 'utf8')).packages['node_modules/three'].version;
if (threeVersion !== lockThree) throw new Error(`three ${threeVersion} in ${deps} is not the pin's ${lockThree}`);

// 3. workers: ten color looks in parallel, then the writer (README.md "Looks and tints")
const work = join(out, '.work');
rmSync(out, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
const env = { ...process.env, WW2_MODEL_DEPS: deps };
const node = (args, label) => new Promise((ok, fail) => {
  const p = spawn(process.execPath, ['--no-warnings', '--max-old-space-size=12288', '--import', join(here, 'register.mjs'), join(here, 'build-worker.mjs'), '--reference', reference, ...(only ? ['--only', only] : []), ...args], { env, stdio: ['ignore', 'pipe', 'pipe'] });
  let log = '';
  p.stdout.on('data', (d) => { log += d; if (label === 'write') process.stdout.write(d); });
  p.stderr.on('data', (d) => { log += d; });
  p.on('exit', (code) => (code === 0 ? ok(log) : fail(new Error(`${label} worker exited ${code}:\n${log.slice(-4000)}`))));
});
const looks = ['slot0', 'slot1', 'slot2', 'slot3', 'slot4', 'slot5', 'o1', 'o2', 'v2', 'v3'];
const queue = [...looks];
await Promise.all(Array.from({ length: Math.min(jobs, queue.length) }, async () => {
  while (queue.length) { const l = queue.shift(); await node(['--mode', 'colors', '--look', l, '--out', join(work, `colors-${l}.bin`)], l); console.log(`colors ${l} done`); }
}));
await node(['--mode', 'write', '--work', work, '--out-dir', out], 'write');
const { results: assets, aliases } = JSON.parse(readFileSync(join(work, 'assets.json'), 'utf8'));

// 4. source art passed through and textures referenced by the materials, copied with their hashes
const copied = [];
const copy = (src, dst, note) => {
  const from = join(reference, src), to = join(out, dst);
  mkdirSync(dirname(to), { recursive: true }); copyFileSync(from, to);
  const buf = readFileSync(to);
  copied.push({ source: src, file: dst, sha256: sha(buf), bytes: buf.length, note });
};
if (!only) {
  copy('art/model-reference-2026-10-03/infantry-retopo-v3.glb', 'source/infantry-retopo-v3.glb', 'Blender retopology source of the authored infantry, byte-identical copy (already glTF 2.0)');
  for (const f of readdirSync(join(reference, 'client/textures')).sort()) if (statSync(join(reference, 'client/textures', f)).isFile()) copy(`client/textures/${f}`, `textures/${f}`, 'texture referenced by material extras');
  for (const f of readdirSync(join(reference, 'client/textures/models')).sort()) copy(`client/textures/models/${f}`, `textures/models/${f}`, 'model texture layer or infantry atlas');
}

// 5. sha256 of every reference file named by an asset (at the pin, checked against the clean working tree)
const sources = {};
const fileSha = (path) => {
  if (sources[path]) return sources[path];
  const blob = execFileSync('git', ['-C', reference, 'show', `${pin}:${path}`], { maxBuffer: 1 << 30 });
  if (sha(blob) !== sha(readFileSync(join(reference, path)))) throw new Error(`${path} differs from the pin`);
  return (sources[path] = sha(blob));
};
for (const a of assets) {
  const mods = new Set([a.source.module, ...(a.source.modules ?? []), ...(a.extras?.dispatch?.modules ?? [])]);
  a.source = { ...a.source, files: Object.fromEntries([...mods].sort().map((m) => [m, fileSha(m)])) };
}
for (const f of ['client/main.js', 'shared/sim.js', 'client/models/geom.js', 'client/model-textures.js', 'client/surfaces.js', 'client/track-motion.js', 'client/wheel-motion.js', 'client/infantry-material.js', 'package-lock.json']) fileSha(f);
for (const c of copied) c.sourceSha256 = fileSha(c.source);

const { readLayers } = await import('./layers.mjs');
const manifest = {
  format: 'ww2-reference-models-v1',
  reference: { path: relative(repo, reference), commit: pin, three: threeVersion },
  generator: { tool: 'unreal/Tools/models/export-reference-models.mjs', node: process.version, files: Object.fromEntries(TOOL_FILES.map((f) => [f, sha(readFileSync(join(here, f)))])) },
  space: { gltf: '+Y up, metres, right-handed (identical to three.js; no conversion applied)', forward: 'unit models face +X, +Z is their right', unreal: 'Interchange glTF: UE (X, Y, Z) = glTF (x, z, y) x 100 (GLTFCore ConversionUtilities.h ConvertVec3, InterchangeGltfPrivate.h GltfUnitConversionMultiplier = 100); +X forward stays UE +X, +Z right becomes UE +Y, +Y up becomes UE +Z' },
  shims: SHIMS,
  matLayers: readLayers(reference),
  sources,
  copied,
  aliases,
  assets,
};
writeFileSync(join(out, 'manifest.json'), JSON.stringify(manifest, null, 1) + '\n');
rmSync(work, { recursive: true, force: true });
const failed = assets.filter((a) => a.error);
console.log(`${assets.length - failed.length} assets exported, ${failed.length} failed, ${assets.reduce((n, a) => n + (a.files?.length ?? 0), 0)} files, manifest ${join(out, 'manifest.json')}`);
