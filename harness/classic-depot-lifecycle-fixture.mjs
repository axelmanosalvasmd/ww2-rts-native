// Original Source construction/cancellation over canonical Classic startup.
// Raw observations stay intact; native checks compare the named components.
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
const input = {
  map: {name: 'Classic Depot canonical command', w: 128, h: 128,
    rows: Array(128).fill('.'.repeat(128)), spawns: [{x: 8, y: 8}, {x: 112, y: 112}],
    points: [{id: 'neutral', x: 64, y: 64, mp: 0, vp: 0}]},
  settings: {mode: 'classic', army: 'standard', tech: true, weather: false, logistics: false},
  names: ['A', 'B'], teams: [0, 1], factions: [0, 1],
};
const cases = [];
function run(name, exercise) {
  const originalRandom = Math.random, constructorDraws = [];
  Math.random = () => { constructorDraws.push(.5); return .5; };
  let g;
  try { g = sim.createGame(structuredClone(input.map), input.names, false,
    input.teams, input.factions, input.settings); }
  finally { Math.random = originalRandom; }
  const own = (slot, type) => [...g.units.values()].find(u => u.owner === slot && u.type === type);
  const raw = () => ({tick: g.tick, next: g.nextId,
    wallets: g.players.map(p => [bits(p.mp), bits(p.mun), bits(p.fuel)]),
    income: g.players.map(p => [bits(p.inc ?? 0), bits(p.fuelInc ?? 0), bits(p.upkeep ?? 0)]),
    story: structuredClone(g.story),
    nodes: g.nodes.map(n => ({c: n.c, x: bits(n.x), z: bits(n.z), depot: n.depot, rate: bits(n.rate), fuel: !!n.fuel})),
    units: [...g.units.values()].map(u => ({id: u.id, owner: u.owner, type: u.type,
      hp: bits(u.hp), built: u.built === undefined ? null : bits(u.built),
      x: bits(u.x), z: bits(u.z), build: u.build || null, cells: u.cells ?? null,
      path: u.path.map(p => [bits(p.x), bits(p.z)]), routeEnd: u.routeEnd ?? null})),
    terrain: g.chars.join(''),
    sections: [...g.structuralCells].map(([c, s]) => ({c, ...structuredClone(s)})),
    cellHp: Array.from(g.cellHp, bits),
    falling: structuredClone(g.fallingSections),
  });
  const initial = raw(), records = [];
  const advance = count => {
    const before = raw();
    for (let i = 0; i < count; i++) sim.step(g);
    assert.equal(g.tick, before.tick + count);
    records.push({op: 'advance', count, before, after: raw()});
  };
  const command = (cmd, expected = null, slot = 0) => {
    const before = raw();
    const result = sim.command(g, slot, structuredClone(cmd)) ?? null;
    assert.equal(result, expected);
    if (result !== null) assert.deepEqual(raw(), before, `${name}: refusal changed state`);
    records.push({op: 'command', cmd, slot, result, before, after: raw()});
  };
  Math.random = () => { throw Error(`${name}: unexpected post-constructor random draw`); };
  let selection;
  try {
    advance(1);
    const hq = own(0, 'hq');
    const index = g.nodes.findIndex(n => !n.depot && Math.hypot(n.x - hq.x, n.z - hq.z) <= 29);
    assert(index >= 0, 'real startup has a visible home node');
    const node = g.nodes[index];
    selection = {index, x: node.x + 1.25, z: node.z - .75};
    const build = {t: 'build', ids: [own(0, 'engineer').id], kind: 'depot', x: selection.x, z: selection.z};
    exercise({g, own, node, build, advance, command});
  } finally { Math.random = originalRandom; }
  cases.push({name, input, constructorDraws, selection, initial, records});
}
run('natural-completion-and-finished-refusal', ({g, own, node, build, advance, command}) => {
  const id = g.nextId, paid = g.players[0].mp;
  command(build);
  assert.equal(g.players[0].mp, paid - sim.UNITS.depot.cost);
  assert.equal(g.units.get(id).hp, sim.UNITS.depot.hpPer * .25);
  assert.equal(own(0, 'engineer').build, id);
  assert(own(0, 'engineer').path.length > 0, 'the actual Engineer must walk');
  advance(200);
  assert(g.units.get(id).built > 0 && g.units.get(id).built < 1,
    'the real Engineer reaches and starts its site without a pose assignment');
  advance(500);
  const site = g.units.get(id);
  assert.equal(site.built, 1, 'ordinary Source crew completes within the original 25-second work window after arrival');
  assert.equal(site.hp, sim.UNITS.depot.hpPer);
  assert.equal(node.depot, id);
  assert.equal(g.story[0].built, 1);
  assert(site.cells.every(c => g.chars[c] === 'K' && g.cellHp[c] === sim.UNITS.depot.hpPer / 4));
  command({t: 'cancel', id}, 'unseen');
  const before = g.players[0].mp;
  advance(1);
  const upkeep = [...g.units.values()].filter(u => u.owner === 0 && u.hp > 0 && !sim.UNITS[u.type].structure)
    .reduce((sum, u) => sum + sim.UNITS[u.type].cost * sim.CFG.classic.upkeep, 0);
  assert(Math.abs((g.players[0].mp - before) / sim.TICK - (sim.CFG.classic.trickle + node.rate - upkeep)) < 1e-8);
});
run('partial-cancel-release-rebuild-and-worker-clear', ({g, own, node, build, advance, command}) => {
  const id = g.nextId;
  command(build);
  advance(200);
  const site = g.units.get(id), cells = [...site.cells];
  assert(site.built > 0 && site.built < 1, 'genuine movement must produce a partial site before cancellation');
  command({t: 'cancel', id}, 'unseen', 1);
  const before = g.players[0].mp;
  command({t: 'cancel', id});
  assert.equal(g.players[0].mp, before + sim.UNITS.depot.cost * .75);
  assert.equal(g.story[0].mpSpent, sim.UNITS.depot.cost * .25);
  assert.equal(g.story[0].losses, 0);
  assert(!g.units.has(id)); assert.equal(node.depot, 0);
  assert(!g.structures.has(site.structureId));
  assert(cells.every(c => g.chars[c] === '.' && !g.structuralCells.has(c)));
  assert.equal(own(0, 'engineer').build, id, 'worker clears its stale target only on its own turn');
  command({t: 'cancel', id}, 'unseen');
  const replacement = g.nextId;
  command(build);
  assert.equal(node.depot, replacement);
  assert.equal(g.units.get(replacement).built, 0);
  command({t: 'cancel', id: replacement});
  assert.equal(node.depot, 0);
  advance(1);
  assert.equal(own(0, 'engineer').build, 0);
});
clean();
const data = JSON.stringify({reference: pin, node: process.version, arch: process.arch,
  scope: 'Raw original Source steps and commands. Native acceptance compares literal Depot lifecycle components, not whole-match snapshots.', cases}) + '\n';
const target = `${root}/rust/crates/core/tests/fixtures/classic-depot-lifecycle.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), data);
else writeFileSync(target, data);
console.log('2 lifecycle scenes; 8 commands; 904 full Source steps; 15 action frames and 2 initial frames');
