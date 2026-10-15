// Pure production component inputs, not a Classic Match admission or runtime trace.
// Keep the extracted Source buy and production functions unchanged.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync, realpathSync, writeFileSync} from 'node:fs';
import {dirname, isAbsolute, relative, resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
assert.ok(process.argv.slice(2).every(arg => ['--check', '--controls'].includes(arg)));
assert.equal(new Set(process.argv.slice(2)).size, process.argv.slice(2).length);
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], {encoding: 'utf8'}).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const tech = await import(pathToFileURL(`${reference}/shared/tech.js`));
const {availability, buyCount} = await import(pathToFileURL(`${reference}/client/availability.js`));
const source = readFileSync(`${reference}/shared/sim.js`, 'utf8');
const section = (start, end) => {
  const i = source.indexOf(start), j = source.indexOf(end, i);
  assert.ok(i >= 0 && j > i);
  return source.slice(i, j);
};
const helpers = section('function productionJobs(g, b)', 'function produceUnit(g, b, owner, type)');
const buy = section("  } else if (cmd.t === 'buy' &&", '\n}\n\n// ---------- simulation').replace(/^  } else /, '');
const names = ['UNITS', 'TECH', 'techCost', 'tierOf', 'canBuild', 'isConstructionMode',
  'isSkirmishBaseMode', 'productionAccess', 'productionBuildings', 'popOf', 'popCap', 'popUse', 'priceOf'];
