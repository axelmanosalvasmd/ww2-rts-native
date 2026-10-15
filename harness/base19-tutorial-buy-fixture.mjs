// Original no-maker Tutorial command, home placement and personal-rally history.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {bits, point, construct, failure, pin, reference, save, sim} from './base-next-fixture-tools.mjs';
const input = {
  map: JSON.parse(readFileSync(`${reference}/maps/tutorial.json`, 'utf8')),
  names: ['me'], teams: [0], factions: [1],
  settings: {mode: 'tutorial', army: 'standard', weather: false, weatherSeed: 1,
    tech: false, supply: false, logistics: false, sandbox: false},
};
const cases = [], result = {reference: pin, seed: 701, node: process.version, arch: process.arch, input, cases};
let active = null;
const prior = Math.random;
try {
  Math.random = () => { throw Error('Only the declared constructor may consume global random values'); };
  for (const rally of [null, {x: 19, z: 45}]) {
    const g = construct(input);
    Math.random = () => { throw Error('Tutorial purchase history must not consume global random values'); };
    const records = [];
    active = {name: rally ? 'personal-rally' : 'map-centre-facing', records};
    cases.push(active);
    const raw = () => ({tick: g.tick, seed: g.seed >>> 0, next: g.nextId,
      money: g.players.map(p => [bits(p.mp), bits(p.mun ?? 0), bits(p.fuel ?? 0)]),
      spent: g.players.map((_, i) => bits(g.story?.[i]?.mpSpent ?? 0)),
      rally: point(g.players[0].rally),
      units: [...g.units.values()].map(u => ({id: u.id, owner: u.owner, type: u.type,
        hp: bits(u.hp), pose: [u.x, u.z, u.rot, u.aim, u.ghost].map(bits),
        path: u.path.map(point), goal: point(u.worldGoal), drift: u.drift === 'rally',
        cells: u.cells ?? null})),
    });
    active.initial = raw();
    assert.equal(g.mode.kind, 'tutorial');
    assert.equal(g.mode.step, 0);
    assert.equal(g.units.size, 3);
    assert.ok([...g.units.values()].every(u => u.owner === 0 && !sim.UNITS[u.type].building));
    const command = (cmd, denial = null) => {
      const before = raw(), row = {op: 'command', cmd, before};
      records.push(row);
      row.result = sim.command(g, 0, cmd) ?? null;
      row.after = raw();
      assert.equal(row.result, denial);
      if (denial) assert.deepEqual(row.after, before, 'denied purchase changes no observed canonical state');
    };
    if (rally) command({t: 'rally', ...rally});
    const id = g.nextId, mp = g.players[0].mp, count = g.units.size;
    // Source ignores from in the no-maker branch, even when the ID does not exist.
    command({t: 'buy', unit: 'rifle', from: 4294967295});
    const u = g.units.get(id), to = rally ?? {x: g.w * sim.CELL / 2, z: g.h * sim.CELL / 2};
    assert.ok(u && u.type === 'rifle' && u.owner === 0);
    assert.equal(g.units.size, count + 1);
    assert.equal(g.nextId, id + 1);
    assert.equal(g.players[0].mp, mp - 100);
    assert.equal(g.story[0].mpSpent, 100);
    assert.equal(u.rot, Math.atan2(to.z - g.players[0].spawn.z, to.x - g.players[0].spawn.x));
    assert.ok(Math.hypot(u.x - g.players[0].spawn.x, u.z - g.players[0].spawn.z) < 20);
    assert.ok(u.ghost > 0);
    assert.ok([...g.units.values()].every(unit => !sim.UNITS[unit.type].building));
    assert.deepEqual(point(u.worldGoal), point(rally));
    assert.equal(u.drift === 'rally', !!rally);
    if (rally) assert.ok(u.path.length > 0, 'accepted personal rally must produce an actual route');
    command({t: 'buy', unit: 'at'}, 'mp');
    active.bought = id;
    if (rally) {
      const row = {op: 'walk', limit: 300, frames: []};
      records.push(row);
      const at = point(u);
      for (let tick = 0; tick < row.limit && u.path.length; tick++) {
        sim.step(g);
        row.frames.push(raw());
        assert.equal(g.mode.step, 0, 'this bounded rally history stays at the first Tutorial goal');
      }
      assert.equal(u.path.length, 0, 'real unit reaches its personal rally within the declared bound');
      assert.notDeepEqual(point(u), at, 'rally must move the purchased unit');
      assert.ok(Math.hypot(u.x - rally.x, u.z - rally.z) < 3);
    }
    active.final = raw();
    active = null;
  }
  save('rust/crates/simtest/tests/fixtures/base19-tutorial-buy.json', result);
  console.log('BASE19: 2 genuine constructors, 5 commands, 2 no-maker births, 2 atomic MP refusals; actual walk frames recorded');
} catch (error) {
  failure('BASE19_FAILURE_PATH', error, {result, active});
  throw error;
} finally { Math.random = prior; }
