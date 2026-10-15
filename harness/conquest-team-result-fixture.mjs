// Original-Source team, finish and report components on a retained authored scene.
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { isDeepStrictEqual } from 'node:util';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
function clean() {
  if (process.version !== 'v24.21.0' || process.arch !== 'x64') throw Error('Use x64 Node 24.21.0');
  if (execFileSync('git', ['rev-parse', 'HEAD'], { cwd: reference, encoding: 'utf8' }).trim() !== pin || execFileSync('git', ['status', '--porcelain', '--untracked-files=no'], { cwd: reference, encoding: 'utf8' }).trim()) throw Error('Clean pinned reference required');
}
clean();
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const story = await import(pathToFileURL(`${reference}/shared/story.js`));
const map = JSON.parse(readFileSync(`${reference}/maps/pegasus-bridge.json`, 'utf8'));
const committed = JSON.parse(readFileSync(`${root}/rust/crates/server/src/authoritative-maps/pegasus-bridge.json`, 'utf8'));
if (!isDeepStrictEqual(map, committed) || map.spawns.length !== 4 || map.scenario) throw Error('Unchanged committed four-spawn map required');
const seed = 3936341;
const bits = n => { const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex'); };
const encode = v => typeof v === 'number' ? bits(v) : Array.isArray(v) ? v.map(encode) : v && typeof v === 'object' ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, encode(x)])) : v;
const names = ['Host', 'Guest', 'Ally', 'Opponent'];
function make(teams = [0, 1, 0, 1]) {
  const old = Math.random;
  // Constructor randomness is isolated fixture input, not native stream alignment.
  Math.random = () => 0.5;
  let g;
  try { g = sim.createGame(map, names, true, teams, [0, 1, 2, 3], { mode: 'conquest', army: 'standard', weather: 'clear', tech: true, supply: true, logistics: false, sandbox: false }); }
  finally { Math.random = old; }
  if (g.players.length !== 4 || g.winVp !== 2400) throw Error('Canonical 2v2 constructor goal required');
  for (const u of g.units.values()) { u.holdPos = true; u.holdFire = true; }
  return g;
}
const roster = g => ({ goal: g.winVp, teams: g.players.map(p => p.team), names: g.players.map(p => p.name), inventory: [...g.units.values()].map(u => `${u.owner}:${u.type}`).sort() });
const counters = g => g.players.map((_, i) => Object.fromEntries(story.STORY_KEYS.map(k => [k, g.story?.[i]?.[k] ?? 0])));
const report = g => g.winner === null ? null : { winner: g.winner, teams: g.players.map(p => p.team), names: g.players.map(p => p.name), ...story.storyResult(g) };
function state(g) {
  return encode({ tick: g.tick, goal: g.winVp, teams: g.players.map(p => p.team), vp: g.players.map(p => p.vp), winner: g.winner, reason: g.endReason ?? null, at: g.endAt ?? null, endTick: g.endTick ?? null, reveal: !!g.reveal, story: counters(g), timeline: g.timeline ?? [], result: report(g), allRecipientsReveal: g.winner === null ? null : g.players.every((_, slot) => sim.snapshotFor(g, slot, []).units.length === g.units.size) });
}
const anchors = g => [...g.units.values()].map(u => [u.id, u.owner, u.type, u.x, u.z]);
const constructors = [[0, 1, 0, 1], [2, 5, 2, 5]].map(teams => ({ teams, expected: encode(roster(make(teams))) }));
const victories = [];
function victory(name, teams, actions, winners) {
  const g = make(teams), before = anchors(g), steps = [];
  for (const action of actions) {
    const tick = action.tick ?? g.tick + 1;
    while (g.tick < tick - 1) sim.step(g);
    for (const [slot, vp] of action.vp) g.players[slot].vp = vp;
    for (const [index, owner, capTick] of action.points ?? []) Object.assign(g.points[index], { owner, capper: owner, progress: 1, capTick });
    sim.step(g);
    if (g.winner !== winners[steps.length] || g.players.some(p => p.vp >= g.winVp)) throw Error(`${name}: intended team-sum boundary not reached`);
    if (!isDeepStrictEqual(anchors(g), before)) throw Error(`${name}: canonical actors changed`);
    steps.push({ action, expected: state(g) });
  }
  victories.push({ name, teams, steps });
}
victory('allied-contributions-below-and-crossing', [0, 1, 0, 1], [
  { vp: [[0, 1199.25], [1, 1100], [2, 1200.25], [3, 1100]] },
  { vp: [[0, 1199.75], [2, 1200.5]] },
], [null, 0]);
victory('simultaneous-sparse-first-slot', [5, 2, 5, 2], [{ tick: 41, vp: [[0, 1200.25], [1, 1200.75], [2, 1200.25], [3, 1200.75]], points: [[0, 0, 30], [1, 2, 40]] }], [5]);
victory('latest-lost-point-is-excluded', [0, 1, 0, 1], [{ tick: 101, vp: [[0, 1100], [2, 1300]], points: [[0, 0, 10], [1, 1, 100], [2, 2, 20]] }], [0]);
victory('equal-capture-time-retains-point-order', [0, 1, 0, 1], [{ tick: 11, vp: [[0, 1100], [2, 1300]], points: [[0, 0, 10], [1, 2, 10]] }], [0]);

