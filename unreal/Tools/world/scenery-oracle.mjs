// The oracle for the native map pieces and scenery (unreal/Source/ThreeCrossroads/World/TCWorldObjects.h, TCScatter.h):
// runs the pinned reference's own client/structures.js buildStructures() and client/props.js candidates() /
// createProps() under Node, unmodified, and writes what they place (every instance matrix and tint) together with the
// inputs they were given, so the native port can be checked against them without a browser.
//
//   WW2_MODEL_DEPS=<dir with node_modules/three> node --import ./unreal/Tools/models/register.mjs \
//     unreal/Tools/world/scenery-oracle.mjs [--reference reference/ww2-rts] [--out unreal/Tools/world/fixtures/scenery-oracle.json]
//
// WW2_MODEL_DEPS is the three.js the models exporter installed from the pin's own package-lock (see
// unreal/Tools/models/README.md). The browser-only APIs the reference touches at module load (document, Image) are
// shimmed inertly by unreal/Tools/models/shims.mjs; nothing is drawn. Each case is self-contained: the native test
// rebuilds the same inputs from the fixture alone. The ground is a fixed analytic height (WAVE below), the same
// function the native test uses, so heights are exercised without the relief.
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join, resolve, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { installShims, settle } from '../models/shims.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const opt = (name, def) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : def; };
const REF = resolve(opt('--reference', join(HERE, '../../../reference/ww2-rts')));
const OUT = resolve(opt('--out', join(HERE, 'fixtures/scenery-oracle.json')));

installShims();
const imp = (p) => import(pathToFileURL(join(REF, p)).href);
const THREE = await import('three');
const { mapForClient } = await imp('shared/world-layers.js');
const structures = await imp('client/structures.js');
const props = await imp('client/props.js');
const { gfx } = await imp('client/gfx.js');
await settle();

// The analytic ground both sides use (metres): a gentle swell, so frames take their lowest corner and props sit on it.
const WAVE = (x, z) => 0.6 * Math.sin(x * 0.13) + 0.4 * Math.cos(z * 0.09) + 0.002 * x;
const r6 = (v) => Math.round(v * 1e6) / 1e6;
const arr = (a) => Array.from(a, r6);

const sha = (p) => createHash('sha256').update(readFileSync(join(REF, p))).digest('hex');
const sources = Object.fromEntries(['client/structures.js', 'client/props.js', 'client/foliage.js', 'client/ground.js', 'shared/sim.js', 'shared/world-layers.js'].map(p => [p, sha(p)]));

function piecesOf({ live, orig, cells, buildings, mineGrid }) {
  const group = new THREE.Group();
  structures.buildStructures(group, live.map(r => [...r]), orig, WAVE, cells, buildings, mineGrid);
  const out = [];
  for (const im of group.children) {
    const n = im.count, m = im.instanceMatrix.array, c = im.instanceColor.array;
    const inst = [];
    for (let i = 0; i < n; i++) inst.push([...arr(m.subarray(i * 16, i * 16 + 16)), ...arr(c.subarray(i * 3, i * 3 + 3))]);
    out.push({ kind: im.name, instances: inst });
  }
  return out;
}

function propsOf(map, grid, nodes, skipCells) {
  const list = props.candidates(map);
  const skip = new Set(skipCells);
  const parent = new THREE.Group();
  const p = props.createProps({ map, grid: grid.map(r => [...r]), hAt: WAVE, parent, prepared: list, skip: c => skip.has(c) });
  if (nodes.length) p.setNodes(nodes);
  const cand = list.map(c => [c.kind, c.cx, c.cy, r6(c.x), r6(c.z), c.seed, r6(c.angle), r6(c.scale), c.anchor ? c.anchor[0] : -1, c.anchor ? c.anchor[1] : -1,
    c.biome ?? 255, c.orchard ? 1 : 0, c.wood ? 1 : 0]);
  const placed = list.map(c => (c.at ? [c.at, ...arr(c.m), ...arr(c.col.toArray()), r6(c.bark.r)] : null));
  const meshes = [];
  parent.traverse(o => { if (o.isInstancedMesh) meshes.push([o.name, o.count]); });
  p.dispose();
  return { candidates: cand, placed, meshes };
}

// a map as the client gets it in a start (server: shared/world-layers.js mapForClient)
const load = (file) => mapForClient(JSON.parse(readFileSync(join(REF, 'maps', file), 'utf8')));
const inputOf = (map, extra = {}) => ({ w: map.w, h: map.h, rows: map.rows, heights: map.heights ?? null, spawns: map.spawns ?? [], points: (map.points ?? []).map(p => ({ x: p.x, y: p.y })),
  buildings: map.buildings ?? [], ...extra });

