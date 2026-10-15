// Pure production component inputs, not a Classic Match admission or runtime trace.
// Keep the extracted Source buy and production functions unchanged.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
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
const {buyCount} = await import(pathToFileURL(`${reference}/client/availability.js`));
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
