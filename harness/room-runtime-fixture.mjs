// Actual pinned room callbacks with real simulation, controlled clocks and socket writes.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {readFileSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {encode} from './state-codec.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], {encoding: 'utf8'}).trim();
const clean = () => { assert.equal(git(['rev-parse', 'HEAD']), pin); assert.equal(git(['status', '--porcelain', '--untracked-files=no']), ''); };
assert.equal(process.arch, 'x64'); assert.equal(process.version, 'v24.21.0');
assert.notEqual(process.env.WW2_TICKLOG, '1');
assert.ok(process.argv.slice(2).every(a => a === '--check')); clean();
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const {aiTick} = await import(pathToFileURL(`${reference}/shared/ai-schedule.js`));
const {storyResult} = await import(pathToFileURL(`${reference}/shared/story.js`));
const {createTickMeter, recordTick, tickStats} = await import(pathToFileURL(`${reference}/tickmeter.js`));
const {createRoomTick} = await import(pathToFileURL(`${reference}/server/room-tick.js`));
const source = readFileSync(`${reference}/server.js`, 'utf8');
const slices = {};
function between(name, start, end) {
  assert.equal(source.split(start).length, 2, `unique start ${name}`);
  assert.equal(source.split(end).length, 2, `unique end ${name}`);
  const a = source.indexOf(start), b = source.indexOf(end, a); assert.ok(b > a);
  return slices[name] = source.slice(a, b);
}
const helpers = between('connection', 'const send = (ws, msg)', '// Spectators have no seat:');
const everyone = between('everyone', 'const everyone = (room)', 'const isHost = (room');
const trimmed = between('trimmed', 'const LISTS = ', 'export function watcherSnapshot');
const pauses = between('pause', 'function pauseMessage(room)', 'function handToAi(room, player)');
const finish = between('finish', 'function finishMatch(room, result)', 'function scheduleSeatCleanup');
const ending = between('ending', 'const HOLD_TICKS = ', '// One running tick:');
const backpressure = between('backpressure', 'function backedUp(p)', 'export function tickRooms()');
const roomsAndPoll = between('roomsAndPoll', 'export function tickRooms()', '// The forwarding process:');
const originalRandom = Math.random, originalNow = Date.now, originalNs = process.hrtime.bigint;
let nowMs = 0, monotonicNs = 0n, sampleNs = 0n;
Date.now = () => nowMs;
process.hrtime.bigint = () => { sampleNs += 10_000n; return sampleNs; };
Math.random = () => .5;
const map = {name: 'Internal ordinary room', w: 100, h: 60, rows: Array(60).fill('.'.repeat(100)), spawns: [{x: 2, y: 2}, {x: 97, y: 57}], points: [{x: 50, y: 30}]};
const options = {weather: 'clear', tech: false, logistics: false};
const traces = [];
function run(name, actions, {winner = null, starting = false, scoreAtGoal = false} = {}) {
  nowMs = 0; monotonicNs = 0n; sampleNs = 0n;
  const g = sim.createGame(map, ['A', 'B'], false, [0, 1], [0, 1], options);
  // Explicit restored quiet actors, retaining the real constructor's terrain and player state.
  g.units.clear(); g.nextId = 1;
  for (const [owner, x, z] of [[0, 41, 41], [1, 181, 101]]) Object.assign(sim.internals.spawnUnit(g, owner, 'rifle'), {x, z});
  if (scoreAtGoal) g.players[0].vp = g.winVp;
  if (winner !== null) { g.winner = winner; g.reveal = true; g.endReason = 'vp'; g.endAt = {x: 100, z: 60}; }
  const initial = encode(g, {runs: true});
  let events = [], writes = [], steps = 0, caches = 0, samples = [], activeAction;
  const pending = new Map(); let nextWrite = 1;
  const socket = label => ({label, readyState: 1, send(json, callback) {
    const message = JSON.parse(json), id = nextWrite++;
    writes.push({id, socket: label, message}); events.push({event: 'write', socket: label, type: message.t});
    if (callback) pending.set(id, {callback, socket: this});
  }});
  const originals = [socket('A:1'), socket('B:1')];
  const room = {code: name, state: 'play', game: g, starting, mode: 'conquest', players: originals.map((ws, i) => ({ws, name: ['A', 'B'][i], team: i, ai: false, rtt: 10 + i})), spectators: [], operatives: [], emptySince: null};
  const rooms = new Map([[name, room]]);
  const step = game => { steps++; events.push({event: 'step', before: game.tick}); sim.step(game); };
  const cache = game => { caches++; events.push({event: 'cache', tick: game.tick}); return sim.snapshotCache(game); };
  const noWatchers = r => { assert.equal(r.spectators.length, 0); };
  const noOperatives = r => { assert.equal(r.operatives.length, 0); };
  const record = (meter, fields, now) => { samples.push(fields); return recordTick(meter, fields, now); };
  let poll;
  const deps = {TICK: sim.TICK, step, snapshotCache: cache, snapshotFor: sim.snapshotFor, unitDelta: sim.unitDelta, clock: {now: () => nowMs}, rooms,
    sendWatchers: noWatchers, sendOperatives: noOperatives, storyResult, hordeResult: () => assert.fail('Horde is outside this fixture'),
    freeOfflineSeats: r => { events.push({event: 'cleanup-hook', offline: r.players.filter(p => !p.ws).length}); },
    lobby: r => { events.push({event: 'lobby-hook', state: r.state}); },
    saveRoom: () => assert.fail('World autosave is outside this fixture'), AUTOSAVE: 1, diag: null, PRIMARY: false,
    process: {hrtime: {bigint: () => monotonicNs}}, setInterval: (callback, ms) => { assert.equal(ms, 10); poll = callback; return 1; },
    createRoomTick, aiTick: (...args) => { assert.deepEqual(args[1], []); return aiTick(...args); }, createTickMeter, recordTick: record, tickStats};
  const timed = 'const timedRoomTick = createRoomTick({step,aiTick,snapshotCache,snapshotFor,trimmed,sendSnapshot,sendWatchers,sendOperatives,connected,backedUp,createTickMeter,recordTick,tickStats});';
  // Only export keywords are removed. Callback bodies and their ordering remain unchanged.
  const body = [helpers, everyone, trimmed, pauses, finish, ending, backpressure, timed, roomsAndPoll.replace('export function tickRooms()', 'function tickRooms()').replace('export const loop = ', 'const loop = ')].join('\n');
  const api = new Function(...Object.keys(deps), `${body}\nreturn {tickRooms,pauseRoom,autoPause,resumeRoom,backedUp,sendSnapshot,due:()=>dueAt};`)(...Object.values(deps));
  const observations = [];
  for (activeAction of actions) {
    events = []; writes = []; samples = []; steps = 0; caches = 0;
    if (activeAction.now !== undefined) nowMs = activeAction.now;
    const before = {tick: g.tick, shots: g.shots.length, cells: g.newCells.length, due: Number(api.due())};
    const input = activeAction;
    if (input.op === 'room') api.tickRooms();
    else if (input.op === 'poll') { monotonicNs = BigInt(input.ns); poll(); }
    else if (input.op === 'pause') api.pauseRoom(room, room.players[input.slot], input.reason);
    else if (input.op === 'drop') { room.players[input.slot].ws = null; api.autoPause(room, room.players[input.slot]); }
    else if (input.op === 'reconnect') room.players[input.slot].ws = input.replace ? socket(`${input.slot}:replacement`) : originals[input.slot];
    else if (input.op === 'resume') api.resumeRoom(room);
    else if (input.op === 'starting') room.starting = input.value;
    else if (input.op === 'ready') room.players[input.slot].ws.readyState = input.value;
    else if (input.op === 'queue') api.sendSnapshot(room, room.players[input.slot], JSON.stringify({t: 'padding', text: 'x'.repeat(input.units)}));
    else if (input.op === 'complete') { const write = pending.get(input.id); assert.ok(write); pending.delete(input.id); write.callback(); }
    else if (input.op === 'transient') { g.shots.push({f: 0, t: 0, x: 100, z: 60, pub: true}); g.newCells.push([2020, 'T']); }
    else assert.fail(`unknown action ${input.op}`);
    observations.push({input, before, expected: {state: room.state, tick: g.tick, winner: g.winner, held: g.held ?? 0, paused: room.pause ? {reason: room.pause.reason, by: room.pause.by, until: room.pause.until} : null,
      away: g.players.map(p => !!p.away), steps, caches, shots: g.shots.length, cells: g.newCells.length, due: Number(api.due()),
      ...(input.op === 'poll' ? {dropped: Number(api.due()) !== before.due + steps * Math.round(sim.TICK * 1e9)} : {}),
      sockets: originals.map(ws => ({label: ws.label, inflight: ws.inflight ?? 0, owed: !!ws.owed})),
      events, writes: writes.map(w => w.message.t === 'padding' ? {...w, message: {t: 'padding', utf16Length: JSON.stringify(w.message).length}} : w), samples,
      units: [...g.units.values()].map(u => ({id: u.id, owner: u.owner, x: u.x, z: u.z, hp: u.hp, cooldown: u.cooldown, retarget: u.retarget})),
      ...(room.result ? {result: room.result, lastMatch: room.players.map(p => p.lastMatch)} : {})}});
  }
  traces.push({name, scope: 'Actual tickRooms/room-tick/sim.step/snapshot producers and socket callbacks. Final lobby/cleanup and empty watcher/operative consumers are explicit hooks; no HTTP server or public start.',
    scene: {names: ['A', 'B'], teams: [0, 1], factions: [0, 1], shuffle: false, options,
      constructorRandom: .5, replaceStartupActors: [{owner: 0, type: 'rifle', x: 41, z: 41}, {owner: 1, type: 'rifle', x: 181, z: 101}], restoredWinner: winner, scoreAtGoal, starting},
    initial, observations});
}
try {
  run('pause-reconnect-and-host-resume', [{op: 'room'}, {op: 'drop', slot: 1, now: 50}, {op: 'room', now: 100}, {op: 'room', now: 1050}, {op: 'reconnect', slot: 1}, {op: 'room', now: 1100}, {op: 'pause', slot: 0, reason: 'host'}, {op: 'room', now: 2200}, {op: 'resume'}, {op: 'room', now: 2250}]);
  run('drop-deadline-resumes-same-callback', [{op: 'drop', slot: 1}, {op: 'room', now: 29999}, {op: 'room', now: 30000}]);
  run('starting-gate', [{op: 'room'}, {op: 'starting', value: false}, {op: 'room'}], {starting: true});
  run('backpressure-current-socket-catchup', [{op: 'room'}, {op: 'queue', slot: 1, units: 262144}, {op: 'room'}, {op: 'complete', id: 1}]);
  run('backpressure-stale-socket', [{op: 'room'}, {op: 'queue', slot: 1, units: 262144}, {op: 'room'}, {op: 'reconnect', slot: 1, replace: true}, {op: 'complete', id: 1}]);
  run('no-eligible-detaches-transients-once', [{op: 'ready', slot: 0, value: 3}, {op: 'ready', slot: 1, value: 3}, {op: 'transient'}, {op: 'room'}, {op: 'room'}, {op: 'room'}, {op: 'room'}]);
  run('catchup-four-backlog-drop', [{op: 'poll', ns: 0}, {op: 'poll', ns: 1000000000}, {op: 'poll', ns: 1000000001}, {op: 'poll', ns: 1050000000}]);
  run('winning-step-forces-snapshot-before-hold', [{op: 'room'}, {op: 'room'}, {op: 'room'}], {scoreAtGoal: true});
  run('ending-hold-120-callbacks', Array.from({length: 121}, (_, i) => ({op: 'room', now: i * 50})), {winner: 0});
} finally { Math.random = originalRandom; Date.now = originalNow; process.hrtime.bigint = originalNs; }
const trace = name => traces.find(t => t.name === name).observations;
const pause = trace('pause-reconnect-and-host-resume');
assert.equal(pause[2].expected.steps, 0); assert.equal(pause[3].expected.steps, 0);
assert.equal(pause[3].expected.caches, 1); assert.equal(pause[3].expected.samples.length, 0);
assert.equal(pause[5].expected.steps, 1); assert.equal(pause[5].expected.paused, null);
assert.equal(pause[7].expected.steps, 0); assert.equal(pause[9].expected.steps, 1);
const deadline = trace('drop-deadline-resumes-same-callback');
assert.equal(deadline[1].expected.steps, 0); assert.equal(deadline[2].expected.steps, 1);
assert.equal(deadline[2].expected.paused, null); assert.deepEqual(deadline[2].expected.away, [false, true]);
const current = trace('backpressure-current-socket-catchup');
assert.equal(current[2].expected.sockets[1].owed, true);
assert.equal(current[3].expected.caches, 1); assert.equal(current[3].expected.writes.length, 1);
assert.equal(current[3].expected.writes[0].socket, 'B:1');
assert.deepEqual(current[3].expected.writes[0].message.shots, []);
const stale = trace('backpressure-stale-socket');
assert.equal(stale[4].expected.caches, 0); assert.equal(stale[4].expected.writes.length, 0);
assert.equal(stale[4].expected.sockets[1].inflight, 0); assert.equal(stale[4].expected.sockets[1].owed, true);
const empty = trace('no-eligible-detaches-transients-once');
assert.equal(empty[3].expected.shots, 1); assert.equal(empty[4].expected.shots, 0);
assert.equal(empty[4].expected.cells, 0); assert.equal(empty[4].expected.caches, 0);
assert.equal(empty[4].expected.samples[0].sent, true); assert.equal(empty[6].expected.caches, 0);
const polls = trace('catchup-four-backlog-drop');
assert.equal(polls[1].expected.steps, 4); assert.equal(polls[1].expected.dropped, true);
assert.equal(polls[2].expected.steps, 0); assert.equal(polls[3].expected.steps, 1);
const held = trace('ending-hold-120-callbacks');
assert.equal(held.reduce((n, o) => n + o.expected.steps, 0), 60);
assert.equal(held[118].expected.state, 'play'); assert.equal(held[119].expected.state, 'lobby');
assert.equal(held[119].expected.held, 120); assert.equal(held[120].expected.steps, 0);
assert.equal(held.reduce((n, o) => n + o.expected.samples.length, 0), 0);
const winning = trace('winning-step-forces-snapshot-before-hold');
assert.equal(winning[0].expected.tick, 1); assert.equal(winning[0].expected.winner, 0);
assert.equal(winning[0].expected.samples[0].sent, true); assert.equal(winning[0].expected.writes.length, 2);
assert.equal(winning[1].expected.steps, 0); assert.equal(winning[1].expected.held, 1);
assert.equal(winning[2].expected.steps, 1); assert.equal(winning[2].expected.samples.length, 0);
clean();
const output = JSON.stringify({format: 1, reference: pin, node: process.version, arch: process.arch, map,
  clocks: 'Controlled host milliseconds and monotonic polling nanoseconds. Timing reads advance 10 microseconds per read for reproducibility, not a performance measurement.',
  sourceSlices: Object.fromEntries(Object.entries(slices).map(([key, value]) => [key, {sha256: createHash('sha256').update(value).digest('hex'),
    startLine: source.slice(0, source.indexOf(value)).split('\n').length, lines: value.split('\n').length - 1}])), traces}) + '\n';
const target = `${root}/rust/crates/server/tests/fixtures/room-runtime.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), output); else writeFileSync(target, output);
console.log(`${traces.length} actual room traces, ${traces.reduce((n, t) => n + t.observations.length, 0)} actions, ${traces.reduce((n, t) => n + t.observations.reduce((m, o) => m + o.expected.steps, 0), 0)} actual simulation steps`);