const factory = new Function(...names, `${helpers}
function tally(){}
function produceUnit(){throw Error('Pure paid queue witness must not spawn');}
function command(g,slot,cmd){${buy}}
return {command,productionRow};`);
const fn = factory(...names.map(name => sim[name] ?? tech[name]));
const bits = value => {
  assert.ok(Number.isFinite(value));
  const data = Buffer.alloc(8);
  data.writeDoubleBE(value);
  return data.toString('hex');
};
async function recordPaidPrice() {
const input = {
  mode: {kind: 'classic'}, army: 'standard', tech: false, naval: false,
  players: [{slot: 0, team: 0, faction: 0, tier: 1, armory: {inf: 0, guns: 0, armor: 0}, lab: [], mp: 301, mun: 0, fuel: 0}],
  units: [{id: 1, owner: 0, type: 'hq', hp: 1000, built: 1, queue: [], productionJobs: [], prog: 0}],
  strikes: [], regions: [],
};
const g = {...structuredClone(input), army: sim.CFG.armies.standard,
  units: new Map(input.units.map(unit => [unit.id, structuredClone(unit)]))};
const producer = g.units.get(1), payer = g.players[0];
const action = {t: 'buy', unit: 'rifle', from: producer.id, slot: 0};
const price = sim.priceOf(g, 'rifle');
assert.deepEqual(price, {mp: 100, fuel: 0});
assert.equal(payer.mp, price.mp * 3 + 1);
const count = () => {
  const fields = sim.UNIT_ROW, row = [];
  for (const [name, value] of Object.entries({id: 1, type: 'hq', owner: 0, hp: producer.hp, built: 1, x: 0, z: 0, cd: 0, flags: 0})) row[fields[name]] = value;
  // This is a declared component projection of the live component queue and wallet.
  return buyCount({mode: g.mode, army: g.army, mp: payer.mp, mun: payer.mun, fuel: payer.fuel,
    units: [row], queues: [[producer.id, 0, 0, producer.prog, ...producer.queue]]}, sim.CFG, action, 5);
};
const state = () => ({next: g.nextProductionJob ?? 0,
  money: [payer.mp, payer.mun, payer.fuel].map(bits), progress: bits(producer.prog),
  jobs: fn.productionRow(producer)[1].map(job => [job.id, job.type, bits(job.mp), bits(job.fuel), job.status])});
const result = {reference: pin, input, price, initial: state(), initialCount: count(), trace: []};
assert.equal(result.initialCount, 3);
const prior = Math.random;
try {
  Math.random = () => { throw Error('Paid production component must not draw random values'); };
  for (let index = 0; index < 4; index++) {
    const before = state();
    const denial = fn.command(g, 0, action) ?? null;
    const expected = state();
    result.trace.push({action: {slot: 0, kind: 'rifle', from: 1}, result: denial, expected, remainingCount: count()});
    assert.equal(denial, index < 3 ? null : 'mp');
    assert.equal(payer.mp, index < 3 ? 301 - 100 * (index + 1) : 1);
    assert.equal(producer.queue.length, Math.min(index + 1, 3));
    assert.equal(result.trace.at(-1).remainingCount, Math.max(2 - index, 0));
    if (index === 3) assert.deepEqual(expected, before, 'MP refusal is atomic');
  }
  clean();
  const file = `${root}/rust/crates/core/tests/fixtures/base13-paid-price.json`;
  const data = JSON.stringify(result) + '\n';
  if (process.argv.includes('--check')) assert.equal(readFileSync(file, 'utf8'), data);
  else writeFileSync(file, data);
  console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} BASE13: 1 component trace, 4 buy calls, 3 paid jobs, 5 count observations, 0 ticks, 0 draws`);
} catch (error) {
  if (process.env.BASE13_FAILURE_PATH) writeFileSync(process.env.BASE13_FAILURE_PATH,
    JSON.stringify({error: String(error), observed: result}) + '\n', {flag: 'wx'});
  throw error;
} finally {
  Math.random = prior;
}

}

// These are independent authored production inputs, not constructor snapshots.
// In particular, queue-room starts funded instead of resetting a live wallet.
function controlInput(makers, fuel = 0) {
  return {
    mode: {kind: 'classic', suddenDeath: false}, army: 'standard', tech: false, naval: false,
    players: [{slot: 0, team: 0, faction: 0, tier: 1, armory: {inf: 0, guns: 0, armor: 0}, lab: [], mp: 10000, mun: 0, fuel}],
    units: makers.map((type, index) => ({id: index + 1, owner: 0, type, hp: 1000, built: 1,
      x: 0, z: 0, cd: 0, flags: 0, queue: [], productionJobs: [], prog: 0})),
    strikes: [], regions: [], nextProductionJob: 0,
  };
}
function recordControls() {
  const file = process.env.BASE13_CONTROLS_OUTPUT;
  assert.ok(file && isAbsolute(file), 'Controls require an explicit absolute external output');
  const outside = relative(realpathSync(root), realpathSync(dirname(file)) + '/');
  assert.ok(outside.startsWith('../'), 'Controls output must stay outside the repository');
  const action = (unit, from) => ({t: 'buy', unit, slot: 0, ...(from === undefined ? {} : {from})});
  const rifle = action('rifle', 1), anyRifle = action('rifle'), second = action('rifle', 2);
  const mg = action('mg'), tank = action('tank', 1);
  const yes = {ok: true, reason: ''}, no = reason => ({ok: false, reason});
  const observe = (action, count, card) => ({op: 'observe', action, count, card});
  const buy = (action, result = null) => ({op: 'buy', action, result});
  const plans = [
    {name: 'queue-room', input: controlInput(['hq']), steps: [
      observe(rifle, 5, yes), buy(rifle), buy(rifle), buy(rifle), observe(rifle, 2, yes),
      buy(rifle), buy(rifle), observe(rifle, 0, no('The training queue is full')), buy(rifle, 'queueFull')]},
    {name: 'no-barracks', input: controlInput(['hq']), steps: [
      observe(mg, 0, no('Needs a Barracks')), buy(mg, 'needs')]},
    {name: 'selected-full', input: controlInput(['hq', 'hq']), steps: [
      buy(rifle), buy(rifle), buy(rifle), buy(rifle), buy(rifle),
      observe(anyRifle, 5, yes), observe(second, 5, yes),
      observe(rifle, 0, no('The training queue is full')), buy(rifle, 'queueFull'),
      buy(anyRifle), buy(second)]},
    {name: 'tank-fuel-low', input: controlInput(['motorpool'], 10), steps: [
      observe(tank, 0, no('Needs 60 fuel')), buy(tank, 'fuel')]},
    {name: 'tank-fuel-paid', input: controlInput(['motorpool'], 100), steps: [
      observe(tank, 1, yes), buy(tank), observe(tank, 0, no('Needs 60 fuel')), buy(tank, 'fuel')]},
  ];
  // Preserve absence separately from every present value. These are read receipts,
  // not a normalization policy and never feed the state or either comparison.
  const field = (object, key) => Object.hasOwn(object, key)
    ? {present: true, value: object[key], ...(typeof object[key] === 'number' ? {bits: bits(object[key])} : {})}
    : {present: false};
  const result = {reference: pin, schema: 'base13-paid-controls-v1',
    inputPolicy: 'independent authored inputs; no live rewrites; strict present values; no comparison mappings',
    ruleReads: ['rifle', 'mg', 'tank'].map(unit => ({unit,
      fields: Object.fromEntries(['cost', 'classicCost', 'fuel', 'max', 'pop'].map(key => [key, field(sim.UNITS[unit], key)]))})),
    scenarios: []};
  const prior = Math.random;
  try {
    Math.random = () => { throw Error('Paid controls must not draw random values'); };
    for (const plan of plans) {
      const input = plan.input;
      const g = {...structuredClone(input), army: sim.CFG.armies.standard,
        units: new Map(input.units.map(unit => [unit.id, structuredClone(unit)]))};
      const payer = g.players[0];
      const state = () => ({next: g.nextProductionJob,
        money: [payer.mp, payer.mun, payer.fuel].map(bits),
        buildings: [...g.units.values()].map(unit => ({id: unit.id, progress: bits(unit.prog),
          queue: [...unit.queue], jobs: fn.productionRow(unit)[1].map(job =>
            [job.id, job.type, bits(job.mp), bits(job.fuel), job.status])}))});
      const view = () => ({mode: g.mode, army: g.army, tech: g.tech,
        mp: payer.mp, mun: payer.mun, fuel: payer.fuel,
        units: [...g.units.values()].map(unit => {
          const row = [];
          for (const key of ['id', 'type', 'owner', 'hp', 'built', 'x', 'z', 'cd', 'flags']) row[sim.UNIT_ROW[key]] = unit[key];
          return row;
        }), queues: [...g.units.values()].map(unit => [unit.id, 0, 0, unit.prog, ...unit.queue])});
      const scenario = {name: plan.name, input, initial: state(), trace: []};
      result.scenarios.push(scenario);
      for (const step of plan.steps) {
        const before = state();
        if (step.op === 'observe') {
          const snapshot = view();
          const card = availability(snapshot, sim.CFG, step.action);
          const count = buyCount(snapshot, sim.CFG, step.action, 5);
          scenario.trace.push({op: step.op, action: step.action, snapshot, card, count, expected: state()});
          assert.deepEqual(card, step.card, `${plan.name} card`);
          assert.equal(count, step.count, `${plan.name} count`);
          assert.deepEqual(state(), before, 'Queries are read-only');
        } else {
          const price = sim.priceOf(g, step.action.unit);
          if (step.action.unit === 'tank') assert.deepEqual(price, {mp: 200, fuel: 60});
          if (step.action.unit === 'rifle') assert.deepEqual(price, {mp: 100, fuel: 0});
          const denial = fn.command(g, 0, step.action) ?? null;
          scenario.trace.push({op: step.op, action: step.action, result: denial, expected: state()});
          assert.equal(denial, step.result, `${plan.name} Buy`);
          if (denial !== null) assert.deepEqual(state(), before, 'Buy refusal is atomic');
          else {
            assert.equal(payer.mp, Buffer.from(before.money[0], 'hex').readDoubleBE() - price.mp);
            assert.equal(payer.fuel, Buffer.from(before.money[2], 'hex').readDoubleBE() - price.fuel);
          }
        }
      }
    }
    clean();
    const data = JSON.stringify(result) + '\n';
    if (process.argv.includes('--check')) assert.equal(readFileSync(file, 'utf8'), data);
    else writeFileSync(file, data, {flag: 'wx'});
    console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} BASE13 controls: 5 independent inputs, 18 buys, 13 paid jobs, 10 cards, 10 counts, 0 ticks, 0 draws`);
  } catch (error) {
    if (process.env.BASE13_FAILURE_PATH) writeFileSync(process.env.BASE13_FAILURE_PATH,
      JSON.stringify({error: String(error), observed: result}) + '\n', {flag: 'wx'});
    throw error;
  } finally {
    Math.random = prior;
  }
}

if (process.argv.includes('--controls')) recordControls();
else await recordPaidPrice();
