// Record the pinned server's selected lobby and in-play parsing behaviors over real TCP.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const output = resolve(root, 'rust/crates/server/tests/fixtures/net-socket-predicates.json');
const inputs = JSON.parse(await readFile(resolve(root, 'harness/net-socket-predicates-inputs.json'), 'utf8'));
assert.equal(inputs.reference, pin);
const { createWorldState } = await import(pathToFileURL(resolve(reference, 'client/world-state.js')));
const views = new WeakMap();
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
assert.equal(execFileSync('git', ['-C', reference, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(), pin);
assert.equal(execFileSync('git', ['--no-optional-locks', '-C', reference, 'status', '--porcelain'], { encoding: 'utf8' }).trim(), '');
const require = createRequire(resolve(reference, 'package.json'));
assert.equal(require('ws/package.json').version, '8.22.0');
const WebSocket = require('ws');
const scratch = await mkdtemp(resolve(tmpdir(), 'ww2-net-socket-predicates-'));
const sockets = [];
const children = [];

class Client {
  constructor(socket) {
    this.socket = socket;
    this.queue = [];
    this.pending = null;
    this.closed = null;
    socket.on('message', bytes => {
      const message = JSON.parse(bytes.toString());
      if (this.pending) {
        const pending = this.pending;
        this.pending = null;
        clearTimeout(pending.timer);
        pending.resolve(message);
      } else this.queue.push(message);
    });
    this.closePromise = new Promise(resolve => socket.on('close', (code, reason) => {
      this.closed = { code, reason: reason.toString() };
      resolve(this.closed);
    }));
    socket.on('error', error => { this.error = error.message; });
    sockets.push(this);
  }
  async next() {
    if (this.queue.length) return this.queue.shift();
    assert.equal(this.pending, null);
    return new Promise((resolve, reject) => {
      this.pending = { resolve, timer: setTimeout(() => {
        this.pending = null;
        reject(new Error(`Missing packet; closed=${JSON.stringify(this.closed)}, error=${this.error}`));
      }, 5000) };
    });
  }
  send(message) { this.socket.send(JSON.stringify(message)); }
  text(text, options) { this.socket.send(text, options); }
  async closeFrame() {
    let timer;
    try {
      return await Promise.race([this.closePromise, new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error('Missing close frame')), 5000);
      })]);
    } finally { clearTimeout(timer); }
  }
  async close() {
    if (!this.closed) this.socket.close();
    return this.closeFrame();
  }
}

