// Private Source-only draft. Execution requires Root's explicit recorder slot.
// Keep complete original callbacks, including assertions outside the selected rows.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {captureWriter, finishCapture, format} from './mov-original-capture-stream.mjs';
import {encode} from './state-codec.mjs';

const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
const args = process.argv.slice(2);
const checking = args.includes('--check'), paths = args.filter(a => a !== '--check');
assert.equal(paths.length, 1, 'one explicit /tmp output path is required');
assert.equal(args.length, 1 + Number(checking));
const output = resolve(paths[0]);
assert.ok(output.startsWith('/tmp/'), 'draft evidence stays outside the candidate tree');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const git = args => execFileSync('git', ['-C', reference, ...args], {encoding: 'utf8'}).trim();
function clean() {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
}
clean();
const raw = await import(pathToFileURL(`${reference}/shared/sim.js`));
const fixtures = await import(pathToFileURL(`${reference}/test-fixtures.js`));
const {generateWorldMap} = await import(pathToFileURL(`${reference}/shared/world-conquest.js`));
const source = readFileSync(`${reference}/test.js`, 'utf8');
const worldSource = readFileSync(`${reference}/test-world-rules.mjs`, 'utf8');
const hash = value => createHash('sha256').update(value).digest('hex');
function section(text, start, end) {
  assert.equal(text.split(start).length, 2, `unique start: ${start}`);
  const a = text.indexOf(start), b = text.indexOf(end, a + start.length);
  assert.ok(b > a, `end: ${end}`);
  return text.slice(a, b);
}
const helpers = section(source, 'const blank = ', '\nawait check(');
const worldHelpers = section(worldSource, 'const fresh = ', '\ntest(');
const selected = ['Roads and mud', 'Ground that wears', 'Slope', 'Wind, weather, dust and fire']
  .map(name => ({name, file: 'test.js', helpers,
    body: section(source, `await check(${JSON.stringify(name)},`, '\n});') + '\n});'}));
selected.push({name: 'road march: calm units move faster on their own land',
  file: 'test-world-rules.mjs', helpers: worldHelpers,
  body: section(worldSource, "test('road march: calm units move faster on their own land',", '\n});') + '\n});'});

const buffer = new DataView(new ArrayBuffer(8));
function bits(n) {
  buffer.setFloat64(0, n, false);
  return buffer.getBigUint64(0, false).toString(16).padStart(16, '0');
}
function witness(g) {
  return encode({tick: g.tick, seed: g.seed, wx: g.wx, wind: g.wind,
    units: [...g.units.values()], players: g.players.map(p => ({slot: p.slot, team: p.team,
      spawn: p.spawn, mp: p.mp, mun: p.mun, out: p.out})),
    motionBits: [...g.units.values()].map(u => [u.id, ...[u.x, u.z, u.rot, u.moveSpeed].map(bits)]),
    chars: g.chars, flags: g.flags, height: g.height, wear: g.wear,
    regions: g.world?.regions, winner: g.winner}, {runs: true});
}
let active;
const ids = new WeakMap();
let nextGame;
let nextCall;
function observe(op, g, input, call) {
  const id = nextCall++;
  capture.write({type: 'action-start', case: caseIndex, call: id,
    event: {op, game: ids.get(g), input: encode(input), before: witness(g)}});
  try {
    const result = call();
    capture.write({type: 'action-end', case: caseIndex, call: id, op, game: ids.get(g), outcome: 'return',
      event: {result: encode(result), after: witness(g)}});
    return result;
  } catch (error) {
    capture.write({type: 'action-end', case: caseIndex, call: id, op, game: ids.get(g), outcome: 'throw',
      event: {failure: {name: error.name, message: error.message, stack: error.stack}, after: witness(g)}});
    throw error;
  }
}
const sim = {...raw};
sim.createGame = (...args) => {
  const g = raw.createGame(...args);
  ids.set(g, nextGame++);
  capture.write({type: 'create', case: caseIndex, call: nextCall++,
    event: {op: 'createGame', game: ids.get(g), input: encode(args), initial: encode(g, {runs: true})}});
  return g;
};
sim.step = g => observe('step', g, null, () => raw.step(g));
sim.findPath = (g, from, to) => observe('findPath', g, {from, to}, () => raw.findPath(g, from, to));
sim.snapshotFor = (g, ...args) => observe('snapshotFor', g, args, () => raw.snapshotFor(g, ...args));
const command = (g, owner, cmd) => observe('fixtureCommand', g, {owner, cmd},
  () => fixtures.fixtureCommand(g, owner, cmd));
const publicCommand = (g, owner, cmd) => observe('command', g, {owner, cmd},
  () => raw.command(g, owner, cmd));
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const packet = {reference: pin, node: process.version, arch: process.arch,
  scope: 'Original Source checks only; no native replay or parity promotion.',
  globalRandom: 'constant 0.5, restored after capture',
  sourceHashes: {'test.js': hash(source), 'test-world-rules.mjs': hash(worldSource),
    'test-fixtures.js': hash(readFileSync(`${reference}/test-fixtures.js`))}};
const partial = `${output}.partial.gz`;
const capture = captureWriter(partial);
capture.write({type: 'header', format, caseCount: selected.length, packet});
let caseIndex = -1, completedCases = 0;
const previousRandom = Math.random;
let failure;
try {
  Math.random = () => 0.5;
  for (const spec of selected) {
    nextGame = 1;
    caseIndex++; nextCall = 0;
    active = {...spec, sha256: hash(spec.helpers + '\n' + spec.body), completed: false};
    capture.write({type: 'case-start', case: caseIndex, spec: active});
    let invoked = 0;
    const check = async (name, fn) => { assert.equal(name, spec.name); invoked++; await fn(); };
    const test = (name, fn) => { assert.equal(name, spec.name); invoked++; fn(); };
    await new AsyncFunction('bindings', `with (bindings) {\n${spec.helpers}\n${spec.body}\n}`)
      ({...sim, sim, assert, check, test, generateWorldMap,
        clearFixtureUnits: fixtures.clearFixtureUnits,
        command: spec.file === 'test.js' ? command : publicCommand});
    assert.equal(invoked, 1);
    active.completed = true;
    capture.write({type: 'case-end', case: caseIndex, calls: nextCall, completed: true});
    completedCases++;
  }
} catch (error) {
  failure = error;
  packet.failure = {name: error.name, message: error.message, stack: error.stack};
  if (active && !active.completed) capture.write({type: 'case-end', case: caseIndex, calls: nextCall, completed: false});
} finally {
  Math.random = previousRandom;
  try { clean(); } catch (error) {
    packet.sourceReadbackFailure = {name: error.name, message: error.message, stack: error.stack};
    failure ??= error;
  }
  // Completed records are already on disk, including before/after failure evidence.
  capture.write({type: 'footer', cases: caseIndex + 1,
    ...(packet.failure ? {failure: packet.failure} : {}),
    ...(packet.sourceReadbackFailure ? {sourceReadbackFailure: packet.sourceReadbackFailure} : {})});
  capture.close();
  finishCapture(partial, output, checking, Boolean(failure));
}
if (failure) throw failure;
console.log(`${completedCases} original Source checks completed; native consumers remain pending`);
