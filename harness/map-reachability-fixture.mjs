// MAP-105: the complete shipped-map check in the unchanged pinned test.js.
// Record only when authorized. --check compares bytes without writing a fixture.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(value => value === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? resolve(root, 'reference/ww2-rts'));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();

// Expose the actual near-wall reader for an observational input capture.
// Keep createGame, clearFixtureUnits, findPath and navigation algorithms unchanged.
const simUrl = pathToFileURL(resolve(reference, 'shared/sim.js')).href;
registerHooks({ load(url, context, next) {
  const loaded = next(url, context);
  if (url !== simUrl) return loaded;
  const source = String(loaded.source);
  assert.equal(source.split('function nearWalls(g) {').length, 2);
  return { ...loaded, source: source + '\nexport { nearWalls };\n' };
} });
const sim = await import(simUrl);
const { clearFixtureUnits } = await import(pathToFileURL(resolve(reference, 'test-fixtures.js')));
const { isGroundVehicle } = await import(pathToFileURL(resolve(reference, 'shared/vehicle-motion.js')));
assert.equal(sim.UNITS[undefined], undefined);
assert.equal(sim.blockOf(undefined), sim.MOVE);
assert.equal(isGroundVehicle(undefined), false);

const runs = values => {
  const packed = [];
  for (const value of values) {
    if (packed.length && packed.at(-1)[0] === value) packed.at(-1)[1]++;
    else packed.push([value, 1]);
  }
  return packed;
};
const floatRuns = values => runs(new Uint32Array(values.buffer, values.byteOffset, values.length));
const numberBits = value => {
  const bytes = Buffer.alloc(8);
  bytes.writeDoubleBE(value);
  return bytes.toString('hex');
};
const pointBits = point => [numberBits(point.x), numberBits(point.z)];
const W = point => ({ x: (point.x + 0.5) * sim.CELL, z: (point.y + 0.5) * sim.CELL });
const files = readdirSync(resolve(reference, 'maps')).filter(file => file.endsWith('.json')).sort();
assert.equal(files.length, 33);
const cases = [];
let queries = 0;
const originalRandom = Math.random;
try {
  for (const filename of files) {
    const map = JSON.parse(readFileSync(resolve(reference, 'maps', filename), 'utf8'));
    const originalMap = structuredClone(map);
    assert.equal(sim.validateMap(map), null, filename);
    let draws = 0;
    Math.random = () => { draws++; return 0.5; };
    // Match test.js:3884 exactly: two players, no shuffle, default options.
    const game = sim.createGame(map, ['a', 'b'], false);
    const removed = [...game.units.values()].map(unit => ({
      id: unit.id, structureId: unit.structureId ?? null, cells: [...unit.cells ?? []],
    }));
    clearFixtureUnits(game);
    assert.equal(game.units.size, 0, `${filename}: clear real starting entities`);
    for (const unit of removed) {
      if (unit.structureId) assert.equal(game.structures.has(unit.structureId), false, filename);
      for (const cell of unit.cells) {
        assert.equal(game.buildingCells.has(cell), false, filename);
        assert.equal(game.structuralCells.has(cell), false, filename);
      }
    }
    assert.deepEqual(map, originalMap, `${filename}: preserve the original map input`);
    assert.equal(game.worldKnown, undefined);
    assert.equal(game.navigationObserved, undefined);
    assert.equal(game.finePathLimit, undefined, 'preserve the default Source expansion limit');
    Math.random = () => { throw Error('The selected reachability check must not draw randomness'); };

    const terrain = {
      chars: game.chars.join(''), ground: game.ground.join(''), objects: game.objects.join(''),
      flags: runs(game.flags), height: game.height ? runs(game.height) : null,
      house: runs(game.house), hp32: floatRuns(game.cellHp),
      mines: runs(game.mineLayer), wear32: floatRuns(game.wear),
      near: runs(sim.nearWalls(game)), roads: !!game.roads, wetness: game.wx?.wet ?? 0,
      fires: [...game.fires.keys()].sort((a, b) => a - b),
      units: game.units.size, buildingCells: game.buildingCells.size,
      buildingStructures: [...game.structures.values()].filter(structure => structure.kind === 'building').length,
    };
    assert.equal(terrain.buildingCells, 0, filename);
    assert.equal(terrain.buildingStructures, 0, filename);
    const recorded = [];
    const query = (spawn, targetKind, target, destination) => {
      const from = W(map.spawns[spawn]), to = W(destination);
      assert.deepEqual(Object.keys(from), ['x', 'z']);
      game.pathStats = { calls: 0, deferred: 0, failed: 0, dropped: 0, expansions: 0, regionRejected: 0 };
      // No type, owner or id: preserve blockOf(undefined) and bypass owned views.
      const path = sim.findPath(game, from, to);
      assert.deepEqual(Object.keys(from), ['x', 'z']);
      const expected = {
        points: path.map(pointBits),
        status: game.pathStats.deferred ? 'ExpansionLimit' : game.pathStats.failed ? 'Unreachable' : 'Found',
      };
      if (path.length) assert.equal(expected.status, 'Found', filename);
      recorded.push({ spawn, targetKind, target, from, to, expected });
      queries++;
      return path.length > 0;
    };
    for (let spawn = 0; spawn < map.spawns.length; spawn++) {
      if (map.naval) {
        // Record all point queries, then enforce the Source per-spawn existential.
        let reachesPoint = false;
        for (let target = 0; target < map.points.length; target++) {
          if (query(spawn, 'point', target, map.points[target])) reachesPoint = true;
        }
        assert.ok(reachesPoint, `${filename}: spawn ${spawn} reaches a point`);
      } else {
        for (let target = 0; target < map.points.length; target++) {
          assert.ok(query(spawn, 'point', target, map.points[target]), `${filename}: spawn ${spawn} reaches point ${target}`);
        }
        for (let target = 0; target < map.spawns.length; target++) {
          if (target !== spawn) assert.ok(query(spawn, 'spawn', target, map.spawns[target]), `${filename}: spawn ${spawn} reaches spawn ${target}`);
        }
      }
    }
    cases.push({ filename, map: originalMap, initializationRandom: { constant: 0.5, draws }, removed, terrain, queries: recorded });
  }
  const fixture = {
    schemaVersion: 1, reference: pin, node: process.version, arch: process.arch,
    producer: 'original-createGame-clearFixtureUnits-findPath',
    selectedCheck: 'test.js:3881-3891',
    profile: { inputFields: ['x', 'z'], definitionAbsent: true, block: sim.MOVE, naval: false, vehicle: false, pull: sim.MOVE | sim.WIRE },
    expansionLimit: 'Source default min(cells,16384)', maps: files, queryCount: queries, cases,
  };
  const output = JSON.stringify(fixture) + '\n';
  const target = resolve(root, 'rust/crates/simtest/tests/fixtures/map-reachability.json');
  clean();
  if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), output);
  else writeFileSync(target, output);
  console.error(`${process.argv.includes('--check') ? 'Verified' : 'Recorded'} ${cases.length} original maps and ${queries} complete reachability queries`);
} finally {
  Math.random = originalRandom;
  clean();
}
