// Record the unchanged source unitDelta using actual recipient snapshots and scalar boundary rows.
import assert from 'node:assert/strict';
import { readdir, readFile, writeFile, mkdir } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { loadReference, REFERENCE_PIN } from './map-assets.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const reference = resolve(process.env.WW2_REFERENCE || resolve(root, 'reference/ww2-rts'));
await loadReference(reference);
assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const check = process.argv.includes('--check');
const sim = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const wire = value => JSON.parse(JSON.stringify(value));
const traces = [];
function trace(label) {
  let sent = new Map();
  const frames = [];
  traces.push({ label, frames });
  return (rows, full = false, reset = false) => {
    if (reset) sent = new Map();
    const input = wire(rows);
    frames.push({ rows: input, full, reset, expected: wire(sim.unitDelta(sent, input, full)) });
  };
}

const originalRandom = Math.random;
try {
  let seed = 0x3c1055;
  Math.random = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed / 2 ** 32; };
  for (const filename of (await readdir(resolve(reference, 'maps'))).filter(name => name.endsWith('.json')).sort()) {
    const map = JSON.parse(await readFile(resolve(reference, 'maps', filename), 'utf8'));
    const game = sim.createGame(map, ['A', 'B'], false, [0, 1], [0, 1], { mode: 'conquest' });
    const record = [trace(`${filename}:seat0`), trace(`${filename}:seat1`)];
    const capture = (full = false, reset = false) => {
      for (let slot = 0; slot < 2; slot++) record[slot](sim.snapshotFor(game, slot, []).units, full, reset);
    };
    capture();
    capture();
    const [moved, removed] = [...game.units.values()].filter(unit => unit.owner === 0);
    assert.ok(moved && removed);
    moved.x += 3;
    moved.hp -= 1;
    game.units.delete(removed.id);
    capture();
    capture(true);
    game.units.set(removed.id, removed);
    capture();
    capture(false, true);
  }
} finally {
  Math.random = originalRandom;
}

const row = (id, ...tail) => [id, 'rifle', 0, 1, 2, 0, 0, 100, 0, 0, 0, 0, 0, 0, 1, ...tail];
const boundary = trace('insertion order, signed xor, full empty and reset');
boundary([row(0xffffffff), row(7), row(0x80000000)]);
boundary([row(0x80000000), row(7), row(0xffffffff)]);
boundary([row(7)]);
boundary([row(7), row(0x80000000), row(0xffffffff)]);
boundary([row(0xffffffff)]);
boundary([], true);
boundary([row(7)], false, true);
const tails = trace('scalar columns and source shortened-tail comparison');
tails([row(1, 2, 3, 4, 5)]);
tails([row(1)]);
tails([row(1, 2, 3, 4, 6)]);
tails([row(1, null, true, 'tail')]);
tails([row(1, null, true, 'tail')]);
tails([row(1)], true);

const output = JSON.stringify({ reference: REFERENCE_PIN, traces }) + '\n';
const target = resolve(root, 'rust/crates/server/tests/fixtures/snapshot-delta.json');
if (check) assert.equal(await readFile(target, 'utf8'), output, 'snapshot delta fixture differs');
else { await mkdir(dirname(target), { recursive: true }); await writeFile(target, output); }
console.log(`${check ? 'Verified' : 'Recorded'} ${traces.length} recipient traces, ${traces.reduce((sum, t) => sum + t.frames.length, 0)} frames`);
