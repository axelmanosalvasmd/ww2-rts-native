// Reproduce MAP56's selected original query bits. Requires an authorized Source slot.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? resolve(root, 'reference/ww2-rts'));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const baseline = '446671b977abfa6bf689fcb8853833d36519faab';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
function clean() {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
}
const sha256 = raw => createHash('sha256').update(raw).digest('hex');
const view = new DataView(new ArrayBuffer(8));
function bits(number) {
  assert.ok(Number.isFinite(number));
  view.setFloat64(0, number);
  return view.getBigUint64(0).toString(16).padStart(16, '0');
}
clean();
// Same existing pre-structure capture point as terrain-height-fixture.mjs.
// Only expose the original private query; do not replace its implementation.
registerHooks({ load(url, context, next) {
  const result = next(url, context);
  if (url !== pathToFileURL(resolve(reference, 'shared/sim.js')).href) return result;
  const source = String(result.source);
  const marker = '  // The map clients download stays separate from later building footprints and terrain edits.';
  assert.equal(source.split(marker).length, 2);
  assert.equal(source.split('function heightAt(g, x, z) {').length, 2);
  return { ...result, source: source.replace(marker, '  return g;\n' + marker) + '\nexport { heightAt };\n' };
} });
const sim = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const directory = resolve(root, 'docs/fixtures/terrain/source-authored');
const names = readdirSync(directory).filter(name => name.endsWith('.json')).sort();
assert.equal(names.length, 33);
assert.equal(sim.CELL, 2);
assert.equal(sim.CFG.levelHeight, 2.5);
const captures = names.map(name => JSON.parse(readFileSync(resolve(directory, name), 'utf8')));
const selected = [];
for (let i = 0; i < names.length; i++) for (let j = 0; j < 16; j++) selected.push([i, j]);
for (let j = 16; j < 1000 && selected.length < 1000; j++) {
  for (let i = 0; i < names.length && selected.length < 1000; i++) selected.push([i, j]);
}
assert.equal(selected.length, 1000);
const coverage = Object.fromEntries(['negative-coordinate', 'outside-half-open', 'exact-map-boundary',
  'cell-centre', 'cell-edge', 'negative-level', 'interpolated-distinct-from-step', 'height-layer-present', 'fractional-interior-distinct-from-step'].map(key => [key, 0]));
const oldRandom = Math.random;
try {
  const maps = names.map((name, i) => {
    const capture = captures[i];
    assert.equal(capture.producer.kind, 'original-javascript');
    assert.equal(capture.producer.referenceSha, pin);
    assert.equal(capture.producer.node, 'v24.21.0');
    assert.equal(capture.producer.arch, 'x64');
    assert.equal(capture.points.length, 1000);
    const mapJson = readFileSync(resolve(reference, 'maps', name), 'utf8');
    const map = JSON.parse(mapJson);
    assert.deepEqual(map, capture.map);
    assert.equal(sim.validateMap(map), null);
    let draws = 0;
    Math.random = () => { draws++; return 0.5; };
    const g = sim.createGame(map, [], false, [], [], { weather: false });
    assert.equal(draws, capture.producer.initializationRandom.draws);
    Math.random = () => { throw Error('Height query drew randomness'); };
    const queries = selected.filter(([index]) => index === i).map(([, j]) => {
      // Old capture supplies coordinates only. Outputs come from actual pinned queries.
      const [x, z] = capture.points[j];
      const raw = sim.levelAt(g, x, z);
      const metres = raw * sim.CFG.levelHeight;
      const smooth = g.height ? sim.heightAt(g, x, z) : 0;
      const tags = [x < 0 || z < 0, x < 0 || z < 0 || x >= 2 * g.w || z >= 2 * g.h,
        x === 0 || x === 2 * g.w || z === 0 || z === 2 * g.h,
        x % 2 === 1 && z % 2 === 1, x % 2 === 0 || z % 2 === 0, raw < 0,
        metres !== smooth, Object.hasOwn(map, 'heights'),
        metres !== smooth && x > 1 && x < 2 * g.w - 1 && z > 1 && z < 2 * g.h - 1 && x % 2 !== 1 && z % 2 !== 1];
      Object.keys(coverage).forEach((key, index) => { coverage[key] += Number(tags[index]); });
      return { sourcePointIndex: j, inputBits: [bits(x), bits(z)],
        expectedBits: [x, z, raw, metres, smooth].map(bits) };
    });
    return { mapKey: capture.mapKey, sourceCapture: {
      path: `docs/fixtures/terrain/source-authored/${name}`,
      sha256: sha256(readFileSync(resolve(directory, name))), producer: capture.producer },
      mapJson, mapSha256: sha256(mapJson), queries };
  });
  const record = { schemaVersion: 1, reference: pin, selectionBaseline: baseline,
    provenance: 'Initial selection copied unchanged original-javascript capture rows without executing Source or native code. Authorized recorder runs independently reproduce these query bits.',
    selection: { name: 'all33-original-boundary16-then-round-robin-v1', mapOrder: 'source capture filename ascending',
      boundaryPointsPerMap: 16, remainingOrder: 'sourcePointIndex ascending from 16, source capture filename ascending, stop at 1000',
      totalQueries: 1000, expectedColumns: ['x', 'z', 'rawLevel', 'levelMetres', 'heightAtMetres'],
      encoding: '16 lowercase hexadecimal IEEE754 binary64 bits per column' }, coverage, maps };
  const output = JSON.stringify(record) + '\n';
  const path = resolve(root, 'rust/crates/simtest/tests/fixtures/map-height-selected.json');
  if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), output);
  else writeFileSync(path, output);
  console.error(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} 1000 actual pinned height queries on 33 maps`);
} finally {
  Math.random = oldRandom;
  clean();
}
