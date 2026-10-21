// The Graphics Low oracle (round 15): runs the pinned reference's own client/props.js, client/structures.js and
// client/gfx.js under Node, unmodified, on authored maps, and records what Graphics Low leaves out and what a switch of
// level in the middle of a match does (gfx.set fires gfx.onChange, which props.js and structures.js subscribe to):
//
//   per map, per scenery candidate (props.js candidates(map)): the kind drawn (c.at) on High, after gfx.set('low')
//   with the props already built (the mid-match switch), freshly built on Low, and after switching back to High;
//   per instanced mesh of props and pieces: its count and castShadow on each of those levels;
//   ground.js createGround's texels per cell (the P expression, read from the pinned source and evaluated for the map
//   and both levels, so the native number is checked against the reference's own text, not a copy of it).
//
//   WW2_MODEL_DEPS=<dir with node_modules/three> node --import ./unreal/Tools/models/register.mjs \
//     unreal/Tools/graphics/gfx-low-oracle.mjs [--reference reference/ww2-rts] [--out unreal/Tools/graphics/fixtures/gfx-low-oracle.json]
//
// The ground is the scenery oracle's analytic height (unreal/Tools/world/scenery-oracle.mjs WAVE), so the native test
// can rebuild the same inputs from this fixture alone. Supply nodes: the first two capture points, as that oracle.
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join, resolve, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { installShims, settle } from '../models/shims.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const opt = (name, def) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : def; };
const REF = resolve(opt('--reference', join(HERE, '../../../reference/ww2-rts')));
const OUT = resolve(opt('--out', join(HERE, 'fixtures/gfx-low-oracle.json')));
const MAPS = (opt('--maps', 'seelow-heights,crossroads-village,hurtgen-forest,ardennes-crossing,bocage,pegasus-bridge')).split(',');

installShims();
const imp = (p) => import(pathToFileURL(join(REF, p)).href);
const THREE = await import('three');
const { mapForClient } = await imp('shared/world-layers.js');
const structures = await imp('client/structures.js');
const props = await imp('client/props.js');
const { gfx } = await imp('client/gfx.js');
await settle();

const WAVE = (x, z) => 0.6 * Math.sin(x * 0.13) + 0.4 * Math.cos(z * 0.09) + 0.002 * x;
const sha = (p) => createHash('sha256').update(readFileSync(join(REF, p))).digest('hex');
const sources = Object.fromEntries(['client/gfx.js', 'client/props.js', 'client/structures.js', 'client/ground.js', 'client/foliage.js', 'shared/sim.js', 'shared/world-layers.js'].map(p => [p, sha(p)]));

// ground.js createGround: `const P = <expr>;` read from the pinned file and evaluated as written
const groundSrc = readFileSync(join(REF, 'client/ground.js'), 'utf8');
const body = groundSrc.slice(groundSrc.indexOf('export function createGround('));
const pm = body.match(/\n\s*const P = ([^;\n]+);/);
if (!pm) throw new Error('ground.js createGround: no `const P = ...;` line');
const texelsOf = new Function('map', 'gfx', `return ${pm[1]};`);

const load = (file) => mapForClient(JSON.parse(readFileSync(join(REF, 'maps', file), 'utf8')));
const meshList = (parent) => { const out = []; parent.traverse(o => { if (o.isInstancedMesh) out.push([o.name, o.count, o.castShadow ? 1 : 0]); }); return out; };
const ats = (list) => list.map(c => c.at ?? null);

const cases = [];
for (const name of MAPS) {
  const map = load(`${name}.json`);
  const nodes = map.points.slice(0, 2).map(p => [(p.x + 0.5) * 2 + 9, (p.y + 0.5) * 2 + 3]);
  const grid = () => map.rows.map(r => [...r]);

  // scenery: built on High, then the level switched mid-match (props.js subscribes refresh() to gfx.onChange)
  gfx.set('high');
  const list = props.candidates(map);
  const parent = new THREE.Group();
  const p = props.createProps({ map, grid: grid(), hAt: WAVE, parent, prepared: list });
  p.setNodes(nodes);
  const high = ats(list), highMeshes = meshList(parent);
  gfx.set('low');
  const switched = ats(list), switchedMeshes = meshList(parent);
  gfx.set('high');
  const back = ats(list), backMeshes = meshList(parent);
  p.dispose();
  // built on Low from the start
  gfx.set('low');
  const list2 = props.candidates(map);
  const parent2 = new THREE.Group();
  const p2 = props.createProps({ map, grid: grid(), hAt: WAVE, parent: parent2, prepared: list2 });
  p2.setNodes(nodes);
  const fresh = ats(list2), freshMeshes = meshList(parent2);
  p2.dispose();
  gfx.set('high');

  // pieces: built on High, switched (structures.js: gfx.onChange rebuilds while its group has a parent), built on Low
  const root = new THREE.Group(), group = new THREE.Group();
  root.add(group);
  structures.buildStructures(group, grid(), map.rows, WAVE, null, map.buildings ?? [], null);
  const piecesHigh = meshList(group);
  gfx.set('low');
  const piecesSwitched = meshList(group);
  gfx.set('high');
  const g2 = new THREE.Group(); root.add(g2);
  gfx.set('low');
  structures.buildStructures(g2, grid(), map.rows, WAVE, null, map.buildings ?? [], null);
  const piecesLow = meshList(g2);
  gfx.set('high');

  cases.push({
    map: name, w: map.w, h: map.h, world: !!map.world,
    input: { w: map.w, h: map.h, rows: map.rows, heights: map.heights ?? null, spawns: map.spawns ?? [], points: (map.points ?? []).map(q => ({ x: q.x, y: q.y })),
      buildings: map.buildings ?? [], nodes },
    texels: { high: texelsOf(map, { low: false }), low: texelsOf(map, { low: true }) },
    candidates: list.map(c => [c.kind, c.cx, c.cy, c.seed, c.wood ? 1 : 0]),
    at: { high, switched, back, fresh },
    propMeshes: { high: highMeshes, switched: switchedMeshes, back: backMeshes, fresh: freshMeshes },
    pieceMeshes: { high: piecesHigh, switched: piecesSwitched, low: piecesLow },
  });
}

const fixture = { format: 'tc-gfx-low-oracle-v1', reference: 'e4e579a', node: process.version, three: THREE.REVISION, sources, groundTexels: pm[1],
  wave: '0.6*sin(x*0.13)+0.4*cos(z*0.09)+0.002*x', cases };
writeFileSync(OUT, JSON.stringify(fixture));
for (const c of cases) {
  const n = (a) => a.filter(Boolean).length, grass = (a) => a.filter(k => k === 'grass').length;
  console.log(`${c.map} ${c.w}x${c.h}: ${c.candidates.length} candidates; shown High ${n(c.at.high)} (grass ${grass(c.at.high)}), Low switched ${n(c.at.switched)} (grass ${grass(c.at.switched)}), Low fresh ${n(c.at.fresh)}, back ${n(c.at.back)}; texels ${c.texels.high}/${c.texels.low}`);
}
console.log(`wrote ${OUT}: ${cases.length} maps, ${readFileSync(OUT).length} bytes`);
