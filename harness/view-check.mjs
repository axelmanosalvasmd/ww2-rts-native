// Replay raw server JSONL through the pinned browser's real row and fog decoders.
// Reports intentionally hash only explicitly received cell rows, not generated map cells.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { gunzipSync } from 'node:zlib';

const reference = resolve(process.env.WW2_REFERENCE ?? new URL('../reference/ww2-rts', import.meta.url).pathname);
const load = name => import(pathToFileURL(join(reference, name)));
const { createWorldState } = await load('client/world-state.js');
const { createFog } = await load('client/fog.js');
const { rememberPlacementTerrain } = await load('client/availability.js');
const { worldLayers, composeWorldCell } = await load('shared/world-layers.js');
const { levelOf, levelChar, startState, CELL } = await load('shared/sim.js');

export function hashBytes(bytes) {
  let hash = 0xcbf29ce484222325n;
  for (const byte of bytes) hash = BigInt.asUintN(64, (hash ^ BigInt(byte)) * 0x100000001b3n);
  return hash.toString(16).padStart(16, '0');
}
export function canonicalBytes(value) {
  const bytes = [];
  const put = value => {
    if (value === null) bytes.push(110);
    else if (typeof value === 'boolean') bytes.push(value ? 116 : 102);
    else if (typeof value === 'number') {
      bytes.push(100);
      const buffer = Buffer.alloc(8); buffer.writeDoubleLE(Object.is(value, -0) ? 0 : value);
      for (const byte of buffer) bytes.push(byte);
    } else if (typeof value === 'string') {
      bytes.push(115); for (const byte of Buffer.from(value, 'utf8')) bytes.push(byte); bytes.push(0);
    } else if (Array.isArray(value)) {
      bytes.push(97); for (const child of value) put(child); bytes.push(93);
    } else if (typeof value === 'object') {
      bytes.push(111); for (const key of Object.keys(value).sort()) { put(key); put(value[key]); } bytes.push(125);
    } else throw new Error(`unsupported canonical value: ${typeof value}`);
  };
  put(value); return bytes;
}
export const hashValue = value => hashBytes(canonicalBytes(value));

// main.js does not export its terrain functions. Execute their source unchanged,
// with drawing-only dependencies stubbed. This keeps its cell application in the oracle.
const main = readFileSync(join(reference, 'client/main.js'), 'utf8');
const between = (from, to) => {
  const start = main.indexOf(from), end = main.indexOf(to, start);
  if (start < 0 || end < 0) throw new Error(`browser source marker missing: ${from}`);
  return main.slice(start, end);
};
const terrainFunctions = between('function rememberLook(', '// a cell client/structures.js')
  + between('function setLevel(', '// Each player\'s HQ:');
const startCellCode = between('  for (const [cell, ch, lv, st, data] of m.cells || [])', '  gp.paint(terrain.grid');
const noop = () => {};
function browserTerrain(m) {
  const map = m.map;
  rememberPlacementTerrain(map, m.cells);
  const layers = worldLayers(map), rows = values => Array.from({ length: map.h }, (_, y) => values.slice(y * map.w, (y + 1) * map.w));
  const groundGrid = rows(layers.ground), objectGrid = rows(layers.objects);
  const terrain = {
    w: map.w, groundGrid, objectGrid, physicalGrid: groundGrid.map((row, y) => row.map((ch, x) => composeWorldCell(ch, objectGrid[y][x]))),
    mineGrid: rows([...layers.mineLayer]), materials: layers.materials, structuralSections: new Map(),
    grid: map.rows.map(r => [...r]), state: Uint8Array.from(map.rows.join(''), startState), ground: { paint: noop },
  };
  if (map.world) map.discovered = new Uint8Array(map.w * map.h);
  const scope = {
    m, map, terrain, lastStart: m, rememberPlacementTerrain, composeWorldCell, levelOf, levelChar, CELL,
    groundVersion: 0, terrainDue: { pieceCells: new Set() }, railways: null, hasPiece: () => false, nearPiece: () => false,
    mergePropBox: (a, b) => a ?? b, relief: { update: noop }, props: null, propPreparation: null,
    water: { changed: noop }, createWater: () => null, waterOptions: () => ({}), hAt: () => 0,
    world: { add: noop }, refreshTerrain: noop, rubbleDecals: null, craterDecals: null, coverPreview: { dirty: noop },
  };
  // Only pinned local browser source is evaluated. No transcript value becomes code.
  const apply = new Function('scope', `with (scope) { ${terrainFunctions}\n${startCellCode}\nreturn applyCells; }`)(scope);
  const verify = cells => {
    for (const [cell, ch, lv, st = 0, data] of new Map((cells ?? []).map(row => [row[0], row])).values()) {
      const y = Math.floor(cell / map.w), x = cell % map.w;
      assert.equal(terrain.grid[y][x], ch, 'browser terrain character');
      assert.equal(terrain.state[cell], st, 'browser terrain state');
      if (lv !== undefined) assert.equal(levelOf(map.heights[y][x]), lv, 'browser terrain height');
      if (data) {
        assert.equal(terrain.groundGrid[y][x], data.ground, 'browser ground layer');
        assert.equal(terrain.objectGrid[y][x], data.object, 'browser object layer');
      }
      if (map.world) assert.equal(map.discovered[cell], 1, 'browser discovery');
    }
  };
  verify(m.cells);
  return cells => { apply(cells); verify(cells); };
}

export function createViewCheck() {
  const world = createWorldState(), cells = new Map();
  let units = [], fog = null, applyCells = null, resync = false;
  return {
    apply(message) {
      // Browser apply mutates snapshots to include all held rows. Keep input raw.
      const m = structuredClone(message);
      if (m.t === 'start') {
        world.reset(); units = []; cells.clear(); fog?.dispose(); resync = false;
        fog = createFog(m.map.w, m.map.h, m.fog, false, !!m.map.world);
        applyCells = browserTerrain(m);
        for (const row of m.cells ?? []) cells.set(row[0], row);
      } else if (m.t === 's') {
        resync = world.apply(m);
        units = m.units;
        fog?.snapshot(m);
        applyCells?.(m.cells);
        for (const row of m.cells ?? []) cells.set(row[0], row);
      }
      const ordered = [...units].sort((a, b) => a[0] - b[0]);
      return { ids: ordered.map(row => row[0]), units: hashValue(ordered), seen: hashBytes(fog?.seen ?? []),
        explored: hashBytes(fog?.explored ?? []), cells: cells.size,
        terrain: hashValue([...cells.values()].sort((a, b) => a[0] - b[0])), resync };
    },
    dispose() { fog?.dispose(); },
  };
}

if (import.meta.main) {
  const files = process.argv.slice(2);
  if (!files.length) throw new Error('usage: node harness/view-check.mjs <messages.jsonl[.gz]> [...]');
  const view = createViewCheck();
  try {
    for (const file of files) {
      const raw = readFileSync(file), text = (file.endsWith('.gz') ? gunzipSync(raw) : raw).toString('utf8');
      for (const line of text.split(/\r?\n/)) if (line.trim()) console.log(JSON.stringify(view.apply(JSON.parse(line))));
    }
  } finally { view.dispose(); }
}
