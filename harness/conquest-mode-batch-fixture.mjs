// Record isolated Conquest components. This is not a whole-match outcome comparison.
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { registerHooks } from 'node:module';
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
registerHooks({ load(url, context, next) {
  const result = next(url, context);
  if (url !== pathToFileURL(`${reference}/shared/sim.js`).href) return result;
  const source = String(result.source);
  const capture = '  for (const p of g.points) {\n    const on = list.filter';
  const score = '  // victory points count per team:';
  const end = '  stepConvoys(g, dt, supplyHooks(g));';
  const victory = "  for (const pl of g.players) if (teamVp(pl.team) >= g.winVp && g.winner === null) finish(g, pl.team, 'vp', lastCapture(g, pl.team)); // winner = team id";
  const periodic = '  if (g.winner === null && (g.tick === 1 || g.tick % SAMPLE_EVERY === 0)) sample(g, isStructure(g));';
  for (const anchor of [capture, score, end, victory, periodic]) if (source.split(anchor).length !== 2) throw Error(`Source anchor changed: ${anchor}`);
  return { ...result, source: `${source}\nexport function __capture(g,list,dt){${source.slice(source.indexOf(capture), source.indexOf(score))}\n}\nexport function __points(g,list,dt){${source.slice(source.indexOf(capture), source.indexOf(end))}\n}\nexport function __victory(g){const teamVp=t=>g.players.reduce((a,q)=>a+(q.team===t?q.vp:0),0);${victory}\n}\nexport function __periodic(g){${periodic}\n}\n` };
} });
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const story = await import(pathToFileURL(`${reference}/shared/story.js`));
const bits = n => { const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex'); };
const encode = v => typeof v === 'number' ? bits(v) : Array.isArray(v) ? v.map(encode) : v && typeof v === 'object' ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, encode(x)])) : v;
const point = (extra = {}) => ({ id: 'p', x: 40, z: 40, vp: 1, mp: 1, needs: -1, kind: null, owner: -1, capper: -1, progress: 0, cut: false, ...extra });
const players = [{ team: 0, out: false }, { team: 1, out: false }, { team: 0, out: false }];
const unit = (extra = {}) => ({ id: 1, owner: 0, type: 'rifle', x: 40, z: 40, hp: 100, riding: 0, retreating: false, ...extra });
const captures = [];
function capture(name, points = [point()], units = [unit()], steps = 1, roster = players) {
  const g = { points: structuredClone(points), players: structuredClone(roster), tick: 0, winner: null };
  const expected = [];
  for (let i = 0; i < steps; i++) { g.tick++; sim.__capture(g, units, sim.TICK); expected.push(encode({ points: g.points.map(p => ({ owner: p.owner, capper: p.capper, progress: p.progress, onPoint: p.onPoint, contested: p.contested })), captures: g.players.map((_, i) => g.story?.[i]?.captures ?? 0) })); }
  captures.push({ name, points, units, players: roster, steps, expected });
}
capture('eight-second-neutral-capture', undefined, undefined, 162);
capture('enemy-drains-to-neutral-before-capture', [point({ owner: 1, capper: 1, progress: 1 })], undefined, 324);
capture('allied-owner-holds-full', [point({ owner: 2, progress: 0.2 })]);
capture('enemy-contests-regardless-of-order', undefined, [unit({ id: 2, owner: 1 }), unit()]);
capture('first-allied-qualifying-unit-owns', [point({ capper: 2, progress: 0.999 })], [unit({ id: 2, owner: 2 }), unit()]);
capture('prerequisite-lock', [point({ id: 'first' }), point({ id: 'locked', needs: 0 })]);
capture('prerequisite-allied-unlock', [point({ id: 'first', owner: 2, progress: 1 }), point({ id: 'next', needs: 0, capper: 0, progress: 0.999 })]);
capture('radius-eight-inclusive', undefined, [unit({ x: 48 })]);
capture('outside-radius-excluded', undefined, [unit({ x: 48.00000001 })]);
for (const [name, extra] of Object.entries({ dead: { hp: 0 }, retreating: { retreating: true }, riding: { riding: 9 }, forced: { logistics: { forced: true } }, vehicle: { type: 'tank' } })) capture(`exclude-${name}`, undefined, [unit(extra)]);
capture('out-player-excluded', undefined, undefined, 1, players.map((p, i) => ({ ...p, out: i === 0 })));
capture('empty-does-not-reset', [point({ capper: 1, progress: 0.4 })], []);
const map = JSON.parse(readFileSync(`${reference}/maps/default.json`, 'utf8'));
const seed = 3936341;
function make() {
  const old = Math.random; Math.random = () => 0.5;
  try { return sim.createGame(map, ['Host', 'Guest'], true, [0, 1], [0, 1], { mode: 'conquest', army: 'standard', weather: 'clear', tech: true, supply: true, logistics: false }); }
  finally { Math.random = old; }
}
const state = g => encode({ tick: g.tick, goal: g.winVp, winner: g.winner, reason: g.endReason ?? null, at: g.endAt ?? null, endTick: g.endTick ?? null, reveal: !!g.reveal, players: g.players.map(p => ({ mp: p.mp, vp: p.vp, inc: p.inc })), points: g.points.map(p => ({ owner: p.owner, capper: p.capper, progress: p.progress, cut: !!p.cut })), story: g.players.map((_, i) => Object.fromEntries(story.STORY_KEYS.map(k => [k, g.story?.[i]?.[k] ?? 0]))), timeline: g.timeline ?? [] });
const phases = [];
function phase(name, actions) {
  const g = make(); const steps = [];
  // Retain canonical structures and every actor, but isolate the point/economy component from nearby infantry.
  for (const u of g.units.values()) if (!sim.UNITS[u.type].structure) { u.x = 2; u.z = 2; }
  for (const action of actions) {
    g.tick = action.tick;
    for (const [slot, key, value] of action.players ?? []) g.players[slot][key] = value;
    for (const [index, values] of action.points ?? []) Object.assign(g.points[index], values);
    if (action.periodic) sim.__periodic(g);
    if (action.pointsPhase) sim.__points(g, [...g.units.values()], sim.TICK);
    sim.__victory(g);
    steps.push({ action, expected: state(g) });
  }
  phases.push({ name, steps });
}
phase('held-uncut-cut-neutral-and-away', [
  { tick: 1, periodic: true, points: [[0, { owner: 0, capper: 0, progress: 1 }], [1, { owner: 0, capper: 0, progress: 1, cut: true }]], pointsPhase: true },
  { tick: 2, players: [[0, 'away', true]], pointsPhase: true },
  { tick: 3, players: [[0, 'away', false]], pointsPhase: true },
]);
phase('threshold-and-frozen-score', [
  { tick: 199, players: [[0, 'vp', 1199.999]] },
  { tick: 200, periodic: true, players: [[0, 'vp', 1200]], points: [[0, { owner: 0, capper: 0, progress: 1, capTick: 199 }]] },
  { tick: 400, periodic: true, pointsPhase: true, players: [[1, 'vp', 9999]] },
]);
phase('simultaneous-first-slot-and-latest-still-held', [{ tick: 200, players: [[0, 'vp', 1200], [1, 'vp', 1200]], points: [[0, { owner: 0, capper: 0, progress: 1, capTick: 30 }], [1, { owner: 1, capper: 1, progress: 1, capTick: 100 }], [2, { owner: 0, capper: 0, progress: 1, capTick: 40 }]] }]);
phase('equal-capture-time-stable-first', [{ tick: 40, players: [[1, 'vp', 1200]], points: [[0, { owner: 1, capTick: 10 }], [1, { owner: 1, capTick: 10 }]] }]);
phase('no-held-point-falls-back-to-centre', [{ tick: 5, players: [[0, 'vp', 1200]] }]);
const terminal = make(); terminal.players[0].vp = terminal.winVp; sim.__victory(terminal);
const terminalBefore = state(terminal); const terminalSteps = [];
const victim = [...terminal.units.values()].find(u => u.owner === 0 && u.type === 'rifle');
const deadActor = { id: victim.id, owner: victim.owner, kind: victim.type };
victim.hp = 0;
const commands = [{ t: 'stop', ids: [1] }, { t: 'buy', unit: 'rifle' }, { t: 'move', orders: [[1, 25, 25]] }, { t: 'holdFire', ids: [1], on: true }];
for (const [i, command] of commands.entries()) {
  const denial = sim.command(terminal, 0, command);
  story.tally(terminal, 0, 'built', 1);
  while (terminal.tick < [1, 2, 200, 220][i]) sim.step(terminal);
  terminalSteps.push({ command, denial, tick: terminal.tick, winner: terminal.winner, endTick: terminal.endTick, retired: !terminal.units.has(victim.id), actorCount: terminal.units.size, story: state(terminal).story, timeline: state(terminal).timeline });
}
const scoring = make();
for (const u of scoring.units.values()) { u.holdPos = true; u.holdFire = true; }
const scorer = [...scoring.units.values()].find(u => u.owner === 0 && u.type === 'rifle');
scorer.x = scoring.points[0].x; scorer.z = scoring.points[0].z;
const scoreActor = { id: scorer.id, owner: scorer.owner, kind: scorer.type };
const scoreSteps = [];
for (const [tick, away] of [[180, false], [380, true], [420, false]]) {
  scoring.players[0].away = away;
  while (scoring.tick < tick) sim.step(scoring);
  scoreSteps.push({ tick, away, expected: state(scoring) });
}
clean();
const goals = [[0, 1], [0, 0, 1, 1], [0, 0, 0, 1, 1, 1], [2, 5, 2], [3], [0, 1, 2, 3, 4, 5]].map(teams => ({ teams, goal: bits(sim.winVp(teams)) }));
const fixture = { format: 1, reference: pin, node: process.version, arch: process.arch, kind: 'isolated-components-not-whole-match', seed, map, captures, goals, phases, scoring: { actor: scoreActor, steps: scoreSteps }, terminal: { before: terminalBefore, deadActor, steps: terminalSteps } };
const path = `${root}/rust/crates/simtest/tests/fixtures/conquest-mode-batch.json`;
const output = `${JSON.stringify(fixture)}\n`;
if (process.argv.includes('--check')) { if (readFileSync(path, 'utf8') !== output) throw Error('Conquest MODE batch fixture differs'); } else writeFileSync(path, output);
console.log(`Conquest MODE batch: ${captures.length} capture cases, ${captures.reduce((n, c) => n + c.steps, 0)} capture checkpoints, ${goals.length} goals, ${phases.length} phase traces, ${terminalSteps.length} terminal checkpoints / 220 actual ticks, ${scoreSteps.length} scoring checkpoints / 420 actual ticks`);
