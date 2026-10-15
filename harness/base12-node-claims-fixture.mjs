// Actual Classic node claims, cancel/destruction and observed enemy occupancy.
import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
import {bits, point, construct, failure, pin, reference, save, sim} from './base-next-fixture-tools.mjs';
const {viewFor} = await import(pathToFileURL(`${reference}/shared/ai-view.js`));
const input = {
  map: {name: 'BASE12 canonical node history', w: 128, h: 128,
    rows: Array(128).fill('.'.repeat(128)), spawns: [{x: 8, y: 8}, {x: 112, y: 112}],
    points: [{id: 'neutral', x: 64, y: 64, mp: 0, vp: 0}]},
  names: ['A', 'B'], teams: [0, 1], factions: [0, 1],
  settings: {mode: 'classic', army: 'standard', tech: true, weather: false,
    weatherSeed: 1, supply: true, logistics: false, sandbox: false},
};
const result = {reference: pin, seed: 701, node: process.version, arch: process.arch, input, records: []};
const prior = Math.random;
let last = null;
try {
  Math.random = () => { throw Error('Only the declared constructor may consume global random values'); };
  // A real coastal constructor distinguishes node reservation from coast/terrain denial.
  const coastalInput = structuredClone(input);
  coastalInput.map.name = 'BASE12 Shipyard node exclusion';
  coastalInput.map.naval = true;
  coastalInput.map.rows = Array.from({length: 128}, (_, y) =>
    Array.from({length: 128}, (_, x) => y < 14 && x >= 21 && x <= 27 ? 'W' : '.').join(''));
  const coast = construct(coastalInput);
  const checks = [];
  result.shipyardCase = {input: coastalInput, checks};
  let blocked = null;
  for (const node of coast.nodes) {
    const place = sim.placementCheck(coast, {kind: 'shipyard', x: node.x, z: node.z, team: 0});
    const stamp = sim.__baseNextCanStamp(coast, place.cells);
    const shore = place.cells ? sim.__baseNextOnCoast(coast, place.cells) : false;
    checks.push({at: point(node), c: node.c, cells: place.cells ?? null, ok: place.ok,
      reason: place.reason ?? null, stamp, shore, depot: node.depot});
    if (!node.depot && place.reason === 'blocked' && stamp && shore) { blocked = node; break; }
  }
  assert.ok(blocked, 'authored coastal map must expose a free stampable node beside water');
  let control = null;
  const controls = result.shipyardCase.controls = [];
  for (let dy = -8; dy <= 8 && !control; dy++) for (let dx = -8; dx <= 8 && !control; dx++) {
    const at = {x: blocked.x + dx * sim.CELL, z: blocked.z + dy * sim.CELL};
    const place = sim.placementCheck(coast, {kind: 'shipyard', ...at, team: 0});
    controls.push({at, ok: place.ok, reason: place.reason ?? null});
    if (place.ok) control = {at, cells: place.cells};
  }
  assert.ok(control, 'nearby non-node coastal footprint must admit an actual Shipyard preview');
  result.shipyardCase.control = control;
  const g = construct(input), memories = [{}, {}];
  Math.random = () => { throw Error('Node claim history must not consume global random values'); };
  const own = (slot, type) => [...g.units.values()].find(u => u.owner === slot && u.type === type);
  const initial = [...g.units.values()].map(u => [u.id, u.owner, u.type, bits(u.hp)]);
  const raw = () => {
    const views = memories.map((memory, slot) => viewFor(g, slot, memory));
    return last = {tick: g.tick, seed: g.seed >>> 0, next: g.nextId,
      money: g.players.map(p => [bits(p.mp), bits(p.mun), bits(p.fuel)]),
      spent: g.players.map((_, slot) => bits(g.story?.[slot]?.mpSpent ?? 0)),
      nodes: g.nodes.map(n => [n.c, point(n), n.depot, bits(n.rate), !!n.fuel]),
      claims: views.map(v => [...v.claimedNodes]),
      ghosts: views.map(v => v.ghosts.map(b => [b.id, b.type, b.owner, b.x, b.z, b.built])),
      units: [...g.units.values()].map(u => ({id: u.id, owner: u.owner, type: u.type,
        hp: bits(u.hp), built: u.built === undefined ? null : bits(u.built),
        at: point(u), path: u.path.map(point), goal: point(u.worldGoal), cells: u.cells ?? null})),
      terrain: g.nodes.map(n => [n.c, [n.c, n.c + 1, n.c + g.w, n.c + g.w + 1]
        .map(c => [c, g.chars[c], bits(g.cellHp[c]), g.cellState[c]])]),
      cellLog: structuredClone(g.cellLog),
    };
  };
  const retained = () => {
    for (const [id, owner, type, hp] of initial) {
      const u = g.units.get(id);
      assert.ok(u, 'every constructor actor remains present');
      assert.deepEqual([u.owner, u.type, bits(u.hp)], [owner, type, hp]);
    }
    assert.equal(g.winner, null);
  };
  const command = (slot, cmd, denial = null) => {
    const row = {op: 'command', slot, cmd, before: raw()};
    result.records.push(row);
    row.result = sim.command(g, slot, cmd) ?? null;
    row.after = raw();
    assert.equal(row.result, denial);
    if (denial) assert.deepEqual(row.after, row.before, 'refusal preserves money, IDs, nodes, units and terrain');
    retained();
  };
  const steps = (label, limit, done) => {
    const row = {op: 'steps', label, limit, frames: []};
    result.records.push(row);
    for (let i = 0; i < limit && !done(); i++) {
      sim.step(g);
      row.frames.push(raw());
      retained();
    }
    assert.ok(done(), `${label} must finish within its declared natural-tick bound`);
  };
  const observe = (label, predicate) => {
    const row = {op: 'observe', label, expected: raw()};
    result.records.push(row);
    assert.ok(predicate(row.expected), label);
  };
  const choose = slot => g.nodes.map((node, index) => ({node, index}))
    .sort((a, b) => Math.hypot(a.node.x - own(slot, 'hq').x, a.node.z - own(slot, 'hq').z)
      - Math.hypot(b.node.x - own(slot, 'hq').x, b.node.z - own(slot, 'hq').z))[0];
  result.initial = raw();
  for (const slot of [0, 1]) command(slot, {t: 'stance', key: 'holdFire', on: true,
    ids: [...g.units.values()].filter(u => u.owner === slot && !sim.UNITS[u.type].structure).map(u => u.id)});
  const home = choose(0), enemy = choose(1), eng = own(0, 'engineer'), foe = own(1, 'engineer');
  result.aliases = {home: home.index, enemy: enemy.index, engineer: eng.id, foe: foe.id, scout: own(0, 'rifle').id};
  const build = (unit, node) => ({t: 'build', ids: [unit.id], kind: 'depot', x: node.x, z: node.z});
  // Test reservation before a Depot exists, so its own stamped cells cannot cause the denial.
  const place = sim.placementCheck(g, {kind: 'barracks', x: home.node.x, z: home.node.z, team: 0}, at => sim.teamSees(g, 0, at));
  assert.ok(sim.__baseNextCanStamp(g, place.cells), 'the free resource footprint otherwise accepts a stamp');
  result.records.push({op: 'placement', kind: 'barracks', at: {x: home.node.x, z: home.node.z}, reason: place.reason, ok: place.ok});
  assert.equal(place.ok, false);
  assert.equal(place.reason, 'blocked');
  const unseen = sim.placementCheck(g, {kind: 'depot', x: enemy.node.x, z: enemy.node.z, team: 0}, at => sim.teamSees(g, 0, at));
  result.records.push({op: 'placement', kind: 'depot', at: {x: enemy.node.x, z: enemy.node.z}, reason: unseen.reason, ok: unseen.ok});
  assert.equal(unseen.ok, false);
  assert.equal(unseen.reason, 'notVisible');
  const boundary = {x: home.node.x + 8, z: home.node.z};
  const nearest = sim.placementCheck(g, {kind: 'depot', ...boundary, team: 0}, at => sim.teamSees(g, 0, at));
  assert.equal(nearest.ok, true);
  assert.equal(nearest.node, home.node, 'inclusive eight-metre click snaps to the actual nearest free node');
  command(0, {...build(eng, home.node), ...boundary});
  let id = home.node.depot;
  const site = g.units.get(id), cells = [...site.cells];
  assert.equal(site.built, 0);
  assert.equal(g.players[0].mp, sim.CFG.classic.mpStart - 60);
  command(0, build(eng, home.node), 'blocked');
  command(0, {t: 'cancel', id});
  observe('cancel-frees-node', () => home.node.depot === 0 && !g.units.has(id));
  assert.ok(!g.structures.has(site.structureId));
  assert.ok(cells.every(c => g.chars[c] === '.' && !g.structuralCells.has(c)));
  command(0, build(eng, home.node));
  id = home.node.depot;
  steps('actual-engineer-completion', 900, () => g.units.get(id)?.built === 1);
  command(0, {t: 'destroy', id});
  steps('actual-destruction-release', 2, () => !g.units.has(id));
  observe('destruction-frees-node', () => home.node.depot === 0);
  command(1, build(foe, enemy.node));
  const hidden = enemy.node.depot, scout = own(0, 'rifle');
  observe('unseen-enemy-does-not-claim', s => !sim.teamSees(g, 0, g.units.get(hidden)) && !s.claims[0].includes(enemy.index));
  steps('enemy-depot-finishes', 900, () => g.units.get(hidden)?.built === 1);
  command(0, {t: 'move', orders: [[scout.id, enemy.node.x, enemy.node.z]]});
  steps('actual-scout-sees-enemy-depot', 2500, () => sim.teamSees(g, 0, g.units.get(hidden)));
  command(0, {t: 'stop', ids: [scout.id]});
  observe('visible-enemy-claims', s => s.claims[0].includes(enemy.index));
  command(0, {t: 'move', orders: [[scout.id, g.players[0].spawn.x, g.players[0].spawn.z]]});
  steps('actual-scout-leaves-sight', 1500, () => !sim.teamSees(g, 0, g.units.get(hidden)));
  command(0, {t: 'stop', ids: [scout.id]});
  observe('remembered-enemy-claims', s => s.claims[0].includes(enemy.index));
  command(1, {t: 'destroy', id: hidden});
  steps('unseen-enemy-destruction', 2, () => !g.units.has(hidden));
  observe('hidden-destruction-retains-belief', s => enemy.node.depot === 0 && s.claims[0].includes(enemy.index));
  command(0, {t: 'move', orders: [[scout.id, enemy.node.x, enemy.node.z]]});
  steps('actual-scout-revisits-empty-node', 1500, () => sim.teamSees(g, 0, enemy.node) && !raw().claims[0].includes(enemy.index));
  observe('revisited-empty-node-clears-belief', s => !s.claims[0].includes(enemy.index));
  result.final = raw();
  save('rust/crates/simtest/tests/fixtures/base12-node-claims.json', result);
  console.log('BASE12: genuine Classic claim/cancel/destruction plus unseen/visible/remembered/revisited enemy node history; actual counts in records');
} catch (error) {
  failure('BASE12_FAILURE_PATH', error, {result, last});
  throw error;
} finally { Math.random = prior; }
