// Original constructors, command bodies and training component. No full-tick claim.
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
assert.ok(process.argv.slice(2).every(a => a === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], {encoding: 'utf8'}).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const url = pathToFileURL(`${reference}/shared/sim.js`).href;
registerHooks({load(u, c, next) {
  const result = next(u, c);
  return u === url ? {...result, source: `${String(result.source)}\nexport {train,gridFor};\n`} : result;
}});
const sim = await import(url);
const prior = JSON.parse(readFileSync(`${root}/rust/crates/simtest/tests/fixtures/match-production.json`, 'utf8'));
assert.equal(prior.reference, pin);
const bits = n => {
  const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex');
};
const point = p => p ? [bits(p.x), bits(p.z)] : null;
const cases = [];
function run(name, world, exercise) {
  // Only declared constructor inputs are reused. No prior expected frame is read.
  const inputCase = prior.cases.find(c => !!c.map.world === world);
  const map = {...structuredClone(inputCase.map), rows: Array(inputCase.map.h).fill(inputCase.map.fill.repeat(inputCase.map.w))};
  delete map.fill;
  const originalRandom = Math.random;
  const constructorDraws = [];
  Math.random = () => { constructorDraws.push(.5); return .5; };
  let g;
  try { g = sim.createGame(map, ['A', 'B'], false, [0, 1], [0, 1], inputCase.settings); }
  finally { Math.random = originalRandom; }
  const initialIds = [...g.units.keys()], initialNext = g.nextId;
  for (const p of g.players) Object.assign(p, {mp: 5000.125, mun: 500.5, fuel: 500.25});
  const makers = [...g.units.values()].filter(u => u.owner >= 0 && u.queue).map(u => u.id);
  const own = (slot, kind = 'hq') => [...g.units.values()].find(u => u.owner === slot && u.type === kind);
  function projection() {
    return {
      tick: g.tick, next: g.nextId, nextJob: g.nextProductionJob ?? 0, mode: g.mode?.kind ?? null,
      wallets: g.players.map(p => [bits(p.mp), bits(p.mun), bits(p.fuel)]),
      spent: g.players.map(p => bits(g.story?.[p.slot]?.mpSpent ?? 0)),
      personal: g.players.map(p => point(p.rally)),
      buildings: makers.map(id => {
        const b = g.units.get(id);
        return [id, b.owner, b.type, bits(b.hp), bits(b.built), bits(b.prog), point(b.rally),
          [...b.queue], (b.productionJobs ?? []).map(j => [j.id, j.type, bits(j.mp), bits(j.fuel), j.status])];
      }),
      actors: [...g.units.values()].map(u => [u.id, u.owner, u.type, bits(u.hp), bits(u.x), bits(u.z), u.cells ?? null]),
      births: [...g.units.values()].filter(u => u.id >= initialNext).map(u => [u.id, u.owner, u.type,
        [u.x, u.z, u.rot, u.aim, u.ghost ?? 0].map(bits), point(u.worldGoal), point(u.routeEnd), u.path.map(point),
        u.drift === 'rally', u.auto, u.autoRetreat, sim.gridFor(g).candidates(u, 12).map(v => v.id)]),
    };
  }
  function recipients() {
    const cache = sim.snapshotCache(g);
    return g.players.map((p, slot) => {
      const pick = s => ({queues: s.queues, productionJobs: s.productionJobs});
      const cached = pick(sim.snapshotFor(g, slot, [], [], cache));
      const uncached = pick(sim.snapshotFor(g, slot, [], []));
      assert.deepEqual(cached, uncached, `${name}: cached/uncached owner rows`);
      for (const rows of [cached.queues, cached.productionJobs]) {
        assert(rows.every(([id]) => g.units.get(id).owner === slot));
      }
      return {cached, uncached};
    });
  }
  const initial = projection(), initialRecipients = recipients(), records = [];
  const command = (cmd, expect = null, slot = 0) => {
    const before = projection();
    const result = sim.command(g, slot, structuredClone(cmd)) ?? null;
    assert.equal(result, expect);
    if (expect !== null) assert.deepEqual(projection(), before, 'refusal preserves measured state');
    records.push({op: 'command', cmd, slot, result, expected: projection(), recipients: recipients()});
  };
  const train = dt => {
    const maker = own(0);
    assert(maker.hp > 0 && maker.built >= 1 && maker.queue.length && !g.mode.suddenDeath);
    sim.train(g, maker, dt);
    records.push({op: 'train', maker: maker.id, dt, expected: projection(), recipients: recipients()});
  };
  Math.random = () => { throw Error(`${name}: unexpected random draw after constructor`); };
  try { exercise({g, own, command, train, projection}); }
  finally { Math.random = originalRandom; }
  cases.push({name, input: {name, map: inputCase.map, settings: inputCase.settings, initialIds}, constructorDraws,
    initialIds, initialNext, makers, initial, initialRecipients, records});
}

