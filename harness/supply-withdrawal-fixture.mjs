// Record the original withdrawal transitions. Startup inputs and draws reuse match-start.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
const root = fileURLToPath(new URL('../', import.meta.url));
const ref = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
assert.equal(execFileSync('git', ['rev-parse', 'HEAD'], { cwd: ref, encoding: 'utf8' }).trim(), pin);
execFileSync('git', ['diff', '--exit-code', pin, '--', 'shared'], { cwd: ref });
const simUrl = pathToFileURL(`${ref}/shared/sim.js`).href;
registerHooks({ load(url, context, next) {
  const result = next(url, context);
  return url === simUrl ? { ...result, source: String(result.source) + '\nexport {supplyWithdrawal,supplyHooks,updateGrid};\n' } : result;
} });
const S = await import(simUrl);
const C = await import(pathToFileURL(`${ref}/shared/supply-convoys.js`));
const base = JSON.parse(readFileSync(`${root}/rust/crates/simtest/tests/fixtures/match-start.json`, 'utf8'));
const mask = (1n << 64n) - 1n, gamma = 0x9e3779b97f4a7c15n;
function mix(x) {
  x = ((x ^ (x >> 30n)) * 0xbf58476d1ce4e5b9n) & mask;
  x = ((x ^ (x >> 27n)) * 0x94d049bb133111ebn) & mask;
  return x ^ (x >> 31n);
}
function stream(seed, purpose) {
  let state = mix((seed ^ ((BigInt(purpose) * gamma) & mask)) & mask);
  return () => mix(state = (state + gamma) & mask);
}
function create(sourceCase) {
  const c = base.cases[sourceCase], map = base.maps[c.map];
  const terrain = stream(BigInt(c.seed), 1), weather = stream(BigInt(c.seed), 2);
  const draws = [...(c.shuffle ? [Number(terrain() >> 11n) / 9007199254740992] : []),
    Number(weather() & 0xffffffffn) / 4294967296,
    ...(c.weatherSeed === undefined && c.weather === 'random' ? [Number(weather() % (1n << 31n)) / 2147483648] : [])];
  let used = 0;
  const old = Math.random;
  Math.random = () => { assert.ok(used < draws.length); return draws[used++]; };
  try {
    const g = S.createGame(map.map, c.players.map(p => p.name), c.shuffle,
      c.players.map(p => p.team), c.players.map(p => p.faction),
      { mode: 'conquest', army: c.army, weather: c.living ? c.weather : false,
        weatherSeed: c.weatherSeed, mapKey: map.name, tech: c.tech, logistics: true });
    assert.equal(used, draws.length);
    return g;
  } finally { Math.random = old; }
}
const bits = n => {
  if (n === undefined || n === null) return null;
  const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex');
};
const point = p => p ? [bits(p.x), bits(p.z)] : null;
function row(u) {
  return {
    at: point(u), path: u.path.map(point), worldGoal: point(u.worldGoal), routeEnd: point(u.routeEnd),
    retreating: u.retreating, garrison: u.garrison, stuck: bits(u.stuck),
    started: !!u.supplyStarted, escape: !!u.supplyEscape, hold: !!u.supplyHold, retry: u.supplyRetry ?? null,
    orders: u.orders.map(c => c.t === 'move'
      ? ['move', c.orders.map(r => [r[0], bits(r[1]), bits(r[2])])]
      : ['ability', bits(c.x), bits(c.z)]),
    attack: u.attackId, target: u.targetId, enter: u.enter, board: u.board,
    nade: point(u.nade), fireAt: u.fireAt, face: u.face ?? null,
    build: !!u.build, dig: !!u.dig, entrench: !!u.entrench, amove: point(u.amove),
    outcome: u.moveOutcome ?? null, outcomeTick: u.moveOutcomeTick ?? null,
    reserve: u.logistics ? { forced: u.logistics.forced, stranded: u.logistics.stranded,
      fuel: bits(u.logistics.fuel), emergency: bits(u.logistics.emergency) } : null,
  };
}
const traces = [];
for (const sourceCase of [5, 27, 60]) for (const profile of ['rifle', 'tank', 'field']) {
  const kind = profile === 'field' ? 'rifle' : profile;
  const g = create(sourceCase), u = S.internals.spawnUnit(g, 0, kind);
  const initial = { id: u.id, kind, x: u.x, z: u.z };
  const stores = [...g.convoys.stores.values()], active = stores.map(s => s.active);
  const home = stores.find(s => s.owner === 0 && s.source);
  const steps = [];
  function step(name, tick, patch = {}, reserve = {}, storeMode = null, others = []) {
    g.tick = tick;
    Object.assign(u, patch);
    for (const patch of others) { const other = g.units.get(patch.id); Object.assign(other, patch); S.updateGrid(g, other); }
    if (u.logistics) Object.assign(u.logistics, reserve);
    if (storeMode) stores.forEach((s, i) => { s.active = storeMode === 'restore' ? active[i] : false; });
    S.updateGrid(g, u);
    const input = { name, tick, patch: structuredClone(patch), reserve, storeMode, others };
    S.supplyWithdrawal(g, u);
    steps.push({ input, expected: row(u) });
  }
  if (profile === 'field') {
    g.tick = 10;
    let command;
    for (const dx of [-8, -4, 0, 4, 8]) for (const dz of [-8, -4, 0, 4, 8]) {
      if (command) continue;
      const candidate = { t: 'dig', ids: [u.id], kind: 'trench', x: u.x + dx, z: u.z + dz, dir: 0 };
      if (S.command(g, 0, candidate) === undefined) command = candidate;
    }
    assert.ok(command, 'No visible trench site for original field admission');
    const visibility = [];
    const placement = S.placementCheck(g, { ...command, team: g.players[0].team }, at => {
      const seen = S.teamSees(g, g.players[0].team, at); visibility.push({ ...at, seen }); return seen;
    });
    assert.ok(placement.ok);
    const fieldStart = { tick: g.tick, command, visibility, mpAfter: bits(g.players[0].mp), initial: row(u) };
    step('first forced cancels paid detailed field work', 20, {}, { forced: true }, 'none');
    assert.equal(u.dig, null);
    assert.equal(bits(g.players[0].mp), fieldStart.mpAfter);
    traces.push({ sourceCase, initial, fieldStart, steps });
    continue;
  }
  const one = [{ x: u.x + 4, z: u.z }], goal = one[0];
  step('unforced no-op', 10);
  step('first forced cancels intents without a store', 20,
    { orders: [{ t: 'move', orders: [[u.id, goal.x, goal.z]], queue: false },
        ...(kind === 'rifle' ? [{ t: 'ability', x: goal.x, z: goal.z }] : [])],
      attackId: 999, targetId: 998, enter: 1, board: 2, nade: goal, fireAt: 3 },
    { forced: true }, 'none');
  step('existing path, stuck at boundary', 21, { path: one, worldGoal: goal, stuck: 1 });
  step('existing path, stuck above boundary', 22, { stuck: 1.0001 });
  step('escape path', 23, { supplyEscape: true, stuck: 0 });
  step('escape goal without path', 24, { path: [], worldGoal: goal });
  step('recovered clears withdrawal', 25,
    { orders: [{ t: 'move', orders: [[u.id, goal.x, goal.z]], queue: false }] }, { forced: false });
  step('forced hold', 26, { supplyHold: true, path: one, worldGoal: goal }, { forced: true });
  step('repeated hold', 27);
  if (kind === 'tank') {
    step('empty ordinary and emergency fuel', 28, { supplyHold: false }, { fuel: 0, emergency: 0 });
    step('emergency fuel enables retry', 29, {}, { emergency: 1 });
  }
  step('retry before boundary', 59, { supplyHold: false, supplyRetry: 60, path: [], worldGoal: null });
  step('retry at boundary without stores', 60);
  step('real source route', 100, { x: g.w, z: g.h, stuck: 0 }, {}, 'restore');
  step('nearby source uses real handoff', 140,
    { x: home.x + 12, z: home.z, path: [], worldGoal: null, supplyRetry: 0 });
  const house = g.chars.indexOf('H');
  if (house >= 0) {
    const other = [...g.units.values()].find(t => t.owner === 0 && t.type === 'rifle' && t.id !== u.id);
    assert.ok(other);
    const occupancy = { id: other.id, garrison: house, x: (house % g.w + .5) * 2, z: (Math.floor(house / g.w) + .5) * 2 };
    step('first forced exits a house while another squad remains inside', 180,
      { x: occupancy.x, z: occupancy.z, garrison: house, supplyStarted: false, path: [], worldGoal: null, supplyRetry: 0 },
      {}, null, [occupancy]);
    assert.equal(other.garrison, house);
  }
  step('riding forced unit is left to carrier', 200, { riding: [...g.units.values()].find(t => t.owner === 0 && t.id !== u.id).id, supplyStarted: false });
  step('riding recovered unit clears prior withdrawal', 201, { supplyStarted: true }, { forced: false });
  traces.push({ sourceCase, initial, steps });
}
// Explicit incoming idle truck state isolates free retirement from placement.
const g = create(5), home = [...g.convoys.stores.values()].find(s => s.owner === 0 && s.source);
const truck = C.addTruck(g, 0, home, S.supplyHooks(g));
const retirement = { sourceCase: 5, initial: { id: truck.id, kind: 'truck', x: truck.x, z: truck.z }, origin: home.id };
g.tick = 40;
C.stepConvoys(g, .05, S.supplyHooks(g));
assert.ok(!g.units.has(truck.id), 'actual source must retire the free idle truck');
retirement.units = [...g.units.keys()];
retirement.reserves = [...g.units.values()].filter(u => u.logistics).map(u => u.id);
const path = `${root}/rust/crates/simtest/tests/fixtures/supply-withdrawal.json`;
const data = JSON.stringify({ reference: pin, traces, retirement }) + '\n';
if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), data);
else writeFileSync(path, data);
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} ${traces.length} original withdrawal traces, ${traces.reduce((n,t)=>n+t.steps.length,0)} transitions`);
