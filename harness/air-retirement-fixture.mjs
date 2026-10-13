// Replay the original ordinary death block, including its real story consumer.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
assert.equal(process.arch, 'x64'); assert.equal(process.version, 'v24.21.0');
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => { assert.equal(git(['rev-parse', 'HEAD']), pin); assert.equal(git(['status', '--porcelain', '--untracked-files=no']), ''); };
clean();
const simUrl = pathToFileURL(`${reference}/shared/sim.js`).href;
registerHooks({ load(url, context, next) {
  const loaded = next(url, context);
  if (url !== simUrl) return loaded;
  const source = String(loaded.source), marker = '  for (const u of list) if (u.hp <= 0) {';
  assert.equal(source.split(marker).length, 2);
  const start = source.indexOf(marker), end = source.indexOf('\n  // Annihilation: a player is out', start);
  assert.ok(start >= 0 && end > start);
  return { ...loaded, source: source + `\nexport {antiAir,updateGrid,rebuildGrid};\nexport function originalRetirement(g,list) {${source.slice(start, end)}}\n` };
} });
const S = await import(simUrl);
const { STORY_KEYS } = await import(pathToFileURL(`${reference}/shared/story.js`));
const map = { name: 'Aircraft retirement arena', w: 40, h: 40, rows: Array(40).fill('.'.repeat(40)),
  spawns: [{ x: 3, y: 3 }, { x: 36, y: 36 }], points: [{ id: 'front', x: 20, y: 20 }] };
const bits = value => { const b = Buffer.alloc(8); b.writeDoubleBE(value); return b.toString('hex'); };
const snapshot = g => ({ units: [...g.units.values()].map(u => [u.id, bits(u.hp), bits(u.x), bits(u.z), u.air?.state ?? null]),
  wallets: g.players.map(p => bits(p.mp)), story: g.story.map(p => STORY_KEYS.map(k => bits(p[k]))),
  chars: g.chars.join(''), wrecks: g.wrecks.map(w => w.id), seed: g.seed >>> 0, nextId: g.nextId });
const cases = [];
function record(name, roster, { order, out = [], aa = false } = {}) {
  const random = Math.random; Math.random = () => .5;
  let g; try { g = S.createGame(map, ['A', 'B'], false, [0, 1], [0, 1], { weather: false, logistics: false, tech: false }); } finally { Math.random = random; }
  g.units.clear(); g.nextId = 1000; g.tick = 10; g.seed = 123; g.shots = []; g.cellJournal = [];
  g.players.forEach((p, slot) => { p.mp = 100; p.out = out.includes(slot); p.visible = new Set(); });
  g.story = g.players.map(() => Object.fromEntries(STORY_KEYS.map(key => [key, 0])));
  const inputs = [];
  for (const spec of roster) {
    const u = S.internals.spawnUnit(g, spec.owner, spec.kind);
    if (spec.hp !== undefined) u.hp = spec.hp;
    if (spec.lastHit !== undefined) u.lastHit = spec.lastHit;
    if (spec.at) { Object.assign(u, spec.at); S.updateGrid(g, u); }
    if (spec.state) u.air.state = spec.state;
    u.still = 2;
    inputs.push({ id: u.id, kind: u.type, owner: u.owner, x: u.x, z: u.z, hp: u.hp, rot: u.rot, still: u.still,
      lastHit: u.lastHit ?? null, air: u.air ? structuredClone(u.air) : null });
  }
  S.rebuildGrid(g);
  if (aa) {
    for (const p of g.players) p.visible = new Set([...g.units.values()].filter(u => u.owner !== p.slot).map(u => u.id));
    S.antiAir(g, [...g.units.values()], .05);
    assert.equal(g.story[1].planesDowned, 1);
    assert.ok([...g.units.values()].find(u => u.air).hp <= 0);
  }
  const before = snapshot(g), shotsBefore = structuredClone(g.shots), current = [...g.units.keys()];
  const captured = order ? order.map(index => current[index]) : current;
  const dead = captured.filter(id => g.units.get(id).hp <= 0);
  S.originalRetirement(g, captured.map(id => g.units.get(id)));
  assert.deepEqual(g.shots, shotsBefore, 'ordinary AIR/infantry retirement adds no hull or AA event');
  assert.deepEqual(g.wrecks, []);
  const after = snapshot(g);
  S.originalRetirement(g, captured.map(id => g.units.get(id)).filter(Boolean));
  assert.deepEqual(snapshot(g), after, 'deleted actors cannot be retired a second time');
  cases.push({ name, units: inputs, order: captured, out, aa, before, dead, after });
}
record('living off-map Base aircraft remains admitted', [{ kind: 'fighter', owner: 0 }, { kind: 'rifle', owner: 1 }]);
record('dead off-map Base aircraft pays enemy bounty once', [{ kind: 'fighter', owner: 0, hp: 0, lastHit: 1 }, { kind: 'bomber', owner: 1 }]);
record('dead stationed aircraft makes no ground hull', [{ kind: 'bomber', owner: 1, hp: -2, lastHit: 0, state: 'station', at: { x: 22, z: 22 } }]);
record('friendly aircraft death has no enemy bounty', [{ kind: 'attacker', owner: 0, hp: 0, lastHit: 0 }]);
record('out enemy gets death story but no bounty', [{ kind: 'fighter', owner: 0, hp: 0, lastHit: 1 }], { out: [1] });
record('mixed captured order preserves live aircraft and ordinary losses', [{ kind: 'fighter', owner: 0 }, { kind: 'attacker', owner: 1, hp: 0, lastHit: 0 }, { kind: 'rifle', owner: 0, hp: 0, lastHit: 1 }], { order: [2, 0, 1] });
record('real AA plane-down precedes ordinary retirement once', [{ kind: 'flak', owner: 1, at: { x: 21, z: 21 } }, { kind: 'fighter', owner: 0, hp: 1, state: 'station', at: { x: 22, z: 21 } }], { aa: true });
clean();
const path = `${root}/rust/crates/simtest/tests/fixtures/air-retirement.json`;
const data = JSON.stringify({ reference: pin, map, cases }) + '\n';
if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), data); else writeFileSync(path, data);
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} ${cases.length} original AIR retirement cases`);
