// Unrun component witnesses over the existing admitted ordinary BASE arena.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? resolve(root, 'reference/ww2-rts'));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], {encoding: 'utf8'}).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const sim = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const {rebuildGrid} = await import(pathToFileURL(resolve(reference, 'shared/grid.js')));
const {storyResult} = await import(pathToFileURL(resolve(reference, 'shared/story.js')));
// Read constructor inputs only. No old or native expected state is installed.
const base = JSON.parse(readFileSync(resolve(root, 'rust/crates/simtest/tests/fixtures/base-ordinary-checks.json'), 'utf8'));
assert.equal(base.reference, pin);
assert.equal(base.seed, 701);
assert.equal(base.opts.mode, 'conquest');
assert.equal(sim.validateMap(base.map), null);
const buffer = new DataView(new ArrayBuffer(8));
const bits = n => {
  buffer.setFloat64(0, n, false);
  return buffer.getBigUint64(0, false).toString(16).padStart(16, '0');
};
const mask = (1n << 64n) - 1n, gamma = 0x9e3779b97f4a7c15n;
const mix = n => {
  n = ((n ^ (n >> 30n)) * 0xbf58476d1ce4e5b9n) & mask;
  n = ((n ^ (n >> 27n)) * 0x94d049bb133111ebn) & mask;
  return n ^ (n >> 31n);
};
const constructorDraw = Number(mix((mix(701n ^ ((2n * gamma) & mask)) + gamma) & mask) & 0xffffffffn) / 2**32;
const originalRandom = Math.random, cases = [];
function record(name, exercise) {
  let draws = 0;
  Math.random = () => { draws++; return constructorDraw; };
  const g = sim.createGame(base.map, base.inputs.names, base.inputs.shuffle, base.inputs.teams, base.inputs.factions, base.opts);
  assert.equal(draws, 1);
  Math.random = () => { throw Error(`${name}: unexpected runtime randomness`); };
  const originalActors = [...g.units.keys()];
  const own = (owner, type) => [...g.units.values()].find(u => u.owner === owner && u.type === type);
  const aliases = {builder: own(0, 'rifle').id, hq: own(0, 'hq').id,
    barracks: own(0, 'barracks').id, enemyHq: own(1, 'hq').id, enemyBarracks: own(1, 'barracks').id};
  for (const u of g.units.values()) Object.assign(u, base.setup.allUnits);
  Object.assign(g.units.get(aliases.builder), base.setup.builder);
  g.players[0].mp = base.setup.player0MP;
  const refresh = () => { rebuildGrid(g); sim.internals.updateVision(g); };
  refresh();
  // This projection is fixed before execution. It is a component observation,
  // not the complete raw state used by the older retained lifecycle recordings.
  const snapshot = () => {
    assert.equal(g.points.length, 1);
    const p = g.points[0];
    assert.deepEqual([p.owner, p.capper, p.progress, p.cut, !!p.contested, p.onPoint ?? []], [-1, -1, 0, false, false, []]);
    for (const cell of g.buildingCells.keys()) {
      assert.ok(Math.hypot((cell % g.w + .5) * sim.CELL - p.x, (Math.floor(cell / g.w) + .5) * sim.CELL - p.z) > 40);
    }
    return {tick: g.tick, nextId: g.nextId,
    actors: [...g.units.values()].map(u => [u.id, u.owner, u.type, bits(u.hp), bits(u.x), bits(u.z),
      u.built === undefined ? null : bits(u.built), u.cells ?? null, u.structureId ?? null]),
    controls: [...g.units.values()].map(u => [u.id, (u.orders ?? []).length, u.build === undefined || u.build === 0 ? null : u.build]),
    wallets: g.players.map(p => bits(p.mp)),
    story: storyResult(g).story.map(p => [bits(p.kills), bits(p.losses), bits(p.mpSpent)]),
    chars: g.chars.join(''),
    buildingCells: [...g.buildingCells].sort((a, b) => a[0] - b[0]).map(([c, b]) => [c, b.id]),
    structures: [...g.structures.values()].map(s => [s.id, s.entityId ?? null, s.sections.map(section => section.c)]),
    };
  };
  const initial = snapshot(), records = [];
  function command(cmd, label, expect = null, alias = null, preview = false) {
    const before = new Set(g.units.keys());
    if (preview) {
      const query = {...cmd, kind: cmd.kind, team: g.players[0].team};
      assert.equal(sim.placementCheck(g, query, at => sim.teamSees(g, g.players[0].team, at)).reason, expect);
    }
    assert.equal(sim.command(g, 0, cmd) ?? null, expect, label);
    if (alias) {
      const born = [...g.units.keys()].filter(id => !before.has(id));
      assert.equal(born.length, 1); aliases[alias] = born[0];
    }
    records.push({op: 'command', label, cmd: structuredClone(cmd), expect, alias, expected: snapshot()});
  }
  const pose = (x, z) => {
    Object.assign(g.units.get(aliases.builder), {x, z}); refresh();
    records.push({op: 'pose', x, z, expected: snapshot()});
  };
  const tick = () => { sim.step(g); records.push({op: 'tick', expected: snapshot()}); };
  exercise({g, aliases, command, pose, tick, snapshot});
  cases.push({name, originalActors, initial, records});
}
try {
  record('base26_conquest_queue_refusal_stop_retry', ({g, aliases: a, command, pose, snapshot}) => {
    pose(71, 71);
    command({t: 'move', orders: [[a.builder, 71, 101]]}, 'active-move');
    for (let i = 0; i < 8; i++) command({t: 'move', orders: [[a.builder, 73 + i * 2, 101]], queue: true}, `waiting-${i}`);
    const builder = g.units.get(a.builder), waiting = structuredClone(builder.orders), before = snapshot();
    assert.equal(waiting.length, 8);
    const build = {t: 'build', ids: [a.builder], kind: 'barracks', x: 91, z: 83, queue: true};
    command(build, 'full-queue', 'queueFull');
    assert.deepEqual(snapshot(), before, 'refusal preserves actors, wallets, identity, terrain and controls');
    assert.deepEqual(builder.orders, waiting, 'all eight accepted orders are unchanged');
    command({t: 'stop', ids: [a.builder]}, 'stop');
    command(build, 'retry', null, 'site');
    const site = g.units.get(a.site);
    assert.equal(site.type, 'barracks'); assert.equal(site.built, 0);
    assert.equal(g.players[0].mp, base.setup.player0MP - sim.UNITS.barracks.cost);
    assert.equal(builder.orders[0].id, site.id);
  });
  for (const [name, target, reason] of [['base27_visible_blocked', 'hq', 'blocked'], ['base27_hidden_masks_blocked', 'enemyHq', 'notVisible']]) {
    record(name, ({g, aliases: a, command, snapshot}) => {
      const at = g.units.get(a[target]), before = snapshot();
      command({t: 'build', ids: [a.builder], kind: 'barracks', x: at.x, z: at.z}, 'placement', reason, null, true);
      assert.deepEqual(snapshot(), before);
    });
  }
  record('recruitment_real_makers_and_prerequisites', ({g, aliases: a, command, snapshot}) => {
    for (const from of [a.enemyBarracks, a.builder]) {
      const before = snapshot();
      command({t: 'buy', unit: 'mg', from}, 'ineligible-maker', 'needs');
      assert.deepEqual(snapshot(), before);
    }
    command({t: 'build', ids: [a.builder], kind: 'airfield', x: 50, z: 80}, 'missing-motorpool', 'needs');
    command({t: 'build', ids: [a.builder], kind: 'barracks', x: 50, z: 80}, 'real-site', null, 'site');
    assert.equal(g.units.get(a.site).built, 0);
    command({t: 'buy', unit: 'mg', from: a.site}, 'unfinished-explicit-maker', 'needs');
    command({t: 'destroy', id: a.site}, 'scuttle-site');
    command({t: 'buy', unit: 'mg', from: a.site}, 'dead-explicit-maker', 'needs');
    command({t: 'buy', unit: 'mg', from: a.barracks}, 'real-maker-positive', null, 'purchased');
    const purchased = g.units.get(a.purchased), producer = g.units.get(a.barracks);
    assert.equal(purchased.type, 'mg'); assert.equal(purchased.owner, 0);
    assert.ok(Math.hypot(purchased.x - producer.x, purchased.z - producer.z) < 12);
    assert.equal(producer.queue.length, 0);
  });
  record('base32_scuttle_owned_hq_and_unit_next_tick', ({g, aliases: a, command, tick}) => {
    const wallets = g.players.map(p => p.mp);
    command({t: 'destroy', ids: [a.enemyHq]}, 'foreign-refused', 'unseen');
    command({t: 'destroy', ids: [a.builder], id: a.hq}, 'own-scuttle');
    assert.deepEqual(g.players.map(p => p.mp), wallets, 'scuttle has no immediate refund');
    assert.ok(g.units.has(a.hq) && g.units.has(a.builder), 'retirement waits for the full tick');
    assert.equal(g.units.get(a.hq).lastHit, 0); assert.equal(g.units.get(a.builder).lastHit, 0);
    tick();
    assert.ok(!g.units.has(a.hq) && !g.units.has(a.builder) && g.units.has(a.enemyHq));
    assert.equal(g.story[1].kills, 0, 'enemy earns no scuttle kills');
    assert.equal(g.story[0].mpSpent, 0, 'scuttle is not a refund');
    assert.equal(g.story[0].losses, 2);
  });
} finally { Math.random = originalRandom; }
clean();
const output = JSON.stringify({reference: pin, node: process.version, arch: process.arch, seed: 701,
  scope: 'Five ordinary Conquest component witnesses; no Classic or whole-row equivalence claim',
  input: {map: base.map, opts: base.opts, inputs: base.inputs, setup: base.setup}, cases}) + '\n';
const target = resolve(root, 'rust/crates/simtest/tests/fixtures/base-owned-witnesses.json');
if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), output);
else writeFileSync(target, output);
console.log(`5 component cases; ${cases.reduce((n, c) => n + c.records.filter(r => r.op === 'tick').length, 0)} full steps`);
