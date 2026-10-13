// Record legacy migration, setup and timed execution from the pinned original engine.
// Requires x64 Node v24.21.0. --check compares without rewriting the fixture.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
assert.equal(process.arch, 'x64'); assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(a => a === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? join(root, 'reference/ww2-rts'));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
function clean() { assert.equal(git(['rev-parse', 'HEAD']), pin); assert.equal(git(['status', '--porcelain', '--untracked-files=no']), ''); }
clean();
const { validateMap } = await import(pathToFileURL(join(reference, 'shared/sim.js')));
const { setupScenario, stepScenario } = await import(pathToFileURL(join(reference, 'shared/scenarios.js')));
const cases = [];
const base = triggers => ({ name: 'Legacy triggers', w: 40, h: 40, rows: Array(40).fill('.'.repeat(40)), spawns: [{ x: 0, y: 0 }, { x: 39, y: 39 }], points: [{ x: 0, y: 1 }, { id: 'named:point', x: 2, y: 1 }], triggers });
const defaultSteps = [0, 1, 59, 60, 61, 6000, 8000, 8400, 8400, 1, 144000].map(tick => ({ tick }));
function record(name, map, steps = defaultSteps, valid = true) {
  const error = validateMap(map);
  assert.equal(!error, valid, `${name}: ${error}`);
  const input = { name, map, error, points: (map.points ?? []).map((p, i) => ({ id: p.id ?? `point:${i}`, owner: -1 })), steps: [] };
  if (!error) {
    const events = [], shots = [];
    shots.push = (...rows) => { for (const row of rows) events.push({ kind: 'say', row: structuredClone(row) }); return Array.prototype.push.apply(shots, rows); };
    const g = { points: input.points.map(p => ({ scenarioId: p.id, owner: p.owner })), players: [{ slot: 0, team: 0 }, { slot: 1, team: 1 }], units: new Map(), shots, tick: 0 };
    const tools = { now: g => g.tick * (1 / 20), allowed: () => true, arrive: () => null, damage: (g, action, execution) => events.push({ kind: 'damage', action: structuredClone(action), execution }) };
    setupScenario(g, map, tools);
    input.initial = g.scenario ? structuredClone(g.scenario) : null;
    assert.equal(events.length, 0, 'setup must not emit actions');
    for (const step of steps) {
      g.tick = step.tick;
      if (step.points) g.points = step.points.map(p => ({ scenarioId: p.id, owner: p.owner }));
      events.length = 0;
      stepScenario(g, tools);
      input.steps.push({ ...step, expected: g.scenario ? structuredClone(g.scenario) : null, events: structuredClone(events) });
    }
  }
  cases.push(input);
}
for (const name of readdirSync(join(reference, 'maps')).filter(n => n.endsWith('.json')).sort()) {
  const map = JSON.parse(readFileSync(join(reference, 'maps', name), 'utf8'));
  if (map.triggers?.length) record(`authored:${name}`, map);
}
assert.equal(cases.length, 5);
record('valid:absent-triggers', (() => { const m = base([]); delete m.triggers; return m; })());
record('valid:empty-triggers', base([]));
record('valid:zero-fractional-and-simultaneous', base([{ at: 1, say: 'first in source' }, { at: 0, say: 'zero', blow: [0, 0, 1, 1] }, { at: 0.025, blow: [2, 2, 2, 2] }, { at: 1, say: 'last in source' }]), [{ tick: 0 }, { tick: 0 }, { tick: 1 }, { tick: 20 }, { tick: 20 }, { tick: 0 }, { tick: 144000 }]);
record('valid:first-step-time-jump', base([{ at: 30, say: 'later comes first' }, { at: 0, blow: [0, 0, 0, 0] }, { at: 5, say: '   ' }]), [{ tick: 144000 }, { tick: 144001 }]);
record('valid:owners-captured-missing-and-returning', base([{ at: 1, say: 'owners' }]), [
  { tick: 0 }, { tick: 1, points: [{ id: 'point:0', owner: 0 }, { id: 'named:point', owner: 1 }] },
  { tick: 2, points: [{ id: 'point:0', owner: -1 }, { id: 'named:point', owner: 0 }] },
  { tick: 3, points: [] }, { tick: 4, points: [{ id: 'point:0', owner: 0 }, { id: 'named:point', owner: -1 }] },
  { tick: 20 }, { tick: 21, points: [{ id: 'point:0', owner: 0 }, { id: 'named:point', owner: 1 }] },
]);
{ const map = base([{ at: 0, say: 'reserved target' }]); map.points[1].id = 'region:test'; record('valid:region-prefixed-point', map, [{ tick: 0 }, { tick: 1, points: [{ id: 'point:0', owner: 0 }, { id: 'region:test', owner: 1 }] }]); }
record('valid:maximum-tick-work', base(Array.from({ length: 15 }, (_, i) => ({ at: 0, say: `full ${i}`, blow: [0, 0, 15, 15] }))), [{ tick: 0 }, { tick: 1 }]);
record('valid:twenty-triggers', base(Array.from({ length: 20 }, (_, i) => ({ at: i, say: `trigger ${i}` }))), [{ tick: 400 }, { tick: 401 }]);
record('valid:last-exact-integer-tick', base([{ at: 7200, say: 'last' }]), [{ tick: 9007199254740991 }]);
for (const [name, triggers] of [
  ['empty-text', [{ at: 0, say: '' }]], ['empty-text-with-destroy', [{ at: 0, say: '', blow: [0, 0, 0, 0] }]],
  ['missing-action', [{ at: 0 }]], ['negative-time', [{ at: -1, say: 'bad' }]], ['late-time', [{ at: 7200.01, say: 'bad' }]],
  ['too-many-triggers', Array.from({ length: 21 }, () => ({ at: 0, say: 'bad' }))],
  ['box-over-budget', [{ at: 0, blow: [0, 0, 16, 15] }]],
  ['total-over-budget', Array.from({ length: 16 }, () => ({ at: 0, say: 'bad', blow: [0, 0, 15, 15] }))],
]) record(`invalid:${name}`, base(triggers), [], false);
clean();
const content = JSON.stringify({ reference: pin, node: process.version, arch: process.arch, cases }) + '\n';
const path = join(root, 'rust/crates/core/tests/fixtures/legacy-scenarios.json');
if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), content); else writeFileSync(path, content);
console.error(`${process.argv.includes('--check') ? 'Verified' : 'Recorded'} ${cases.length} legacy cases (${cases.filter(c => !c.error).length} valid, ${cases.filter(c => c.error).length} invalid), ${cases.reduce((n, c) => n + c.steps.length, 0)} steps`);
