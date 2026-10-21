// The oracle for the native painted ground (unreal/Source/ThreeCrossroads/World/TCGroundPaint.h): runs the pinned
// reference's own client/ground.js (createGround, paint, the per-cell attributes and the raster), client/rubble-decals.js
// and client/crater-decals.js under Node, unmodified, on every authored map (as a start sends it, shared/world-layers.js
// mapForClient), on battle snapshots of eight of them, and on a World Conquest sample made by the reference server code
// (shared/world-conquest.js generateWorldMap, shared/sim.js createGame, worldMapFor, terrainFor, fogFor, damageCells,
// step). The client-side handling of a start and of snapshot cell rows is client/main.js startGame and applyCells, copied
// line for line below (main.js wires the DOM and cannot be imported). shared/sim.js heightAt is not exported: its source
// is cut out of the pinned file verbatim and evaluated (heights.json, decal heights).
//
//   WW2_MODEL_DEPS=<dir with node_modules/three> node --import ./unreal/Tools/models/register.mjs \
//     unreal/Tools/terrain/ground-oracle.mjs --out <scratch dir> [--only <map>,...] [--no-world]
//
// Writes into --out: input.txt (the harness's input, see harness/ground-harness.cpp), one <case>.<k>.raster (RGBA, the
// canvas after raster(), before the vector overlays), <case>.<k>.attrs (the cell attributes), <case>.<k>.ops.json (the
// overlays as drawn: shell holes, trench cuts, contour lines), <case>.<k>.decals.json and summary.json (sizes and
// SHA-256 of every output). heights.json.gz: every cell centre's heightAt and levelAt on each authored map and 1,000
// seeded samples per map.
import { readFileSync, writeFileSync, mkdirSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { gzipSync } from 'node:zlib';
import { join, resolve, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { installGroundShims } from './canvas-recorder.mjs';
import { makeReplica } from './client-replica.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const opt = (name, def) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : def; };
const REF = resolve(opt('--reference', join(HERE, '../../../reference/ww2-rts')));
const OUT = resolve(opt('--out', ''));
if (!opt('--out')) { console.error('usage: ground-oracle.mjs --out <scratch dir>'); process.exit(2); }
const ONLY = opt('--only', '') ? opt('--only').split(',') : null;
const NO_WORLD = args.includes('--no-world');
mkdirSync(OUT, { recursive: true });

installGroundShims();
const imp = (p) => import(pathToFileURL(join(REF, p)).href);
const THREE = await import('three');
const { mapForClient, worldLayers, composeWorldCell } = await imp('shared/world-layers.js');
const sim = await imp('shared/sim.js');
const { levelOf, levelChar, startState, CELL, CFG } = sim;
const ground = await imp('client/ground.js');
const { createRubbleDecals } = await imp('client/rubble-decals.js');
const { createCraterDecals } = await imp('client/crater-decals.js');
const { generateWorldMap } = await imp('shared/world-conquest.js');

// shared/sim.js heightAt (module private): the pinned source, cut out and evaluated as it stands
const simSource = readFileSync(join(REF, 'shared/sim.js'), 'utf8');
const hStart = simSource.indexOf('function heightAt(g, x, z) {'), hEnd = simSource.indexOf('\n}\n', hStart) + 2;
const heightAtSource = simSource.slice(hStart, hEnd);
const heightAt = new Function('CELL', 'CFG', `${heightAtSource}; return heightAt;`)(CELL, CFG);

const sha = (buf) => createHash('sha256').update(buf).digest('hex');
const fileSha = (p) => sha(readFileSync(join(REF, p)));
const sources = Object.fromEntries(['client/ground.js', 'client/surfaces.js', 'client/rubble-decals.js', 'client/crater-decals.js', 'client/main.js', 'shared/sim.js', 'shared/world-layers.js', 'shared/world-conquest.js'].map(p => [p, fileSha(p)]));

// ---------- seeded helpers (the oracle's own choices of battle damage, not reference code) ----------
function mulberry(seed) { let s = seed >>> 0; return () => { s = (s + 0x6d2b79f5) >>> 0; let t = s; t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; }; }

// ---------- client/main.js startGame and applyCells (client-replica.mjs) ----------
const { clientStart, clientCells } = makeReplica({ ground, worldLayers, startState, levelChar });

// heightAt over the client's current heights (the decals' hAt here; the game's is the relief's)
// (reads the client's levels as they are when called, like the relief's hAt after a cell sinks)
const hAtOf = (map) => {
  let key = null, g = null;
  return (x, z) => {
    const rows = map.heights ?? map.rows.map(r => '0'.repeat(r.length)), k = rows.join('');
    if (k !== key) { key = k; g = { w: map.w, h: map.h, height: Int8Array.from(k, levelOf) }; }
    return heightAt(g, x, z);
  };
};

// ---------- output ----------
const lines = [];
const summary = { format: 'tc-ground-oracle-v1', reference: execRef(), node: process.version, three: THREE.REVISION, sources, heightAt: sha(heightAtSource), cases: [] };
function execRef() { try { return readFileSync(join(REF, '.git/HEAD'), 'utf8').trim(); } catch { return 'e4e579a'; } }
const fieldsOf = (S) => S.fields;

const replay = {}; // case -> { start, steps }: the browser page (tiles-page.mjs) replays the same messages
function writeMapInput(name, m) {
  const map = m.map;
  replay[name] = { start: structuredClone({ map: { ...map, biome: map.biome ? Array.from(map.biome) : undefined }, cells: m.cells ?? [], fog: m.fog }), steps: [] };
  replay.current = name;
  lines.push(`CASE ${name}`);
  lines.push(`MAP ${map.w} ${map.h} ${map.world ? 1 : 0}`);
  for (const r of map.rows) lines.push(`ROW ${r}`);
  if (map.heights) for (const r of map.heights) lines.push(`HEI ${r}`);
  if (map.layers?.ground) for (const r of map.layers.ground) lines.push(`LG ${r}`);
  if (map.layers?.objects) for (const r of map.layers.objects) lines.push(`LO ${r}`);
  for (const s of map.spawns ?? []) lines.push(s ? `SPAWN ${s.x} ${s.y}` : 'SPAWN null');
  for (const p of map.points ?? []) lines.push(`POINT ${p.x} ${p.y}`);
  if (map.biome) lines.push(`BIOMEMAP ${Array.from(map.biome).join(' ')}`);
  writeCells(m.cells ?? []);
}
function writeCells(cells, step = false) {
  if (step) replay[replay.current].steps.push(structuredClone(cells));
  for (const [c, ch, lv, st, data] of cells) {
    const d = data ?? null;
    lines.push(`CELL ${c} ${ch} ${lv ?? '-'} ${st ?? '-'} ${d ? 1 : 0} ${d?.ground ?? '-'} ${d?.object ?? '-'} ${d?.biome ?? '-'} ${d?.material ?? '-'}`);
  }
}
function capture(name, k, client, decals) {
  const S = globalThis.window.__ground, A = S.attrs, canvas = S.canvas;
  const base = `${name}.${k}`;
  writeFileSync(join(OUT, base + '.raster'), canvas.data);
  const parts = [A.prim, A.sec, A.amt, A.lev, A.ov, A.scar, A.near, A.depth, A.char, A.tr, A.tg, A.tb, S.fields].map(a => Buffer.from(a.buffer, a.byteOffset, a.byteLength));
  const attrs = Buffer.concat(parts);
  writeFileSync(join(OUT, base + '.attrs'), attrs);
  const ops = canvas.ops.splice(0);
  const opsJson = JSON.stringify(ops);
  writeFileSync(join(OUT, base + '.ops.json'), opsJson);
  const dj = JSON.stringify(decals);
  writeFileSync(join(OUT, base + '.decals.json'), dj);
  summary.cases.push({ name, k, w: S.w, h: S.h, P: S.P, painted: S.painted, stats: S.stats, raster: sha(canvas.data), attrs: sha(attrs), ops: ops.length, opsSha: sha(opsJson), decals: sha(dj),
    rubble: decals.rubble?.count ?? 0, craters: decals.craters?.count ?? 0 });
  lines.push(`PAINT ${k}`);
  console.log(`${base}: ${S.w}x${S.h} P=${S.P} ${S.stats?.kind} ${S.stats?.ms ?? ''}ms ops=${ops.length} rubble=${decals.rubble?.count ?? 0} craters=${decals.craters?.count ?? 0}`);
}
// the decals as the reference builds them for this client's cells (a parent group stands in for the scene)
function makeDecals(client) {
  const { map, terrain } = client, hAt = hAtOf(map), parent = new THREE.Group();
  const rubble = createRubbleDecals({ parent, hAt, w: terrain.w, grid: terrain.objectGrid, materialAt: c => terrain.materials.get(c) });
  rubble.update(terrain.objectGrid.flatMap((row, y) => row.flatMap((ch, x) => ch === 'R' ? [y * terrain.w + x] : [])));
  const craters = map.world ? createCraterDecals({ parent, hAt, w: terrain.w, grid: terrain.grid }) : null;
  craters?.update(terrain.grid.flatMap((row, y) => row.flatMap((ch, x) => ch === '+' ? [y * terrain.w + x] : [])));
  return { rubble, craters, hAt };
}
function decalState(d) {
  const r = d.rubble.mesh.geometry, out = {};
  const rc = r.drawRange.count / 96;
  out.rubble = { count: rc, positions: Array.from(r.attributes.position.array.subarray(0, rc * 25 * 3), v => Math.round(v * 1e5) / 1e5), uvs: Array.from(r.attributes.uv.array.subarray(0, rc * 25 * 2), v => Math.round(v * 1e6) / 1e6) };
  if (d.craters) {
    const g = d.craters.mesh.geometry, cc = g.drawRange.count / 96;
    out.craters = { count: cc, positions: Array.from(g.attributes.position.array.subarray(0, cc * 25 * 3), v => Math.round(v * 1e5) / 1e5) };
  }
  return out;
}

// ---------- battle snapshots for an authored map (seeded; each row is one the server can send) ----------
function battleRows(client, seed) {
  const { map, terrain } = client, w = map.w, h = map.h, R = mulberry(seed), rows = [];
  const at = (x, y) => terrain.grid[y]?.[x];
  const lv = (x, y) => levelOf(map.heights?.[y]?.[x] ?? '0');
  const row = (x, y, ch, level, st, data) => rows.push([y * w + x, ch, level, st, data]);
  const used = new Set();
  const put = (x, y, ch, level, st, data) => { if (x < 1 || y < 1 || x >= w - 1 || y >= h - 1) return; const c = y * w + x; if (used.has(c)) return; used.add(c); row(x, y, ch, level, st, data); };
  const open = (x, y) => at(x, y) === '.';
  // lone shell hits: churned earth (scar bits), a few sunk into craters a level down
  for (let i = 0; i < 40; i++) {
    const x = 2 + Math.floor(R() * (w - 4)), y = 2 + Math.floor(R() * (h - 4));
    if (!open(x, y)) continue;
    if (R() < 0.35) put(x, y, '+', lv(x, y) - 1, Math.floor(R() * 4), { ground: '+', object: '.', material: 'soil' });
    else put(x, y, '.', undefined, (1 + Math.floor(R() * 3)) << 5 | Math.floor(R() * 2), null);
  }
  // a pounded block: craters in the middle, scarred and burnt ground round it
  const bx = Math.floor(w * (0.25 + 0.5 * R())), by = Math.floor(h * (0.25 + 0.5 * R()));
  for (let dy = -4; dy <= 4; dy++) for (let dx = -4; dx <= 4; dx++) {
    const x = bx + dx, y = by + dy, d = Math.hypot(dx, dy);
    if (!open(x, y) || d > 4.2) continue;
    if (d < 2.6) put(x, y, '+', lv(x, y) - 1, 1 + Math.floor(R() * 3), { ground: '+', object: '.', material: 'soil' });
    else put(x, y, '.', undefined, 96 | (R() < 0.4 ? 4 : 0) | Math.floor(R() * 4), null);
  }
  // a bombing stick: a line of craters 12 cells long
  const sx = Math.floor(w * (0.2 + 0.6 * R())), sy = Math.floor(h * (0.2 + 0.6 * R())), ang = R() * Math.PI;
  for (let k = 0; k < 12; k++) {
    const x = Math.round(sx + Math.cos(ang) * k * 1.1), y = Math.round(sy + Math.sin(ang) * k * 1.1);
    if (open(x, y)) put(x, y, '+', lv(x, y) - 1, 2, { ground: '+', object: '.', material: 'soil' });
  }
  // houses shot to rubble, hedges and walls damaged, roads worn, tracks worn into the fields, a burnt meadow
  for (let y = 1; y < h - 1; y++) for (let x = 1; x < w - 1; x++) {
    const ch = at(x, y), r = R();
    if (ch === 'B' && r < 0.12) put(x, y, 'R', undefined, 0, { ground: '.', object: 'R', material: R() < 0.5 ? 'brick' : 'stone' });
    else if ((ch === 'H' || ch === '#') && r < 0.1) put(x, y, ch, undefined, r < 0.05 ? 16 : 8, null);
    else if (ch === 'D' && r < 0.15) put(x, y, 'D', undefined, 1 + Math.floor(R() * 3), null);
    else if (ch === '.' && r < 0.01) put(x, y, '.', undefined, 1 + Math.floor(R() * 3) | (R() < 0.3 ? 4 : 0), null);
  }
  // a dug trench line and a known mine (its ground and object travel in the data)
  const tx = Math.floor(w * 0.3 + R() * w * 0.4), ty = Math.floor(h * 0.3 + R() * h * 0.4);
  for (let k = 0; k < 8; k++) if (open(tx + k, ty)) put(tx + k, ty, 'T', undefined, 0, { ground: '.', object: 'T', material: 'soil' });
  for (let k = 0; k < 6; k++) { const x = 2 + Math.floor(R() * (w - 4)), y = 2 + Math.floor(R() * (h - 4)); if (open(x, y)) put(x, y, 'N', undefined, 0, { ground: '.', object: '.', material: 'soil', mine: true }); }
  return rows;
}

// a few shells on one spot and a house beside it brought down: few enough cells for the tile repaint path
function localRows(client, seed) {
  const { map, terrain } = client, w = map.w, h = map.h, R = mulberry(seed), rows = [];
  const lv = (x, y) => levelOf(map.heights?.[y]?.[x] ?? '0');
  let cx = Math.floor(w / 2), cy = Math.floor(h / 2);
  for (let k = 0; k < 200 && terrain.grid[cy][cx] !== '.'; k++) { cx = 3 + Math.floor(R() * (w - 6)); cy = 3 + Math.floor(R() * (h - 6)); }
  for (const [dx, dy] of [[0, 0], [1, 0], [0, 1], [-1, 1]]) {
    const x = cx + dx, y = cy + dy;
    if (terrain.grid[y]?.[x] === '.') rows.push([y * w + x, '+', lv(x, y) - 1, 1, { ground: '+', object: '.', material: 'soil' }]);
  }
  if (terrain.grid[cy - 2]?.[cx + 2] === '.') rows.push([(cy - 2) * w + cx + 2, '.', undefined, 32 | 4, null]);
  for (let y = 1; y < h - 1 && rows.length < 7; y++) for (let x = 1; x < w - 1; x++) if (terrain.grid[y][x] === 'B') { rows.push([y * w + x, 'R', undefined, 0, { ground: '.', object: 'R', material: 'wood' }]); break; }
  return rows;
}

// ---------- authored maps ----------
const mapFiles = readdirSync(join(REF, 'maps')).filter(f => f.endsWith('.json')).sort();
const BATTLE = ['default.json', 'crossroads-village.json', 'bocage.json', 'crater-field.json', 'stalingrad-factory.json', 'hill-112.json', 'the-polder.json', 'no-mans-land.json'];
const heights = { format: 'tc-heights-oracle-v1', heightAt: sha(heightAtSource), maps: [] };
for (const file of mapFiles) {
  if (ONLY && !ONLY.includes(file)) continue;
  const raw = JSON.parse(readFileSync(join(REF, 'maps', file), 'utf8'));
  // heights: every cell centre and 1,000 seeded points (shared/sim.js heightAt and levelAt on the map's own levels)
  {
    const g = { w: raw.w, h: raw.h, height: raw.heights ? Int8Array.from(raw.heights.join(''), levelOf) : null };
    const R = mulberry(0x5eed ^ raw.w * 131 ^ raw.h);
    const centres = [], samples = [];
    for (let y = 0; y < raw.h; y++) for (let x = 0; x < raw.w; x++) {
      const cx = (x + 0.5) * CELL, cz = (y + 0.5) * CELL;
      centres.push([g.height ? heightAt(g, cx, cz) : 0, sim.levelAt(g, cx, cz)]);
    }
    for (let i = 0; i < 1000; i++) {
      const x = Math.round(R() * raw.w * CELL * 1000) / 1000, z = Math.round(R() * raw.h * CELL * 1000) / 1000;
      samples.push([x, z, g.height ? heightAt(g, x, z) : 0, sim.levelAt(g, x, z)]);
    }
    heights.maps.push({ file, w: raw.w, h: raw.h, heights: raw.heights ?? null, centres, samples });
  }
  const start = { map: mapForClient(raw), cells: [] };
  const name = file.replace(/\.json$/, '');
  writeMapInput(name, start);
  const client = clientStart(start);
  const decals = makeDecals(client);
  capture(name, 0, client, decalState(decals));
  if (BATTLE.includes(file)) {
    const rows = battleRows(client, 0xba771e ^ client.map.w * 7919 ^ client.map.h);
    lines.push('STEP');
    writeCells(rows, true);
    const touched = clientCells(client, rows);
    decals.rubble.update(touched);
    capture(name, 1, client, decalState(decals));
    // a small local change after it: the tile repaint (only the 4x4-cell tiles round the changed cells)
    const small = localRows(client, 0x10ca1 ^ client.map.w);
    lines.push('STEP');
    writeCells(small, true);
    decals.rubble.update(clientCells(client, small));
    capture(name, 2, client, decalState(decals));
  }
  lines.push('END');
  ground.createGround({ w: 1, h: 1, rows: ['.'], spawns: [], points: [] }, null).dispose?.(); // drop the canvas between maps
}
writeFileSync(join(OUT, 'heights.json.gz'), gzipSync(JSON.stringify(heights)));

// ---------- a World Conquest sample, made by the reference server code ----------
if (!NO_WORLD) {
  const seed = 20261013;
  const wmap = generateWorldMap({ size: 'huge', seed, players: 2, teams: [0, 1] });
  const g = sim.createGame(wmap, ['Alder', 'Birch'], false, [0, 1], [0, 1], { mode: 'world' });
  sim.step(g);
  const start = { map: sim.worldMapFor(g, 0), cells: sim.terrainFor(g, 0, true), fog: sim.fogFor(g, 0, true) };
  const name = 'world-huge-seed' + seed;
  writeMapInput(name, start);
  const client = clientStart(start);
  const decals = makeDecals(client);
  capture(name, 0, client, decalState(decals));
  // scouting: seat 0's units stand at points round its home, the vision pass runs, the team's memory grows
  const own = [...g.units.values()].filter(u => u.owner === 0 && u.hp > 0);
  const home = { x: own[0].x, z: own[0].z };
  const R = mulberry(seed);
  for (let k = 0; k < 4; k++) {
    for (const u of own) { const a = R() * Math.PI * 2, d = 40 + R() * 90; u.x = Math.min(g.w * CELL - 4, Math.max(4, home.x + Math.cos(a) * d)); u.z = Math.min(g.h * CELL - 4, Math.max(4, home.z + Math.sin(a) * d)); u.path = []; }
    for (let t = 0; t < 8; t++) sim.step(g);
  }
  let rows = sim.terrainFor(g, 0);
  lines.push('STEP'); writeCells(rows, true);
  let touched = clientCells(client, rows); decals.rubble.update(touched); decals.craters.update(touched);
  capture(name, 1, client, decalState(decals));
  // a fight in the known ground: shells and a bombing stick through the reference's own damageCells, then fires burn out
  const list = [...g.units.values()];
  for (let i = 0; i < 60; i++) {
    const a = R() * Math.PI * 2, d = 10 + R() * 60, at = { x: home.x + Math.cos(a) * d, z: home.z + Math.sin(a) * d };
    for (let k = 0; k < 10; k++) sim.damageCells(g, list, at, 3 + R() * 3, 120, 1);
  }
  for (let k = 0; k < 10; k++) for (let s = 0; s < 4; s++) sim.damageCells(g, list, { x: home.x - 40 + k * 4, z: home.z + 30 + k * 1.5 }, 4, 200, 1);
  for (let t = 0; t < 200; t++) sim.step(g);
  rows = sim.terrainFor(g, 0);
  lines.push('STEP'); writeCells(rows, true);
  touched = clientCells(client, rows); decals.rubble.update(touched); decals.craters.update(touched);
  capture(name, 2, client, decalState(decals));
  // the side's fog after it all (shared/sim.js fogFor), for the Unreal capture's fog (capture-prep.mjs)
  replay[name].fogEnd = sim.fogFor(g, 0, true);
  lines.push('END');
}

writeFileSync(join(OUT, 'input.txt'), lines.join('\n') + '\n');
delete replay.current;
for (const [name, r] of Object.entries(replay)) writeFileSync(join(OUT, name + '.replay.json'), JSON.stringify(r));
writeFileSync(join(OUT, 'summary.json'), JSON.stringify(summary, null, 1));
console.log(`wrote ${summary.cases.length} paints to ${OUT}`);
