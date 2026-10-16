// Complete selected Source callbacks plus ordinary-command companion inputs.
// Source capture requires a new exact Root grant. This file is not a result.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { gzipSync } from 'node:zlib';
import { encode } from './state-codec.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(root, 'reference/ww2-rts');
const inputsText = readFileSync(resolve(root, 'harness/mov-ordinary-inputs.json'), 'utf8');
const inputs = JSON.parse(inputsText);
assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), inputs.reference);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const raw = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const fixtures = await import(pathToFileURL(resolve(reference, 'test-fixtures.js')));
const source = readFileSync(resolve(reference, 'test.js'), 'utf8');
const engine = readFileSync(resolve(reference, 'test-engine-movement.js'), 'utf8');
const hash = text => createHash('sha256').update(text).digest('hex');
const slice = (text, start, end) => {
  assert.equal(text.split(start).length, 2, `unique Source boundary: ${start}`);
  const a = text.indexOf(start), b = text.indexOf(end, a + start.length);
  assert.ok(b > a);
  return text.slice(a, b);
};
const helpers = slice(source, 'const blank = ', '\nawait check(');
const field = slice(source, '\nconst field = ', '\n').trimStart();
assert.equal(helpers + '\n' + field, inputs.originalHelpers);
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const originals = [], companions = [];
const randomReceipts = [];
let randomState = inputs.sourceRandom.seed >>> 0;
const savedRandom = Math.random;
// This is the declared harness stream. Never restore a birth's RNG or fields.
Math.random = () => {
  randomState = (Math.imul(1664525, randomState) + 1013904223) >>> 0;
  randomReceipts.push(randomState);
  return randomState / 2 ** 32;
};
const distance = (a, b) => Math.hypot(a.x - b.x, a.z - b.z);
const pt = a => ({ x: a[0], z: a[1] });
const cfg = raw.CFG;
// Read the pinned private pure expression unchanged. In these open-ground
// scenes, from=null and no trench branch needs an unexported dependency.
const simSource = readFileSync(resolve(reference, 'shared/sim.js'), 'utf8');
const coverSource = slice(simSource, 'const coverRank = ', '\n');
assert.equal(coverSource, inputs.coverRankSource);
const coverRank = new Function('bindings', `with (bindings) { ${coverSource} return coverRank; }`)
  ({ ...raw, flagsAt: raw.internals.flagsAt });
let originalActive;
const observeOriginal = (op, g, input, invoke) => {
  const before = encode(g, { runs: true });
  const result = invoke();
  originalActive.calls.push({ op, input: encode(input), before,
    after: encode(g, { runs: true }), result: encode(result) });
  return result;
};
const originalStep = g => observeOriginal('step', g, null, () => raw.step(g));
const originalCommand = (g, owner, command) => observeOriginal('fixtureCommand', g, { owner, command },
  () => fixtures.fixtureCommand(g, owner, command));

