// Original missing-ID cancellation and live-node income. Earlier recipes stay frozen.
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
const cases = [];
function run(mode, exercise) {
  const input = {map: {name: 'Classic Depot canonical command', w: 128, h: 128,
    rows: Array(128).fill('.'.repeat(128)), spawns: [{x: 8, y: 8}, {x: 112, y: 112}],
    points: [{id: 'neutral', x: 64, y: 64, mp: 0, vp: 0}]},
    settings: {mode, army: 'standard', tech: true, weather: false, logistics: false},
    names: ['A', 'B'], teams: [0, 1], factions: [0, 1]};
  const originalRandom = Math.random, constructorDraws = [];
  Math.random = () => { constructorDraws.push(.5); return .5; };
  let g;
  try { g = sim.createGame(structuredClone(input.map), input.names, false, input.teams, input.factions, input.settings); }
  finally { Math.random = originalRandom; }
  const own = type => [...g.units.values()].find(u => u.owner === 0 && u.type === type);
  const raw = () => ({tick: g.tick, next: g.nextId,
    players: g.players.map(p => ({mp: bits(p.mp), mun: bits(p.mun), fuel: bits(p.fuel),
      inc: bits(p.inc ?? 0), fuelInc: bits(p.fuelInc ?? 0), upkeep: bits(p.upkeep ?? 0)})),
    nodes: (g.nodes ?? []).map(n => ({c: n.c, depot: n.depot, rate: bits(n.rate), fuel: !!n.fuel})),
    story: structuredClone(g.story),
    units: [...g.units.values()].map(u => ({id: u.id, owner: u.owner, type: u.type,
      hp: bits(u.hp), built: u.built === undefined ? null : bits(u.built), x: bits(u.x), z: bits(u.z),
      build: u.build || null, path: u.path.map(p => [bits(p.x), bits(p.z)]), cells: u.cells ?? null})),
    terrain: g.chars.join(''), cellHp: Array.from(g.cellHp, bits),
    structures: [...g.structures], sections: [...g.structuralCells], falling: structuredClone(g.fallingSections),
  });
  const initial = raw(), records = [];
  const command = (cmd, expect = null) => {
    const before = raw(), result = sim.command(g, 0, structuredClone(cmd)) ?? null;
    assert.equal(result, expect);
    if (result !== null) assert.deepEqual(raw(), before);
    records.push({op: 'command', cmd, result, before, after: raw()});
  };
  const advance = (count, phase) => {
    const before = raw();
    for (let i = 0; i < count; i++) sim.step(g);
    assert.equal(g.tick, before.tick + count);
    records.push({op: 'advance', count, phase, before, after: raw()});
  };
  Math.random = () => { throw Error(`${mode}: unexpected random draw after construction`); };
  try { exercise({g, own, command, advance}); }
  finally { Math.random = originalRandom; }
  cases.push({mode, input, constructorDraws, initial, records});
}
run('classic', ({g, own, command, advance}) => {
  command({t: 'cancel'}, 'unseen');
  advance(1, 'baseline');
  const baseline = g.players[0].inc, hq = own('hq');
  const node = g.nodes.find(n => !n.depot && Math.hypot(n.x - hq.x, n.z - hq.z) <= 29);
  assert(node && !node.fuel);
  const build = {t: 'build', ids: [own('engineer').id], kind: 'depot', x: node.x + 1.25, z: node.z - .75};
  const first = g.nextId;
  command(build);
  command({t: 'cancel', ids: [first]}, 'unseen');
  command({t: 'cancel', id: null, ids: [first]}, 'unseen');
  command({t: 'cancel', id: 0}, 'unseen');
  advance(1, 'unfinished');
  assert.equal(g.players[0].inc, baseline, 'a bound unfinished node pays no extra MP');
  assert.equal(node.depot, first);
  assert(g.units.get(first).built < 1);
  command({t: 'cancel', id: first});
  advance(1, 'cancelled');
  assert.equal(node.depot, 0);
  assert.equal(g.players[0].inc, baseline, 'a cancelled node pays no extra MP');
  const second = g.nextId;
  command(build);
  advance(200, 'started');
  assert(g.units.get(second).built > 0 && g.units.get(second).built < 1);
  assert.equal(g.players[0].inc, baseline);
  advance(500, 'completed');
  assert.equal(g.units.get(second).built, 1);
  advance(1, 'paying');
  assert(Math.abs(g.players[0].inc - baseline - node.rate) < 1e-12,
    'the actual finished node adds its rate once');
  command({t: 'cancel', id: second}, 'unseen');
  advance(1, 'still-paying');
  assert(Math.abs(g.players[0].inc - baseline - node.rate) < 1e-12);
});
run('conquest', ({own, command}) => {
  const hq = own('hq');
  command({t: 'cancel'}, 'unseen');
  command({t: 'cancel', ids: [hq.id]}, 'unseen');
  command({t: 'cancel', id: null, ids: [hq.id]}, 'unseen');
  command({t: 'cancel', id: 0}, 'unseen');
});
clean();
const data = JSON.stringify({reference: pin, node: process.version, arch: process.arch,
  scope: 'Raw constructors, original commands and actual steps. Compare named cancellation refusals and live-node income components.', cases}) + '\n';
const target = `${root}/rust/crates/core/tests/fixtures/classic-depot-income.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), data);
else writeFileSync(target, data);
console.log('2 scenes; 12 commands; 705 full Source steps; 19 action frames and 2 initial frames');
