// The oracle for the native relief (unreal/Source/ThreeCrossroads/World/TCRelief.h): runs the pinned reference's own
// client/relief.js createRelief() (with water-levels.js), client/ground.js cellAttrs (the isRoad mask relief.js is given
// by client/main.js) and shared/sim.js levelAt / heightAt under Node, unmodified, on all 33 authored maps, two
// incremental cases, a Low-graphics case and a World Conquest sample (start, then a second scouting step through
// relief.update), and writes the inputs it gave the reference with everything the reference built:
//   <out>/<case>.json         inputs (w, h, low, levels, physical grid, road mask, known mask, frozen levels) and stats
//   <out>/<case>.<field>.bin  the drawn geometry (pos, nrm, uv, paint, scar: Float32; idx: Uint32), the sample points
//                             (pts: Float64 x, z), relief.hAt (hat), sim heightAt / levelAt x 2.5 (simh, siml), the
//                             water line at every cell centre (water), pick rays (rays: Float64 ox oy oz dx dy dz) and
//                             mesh.raycast hits (hits: Float64 x y z, NaN for a miss)
//   <out>/fixture.json        the compact fixture the engine test reads (see writeFixture)
//
//   WW2_MODEL_DEPS=<dir with node_modules/three> node --import ./unreal/Tools/relief/register.mjs \
//     unreal/Tools/relief/relief-oracle.mjs --out <dir> [--only name,name]
import { readFileSync, writeFileSync, mkdirSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join, resolve, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { installShims, settle } from '../models/shims.mjs';
import { SIM_EXPORT } from './register.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const opt = (name, def) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : def; };
const REF = resolve(opt('--reference', join(HERE, '../../../reference/ww2-rts')));
const OUT = resolve(opt('--out', join(HERE, 'out')));
const ONLY = opt('--only', '') ? new Set(opt('--only', '').split(',')) : null;
mkdirSync(OUT, { recursive: true });

installShims();
const imp = (p) => import(pathToFileURL(join(REF, p)).href);
const THREE = await import('three');
const sim = await imp('shared/sim.js');
const { createRelief } = await imp('client/relief.js');
const ground = await imp('client/ground.js');
const { mapForClient, worldLayers, composeWorldCell } = await imp('shared/world-layers.js');
const { generateWorldMap } = await imp('shared/world-conquest.js');
ground.internals.buildNoise(); // createGround() does this before its first cellAttrs
await settle();
const { CELL, CFG, levelOf, levelAt } = sim, heightAt = sim.__oracleHeightAt;
const levelChar = (n) => (n < 0 ? String.fromCharCode(96 - n) : String(n));
const sha = (p) => createHash('sha256').update(readFileSync(join(REF, p))).digest('hex');

// a small deterministic generator for sample points (not the reference's: only where to sample)
function lcg(seed) { let s = seed >>> 0; return () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) / 4294967296); }

// client/ground.js cellAttrs over this map and grid: prim === ROAD is what main.js hands relief.js as isRoad
function roadMask(map, grid) {
  const S = { w: map.w, h: map.h, map, fields: ground.fieldCells(map), state: null, groundGrid: null, objectGrid: null };
  const attrs = ground.internals.cellAttrs(S, grid);
  return Uint8Array.from(attrs.prim, (p) => (p === 4 ? 1 : 0));
}
// main.js startGame: physicalGrid = composeWorldCell(layers.ground, layers.objects)
function physicalGrid(map) {
  const layers = worldLayers(map);
  return Array.from({ length: map.h }, (_, y) => layers.ground.slice(y * map.w, (y + 1) * map.w).map((ch, x) => composeWorldCell(ch, layers.objects[y * map.w + x])));
}
const levelsOf = (map) => Int8Array.from({ length: map.w * map.h }, (_, c) => levelOf(map.heights?.[Math.floor(c / map.w)]?.[c % map.w] ?? '0'));
function setLevel(map, cell, lv) { // client/main.js setLevel
  map.heights ??= map.rows.map((r) => '0'.repeat(r.length));
  const x = cell % map.w, y = Math.floor(cell / map.w), r = map.heights[y];
  map.heights[y] = r.slice(0, x) + levelChar(lv) + r.slice(x + 1);
}

