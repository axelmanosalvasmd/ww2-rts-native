// Record only pinned result-seat helpers, without importing the server or its credentials.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadReference, REFERENCE_PIN } from './map-assets.mjs';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
await loadReference(reference);
const server = readFileSync(`${reference}/server.js`, 'utf8');
function section(begin, end) {
  assert.equal(server.split(begin).length, 2);
  const first = server.indexOf(begin), last = server.indexOf(end, first);
  assert.ok(last > first);
  return server.slice(first, last);
}
const original = section('const SLOT_LISTS = ', 'function freeOfflineSeats(')
  + section('function addSeat(', 'function finishMatch(');
// The sole timer dependency cancels an opaque cleanup identity. No host time,
// scheduled callback, socket, result serializer or server module runs here.
const cleared = [];
const clock = { clearTimeout: identity => cleared.push(identity) };
const helpers = new Function('clock', `${original}\nreturn {retainSeats, addSeat};`)(clock);
const player = name => ({ name, cleanupTimer: `cleanup:${name}`, lastMatch: { team: name.charCodeAt(0) - 97, outcome: 'victory' } });
const initialReport = { teams: [0, 1, 2, 8], names: ['a', 'b', 'c', 'past'], story: [{ kills: 2 }, { kills: 5 }, { kills: 9 }, { kills: 20 }] };
const traces = [];
const snapshot = room => structuredClone({ players: room.players.map(({ name, lastMatch }) => ({ name, lastMatch })), result: room.result });
function trace(name, report, actions) {
  const room = { players: ['a', 'b', 'c'].map(player), result: structuredClone(report) };
  const people = new Map(room.players.map(p => [p.name, p]));
  const initial = snapshot(room);
  const steps = actions.map(action => {
    if (action.op === 'retain') {
      helpers.retainSeats(room, room.players.filter(p => action.names.includes(p.name)));
    } else {
      assert.equal(action.op, 'add');
      if (!people.has(action.name)) people.set(action.name, player(action.name));
      helpers.addSeat(room, people.get(action.name));
    }
    return { ...action, expected: snapshot(room) };
  });
  traces.push({ name, initial, steps });
}
trace('retained-report-prefix-and-historical-tail', initialReport, [
  { op: 'retain', names: ['b', 'c'] }, { op: 'add', name: 'd' },
  { op: 'retain', names: ['c', 'd'] }, { op: 'add', name: 'a' },
  { op: 'retain', names: ['c', 'd', 'a'] },
]);
trace('all-offline-then-seat-refill', initialReport, [
  { op: 'retain', names: [] }, { op: 'add', name: 'b' }, { op: 'add', name: 'e' },
  { op: 'retain', names: ['e'] },
]);
trace('lobby-without-prior-result', null, [
  { op: 'retain', names: ['a', 'c'] }, { op: 'add', name: 'd' },
]);
assert.equal(cleared.length, 7);
await loadReference(reference);
const output = JSON.stringify({ format: 1, reference: REFERENCE_PIN,
  sourceSha256: createHash('sha256').update(original).digest('hex'),
  node: process.version, arch: process.arch, traces }) + '\n';
const destination = `${root}/rust/crates/server/tests/fixtures/room-lobby-result.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(destination, 'utf8'), output);
else writeFileSync(destination, output);
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} ${traces.length} result-roster traces/${traces.reduce((n, trace) => n + trace.steps.length, 0)} steps`);