const freshFinishes = [];
for (const input of [
  { name: 'draw-forces-reason-and-centre', winner: -1, reason: 'timer', at: null },
  { name: 'positive-negative-half-rounding', winner: 0, reason: 'hq', at: { x: 17.25, z: -0.05 } },
  { name: 'negative-and-positive-tenths', winner: 1, reason: 'vp', at: { x: -17.25, z: 0.15 } },
  { name: 'sparse-team-default-centre', teams: [2, 5, 2, 5], winner: 5, reason: 'vp', at: null },
]) {
  const g = make(input.teams), before = anchors(g);
  sim.finish(g, input.winner, input.reason, input.at);
  const expected = state(g);
  sim.finish(g, -1, 'draw', { x: 1, z: 1 });
  if (!isDeepStrictEqual(state(g), expected) || !isDeepStrictEqual(anchors(g), before)) throw Error('First finish or canonical anchors changed');
  freshFinishes.push({ input, expected });
}

const timelineGame = make([2, 5, 2, 5]), timelineAnchors = anchors(timelineGame), timeline = [];
timelineGame.players.forEach((p, i) => { p.vp = [1.75, 2.5, 3.5, 4.75][i]; });
for (const tick of [1, 199, 200, 201, 202]) {
  if (tick === 202) { timelineGame.players[0].vp = 1000.25; timelineGame.players[2].vp = 1400.25; }
  while (timelineGame.tick < tick) sim.step(timelineGame);
  if (!isDeepStrictEqual(anchors(timelineGame), timelineAnchors)) throw Error('Timeline trace changed canonical actors');
  if ((timelineGame.timeline?.length ?? 0) !== [1, 1, 2, 2, 3][timeline.length]) throw Error('Actual sample scheduling differs');
  if (timelineGame.timeline[0].vp.length !== 6 || timelineGame.timeline[0].vp[2] !== 5 || timelineGame.timeline[0].vp[5] !== 7) throw Error('Sparse allied team flooring differs');
  timeline.push({ tick, expected: state(timelineGame) });
}

// Callback-only HQ/barracks records retain every live actor. They do not prove destruction.
const callbackGame = make(), callbacks = [], fallenActors = {};
function callback(action) {
  if (action.kind === 'tally') story.tally(callbackGame, action.slot, action.key, action.amount);
  else {
    const u = [...callbackGame.units.values()].find(u => u.owner === action.owner && u.type === action.unit);
    if (!u) throw Error('Canonical callback actor absent');
    story.died(callbackGame, u, sim.UNITS[u.type], action.by);
    fallenActors.structure = u; fallenActors.hq = u;
  }
  callbacks.push({ action, expected: encode({ story: counters(callbackGame), fallenAtCanonicalActors: Object.fromEntries(Object.entries(fallenActors).map(([key, u]) => [key, callbackGame.fallen[key].x === u.x && callbackGame.fallen[key].z === u.z])), actorCount: callbackGame.units.size }) });
}
story.STORY_KEYS.forEach((key, i) => callback({ kind: 'tally', slot: i % 4, key, amount: i + 0.5 }));
callback({ kind: 'tally', slot: -1, key: 'built', amount: 100 });
callback({ kind: 'death-record', owner: 0, unit: 'hq', by: 1 });
callback({ kind: 'death-record', owner: 1, unit: 'barracks', by: 0 });
sim.finish(callbackGame, 0, 'vp');
const callbackResult = state(callbackGame);
for (const key of story.STORY_KEYS) story.tally(callbackGame, 1, key, 100);
if (!isDeepStrictEqual(state(callbackGame), callbackResult)) throw Error('Finished callback changed report');

const deathGame = make(), deaths = [];
for (const action of [{ owner: 0, by: 1 }, { owner: 2, by: 0 }, { owner: 1, by: null }]) {
  const victim = [...deathGame.units.values()].find(u => u.owner === action.owner && u.type === 'rifle');
  victim.hp = 0; victim.lastHit = action.by ?? undefined;
  sim.step(deathGame);
  if (deathGame.units.has(victim.id)) throw Error('Actual rifle retirement required');
  deaths.push({ action, expected: state(deathGame), actorCount: deathGame.units.size });
}
deathGame.players[0].vp = 1100; deathGame.players[2].vp = 1300;
if (deathGame.story[0].losses !== 1 || deathGame.story[0].kills !== 0 || deathGame.story[1].kills !== 1 || deathGame.story[1].losses !== 1 || deathGame.story[2].losses !== 1) throw Error('Enemy/allied/unattributed real death counters differ');
sim.step(deathGame);
const deathResult = state(deathGame);
clean();
const fixture = { format: 1, reference: pin, node: process.version, arch: process.arch, kind: 'isolated-components-not-whole-match', seed, map, constructors, victories, freshFinishes, timeline, callbacks, callbackResult, deaths, deathResult };
const path = `${root}/rust/crates/simtest/tests/fixtures/conquest-team-result.json`;
const output = `${JSON.stringify(fixture)}\n`;
if (process.argv.includes('--check')) { if (readFileSync(path, 'utf8') !== output) throw Error('Conquest team/result fixture differs'); } else writeFileSync(path, output);
console.log(`Conquest team/result: ${constructors.length} canonical constructors, ${victories.length} victory traces / ${victories.reduce((n, v) => n + (v.steps.at(-1).action.tick ?? v.steps.length), 0)} actual ticks, ${freshFinishes.length} fresh finish vectors, ${timeline.length} timeline checkpoints / 202 actual ticks, ${callbacks.length} callback actions, ${deaths.length} actual rifle retirements / 4 actual ticks`);