class Scene {
  constructor(spec, cap = inputs.defaultRetries) {
    this.spec = spec;
    this.events = [];
    this.commands = [];
    this.pending = [];
    this.randomState = inputs.sourceRandom.seed >>> 0;
    this.randomReceipts = [];
    assert.equal(raw.validateMap(spec.map), null);
    this.g = this.withRandom(() => raw.createGame(spec.map, ['A', 'B'], false, [0, 0], [0, 1],
      { weather: false, supply: false, tech: false, logistics: false }));
    this.g.pathBudget = inputs.pathBudget;
    // Twenty exercises the actual absent-field default at sim.js:3226.
    if (cap !== inputs.defaultRetries) this.g.pathRetryLimit = cap;
    assert.equal(Object.hasOwn(this.g, 'pathRetryLimit'), cap !== inputs.defaultRetries);
    this.g.players[0].mp = inputs.wallet;
    this.births = encode([...this.g.units.values()]);
    for (let owner = 0; owner < 2; owner++) this.issue({ t: 'holdPos',
      ids: [...this.g.units.values()].filter(u => u.owner === owner && !raw.UNITS[u.type].structure).map(u => u.id), on: true }, owner);
    this.step();
  }
  withRandom(invoke) {
    const previous = Math.random;
    Math.random = () => {
      this.randomState = (Math.imul(1664525, this.randomState) + 1013904223) >>> 0;
      this.randomReceipts.push(this.randomState);
      return this.randomState / 2 ** 32;
    };
    try { return invoke(); } finally { Math.random = previous; }
  }
  issue(command, owner = 0) { this.pending.push({ owner, command }); }
  step() {
    for (const { owner, command } of this.pending.splice(0)) {
      console.error('MOV_SOURCE_PRE_COMMAND ' + JSON.stringify(encode({
        scene: this.spec, owner, command, sequence: this.commands.length + 1,
        tick: this.g.tick, winner: this.g.winner, players: this.g.players,
        units: this.g.units, chars: this.g.chars, flags: this.g.flags, height: this.g.height,
        pathRetryLimitPresent: Object.hasOwn(this.g, 'pathRetryLimit'), pathRetryLimit: this.g.pathRetryLimit,
        pathStats: this.g.pathStats, randomState: this.randomState, randomReceipts: this.randomReceipts,
      }, { runs: true })));
      assert.equal(this.withRandom(() => raw.command(this.g, owner, command)), undefined, 'real Source command admission');
      this.commands.push({ tick: this.g.tick + 1, owner, sequence: this.commands.length + 1, command });
    }
    const before = this.g.pathStats?.failed ?? 0;
    this.withRandom(() => raw.step(this.g));
    this.events.push({ tick: this.g.tick, stats: structuredClone(this.g.pathStats),
      units: [...this.g.units.values()].map(u => encode({ id: u.id, x: u.x, z: u.z,
        hp: u.hp, react: u.react, repath: u.repath, path: u.path, drift: u.drift,
        worldGoal: u.worldGoal, amove: u.amove, orders: u.orders,
        moveOutcome: u.moveOutcome, moveOutcomeTick: u.moveOutcomeTick, moveResult: u.moveResult })) });
    return (this.g.pathStats?.failed ?? 0) - before;
  }
  rifles(count) {
    const ids = [...this.g.units.values()].filter(u => u.owner === 0 && u.type === 'rifle').map(u => u.id);
    const hq = [...this.g.units.values()].find(u => u.owner === 0 && u.type === 'hq');
    while (ids.length < count) {
      const before = this.g.nextId;
      this.issue({ t: 'buy', unit: 'rifle', from: hq.id }); this.step();
      const born = [...this.g.units.values()].filter(u => u.id >= before && u.owner === 0 && u.type === 'rifle');
      assert.equal(born.length, 1, 'canonical HQ purchase');
      this.issue({ t: 'holdPos', ids: [born[0].id], on: true }); this.step();
      ids.push(born[0].id);
    }
    return ids.slice(0, count);
  }
  move(goals, amove = false, queue = false) {
    this.issue({ t: amove ? 'amove' : 'move', orders: goals.map(([id, at]) => [id, at.x, at.z]), queue });
  }
  stage(ids) {
    const goals = ids.map((id, i) => [id, pt(this.spec.starts[i])]);
    this.move(goals);
    for (let n = 0; n < inputs.stageTicks; n++) {
      this.step();
      if (goals.every(([id, at]) => {
        const u = this.g.units.get(id);
        return distance(u, at) <= inputs.stageTolerance && !u.path.length && !u.worldGoal;
      })) return;
    }
    assert.fail('real commands did not reach the declared measurement arrangement');
  }
  release(ids) { this.issue({ t: 'holdPos', ids, on: false }); this.step(); }
  packet() { return raw.snapshotFor(this.g, 0, [], [], raw.snapshotCache(this.g)); }
  receipt(name, ids, result) {
    companions.push({ name, inputs: this.spec, ids, births: this.births, commands: this.commands,
      events: this.events, randomReceipts: this.randomReceipts, randomState: this.randomState,
      result: encode(result), assertionsCompleted: true });
  }
}

