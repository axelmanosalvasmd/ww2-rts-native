// Record a real buy, rally completion and replacement reaction route.
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
assert.equal(process.arch, 'x64'); assert.equal(process.version, 'v24.21.0');
assert.equal(git(['rev-parse', 'HEAD']), pin);
assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
const url = pathToFileURL(`${reference}/shared/sim.js`).href;
registerHooks({ load(path, context, next) {
  const out = next(path, context); if (path !== url) return out;
  const source = String(out.source), lines = source.split('\n');
  const complete = lines.filter(line => line.includes('if (u.worldGoal && (u.retreating'));
  const clear = lines.filter(line => line.trim() === 'if (!u.path.length) u.drift = false;');
  assert.equal(complete.length, 1); assert.equal(clear.length, 1);
  return { ...out, source: `${source}\nexport { react as __react, walkRoute as __walk, speedMul as __speedMul, knownTerrainFor as __known, rebuildGrid as __grid, placeUnit as __place };\nexport function __complete(g,u) { ${complete[0]} }\nexport function __clearDrift(u) { ${clear[0]} }\n` };
} });
const sim = await import(url);
const bits = n => { const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex'); };
const point = p => p ? [bits(p.x), bits(p.z)] : null;
const snapshot = u => ({ at: point(u), path: u.path.map(point), goal: point(u.worldGoal), amove: point(u.amove), drift: u.drift, target: u.targetId });
const rows = Array(32).fill('.'.repeat(40));
rows[12] = '.'.repeat(19) + 'T' + '.'.repeat(20);
const map = { name: 'Rally route replacement', w: 40, h: 32, rows,
  spawns: [{ x: 2, y: 2 }, { x: 37, y: 29 }], points: [{ id: 'p', x: 30, y: 24 }] };
const old = Math.random; Math.random = () => 0.5;
let g, players;
try {
  g = sim.createGame(map, [], false, [], [], { weather: false });
  players = sim.createGame(map, ['A', 'B'], false, [0, 1], [0, 1], { weather: false, tech: false }).players;
} finally { Math.random = old; }
Object.assign(g, { players, units: new Map(), nextId: 1, tick: 100, reveal: true, pathBudget: 4096, winner: null });
g.players[0].mp = 1000;
const maker = sim.internals.spawnUnit(g, 0, 'barracks');
Object.assign(maker, { x: 5, z: 5, built: 1, queue: [], productionJobs: [], prog: 0, rally: { x: 35, z: 25 } });
const shooter = sim.internals.spawnUnit(g, 1, 'rifle');
sim.__place(g, shooter, { x: 75, z: 55 }); sim.__grid(g);
const initial = [...g.units.values()].map(u => ({ id: u.id, kind: u.type, owner: u.owner, x: u.x, z: u.z, rot: u.rot, hp: u.hp, built: u.built ?? null }));
const id = g.nextId;
assert.equal(sim.command(g, 0, { t: 'buy', unit: 'rifle', from: maker.id }), undefined);
const u = g.units.get(id); assert.equal(u.drift, 'rally');
const stages = [{ name: 'born-rally', expected: snapshot(u) }];
let movements = 0;
while (u.path.length) {
  assert.ok(movements++ < 1000);
  g.tick++;
  sim.__walk(g, u, sim.UNITS.rifle, u.path, sim.UNITS.rifle.speed * sim.__speedMul(g, u), sim.TICK, false, { yielding: false }, () => sim.__known(g, u.owner));
  sim.__clearDrift(u); sim.__grid(g);
}
// Infantry retains its canonical zero hull speed. Use the actual completion predicate.
sim.__complete(g, u);
assert.equal(u.worldGoal, null); assert.equal(u.drift, false);
stages.push({ name: 'completed-rally', expected: snapshot(u) });
sim.__place(g, shooter, { x: 43, z: 25 }); sim.__grid(g);
Object.assign(u, { hitAt: g.tick, hitBy: shooter.id, hitFrom: { x: shooter.x, z: shooter.z }, holdFire: true });
sim.__react(g, u, sim.UNITS.rifle);
assert.equal(u.drift, 'cover'); assert.ok(u.path.length);
stages.push({ name: 'cover-route', expected: snapshot(u) });
u.holdFire = false;
sim.__react(g, u, sim.UNITS.rifle);
assert.equal(u.drift, 'cover'); assert.ok(u.path.length); assert.equal(u.amove, null);
stages.push({ name: 'cover-route-can-answer', expected: snapshot(u) });
const output = JSON.stringify({ reference: pin, node: process.version, arch: process.arch, map,
  initial, spawns: players.map(p => p.spawn), id, maker: maker.id, shooter: shooter.id, rally: maker.rally,
  movements, finalTick: g.tick, threat: { x: 43, z: 25 }, stages }) + '\n';
const path = `${root}/rust/crates/core/tests/fixtures/rally-drift.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), output);
else writeFileSync(path, output);
console.log(`Verified real rally completion and cover replacement: ${stages.length} stages, ${movements} movement calls`);
