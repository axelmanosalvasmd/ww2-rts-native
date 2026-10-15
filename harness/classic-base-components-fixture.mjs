// Original Classic constructor and full Source steps, projected to base components.
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
assert(process.argv.slice(2).every(a => a === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], {encoding: 'utf8'}).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`).href);
const bits = n => { const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex'); };
const point = p => p ? [bits(p.x), bits(p.z)] : null;
const inputMap = {name: 'Classic canonical bases', w: 128, h: 128, fill: '.',
  spawns: [{x: 8, y: 8}, {x: 112, y: 112}], points: []};
const cases = [];
function run(name, army, exercise, teams = [0, 1]) {
  const authored = structuredClone(inputMap);
  if (teams.length === 3) authored.spawns.push({x: 8, y: 112});
  const map = {...authored, rows: Array(inputMap.h).fill(inputMap.fill.repeat(inputMap.w))};
  delete map.fill;
  const settings = {mode: 'classic', army, tech: true, weather: false, logistics: false};
  const originalRandom = Math.random, constructorDraws = [];
  Math.random = () => { constructorDraws.push(.5); return .5; };
  const names = teams.map((_, i) => String.fromCharCode(65 + i));
  const factions = teams.map((_, i) => i % 2);
  let g;
  try { g = sim.createGame(map, names, false, teams, factions, settings); }
  finally { Math.random = originalRandom; }
  assert.deepEqual([...g.units.values()].map(u => u.type),
    [...teams.map(() => 'hq'), ...teams.flatMap(() => ['engineer', 'rifle'])]);
  assert(g.players.every(p => p.mp === sim.CFG.classic.mpStart * sim.CFG.armies[army].income && p.mun === 0 && p.fuel === 0));
  const numericBits = value => typeof value === 'number' ? bits(value)
    : Array.isArray(value) ? value.map(numericBits)
    : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([k,v]) => [k,numericBits(v)])) : value;
  const recipients = () => {
    const cache = sim.snapshotCache(g);
    return g.players.map((p, slot) => {
      const pick = s => ({queues: s.queues, productionJobs: s.productionJobs});
      const cached = pick(sim.snapshotFor(g, slot, [], [], cache));
      const uncached = pick(sim.snapshotFor(g, slot, [], []));
      assert.deepEqual(cached, uncached);
      for (const rows of [cached.queues, cached.productionJobs]) assert(rows.every(([id]) => g.units.get(id).owner === slot));
      return {cached: numericBits(cached), uncached: numericBits(uncached)};
    });
  };
  const project = () => ({tick: g.tick, next: g.nextId, nextJob: g.nextProductionJob ?? 0,
    time: bits(g.mode.timeLeft), suddenDeath: !!g.mode.suddenDeath, teams: g.mode.teams,
    nodes: g.nodes.map(n => [n.c, point(n), n.depot, bits(n.rate), !!n.fuel]),
    wallets: g.players.map(p => [bits(p.mp), bits(p.mun), bits(p.fuel), bits(p.inc),
      p.fuelInc === undefined ? null : bits(p.fuelInc), p.upkeep === undefined ? null : bits(p.upkeep)]),
    spent: g.players.map(p => bits(g.story?.[p.slot]?.mpSpent ?? 0)),
    producers: [...g.units.values()].filter(b => b.queue).map(b => [b.id, b.owner,
      bits(b.hp), bits(b.built), bits(b.prog), point(b.rally), [...b.queue],
      (b.productionJobs ?? []).map(j => [j.id, j.type, bits(j.mp), bits(j.fuel), j.status])]),
    actors: [...g.units.values()].map(u => [u.id, u.owner, u.type]),
    rallyEnds: [...g.units.values()].filter(u => u.id > teams.length * 3).map(u => [u.id, point(u.routeEnd)]),
  });
  const initial = project(), initialRecipients = recipients(), records = [];
  const initialPoses = [...g.units.values()].map(u => [u.id, bits(u.x), bits(u.z), bits(u.rot), bits(u.aim)]);
  const command = (cmd, expect = null, slot = 0) => {
    const before = project();
    const result = sim.command(g, slot, structuredClone(cmd)) ?? null;
    assert.equal(result, expect);
    if (expect !== null) assert.deepEqual(project(), before);
    records.push({op: 'command', cmd, slot, result, expected: project(), recipients: recipients()});
  };
  let ticks = 0;
  const step = count => {
    for (let i = 0; i < count; i++) { sim.step(g); ticks++; }
    records.push({op: 'steps', count, expected: project(), recipients: recipients()});
  };
  Math.random = () => { throw Error(`${name}: unexpected random draw after constructor`); };
  try { exercise({g, command, step}); }
  finally { Math.random = originalRandom; }
  cases.push({name, input: {map: authored, settings, names, teams, factions}, constructorDraws, initialPoses, initial, initialRecipients, records, ticks});
}
run('classic-start-wallet-large', 'large', ({g, command}) => {
  const hq = [...g.units.values()].find(u => u.owner === 0 && u.type === 'hq');
  command({t: 'buy', unit: 'rifle', from: hq.id});
  assert(g.players[0].team === g.players[1].team);
  command({t: 'cancelProduction', id: hq.id, job: hq.productionJobs[0].id}, 'unseen', 1);
}, [0, 0, 1]);
run('classic-real-queue-cancel-and-birth', 'standard', ({g, command, step}) => {
  step(3000); // Real income from startup, never an assigned purchase wallet.
  const hq = [...g.units.values()].find(u => u.owner === 0 && u.type === 'hq');
  const foreign = [...g.units.values()].find(u => u.owner === 1 && u.type === 'hq');
  for (const from of [foreign.id, 3, String(hq.id), null, 0, {}]) {
    command({t: 'buy', unit: 'rifle', from}, 'queueFull');
  }
  for (let i = 0; i < 5; i++) command({t: 'buy', unit: 'rifle', from: hq.id});
  command({t: 'buy', unit: 'rifle', from: hq.id}, 'queueFull');
  const paid = hq.productionJobs.map(j => ({...j}));
  command({t: 'cancelProduction', id: hq.id, job: paid[2].id}, 'unseen', 1);
  command({t: 'cancelProduction', id: hq.id, job: paid[0].id}, 'notWaiting');
  command({t: 'cancelProduction', id: hq.id, job: paid[2].id});
  command({t: 'cancelProduction', id: hq.id, job: paid[2].id}, 'notWaiting');
  const born = g.nextId;
  command({t: 'rally', x: 41, z: 41});
  assert.equal(g.players[0].rally, null, 'Classic has no personal rally');
  command({t: 'rally', id: hq.id, x: 41, z: 41});
  step(100); assert.equal(g.nextId, 7);
  step(220); assert.equal(g.nextId, 8);
  assert.equal(hq.productionJobs[0].id, paid[1].id);
  assert.deepEqual(g.units.get(born).routeEnd, {x: 41, z: 41});
  command({t: 'cancelProduction', id: hq.id, job: paid[1].id}, 'notWaiting');
});
run('classic-genuine-deadline-clear-and-decay', 'standard', ({g, command, step}) => {
  const hq = [...g.units.values()].find(u => u.owner === 0 && u.type === 'hq');
  const before = Math.floor((sim.CFG.classic.time - 1) / sim.TICK);
  step(before);
  assert(!g.mode.suddenDeath && g.mode.timeLeft > 0);
  for (let i = 0; i < 3; i++) command({t: 'buy', unit: 'rifle', from: hq.id});
  const foreign = [...g.units.values()].find(u => u.owner === 1 && u.type === 'hq');
  command({t: 'buy', unit: 'rifle', from: foreign.id}, null, 1);
  const charged = g.players.map(p => g.story[p.slot].mpSpent);
  let remaining = 0;
  while (!g.mode.suddenDeath && remaining++ < 24) step(1);
  assert(g.mode.suddenDeath);
  assert.equal(hq.queue.length, 0); assert.equal(hq.productionJobs.length, 0); assert.equal(hq.prog, 0);
  assert.deepEqual(g.players.map(p => g.story[p.slot].mpSpent), charged);
  assert([...g.units.values()].filter(b => b.queue).every(b => !b.queue.length && !b.productionJobs.length && b.prog === 0));
  const hp = hq.hp;
  step(200);
  assert(Math.abs(hp - hq.hp - sim.UNITS.hq.hpPer * sim.CFG.classic.decay * 10) < 5);
  command({t: 'buy', unit: 'rifle', from: hq.id}, 'suddenDeath');
  command({t: 'tech', kind: 'tier'}, 'suddenDeath');
  command({t: 'cancelProduction', id: hq.id, job: 1}, 'suddenDeath');
  command({t: 'build', ids: [3], kind: 'barracks', x: 30, z: 30}, 'suddenDeath');
});
clean();
const data = JSON.stringify({reference: pin, node: process.version, arch: process.arch,
  scope: 'Canonical Classic startup and base components from unmodified full Source steps. Native replay is a bounded base caller, not full-match parity.', cases}) + '\n';
const target = `${root}/rust/crates/core/tests/fixtures/classic-base-components.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), data);
else writeFileSync(target, data);
console.log(`${cases.length} Classic scenes; ${cases.reduce((n,c) => n+c.ticks,0)} actual Source full steps`);
