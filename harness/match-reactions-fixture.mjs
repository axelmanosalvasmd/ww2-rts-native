// Exercise the pinned reaction and retreat bodies on real game state.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.equal(git(['rev-parse', 'HEAD']), pin);
assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
const url = pathToFileURL(`${reference}/shared/sim.js`).href;
registerHooks({ load(path, context, next) {
  const result = next(path, context);
  if (path !== url) return result;
  const source = String(result.source);
  const automatic = source.split('\n').filter(line => line.includes('if (!u.convoy && u.autoRetreat'));
  assert.equal(automatic.length, 1);
  return { ...result, source: `${source}\nexport { react as __react, retreatUnit as __retreat, homeOf as __home, placeUnit as __placeUnit, claim as __claim };\nexport function __automaticRetreat(g,u) { const def=UNITS[u.type]; ${automatic[0]} }\n` };
} });
const sim = await import(url);
const buffer = Buffer.alloc(8);
const bits = n => { buffer.writeDoubleBE(n); return buffer.toString('hex'); };
const point = p => p ? [bits(p.x), bits(p.z)] : null;
const snapshot = u => ({ path: u.path.map(point), worldGoal: point(u.worldGoal), amove: point(u.amove),
  reverse: point(u.reverse), react: bits(u.react), drift: u.drift, target: u.targetId, attack: u.attackId,
  retreating: u.retreating, orders: u.orders.length, garrison: u.garrison, holdFire: !!u.holdFire,
  holdPos: !!u.holdPos, autoRetreat: !!u.autoRetreat, nade: point(u.nade), enter: u.enter,
  fireAt: u.fireAt, board: u.board, build: !!u.build, dig: !!u.dig, entrench: !!u.entrench });
const rows = Array(32).fill('.'.repeat(40));
rows[12] = '.'.repeat(14) + 'TTT...HH...OO' + '.'.repeat(13);
rows[13] = rows[12];
assert(rows.every(row => row.length === 40));
const map = { name: 'reaction-cover', w: 40, h: 32, rows,
  spawns: [{ x: 0, y: 0 }, { x: 39, y: 31 }], points: [{ id: 'p', x: 30, y: 24 }] };
