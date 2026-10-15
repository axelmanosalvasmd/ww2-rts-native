// Complete pending-drop predicates on a retained canonical ordinary scene.
// Source constructor/command/step bodies and the original named test stay unchanged.
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
const view = new DataView(new ArrayBuffer(8));
const bits = value => {
  assert.ok(Number.isFinite(value));
  view.setFloat64(0, value, false);
  return view.getBigUint64(0, false).toString(16).padStart(16, '0');
};
const point = value => value ? [bits(value.x), bits(value.z)] : null;
const mask = (1n << 64n) - 1n, gamma = 0x9e3779b97f4a7c15n;
const mix = value => {
  value = ((value ^ (value >> 30n)) * 0xbf58476d1ce4e5b9n) & mask;
  value = ((value ^ (value >> 27n)) * 0x94d049bb133111ebn) & mask;
  return value ^ (value >> 31n);
};
const weatherState = (mix(701n ^ ((2n * gamma) & mask)) + gamma) & mask;
const constructorDraw = Number(mix(weatherState) & 0xffffffffn) / 2**32;
// The existing purpose adapter supplies Source draws only. Native RNG is never reset.
function randomInputs() {
  const states = new Map(), counts = new Map(), sites = new Map();
  return {
    next() {
      const stack = new Error().stack;
      const line = stack.split('\n').find(row => row.includes(`${resolve(reference, 'shared/sim.js')}:`));
      assert.ok(line, `Unknown Source random site: ${stack}`);
      const number = Number(line.match(/:(\d+):\d+\)?$/)?.[1]);
      const purpose = [2055, 4148, 4171, 4172].includes(number) ? 'combat'
        : [4705, 4710, 5273, 5280, 5287].includes(number) ? 'offMap' : null;
      assert.ok(purpose, `Unclassified pinned Source random site: ${line}`);
      const key = purpose === 'combat' ? 3n : 4n;
      const state = ((states.get(purpose) ?? mix(701n ^ ((key * gamma) & mask))) + gamma) & mask;
      states.set(purpose, state); counts.set(purpose, (counts.get(purpose) ?? 0) + 1);
      sites.set(`${purpose}:${number}`, (sites.get(`${purpose}:${number}`) ?? 0) + 1);
      return Number(mix(state) >> 11n) / 2**53;
    },
    projection() {
      return {weather: weatherState.toString(16).padStart(16, '0'), combat: states.has('combat') ? states.get('combat').toString(16).padStart(16, '0') : null, offMap: states.has('offMap') ? states.get('offMap').toString(16).padStart(16, '0') : null};
    },
    accounting() { return {counts: Object.fromEntries([...counts].sort()), sites: Object.fromEntries([...sites].sort())}; },
  };
}
const map = {name: 'BASE pending-drop retained arena', w: 80, h: 80, rows: Array(80).fill('.'.repeat(80)), spawns: [{x: 12, y: 40}, {x: 68, y: 40}], points: [{x: 40, y: 8}], defend: [0]};
const opts = {mode: 'conquest', weather: false, weatherSeed: 1, tech: false, supply: false, logistics: false, army: 'standard', sandbox: false};
const inputs = {names: ['a', 'b'], shuffle: false, teams: [0, 1], factions: [0, 1]};
const setup = {mpPerSeat: 10000, originalControls: {holdFire: true, holdPos: true, auto: false, autoRetreat: false}};
const scope = 'Complete selected pending-drop predicates on a retained admitted arena; original cleared-fixture pop==global-size is unequal here and requires independent correspondence review';
assert.equal(sim.validateMap(map), null);
assert.deepEqual(sim.SUPPORT.para.units, ['rifle', 'rifle', 'mg']);
assert.equal(sim.SUPPORT.para.delay, 6); assert.equal(sim.SUPPORT.cover.delay, 2);
const originalRandom = Math.random, cases = [];
try {
  for (const name of ['whole-stick-insufficient', 'pending-then-delivered', 'pending-then-intercepted']) {
    let constructorDraws = 0;
    Math.random = () => { constructorDraws++; return constructorDraw; };
    const g = sim.createGame(map, inputs.names, inputs.shuffle, inputs.teams, inputs.factions, opts);
    assert.equal(constructorDraws, 1);
    const originalIds = [...g.units.keys()]; assert.equal(originalIds.length, 10);
    const hq = [...g.units.values()].find(u => u.owner === 0 && u.type === 'hq'); assert.ok(hq);
    const protectedIdentity = () => originalIds.map(id => {
      const u = g.units.get(id); assert.ok(u, `Original actor ${id} was lost`);
      return [id, u.owner, u.type, sim.UNITS[u.type].structure || u.owner === 1 ? [bits(u.hp), bits(u.x), bits(u.z), u.cells ?? null, u.structureId ?? null] : null];
    });
    const originalIdentity = protectedIdentity();
    for (const u of g.units.values()) Object.assign(u, setup.originalControls);
    for (const p of g.players) p.mp = setup.mpPerSeat;
    rebuildGrid(g); sim.internals.updateVision(g);
    const rng = randomInputs(); Math.random = () => rng.next();
    const records = [], allEvents = [];
    let ticks = 0, commands = 0;
    const population = () => {
      const own = [...g.units.values()].filter(u => u.owner === 0);
      const field = own.filter(u => !sim.UNITS[u.type].structure).reduce((n, u) => n + sim.popUse(u.type), 0);
      const queued = own.filter(u => sim.UNITS[u.type].structure).reduce((n, u) => n + (u.queue ?? []).reduce((q, kind) => q + sim.popUse(kind), 0), 0);
      const reserved = g.strikes.filter(s => s.owner === 0 && !s.live).reduce((n, s) => n + sim.dropPop(s.kind), 0);
      return {field, queued, reserved, total: sim.popOf(g, 0), cap: sim.popCap(g, 0), globalCount: g.units.size};
    };
    const invariant = () => {
      assert.deepEqual(protectedIdentity(), originalIdentity);
      assert.equal(g.points.length, 1);
      const p = g.points[0], cell = 8 * g.w + 40;
      assert.deepEqual([p.x, p.z, p.owner, p.capper, p.progress, p.cut, !!p.contested, p.onPoint ?? []], [81, 17, -1, -1, 0, false, false, []]);
      assert.equal(p.kind, undefined); assert.equal(g.chars[cell], '.');
      assert.equal(g.buildingCells.has(cell), false); assert.equal(g.structuralCells.has(cell), false);
      assert.equal(g.winner, null);
      for (const player of g.players) { assert.equal(player.vp, 0); assert.equal(player.inc, sim.CFG.mpBase); }
      for (const u of g.units.values()) {
        assert.equal(u.logistics, undefined, 'Logistics stays disabled');
        assert.ok(Math.hypot(u.x - p.x, u.z - p.z) > 40);
        if (sim.UNITS[u.type].building) {
          assert.equal(g.structures.get(u.structureId)?.entityId, u.id);
          assert.deepEqual(g.structures.get(u.structureId).sections.map(s => s.c), u.cells);
          for (const c of u.cells) assert.equal(g.buildingCells.get(c)?.id, u.id);
          assert.deepEqual(u.queue, []); assert.equal((u.productionJobs ?? []).length, 0);
        }
      }
      for (const c of g.covers ?? []) assert.ok(Math.hypot(c.x - p.x, c.z - p.z) > c.r);
      for (const c of g.buildingCells.keys()) assert.ok(Math.hypot((c % g.w + .5) * sim.CELL - p.x, (Math.floor(c / g.w) + .5) * sim.CELL - p.z) > 40);
      const pop = population(); assert.equal(pop.total, pop.field + pop.queued + pop.reserved);
      assert.equal(pop.queued, 0); assert.equal(pop.cap, 12);
    };
    const snapshot = () => {
      invariant();
      return {
        tick: g.tick, nextId: g.nextId, seed: g.seed >>> 0,
        clearedFixtureEquality: sim.popOf(g, 0) === g.units.size,
        population: Object.fromEntries(Object.entries(population()).map(([key, value]) => [key, key === 'globalCount' ? value : bits(value)])),
        actors: [...g.units.values()].map(u => [u.id, u.owner, u.type, bits(u.hp), bits(u.x), bits(u.z), bits(u.rot), bits(u.aim), u.built === undefined ? null : bits(u.built), u.cells ?? null, u.structureId ?? null, u.path.map(point), bits(u.ghost ?? 0), !!u.holdPos, !!u.holdFire, !!u.auto, !!u.autoRetreat, point(u.worldGoal), u.drift === 'rally', bits(u.reinf)]),
        players: g.players.map(p => [bits(p.mp), bits(p.inc), bits(p.vp), point(p.spawn), point(p.rally), Object.entries(p.sup).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([kind, value]) => [kind, bits(value)])]),
        producers: [...g.units.values()].filter(u => sim.UNITS[u.type].building).map(u => [u.id, u.queue, bits(u.prog), point(u.rally)]),
        buildingCells: [...g.buildingCells].sort((a, b) => a[0] - b[0]).map(([c, b]) => [c, b.id, b.owner, bits(b.x), bits(b.z)]),
        strikes: g.strikes.map(s => [s.kind, s.owner, bits(s.x), bits(s.z), bits(s.dir), bits(s.t), bits(s.left), bits(s.next), s.live]),
        covers: (g.covers ?? []).map(c => [c.owner, c.team, bits(c.x), bits(c.z), bits(c.r), bits(c.t)]),
        random: rng.projection(),
      };
    };
    const shot = s => {
      if (s.k === 'para') return ['plane', 'para', bits(s.x), bits(s.z), bits(s.dir), s.fo];
      if (s.k === 'chutes') return ['chutes', bits(s.x), bits(s.z)];
      if (s.k === 'shotdown') return ['shotdown', s.kind, bits(s.x), bits(s.z), bits(s.dir), s.by === 'fighters'];
      assert.fail(`Unexpected effect in peaceful pending-drop scene: ${JSON.stringify(s)}`);
    };
    const initial = snapshot(); assert.equal(population().field, 3);
    const command = (label, slot, cmd, expected = null) => {
      const before = snapshot(), firstShot = g.shots.length;
      const result = sim.command(g, slot, cmd) ?? null; commands++;
      assert.equal(result, expected, `${name}:${label}`);
      assert.equal(g.shots.length, firstShot);
      const after = snapshot();
      if (expected === 'pop') assert.deepEqual(after, before, `${label}: denial preserves all projected state`);
      records.push({op: 'command', label, slot, cmd, result, events: [], expected: after});
    };
    const steps = (count, prefix) => {
      for (let i = 1; i <= count; i++) {
        const firstShot = g.shots.length; sim.step(g); ticks++;
        const events = g.shots.slice(firstShot).map(shot); allEvents.push(...events);
        records.push({op: 'tick', label: `${prefix}-${i}`, events, expected: snapshot()});
      }
    };
    const fill = name === 'whole-stick-insufficient' ? 7 : 6;
    for (let i = 1; i <= fill; i++) {
      const before = g.units.size, next = g.nextId, mp = g.players[0].mp;
      command(`fill-${i}`, 0, {t: 'buy', unit: 'rifle', from: hq.id});
      const born = g.units.get(next); assert.equal(g.units.size, before + 1);
      assert.ok(born); assert.equal(born.owner, 0); assert.equal(born.type, 'rifle');
      assert.equal(g.players[0].mp, mp - sim.UNITS.rifle.cost);
    }
    const count = g.units.size;
    assert.equal(population().field, name === 'whole-stick-insufficient' ? 10 : 9);
    if (name === 'whole-stick-insufficient') {
      command('whole-stick-denied', 0, {t: 'support', kind: 'para', x: 45, z: 81}, 'pop');
    } else {
      if (name === 'pending-then-intercepted') {
        const mp = g.players[1].mp;
        command('cover-accepted', 1, {t: 'support', kind: 'cover', x: 45, z: 81});
        assert.equal(g.players[1].mp, mp - sim.SUPPORT.cover.cost);
        steps(41, 'cover-tick');
        assert.equal(g.strikes.length, 0); assert.equal(g.covers.length, 1);
        assert.deepEqual([g.covers[0].owner, g.covers[0].team, g.covers[0].x, g.covers[0].z], [1, 1, 45, 81]);
      }
      const mp = g.players[0].mp;
      command('para-accepted', 0, {t: 'support', kind: 'para', x: 45, z: 81});
      assert.equal(g.players[0].mp, mp - sim.SUPPORT.para.cost);
      assert.equal(g.strikes.length, 1); assert.equal(g.strikes[0].live, false);
      assert.equal(g.units.size, count); assert.equal(population().reserved, 3); assert.equal(sim.popOf(g, 0), sim.popCap(g, 0));
      command('reserved-buy-denied', 0, {t: 'buy', unit: 'rifle', from: hq.id}, 'pop');
      const preDropIds = new Set(g.units.keys());
      steps(124, 'para-tick');
      const intercepted = name === 'pending-then-intercepted';
      assert.equal(g.units.size, count + (intercepted ? 0 : 3)); assert.equal(g.strikes.length, 0);
      assert.equal(population().reserved, 0); assert.equal(sim.popOf(g, 0), population().field + population().queued);
      assert.notEqual(sim.popOf(g, 0), g.units.size, 'Original cleared-fixture equality is deliberately unequal in the retained constructor scene');
      const births = [...g.units.values()].filter(u => !preDropIds.has(u.id));
      assert.deepEqual(births.map(u => [u.owner, u.type]), intercepted ? [] : [[0, 'rifle'], [0, 'rifle'], [0, 'mg']]);
      assert.equal(allEvents.filter(e => e[0] === 'plane').length, 1);
      assert.equal(allEvents.filter(e => e[0] === 'chutes').length, intercepted ? 0 : 3);
      assert.equal(allEvents.filter(e => e[0] === 'shotdown').length, intercepted ? 1 : 0);
      if (intercepted) {
        assert.equal(g.covers.length, 0);
        assert.equal(allEvents.find(e => e[0] === 'shotdown')[5], true);
        const before = g.players[0].mp, next = g.nextId;
        command('released-buy', 0, {t: 'buy', unit: 'rifle', from: hq.id});
        assert.equal(g.units.size, count + 1); assert.equal(g.units.get(next).type, 'rifle');
        assert.equal(g.players[0].mp, before - sim.UNITS.rifle.cost);
        assert.equal(sim.popOf(g, 0), sim.popCap(g, 0) - 3 + 1);
      } else assert.equal(sim.popOf(g, 0), sim.popCap(g, 0));
    }
    const final = snapshot();
    cases.push({name, originalIds, originalIdentity, hq: hq.id, constructorDraws, initial, records, final, ticks, commands, randomAccounting: rng.accounting()});
  }
  clean();
  assert.deepEqual(cases.map(c => [c.commands, c.ticks]), [[8, 0], [8, 124], [10, 165]]);
  const output = JSON.stringify({reference: pin, node: process.version, arch: process.arch, seed: 701, scope, map, opts, inputs, setup, cases}) + '\n';
  const target = resolve(root, 'rust/crates/simtest/tests/fixtures/base-pending-paratroopers.json');
  if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), output);
  else writeFileSync(target, output);
  console.log(`Verified ${cases.length} pending-drop scenes, ${cases.reduce((n, c) => n + c.commands, 0)} commands, ${cases.reduce((n, c) => n + c.ticks, 0)} complete Source ticks`);
} finally { Math.random = originalRandom; clean(); }
