// Original selected BASE command, queue, crew and section-repair components.
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
const {storyResult} = await import(pathToFileURL(`${reference}/shared/story.js`).href);
const bits = n => { const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex'); };
const point = p => p ? [bits(p.x), bits(p.z)] : null;
const cases = [];
function run(name, mode, shared, exercise) {
  const input = {map: {name: 'BASE consumer completion', w: 128, h: 128,
    rows: Array(128).fill('.'.repeat(128)), spawns: [{x: 8, y: 8}, {x: 112, y: 112}],
    points: [{id: 'neutral', x: 64, y: 64, mp: 0, vp: 0}]},
    settings: {army: 'standard', tech: true, weather: false, supply: false, logistics: false},
    names: ['A', 'B'], teams: [0, 1], factions: [0, 1]};
  if (mode !== 'default') input.settings.mode = mode;
  if (shared) {
    input.map.spawns = [{x: 8, y: 8}, {x: 15, y: 8}, {x: 112, y: 112}];
    input.names = ['A', 'B', 'C']; input.teams = [0, 0, 1]; input.factions = [0, 1, 0];
  }
  assert.equal(sim.validateMap(input.map), null);
  const originalRandom = Math.random, constructorDraws = [];
  Math.random = () => { constructorDraws.push(.5); return .5; };
  let g;
  try { g = sim.createGame(structuredClone(input.map), input.names, false, input.teams, input.factions, input.settings); }
  finally { Math.random = originalRandom; }
  // The real report-card observer initializes Source's lazy story bookkeeping.
  // No gameplay field or expected native value is installed.
  const initialStory = storyResult(g);
  const own = (slot, type) => [...g.units.values()].find(u => u.owner === slot && u.type === type);
  const recipients = () => {
    const cache = sim.snapshotCache(g);
    return g.players.map((_, slot) => {
      const pick = s => ({queues: s.queues, productionJobs: s.productionJobs});
      const cached = pick(sim.snapshotFor(g, slot, [], [], cache));
      const uncached = pick(sim.snapshotFor(g, slot, [], []));
      assert.deepEqual(cached, uncached);
      for (const rows of [cached.queues, cached.productionJobs])
        assert(rows.every(([id]) => g.units.get(id).owner === slot));
      return {cached, uncached};
    });
  };
  const project = () => ({tick: g.tick, next: g.nextId,
    wallets: g.players.map(p => [bits(p.mp), bits(p.mun), bits(p.fuel)]),
    spent: g.players.map(p => bits(g.story[p.slot].mpSpent)), personal: g.players.map(p => point(p.rally)),
    actors: [...g.units.values()].map(u => [u.id, u.owner, u.type, bits(u.hp), u.built === undefined ? null : bits(u.built)]),
    producers: [...g.units.values()].filter(u => u.queue).map(u => [u.id, bits(u.prog), point(u.rally), [...u.queue],
      (u.productionJobs ?? []).map(j => [j.id, j.type, bits(j.mp), bits(j.fuel), j.status])]),
    sections: [...g.units.values()].filter(u => u.cells).map(u => [u.id, u.cells.map(c => {
      const s = g.structuralCells.get(c);
      return [c, bits(s.hp), bits(s.maxHp), s.state === 'failed', g.chars[c]];
    })]), recipients: recipients()});
  const initial = project(), records = [];
  const command = (cmd, result = null, slot = 0) => {
    const before = project(), actual = sim.command(g, slot, structuredClone(cmd)) ?? null;
    assert.equal(actual, result);
    const after = project();
    if (result !== null) assert.deepEqual(after, before, `${name}: refusal changed selected state`);
    records.push({op: 'command', cmd, slot, result, before, after});
  };
  const advance = (count, phase) => {
    const before = project();
    for (let i = 0; i < count; i++) sim.step(g);
    records.push({op: 'advance', count, phase, before, after: project()});
  };
  const breach = (building, contact) => {
    const cell = building.cells[0], section = g.structuralCells.get(cell), damage = section.maxHp;
    const before = project(), result = sim.damageWorldSection(g, cell, damage, {contactId: contact, owner: 1});
    records.push({op: 'breach', building: building.id, cell, damage, contact, result: bits(result), before, after: project()});
    return {cell, section, result};
  };
  Math.random = () => { throw Error(`${name}: unexpected random draw after constructor`); };
  try { exercise({g, own, command, advance, breach}); }
  finally { Math.random = originalRandom; }
  cases.push({name, input, constructorDraws, initialStory, initial, records});
}
for (const mode of ['default', 'conquest']) run(`${mode}-cancel-rally-and-real-recruit`, mode, false, ({g, own, command}) => {
  const hq = own(0, 'hq'), foreign = own(1, 'hq'), rifle = own(0, 'rifle');
  command({t: 'cancelProduction', id: hq.id, job: 1}, 'needs');
  command({t: 'rally', x: 41, z: 41});
  command({t: 'rally', x: -1, z: 41}, 'blocked');
  command({t: 'rally', x: hq.x, z: hq.z}, 'blocked');
  command({t: 'rally', ids: [foreign.id], x: 41, z: 41}, 'unseen');
  command({t: 'rally', ids: [rifle.id], x: 41, z: 41}, 'needs');
  command({t: 'rally', ids: [hq.id], id: null, x: 41, z: 41}, 'unseen');
  const target = {x: hq.x + 20, z: hq.z + 20};
  command({t: 'rally', ids: [hq.id, foreign.id], ...target});
  assert.equal(foreign.rally, null);
  command({t: 'rally', x: 45, z: 45});
  const born = g.nextId;
  command({t: 'buy', unit: 'rifle', from: hq.id});
  assert.deepEqual(g.units.get(born).routeEnd, target, 'building rally precedes personal rally');
  assert.equal(hq.queue.length, 0, 'ordinary recruitment is immediate');
  command({t: 'cancelProduction', id: hq.id, job: 1}, 'needs');
});
run('classic-owned-waiter-identity-and-promotion', 'classic', true, ({g, own, command, advance}) => {
  advance(3000, 'earned');
  const hq = own(0, 'hq'), ally = own(1, 'hq');
  for (let i = 0; i < 3; i++) command({t: 'buy', unit: 'rifle', from: hq.id});
  command({t: 'buy', unit: 'rifle', from: ally.id}, null, 1);
  advance(40, 'head-progress');
  const [head, second, third] = hq.productionJobs.map(j => structuredClone(j));
  assert.equal(new Set([head.id, second.id, third.id]).size, 3);
  assert.deepEqual(hq.productionJobs.map(j => j.status), ['active', 'waiting', 'waiting']);
  command({t: 'cancelProduction', id: hq.id, job: third.id}, 'unseen', 1);
  command({t: 'cancelProduction', id: hq.id, job: third.id}, 'unseen', 2);
  command({t: 'cancelProduction', id: hq.id, job: head.id}, 'notWaiting');
  const progress = hq.prog, wallet = g.players[0].mp, fuel = g.players[0].fuel;
  command({t: 'cancelProduction', id: hq.id, job: third.id});
  assert.equal(g.players[0].mp, wallet + third.mp); assert.equal(g.players[0].fuel, fuel + third.fuel);
  assert.equal(hq.prog, progress); assert.deepEqual(hq.productionJobs.map(j => j.id), [head.id, second.id]);
  command({t: 'cancelProduction', id: hq.id, job: third.id}, 'notWaiting');
  command({t: 'rally', x: 41, z: 41}, 'unseen');
  command({t: 'rally', id: hq.id, x: 41, z: 41});
  advance(280, 'head-born-waiter-promoted');
  assert.equal(hq.productionJobs[0].id, second.id); assert.equal(hq.productionJobs[0].status, 'active');
  command({t: 'cancelProduction', id: hq.id, job: second.id}, 'notWaiting');
  command({t: 'cancelProduction', id: hq.id, job: head.id}, 'notWaiting');
});
run('classic-sequential-sites-queued-repair-and-paid-mg', 'classic', false, ({g, own, command, advance, breach}) => {
  advance(3000, 'earned');
  const engineer = own(0, 'engineer'), hq = own(0, 'hq');
  const first = g.nextId;
  command({t: 'build', kind: 'barracks', ids: [engineer.id], x: hq.x + 20, z: hq.z});
  const second = g.nextId;
  command({t: 'build', kind: 'barracks', ids: [engineer.id], x: hq.x + 20, z: hq.z + 16, queue: true});
  assert.equal(engineer.build, first); assert.equal(engineer.orders[0].id, second);
  assert.equal(g.units.get(second).built, 0);
  command({t: 'buy', unit: 'mg', from: first}, 'needs');
  advance(1600, 'both-sites-complete');
  const bar = g.units.get(first);
  assert.equal(bar.built, 1); assert.equal(g.units.get(second).built, 1);
  assert.equal(engineer.orders.length, 0);
  command({t: 'move', orders: [[engineer.id, bar.x + 5, bar.z]]});
  advance(160, 'engineer-walks-to-repair-reach');
  command({t: 'rally', id: first, x: bar.x + 4, z: bar.z + 24});
  command({t: 'buy', unit: 'mg', from: first});
  const job = structuredClone(bar.productionJobs[0]), hp = bar.hp;
  const hit = breach(bar, 'building-contact');
  assert(hit.result > 0); assert(Math.abs(bar.hp - (hp - hit.result)) < .001);
  assert.equal(hit.section.state, 'failed'); assert.deepEqual(bar.productionJobs, [job]);
  assert.equal(breach(bar, 'building-contact').result, 0, 'same contact debits once');
  command({t: 'assist', ids: [engineer.id], id: first});
  const spent = g.story[0].mpSpent;
  advance(2, 'supported-section-restored');
  assert(hit.section.hp > 0); assert.equal(g.chars[hit.cell], 'K');
  assert.equal(g.story[0].mpSpent, spent); assert.equal(bar.productionJobs[0].id, job.id);
  const target = {x: bar.x + 5, z: bar.z + 12};
  command({t: 'move', orders: [[engineer.id, target.x, target.z]]});
  command({t: 'assist', ids: [engineer.id], id: first, queue: true});
  assert.equal(engineer.build, 0, 'waiting repair preserves the real active Move');
  assert.equal(engineer.orders[0].id, first);
  advance(700, 'queued-repair-and-real-mg-complete');
  assert.equal(bar.hp, sim.UNITS.barracks.hpPer); assert.equal(g.story[0].mpSpent, spent);
  assert.equal(engineer.orders.length, 0); assert.equal(engineer.build, 0);
  const mg = own(0, 'mg'); assert(mg); assert.equal(bar.queue.length, 0);
  assert.deepEqual(mg.routeEnd, bar.rally);
});
run('classic-real-deadline-cancel-denial-precedence', 'classic', false, ({g, own, command, advance}) => {
  const before = Math.floor((sim.CFG.classic.time - 1) / sim.TICK);
  advance(before, 'before-deadline');
  assert(!g.mode.suddenDeath);
  const hq = own(0, 'hq');
  for (let i = 0; i < 3; i++) command({t: 'buy', unit: 'rifle', from: hq.id});
  const spent = g.story[0].mpSpent;
  advance(24, 'genuine-deadline');
  assert(g.mode.suddenDeath); assert.equal(hq.queue.length, 0); assert.equal(hq.prog, 0);
  assert.equal(g.story[0].mpSpent, spent, 'late clear refunds nothing');
  for (const cmd of [{t: 'cancelProduction'}, {t: 'cancelProduction', id: null, job: 'bad'},
    {t: 'cancelProduction', id: hq.id, job: 1}]) command(cmd, 'suddenDeath');
  const hp = hq.hp;
  advance(200, 'decay-ten-seconds');
  assert(Math.abs(hp - hq.hp - sim.UNITS.hq.hpPer * sim.CFG.classic.decay * 10) < 5);
});
clean();
const data = JSON.stringify({reference: pin, node: process.version, arch: process.arch,
  scope: 'Selected original components with natural constructors, paid commands, real section contacts and fixed ticks. No wire, reconnect, full retirement or whole-row claim.', cases}) + '\n';
const target = `${root}/rust/crates/core/tests/fixtures/base-consumer-completion.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), data);
else writeFileSync(target, data);
console.log(`${cases.length} scenes; ${cases.reduce((n,c) => n+c.records.length,0)} actions; ${cases.reduce((n,c) => n+c.records.reduce((a,r) => a+(r.count ?? 0),0),0)} original Source steps`);
