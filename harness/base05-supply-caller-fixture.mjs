// Genuine point capture, enemy movement, destruction and paid fill through the original runner.
// Source-only preparation. Root must grant this frozen recorder before execution.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {gzipSync, gunzipSync} from 'node:zlib';

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
const original = readFileSync(resolve(reference, 'test.js'), 'utf8');
for (const literal of ['a blown bridge cuts it', 'an enemy fighting for the point itself does not cut it', 'a street choked with rubble cuts the point']) {
  assert.ok(original.includes(literal));
}
const view = new DataView(new ArrayBuffer(8));
const bits = value => {
  assert.ok(Number.isFinite(value));
  view.setFloat64(0, value, false);
  return view.getBigUint64(0, false).toString(16).padStart(16, '0');
};
const numberBits = value => typeof value === 'number' ? bits(value)
  : Array.isArray(value) ? value.map(numberBits) : value;
const mask = (1n << 64n) - 1n, gamma = 0x9e3779b97f4a7c15n;
const mix = value => {
  value = ((value ^ (value >> 30n)) * 0xbf58476d1ce4e5b9n) & mask;
  value = ((value ^ (value >> 27n)) * 0x94d049bb133111ebn) & mask;
  return value ^ (value >> 31n);
};
const bootstrap = Number(mix((mix(701n ^ ((2n * gamma) & mask)) + gamma) & mask) & 0xffffffffn) / 2 ** 32;
const crossing = [39, 40, 41].map(x => 40 * 80 + x);
const houses = [39, 40, 41].map(x => 39 * 80 + x);
const local = Array.from({length: 7}, (_, y) => Array.from({length: 9}, (_, x) => (y + 36) * 80 + x + 36)).flat();
const cases = [], prior = Math.random;
let active = null, liveGame = null;
try {
  for (const [name, rubble, supply] of [['bridge-on', false, true], ['bridge-off', false, false], ['rubble-on', true, true]]) {
    const rows = Array.from({length: 80}, (_, y) => '.'.repeat(39)
      + (y === 40 ? (rubble ? 'DDD' : '===') : rubble && y === 39 ? 'BBB' : 'WWW') + '.'.repeat(38));
    const map = {name: `BASE05 ${name}`, w: 80, h: 80, rows,
      spawns: [{x: 12, y: 40}, {x: 68, y: 40}], points: [{x: 52, y: 40, mp: 1, vp: 1}], defend: [0]};
    assert.equal(sim.validateMap(map), null);
    let draws = 0;
    Math.random = () => { draws++; return bootstrap; };
    const g = sim.createGame(map, ['a', 'b'], false, [0, 1], [0, 1], {
      mode: 'conquest', weather: false, weatherSeed: 1, tech: false, supply,
      logistics: false, army: 'standard', sandbox: false,
    });
    liveGame = g;
    assert.equal(draws, 1);
    Math.random = () => { throw Error(`${name}: unexpected global random consumer`); };
    const initialActors = [...g.units.values()].map(u => [u.id, u.owner, u.type, bits(u.hp), bits(u.x), bits(u.z)]);
    const crew = [...g.units.values()].find(u => u.owner === 0 && u.type === 'rifle');
    const enemy = [...g.units.values()].find(u => u.owner === 1 && u.type === 'rifle');
    assert.ok(crew && enemy, 'select two actual constructor rifles');
    assert.equal(initialActors.length, 10);
    const records = [], p = g.points[0];
    active = {name, map, supply, initialActors, aliases: [crew.id, enemy.id], records};
    const retained = () => {
      assert.equal(g.units.size, initialActors.length);
      for (const row of initialActors) {
        const u = g.units.get(row[0]);
        assert.ok(u);
        assert.deepEqual([u.id, u.owner, u.type, bits(u.hp)], row.slice(0, 4));
        if (sim.UNITS[u.type].building) {
          assert.equal(g.structures.get(u.structureId)?.entityId, u.id);
          for (const c of u.cells) assert.equal(g.buildingCells.get(c)?.id, u.id);
        }
      }
      assert.equal(g.winner, null);
    };
    const projection = () => {
      retained();
      return {
        tick: g.tick, seed: g.seed >>> 0,
        actors: [...g.units.values()].map(u => [u.id, u.owner, u.type, bits(u.hp), bits(u.x), bits(u.z)]),
        wallets: g.players.map(pl => [bits(pl.mp), bits(pl.inc), bits(pl.vp)]),
        point: [p.owner, p.capper, bits(p.progress), p.cut, !!p.contested, p.onPoint ?? []],
        recipient: numberBits(sim.snapshotFor(g, 0, []).points),
        terrain: local.map(c => [c, g.chars[c], g.ground[c], g.objects[c], g.flags[c], new Uint32Array(g.cellHp.buffer)[c], g.cellState[c]]),
      };
    };
    active.initial = projection();
    const command = (owner, raw, label) => {
      const before = g.players[owner].mp;
      assert.equal(sim.command(g, owner, raw), undefined, `${name}:${label}`);
      records.push({op: 'command', owner, raw, label, debit: bits(before - g.players[owner].mp), expected: projection()});
    };
    const move = (u, x, z, label) => command(u.owner, {t: 'move', orders: [[u.id, x, z]]}, label);
    const steps = (limit, label, condition = null, unit = null) => {
      const frames = [];
      // Attach the live frame list before advancing so the first failure keeps its history.
      records.push({op: 'steps', label, limit, condition, unit: unit?.id ?? null, frames});
      const done = () => condition === 'captured' ? p.owner === 0 && p.progress === 1
        : condition === 'arrived' ? unit.path.length === 0 && !unit.worldGoal
          : condition === 'filled' ? crossing.every(c => g.chars[c] === 'D') : false;
      while (frames.length < limit && (!condition || !done())) {
        sim.step(g);
        frames.push(projection());
      }
      if (condition) assert.ok(done(), `${name}:${label} failed within ${limit} ticks`);
    };
    const observe = (label, cut) => {
      assert.equal(p.owner, 0, `${name}:${label}: genuine capture retained`);
      assert.equal(p.cut, cut, `${name}:${label}: point supply`);
      assert.equal(sim.snapshotFor(g, 0, []).points[0][4], Number(cut));
      const vp = g.players[0].vp;
      const mp = g.players[0].mp;
      steps(20, `${label}-income-window`);
      assert.equal(p.cut, cut);
      assert.equal(g.players[0].inc, cut ? 4 : 5);
      if (cut) {
        assert.equal(g.players[0].vp, vp, `${name}:${label}: cut point pays no VP`);
        assert.ok(Math.abs((g.players[0].mp - mp) - 4) < 1e-9);
      } else {
        assert.ok(g.players[0].vp > vp);
        assert.ok(Math.abs((g.players[0].mp - mp) - 5) < 1e-9);
      }
      records.push({op: 'observe', label, cut, expected: projection()});
    };
    for (const owner of [0, 1]) {
      const ids = [...g.units.values()].filter(u => u.owner === owner).map(u => u.id);
      for (const [key, on] of [['holdFire', true], ['holdPos', true], ['autoRetreat', false]]) {
        command(owner, {t: 'stance', ids, key, on}, `quiet-${owner}-${key}`);
      }
      command(owner, {t: 'autocast', ids, on: false}, `quiet-${owner}-autocast`);
    }
    if (rubble) {
      for (const cell of houses) assert.equal(sim.mutateWorldCell(g, cell, {ground: 'W'}), true);
      records.push({op: 'ground', cells: houses, expected: projection()});
    }
    move(crew, p.x, p.z, 'walk-and-capture');
    steps(900, 'actual-point-capture', 'captured');
    move(crew, 65, 81, 'leave-point-and-crossing-clear');
    steps(600, 'crew-withdrawal', 'arrived', crew);
    steps(45, 'initial-supply-beat');
    observe('bridge-or-road-supplies-owned-point', false);
    if (!rubble) {
      move(enemy, 81, 81, 'enemy-walk-to-bridge');
      steps(900, 'enemy-bridge-arrival', 'arrived', enemy);
      steps(45, 'enemy-bridge-supply-beat');
      observe('enemy-on-bridge', supply);
      move(enemy, p.x, p.z - 12, 'enemy-walk-to-held-point-free-zone');
      steps(600, 'enemy-point-arrival', 'arrived', enemy);
      assert.ok(Math.hypot(enemy.x - p.x, enemy.z - p.z) <= 16);
      assert.ok(Math.hypot(enemy.x - p.x, enemy.z - p.z) > 8);
      steps(45, 'point-exemption-beat');
      observe('held-point-exempts-near-enemy', false);
    }
    const blast = rubble ? {x: 81, z: 75.9, radius: 3, damage: 2000, scar: 0}
      : {x: 81, z: 81, radius: 5, damage: 2000, scar: 0};
    sim.damageCells(g, [...g.units.values()], {x: blast.x, z: blast.z}, blast.radius, blast.damage, blast.scar);
    records.push({op: 'blast', blast, expected: projection()});
    steps(52, 'normal-debris-settlement');
    if (rubble) {
      assert.equal(g.chars[crossing[1]], 'R', 'a falling house puts rubble on the road');
      assert.ok(g.flags[crossing[1]] & sim.VBLOCK);
    } else {
      for (const cell of crossing) assert.equal(g.chars[cell], 'W');
    }
    steps(45, 'destroyed-crossing-supply-beat');
    observe(rubble ? 'settled-rubble-cuts-point' : 'blown-bridge-cuts-point', supply);
    if (rubble) {
      move(crew, 73, 81, 'walk-to-rubble-work');
      steps(600, 'crew-at-rubble', 'arrived', crew);
      command(0, {t: 'dig', ids: [crew.id], kind: 'fill', x: 81, z: 81, dir: 0}, 'paid-real-fill');
      const paid = records.at(-1).debit;
      assert.ok(BigInt(`0x${paid}`) > 0n);
      steps(1200, 'normal-paid-fill-completion', 'filled');
      steps(45, 'cleared-road-supply-beat');
      observe('cleared-road-restores-point', false);
    }
    active.ticks = g.tick;
    active.commands = records.filter(row => row.op === 'command').length;
    cases.push(active);
    console.error(JSON.stringify({complete: name, ticks: g.tick, commands: active.commands}));
    active = null;
    liveGame = null;
  }
  clean();
  const data = JSON.stringify({reference: pin, node: process.version, arch: process.arch, cases}) + '\n';
  const path = resolve(root, 'rust/crates/simtest/tests/fixtures/base05-supply-caller.json.gz');
  if (process.argv.includes('--check')) assert.equal(gunzipSync(readFileSync(path)).toString(), data);
  else writeFileSync(path, gzipSync(data, {level: 9}));
  console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} ${cases.length} BASE05 ordinary caller histories`);
} catch (error) {
  const failureBits = value => {
    view.setFloat64(0, value, false);
    return view.getBigUint64(0, false).toString(16).padStart(16, '0');
  };
  if (process.env.BASE05_FAILURE_PATH) writeFileSync(process.env.BASE05_FAILURE_PATH,
    JSON.stringify({error: String(error), completed: cases, active,
      lastObserved: liveGame ? {
        tick: liveGame.tick,
        actors: [...liveGame.units.values()].map(u => [u.id, u.owner, u.type,
          failureBits(u.hp), failureBits(u.x), failureBits(u.z)]),
        points: liveGame.points.map(p => [p.owner, p.capper, failureBits(p.progress), p.cut]),
      } : null,
    }) + '\n', {flag: 'wx'});
  throw error;
} finally {
  Math.random = prior;
}