const cases = [];
function addCase(name, map, { live = map.rows, orig = map.rows, cells = null, buildings = map.buildings ?? [], mines = [], nodes = [], skip = [], low = false, world = null, piecesToo = true, propsToo = true } = {}) {
  gfx.set(low ? 'low' : 'high');
  const mineGrid = mines.length ? Array.from({ length: map.h }, (_, y) => Array.from({ length: map.w }, (_, x) => (mines.includes(y * map.w + x) ? 1 : 0))) : null;
  const pmap = world ? { ...map, ...world } : map;
  const c = {
    name, low,
    input: inputOf(map, { live, orig, cells: cells ? Array.from(cells) : null, buildingsUsed: buildings, mines, nodes, skip,
      biome: world?.biome ? Array.from(world.biome) : null, world: !!world?.world, discovered: world?.discovered ? Array.from(world.discovered).join('') : null }),
  };
  if (piecesToo) c.pieces = piecesOf({ live, orig, cells, buildings, mineGrid });
  if (propsToo) c.props = propsOf(pmap, live, nodes, skip);
  cases.push(c);
  gfx.set('high');
}

// authored maps: villages, bocage, a factory town, forest with named buildings, a river with stone bridges, trenches
for (const file of ['default.json', 'crossroads-village.json', 'bocage.json', 'stalingrad-factory.json', 'hurtgen-forest.json', 'the-great-bridge.json', 'no-mans-land.json']) {
  const map = load(file);
  addCase(`${file} as drawn`, map, { nodes: map.points.slice(0, 2).map(p => [(p.x + 0.5) * 2 + 9, (p.y + 0.5) * 2 + 3]) });
}
{
  // ruins: alternate house cells shot to rubble (the live grid only; the map file keeps its houses)
  const map = load('crossroads-village.json');
  addCase('crossroads-village.json ruins', map, { live: map.rows.map((r, y) => [...r].map((ch, x) => (ch === 'B' && (x + y) % 2 === 0 ? 'R' : ch)).join('')), propsToo: false });
  // every cell damaged (stage 1), then nearly gone (stage 2)
  addCase('crossroads-village.json stage 1', map, { cells: new Uint8Array(map.w * map.h).fill(1 << 3), propsToo: false });
  addCase('bocage.json stage 2', load('bocage.json'), { cells: new Uint8Array(96 * 96).fill(2 << 3), propsToo: false });
  // map walls as built in play (sandbags) and known mines
  const nml = load('no-mans-land.json');
  const mines = [];
  for (let c = 0; c < nml.w * nml.h; c += 97) if (nml.rows[Math.floor(c / nml.w)][c % nml.w] === '.') mines.push(c);
  addCase("no-mans-land.json '#' as sandbags, mines", nml, { orig: nml.rows.map(r => r.replaceAll('#', '.')), mines, propsToo: false });
  // landmarks: a mine head and a factory on the first two house blocks, a church on a third
  const sf = load('stalingrad-factory.json'), flat = sf.rows.join(''), bs = [];
  for (let c = 0, k = 0; c < flat.length && k < 3; c++) if (flat[c] === 'B' && !bs.some(b => Math.abs(b.x - c % sf.w) + Math.abs(b.y - Math.floor(c / sf.w)) < 12)) bs.push({ x: c % sf.w, y: Math.floor(c / sf.w), kind: ['mine head', 'factory', 'church'][k++] });
  addCase('stalingrad-factory.json landmarks', sf, { buildingsUsed: bs, buildings: bs, propsToo: false });
  // Low graphics (gfx.low): fewer shrubs and chunks, no grass, odd seeds thinned
  addCase('bocage.json low', load('bocage.json'), { low: true });
}
{
  // World Conquest: region looks (BIOME_LOOKS) on part of the map, only discovered cells dressed, railway cells skipped
  const map = load('the-polder.json'), w = map.w, h = map.h, n = w * h;
  const biome = new Uint8Array(n), discovered = new Uint8Array(n);
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    const i = y * w + x;
    biome[i] = (x + y) % 37 === 0 ? 255 : (Math.floor(x / 17) + 2 * Math.floor(y / 13)) % 6;
    discovered[i] = x < w * 0.8 && y > 4 ? 1 : 0;
  }
  const rows = map.rows.map((r, y) => [...r].map((ch, x) => (discovered[y * w + x] ? ch : '.')).join(''));
  const wmap = { ...map, rows };
  const skip = [];
  for (let x = 10; x < 60; x++) skip.push(40 * w + x);
  addCase('the-polder.json world regions', wmap, { world: { world: true, biome, discovered }, skip, piecesToo: false });
}

const fixture = { format: 'tc-scenery-oracle-v1', reference: 'e4e579a', node: process.version, three: THREE.REVISION, sources, wave: '0.6*sin(x*0.13)+0.4*cos(z*0.09)+0.002*x', cases };
writeFileSync(OUT, JSON.stringify(fixture));
let pieces = 0, placed = 0;
for (const c of cases) { for (const k of c.pieces ?? []) pieces += k.instances.length; for (const p of c.props?.placed ?? []) if (p) placed++; }
console.log(`wrote ${OUT}: ${cases.length} cases, ${pieces} piece instances, ${placed} placed props, ${readFileSync(OUT).length} bytes`);