try {
  // Execute the four complete, unedited callbacks with their actual original
  // fresh/put helpers. Their authored pose/repath writes stay raw evidence only.
  for (const spec of inputs.originalChecks.slice(0, 4)) {
    const end = spec.name.startsWith('A formation spot') ? '\n} });' : '\n});';
    const body = slice(source, `await check(${JSON.stringify(spec.name)},`, end) + end;
    assert.equal(hash(body), spec.sha256);
    assert.equal(body, spec.body);
    originalActive = { ...spec, body, helpers: helpers + '\n' + field, calls: [], assertionsCompleted: false };
    let invoked = 0;
    await new AsyncFunction('bindings', `with (bindings) {\n${helpers}\n${field}\n${body}\n}`)({
      ...raw, assert, command: originalCommand, step: originalStep,
      clearFixtureUnits: fixtures.clearFixtureUnits,
      check: async (name, fn) => { assert.equal(name, spec.name); invoked++; await fn(); },
    });
    assert.equal(invoked, 1);
    originalActive.assertionsCompleted = true;
    originals.push(originalActive);
  }
  for (const spec of inputs.scenes) {
    if (spec.name === 'retry') {
      for (const cap of spec.caps) {
        const s = new Scene(spec, cap), id = s.rifles(1)[0]; s.stage([id]);
        const u = s.g.units.get(id), target = pt(spec.target), dropped = s.g.pathStats?.dropped ?? 0;
        s.move([[id, target]], true);
        let attempts = 0;
        const attemptTrace = [];
        for (let n = 0; n < cap * inputs.dropTickBudgetPerRetry + inputs.dropExtraTicks; n++) {
          const failed = s.step();
          assert.ok(failed === 0 || failed === 1, 'one actor can retry once per tick');
          attempts += failed;
          if (failed) attemptTrace.push({ tick: s.g.tick, attempt: attempts, dropped: s.g.pathStats.dropped - dropped });
          assert.equal(u.path.length, 0);
          assert.ok(distance(u, pt(spec.starts[0])) <= inputs.stageTolerance);
          if (u.moveOutcome === 'unreachable') break;
          assert.equal(s.g.pathStats.dropped, dropped);
        }
        assert.equal(attempts, cap);
        assert.equal(s.g.pathStats.dropped, dropped + 1); assert.equal(u.amove, null);
        assert.equal(u.moveOutcome, 'unreachable');
        const quietCalls = s.g.pathStats.calls;
        for (let n = 0; n < inputs.quietTicks; n++) {
          assert.equal(s.step(), 0); assert.equal(s.g.pathStats.calls, quietCalls);
        }
        s.move([[id, target]], true);
        let searches = 0;
        for (let n = 0; n < inputs.freshTicks && searches === 0; n++) searches += s.step();
        assert.equal(searches, 1); assert.ok(u.amove);
        assert.equal(s.g.pathStats.dropped, dropped + 1);
        let freshAttempts = searches;
        for (let n = 0; n < cap * inputs.dropTickBudgetPerRetry + inputs.dropExtraTicks && u.amove; n++) {
          const failed = s.step();
          assert.ok(failed === 0 || failed === 1);
          freshAttempts += failed;
          assert.equal(s.g.pathStats.dropped, dropped + (freshAttempts === cap ? 2 : 1));
        }
        assert.equal(freshAttempts, cap, 'new immediate command receives the complete allowance');
        assert.equal(u.amove, null); assert.equal(s.g.pathStats.dropped, dropped + 2);
        const queuedStart = s.g.tick + 1, queuedDrops = s.g.pathStats.dropped;
        const feedback = [];
        // Preserve the original engine block and execute its full command,
        // two-tick feedback loop and both final assertions through local bindings.
        const block = slice(engine, '  fixture();\n  const blocked =', '\n  fixture();');
        const originalSpec = inputs.originalChecks[4];
        assert.equal(hash(block), originalSpec.sha256);
        assert.equal(block, originalSpec.body);
        assert.deepEqual(spec.target, [70, 30]); assert.deepEqual(spec.queued, [20, 30]);
        const tail = block.slice(block.indexOf('  await move(blocked, 70, 30);'));
        const host = { rows: new Map(), snapshot: {} };
        const update = () => {
          host.snapshot = s.packet(); host.rows = new Map(host.snapshot.units.map(row => [row[0], row]));
          feedback.push({ tick: s.g.tick, movement: structuredClone(host.snapshot.movement) });
        };
        const executed = cap === 3 ? tail : `
          await move(blocked, 70, 30); await move(blocked, 20, 30, { queue: true });
          let recovered = false, unreachable = false;
          for (let n = 0; n < ${cap * inputs.dropTickBudgetPerRetry + inputs.queueExtraTicks} && !recovered; n += 2) {
            await tick(2); unreachable ||= host.snapshot.movement.some(m => m[0] === blocked.id && (m[1] === 'unreachable' || m[3] === 'unreachable'));
            const r = host.rows.get(blocked.id); recovered = Math.hypot(r[3] - 20, r[4] - 30) < 0.5;
          }
          assert.ok(unreachable); assert.ok(recovered);
        `;
        await new AsyncFunction('bindings', `with (bindings) {\n${executed}\n}`)({
          assert, host, blocked: u,
          move: async (actor, x, z, extra = {}) => s.move([[actor.id, { x, z }]], false, extra.queue === true),
          tick: async (count = 2) => { for (let n = 0; n < count; n++) s.step(); update(); },
        });
        // The original arrival observation is rounded and may precede clearing
        // the live goal. Retain it, then separately require complete settling.
        for (let n = 0; n < inputs.settleTicks && (u.path.length || u.worldGoal); n++) { s.step(); update(); }
        const freshFeedback = feedback.some(p => p.movement.some(row => row[0] === id
          && (row[1] === 'unreachable' && row[2] > queuedStart
            || row[3] === 'unreachable' && row[4] > queuedStart)));
        assert.ok(freshFeedback, 'new unreachable tick, not a retained earlier result');
        assert.equal(s.g.pathStats.dropped, queuedDrops + 1);
        assert.ok(distance(u, pt(spec.queued)) < 0.5 && !u.path.length && !u.worldGoal && !u.orders.length);
        if (cap === 3) originals.push({ ...originalSpec, body: block, executedBody: tail,
          adaptation: 'canonical constructor, retained actors, public HQ purchase and staging; local commands and actual recipient rows',
          assertionsCompleted: true });
        s.receipt(`retry-${cap}`, [id], { cap, attempts, dropped: s.g.pathStats.dropped - dropped,
          searches, freshAttempts, attemptTrace, quietCalls, queuedStart, feedback, freshFeedback, recovered: true });
      }
    } else if (spec.name === 'idle') {
      const actual = new Scene(spec), control = new Scene(spec);
      const ids = actual.rifles(2), controlIds = control.rifles(2);
      assert.deepEqual(ids, controlIds); actual.stage(ids); control.stage(ids);
      const starts = ids.map(id => ({ x: actual.g.units.get(id).x, z: actual.g.units.get(id).z }));
      assert.deepEqual(starts, ids.map(id => ({ x: control.g.units.get(id).x, z: control.g.units.get(id).z })));
      assert.ok(distance(starts[0], starts[1]) < 0.75);
      const actualBefore = encode(actual.g, { runs: true }), controlBefore = encode(control.g, { runs: true });
      assert.deepEqual(actualBefore, controlBefore, 'same scene inputs before the sole timer intervention');
      assert.deepEqual(actual.randomReceipts, control.randomReceipts);
      assert.deepEqual(actual.commands, control.commands);
      for (const id of ids) control.g.units.get(id).react = inputs.controlReact;
      const idleStartTick = actual.g.tick + 1;
      let routed = false, expectedTimer = inputs.controlReact;
      for (let n = 0; n < inputs.idleTicks; n++) {
        if (n === 0) { actual.release(ids); control.release(ids); }
        else { actual.step(); control.step(); }
        const b = actual.g.units.get(ids[1]); routed ||= b.drift === 'space' && b.path.length > 0;
        expectedTimer -= raw.TICK;
        for (const id of ids) {
          const c = control.g.units.get(id);
          assert.equal(c.react, expectedTimer); assert.equal(c.path.length, 0);
          assert.equal(c.hp, actual.g.units.get(id).hp);
        }
      }
      const a = actual.g.units.get(ids[0]), b = actual.g.units.get(ids[1]);
      const ca = control.g.units.get(ids[0]), cb = control.g.units.get(ids[1]);
      assert.ok(routed && distance(a, b) >= cfg.behavior.spacing - 0.25);
      assert.ok(distance(ca, cb) < cfg.behavior.spacing - 0.25);
      assert.ok(distance(b, starts[1]) <= cfg.behavior.coverSeek);
      assert.ok(coverRank(actual.g, b, null) <= coverRank(actual.g, starts[1], null));
      assert.ok(expectedTimer > 0);
      actual.receipt('idle', ids, { routed, starts, actualBefore, controlBefore,
        controlRandomReceipts: control.randomReceipts, idleStartTick, ticks: inputs.idleTicks,
        coverRanks: [coverRank(actual.g, starts[1], null), coverRank(actual.g, b, null)],
        separation: distance(a, b), controlSeparation: distance(ca, cb),
        controlCommands: control.commands, controlEvents: control.events, coverAndRadius: true });
    } else {
      const s = new Scene(spec), ids = s.rifles(spec.starts.length); s.stage(ids); s.release(ids);
      const click = spec.name === 'capture' ? s.g.points[0] : pt(spec.click);
      s.move(ids.map(id => [id, click])); s.step();
      const ends = ids.map(id => { const path = s.g.units.get(id).path; assert.ok(path.length); return path.at(-1); });
      if (spec.name === 'spreading') {
        assert.ok(ends.every(p => distance(p, click) < 12));
        for (let i = 0; i < ends.length; i++) for (const q of ends.slice(i + 1)) assert.ok(distance(ends[i], q) >= cfg.behavior.spacing);
      } else if (spec.name === 'capture') assert.ok(ends.every(p => distance(p, click) <= cfg.pointRadius));
      else assert.ok(ends.every(p => p.z < 40));
      s.receipt(spec.name, ids, { ends, click, completePredicate: true });
    }
  }
} finally { Math.random = savedRandom; clean(); }
assert.equal(originals.length, 5);
assert.deepEqual(companions.map(c => c.name), ['retry-3', 'retry-20', 'spreading', 'capture', 'river', 'cliff', 'idle']);
const output = gzipSync(JSON.stringify({ reference: inputs.reference, node: process.version, arch: process.arch,
  inputs, inputsSha256: hash(inputsText), sourceFiles: { 'test.js': hash(source), 'test-engine-movement.js': hash(engine) },
  originals, companions, randomReceipts,
  scope: 'Full original selected assertions and disclosed ordinary companions. No aligned full-match comparison or runtime qualification is inferred.' }) + '\n', { level: 9 });
const target = resolve(root, 'rust/crates/simtest/tests/fixtures/mov-ordinary-predicates.json.gz');
if (process.argv.includes('--check')) assert.deepEqual(readFileSync(target), output);
else writeFileSync(target, output);
console.log('Five complete original selected checks and seven ordinary companions captured; native qualification remains separate');
