// Original components and real Move inputs for the LOG24 causal grace/cut witness.
// Source-only until Root grants this exact recorder and its frozen inputs a slot.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync, writeFileSync} from 'node:fs';
import {registerHooks} from 'node:module';
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
const simUrl = pathToFileURL(resolve(reference, 'shared/sim.js')).href;
registerHooks({load(url, context, next) {
  const loaded = next(url, context);
  return url === simUrl ? {...loaded, source: String(loaded.source) + '\nexport {supplyHooks};\n'} : loaded;
}});
const sim = await import(simUrl);
const selectedTest = readFileSync(resolve(reference, 'test-supply-convoys.mjs'), 'utf8');
assert.ok(selectedTest.includes('an enemy beside the troop starts the grace countdown'));
assert.ok(selectedTest.includes('enemy zone of control cuts it off once the grace runs out'));
const view = new DataView(new ArrayBuffer(8));
const bits = value => {
  assert.ok(Number.isFinite(value));
  view.setFloat64(0, value, false);
  return view.getBigUint64(0, false).toString(16).padStart(16, '0');
};
const mask = (1n << 64n) - 1n, gamma = 0x9e3779b97f4a7c15n;
const mix = value => {
  value = ((value ^ (value >> 30n)) * 0xbf58476d1ce4e5b9n) & mask;
  value = ((value ^ (value >> 27n)) * 0x94d049bb133111ebn) & mask;
  return value ^ (value >> 31n);
};
const weatherState = (mix(701n ^ ((2n * gamma) & mask)) + gamma) & mask;
const constructorDraw = Number(mix(weatherState) & 0xffffffffn) / 2 ** 32;
const map = {
  name: 'LOG24 retained-constructor causal arena', w: 80, h: 80,
  rows: Array(80).fill('.'.repeat(80)),
  spawns: [{x: 12, y: 40}, {x: 68, y: 40}],
  points: [{x: 40, y: 8, vp: 0, mp: 0}], defend: [0],
};
const opts = {
  mode: 'conquest', weather: false, weatherSeed: 1, tech: false,
  supply: true, logistics: true, army: 'standard', sandbox: false,
};
assert.equal(sim.validateMap(map), null);
const prior = Math.random;
let constructorDraws = 0;
let active = null, liveGame = null;
try {
  Math.random = () => { constructorDraws++; return constructorDraw; };
  const g = sim.createGame(map, ['a', 'b'], false, [0, 1], [0, 1], opts);
  liveGame = g;
  assert.equal(constructorDraws, 1);
  // Continue the initialized match without replacing any engine RNG state.
  Math.random = () => { throw Error('Unexpected post-constructor random consumer in peaceful LOG24 scene'); };
  const actors = () => [...g.units.values()].map(u => [u.id, u.owner, u.type, bits(u.hp), bits(u.x), bits(u.z)]);
  const troop = [...g.units.values()].find(u => u.owner === 0 && u.type === 'rifle');
  const enemy = [...g.units.values()].find(u => u.owner === 1 && u.type === 'rifle');
  assert.ok(troop && enemy, 'select two actual constructor rifles');
  const initialEnemy = {x: enemy.x, z: enemy.z};
  const terrain = [...g.chars];
  assert.equal(g.units.size, 10);
  const result = {reference: pin, map, aliases: [troop.id, enemy.id], initialActors: actors()};
  active = result;
  const target = {x: troop.x, z: troop.z - 6};
  const targetCell = Math.floor(target.z / 2) * g.w + Math.floor(target.x / 2);
  assert.ok(targetCell >= 0 && targetCell < g.w * g.h);
  assert.equal(g.flags[targetCell] & sim.MOVE, 0, 'declared Move endpoint is actual open ground');
  assert.equal(g.buildingCells.has(targetCell), false, 'retain actual blocking HQ footprints');
  result.inputReady = {
    movement: {owner: 1, unit: enemy.id, target: [bits(target.x), bits(target.z)]},
    targetCell: {cell: targetCell, ch: g.chars[targetCell], flags: g.flags[targetCell], footprint: false},
    buildingFootprints: [...g.units.values()].filter(u => sim.UNITS[u.type].building).map(u => [u.id, [...u.cells]]),
  };
  const commands = [];
  result.commands = commands;
  const command = (owner, raw) => {
    assert.equal(sim.command(g, owner, raw), undefined);
    commands.push({owner, raw});
  };
  for (const owner of [0, 1]) {
    const ids = [...g.units.values()].filter(u => u.owner === owner).map(u => u.id);
    for (const [key, on] of [['holdFire', true], ['holdPos', true], ['autoRetreat', false]]) {
      command(owner, {t: 'stance', ids, key, on});
    }
    command(owner, {t: 'autocast', ids, on: false});
  }
  const projection = () => {
    const row = sim.snapshotFor(g, 0, []).logistics.units.find(row => row.id === troop.id);
    assert.ok(!sim.snapshotFor(g, 1, []).logistics.units.some(row => row.id === troop.id));
    return {
      rate: bits(sim.supplyHooks(g).supplyRate(0, troop)),
      cutFor: bits(troop.logistics.cutFor), cachedRate: bits(troop.logistics.supplyRate),
      recipient: {id: row.id, supply: bits(row.supply), cut: row.cut, grace: row.grace},
    };
  };
  result.initial = projection();
  assert.ok(sim.supplyHooks(g).supplyRate(0, troop) > 0);
  assert.equal(troop.logistics.cutFor, 0);
  command(1, {t: 'move', orders: [[enemy.id, target.x, target.z]]});
  let firstZero = null;
  result.causalTrace = [];
  for (let n = 0; n < 1140; n++) {
    sim.step(g);
    const rate = sim.supplyHooks(g).supplyRate(0, troop);
    if (firstZero === null && rate === 0) {
      firstZero = g.tick;
      assert.ok(troop.logistics.cutFor > 0);
      assert.notDeepEqual({x: enemy.x, z: enemy.z}, initialEnemy);
      assert.ok(Math.hypot(enemy.x - troop.x, enemy.z - troop.z) <= 8);
      const cell = Math.floor(troop.z / 2) * g.w + Math.floor(troop.x / 2);
      assert.notEqual(g.supplyNet[0].closed[cell], 0);
    }
    result.causalTrace.push({tick: g.tick, enemy: [bits(enemy.x), bits(enemy.z)], selected: projection()});
    if (firstZero !== null) {
      const elapsed = g.tick - firstZero;
      assert.equal(rate, 0, 'enemy must keep the real network closed');
      if (elapsed === 40) {
        result.afterTwoSeconds = projection();
        assert.ok(result.afterTwoSeconds.recipient.grace > 0);
        assert.equal(result.afterTwoSeconds.recipient.cut, false);
      }
      if (elapsed === 240) {
        result.afterTwelveSeconds = projection();
        assert.equal(result.afterTwelveSeconds.recipient.cut, true);
        break;
      }
    }
  }
  assert.notEqual(firstZero, null, 'real Move must bring an armed enemy within closure range');
  assert.ok(result.afterTwoSeconds && result.afterTwelveSeconds);
  assert.deepEqual([...g.chars], terrain);
  const finalActors = actors();
  assert.equal(finalActors.length, result.initialActors.length);
  for (let i = 0; i < finalActors.length; i++) {
    assert.deepEqual(finalActors[i].slice(0, 4), result.initialActors[i].slice(0, 4));
    if (finalActors[i][0] !== enemy.id) assert.deepEqual(finalActors[i], result.initialActors[i]);
  }
  assert.ok([...g.units.values()].every(u => u.type !== 'truck'));
  result.commands = commands;
  result.firstZero = firstZero;
  result.ticks = g.tick;
  clean();
  const path = resolve(root, 'rust/crates/core/tests/fixtures/log24-causal-history.json');
  const data = JSON.stringify(result) + '\n';
  if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), data);
  else writeFileSync(path, data);
  console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} LOG24 causal history: ${g.tick} ticks, ${commands.length} commands`);
} catch (error) {
  if (process.env.LOG24_FAILURE_PATH) writeFileSync(process.env.LOG24_FAILURE_PATH,
    JSON.stringify({
      error: String(error), constructorDraws, active,
      lastObserved: liveGame ? {
        tick: liveGame.tick,
        actors: [...liveGame.units.values()].map(u => [u.id, u.owner, u.type, bits(u.hp), bits(u.x), bits(u.z)]),
        reserves: [...liveGame.units.values()].filter(u => u.logistics).map(u => [u.id, u.logistics]),
      } : null,
    }) + '\n', {flag: 'wx'});
  throw error;
} finally {
  Math.random = prior;
}
