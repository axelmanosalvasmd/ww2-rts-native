// Original Classic commands and steps, with real waiting orders and constructors.
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
const cases = [];
function run(name, shared, exercise) {
  const input = {map: {name: 'Classic Depot canonical command', w: 128, h: 128,
    rows: Array(128).fill('.'.repeat(128)), spawns: [{x: 8, y: 8}, {x: 112, y: 112}],
    points: [{id: 'neutral', x: 64, y: 64, mp: 0, vp: 0}]},
    settings: {mode: 'classic', army: 'standard', tech: true, weather: false, logistics: false},
    names: ['A', 'B'], teams: [0, 1], factions: [0, 1]};
  if (shared) {
    input.map.spawns = [{x: 8, y: 8}, {x: 15, y: 8}, {x: 112, y: 112}];
    input.names = ['A', 'B', 'C']; input.teams = [0, 0, 1]; input.factions = [0, 1, 0];
  }
  const originalRandom = Math.random, constructorDraws = [];
  Math.random = () => { constructorDraws.push(.5); return .5; };
  let g;
  try { g = sim.createGame(structuredClone(input.map), input.names, false, input.teams, input.factions, input.settings); }
  finally { Math.random = originalRandom; }
  const own = (slot, type) => [...g.units.values()].find(u => u.owner === slot && u.type === type);
  const raw = () => ({tick: g.tick, next: g.nextId,
    wallets: g.players.map(p => [bits(p.mp), bits(p.mun), bits(p.fuel)]),
    story: structuredClone(g.story), nodes: (g.nodes ?? []).map(n => [n.c, n.depot, bits(n.rate), !!n.fuel]),
    units: [...g.units.values()].map(u => ({id: u.id, owner: u.owner, type: u.type, hp: bits(u.hp),
      x: bits(u.x), z: bits(u.z), built: u.built === undefined ? null : bits(u.built),
      cells: u.cells ?? null, build: u.build || null, path: u.path.map(point), routeEnd: point(u.routeEnd),
      orders: structuredClone(u.orders ?? []), queue: u.queue ?? null, prog: u.prog === undefined ? null : bits(u.prog),
      jobs: structuredClone(u.productionJobs ?? []), rally: point(u.rally)})),
    terrain: g.chars.join(''), cellHp: Array.from(g.cellHp, bits),
    structures: [...g.structures], sections: [...g.structuralCells], falling: structuredClone(g.fallingSections),
  });
  const initial = raw(), records = [];
  const command = (cmd, expect = null, slot = 0) => {
    const before = raw(), result = sim.command(g, slot, structuredClone(cmd)) ?? null;
    assert.equal(result, expect);
    if (result !== null) assert.deepEqual(raw(), before, `${name}: refused command changed state`);
    records.push({op: 'command', cmd, slot, result, before, after: raw()});
  };
  const advance = (count, phase) => {
    const before = raw();
    for (let i = 0; i < count; i++) sim.step(g);
    assert.equal(g.tick, before.tick + count);
    records.push({op: 'advance', count, phase, before, after: raw()});
  };
  Math.random = () => { throw Error(`${name}: unexpected post-constructor random draw`); };
  try { exercise({g, own, command, advance}); }
  finally { Math.random = originalRandom; }
  cases.push({name, input, constructorDraws, initial, records});
}
run('full-queue-barracks-retry-and-real-mg', false, ({g, own, command, advance}) => {
  advance(1000, 'earned');
  const engineer = own(0, 'engineer'), hq = own(0, 'hq');
  assert(g.players[0].mp >= sim.UNITS.barracks.cost + sim.UNITS.mg.cost);
  const at = {x: hq.x + 20, z: hq.z};
  assert(sim.placementCheck(g, {kind: 'barracks', ...at, team: 0}, p => sim.teamSees(g, 0, p)).ok);
  const reserved = g.nodes.find(n => Math.hypot(n.x - hq.x, n.z - hq.z) <= 29);
  assert(reserved);
  command({t: 'build', kind: 'barracks', ids: [engineer.id], x: reserved.x, z: reserved.z}, 'blocked');
  command({t: 'move', orders: [[engineer.id, hq.x, hq.z + 20]]});
  for (let i = 0; i < 8; i++) command({t: 'move', orders: [[engineer.id, hq.x + i * 2, hq.z + 24]], queue: true});
  assert.equal(engineer.orders.length, 8);
  advance(1, 'moves-still-waiting');
  assert.equal(engineer.orders.length, 8);
  const build = {t: 'build', kind: 'barracks', ids: [engineer.id], ...at, queue: true};
  command(build, 'queueFull');
  command({t: 'stop', ids: [engineer.id]});
  const id = g.nextId, paid = g.players[0].mp;
  command(build);
  const site = g.units.get(id);
  assert.equal(site.type, 'barracks'); assert.equal(site.built, 0);
  assert.equal(g.players[0].mp, paid - sim.UNITS.barracks.cost);
  assert.equal(engineer.orders.length, 1);
  assert.equal(engineer.orders[0].t, 'assist'); assert.equal(engineer.orders[0].id, id);
  command({t: 'buy', unit: 'mg', from: id}, 'needs');
  assert.equal(site.queue.length, 0, 'unfinished real Barracks cannot accept a training job');
  advance(1, 'queued-assist-start');
  assert.equal(engineer.build, id); assert.equal(engineer.orders.length, 0);
  advance(800, 'barracks-complete');
  assert.equal(site.built, 1); assert.equal(site.hp, sim.UNITS.barracks.hpPer);
  command({t: 'rally', id, x: at.x + 4, z: at.z + 24});
  command({t: 'buy', unit: 'mg', from: id});
  advance(300, 'still-training');
  assert(!own(0, 'mg')); assert.equal(site.queue.length, 1);
  advance(80, 'mg-born');
  const mg = own(0, 'mg');
  assert(mg && mg.hp > 0); assert.equal(site.queue.length, 0);
  assert.deepEqual(mg.routeEnd, site.rally);
});
run('allied-engineer-assist-queue-and-owner-cancel', true, ({g, own, command, advance}) => {
  advance(1, 'warmup');
  const hq = own(0, 'hq'), engineer = own(0, 'engineer'), ally = own(1, 'engineer');
  const node = g.nodes.find(n => !n.depot && Math.hypot(n.x - hq.x, n.z - hq.z) <= 29);
  assert(node);
  const id = g.nextId;
  command({t: 'build', kind: 'depot', ids: [engineer.id], x: node.x, z: node.z});
  command({t: 'assist', ids: [own(0, 'rifle').id], id}, 'noBuilders');
  command({t: 'assist', ids: [own(2, 'engineer').id], id}, 'noBuilders');
  const wallet = g.players[1].mp;
  command({t: 'assist', ids: [ally.id], id}, null, 1);
  assert.equal(ally.build, id); assert.equal(g.players[1].mp, wallet);
  command({t: 'cancel', id}, 'unseen', 1);
  for (let i = 0; i < 8; i++) command({t: 'assist', ids: [ally.id], id, queue: true}, null, 1);
  command({t: 'assist', ids: [ally.id], id, queue: true}, 'queueFull', 1);
  assert.equal(ally.orders.length, 8);
  command({t: 'cancel', id});
  assert(!g.units.has(id)); assert.equal(node.depot, 0);
  advance(1, 'cancelled-assist-cleared');
  assert.equal(engineer.build, 0); assert.equal(ally.build, 0); assert.equal(ally.orders.length, 0);
  const replacement = g.nextId;
  command({t: 'build', kind: 'depot', ids: [engineer.id], x: node.x, z: node.z});
  command({t: 'assist', ids: [ally.id], id: replacement}, null, 1);
  for (let i = 0; i < 8; i++) command({t: 'assist', ids: [ally.id], id: replacement, queue: true}, null, 1);
  advance(500, 'allied-complete-and-queue-cleared');
  assert.equal(g.units.get(replacement).built, 1); assert.equal(g.units.get(replacement).hp, sim.UNITS.depot.hpPer);
  assert.equal(engineer.build, 0); assert.equal(ally.build, 0); assert.equal(ally.orders.length, 0);
  assert.equal(g.story[1].mpSpent, 0); assert.equal(g.story[0].mpSpent, sim.UNITS.depot.cost * 1.25);
});
clean();
const data = JSON.stringify({reference: pin, node: process.version, arch: process.arch,
  scope: 'Original queued construction, allied Assist and paid training components over real Classic constructors.', cases}) + '\n';
const target = `${root}/rust/crates/core/tests/fixtures/classic-construction-queue.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), data);
else writeFileSync(target, data);
console.log(`${cases.length} scenes; ${cases.reduce((n,c) => n+c.records.filter(r => r.op === 'command').length,0)} commands; ${cases.reduce((n,c) => n+c.records.reduce((a,r) => a+(r.count ?? 0),0),0)} original Source steps`);
