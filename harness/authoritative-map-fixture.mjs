// Record the pinned validator and pre-match terrain initialization, without changing the reference.
// Run with x64 Node v24.21.0. --check compares the checked-in fixture without writing it.
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
function clean() {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
}
clean();
const original = await import(pathToFileURL(join(reference, 'shared/sim.js')));
const directory = mkdtempSync(join(tmpdir(), 'ww2-authoritative-map-'));
try {
  // Run the original initialization statements through g.height. Stop before structural graphs,
  // starting units, mode setup or base placement can change this static authoring terrain.
  const marker = '  // The map clients download stays separate from later building footprints and terrain edits.';
  let source = readFileSync(join(reference, 'shared/sim.js'), 'utf8');
  assert.equal(source.split(marker).length, 2, 'initialization capture point must be unique');
  source = source.replace(marker, '  return g;\n' + marker)
    .replace(/from '(\.\/[^']+)'/g, (_, path) => `from '${pathToFileURL(join(reference, 'shared', path)).href}'`);
  writeFileSync(join(directory, 'sim.mjs'), source);
  const capture = await import(pathToFileURL(join(directory, 'sim.mjs')));
  const runs = values => {
    const out = [];
    for (const value of values) {
      if (out.length && out.at(-1)[0] === value) out.at(-1)[1]++;
      else out.push([value, 1]);
    }
    return out;
  };
  const floatRuns = array => array && runs(new Uint32Array(array.buffer, array.byteOffset, array.length));
  const sorted = map => [...map].sort((a, b) => a[0] - b[0]);
  const cases = [];
  const originalRandom = Math.random;
  Math.random = () => 0.5;
  function record(name, map, unsupported = null) {
    let error;
    try { error = original.validateMap(map); } catch { error = 'validator throws'; }
    if (name.startsWith('boundary:') || name.startsWith('authored:')) assert.equal(error, null, `${name} must validate`);
    if (name.startsWith('invalid:')) assert.ok(error, `${name} must fail validation`);
    let terrain = null;
    if (!error && !unsupported) {
      const g = capture.createGame(map, [], false, [], [], { weather: false });
      terrain = {
        chars: g.chars.join(''), ground: g.ground.join(''), objects: g.objects.join(''),
        mines: runs(g.mineLayer), materials: sorted(g.materials), baseMaterials: sorted(g.baseMaterials),
        objectMaterials: sorted(g.objectMaterials), mineMaterials: sorted(g.mineMaterials),
        flags: runs(g.flags), height: g.height && runs(g.height), house: runs(g.house),
        cellHp: floatRuns(g.cellHp), tough: g.tough && runs(g.tough), wear: floatRuns(g.wear),
        surfaceWear: floatRuns(g.surfaceWear), trenchFront: floatRuns(g.trenchFront),
        shoal: g.shoal && runs(g.shoal), roads: g.roads, naval: g.naval,
      };
    }
    cases.push({ name, map, error, unsupported, terrain });
  }
  for (const name of readdirSync(join(reference, 'maps')).filter(n => n.endsWith('.json')).sort()) {
    record(`authored:${name}`, JSON.parse(readFileSync(join(reference, 'maps', name), 'utf8')));
  }
  const base = () => ({ name: 'Boundary', w: 20, h: 20, rows: Array(20).fill('.'.repeat(20)),
    spawns: [{ x: 0, y: 0 }, { x: 19, y: 19 }], points: [{ x: 10, y: 10 }] });
  const edit = (name, fn, unsupported) => { const m = base(); fn(m); record(name, m, unsupported); };
  const cell = (rows, x, y, char) => { rows[y] = rows[y].slice(0, x) + char + rows[y].slice(x + 1); };
  record('boundary:minimum', base());
  edit('boundary:maximum', m => { m.w = m.h = 1024; m.rows = Array(1024).fill('.'.repeat(1024)); m.spawns[1] = { x: 1023, y: 1023 }; });
  edit('boundary:null-point-income', m => { m.points[0].vp = m.points[0].mp = null; });
  edit('boundary:twenty-triggers', m => { m.triggers = Array.from({ length: 20 }, () => ({ at: 7200, say: 'x'.repeat(140) })); });
  record('invalid:null', null); record('invalid:array', []);
  edit('boundary:all-terrain', m => { m.rows[5] = '.BH#+TWF=RXYDMNOQ'.padEnd(20, '.'); });
  edit('boundary:heights', m => { m.heights = Array(20).fill('01234ab'.repeat(2) + '01234a'); });
  edit('boundary:metadata', m => { m.custom = { text: 'retained', nested: [null, true, 3] }; m.points[0].id = 'point:12'; m.pointSequence = 13; m.assaultTime = 3600; m.defend = [0]; });
  edit('boundary:six-spawns', m => { m.spawns.push(...[1, 2, 3, 4].map(x => ({ x, y: 1, assault: true }))); });
  edit('boundary:name-utf16-40', m => { m.name = '😀'.repeat(20); });
  edit('boundary:legacy-triggers', m => { m.triggers = [{ at: 0, say: 'a' }, { at: 7200, blow: [0, 0, 15, 15] }]; });
  edit('boundary:nine-points', m => { m.points = Array.from({ length: 9 }, (_, i) => ({ x: i, y: 4, needs: i ? i - 1 : undefined, vp: 0, mp: 5, kind: i % 2 ? 'radio' : 'depot' })); });
  const layered = () => {
    const m = base(); m.worldVersion = 2;
    m.layers = { ground: [...m.rows], objects: [...m.rows], mines: [...m.rows] };
    const put = (x, y, ground, object, mine = '.') => {
      cell(m.layers.ground, x, y, ground); cell(m.layers.objects, x, y, object); cell(m.layers.mines, x, y, mine);
      cell(m.rows, x, y, object === 'N' || mine === 'N' ? 'N' : object !== '.' ? object : ground);
    };
    put(1, 1, 'D', 'N'); put(2, 1, 'M', 'T', 'N'); put(3, 1, 'W', '='); put(4, 1, '+', 'R'); put(5, 1, '.', 'B');
    m.layers.materials = [{ c: 21, material: 'steel' }, { c: 22, material: 'wood' }, { c: 23, material: 'stone' }];
    m.layers.groundMaterials = [{ c: 21, material: 'road' }, { c: 22, material: 'soil' }];
    m.layers.objectMaterials = [{ c: 21, material: 'concrete' }, { c: 22, material: 'stone' }];
    m.layers.mineMaterials = [{ c: 21, material: 'wood' }, { c: 22, material: 'steel' }];
    return m;
  };
  record('boundary:layered-material-precedence', layered());
  { const m = layered(); delete m.layers.mines; cell(m.rows, 2, 1, 'T'); record('boundary:optional-mine-layer', m); }
  { const m = layered(); m.naval = true; record('boundary:layered-naval-mines', m); }
  edit('boundary:house-kind-order', m => {
    for (const [x, y] of [[4, 4], [5, 4], [4, 5], [5, 5]]) cell(m.rows, x, y, 'B');
    m.buildings = [{ x: 4, y: 4, kind: original.CFG.houses[0].name }, { x: 5, y: 5, kind: original.CFG.houses.at(-1).name }];
  });
  for (const [name, fn] of [
    ['building-limit', m => { m.buildings = Array(501).fill({ x: 0, y: 0, kind: 'x' }); }],
    ['trigger-limit', m => { m.triggers = Array(21).fill({ at: 0, say: 'a' }); }],
    ['trigger-work', m => { m.triggers = Array(17).fill({ at: 0, blow: [0, 0, 15, 15] }); }],
    ['trigger-duplicate-id', m => { m.triggers = [{ at: 0, say: 'a' }]; m.points.push({ x: 3, y: 3, id: 'point:0' }); }],
    ['point-limit', m => { m.points = Array(10).fill({ x: 1, y: 1 }); }],
    ['spawn-limit', m => { m.spawns = Array(7).fill({ x: 1, y: 1 }); }],
    ['name-missing', m => { delete m.name; }], ['name-long', m => { m.name = '😀'.repeat(21); }],
    ['width-low', m => { m.w = 19; }], ['width-high', m => { m.w = 1025; }], ['height-fraction', m => { m.h = 20.5; }],
    ['row-count', m => { m.rows.pop(); }], ['row-width', m => { m.rows[0] += '.'; }], ['row-unicode', m => { cell(m.rows, 1, 1, 'é'); }],
    ['row-building', m => { cell(m.rows, 1, 1, 'K'); }], ['row-aux', m => { cell(m.rows, 1, 1, 'A'); }],
    ['height-letter', m => { m.heights = Array(20).fill('c'.repeat(20)); }], ['height-null', m => { m.heights = null; }],
    ['world-version', m => { m.worldVersion = 1; }], ['layers-absent', m => { m.worldVersion = 2; }],
    ['spawn-count', m => { m.spawns.pop(); }], ['spawn-fraction', m => { m.spawns[0].x = .5; }],
    ['spawn-blocked', m => { cell(m.rows, 0, 0, 'B'); }], ['spawn-assault', m => { m.spawns[0].assault = 'yes'; }],
    ['spawn-no-normal', m => { m.spawns[0].assault = true; }], ['defend-duplicate', m => { m.spawns.push({ x: 2, y: 2 }); m.defend = [0, 0]; }],
    ['defend-all', m => { m.defend = [0, 1]; }], ['assault-time', m => { m.assaultTime = 299; }], ['naval', m => { m.naval = 1; }],
    ['trench-facing', m => { m.trenchFacing = null; }], ['points-none', m => { m.points = []; }], ['point-vp', m => { m.points[0].vp = 6; }],
    ['point-kind', m => { m.points[0].kind = 'farm'; }], ['point-self', m => { m.points[0].needs = 0; }],
    ['point-cycle', m => { m.points.push({ x: 9, y: 9, needs: 0 }); m.points[0].needs = 1; }],
    ['point-sequence', m => { m.points[0].id = 'point:12'; m.pointSequence = 12; }],
    ['building-kind', m => { m.buildings = [{ x: 0, y: 0, kind: 'stone bridge' }]; }],
    ['trigger-time', m => { m.triggers = [{ at: -1, say: 'a' }]; }], ['trigger-empty', m => { m.triggers = [{ at: 0 }]; }],
    ['trigger-box', m => { m.triggers = [{ at: 0, blow: [2, 2, 1, 1] }]; }], ['trigger-work-box', m => { m.triggers = [{ at: 0, blow: [0, 0, 19, 19] }]; }],
    ['trigger-point-id', m => { m.triggers = [{ at: 0, say: 'a' }]; m.points[0].id = 'bad id'; }],
  ]) edit(`invalid:${name}`, fn);
  for (const [name, fn] of [
    ['compose', m => { cell(m.rows, 1, 1, '.'); }], ['wet-mine', m => { cell(m.layers.ground, 1, 1, 'W'); }],
    ['bridge-ground', m => { cell(m.layers.ground, 3, 1, '.'); }], ['wet-object', m => { cell(m.layers.ground, 5, 1, 'F'); }],
    ['ground-char', m => { cell(m.layers.ground, 5, 1, 'B'); }], ['object-char', m => { cell(m.layers.objects, 5, 1, 'Q'); }],
    ['mine-char', m => { cell(m.layers.mines, 5, 1, 'X'); }], ['version-absent', m => { delete m.worldVersion; }],
    ['material-duplicate', m => { m.layers.materials.push({ c: 21, material: 'soil' }); }],
    ['material-name', m => { m.layers.mineMaterials[0].material = 'glass'; }], ['material-cell', m => { m.layers.groundMaterials[0].c = 400; }],
  ]) { const m = layered(); fn(m); record(`invalid:layer-${name}`, m); }
  edit('unsupported:structures', m => { m.structures = []; }, 'structures');
  edit('unsupported:scenario', m => { m.scenario = { version: 1 }; }, 'scenario');
  edit('unsupported:world', m => { m.world = {}; }, 'world');
  Math.random = originalRandom;
  clean();
  const content = JSON.stringify({ reference: pin, node: process.version, arch: process.arch, cases }) + '\n';
  const path = join(root, 'rust/crates/core/tests/fixtures/authoritative-maps.json');
  if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), content);
  else writeFileSync(path, content);
  console.error(`${process.argv.includes('--check') ? 'Verified' : 'Recorded'} ${cases.length} map cases (${cases.filter(c => c.terrain).length} terrain states)`);
} finally { rmSync(directory, { recursive: true, force: true }); }
