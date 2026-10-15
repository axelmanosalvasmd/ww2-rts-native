// Original Classic constructor and Depot command. No fixture-created actors/nodes.
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
// Valid native admission is authored before either constructor, not amended
// after Source recording. This point pays nothing and is away from the starts.
const map = {name: 'Classic Depot canonical command', w: 128, h: 128,
  rows: Array(128).fill('.'.repeat(128)), spawns: [{x: 8, y: 8}, {x: 112, y: 112}],
  points: [{id: 'neutral', x: 64, y: 64, mp: 0, vp: 0}]};
const settings = {mode: 'classic', army: 'standard', tech: true, weather: false, logistics: false};
const cases = [];
function run(name, exercise, shared = false) {
  const authored = structuredClone(map);
  const teams = shared ? [0, 0, 1] : [0, 1];
  const factions = shared ? [0, 1, 0] : [0, 1];
  const names = shared ? ['A', 'B', 'C'] : ['A', 'B'];
  if (shared) authored.spawns = [{x: 8, y: 8}, {x: 15, y: 8}, {x: 112, y: 112}];
  const originalRandom = Math.random, constructorDraws = [];
  Math.random = () => { constructorDraws.push(.5); return .5; };
  let g;
  try { g = sim.createGame(authored, names, false, teams, factions, settings); }
  finally { Math.random = originalRandom; }
  const own = (slot, type) => [...g.units.values()].find(u => u.owner === slot && u.type === type);
  assert.deepEqual([...g.units.values()].map(u => u.type),
    [...teams.map(() => 'hq'), ...teams.flatMap(() => ['engineer', 'rifle'])]);
  const project = () => ({tick: g.tick, next: g.nextId, nextJob: g.nextProductionJob ?? 0,
    wallets: g.players.map(p => [bits(p.mp), bits(p.mun), bits(p.fuel)]),
    spent: g.players.map(p => bits(g.story?.[p.slot]?.mpSpent ?? 0)),
    nodes: g.nodes.map(n => [n.c, point(n), n.depot, bits(n.rate), !!n.fuel]),
    units: [...g.units.values()].map(u => ({id: u.id, owner: u.owner, type: u.type, hp: bits(u.hp),
      at: point(u), built: u.built === undefined ? null : bits(u.built), cells: u.cells ?? null,
      build: u.build === undefined || u.build === 0 ? null : u.build,
      path: u.path.map(point), routeEnd: point(u.routeEnd),
      queues: u.queue ?? null, jobs: (u.productionJobs ?? []).map(j => [j.id, j.type, bits(j.mp), bits(j.fuel), j.status])})),
    terrain: g.chars.join(''),
    sections: [...g.units.values()].filter(u => u.type === 'depot').map(u => [u.id,
      u.cells.map(c => [c, g.chars[c], bits(g.cellHp[c])])]),
  });
  const initial = project(), records = [];
  const command = (cmd, expect = null, slot = 0) => {
    const before = project();
    const result = sim.command(g, slot, structuredClone(cmd)) ?? null;
    assert.equal(result, expect);
    if (result !== null) assert.deepEqual(project(), before, `${name}: refusal changed measured state`);
    records.push({op: 'command', cmd, slot, result, expected: project()});
  };
  const warmup = () => {
    assert.equal(g.tick, 0);
    sim.step(g);
    assert.equal(g.tick, 1);
    records.push({op: 'warmup', count: 1, expected: project()});
  };
  // Resolve against genuine Source nodes in their canonical insertion order.
  const choose = () => {
    const hq = own(0, 'hq');
    const index = g.nodes.findIndex(n => !n.depot && Math.hypot(n.x - hq.x, n.z - hq.z) <= 29);
    assert(index >= 0, 'constructor supplies an actually visible home node');
    const n = g.nodes[index];
    return {index, x: n.x + 1.25, z: n.z - .75};
  };
  Math.random = () => { throw Error(`${name}: unexpected random draw after constructor`); };
  let selection;
  try { selection = exercise({g, own, command, warmup, choose}); }
  finally { Math.random = originalRandom; }
  cases.push({name, input: {map: authored, settings, names, teams, factions}, constructorDraws, selection, initial, records});
}
run('classic-depot-paid-node-and-duplicate-refusal', ({g, own, command, warmup, choose}) => {
  warmup();
  const selection = choose(), engineer = own(0, 'engineer'), id = g.nextId;
  const before = g.players[0].mp;
  const build = {t: 'build', ids: [engineer.id], kind: 'depot', x: selection.x, z: selection.z};
  command(build);
  const depot = g.units.get(id), node = g.nodes[selection.index];
  assert.equal(g.players[0].mp, before - 60);
  assert.equal(node.depot, id);
  assert.equal(depot.type, 'depot'); assert.equal(depot.owner, 0);
  assert.equal(depot.built, 0); assert.equal(depot.hp, sim.UNITS.depot.hpPer * .25);
  assert.equal(depot.cells.length, 4);
  assert.deepEqual({x: depot.x, z: depot.z}, {x: node.x, z: node.z});
  assert.equal(engineer.build, id);
  assert(engineer.path.length > 0);
  command(build, 'blocked');
  return selection;
});
run('classic-depot-real-selection-and-funds-refusals', ({g, own, command, warmup, choose}) => {
  warmup();
  const selection = choose();
  const build = ids => ({t: 'build', ids, kind: 'depot', x: selection.x, z: selection.z});
  command(build([own(0, 'rifle').id]), 'noBuilders');
  command(build([own(1, 'engineer').id]), 'noBuilders');
  command({...build([own(0, 'engineer').id]), x: 128, z: 128}, 'blocked');
  const hq = own(0, 'hq');
  command({t: 'buy', unit: 'rifle', from: hq.id});
  command({t: 'buy', unit: 'rifle', from: hq.id});
  assert(g.players[0].mp >= 0 && g.players[0].mp < 60);
  command(build([own(0, 'engineer').id]), 'mp');
  assert(g.nodes.every(n => !n.depot));
  return selection;
});
run('classic-depot-equal-distance-node-order-and-next-free', ({g, own, command, warmup}) => {
  warmup();
  let selection;
  // Both candidates are genuine startup nodes. Their exact midpoint creates
  // a distance tie without moving or importing either canonical node.
  for (let i = 0; i < g.nodes.length && !selection; i++) {
    for (let j = i + 1; j < g.nodes.length && !selection; j++) {
      const a = g.nodes[i], b = g.nodes[j];
      if (Math.hypot(a.x - b.x, a.z - b.z) > 16) continue;
      const at = {x: (a.x + b.x) / 2, z: (a.z + b.z) / 2};
      const candidates = g.nodes.map((n, index) => ({index, d: Math.hypot(n.x - at.x, n.z - at.z)}))
        .filter(n => n.d <= 8).sort((a, b) => a.d - b.d);
      if (candidates.length !== 2 || candidates[0].d !== candidates[1].d) continue;
      const team = g.players[0].team;
      if (!sim.teamSees(g, team, a) || !sim.teamSees(g, team, b)) continue;
      selection = {...at, first: candidates[0].index, second: candidates[1].index};
    }
  }
  assert(selection, 'natural allied-HQ nodes provide a visible two-node midpoint tie');
  const engineer = own(0, 'engineer');
  const build = {t: 'build', ids: [engineer.id], kind: 'depot', x: selection.x, z: selection.z};
  const first = g.nextId;
  command(build);
  assert.equal(g.nodes[selection.first].depot, first, 'node insertion order breaks the exact tie');
  const second = g.nextId;
  command(build);
  assert.equal(g.nodes[selection.second].depot, second, 'occupied first node leaves next free node selectable');
  command(build, 'blocked');
  return selection;
}, true);

clean();
const data = JSON.stringify({reference: pin, node: process.version, arch: process.arch,
  scope: 'Canonical Classic constructors, one genuine Source warmup tick per scene and original Depot commands. No full Classic native runner claim.', cases}) + '\n';
const target = `${root}/rust/crates/core/tests/fixtures/classic-depot.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), data);
else writeFileSync(target, data);
console.log(`3 Depot scenes; ${cases.reduce((n,c) => n+c.records.filter(r => r.op === 'command').length,0)} commands; 3 full Source warmup ticks`);