run('world-five-slots-invalid-makers', true, ({g, own, command}) => {
  const hq = own(0), foreign = own(1), nonmaker = own(0, 'engineer');
  const rifles = [...g.units.values()].filter(u => u.owner === 0 && u.type === 'rifle').length;
  command({t: 'buy', unit: 'rifle', from: foreign.id}, null, 1);
  for (const from of [foreign.id, nonmaker.id, String(hq.id), null, 0, {}]) {
    command({t: 'buy', unit: 'rifle', from}, 'queueFull');
  }
  command({t: 'buy', unit: 'rifle', from: hq.id});
  for (let i = 0; i < 4; i++) command({t: 'buy', unit: 'rifle'});
  assert.equal(hq.queue.length, 5);
  command({t: 'buy', unit: 'rifle'}, 'queueFull');
  command({t: 'buy', unit: 'rifle', from: hq.id}, 'queueFull');
  assert.equal([...g.units.values()].filter(u => u.owner === 0 && u.type === 'rifle').length, rifles);
});

run('world-waiter-cancel-preserves-head', true, ({g, own, command, train}) => {
  const hq = own(0);
  for (let i = 0; i < 3; i++) command({t: 'buy', unit: 'rifle', from: hq.id});
  const [head, second, third] = hq.productionJobs.map(j => j.id);
  assert.equal(new Set([head, second, third]).size, 3);
  train(5);
  command({t: 'cancelProduction', id: hq.id, job: head}, 'notWaiting');
  command({t: 'cancelProduction', id: hq.id, job: second}, 'unseen', 1);
  command({t: 'cancelProduction', id: hq.id, job: String(second)}, 'unseen');
  command({t: 'cancelProduction', id: hq.id, job: -1}, 'notWaiting');
  const before = g.players[0].mp, spent = g.story[0].mpSpent, refund = hq.productionJobs[1].mp;
  command({t: 'cancelProduction', id: hq.id, job: second});
  assert.equal(g.players[0].mp, before + refund);
  assert.equal(g.story[0].mpSpent, spent - refund);
  assert.equal(hq.prog, 5);
  assert.deepEqual(hq.productionJobs.map(j => j.id), [head, third]);
  command({t: 'cancelProduction', id: hq.id, job: second}, 'notWaiting');
  train(11);
  assert.equal(hq.productionJobs[0].id, third);
  command({t: 'cancelProduction', id: hq.id, job: third}, 'notWaiting');
});

run('world-real-trained-rifle-rally', true, ({g, own, command, train}) => {
  const hq = own(0), initial = g.nextId;
  command({t: 'rally', id: hq.id, x: 57, z: 33});
  command({t: 'buy', unit: 'rifle', from: hq.id});
  command({t: 'buy', unit: 'rifle', from: hq.id});
  train(5); assert.equal(g.nextId, initial);
  train(11); assert.equal(g.nextId, initial + 1);
  const rifle = g.units.get(initial);
  assert.equal(rifle.type, 'rifle'); assert.equal(rifle.owner, 0);
  assert.deepEqual(rifle.path.at(-1), {x: 57, z: 33});
  assert.equal(hq.productionJobs.length, 1); assert.equal(hq.prog, 0);
  train(15); assert.equal(g.nextId, initial + 2);
});

run('conquest-personal-rally-real-birth', false, ({g, own, command}) => {
  assert.equal(g.players[0].rally, null);
  command({t: 'rally', x: 41, z: 41});
  for (const [x, z] of [[-1, 41], [41, -1], [80, 41], [41, 60], [null, 41], ['41', 41]]) {
    command({t: 'rally', x, z}, 'blocked');
  }
  command({t: 'rally', x: own(0).x, z: own(0).z}, 'blocked');
  const born = g.nextId;
  command({t: 'buy', unit: 'rifle', from: own(0).id});
  assert.deepEqual(g.units.get(born).path.at(-1), {x: 41, z: 41});
});

run('conquest-building-rally-selection', false, ({g, own, command}) => {
  const hq = own(0), foreign = own(1), rifle = own(0, 'rifle');
  command({t: 'rally', ids: [foreign.id], x: 20, z: 18}, 'unseen');
  command({t: 'rally', ids: [rifle.id], x: 20, z: 18}, 'needs');
  command({t: 'rally', ids: [...Array(50).fill(rifle.id), hq.id], x: 20, z: 18}, 'needs');
  command({t: 'rally', ids: [hq.id], id: null, x: 20, z: 18}, 'unseen');
  command({t: 'rally', ids: [hq.id], x: -50, z: 500});
  assert.deepEqual(hq.rally, {x: 0, z: 60});
  command({t: 'rally', id: hq.id, x: 20, z: 18});
  assert.equal(g.players[0].rally, null);
  const born = g.nextId;
  command({t: 'buy', unit: 'rifle', from: hq.id});
  assert.deepEqual(g.units.get(born).path.at(-1), {x: 20, z: 18});
});

clean();
const data = JSON.stringify({reference: pin, node: process.version, arch: process.arch,
  scope: 'Five original native-command component scenes. Training helpers are not full ticks. No Classic, Horde, wire or whole-row equivalence claim.', cases}) + '\n';
const target = `${root}/rust/crates/simtest/tests/fixtures/base-command-consumers.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), data);
else writeFileSync(target, data);
console.log(`5 command-consumer scenes; ${cases.reduce((n, c) => n + c.records.length, 0)} action frames; 0 full ticks`);