function lobby(message) {
  assert.equal(message.t, 'lobby');
  return {
    code: message.code, state: message.state, mode: message.mode, you: message.you,
    players: message.players.map(({ name, connected, ai, team, faction, level }) => ({ name, connected, ai, team, faction, level })),
  };
}
async function connect(base, path) {
  const socket = new WebSocket(base.replace('http:', 'ws:') + path, { perMessageDeflate: false });
  const client = new Client(socket);
  await new Promise((resolve, reject) => { socket.once('open', resolve); socket.once('error', reject); });
  return client;
}
async function join(base, code, token) {
  const client = await connect(base, '/ws?room=' + code);
  client.send({ t: 'hello', token, name: token });
  return { client, initial: lobby(await client.next()) };
}
async function startSource(maxRooms) {
  const environment = { ...process.env };
  delete environment.MAX_ROOMS;
  if (maxRooms !== undefined) environment.MAX_ROOMS = maxRooms;
  const child = spawn(process.execPath, [resolve(reference, 'server.js')], {
    cwd: reference,
    env: { ...environment, HOST: '127.0.0.1', PORT: '0', PUBLIC_URL: 'https://net-session.invalid', EDIT_PASSWORD: 'source-fixture-only', SAVES_DIR: resolve(scratch, 'unused-saves'), WW2_SHARDS: '1', WW2_DIAG: '0', WW2_TICKLOG: '0' },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  children.push(child);
  const exited = new Promise(resolve => child.once('exit', (code, signal) => resolve({ code, signal })));
  const base = await new Promise((resolve, reject) => {
    let log = '';
    const timer = setTimeout(() => reject(new Error('Source did not bind: ' + log)), 15000);
    const append = bytes => {
      process.stderr.write(bytes);
      log += bytes;
      const found = /ww2-rts on http:\/\/127\.0\.0\.1:(\d+)/.exec(log);
      if (found) { clearTimeout(timer); resolve('http://127.0.0.1:' + found[1]); }
    };
    child.stdout.on('data', append); child.stderr.on('data', append);
    child.once('error', reject);
    child.once('exit', code => { clearTimeout(timer); reject(new Error(`Source exited ${code}: ${log}`)); });
  });
  return { child, base, exited };
}
async function stopSource({ child, exited }) {
  if (child.exitCode === null && child.signalCode === null) child.kill('SIGTERM');
  return exited;
}
const units = text => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
const roster = m => m.players.map(({ name, team, faction, connected, ai, level }) => ({ nameUnits: units(name), team, faction, connected, ai, level }));
async function until(client, predicate) {
  for (let i = 0; i < 1024; i++) {
    const message = await client.next();
    if (predicate(message)) return message;
  }
  throw new Error('No matching Source message');
}
async function barrier(client, marker) {
  const before = [];
  client.send({ t: 'ping', c: marker });
  for (;;) {
    const message = await client.next();
    if (message.t === 'pong' && message.c === marker) return { before, pong: { t: message.t, c: message.c } };
    before.push(message);
  }
}
async function receiveState(client) {
  const message = await client.next();
  let view = views.get(client);
  if (!view) { view = createWorldState(); views.set(client, view); }
  if (message.t === 'start') view.reset();
  if (message.t === 's') assert.equal(view.apply(structuredClone(message)), false, 'actual Source client held fingerprint');
  return message;
}
async function untilState(client, predicate) {
  for (let i = 0; i < 1024; i++) {
    const message = await receiveState(client);
    if (predicate(message)) return message;
  }
  throw new Error('No matching stateful Source packet');
}
async function stateBarrier(client, marker) {
  const before = [];
  client.send({ t: 'ping', c: marker });
  for (;;) {
    const message = await receiveState(client);
    if (message.t === 'pong' && message.c === marker) return { before, pong: { t: message.t, c: message.c } };
    before.push(message);
  }
}
function full(snapshot) {
  const ids = snapshot.units.map(row => row[0]);
  assert.ok(ids.length > 0);
  assert.equal(new Set(ids).size, ids.length);
  assert.equal(snapshot.gone, undefined);
  assert.deepEqual(snapshot.held, [ids.length, ids.reduce((xor, id) => xor ^ id, 0)]);
  return { nonempty: true, unique: true, noGone: true, completeHeld: true };
}
async function recordPlay() {
  const source = await startSource();
  const host = (await join(source.base, 'ABC', 'a')).client;
  const guest = (await join(source.base, 'ABC', 'b')).client;
  await until(host, m => m.t === 'lobby' && m.players.length === 2);
  host.send({ t: 'start' });
  const start = await until(host, m => m.t === 'start');
  assert.equal((await until(guest, m => m.t === 'start')).matchId, start.matchId);
  const before = await until(host, m => m.t === 's');
  for (const text of inputs.play.inputs) host.text(text);
  const afterBarrier = await barrier(host, inputs.play.pong.c);
  const after = await until(host, m => m.t === 's' && m.tick > before.tick);
  assert.ok(before.units.length > 0);
  await guest.close();
  await until(host, m => m.t === 'pause' && m.paused === true && m.reason === 'drop');
  const replacement = await connect(source.base, '/ws?room=ABC');
  replacement.send({ t: 'hello', token: 'b', name: 'b' });
  const resumed = await until(replacement, m => m.t === 'start');
  assert.equal(resumed.matchId, start.matchId);
  await until(replacement, m => m.t === 'pause' && m.paused === false);
  assert.ok((await until(replacement, m => m.t === 's')).units.length > 0);
  const play = { inputs: inputs.play.inputs, pong: afterBarrier.pong, advanced: after.tick > before.tick, unitsPresent: true, sameMatchAfterReconnect: true, you: resumed.you };
  assert.deepEqual(play, inputs.play, 'preserve all thirteen original recorded play inputs and predicates');
  await stopSource(source);
  return play;
}
async function recordReplacement() {
  const source = await startSource();
  const host = (await join(source.base, 'ABC', 'a')).client;
  const guest = (await join(source.base, 'ABC', 'b')).client;
  await until(host, m => m.t === 'lobby' && m.players.length === 2);
  host.send({ t: 'start' });
  const firstStart = await untilState(host, m => m.t === 'start');
  assert.equal(views.get(host).size, 0);
  const initial = full(await untilState(host, m => m.t === 's'));
  await untilState(guest, m => m.t === 'start');
  full(await untilState(guest, m => m.t === 's'));
  host.send({ t: 'team', slot: 0, v: 5 });
  host.send({ t: 'faction', slot: 0, v: 3 });
  await stateBarrier(host, 'settings-refused-in-play');
  assert.ok(views.get(host).size > 0);
  const replacement = await connect(source.base, '/ws?room=ABC');
  views.set(replacement, views.get(host));
  replacement.send({ t: 'hello', token: 'a', name: 'Replacement' });
  await until(host, m => m.t === 'replaced');
  const closed = await host.closeFrame();
  const incoming = await untilState(replacement, m => m.t === 'start');
  assert.equal(incoming.matchId, firstStart.matchId);
  assert.equal(incoming.you, firstStart.you);
  assert.deepEqual(incoming.teams, firstStart.teams, 'team changes remain refused in play');
  assert.deepEqual(incoming.factions, firstStart.factions, 'faction changes remain refused in play');
  assert.equal(views.get(replacement).size, 0, 'start clears actual warmed prior rows');
  const rejoined = full(await untilState(replacement, m => m.t === 's'));
  const alive = await stateBarrier(guest, 'after-replace');
  assert.equal(alive.before.some(m => m.t === 'pause' && m.reason === 'drop' && m.paused), false);
  await guest.close();
  await untilState(replacement, m => m.t === 'pause' && m.paused && m.reason === 'drop');
  const returned = await connect(source.base, '/ws?room=ABC');
  views.set(returned, views.get(guest));
  returned.send({ t: 'hello', token: 'b', name: 'b' });
  const dropStart = await untilState(returned, m => m.t === 'start');
  assert.equal(dropStart.matchId, firstStart.matchId);
  assert.equal(dropStart.you, 1);
  assert.equal(views.get(returned).size, 0);
  const dropFull = full(await untilState(returned, m => m.t === 's'));
  assert.ok(views.get(replacement).size > 0);
  replacement.send({ t: 'restart' });
  const restart = await untilState(replacement, m => m.t === 'start' && m.matchId !== firstStart.matchId);
  assert.equal(restart.matchId, firstStart.matchId + 1);
  assert.equal(views.get(replacement).size, 0);
  const restarted = full(await untilState(replacement, m => m.t === 's'));
  await stopSource(source);
  return { replaced: { t: 'replaced' }, oldClose: closed, sameSeat: true, sameMatch: true, noDropPause: true, initial, rejoined, dropFull, restarted, startClearsWarmedRows: true, restartIncrements: true, settingsRefusedInPlay: true, roles: 'two human commanders only', unobserved: ['private object identity', 'AI/commander WeakMap reset', 'late-delivery injection', 'manual pause', 'spectator/operative variants'] };
}
async function recordMalformed() {
  const source = await startSource();
  const client = await connect(source.base, '/ws?room=Malformed');
  for (const text of inputs.invalid) client.text(text);
  client.send({ t: 'hello', token: 'malformed', name: 'malformed' });
  const first = await client.next();
  assert.equal(first.t, 'lobby');
  for (const text of inputs.invalid) client.text(text);
  const after = await barrier(client, 'after-invalid');
  assert.deepEqual(after.before, []);
  await stopSource(source);
  return { invalid: inputs.invalid, first: roster(first), pong: after.pong };
}
async function recordSlots() {
  const source = await startSource(), cases = [];
  for (const input of inputs.slotCases) {
    const host = (await join(source.base, input.code, 'Host')).client;
    let guest;
    if (input.guest) {
      guest = (await join(source.base, input.code, 'Guest')).client;
      await until(host, m => m.t === 'lobby' && m.players.length === 2);
    }
    host.send({ t: 'addAi' });
    let current = await until(host, m => m.t === 'lobby' && m.players.length === (guest ? 3 : 2));
    if (guest) await until(guest, m => m.t === 'lobby' && m.players.length === 3);
    const initial = roster(current), steps = [];
    if (input.code === 'Slots') {
      assert.equal(initial.length, 2);
      assert.equal(initial[1].team, 1);
      assert.equal(initial[1].faction, 1);
    }
    for (const [index, action] of input.actions.entries()) {
      const prior = roster(current);
      const actor = action.actor === 'guest' ? guest : host;
      actor.send(action.message);
      if (action.broadcast) {
        current = await until(host, m => m.t === 'lobby');
        if (guest) await until(guest, m => m.t === 'lobby');
      }
      const beforeProbe = await barrier(actor, 'slot-' + index);
      assert.deepEqual(beforeProbe.before, [], 'the named slot guard emits no stray messages');
      // An ordinary accepted no-op broadcasts a fresh actual roster after the invalid command.
      const probe = { t: 'team', slot: 0, v: current.players[0].team };
      host.send(probe);
      current = await until(host, m => m.t === 'lobby');
      const actual = roster(current);
      if (!action.broadcast) assert.deepEqual(actual, prior, 'invalid slot/value/authority preserves every roster field');
      else if (action.message.t === 'kick') assert.equal(actual.length, prior.length - 1);
      else assert.equal(actual[action.message.slot][action.message.t], action.message.v);
      if (guest) assert.deepEqual(roster(await until(guest, m => m.t === 'lobby')), actual);
      steps.push({ ...action, barrier: beforeProbe.pong, probe, roster: actual });
    }
    cases.push({ code: input.code, guest: input.guest, initial, steps });
  }
  await stopSource(source);
  return cases;
}
try {
  const play = await recordPlay();
  const malformed = await recordMalformed();
  const replacement = await recordReplacement();
  const slots = await recordSlots();
  const fixture = { reference: pin, recorded: true, rows: inputs.rows, scope: 'Selected complete predicates for NET-05/34 and ordinary-human start/reset NET-68. No NET-16/20/22/25 completion.', play, malformed, replacement, slots };
  const bytes = JSON.stringify(fixture, null, 2) + '\n';
  if (process.argv.includes('--check')) assert.equal(await readFile(output, 'utf8'), bytes, 'Source socket predicate fixture changed');
  else await writeFile(output, bytes);
  console.log('PASS recorded original Source socket predicates: malformed play/text, numeric slots, start/reconnect decoder resets');
} finally {
  for (const client of sockets) client.socket.terminate();
  for (const child of children) if (child.exitCode === null && child.signalCode === null) {
    const exited = new Promise(resolve => child.once('exit', resolve));
    child.kill('SIGTERM'); await exited;
  }
  await rm(scratch, { recursive: true, force: true });
}
