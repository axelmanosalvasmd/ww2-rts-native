// Capture original authored-structure validation, setup and support traversal.
// Run with x64 Node v24.21.0. --check compares without rewriting the fixture.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';
assert.equal(process.arch, 'x64'); assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(a => a === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? join(root, 'reference/ww2-rts'));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
function clean() { assert.equal(git(['rev-parse', 'HEAD']), pin); assert.equal(git(['status', '--porcelain', '--untracked-files=no']), ''); }
clean();
const original = await import(pathToFileURL(join(reference, 'shared/sim.js')));
const { supportedSections } = await import(pathToFileURL(join(reference, 'shared/structures.js')));
const directory = mkdtempSync(join(tmpdir(), 'ww2-structures-'));
try {
  let source = readFileSync(join(reference, 'shared/sim.js'), 'utf8');
  const marker = '  setupStructures(g, map);';
  assert.equal(source.split(marker).length, 2);
  source = source.replace(marker, marker + '\n  return g;')
    .replace(/from '(\.\/[^']+)'/g, (_, path) => `from '${pathToFileURL(join(reference, 'shared', path)).href}'`);
  writeFileSync(join(directory, 'sim.mjs'), source);
  const capture = await import(pathToFileURL(join(directory, 'sim.mjs')));
  const runs = values => { const out = []; for (const v of values) { if (out.length && out.at(-1)[0] === v) out.at(-1)[1]++; else out.push([v, 1]); } return out; };
  const pack = s => [s.id, s.kind, s.sections.map(x => [x.id, x.structureId, x.c, x.hp, x.maxHp, x.material, x.anchor, x.supports, x.state, x.rubble])];
  const cases = [];
  const random = Math.random; Math.random = () => 0.5;
  function record(name, map, valid) {
    let error; try { error = original.validateMap(map); } catch { error = 'validator throws'; }
    assert.equal(!error, valid, `${name}: ${error}`);
    let expected = null;
    if (!error) {
      const g = capture.createGame(map, [], false, [], [], { weather: false });
      const structures = [...g.structures.values()];
      const probes = new Map();
      for (let c = 0; c < g.chars.length; c++) {
        const key = JSON.stringify([g.ground[c], g.objects[c], g.materials.get(c), g.baseMaterials.get(c), g.structuralCells.get(c)?.material]);
        if (!probes.has(key)) probes.set(key, c);
      }
      expected = { structures: structures.map(pack), cells: [...g.structuralCells.keys()],
        materialProbes: [...probes.values()].map(c => [c, capture.worldMaterial(g, c).name, capture.worldMaterial(g, c, true).name]),
        cellHp: runs(new Uint32Array(g.cellHp.buffer)), supported: structures.map(s => [...supportedSections(s)]),
        materials: [...g.structuralCells.keys()].map(c => [c, capture.worldMaterial(g, c).name, capture.worldMaterial(g, c, true).name]) };
    }
    cases.push({ name, map: structuredClone(map), error, expected });
  }
  for (const name of readdirSync(join(reference, 'maps')).filter(n => n.endsWith('.json')).sort()) {
    record(`authored:${name}`, JSON.parse(readFileSync(join(reference, 'maps', name), 'utf8')), true);
  }
  const base = (w = 40, h = 40) => ({ name: 'Structures', w, h, rows: Array(h).fill('.'.repeat(w)), spawns: [{ x: 0, y: 0 }, { x: w - 1, y: h - 1 }], points: [{ x: 0, y: 1 }] });
  const cell = (m, x, y, ch) => { m.rows[y] = m.rows[y].slice(0, x) + ch + m.rows[y].slice(x + 1); };
  const section = (id, c, options = {}) => ({ id, c, hp: 100.125, material: 'concrete', anchor: true, supports: [], ...options });
  const house = () => { const m = base(); cell(m, 5, 5, 'B'); cell(m, 6, 5, 'B'); m.structures = [{ id: 'house', kind: 'house', sections: [section('left', 205), section('right', 206, { anchor: false, supports: ['left'] })] }]; return m; };
  const edit = (name, fn, valid = false) => { const m = house(); fn(m); record(name, m, valid); };
  record('valid:authored-one-way-support', house(), true);
  edit('valid:empty-structures', m => { m.structures = []; }, true);
  edit('valid:authored-and-implicit', m => { m.structures[0].sections.pop(); }, true);
  edit('valid:separate-authored-parts', m => { m.structures = [m.structures[0], { id: 'other', kind: 'house', sections: [section('left', 325)] }]; cell(m, 5, 8, 'B'); }, true);
  edit('valid:minimum-positive-health', m => { m.structures[0].sections[0].hp = Number.MIN_VALUE; }, true);
  edit('valid:maximum-health', m => { m.structures[0].sections[0].hp = 100000; }, true);
  const snake = count => { const m = base(); const cells = []; for (let i = 0; i < count; i++) { const y = 5 + Math.floor(i / 16), offset = i % 16, x = 5 + (Math.floor(i / 16) % 2 ? 15 - offset : offset); cell(m, x, y, 'B'); cells.push(y * m.w + x); } return [m, cells]; };
  for (const count of [127, 128, 129, 257]) { const [m] = snake(count); record(`valid:automatic-${count}`, m, true); }
  for (const count of [128, 129]) {
    const [m, cells] = snake(count); m.structures = [{ id: 'max', kind: 'house', sections: cells.map((c, i) => section(`s${i}`, c, { anchor: i === 0, supports: i ? [`s${i - 1}`] : [] })) }];
    record(`${count === 128 ? 'valid' : 'invalid'}:authored-${count}`, m, count === 128);
  }
  {
    const m = base(64, 64); m.structures = Array.from({ length: 2048 }, (_, i) => {
      const c = i + 128; cell(m, c % m.w, Math.floor(c / m.w), 'B'); return { id: `b${i}`, kind: 'house', sections: [section('s', c)] };
    }); record('valid:2048-structures', m, true); m.structures.push(structuredClone(m.structures[0])); record('invalid:2049-structures', m, false);
  }
  for (const count of [128, 257]) {
    const m = base(count + 8, 20); m.rows = Array(m.h).fill('W'.repeat(m.w));
    for (const [x, y] of [[0, 0], [m.w - 1, m.h - 1], [0, 1], [3, 10], [count + 4, 10]]) cell(m, x, y, '.');
    for (let x = 4; x < count + 4; x++) cell(m, x, 10, '=');
    record(`valid:automatic-bridge-${count}`, m, true);
  }
  {
    const m = house(); delete m.structures; m.worldVersion = 2;
    m.layers = { ground: Array(m.h).fill('.'.repeat(m.w)), objects: [...m.rows], objectMaterials: [{ c: 205, material: 'concrete' }], groundMaterials: [{ c: 205, material: 'road' }] };
    record('valid:implicit-material-overrides', m, true);
  }
  const bridge = () => {
    const m = base(); m.rows = Array(m.h).fill('W'.repeat(m.w));
    for (const [x, y] of [[0, 0], [39, 39], [0, 1], [4, 10], [10, 10]]) cell(m, x, y, '.');
    const cells = [5, 6, 7, 8, 9].map(x => { cell(m, x, 10, '='); return 400 + x; });
    m.structures = [{ id: 'bridge', kind: 'bridge', sections: cells.map((c, i) => section(`s${i}`, c, { anchor: i === 0 || i === 4, supports: [i - 1, i + 1].filter(i => i >= 0 && i < 5).map(i => `s${i}`) })) }]; return m;
  };
  record('valid:bridge-two-anchors', bridge(), true);
  { const m = bridge(); m.structures[0].sections = m.structures[0].sections.slice(0, 2); m.structures[0].sections[1].supports = ['s0']; record('valid:bridge-authored-auto-split', m, true); }
  { const m = bridge(); cell(m, 4, 10, 'W'); cell(m, 10, 10, 'W'); record('invalid:bridge-no-bank', m, false); }
  { const m = bridge(); m.structures[0].sections.forEach(s => { s.anchor = false; }); record('invalid:bridge-no-anchor', m, false); }
  {
    const m = house(); cell(m, 6, 6, 'B'); cell(m, 5, 6, 'B');
    m.structures[0].sections = [205, 206, 246, 245].map((c, i) => section(`s${i}`, c, { anchor: i === 0, supports: [`s${(i + 1) % 4}`] }));
    record('valid:anchored-one-way-cycle', m, true); m.structures[0].sections[0].anchor = false; record('invalid:unanchored-cycle', m, false);
  }
  {
    const m = house(); m.worldVersion = 2; m.layers = { ground: Array(m.h).fill('.'.repeat(m.w)), objects: [...m.rows], mines: Array(m.h).fill('.'.repeat(m.w)), objectMaterials: [{ c: 205, material: 'wood' }], groundMaterials: [{ c: 205, material: 'road' }] };
    m.layers.mines[5] = '.....N' + '.'.repeat(34); cell(m, 5, 5, 'N'); record('valid:layered-hidden-mine-and-materials', m, true);
  }
  for (const [name, fn] of [
    ['structure-list', m => { m.structures = {}; }], ['structure-id', m => { m.structures[0].id = 'has:colon'; }],
    ['structure-id-length', m => { m.structures[0].id = 'x'.repeat(49); }], ['structure-duplicate', m => { m.structures.push(structuredClone(m.structures[0])); }],
    ['kind', m => { m.structures[0].kind = 'tower'; }], ['section-list', m => { m.structures[0].sections = {}; }], ['empty-sections', m => { m.structures[0].sections = []; }],
    ['section-id', m => { m.structures[0].sections[0].id = ''; }], ['section-unicode-id', m => { m.structures[0].sections[0].id = 'é'; }],
    ['duplicate-section', m => { m.structures[0].sections[1].id = 'left'; }], ['duplicate-cell', m => { m.structures[0].sections[1].c = 205; }],
    ['cross-structure-cell', m => { m.structures.push({ id: 'other', kind: 'house', sections: [section('s', 205)] }); }],
    ['cell-negative', m => { m.structures[0].sections[0].c = -1; }], ['cell-off-map', m => { m.structures[0].sections[0].c = 1600; }],
    ['cell-fraction', m => { m.structures[0].sections[0].c = 205.5; }], ['footprint', m => { cell(m, 5, 5, '.'); }],
    ['health-zero', m => { m.structures[0].sections[0].hp = 0; }], ['health-high', m => { m.structures[0].sections[0].hp = 100001; }],
    ['health-text', m => { m.structures[0].sections[0].hp = '100'; }], ['material', m => { m.structures[0].sections[0].material = 'glass'; }],
    ['anchor', m => { m.structures[0].sections[0].anchor = 1; }], ['support-list', m => { m.structures[0].sections[0].supports = null; }],
    ['support-count', m => { m.structures[0].sections[0].supports = Array(9).fill('right'); }], ['support-duplicate', m => { m.structures[0].sections[1].supports = ['left', 'left']; }],
    ['support-self', m => { m.structures[0].sections[0].supports = ['left']; }], ['support-missing', m => { m.structures[0].sections[1].supports = ['other']; }],
    ['support-type', m => { m.structures[0].sections[0].supports = [0]; }], ['support-diagonal', m => { cell(m, 6, 5, '.'); cell(m, 6, 6, 'B'); m.structures[0].sections[1].c = 246; }],
    ['no-anchor', m => { m.structures[0].sections[0].anchor = false; }], ['disconnected', m => { m.structures[0].sections[1].supports = []; }],
    ['row-wrap', m => { cell(m, 5, 5, '.'); cell(m, 6, 5, '.'); cell(m, 39, 4, 'B'); cell(m, 0, 5, 'B'); m.structures[0].sections[0].c = 199; m.structures[0].sections[1].c = 200; }],
  ]) edit(`invalid:${name}`, fn);
  for (const material of ['soil', 'road', 'wood', 'stone', 'concrete', 'steel', 'water']) edit(`valid:material-${material}`, m => { m.structures[0].sections[0].material = material; }, true);
  const supportCases = [];
  const make = () => ({ id: 'graph', kind: 'house', sections: [section('a', 0, { supports: ['b'] }), section('b', 1, { anchor: false, supports: ['c'] }), section('c', 2, { anchor: false, supports: ['a'] }), section('isolated', 9, { anchor: false })].map(s => ({ ...s, structureId: 'graph', maxHp: s.hp, state: 'intact', rubble: [] })) });
  for (const [name, mutate] of [
    ['anchored-cycle', () => {}], ['failed-anchor', s => { s.sections[0].state = 'failed'; }],
    ['dead-anchor', s => { s.sections[0].hp = 0; }], ['negative-anchor', s => { s.sections[0].hp = -1; }],
    ['burning-anchor', s => { s.sections[0].state = 'burning'; }], ['damaged-node', s => { s.sections[1].state = 'damaged'; }],
    ['all-anchors', s => { s.sections.forEach(x => { x.anchor = true; }); }], ['reverse-links', s => { s.sections[0].supports = []; s.sections[1].supports = ['a']; s.sections[2].supports = ['b']; }],
  ]) { const s = make(); mutate(s); supportCases.push({ name, structure: pack(s), expected: [...supportedSections(s)] }); }
  Math.random = random; clean();
  const content = JSON.stringify({ reference: pin, node: process.version, arch: process.arch, sectionFields: ['id', 'structureId', 'c', 'hp', 'maxHp', 'material', 'anchor', 'supports', 'state', 'rubble'], cases, supportCases }) + '\n';
  const path = join(root, 'rust/crates/core/tests/fixtures/structures.json');
  if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), content); else writeFileSync(path, content);
  console.error(`${process.argv.includes('--check') ? 'Verified' : 'Recorded'} ${cases.length} structural maps (${cases.filter(c => c.expected).length} initialized), ${supportCases.length} support traversals`);
} finally { rmSync(directory, { recursive: true, force: true }); }
