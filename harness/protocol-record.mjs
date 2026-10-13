// Record the pinned server's real memory-socket messages without opening a port.
// WW2_REFERENCE can point at an initialized read-only reference checkout.
import assert from 'node:assert/strict';
import http from 'node:http';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { gzipSync } from 'node:zlib';

assert.equal(process.arch, 'x64', 'record with x64 Node: reference math differs on arm64 (DESIGN.md)');
const args = process.argv.slice(2);
const option = (name, fallback) => args.includes(name) ? args[args.indexOf(name) + 1] : fallback;
const scenario = option('--scenario', 'conquest');
const ticks = Number(option('--ticks', '200'));
const seed = Number(option('--seed', '0x3c1055'));
const reference = resolve(process.env.WW2_REFERENCE ?? new URL('../reference/ww2-rts', import.meta.url).pathname);
assert.equal(execFileSync('git', ['-C', reference, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(), 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d', 'reference must match the pinned commit');
const output = resolve(option('--out', new URL('./out/protocol', import.meta.url).pathname));
const mapName = option('--map', 'six-fronts');
assert(['conquest', 'world'].includes(scenario), '--scenario must be conquest or world');
assert(Number.isSafeInteger(ticks) && ticks >= 2 && ticks < 2400, '--ticks must be 2..2399 (before automatic saves)');
assert(Number.isSafeInteger(seed) && seed >= 0 && seed <= 0xffffffff, '--seed must be a uint32');
const source = name => import(pathToFileURL(join(reference, name)));
const original = { random: Math.random, now: Date.now, hrtime: process.hrtime.bigint, listen: http.Server.prototype.listen, interval: globalThis.setInterval };
const envKeys = ['PUBLIC_URL', 'EDIT_PASSWORD', 'SAVES_DIR', 'WW2_SHARDS', 'WW2_DIAG', 'WW2_TICKLOG'];
const oldEnv = Object.fromEntries(envKeys.map(k => [k, process.env[k]]));
let state = seed >>> 0, time = 1700000000000, nanos = 0n, serverModule;
const timers = new Map();
const sockets = [], actions = [], transcripts = [[], []];
Math.random = () => { let t = state += 0x6d2b79f5; t = Math.imul(t ^ t >>> 15, t | 1); t ^= t + Math.imul(t ^ t >>> 7, t | 61); return ((t ^ t >>> 14) >>> 0) / 4294967296; };
Date.now = () => time;
// Wall-time profiling otherwise changes the AI's snapshot beat on busy machines.
process.hrtime.bigint = () => nanos++;
process.env.PUBLIC_URL = 'https://protocol.invalid';
process.env.EDIT_PASSWORD = 'public-fixture-not-a-password';
process.env.SAVES_DIR = join(output, 'unused-saves');
process.env.WW2_SHARDS = '1';
process.env.WW2_DIAG = '0';
process.env.WW2_TICKLOG = '0';
// Import server.js intact, with only the listening and automatic-tick seams disabled.
http.Server.prototype.listen = function () { return this; };
globalThis.setInterval = () => null;
try {
  serverModule = await source('server.js');
  http.Server.prototype.listen = original.listen;
  globalThis.setInterval = original.interval;
  const { rooms, wss, clock, mapFiles, tickRooms } = serverModule;
  const { memoryConnect } = await source('test-socket.js');
  const files = readdirSync(join(reference, 'maps')).filter(f => f.endsWith('.json')).sort();
  const maps = new Map(files.map(f => [f.slice(0, -5), readFileSync(join(reference, 'maps', f), 'utf8')]));
  // Keep real map contents, but make lobby reads settle only on microtasks.
  mapFiles.list = async () => files;
  mapFiles.read = async name => { if (!maps.has(name)) throw Error(`unknown map ${name}`); return maps.get(name); };
  clock.now = () => time;
  clock.setTimeout = (fn, ms) => { const id = Symbol(); timers.set(id, { at: time + ms, fn }); return id; };
  clock.clearTimeout = id => timers.delete(id);
  const settle = async () => { for (let n = 0; n < 32; n++) await Promise.resolve(); };
  function connect(slot) {
    const ws = memoryConnect(wss, '/ws?room=protocol');
    ws.on('message', bytes => transcripts[slot].push(JSON.parse(bytes.toString())));
    sockets.push(ws); return ws;
  }
  async function send(slot, message) {
    actions.push({ tick: rooms.get('protocol')?.game?.tick ?? 0, recipient: slot, message });
    sockets[slot].send(JSON.stringify(message)); await settle();
  }
  for (let i = 0; i < 2; i++) { connect(i); await send(i, { t: 'hello', token: `fixture-player-${i}`, name: `Player ${i + 1}` }); }
  if (scenario === 'conquest') await send(0, { t: 'map', name: mapName });
  else await send(0, { t: 'mode', v: 'world' });
  await send(0, { t: 'addAi' }); await send(0, { t: 'addAi' });
  await send(0, { t: 'weather', v: 'clear' });
  const room = rooms.get('protocol');
  room.worldSeed = seed;
  await send(0, { t: 'start' });
  assert(room.game && !room.starting, 'match did not start');
  const initialUnits = slot => [...room.game.units.values()].filter(u => u.owner === slot && !u.building && !['hq', 'barracks', 'motorpool'].includes(u.type));
  // Orders go through the server. The only state overrides above are deterministic clocks/seeds.
  for (let slot = 0; slot < 2; slot++) {
    const ids = initialUnits(slot).filter(u => ['rifle', 'engineer', 'scout', 'mg'].includes(u.type)).map(u => u.id);
    const target = scenario === 'world' ? room.game.world.regions.find(r => r.team !== room.game.players[slot].team) : { x: room.game.w, z: room.game.h };
    if (ids.length && target) await send(slot, { t: 'move', orders: ids.map(id => [id, target.x, target.z]) });
    await send(slot, { t: 'buy', unit: 'rifle' });
  }
  await send(0, { t: 'not-a-command' });
  for (let i = 1; i <= ticks; i++) {
    time += 50;
    for (const [id, timer] of timers) if (timer.at <= time) { timers.delete(id); timer.fn(); }
    if (i === Math.max(2, Math.floor(ticks / 2))) await send(0, { t: 'resync' });
    tickRooms();
    await settle();
  }
  mkdirSync(output, { recursive: true });
  for (let slot = 0; slot < 2; slot++) {
    const text = transcripts[slot].map(m => JSON.stringify(m)).join('\n') + '\n';
    const stem = join(output, `protocol-${scenario}-p${slot}.jsonl`);
    writeFileSync(stem, text);
    writeFileSync(stem + '.gz', gzipSync(text, { level: 9, mtime: 0 }));
    const snapshots = transcripts[slot].filter(m => m.t === 's');
    console.log(JSON.stringify({ path: stem, bytes: Buffer.byteLength(text), gzipBytes: gzipSync(text, { level: 9, mtime: 0 }).length,
      messages: transcripts[slot].length, snapshots: snapshots.length, full: snapshots.filter(m => m.all).length,
      gone: snapshots.reduce((n, m) => n + (m.gone?.length ?? 0), 0), fogChanges: snapshots.filter(m => m.fog).length,
      terrainRows: snapshots.reduce((n, m) => n + (m.cells?.length ?? 0), 0) }));
  }
  writeFileSync(join(output, `protocol-${scenario}-actions.jsonl`), actions.map(a => JSON.stringify(a)).join('\n') + '\n');
} finally {
  for (const socket of sockets) socket.removeAllListeners();
  if (serverModule) { clearInterval(serverModule.loop); serverModule.rooms.clear(); serverModule.wss.close(); serverModule.server.close(); }
  Math.random = original.random; Date.now = original.now; process.hrtime.bigint = original.hrtime;
  http.Server.prototype.listen = original.listen; globalThis.setInterval = original.interval;
  for (const [key, value] of Object.entries(oldEnv)) { if (value === undefined) delete process.env[key]; else process.env[key] = value; }
}