const cases = [];
function record(name, kind, fields, action = 'react', options = {}) {
  const save = Math.random;
  Math.random = () => 0.5;
  let g, players;
  try {
    g = sim.createGame(map, [], false, [], [], { weather: false });
    players = sim.createGame(map, ['A', 'B'], false, [0, 1], [0, 1], { weather: false }).players;
  } finally { Math.random = save; }
  Object.assign(g, { players, units: new Map(), nextId: 1, tick: 100, pathBudget: 4096, winner: null });
  const add = (owner, type, at, extra = {}) => {
    const u = sim.internals.spawnUnit(g, owner, type);
    Object.assign(u, at, extra);
    sim.__placeUnit(g, u, at);
    return u;
  };
  if (options.spacing) add(0, 'rifle', { x: 35.5, z: 25 });
  const u = add(0, kind, options.at ?? (options.capture ? { x: 61, z: 49 } : { x: 35, z: 25 }), fields);
  const shooter = add(1, 'rifle', { x: options.far ? 75 : 39, z: 25 });
  for (const home of options.homes ?? []) add(home.owner ?? 0, 'hq', { x: home.x, z: home.z }, { built: home.built ?? 1, hp: home.hp ?? 800 });
  if (options.close) add(0, 'rifle', { x: 35.5, z: 25 });
  u.hitBy = shooter.id;
  if (!options.stale) { u.hitAt = g.tick; u.hitFrom = { x: shooter.x, z: shooter.z }; }
  for (const p of players) p.visible = new Set(options.unseen ? [] : [shooter.id]);
  const input = [...g.units.values()].map(v => ({ id: v.id, kind: v.type, owner: v.owner, x: v.x, z: v.z,
    rot: v.rot, hp: v.hp, hitAt: v.hitAt, hitBy: v.hitBy, hitFrom: v.hitFrom, path: v.path,
    worldGoal: v.worldGoal ?? null, amove: v.amove, reverse: v.reverse, react: v.react,
    drift: v.drift, target: v.targetId, attack: v.attackId, retreating: v.retreating,
    orders: v.orders, garrison: v.garrison, holdFire: !!v.holdFire, holdPos: !!v.holdPos,
    autoRetreat: !!v.autoRetreat, nade: v.nade, enter: v.enter, fireAt: v.fireAt,
    board: v.board, riding: v.riding, build: v.build, dig: v.dig, entrench: v.entrench,
    built: v.built }));
  const before = snapshot(u);
  g.claims = new Map();
  for (const v of g.units.values()) if (v.targetId && v.hp > 0) sim.__claim(g, v, v.targetId, 1);
  if (action === 'retreat') sim.__retreat(g, u);
  else if (action === 'automatic') sim.__automaticRetreat(g, u);
  else if (action !== 'home') sim.__react(g, u, sim.UNITS[kind]);
  cases.push({ name, kind, fields, action, options, unit: u.id, input,
    visible: [...players[0].visible], spawns: players.map(p => p.spawn),
    before, expected: snapshot(u), home: point(sim.__home(g, u)),
    claims: [...g.claims].map(([key, value]) => [key, bits(value)]) });
}
for (const kind of ['rifle', 'mg', 'tank']) {
  record(`${kind}-answer`, kind, {});
  record(`${kind}-hold-fire`, kind, { holdFire: true });
  record(`${kind}-unseen`, kind, {}, 'react', { unseen: true });
  record(`${kind}-far`, kind, {}, 'react', { far: true });
  record(`${kind}-low-health`, kind, { hp: 1 }, 'react', { far: true });
  record(`${kind}-retreat`, kind, { attackId: 2, targetId: 2, path: [{ x: 41, z: 25 }] }, 'retreat');
}
for (const [field, value] of Object.entries({ holdPos: true, orders: [{ t: 'stop' }], entrench: {},
  retreating: true, garrison: 10, attackId: 2, amove: { x: 41, z: 25 }, nade: {}, dig: {},
  build: 9, enter: 10, board: 2, riding: 2, fireAt: 10, worldGoal: { x: 41, z: 25 }, react: 1 })) {
  record(`blocked-${field}`, 'rifle', { [field]: value });
}
record('rally-answer', 'rifle', { path: [{ x: 41, z: 25 }], drift: 'rally' });
record('rally-hold-fire', 'rifle', { path: [{ x: 41, z: 25 }], drift: 'rally', holdFire: true });
record('stale-hit', 'rifle', {}, 'react', { stale: true });
record('idle-spacing', 'rifle', {}, 'react', { stale: true, spacing: true });
record('idle-spacing-targeted', 'rifle', { targetId: 3 }, 'react', { stale: true, spacing: true });
record('idle-spacing-hold-position', 'rifle', { holdPos: true }, 'react', { stale: true, spacing: true });
record('vehicle-capture-no-pullback', 'tank', { hp: 1 }, 'react', { capture: true, far: true });
for (const hp of [34.999, 35, 35.001]) record(`auto-threshold-${hp}`, 'rifle', { hp }, 'automatic');
record('auto-disabled', 'rifle', { hp: 1, autoRetreat: false }, 'automatic');
record('auto-already-retreating', 'rifle', { hp: 1, retreating: true }, 'automatic');
record('auto-hold-position', 'rifle', { hp: 1, holdPos: true }, 'automatic');
record('auto-at-home-radius', 'rifle', { hp: 1 }, 'automatic', { at: { x: 16, z: 1 } });
record('auto-outside-home-radius', 'rifle', { hp: 1 }, 'automatic', { at: { x: 16.001, z: 1 } });
record('nearest-owned-finished-hq', 'rifle', {}, 'home', { homes: [
  { x: 34, z: 25, owner: 1 }, { x: 35, z: 26, built: 0.9 },
  { x: 36, z: 25, hp: 0 }, { x: 30, z: 25 }, { x: 45, z: 25 } ] });
record('equal-distance-hq-order', 'rifle', {}, 'home', { homes: [{ x: 30, z: 25 }, { x: 40, z: 25 }] });
const text = JSON.stringify({ reference: pin, node: process.version, arch: process.arch, map, cases }) + '\n';
const destination = `${root}/rust/crates/core/tests/fixtures/match-reactions.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(destination, 'utf8'), text);
else writeFileSync(destination, text);
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} ${cases.length} reaction and retreat cases`);
