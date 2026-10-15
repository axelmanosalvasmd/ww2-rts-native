// Isolated original Tutorial definitions and completion callbacks. Not a full match.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
function clean() {
  assert.equal(process.version, 'v24.21.0');
  assert.equal(process.arch, 'x64');
  assert.equal(execFileSync('git', ['rev-parse', 'HEAD'], { cwd: reference, encoding: 'utf8' }).trim(), pin);
  assert.equal(execFileSync('git', ['status', '--porcelain', '--untracked-files=no'], { cwd: reference, encoding: 'utf8' }).trim(), '');
}
clean();
const source = readFileSync(`${reference}/shared/tutorial.js`, 'utf8');
// Export the private context adapter without changing any original function body.
const { STEPS, begin, checks } = await import(`data:text/javascript;base64,${Buffer.from(`${source}\nexport { begin, checks };\n`).toString('base64')}`);
assert.equal(STEPS.length, 9);
const definitions = STEPS.map((s, step) => {
  const born = [], commands = new Map();
  const home = { x: 10, z: 20 };
  const g = {
    mode: { step, slot: 1, lines: [], spawned: [] },
    players: [{ team: 0, mp: 0 }, { team: 1, mp: 0, spawn: home }, { team: 0, mp: 500 }],
  };
  // Declared spawn/command hooks isolate original begin(), not real actor creation.
  begin(g, {
    CELL: 2,
    spawnUnit(game, owner, kind, index) {
      assert.equal(index, 0);
      const unit = { id: born.length + 1, type: kind, hp: 101, x: game.players[owner].spawn.x + 4, z: game.players[owner].spawn.z };
      born.push(unit);
      return unit;
    },
    command(game, owner, command) {
      assert.equal(game, g);
      assert.equal(owner, 1);
      const id = command.t === 'amove' ? command.orders[0][0] : command.ids[0];
      const at = command.t === 'amove' ? { x: command.orders[0][1], z: command.orders[0][2] } : { x: command.x, z: command.z };
      commands.set(id, { kind: command.t, at });
    },
  });
  assert.equal(g.players[1].spawn, home, 'temporary enemy home restored');
  assert.equal(g.players[0].mp, s.mp ?? 0, 'floor given to low team-zero wallet');
  assert.equal(g.players[2].mp, 500, 'higher allied wallet retained');
  assert.equal(g.players[1].mp, 0, 'enemy wallet unchanged');
  assert.deepEqual(g.mode.spawned, born.map(u => u.id));
  return {
    goal: g.mode.goal, at: { x: g.mode.lines[0].x, z: g.mode.lines[0].z }, manpowerFloor: s.mp ?? null,
    spawns: born.map((u, index) => {
      const healthShare = s.spawn[index][3]?.hp ?? null;
      assert.equal(u.hp, Math.ceil(101 * (healthShare ?? 1)));
      return { kind: u.type, at: { x: u.x, z: u.z }, healthShare, autoRetreat: u.autoRetreat, order: commands.get(u.id) ?? null };
    }),
  };
});

