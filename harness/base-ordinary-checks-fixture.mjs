// Selected BASE checks through original constructors, commands and complete steps.
// Original actor and structure identities follow their actual lifecycle in every scene.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync, writeFileSync} from 'node:fs';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
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
// Export the unchanged pure Source function; no constructor, command or step is replaced.
registerHooks({load(url, context, next) {
  const result = next(url, context);
  if (url !== pathToFileURL(resolve(reference, 'shared/sim.js')).href) return result;
  const source = String(result.source);
  assert.equal(source.split('const visionOf = (g, u) =>').length, 2);
  return {...result, source: source + '\nexport const baseCheckVisionOf = visionOf;\n'};
}});
const sim = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const {rebuildGrid} = await import(pathToFileURL(resolve(reference, 'shared/grid.js')));
const view = new DataView(new ArrayBuffer(8));
const bits = n => {
  view.setFloat64(0, n, false);
  return view.getBigUint64(0, false).toString(16).padStart(16, '0');
};
const mask = (1n << 64n) - 1n, gamma = 0x9e3779b97f4a7c15n;
const mix = x => {
  x = ((x ^ (x >> 30n)) * 0xbf58476d1ce4e5b9n) & mask;
  x = ((x ^ (x >> 27n)) * 0x94d049bb133111ebn) & mask;
  return x ^ (x >> 31n);
};
// Pair only the constructor bootstrap and the existing Combat stream inputs.
// Native MatchRandom is never reset or imported from a checkpoint.
const constructorDraw = Number(mix((mix(701n ^ ((2n * gamma) & mask)) + gamma) & mask) & 0xffffffffn) / 2**32;
function combatRandom() {
  let state = mix(701n ^ ((3n * gamma) & mask)), draws = 0;
  return {
    next() { state = (state + gamma) & mask; draws++; return Number(mix(state) >> 11n) / 2**53; },
    snapshot() { return draws ? state.toString(16).padStart(16, '0') : null; },
    draws() { return draws; },
  };
}
const map = {
  name: 'Authentic ordinary BASE acceptance', w: 80, h: 80,
  rows: Array(80).fill('.'.repeat(80)),
  // A remote neutral point makes this authored arena a valid native ordinary map.
  // The selected Source tests use no points. This is not their literal map replay.
  spawns: [{x: 12, y: 40}, {x: 68, y: 40}], points: [{x: 40, y: 8}], defend: [0],
};
assert.equal(sim.validateMap(map), null);
const opts = {mode: 'conquest', weather: false, weatherSeed: 1, tech: true, supply: false, logistics: false, army: 'standard', sandbox: false};
const inputs = {names: ['a', 'b'], shuffle: false, teams: [0, 1], factions: [0, 1]};
const setup = {allUnits: {holdFire: true, holdPos: true, auto: false, autoRetreat: false}, builder: {x: 46, z: 80}, player0MP: 10000};
const originalRandom = Math.random, cases = [];
try {
  function record(name, exercise) {
    let constructorDraws = 0;
    Math.random = () => { constructorDraws++; return constructorDraw; };
    const g = sim.createGame(map, inputs.names, inputs.shuffle, inputs.teams, inputs.factions, opts);
    assert.equal(constructorDraws, 1);
    const originalActors = [...g.units.keys()];
    const own = (owner, type) => [...g.units.values()].find(u => u.owner === owner && u.type === type);
    const aliases = {hq: own(0, 'hq').id, builder: own(0, 'rifle').id, target: own(1, 'rifle').id};
    const rng = combatRandom();
    Math.random = () => rng.next();
    const refresh = () => { rebuildGrid(g); sim.internals.updateVision(g); };
    for (const u of g.units.values()) Object.assign(u, setup.allUnits);
    g.players[0].mp = setup.player0MP;
    Object.assign(g.units.get(aliases.builder), setup.builder);
    refresh();
    const retired = [], records = [];
    let ticks = 0;
    const isolatedPoint = () => {
      assert.equal(g.points.length, 1);
      const p = g.points[0];
      assert.deepEqual([p.owner, p.capper, p.progress, p.cut, !!p.contested, p.onPoint ?? []], [-1, -1, 0, false, false, []]);
      assert.equal(p.kind, undefined);
      assert.deepEqual([p.x, p.z], [81, 17]);
      const cell = Math.floor(p.z / sim.CELL) * g.w + Math.floor(p.x / sim.CELL);
      assert.equal(g.chars[cell], '.');
      assert.equal(g.buildingCells.has(cell), false);
      assert.equal(g.structuralCells.has(cell), false);
      assert.equal(g.winner, null);
      for (const player of g.players) {
        assert.equal(player.vp, 0);
        assert.equal(player.inc, sim.CFG.mpBase * g.army.income);
      }
      for (const u of g.units.values()) assert.ok(Math.hypot(u.x - p.x, u.z - p.z) > 40);
      for (const c of g.buildingCells.keys()) assert.ok(Math.hypot((c % g.w + .5) * sim.CELL - p.x, (Math.floor(c / g.w) + .5) * sim.CELL - p.z) > 40);
    };
    const snapshot = () => {
      isolatedPoint();
      for (const u of g.units.values()) if (sim.UNITS[u.type].building) {
        assert.ok(u.cells?.length);
        assert.equal(g.structures.get(u.structureId)?.entityId, u.id);
        for (const c of u.cells) assert.equal(g.buildingCells.get(c)?.id, u.id);
      }
      return {
        tick: g.tick, nextId: g.nextId, seed: g.seed >>> 0,
        actors: [...g.units.values()].map(u => [u.id, u.owner, u.type, bits(u.hp), bits(u.x), bits(u.z), u.built === undefined ? null : bits(u.built), u.cells ?? null, u.structureId ?? null]),
        aliases: structuredClone(aliases),
        players: g.players.map(p => [bits(p.mp), p.tier, [...p.visible], bits(p.inc), bits(p.vp)]),
        point: g.points.map(p => [bits(p.x), bits(p.z), p.owner, p.capper, bits(p.progress), p.cut, !!p.contested, p.onPoint ?? []]),
        buildingCells: [...g.buildingCells].sort((a, b) => a[0] - b[0]).map(([c, b]) => [c, b.id, b.owner, bits(b.x), bits(b.z)]),
        recovery: [...g.units.values()].map(u => [u.id, bits(u.reinf)]),
        lookout: aliases.site && g.units.get(aliases.site)?.type === 'tower' ? bits(sim.baseCheckVisionOf(g, g.units.get(aliases.site))) : null,
        retired: structuredClone(retired), combatState: rng.snapshot(),
      };
    };
    const initial = snapshot();
    const command = (cmd, label, expect = null, as = null) => {
      const before = new Set(g.units.keys()), result = sim.command(g, 0, cmd) ?? null;
      assert.equal(result, expect, `${name}:${label}`);
      if (as) {
        const fresh = [...g.units.keys()].filter(id => !before.has(id));
        assert.equal(fresh.length, 1); aliases[as] = fresh[0];
      }
      records.push({op: 'command', cmd, label, expect, as, expected: snapshot()});
    };
    const prime = (id, fields, label) => {
      assert.ok(originalActors.includes(id), 'Only original mobile actors may receive authored poses/health');
      assert.ok(!sim.UNITS[g.units.get(id).type].building);
      Object.assign(g.units.get(id), fields); refresh();
      records.push({op: 'prime', id, fields, label, expected: snapshot()});
    };
    const steps = (count, label, condition = null, target = null, each = null) => {
      let n = 0;
      const done = () => condition === 'finished' ? g.units.get(target)?.built >= 1 : condition === 'tier2' ? g.players[0].tier === 2 : false;
      while (n < count && (!condition || !done())) {
        const before = [...g.units.keys()]; sim.step(g); n++; ticks++;
        isolatedPoint();
        for (const id of before) if (!g.units.has(id)) retired.push([g.tick, id]);
        if (each) each();
      }
      if (condition) assert.ok(done(), `${name}:${label} condition not reached within ${count} ticks`);
      records.push({op: 'steps', count: n, limit: count, label, condition, target, expected: snapshot()});
    };
    exercise({g, aliases, command, prime, steps});
    // No original actor can disappear except the explicitly scuttled own HQ.
    const missing = originalActors.filter(id => !g.units.has(id));
    assert.deepEqual(missing, name === 'hq-retirement-rebuild-and-engineer-denial' ? [aliases.hq] : []);
    cases.push({name, originalActors, initialAliases: initial.aliases, initial, records, ticks, combatDraws: rng.draws()});
  }
  record('hq-retirement-rebuild-and-engineer-denial', ({g, aliases: a, command, prime, steps}) => {
    const hq = g.units.get(a.hq), oldCells = [...hq.cells];
    const count = g.units.size, mp = g.players[0].mp;
    command({t: 'buy', unit: 'engineer'}, 'engineer-denied', 'needs');
    assert.equal(g.units.size, count); assert.equal(g.players[0].mp, mp);
    prime(a.builder, {x: hq.x + 5, z: hq.z, hp: sim.UNITS.rifle.hpPer}, 'injured-at-live-hq');
    steps(40, 'live-hq-positive-control');
    assert.ok(g.units.get(a.builder).hp > sim.UNITS.rifle.hpPer);
    prime(a.builder, {hp: sim.UNITS.rifle.hpPer}, 'injured-before-scuttle');
    command({t: 'destroy', ids: [a.hq]}, 'scuttle-original-hq');
    steps(1, 'hq-retired');
    assert.equal(g.units.has(a.hq), false);
    assert.ok(oldCells.every(c => g.chars[c] === 'R'));
    const hp = g.units.get(a.builder).hp;
    steps(60, 'fresh-post-retirement-sixty-ticks', null, null, () => assert.equal(g.units.get(a.builder).hp, hp));
    const at = sim.siteNear(g, hq.x, hq.z, 3); assert.ok(at);
    const before = g.players[0].mp;
    command({t: 'build', kind: 'hq', ids: [a.builder], ...at}, 'hq-rebuilt', null, 'site');
    const site = g.units.get(a.site);
    assert.notEqual(site.id, a.hq); assert.equal(site.cells.length, 9);
    assert.equal(g.players[0].mp, before - 200);
    assert.equal(site.built, 0); assert.equal(site.hp, 3000 * .25);
    assert.equal(g.units.get(a.builder).build, site.id);
  });
  record('tier-one-tower-twelve-and-sixty-metre-vision', ({g, aliases: a, command, prime, steps}) => {
    assert.equal(g.players[0].tier, 1);
    command({t: 'build', kind: 'tower', ids: [a.builder], x: 50, z: 80}, 't1-tower-built', null, 'site');
    const tower = g.units.get(a.site);
    assert.equal(tower.cells.length, 1); assert.equal(sim.UNITS.tower.w, null);
    command({t: 'stop', ids: [a.builder]}, 'pause-tower');
    prime(a.target, {x: tower.x + 55, z: tower.z}, 'enemy-fifty-five-metres');
    steps(20, 'unfinished-vision');
    assert.equal(sim.baseCheckVisionOf(g, tower), 12);
    assert.equal(g.players[0].visible.has(a.target), false);
    command({t: 'assist', ids: [a.builder], id: a.site}, 'resume-tower');
    steps(1600, 'tower-completed', 'finished', a.site);
    steps(20, 'finished-vision');
    assert.equal(sim.baseCheckVisionOf(g, tower), 60);
    assert.equal(g.players[0].visible.has(a.target), true);
  });
  record('unfinished-and-finished-pillbox-fire', ({g, aliases: a, command, prime, steps}) => {
    command({t: 'build', kind: 'pillbox', ids: [a.builder], x: 50, z: 80}, 't1-pillbox-denied', 'tier');
    command({t: 'tech', kind: 'tier'}, 'research-company-hq');
    steps(1600, 'company-hq-completed', 'tier2');
    command({t: 'build', kind: 'pillbox', ids: [a.builder], x: 50, z: 80}, 'pillbox-built', null, 'site');
    const box = g.units.get(a.site);
    command({t: 'stop', ids: [a.builder]}, 'pause-pillbox');
    prime(a.target, {x: box.x + 22, z: box.z}, 'enemy-in-front');
    const hp = g.units.get(a.target).hp;
    steps(80, 'unfinished-no-fire', null, null, () => assert.equal(g.units.get(a.target).hp, hp));
    assert.ok(box.built < 1);
    command({t: 'assist', ids: [a.builder], id: a.site}, 'resume-pillbox');
    steps(1600, 'pillbox-completed', 'finished', a.site);
    const completedHp = g.units.get(a.target).hp;
    steps(120, 'finished-damages-target');
    assert.ok(g.units.get(a.target).hp < completedHp);
  });
  clean();
  const scope = 'Selected predicates on an admitted ordinary arena; not a literal replay of the zero-point Source maps';
  const output = JSON.stringify({reference: pin, node: process.version, arch: process.arch, seed: 701, scope, map, opts, inputs, setup, cases}) + '\n';
  const target = resolve(root, 'rust/crates/simtest/tests/fixtures/base-ordinary-checks.json');
  if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), output);
  else writeFileSync(target, output);
  console.log(`Verified ${cases.length} BASE scenes, ${cases.reduce((n, c) => n + c.ticks, 0)} complete Source ticks`);
} finally { Math.random = originalRandom; clean(); }
