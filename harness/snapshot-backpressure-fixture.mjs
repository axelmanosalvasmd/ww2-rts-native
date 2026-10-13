// Execute pinned snapshot backpressure and watcher admission without starting the server.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadReference, REFERENCE_PIN } from './map-assets.mjs';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(a => a === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
await loadReference(reference);
const source = readFileSync(`${reference}/server.js`, 'utf8');
function between(begin, end) {
  assert.equal(source.split(begin).length, 2, `unique ${begin}`);
  assert.equal(source.split(end).length, 2, `unique ${end}`);
  const start = source.indexOf(begin), stop = source.indexOf(end, start);
  assert.ok(stop > start);
  return source.slice(start, stop);
}
const backpressure = between('function backedUp(p)', 'export function tickRooms()');
const watchers = between('const WATCH_BEHIND = ', '// Each companion owns an independent delta/terrain/fog stream.');
const sockets = between('const send = (ws, msg)', '// Spectators have no seat:');
const { connected, send } = new Function(`${sockets}\nreturn {connected,send};`)();
const ascii = length => JSON.stringify('x'.repeat(length - 2));
const traces = [];
function trace(name, actions) {
  const callbacks = new Map();
  const oldSocket = { readyState: 1, send(_json, callback) { callbacks.set(activeId, callback); } };
  const p = { ws: oldSocket, rtt: 14 }, room = { players: [p], state: 'play', game: {} };
  let activeId, catches = 0;
  const { backedUp, sendSnapshot } = new Function('connected', 'sendSeat', 'snapshotCache', `${backpressure}\nreturn {backedUp,sendSnapshot};`)(
    connected,
    (r, i, shots, cells, _cache, extra) => {
      assert.equal(r, room); assert.equal(i, 0);
      assert.deepEqual(shots, []); assert.deepEqual(cells, []);
      assert.deepEqual(extra, { online: [true], ping: [14] });
      catches++;
    },
    _game => ({}),
  );
  const steps = [];
  for (const action of actions) {
    const before = catches;
    let skip;
    if (action.op === 'send') {
      assert.ok(!callbacks.has(action.id), 'write IDs are unique');
      assert.equal(p.ws, oldSocket, 'send uses the original socket');
      activeId = action.id;
      sendSnapshot(room, p, action.json);
    } else if (action.op === 'admit') {
      skip = !!backedUp(p);
    } else {
      assert.equal(action.op, 'complete');
      const callback = callbacks.get(action.id);
      assert.ok(callback, 'one completion per pending write');
      callbacks.delete(action.id);
      p.ws = action.currentSocket ? oldSocket : { readyState: 1 };
      room.players = action.seatPresent ? [p] : [];
      room.state = action.playing ? 'play' : 'lobby';
      room.game = action.hasGame ? {} : null;
      callback();
    }
    const expected = {
      inflight: oldSocket.inflight ?? 0,
      owed: !!oldSocket.owed,
      ...(action.op === 'admit' ? { skip } : {}),
      ...(action.op === 'complete' ? { catchUp: catches > before } : {}),
    };
    assert.ok(Number.isSafeInteger(expected.inflight) && expected.inflight >= 0);
    steps.push({ ...action, expected });
  }
  assert.equal(callbacks.size, 0, 'all writes completed');
  traces.push({ name, steps });
}
const sendWrite = (id, json) => ({ op: 'send', id, json });
const admit = { op: 'admit' };
const complete = (id, options = {}) => ({ op: 'complete', id, currentSocket: true, seatPresent: true, playing: true, hasGame: true, ...options });
const gate = 256 * 1024;
trace('below-boundary-admission', [admit, sendWrite(1, ascii(gate - 1)), admit, complete(1), admit]);
trace('exact-boundary-owed-and-catch-up', [sendWrite(1, ascii(gate)), admit, admit, complete(1), admit]);
trace('completion-still-at-boundary-keeps-owed', [sendWrite(1, ascii(gate)), sendWrite(2, ascii(2)), admit, complete(2), admit, complete(1)]);
trace('one-below-boundary-completion-catches-up', [sendWrite(1, ascii(gate - 1)), sendWrite(2, ascii(2)), admit, complete(2), complete(1)]);
trace('mixed-out-of-order-writes', [sendWrite(1, ascii(180000)), sendWrite(2, ascii(100000)), sendWrite(3, ascii(20000)), admit, complete(2), admit, complete(3), complete(1)]);
trace('stale-socket-keeps-old-owed', [sendWrite(1, ascii(gate)), admit, complete(1, { currentSocket: false })]);
for (const options of [{ seatPresent: false }, { playing: false }, { hasGame: false }, { seatPresent: false, playing: false, hasGame: false }]) {
  const name = Object.keys(options).join('-');
  trace(`owed-clears-before-invalid-${name}`, [sendWrite(1, ascii(gate)), admit, complete(1, options)]);
}
trace('valid-completion-without-owed-does-not-catch-up', [sendWrite(1, '{}'), complete(1)]);
const unicode = JSON.stringify('😀'.repeat((gate - 2) / 2));
assert.equal(unicode.length, gate);
assert.ok(Buffer.byteLength(unicode, 'utf8') > gate);
trace('unicode-utf16-length-exact-gate', [sendWrite(1, unicode), admit, complete(1)]);
trace('unicode-small-write-and-empty-string', [sendWrite(1, '"😀é漢"'), sendWrite(2, ''), admit, complete(2), complete(1)]);

const sendWatchers = new Function('connected', 'send', 'worldWatchGame', 'watchGame', 'watcherSnapshot', 'trimmed', `${watchers}\nreturn sendWatchers;`)(
  connected, send, (g, _watcher) => g, (g, _memory, _pending) => g,
  (_view, _shots, _cells, _cache, _extra) => ({ t: 's', units: [] }),
  (_room, _net, msg, _cache) => msg,
);
const watcherCases = [];
for (const connectedValue of [false, true]) {
  for (const buffered of [null, 0, 4 * 1024 * 1024 - 1, 4 * 1024 * 1024, 4 * 1024 * 1024 + 1, 8 * 1024 * 1024]) {
    let expected;
    // Both actual branches apply the same cutoff. View producers are the only game substitutes.
    for (const mode of ['conquest', 'world']) {
      const actions = [];
      const ws = {
        readyState: connectedValue ? 1 : 3,
        ...(buffered === null ? {} : { bufferedAmount: buffered }),
        close(code, reason) {
          assert.equal(code, 1013); assert.equal(reason, 'too far behind');
          actions.push('close'); this.readyState = 2;
        },
        send(json) { assert.deepEqual(JSON.parse(json), { t: 's', units: [] }); actions.push('send'); },
      };
      sendWatchers({ spectators: [{ ws }], game: { mode: { kind: mode }, cellLog: new Map() } }, [], [], {}, {});
      assert.ok(actions.length <= 1);
      const outcome = actions[0] ?? 'disconnected';
      if (expected === undefined) expected = outcome;
      else assert.equal(outcome, expected);
    }
    watcherCases.push({ connected: connectedValue, buffered, expected });
  }
}
await loadReference(reference);
const output = JSON.stringify({ format: 1, reference: REFERENCE_PIN, node: process.version, arch: process.arch, traces, watcherCases }) + '\n';
const destination = `${root}/rust/crates/server/tests/fixtures/snapshot-backpressure.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(destination, 'utf8'), output);
else writeFileSync(destination, output);
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} ${traces.length} original backpressure traces, ${traces.reduce((n, t) => n + t.steps.length, 0)} steps, ${watcherCases.length} watcher cases in both source branches`);
