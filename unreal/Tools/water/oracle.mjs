// The oracle for the native water and apron (unreal/Source/ThreeCrossroads/World/TCWater.h, TCApron.h): runs the pinned
// reference's own client/water.js createWater, client/apron.js createApron, client/relief.js createRelief (the ground
// both drape on), client/ground.js createGround (the per-cell materials the apron paints from) and shared/sim.js
// (createGame, worldMapFor, terrainFor for a World Conquest start) under Node, unmodified, on all 33 authored maps and
// a World Conquest start with a scouting step, and writes for each case what the harness needs to rebuild the same
// inputs (cells, levels, knowledge, every ground height the reference code asked for) and what the reference built
// (water positions, flow and indices, the 4-texel-per-cell mask, the apron's positions, normals and indices, its
// cell texture, both noise textures), plus the terrain.md water checks.
//
//   WW2_MODEL_DEPS=<dir with node_modules/three> node --import ./unreal/Tools/models/register.mjs \
//     unreal/Tools/water/oracle.mjs --out <scratch dir> [--reference reference/ww2-rts] [--only name,name]
//
// The browser-only APIs touched at module load are shimmed inertly (unreal/Tools/models/shims.mjs); nothing is drawn.
// The heights the water (drape over raised spans) and the apron (edge profile, lip strip) read come from the reference
// relief through a logging wrapper: the call is the reference's, the wrapper only records (x, z, value).
import { readFileSync, writeFileSync, mkdirSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join, resolve, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { installShims, settle } from '../models/shims.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const opt = (name, def) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : def; };
const REF = resolve(opt('--reference', join(HERE, '../../../reference/ww2-rts')));
const OUT = resolve(opt('--out', ''));
const ONLY = opt('--only', '') ? new Set(opt('--only', '').split(',')) : null;
if (!opt('--out', '')) throw new Error('--out <dir> is required');
mkdirSync(OUT, { recursive: true });

installShims();
const imp = (p) => import(pathToFileURL(join(REF, p)).href);
const { CELL, CFG, levelOf, startState, createGame, worldMapFor, terrainFor } = await imp('shared/sim.js');
const { worldLayers, composeWorldCell, mapForClient } = await imp('shared/world-layers.js');
const { generateWorldMap } = await imp('shared/world-conquest.js');
const { createWater } = await imp('client/water.js');
const { createWaterLevels, WATER_LIFT } = await imp('client/water-levels.js');
const { createApron } = await imp('client/apron.js');
const { createRelief, TRENCH_DEPTH } = await imp('client/relief.js');
const { createGround, groundLook } = await imp('client/ground.js');
await settle();

const sha = (p) => createHash('sha256').update(readFileSync(join(REF, p))).digest('hex');
const SOURCES = ['client/water.js', 'client/water-levels.js', 'client/apron.js', 'client/relief.js', 'client/ground.js', 'client/surfaces.js', 'client/main.js', 'shared/sim.js', 'shared/world-layers.js', 'shared/world-conquest.js'];
const levelChar = value => value < 0 ? String.fromCharCode(96 - value) : String(value);
const hex = (buf) => createHash('sha256').update(Buffer.from(buf.buffer, buf.byteOffset, buf.byteLength)).digest('hex');

// main.js setLevel
function setLevel(map, cell, lv) {
  map.heights ??= map.rows.map(r => '0'.repeat(r.length));
  const x = cell % map.w, y = Math.floor(cell / map.w), r = map.heights[y];
  map.heights[y] = r.slice(0, x) + levelChar(lv) + r.slice(x + 1);
}
const levelAtCell = (map, c) => levelOf(map.heights?.[Math.floor(c / map.w)]?.[c % map.w] ?? '0');

// main.js startGame, the part that builds terrain, relief, apron and water from a start message m
function start(m) {
  const map = m.map;
  const layers = worldLayers(map);
  const terrain = {
    w: map.w,
    groundGrid: Array.from({ length: map.h }, (_, y) => layers.ground.slice(y * map.w, (y + 1) * map.w)),
    objectGrid: Array.from({ length: map.h }, (_, y) => layers.objects.slice(y * map.w, (y + 1) * map.w)),
    physicalGrid: Array.from({ length: map.h }, (_, y) => layers.ground.slice(y * map.w, (y + 1) * map.w).map((ch, x) => composeWorldCell(ch, layers.objects[y * map.w + x]))),
    grid: map.rows.map(r => [...r]),
  };
  terrain.state = Uint8Array.from(map.rows.join(''), startState);
  if (map.world) map.discovered = new Uint8Array(map.w * map.h);
  for (const [cell, ch, lv, st, data] of m.cells || []) {
    const y = Math.floor(cell / map.w), x = cell % map.w;
    if (data) { terrain.groundGrid[y][x] = data.ground; terrain.objectGrid[y][x] = data.object; }
    terrain.physicalGrid[y][x] = ch === 'N' ? composeWorldCell(terrain.groundGrid[y][x], terrain.objectGrid[y][x]) : ch;
    if (map.discovered) {
      map.discovered[cell] = 1;
      if (data?.biome !== undefined && map.biome?.[cell] !== data.biome) (map.biome ??= new Uint8Array(map.w * map.h).fill(255))[cell] = data.biome;
    }
    terrain.grid[y][x] = ch; terrain.state[cell] = st ?? 0; if (lv !== undefined) setLevel(map, cell, lv);
  }
  if (map.world) map.rows = terrain.grid.map(r => r.join(''));
  const gp = createGround(map, null, {});
  gp.paint(terrain.grid, terrain.state, terrain.groundGrid, terrain.objectGrid);
  const relief = createRelief(map, terrain.physicalGrid, { isRoad: gp.isRoad, low: !!map.world });
  const log = { apron: [], water: [] };
  let into = null;
  const hAt = (x, z) => { const v = relief.hAt(x, z); into?.push([x, z, v]); return v; };
  into = log.apron;
  const apron = createApron({ ground: gp, relief: { hAt, mesh: relief.mesh }, grid: terrain.physicalGrid, map });
  into = log.water;
  const frozen = Array.from(createWaterLevels(map).levels); // what createWater freezes now
  const water = createWater(terrain.physicalGrid, map, hAt, map.world ? { unknown: c => !map.discovered?.[c] } : {});
  into = null;
  return { map, terrain, gp, relief, apron, water, log, frozen: water ? frozen : null, hAt, setInto: (arr) => { into = arr; } };
}

// main.js applyCells, the part water and relief see
function apply(S, cells) {
  const { map, terrain } = S;
  for (const [cell, ch, lv, st = 0, data] of cells) {
    const y = Math.floor(cell / map.w), x = cell % map.w;
    if (data) { terrain.groundGrid[y][x] = data.ground; terrain.objectGrid[y][x] = data.object; }
    terrain.physicalGrid[y][x] = ch === 'N' ? composeWorldCell(terrain.groundGrid[y][x], terrain.objectGrid[y][x]) : ch;
    if (map.world) { map.discovered[cell] = 1; const row = map.rows[y]; map.rows[y] = row.slice(0, x) + ch + row.slice(x + 1); }
    terrain.grid[y][x] = ch; terrain.state[cell] = st;
    if (lv !== undefined) setLevel(map, cell, lv);
  }
  S.relief.update(cells);
  const queries = [];
  S.setInto(queries);
  let created = null;
  if (!S.water && cells.some(([, ch]) => 'WF='.includes(ch))) {
    created = Array.from(createWaterLevels(map).levels);
    S.water = createWater(terrain.physicalGrid, map, S.hAt, map.world ? { unknown: c => !map.discovered?.[c] } : {});
    if (!S.water) created = null; else S.frozen = created;
  } else { S.water?.changed(cells); S.water?.tick(0); }
  S.setInto(null);
  return { queries, created };
}

const uniformsOf = (material) => { const sh = { uniforms: {}, vertexShader: '#include <common>', fragmentShader: '#include <common>' }; material.onBeforeCompile(sh); return sh.uniforms; };

function waterBlobs(water) {
  if (!water) return null;
  const g = water.mesh.geometry, u = uniformsOf(water.mesh.material);
  return { pos: g.attributes.position.array, flow: g.attributes.flow.array, nor: g.attributes.normal.array, index: Uint32Array.from(g.index.array), mask: u.uMask.value.image.data };
}

// terrain.md: each body at a line from the frozen levels plus 0.06 m; fords 0.15 m below it; channels 0.42 m below
// plus 0.38 m per row inward; the mesh's surface at wet cell centres (on the b-c diagonal: the mean of two corners)
function waterChecks(S, frozen) {
  const { map, terrain, relief, water } = S;
  if (!water) return { wet: 0 };
  const w = map.w, h = map.h, W1 = w + 1;
  const levels = new Int8Array(frozen);
  const lv = createWaterLevels({ ...map, heights: Array.from({ length: h }, (_, y) => Array.from({ length: w }, (_, x) => levelChar(levels[y * w + x])).join('')) }).read(terrain.physicalGrid);
  const pos = water.mesh.geometry.attributes.position.array;
  let dented = 0, dentedLineMax = 0, dentedBelow = Infinity, wet = 0, fords = 0, bridges = 0, raised = 0, flatCells = 0, lineExact = 0, lineMax = 0, waterAtMax = 0, fordMax = 0, fordN = 0;
  for (let c = 0; c < w * h; c++) {
    if (!lv.wet[c]) continue;
    wet++;
    const x = c % w, y = Math.floor(c / w), ch = terrain.physicalGrid[y][x];
    if (ch === 'F') fords++; if (ch === '=') bridges++;
    const line = lv.wl[c] + WATER_LIFT;
    waterAtMax = Math.max(waterAtMax, Math.abs(relief.waterAt((x + 0.5) * CELL, (z => z)((y + 0.5) * CELL)) - line));
    if (lv.raised[c]) { raised++; continue; }
    // corners touching only this body's line (no lower neighbour line): the surface lies exactly on the line there
    let simple = true;
    for (let dy = -1; dy <= 1; dy++) for (let dx = -1; dx <= 1; dx++) {
      const nx = x + dx, ny = y + dy;
      if (nx < 0 || ny < 0 || nx >= w || ny >= h) continue;
      const k = ny * w + nx;
      if (!Number.isNaN(lv.wl[k]) && lv.wl[k] < lv.wl[c]) simple = false;
      if (lv.raised[k]) simple = false;
    }
    const b = y * W1 + x + 1, cc = (y + 1) * W1 + x, centre = (pos[b * 3 + 1] + pos[cc * 3 + 1]) / 2;
    if (simple) { flatCells++; const d = Math.abs(centre - line); lineMax = Math.max(lineMax, d); if (d < 1e-6) lineExact++; }
    // a ford the map was drawn with (its level never dug): its bed 0.15 m under the line. A flooded crater keeps the
    // crater's depth under the body's line (shared/sim.js flood: "a ford as deep as the crater was").
    if (ch === 'F' && levelAtCell(map, c) === levels[c]) { fordN++; fordMax = Math.max(fordMax, Math.abs(relief.hAt((x + 0.5) * CELL, (y + 0.5) * CELL) - (line - 0.15))); }
    if (levelAtCell(map, c) < levels[c]) { dented++; dentedLineMax = Math.max(dentedLineMax, Math.abs(relief.waterAt((x + 0.5) * CELL, (y + 0.5) * CELL) - line)); dentedBelow = Math.min(dentedBelow, line - relief.hAt((x + 0.5) * CELL, (y + 0.5) * CELL)); }
  }
  return { wet, fords, bridges, raised, flatCells, lineExact, lineMaxDev: lineMax, waterAtMaxDev: waterAtMax, fordCentres: fordN, fordBedMaxDev: fordMax, dentedWet: dented, dentedLineMaxDev: dentedLineMax, dentedMinDepth: dented ? dentedBelow : null };
}

// ---------- the case files ----------
// input.txt (harness): line-based, see harness/water-harness.cpp ReadCase
// the start's inputs, taken before any step changes the grid
function startLines(S) {
  const { map, terrain } = S, w = map.w, h = map.h, n = w * h;
  const L = [];
  L.push(`size ${w} ${h} ${map.world ? 1 : 0}`);
  for (let y = 0; y < h; y++) L.push('phys ' + terrain.physicalGrid[y].join(''));
  for (let y = 0; y < h; y++) L.push('grid ' + terrain.grid[y].join(''));
  for (let y = 0; y < h; y++) L.push('orig ' + map.rows[y]);
  L.push('levels ' + Array.from({ length: n }, (_, c) => levelAtCell(map, c)).join(' '));
  if (S.startFrozen) L.push('frozen ' + S.startFrozen.join(' '));
  L.push('known ' + (map.world ? Array.from(map.discovered).join('') : '1'.repeat(n)));
  L.push('state ' + Array.from(terrain.state).join(' '));
  L.push('biome ' + (map.biome ? Array.from(map.biome).join(' ') : 'none'));
  L.push('spawns ' + (map.spawns || []).filter(Boolean).map(p => `${p.x},${p.y}`).join(' '));
  L.push('points ' + (map.points || []).map(p => `${p.x},${p.y}`).join(' '));
  const q = (tag, arr) => { for (const [x, z, v] of arr) L.push(`${tag} ${x} ${z} ${v}`); };
  q('happron', S.log.apron);
  q('hwater', S.log.water);
  return L;
}
function writeCase(dir, L, extra) {
  const q = (tag, arr) => { for (const [x, z, v] of arr) L.push(`${tag} ${x} ${z} ${v}`); };
  for (const step of extra.steps) {
    L.push('step ' + step.name);
    if (step.created) L.push('create ' + step.created.join(' '));
    for (const [c, ch, lv] of step.cells) L.push(`cell ${c} ${ch} ${lv === undefined ? 'none' : lv}`);
    q('hwater', step.queries);
    L.push('tick');
  }
  L.push('end');
  writeFileSync(join(dir, 'input.txt'), L.join('\n') + '\n');
}
function writeBlobs(dir, tag, blobs) {
  if (!blobs) { writeFileSync(join(dir, `${tag}.none`), ''); return; }
  for (const [k, v] of Object.entries(blobs)) writeFileSync(join(dir, `${tag}.${k}.bin`), Buffer.from(v.buffer, v.byteOffset, v.byteLength));
}

const summary = { format: 'tc-water-oracle-v1', reference: REF, sources: Object.fromEntries(SOURCES.map(p => [p, sha(p)])), constants: { CELL, levelHeight: CFG.levelHeight, WATER_LIFT, TRENCH_DEPTH }, cases: [] };
// the two noise textures (shared by every case)
{
  const probe = { w: 2, h: 1, rows: ['W.'], heights: ['00'] };
  const wtr = createWater(probe.rows.map(r => [...r]), probe, () => 0);
  const noise = uniformsOf(wtr.mesh.material).uNoise.value.image.data;
  writeFileSync(join(OUT, 'water-noise.bin'), Buffer.from(noise.buffer, noise.byteOffset, noise.byteLength));
  const look = groundLook(), nn = look.noise.size, data = new Uint8Array(nn * nn * 4);
  for (let i = 0; i < nn * nn; i++) { data[i * 4] = look.noise.NP[i] * 255; data[i * 4 + 1] = (look.noise.NA[i] * 0.5 + 0.5) * 255; data[i * 4 + 3] = 255; }
  writeFileSync(join(OUT, 'ground-noise.bin'), Buffer.from(data.buffer));
  summary.noise = { water: hex(noise), ground: hex(data), flat: look.flat, meters: look.meters };
  wtr.dispose();
}

function runCase(name, m, makeSteps) {
  if (ONLY && !ONLY.has(name)) return;
  const t0 = Date.now();
  const S = start(m);
  S.startFrozen = S.frozen;
  const lines = startLines(S);
  const dir = join(OUT, name);
  mkdirSync(dir, { recursive: true });
  const checks = waterChecks(S, S.frozen);
  writeBlobs(dir, 'water0', waterBlobs(S.water));
  const u = uniformsOf(S.apron.mesh.material);
  const ag = S.apron.mesh.geometry;
  writeBlobs(dir, 'apron0', { pos: ag.attributes.position.array, nor: ag.attributes.normal.array, index: ag.index.array, cells: u.uCells.value.image.data,
    cloud: Uint32Array.of(S.apron.overlayGeometry.index.count) });
  const steps = [];
  for (const step of makeSteps ? makeSteps(S) : []) {
    if (step.lazy) step.cells = step.lazy();
    const done = apply(S, step.cells);
    step.queries = done.queries; step.created = done.created;
    steps.push(step);
    writeBlobs(dir, `water${steps.length}`, waterBlobs(S.water));
    step.checks = waterChecks(S, S.frozen);
  }
  writeCase(dir, lines, { steps });
  const row = { name, w: S.map.w, h: S.map.h, world: !!S.map.world, water: !!S.water, verts: S.water ? S.water.mesh.geometry.attributes.position.count : 0,
    tris: S.water ? S.water.mesh.geometry.index.count / 3 : 0, apronVerts: ag.attributes.position.count, apronTris: ag.index.count / 3, checks,
    steps: steps.map(s => ({ name: s.name, cells: s.cells.length, checks: s.checks })), ms: Date.now() - t0 };
  summary.cases.push(row);
  console.log(`${name}: ${row.w}x${row.h} water ${row.water ? `${row.verts} verts ${row.tris} tris` : 'none'}, apron ${row.apronVerts} verts, steps ${steps.length}, ${row.ms} ms`);
  S.water?.dispose(); S.apron.dispose(); S.relief.dispose?.(); S.gp.dispose?.();
}

// dynamic steps on an authored map: a flooded crater, a destroyed bridge, a dent in a river bed, a dent beside a raised span
function authoredSteps(S) {
  const { map, terrain } = S, w = map.w, h = map.h, steps = [];
  const ch = (c) => terrain.physicalGrid[Math.floor(c / w)][c % w], lvl = (c) => levelAtCell(map, c);
  const n4 = (c) => { const x = c % w, y = Math.floor(c / w); return [[x + 1, y], [x - 1, y], [x, y + 1], [x, y - 1]].filter(([a, b]) => a >= 0 && b >= 0 && a < w && b < h).map(([a, b]) => b * w + a); };
  // flooded crater: a '.' beside W or F at the same level: first a crater one level down, then it floods into a ford
  let crater = -1;
  for (let c = 0; c < w * h && crater < 0; c++) if (ch(c) === '.' && n4(c).some(k => 'WF'.includes(ch(k)) && lvl(k) >= lvl(c))) crater = c;
  if (crater >= 0) {
    const lv = Math.max(CFG.minLevel, lvl(crater) - 1);
    steps.push({ name: 'crater-beside-water', cells: [[crater, '+', lv]] });
    steps.push({ name: 'crater-floods', cells: [[crater, 'F', lv]] });
  }
  // a destroyed bridge cell becomes river at its own level (its line stays the body's frozen line)
  let bridge = -1;
  for (let c = 0; c < w * h && bridge < 0; c++) if (ch(c) === '=') bridge = c;
  if (bridge >= 0) steps.push({ name: 'bridge-destroyed', cells: [[bridge, 'W', lvl(bridge)]] });
  // a shell dents a river bed: the frozen line does not move
  let bed = -1;
  for (let c = 0; c < w * h && bed < 0; c++) if (ch(c) === 'W' && c !== bridge && lvl(c) > CFG.minLevel) bed = c;
  if (bed >= 0) steps.push({ name: 'river-bed-dented', cells: [[bed, 'W', lvl(bed) - 1]] });
  // a dent next to a raised span (a bridge above its body's line) moves the ground it drapes on
  const water = createWaterLevels({ ...map }).read(terrain.physicalGrid);
  let beside = -1;
  for (let c = 0; c < w * h && beside < 0; c++) if (water.raised[c]) for (const k of n4(c)) if (!water.wet[k] && lvl(k) > CFG.minLevel) { beside = k; break; }
  if (beside >= 0) steps.push({ name: 'dent-beside-raised-span', cells: [[beside, ch(beside), lvl(beside) - 1]] });
  return steps;
}

const maps = readdirSync(join(REF, 'maps')).filter(f => f.endsWith('.json')).sort();
for (const file of maps) {
  const raw = JSON.parse(readFileSync(join(REF, 'maps', file), 'utf8'));
  const m = { map: mapForClient(raw), cells: [] };
  runCase(file.replace(/\.json$/, ''), m, authoredSteps);
}

// World Conquest: a real start for seat 0 from the pinned sim, then a scouting step that brings the cell rows of a
// band of the true map beside known water (rows as the server sends them: [cell, char, level, state])
{
  const name = 'world-conquest-seed17';
  if (!ONLY || ONLY.has(name)) {
    const g = createGame(generateWorldMap({ seed: 17, players: 2 }), ['Alder', 'Birch'], false, [0, 1], [0, 1], { mode: 'world', weather: false });
    const m = { map: worldMapFor(g, 0), cells: terrainFor(g, 0, true) };
    summary.world = { seed: 17, size: 'huge', w: g.w, h: g.h, knownAtStart: m.cells.length };
    const rowOf = (c) => [c, g.chars[c], g.height ? g.height[c] : 0];
    runCase(name, m, (S) => {
      const { map } = S, w = map.w, h = map.h, known = map.discovered;
      // first: the nearest true water to what is known, and a 48 x 48 square of the true map around it (a scout's walk)
      let sx = 0, sy = 0, k0 = 0;
      for (let c = 0; c < w * h; c++) if (known[c]) { sx += c % w; sy += Math.floor(c / w); k0++; }
      sx /= k0; sy /= k0;
      let best = -1, bd = Infinity;
      for (let c = 0; c < w * h; c++) if ('WF'.includes(g.chars[c])) { const d = Math.hypot(c % w - sx, Math.floor(c / w) - sy); if (d < bd) { bd = d; best = c; } }
      const square = [];
      for (let y = Math.floor(best / w) - 24; y < Math.floor(best / w) + 24; y++) for (let x = best % w - 24; x < best % w + 24; x++) if (x >= 0 && y >= 0 && x < w && y < h && !known[y * w + x]) square.push(y * w + x);
      summary.world.firstWater = { cell: best, distance: bd, revealed: square.length, wet: square.filter(c => 'WF='.includes(g.chars[c])).length };
      return [{ name: 'scout-finds-water', cells: square.map(rowOf) }, { name: 'scouting-band', lazy: () => band(S) }];
    });
    function band(S) {
      const { map } = S, w = map.w, h = map.h, known = map.discovered;
      // the unknown cells within 6 of known water, on the true map (what a scout walking the bank would see)
      const band = [];
      for (let c = 0; c < w * h; c++) {
        if (known[c]) continue;
        const x = c % w, y = Math.floor(c / w);
        let near = false;
        for (let dy = -6; dy <= 6 && !near; dy++) for (let dx = -6; dx <= 6; dx++) {
          const k = (y + dy) * w + x + dx;
          if (x + dx >= 0 && y + dy >= 0 && x + dx < w && y + dy < h && known[k] && 'WF='.includes(S.terrain.physicalGrid[y + dy][x + dx])) { near = true; break; }
        }
        if (near) band.push(c);
      }
      const rows = band.slice(0, 4000).map(rowOf);
      summary.world.scouted = rows.length; summary.world.scoutedWet = rows.filter(r => 'WF='.includes(r[1])).length;
      summary.world.scoutedWetNonzeroLevel = rows.filter(r => 'WF='.includes(r[1]) && r[2] !== 0).length;
      return rows;
    }
  }
}
writeFileSync(join(OUT, 'summary.json'), JSON.stringify(summary, null, 1));
console.log(`wrote ${summary.cases.length} cases to ${OUT}`);
