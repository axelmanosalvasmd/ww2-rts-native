// Record the pinned server's selected session-admission behaviors over real TCP.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const output = resolve(root, 'rust/crates/server/tests/fixtures/net-session-batch.json');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
assert.equal(execFileSync('git', ['-C', reference, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(), pin);
assert.equal(execFileSync('git', ['--no-optional-locks', '-C', reference, 'status', '--porcelain'], { encoding: 'utf8' }).trim(), '');
const require = createRequire(resolve(reference, 'package.json'));
assert.equal(require('ws/package.json').version, '8.22.0');
const WebSocket = require('ws');
const scratch = await mkdtemp(resolve(tmpdir(), 'ww2-net-session-'));
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
function sizedPing(bytes, echo) {
  const empty = JSON.stringify({ t: 'ping', c: echo, pad: '' });
  const text = JSON.stringify({ t: 'ping', c: echo, pad: 'x'.repeat(bytes - Buffer.byteLength(empty)) });
  assert.equal(Buffer.byteLength(text), bytes);
  return text;
}

async function startSource() {
  const child = spawn(process.execPath, [resolve(reference, 'server.js')], {
    cwd: reference,
    env: { ...process.env, HOST: '127.0.0.1', PORT: '0', PUBLIC_URL: 'https://net-session.invalid', EDIT_PASSWORD: 'source-fixture-only', SAVES_DIR: resolve(scratch, 'unused-saves'), WW2_SHARDS: '1', WW2_DIAG: '0', WW2_TICKLOG: '0' },
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

try {
  const { base } = await startSource();
  const queries = [];
  for (const [id, path, valid] of [
    ['missing', '/ws', false], ['empty', '/ws?room=', false], ['short', '/ws?room=ab', false],
    ['punctuation', '/ws?room=x!y', false], ['long', '/ws?room=abcdefghijklm', false],
    ['minimum', '/ws?room=abc', true], ['case', '/ws?room=ABC', true],
    ['maximum', '/ws?room=Abcdef123456', true], ['encoded-value', '/ws?room=%50ercent', true],
    ['encoded-key', '/ws?%72oom=KeyRoom', true], ['first-value', '/ws?room=FirstRoom&room=SecondRoom', true],
    ['unrelated-bad-key', '/ws?%=x&room=BadPrefix', true],
    ['unrelated-bad-utf8-key', '/ws?%FF=x&room=UtfPrefix', true],
    ['unrelated-bad-value', '/ws?x=%zz&room=ValueRoom', true],
    ['space', '/ws?room=a+b', false], ['bad-room-escape', '/ws?room=%zz', false],
  ]) {
    console.error(`Source NET-02 ${id}: ${path}`);
    const client = await connect(base, path);
    const entry = { id, path, valid };
    if (valid) {
      client.send({ t: 'hello', token: id, name: id });
      entry.lobby = lobby(await client.next());
      assert.equal(entry.lobby.players.length, 1);
      await client.close();
    } else {
      entry.close = await client.closeFrame();
      assert.deepEqual(entry.close, { code: 1008, reason: 'bad room' });
    }
    queries.push(entry);
  }

  const payloads = [];
  for (const [id, bytes, parts] of [
    ['limit', 16384, [16384]], ['over-limit', 16385, [16385]], ['17-kib', 17408, [17408]],
    ['fragment-limit', 16384, [8192, 8192]], ['fragment-over-limit', 16385, [8192, 8193]],
  ]) {
    console.error(`Source NET-03 ${id}: ${bytes} bytes in ${parts.join('+')}`);
    const service = await startSource();
    const { client } = await join(service.base, 'Size' + payloads.length, id);
    const text = sizedPing(bytes, id);
    let offset = 0;
    for (let i = 0; i < parts.length; i++) {
      client.text(text.slice(offset, offset + parts[i]), { fin: i === parts.length - 1, binary: false });
      offset += parts[i];
    }
    assert.equal(offset, bytes);
    const entry = { id, bytes, parts };
    if (bytes === 16384) {
      entry.pong = await client.next();
      assert.deepEqual(entry.pong, { t: 'pong', c: id });
      await client.close();
    } else {
      entry.close = await client.closeFrame();
      assert.equal(entry.close.code, 1009);
      let timer;
      entry.sourceExit = await Promise.race([service.exited, new Promise(resolve => {
        timer = setTimeout(() => resolve(null), 1000);
      })]);
      clearTimeout(timer);
    }
    await stopSource(service);
    payloads.push(entry);
  }

  const invalid = ['{bad json', 'null', 'false', '7', '"text"', '[]', '[1]', '{}', '{"t":null}', '{"t":7}', '{"t":[]}', '{"t":{}}'];
  const malformedClient = await connect(base, '/ws?room=Malformed');
  for (const text of invalid) malformedClient.text(text);
  malformedClient.send({ t: 'hello', token: 'malformed', name: 'malformed' });
  const malformed = { invalid, lobby: lobby(await malformedClient.next()) };
  for (const text of invalid) malformedClient.text(text);
  malformedClient.send({ t: 'ping', c: 'after-invalid' });
  malformed.pong = await malformedClient.next();
  assert.deepEqual(malformed.pong, { t: 'pong', c: 'after-invalid' });
  malformedClient.send({ t: 'ping', c: 'still-alive' });
  malformed.barrier = await malformedClient.next();
  assert.deepEqual(malformed.barrier, { t: 'pong', c: 'still-alive' });
  await malformedClient.close();

  const maps = await (await fetch(base + '/maps')).json();
  const files = (await readdir(resolve(reference, 'maps'))).filter(f => /^[a-z0-9-]{1,32}\.json$/.test(f)).map(f => f.slice(0, -5)).sort();
  assert.equal(files.length, 33);
  assert.deepEqual(maps, files);

  const seatClients = [], joins = [];
  for (let i = 0; i < 6; i++) {
    const joined = await join(base, 'SeatDefaults', 'seat-' + i);
    seatClients.push(joined.client);
    const update = i === 0 ? joined.initial : lobby(await seatClients[0].next());
    assert.equal(update.players.length, i + 1);
    assert.deepEqual(update.players.map(p => p.team), Array.from({ length: i + 1 }, (_, slot) => slot));
    assert.deepEqual(update.players.map(p => p.faction), Array.from({ length: i + 1 }, (_, slot) => slot % 3));
    joins.push(update);
  }
  await seatClients[2].close();
  const disconnected = lobby(await seatClients[0].next());
  assert.equal(disconnected.players[2].connected, false);
  seatClients[0].send({ t: 'kick', slot: 2 });
  const afterKick = lobby(await seatClients[0].next());
  assert.equal(afterKick.players.length, 5);
  const replacement = await join(base, 'SeatDefaults', 'replacement');
  const afterReplacement = lobby(await seatClients[0].next());
  assert.deepEqual(afterReplacement.players.at(-1), { name: 'replacement', connected: true, ai: false, team: 2, faction: 2, level: null });
  const seats = { joins, disconnected, afterKick, replacement: replacement.initial, afterReplacement };

  const fixture = {
    reference: pin, rows: ['NET-02', 'NET-03', 'NET-05', 'NET-10', 'NET-22'],
    scope: 'Real TCP, existing lobby commanders only. Exact selected transport outcomes and roster fields; no match, spectator, private seed or whole-match claim.',
    queries, payloads, malformed, maps, seats,
  };
  const bytes = JSON.stringify(fixture, null, 2) + '\n';
  if (process.argv.includes('--check')) assert.equal(await readFile(output, 'utf8'), bytes, 'Source session fixture changed');
  else await writeFile(output, bytes);
  console.log(`PASS: Source session admission batch: ${queries.length} queries, ${payloads.length} payload boundaries, ${invalid.length * 2} malformed messages, ${maps.length} map names, ${joins.length} initial seat rosters plus hole reuse`);
} finally {
  for (const client of sockets) client.socket.terminate();
  for (const child of children) if (child.exitCode === null && child.signalCode === null) {
    const exited = new Promise(resolve => child.once('exit', resolve));
    child.kill('SIGTERM');
    await exited;
  }
  await rm(scratch, { recursive: true, force: true });
}
