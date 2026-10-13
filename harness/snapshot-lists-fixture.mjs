// Record the pinned server's unchanged list trimming and actual sim.unitDelta.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { loadReference, REFERENCE_PIN } from './map-assets.mjs';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(a => a === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
await loadReference(reference);
const { unitDelta } = await import(pathToFileURL(`${reference}/shared/sim.js`));
const source = readFileSync(`${reference}/server.js`, 'utf8');
const begin = "const LISTS = ['wrecks', 'nodes'];";
const end = 'const sendSeat = ';
assert.equal(source.split(begin).length, 2);
assert.equal(source.split(end).length, 2);
const start = source.indexOf(begin), stop = source.indexOf(end, start);
assert.ok(stop > start);
const trimmed = new Function('unitDelta', `${source.slice(start, stop)}\nreturn trimmed;`)(unitDelta);
const clone = value => JSON.parse(JSON.stringify(value));
const row = (id, x = id * 2, hp = 100) => [id, 'rifle', id % 2, x, 4, 0, hp, 0, 0, 0, 0, 0, 0, 1, 0];
const wreck = (id, motion) => ({ id, type: 'tank', owner: 0, x: 12, z: 14, rot: 0, ...(motion === undefined ? {} : { motion }) });
const node = (id, stock = 20) => ({ id, x: 7, z: 9, kind: 'fuel', stock });
const traces = [];
function trace(name) {
  const room = {}, nets = new Map(), caches = new Map();
  const steps = [];
  const push = ({ mode = 'ordinary', recipient = 'seat:0', cacheId = 1, cache = {}, message, reset = false, full = false }) => {
    assert.ok(Number.isSafeInteger(cacheId) && cacheId > 0);
    assert.ok(['ordinary', 'world'].includes(mode));
    const input = clone({ mode, recipient, cacheId, cache, message, reset, full });
    for (const unit of message.units) {
      assert.equal(unit.length, 15);
      assert.ok(unit.every(v => v === null || ['string', 'number', 'boolean'].includes(typeof v)));
    }
    room.mode = mode === 'world' ? 'world' : 'conquest';
    let shared = caches.get(cacheId);
    if (!shared) { shared = {}; caches.set(cacheId, shared); }
    for (const key of Object.keys(shared)) delete shared[key];
    Object.assign(shared, clone(cache));
    if (reset || !nets.has(recipient)) nets.set(recipient, { sent: new Map() });
    const net = nets.get(recipient);
    if (full) net.full = true;
    const expected = clone(trimmed(room, net, clone(message), shared));
    steps.push({ ...input, expected });
    return expected;
  };
  traces.push({ name, steps });
  return push;
}
const message = (units, lists = {}, extra = {}) => ({ t: 's', tick: 1, units, shots: [], paused: false, ...lists, ...extra });