function write(name, field, typed) { writeFileSync(join(OUT, `${name}.${field}.bin`), Buffer.from(typed.buffer, typed.byteOffset, typed.byteLength)); }
const fixture = { reference: 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d', cases: [] };

// Everything one built relief shows: geometry, hAt at points, the sim's heights there, picks.
function record(name, map, grid, relief, inputs, extra = {}) {
  const { w, h } = map, n = w * h, geo = relief.geometry, at = geo.attributes;
  write(name, 'pos', at.position.array); write(name, 'nrm', at.normal.array); write(name, 'uv', at.uv.array);
  write(name, 'paint', at.reliefPaint.array); write(name, 'scar', at.reliefScar.array); write(name, 'idx', geo.index.array);
  const rnd = lcg(0x5eed ^ (w * 7919 + h));
  const pts = [];
  for (let c = 0; c < n; c++) pts.push(((c % w) + 0.5) * CELL, (Math.floor(c / w) + 0.5) * CELL); // every cell centre first
  const extraPoints = Math.min(40000, Math.max(4000, n));
  for (let i = 0; i < extraPoints; i++) {
    const k = i % 4; // uniform, on a cell edge, on a quarter node, on a corner
    let x = rnd() * w, z = rnd() * h;
    if (k === 1) { if (rnd() < 0.5) x = Math.round(x); else z = Math.round(z); }
    else if (k === 2) { x = Math.round(x * 4) / 4; z = Math.round(z * 4) / 4; }
    else if (k === 3) { x = Math.round(x); z = Math.round(z); }
    pts.push(x * CELL, z * CELL);
  }
  const P = Float64Array.from(pts), m = P.length / 2;
  const hat = new Float64Array(m), simh = new Float64Array(m), siml = new Float64Array(m);
  const g = { w, h, height: levelsOf(map) };
  for (let i = 0; i < m; i++) {
    hat[i] = relief.hAt(P[i * 2], P[i * 2 + 1]);
    simh[i] = heightAt(g, P[i * 2], P[i * 2 + 1]);
    siml[i] = levelAt(g, P[i * 2], P[i * 2 + 1]) * CFG.levelHeight;
  }
  const water = new Float64Array(n);
  for (let c = 0; c < n; c++) water[c] = relief.waterAt(((c % w) + 0.5) * CELL, (Math.floor(c / w) + 0.5) * CELL);
  write(name, 'pts', P); write(name, 'hat', hat); write(name, 'simh', simh); write(name, 'siml', siml); write(name, 'water', water);
  // pick rays as the RTS camera casts them (client/camera.js pitch 0.95 rad, heights 30 to 160 m), at random targets
  const rays = [], hits = [];
  const raycaster = new THREE.Raycaster();
  const count = Math.min(3000, Math.max(600, n >> 4));
  for (let i = 0; i < count; i++) {
    const tx = rnd() * w * CELL, tz = rnd() * h * CELL, yaw = rnd() * Math.PI * 2, pitch = 0.5 + rnd() * 0.9, dist = 30 + rnd() * 130;
    const o = new THREE.Vector3(tx + Math.sin(yaw) * Math.cos(pitch) * dist, Math.sin(pitch) * dist, tz + Math.cos(yaw) * Math.cos(pitch) * dist);
    const d = new THREE.Vector3(tx, 0, tz).sub(o).normalize();
    raycaster.set(o, d); raycaster.near = 0; raycaster.far = Infinity;
    const out = [];
    relief.mesh.raycast(raycaster, out);
    rays.push(o.x, o.y, o.z, d.x, d.y, d.z);
    if (out.length) hits.push(out[0].point.x, out[0].point.y, out[0].point.z); else hits.push(NaN, NaN, NaN);
  }
  write(name, 'rays', Float64Array.from(rays)); write(name, 'hits', Float64Array.from(hits));
  const meta = { name, w, h, low: !!inputs.low, ...extra, stats: { vertices: relief.stats.vertices, triangles: relief.stats.triangles, cliffEdges: relief.stats.cliffEdges },
    points: m, rays: count, levels: Array.from(inputs.levels), grid: inputs.grid, roads: Array.from(inputs.roads).join(''),
    known: inputs.known ? Array.from(inputs.known).join('') : null, frozen: Array.from(inputs.frozen) };
  writeFileSync(join(OUT, `${name}.json`), JSON.stringify(meta));
  // the engine test's share: inputs and a sampled subset of points and rays
  const pick = [], step = Math.max(1, Math.floor(m / 1500));
  for (let i = 0; i < m; i += step) pick.push([+P[i * 2].toFixed(6), +P[i * 2 + 1].toFixed(6), hat[i]]);
  const picks = [];
  for (let i = 0; i < Math.min(200, count); i++) picks.push([...rays.slice(i * 6, i * 6 + 6), ...hits.slice(i * 3, i * 3 + 3)].map((v) => (Number.isFinite(v) ? v : null)));
  fixture.cases.push({ name, w, h, low: !!inputs.low, levels: meta.levels.map(levelChar).join(''), grid: inputs.grid, roads: meta.roads,
    known: meta.known, frozen: meta.frozen.map(levelChar).join(''), stats: meta.stats, samples: pick, picks, ...extra });
  console.log(`${name}: ${w}x${h}${inputs.low ? ' low' : ''}, ${relief.stats.vertices} vertices, ${relief.stats.triangles} triangles, ${relief.stats.cliffEdges} cliff edges, ${m} points, ${count} rays`);
}
const gridString = (grid) => grid.map((r) => r.join('')).join('');

function authored(file, { low = false, name = file.replace(/\.json$/, ''), edits = null } = {}) {
  const raw = JSON.parse(readFileSync(join(REF, 'maps', file), 'utf8'));
  const map = mapForClient(raw);
  if (!map.heights) map.heights = map.rows.map((r) => '0'.repeat(r.length));
  const grid = physicalGrid(map);
  let roads = roadMask(map, grid);
  const frozen = levelsOf(map);
  const relief = createRelief(map, grid, { low, isRoad: (x, y) => x >= 0 && y >= 0 && x < map.w && y < map.h && roads[y * map.w + x] === 1 });
  const inputs = () => ({ low, levels: levelsOf(map), grid: gridString(grid), roads, frozen });
  if (!edits) { record(name, map, grid, relief, inputs(), { source: `maps/${file}`, sha256: sha(`maps/${file}`) }); relief.dispose(); return; }
  record(`${name}-before`, map, grid, relief, inputs(), { source: `maps/${file}`, sha256: sha(`maps/${file}`) });
  const cells = [];
  for (const [x, y, ch, lv] of edits(map, grid)) {
    const c = y * map.w + x;
    grid[y][x] = ch; if (lv !== undefined) setLevel(map, c, lv);
    cells.push([c, ch, lv]);
  }
  roads = roadMask(map, grid); // main.js paints (ground.js) before relief.update
  relief.update(cells);
  record(`${name}-after`, map, grid, relief, inputs(), { source: `maps/${file}`, sha256: sha(`maps/${file}`), edits: cells, base: `${name}-before` });
  relief.dispose();
}

const maps = readdirSync(join(REF, 'maps')).filter((f) => f.endsWith('.json')).sort();
if (maps.length !== 33) throw new Error(`expected 33 authored maps, found ${maps.length}`);
for (const file of maps) if (!ONLY || ONLY.has(file.replace(/\.json$/, ''))) authored(file);

// Incremental: a bombing run, a dug trench, rubble and a fallen bridge (client/main.js applyCells -> relief.update).
if (!ONLY || ONLY.has('pegasus-edits')) authored('pegasus-bridge.json', { name: 'pegasus-edits', edits: (map, grid) => {
  const out = [];
  for (let y = 0; y < map.h; y++) for (let x = 0; x < map.w; x++) if (grid[y][x] === '=' && out.length < 4) out.push([x, y, 'W']);
  const L = (x, y) => levelOf(map.heights[y][x]);
  for (let k = 0; k < 9; k++) out.push([20 + k, 30 + (k >> 1), '+', L(20 + k, 30 + (k >> 1)) - 1]); // a stick of bombs
  for (let k = 0; k < 6; k++) out.push([40 + k, 60, 'T']);
  out.push([41, 61, 'T'], [60, 20, 'R'], [61, 20, 'R'], [60, 21, '+', L(60, 21) - 1]);
  return out;
} });
if (!ONLY || ONLY.has('hill-edits')) authored('hill-112.json', { name: 'hill-edits', edits: (map) => {
  const out = [], L = (x, y) => levelOf(map.heights[y][x]);
  // a blast at the foot of a cliff and on a ramp; a level raised into a new cliff step
  for (let y = 0; y < map.h - 1; y++) for (let x = 0; x < map.w - 1; x++)
    if (out.length < 12 && Math.abs(L(x, y) - L(x + 1, y)) >= 2 && (x + y) % 3 === 0) out.push([x, y, '+', Math.max(-2, L(x, y) - 1)]);
  out.push([10, 10, '.', 3], [11, 10, '.', 3], [10, 11, '.', 3]);
  return out;
} });
// Low graphics (client/gfx.js low): two bands per wall, the coarse rock models, no fan detail
if (!ONLY || ONLY.has('hot-gates-low')) authored('hot-gates.json', { name: 'hot-gates-low', low: true });

// World Conquest: what one team's client holds (client/main.js startGame with start.map and start.cells), then a
// second scouting step through applyCells -> relief.update. Unknown cells are '.' at level 0 in start.map.
if (!ONLY || ONLY.has('world')) {
  const full = generateWorldMap({ seed: 20261013, size: 'huge', players: 2 });
  const { w, h } = full, n = w * h;
  const fullLayers = worldLayers(full), fullGrid = (c) => composeWorldCell(fullLayers.ground[c], fullLayers.objects[c]);
  const known = new Uint8Array(n);
  const discs = (list) => { for (const [cx, cy, r] of list) for (let y = Math.max(0, cy - r); y <= Math.min(h - 1, cy + r); y++) for (let x = Math.max(0, cx - r); x <= Math.min(w - 1, cx + r); x++) if ((x - cx) ** 2 + (y - cy) ** 2 <= r * r) known[y * w + x] = 1; };
  const spawn = full.spawns?.find(Boolean) ?? { x: w >> 2, y: h >> 2 };
  discs([[spawn.x, spawn.y, 46], [Math.min(w - 1, spawn.x + 60), spawn.y, 22]]);
  const map = { w, h, world: true, rows: Array.from({ length: h }, () => '.'.repeat(w)), heights: Array.from({ length: h }, () => '0'.repeat(w)), spawns: full.spawns, points: [] };
  const grid = Array.from({ length: h }, () => Array.from({ length: w }, () => '.'));
  const fullLevel = (c) => levelOf(full.heights?.[Math.floor(c / w)]?.[c % w] ?? '0');
  const reveal = (c) => { const x = c % w, y = Math.floor(c / w), ch = fullGrid(c); grid[y][x] = ch; map.rows[y] = map.rows[y].slice(0, x) + ch + map.rows[y].slice(x + 1); setLevel(map, c, fullLevel(c)); };
  for (let c = 0; c < n; c++) if (known[c]) reveal(c);
  let roads = roadMask(map, grid);
  const frozen = levelsOf(map); // createWaterLevels reads map.heights once, at createRelief
  const relief = createRelief(map, grid, { low: true, isRoad: (x, y) => x >= 0 && y >= 0 && x < w && y < h && roads[y * w + x] === 1 });
  record('world-start', map, grid, relief, { low: true, levels: levelsOf(map), grid: gridString(grid), roads, known, frozen }, { world: true, seed: 20261013 });
  const before = known.slice();
  discs([[Math.min(w - 1, spawn.x + 40), Math.min(h - 1, spawn.y + 40), 30]]);
  const cells = [];
  for (let c = 0; c < n; c++) if (known[c] && !before[c]) { reveal(c); cells.push([c, grid[Math.floor(c / w)][c % w], fullLevel(c)]); }
  roads = roadMask(map, grid);
  relief.update(cells);
  record('world-scouted', map, grid, relief, { low: true, levels: levelsOf(map), grid: gridString(grid), roads, known, frozen }, { world: true, seed: 20261013, base: 'world-start', edits: cells });
  relief.dispose();
}

fixture.sources = Object.fromEntries(['client/relief.js', 'client/relief-material.js', 'client/water-levels.js', 'client/ground.js', 'shared/sim.js', 'shared/world-layers.js', 'shared/world-conquest.js'].map((p) => [p, sha(p)]));
fixture.simExport = SIM_EXPORT.trim();
writeFileSync(join(OUT, 'fixture.json'), JSON.stringify(fixture));
console.log(`wrote ${fixture.cases.length} cases to ${OUT}`);
