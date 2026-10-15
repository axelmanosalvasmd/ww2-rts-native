// Record an unchanged Source match. Native RNG/state is never imported here.
import {readFileSync, writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';

if (process.arch !== 'x64' || process.version !== 'v24.21.0') throw Error('Use x64 Node v24.21.0, matching the maintained recorders');
const args = process.argv.slice(2);
const option = name => {
  const i = args.indexOf(name);
  if (i < 0 || !args[i + 1]) throw Error(`Required ${name}`);
  return args[i + 1];
};
const reference = resolve(option('--reference'));
const output = resolve(option('--output'));
const seedText = option('--seed');
if (!/^(0|[1-9][0-9]*)$/.test(seedText) || BigInt(seedText) > (1n << 64n) - 1n) throw Error('Seed must be a decimal u64');
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = (...a) => execFileSync('git', ['-C', reference, ...a], {encoding: 'utf8'}).trim();
const clean = () => {
  if (git('rev-parse', 'HEAD') !== pin || git('status', '--porcelain', '--untracked-files=no')) throw Error('A clean pinned Source checkout is required');
};
clean();
const sim = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const mapBytes = readFileSync(resolve(reference, 'maps/default.json'));
const map = JSON.parse(mapBytes);
const buffer = new DataView(new ArrayBuffer(8));
const number = value => {
  buffer.setFloat64(0, value);
  return {bits: buffer.getBigUint64(0).toString(16).padStart(16, '0'), value: Number.isFinite(value) ? value : String(value)};
};
const resource = value => value === undefined ? {absent: true} : number(value);
// One global SplitMix64 stream supplies Source Math.random from before createGame.
// Rust instead retains its own unmodified purpose streams. Equal seeds do not align them.
const mask = (1n << 64n) - 1n;
let state = BigInt(seedText), draws = 0;
const random = () => {
  state = (state + 0x9e3779b97f4a7c15n) & mask;
  let z = state;
  z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & mask;
  z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & mask;
  draws++;
  return Number((z ^ (z >> 31n)) >> 11n) / 9007199254740992;
};
const orders = [];
for (const [issuer, ids] of [[0, [1, 2, 3]], [1, [4, 5, 6]]]) {
  orders.push({tick: 1, issuer, sequence: 1, command: {t: 'stance', key: 'holdPos', on: true, ids}});
  orders.push({tick: 2, issuer, sequence: 2, command: {t: 'move', orders: [[ids[0], 81, 81]]}});
  orders.push({tick: 120, issuer, sequence: 3, command: {t: 'stop', ids: [ids[0]]}});
  orders.push({tick: 140, issuer, sequence: 4, command: {t: 'amove', orders: [[ids[0], 81, 81]]}});
  orders.push({tick: 400, issuer, sequence: 5, command: {t: 'stop', ids: [ids[0]]}});
}
orders.sort((a, b) => a.tick - b.tick || a.issuer - b.issuer || a.sequence - b.sequence);
const snapshot = g => ({
  tick: g.tick,
  units: [...g.units.values()].map(u => ({id: u.id, owner: u.owner, kind: u.type, x: number(u.x), z: number(u.z), hp: number(u.hp)})),
  players: g.players.map(p => ({slot: p.slot, team: p.team, mp: number(p.mp), mun: resource(p.mun), fuel: resource(p.fuel), vp: number(p.vp), inc: number(p.inc)})),
  mode: {requested: 'conquest', runtime: g.mode ?? null},
  score: {winner: g.winner, vp: g.players.map(p => number(p.vp))},
  points: g.points.map(p => ({owner: p.owner, capper: p.capper, progress: number(p.progress), cut: p.cut})),
});
const originalRandom = Math.random;
const frames = [], denials = [];
let completed = 0, failure = null, movedTicks = 0, previous;
try {
  Math.random = random;
  const g = sim.createGame(map, ['A', 'B'], true, [0, 1], [0, 1], {
    mode: 'conquest', army: 'standard', sandbox: false, logistics: false,
    tech: true, supply: true, weather: 'clear', mapKey: 'default',
  });
  if (![1, 2, 3].every(id => g.units.get(id)?.owner === 0) || ![4, 5, 6].every(id => g.units.get(id)?.owner === 1)) throw Error('Canonical initial IDs no longer bind the recorded order stream');
  frames.push(snapshot(g));
  for (let tick = 1; tick <= 600; tick++) {
    previous = [...g.units.values()].map(u => [u.id, u.x, u.z]);
    for (const row of orders.filter(row => row.tick === tick)) {
      const reason = sim.command(g, row.issuer, row.command);
      if (reason) denials.push({tick, issuer: row.issuer, sequence: row.sequence, reason});
    }
    sim.step(g);
    completed++;
    if (previous.some(([id, x, z]) => { const u = g.units.get(id); return u && (u.x !== x || u.z !== z); })) movedTicks++;
    frames.push(snapshot(g));
  }
} catch (error) {
  failure = {nextTick: completed + 1, message: String(error)};
} finally {
  Math.random = originalRandom;
}
clean();
const trace = {
  format: 'r1-whole-match-differential-v1', reference: pin, seed: seedText,
  rng: 'Source global SplitMix64 from explicit u64; native purpose streams remain independent',
  node: process.version, requestedTicks: 600, checkpointEvery: 10,
  map, mapSha256: createHash('sha256').update(mapBytes).digest('hex'),
  sourceSha256: createHash('sha256').update(readFileSync(resolve(reference, 'shared/sim.js'))).digest('hex'),
  profile: {mode: 'conquest', army: 'standard', weather: 'clear', shuffle: true, tech: true, supply: true, livingWeather: true, logistics: false, sandbox: false, weatherSeed: null},
  orders, frames, completed, movedTicks, draws, denials, failure,
};
// The complete trace contains the private seed. Only the sanitized summary goes to stdout.
writeFileSync(output, JSON.stringify(trace) + '\n', {mode: 0o600});
console.log(JSON.stringify({engine: 'Source', completed, movedTicks, frames: frames.length, checkpoints: frames.filter(f => f.tick % 10 === 0).length, failure}));
if (failure) process.exitCode = 2;