function base() {
  return {
    players: [{ team: 0, artillery: 0 }, { team: 1, artillery: 0 }, { team: 0, artillery: 0 }],
    units: [
      { id: 1, owner: 0, kind: 'rifle', hp: 100, x: 0, retreating: false },
      { id: 2, owner: 2, kind: 'rifle', hp: 100, x: 24, retreating: false },
      { id: 10, owner: 1, kind: 'rifle', hp: 100, x: 49, retreating: false },
      { id: 11, owner: 1, kind: 'rifle', hp: 100, x: 49, retreating: false },
    ],
    points: [-1, -1], spawned: [10, 11],
  };
}
const cases = [];
function add(step, name, change, expected) {
  const input = base();
  change(input);
  const g = {
    players: input.players.map(p => ({ team: p.team, sup: { artillery: p.artillery } })),
    units: new Map(input.units.map(u => [u.id, { ...u, type: u.kind }])),
    points: input.points.map(owner => ({ owner })),
  };
  const observed = STEPS[step].done(checks(g, { spawned: input.spawned }, 2));
  assert.equal(observed, expected, `${step + 1}:${name}: Source boundary`);
  cases.push({ step, name, input, expected: observed });
}
add(0, 'before-threshold', () => {}, false);
add(0, 'at-threshold', v => { v.units[0].x = 24; }, true);
add(0, 'below-threshold', v => { v.units[0].x = 23.999999999999996; }, false);
add(0, 'ally-behind', v => { v.units[0].x = 24; v.units[1].x = 0; }, false);
add(0, 'dead-behind', v => { v.units[0].hp = 0; }, true);
add(0, 'empty-team-zero', v => { v.units = v.units.filter(u => u.owner === 1); }, true);
for (const step of [1, 4, 5]) {
  add(step, 'both-alive', () => {}, false);
  add(step, 'one-dead', v => { v.units[2].hp = 0; }, false);
  add(step, 'both-dead', v => { v.units[2].hp = 0; v.units[3].hp = -1; }, true);
  add(step, 'retired-and-dead', v => { v.units = v.units.filter(u => u.id !== 10); v.units.at(-1).hp = 0; }, true);
  add(step, 'both-retired-unrelated-enemy-lives', v => { v.units = v.units.filter(u => u.id < 10); v.units.push({ id: 99, owner: 1, kind: 'tank', hp: 500, x: 0, retreating: false }); }, true);
  add(step, 'no-spawned-ids', v => { v.spawned = []; }, true);
}
for (const [step, point] of [[2, 0], [8, 1]]) {
  add(step, 'neutral', () => {}, false);
  add(step, 'enemy', v => { v.points[point] = 1; }, false);
  add(step, 'slot-zero', v => { v.points[point] = 0; }, true);
  add(step, 'allied-slot', v => { v.points[point] = 2; }, true);
  add(step, 'other-point-held', v => { v.points[1 - point] = 0; }, false);
}
add(3, 'no-at', () => {}, false);
add(3, 'enemy-at', v => { v.units[2].kind = 'at'; }, false);
add(3, 'dead-at', v => { v.units[0].kind = 'at'; v.units[0].hp = 0; }, false);
add(3, 'owned-at', v => { v.units[0].kind = 'at'; }, true);
add(3, 'allied-at', v => { v.units[1].kind = 'at'; }, true);
add(6, 'no-retreat', () => {}, false);
add(6, 'enemy-retreat', v => { v.units[2].retreating = true; }, false);
add(6, 'dead-retreat', v => { v.units[0].retreating = true; v.units[0].hp = 0; }, false);
add(6, 'owned-retreat', v => { v.units[0].retreating = true; }, true);
add(6, 'allied-retreat', v => { v.units[1].retreating = true; }, true);
add(7, 'no-call', () => {}, false);
add(7, 'enemy-call', v => { v.players[1].artillery = 30; }, false);
add(7, 'zero-cooldown', v => { v.players[0].artillery = 0; }, false);
add(7, 'negative-cooldown', v => { v.players[0].artillery = -1; }, false);
add(7, 'owned-call', v => { v.players[0].artillery = 0.000001; }, true);
add(7, 'allied-call', v => { v.players[2].artillery = 30; }, true);
clean();
const fixture = {
  format: 1, reference: pin, kind: 'tutorial-step-components-not-match-runtime',
  sourceSha256: createHash('sha256').update(source).digest('hex'), definitions, cases,
};
const output = Buffer.from(`${JSON.stringify(fixture)}\n`);
const path = `${root}/rust/crates/simtest/tests/fixtures/tutorial-steps.json`;
if (process.argv.includes('--check')) assert.ok(readFileSync(path).equals(output), 'Tutorial fixture differs');
else writeFileSync(path, output);
console.log(`Tutorial steps: ${definitions.length} definitions and ${cases.length} original completion callbacks`);