// Ordinary lists use room-wide raw-cache versions, with per-stream acknowledgments.
{
  const push = trace('ordinary-shared-cache-identity-and-versions');
  const a = { wrecks: [wreck(1)], nodes: [node(1)] };
  const first = push({ cache: a, message: message([row(1), row(2)], a) });
  assert.equal(first.units.length, 2); assert.ok('wrecks' in first && 'nodes' in first);
  const same = push({ cache: a, message: message([row(1), row(2)], a) });
  assert.deepEqual(same.units, []); assert.ok(!('wrecks' in same) && !('nodes' in same));
  // A mutation of the same raw-cache object deliberately does not update room list versions.
  const mutated = { wrecks: [wreck(1), wreck(3)], nodes: [node(1, 19)] };
  const identity = push({ cache: mutated, message: message([row(1, 3), row(2)], mutated) });
  assert.ok(!('wrecks' in identity) && !('nodes' in identity));
  const refreshed = push({ cacheId: 2, cache: mutated, message: message([row(1, 3), row(2)], mutated) });
  assert.ok('wrecks' in refreshed && 'nodes' in refreshed);
  const unchanged = push({ cacheId: 3, cache: mutated, message: message([row(2)], mutated) });
  assert.deepEqual(unchanged.gone, [1]); assert.ok(!('wrecks' in unchanged));
  push({ cacheId: 4, cache: { ...mutated, nodes: [node(1, 18)] }, message: message([row(2)], { ...mutated, nodes: [node(1, 18)] }) });
  push({ cacheId: 5, cache: mutated, message: message([row(2), row(4)], mutated) });
}
{
  const push = trace('ordinary-independent-seat-operative-and-shared-watch-streams');
  const cache = { wrecks: [wreck(1)], nodes: [node(1)] };
  for (const recipient of ['seat:0', 'seat:1', 'operative:0', 'watch:shared'])
    push({ recipient, cache, message: message([row(1), row(2)], cache) });
  for (const recipient of ['seat:0', 'seat:1', 'operative:0', 'watch:shared'])
    push({ recipient, cache, message: message([row(1), row(2)], cache) });
  // A late ordinary spectator forces a complete shared stream, retaining its prior sent map.
  const joined = push({ recipient: 'watch:shared', full: true, cache, message: message([row(1), row(2)], cache) });
  assert.equal(joined.all, true); assert.equal(joined.units.length, 2); assert.ok('nodes' in joined);
  push({ recipient: 'watch:shared', cache, message: message([row(1), row(2)], cache) });
  push({ recipient: 'seat:0', full: true, cache, message: message([row(2)], cache) });
  const reset = push({ recipient: 'seat:0', reset: true, cache, message: message([row(2)], cache) });
  assert.equal(reset.units.length, 1); assert.ok(!('all' in reset) && 'nodes' in reset);
  push({ recipient: 'operative:0', reset: true, full: true, cache, message: message([], cache) });
}
{
  const push = trace('ordinary-recipient-hidden-lists-follow-global-versions');
  const cache = { wrecks: [wreck(1), wreck(2)], nodes: [node(1), node(2)] };
  const visible = { wrecks: [wreck(1)], nodes: [node(1)] };
  const hidden = { wrecks: [], nodes: [] };
  push({ cache, message: message([row(1)], visible) });
  const suppressed = push({ cache, message: message([row(1)], hidden) });
  assert.ok(!('wrecks' in suppressed) && !('nodes' in suppressed));
  push({ recipient: 'seat:1', cache, message: message([row(2)], hidden) });
  push({ cacheId: 2, cache: { ...cache, nodes: [node(1), node(2, 19)] }, message: message([row(1)], hidden) });
  push({ cacheId: 2, cache: { ...cache, nodes: [node(1), node(2, 19)] }, full: true, message: message([row(1)], visible) });
}
{
  const push = trace('world-per-recipient-list-content-and-independent-streams');
  const cache = { wrecks: [wreck(1), wreck(2)], nodes: [node(1), node(2)] };
  const a = { wrecks: [wreck(1)], nodes: [node(1)] }, b = { wrecks: [wreck(2)], nodes: [] };
  for (const recipient of ['seat:0', 'seat:1', 'watch:0', 'watch:1'])
    push({ mode: 'world', recipient, cache, message: message([row(1)], recipient.endsWith(':0') ? a : b) });
  push({ mode: 'world', cache, message: message([row(1)], a) });
  const changed = push({ mode: 'world', cache, message: message([row(1)], b) });
  assert.ok('wrecks' in changed && 'nodes' in changed);
  const untouched = push({ mode: 'world', recipient: 'seat:1', cache, message: message([row(1)], b) });
  assert.ok(!('wrecks' in untouched) && !('nodes' in untouched));
  push({ mode: 'world', cacheId: 2, cache: { wrecks: [], nodes: [] }, message: message([row(1)], b) });
  push({ mode: 'world', full: true, cache, message: message([row(1)], b) });
  push({ mode: 'world', reset: true, full: true, cache, message: message([], a) });
}
{
  const push = trace('absent-null-empty-lists-and-wreck-motion');
  for (const mode of ['ordinary', 'world']) {
    const recipient = `seat:${mode}`;
    let cacheId = mode === 'ordinary' ? 1 : 20;
    for (const lists of [
      { wrecks: [] }, { wrecks: [], nodes: null }, { wrecks: [], nodes: [] },
      { wrecks: [wreck(1)], nodes: [] },
      { wrecks: [wreck(1, null)], nodes: [] },
      { wrecks: [wreck(1, { state: 'moving', vx: 0.25, vz: -0.5, contacts: [1, 2] })], nodes: [] },
      { wrecks: [wreck(1, { state: 'settled', vx: 0, vz: 0, contacts: [] })], nodes: [] },
      { wrecks: [], nodes: [] }, { wrecks: null }, {},
    ]) {
      push({ mode, recipient, cacheId: cacheId++, cache: lists, message: message([row(1)], lists) });
    }
    push({ mode, recipient, cacheId: cacheId++, full: true, cache: {}, message: message([row(1)]) });
  }
}
{
  // Exercise trimmed's branch changes; this does not claim server room-transition orchestration.
  const push = trace('mode-branch-and-recipient-role-switching');
  const cache = { wrecks: [wreck(1)], nodes: [node(1)] };
  for (const mode of ['ordinary', 'world', 'world', 'ordinary', 'ordinary'])
    push({ mode, cache, message: message([row(1)], cache) });
  push({ recipient: 'watch:shared', cache, full: true, message: message([row(1), row(2)], cache, { role: 'spectator' }) });
  push({ mode: 'world', recipient: 'watch:0', cache, full: true, message: message([row(1)], cache, { role: 'spectator' }) });
  push({ recipient: 'seat:0', cache, reset: true, message: message([row(2)], cache, { role: 'seat' }) });
}
{
  const push = trace('stale-unit-delta-fields-are-overwritten');
  const cache = { wrecks: [], nodes: [] };
  const stale = { all: true, gone: [999], held: [99, 999] };
  for (const mode of ['ordinary', 'world']) {
    const recipient = `stale:${mode}`;
    push({ mode, recipient, cache, message: message([row(1), row(2)], cache, stale) });
    const quiet = push({ mode, recipient, cache, message: message([row(1), row(2)], cache, stale) });
    assert.deepEqual(quiet.units, []);
    assert.ok(!('gone' in quiet) && !('all' in quiet));
    assert.deepEqual(quiet.held, [2, 3]);
    const changed = push({ mode, recipient, cache, message: message([row(1, 3), row(2)], cache, stale) });
    assert.deepEqual(changed.units, [row(1, 3)]);
    assert.ok(!('gone' in changed) && !('all' in changed));
    assert.deepEqual(changed.held, [2, 3]);
    const removed = push({ mode, recipient, cache, message: message([row(1, 3)], cache, stale) });
    assert.deepEqual(removed.gone, [2]);
    assert.ok(!('all' in removed));
    assert.deepEqual(removed.held, [1, 1]);
    const full = push({ mode, recipient, cache, full: true, message: message([row(1, 3)], cache, stale) });
    assert.deepEqual(full.units, [row(1, 3)]);
    assert.equal(full.all, true);
    assert.ok(!('gone' in full));
    assert.deepEqual(full.held, [1, 1]);
  }
}
await loadReference(reference);
const output = JSON.stringify({ format: 1, reference: REFERENCE_PIN, node: process.version, arch: process.arch, traces }) + '\n';
const destination = `${root}/rust/crates/server/tests/fixtures/snapshot-lists.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(destination, 'utf8'), output);
else writeFileSync(destination, output);
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} ${traces.length} original trimming traces, ${traces.reduce((n, t) => n + t.steps.length, 0)} steps`);
