// Complete original MOV-13 callback. Execution requires a separate Root slot.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {readFileSync, writeFileSync} from 'node:fs';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {gzipSync} from 'node:zlib';
import {encode} from './state-codec.mjs';
const root = fileURLToPath(new URL('../', import.meta.url));
const ref = process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`;
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
const args = process.argv.slice(2), checking = args.includes('--check');
const paths = args.filter(a => a !== '--check');
assert.equal(paths.length, 1);
assert.equal(args.length, 1 + Number(checking));
const target = paths[0];
assert.ok(target.startsWith('/tmp/'));
const git = args => execFileSync('git', ['-C', ref, ...args], {encoding: 'utf8'}).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const raw = await import(pathToFileURL(`${ref}/shared/sim.js`));
const source = readFileSync(`${ref}/test.js`, 'utf8');
const name = 'Sight rays ending on a cell border must stop at the destination, including mixed-sign diagonals';
const start = `await check(${JSON.stringify(name)},`;
assert.equal(source.split(start).length, 2);
const first = source.indexOf(start), last = source.indexOf('\n});', first);
assert.ok(last > first);
const body = source.slice(first, last) + '\n});';
const hash = text => createHash('sha256').update(text).digest('hex');
const packet = {reference: pin, node: process.version, arch: process.arch,
  name, body, bodySha256: hash(body), sourceSha256: hash(source), constructors: [], queries: [], completed: false};
const sim = {...raw};
sim.createGame = (...args) => {
  const row = {input: structuredClone(args), completed:false};
  packet.constructors.push(row);
  try {
    const g = raw.createGame(...args);
    Object.assign(row, {state: encode(g, {runs: true}), w:g.w, h:g.h,
      flags:[...g.flags], height:g.height ? [...g.height] : null,
      smokes:structuredClone(g.smokes), completed:true});
    return g;
  } catch (error) {
    row.failure = {name:error.name,message:error.message,stack:error.stack};
    throw error;
  }
};
const los = (g, a, b) => {
  // All query inputs are captured; expected booleans are never setup data.
  const row = {a: structuredClone(a), b: structuredClone(b), w:g.w, h:g.h, completed:false, flags: [...g.flags],
    height: g.height ? [...g.height] : null, smokes: structuredClone(g.smokes)};
  packet.queries.push(row);
  try {
    row.expected = raw.los(g, a, b); row.completed = true;
    return row.expected;
  } catch (error) {
    row.failure = {name:error.name,message:error.message,stack:error.stack};
    throw error;
  }
};
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const previous = Math.random;
let failure, invoked = 0;
try {
  Math.random = () => 0.5;
  const check = async (actual, fn) => { assert.equal(actual, name); invoked++; await fn(); };
  await new AsyncFunction('bindings', `with (bindings) {\n${body}\n}`)
    ({assert, check, sim, createGame: sim.createGame, los});
  assert.equal(invoked, 1);
  assert.equal(packet.constructors.length, 1);
  assert.equal(packet.queries.length, 244);
  packet.completed = true;
} catch (error) {
  failure = error;
  packet.failure = {name: error.name, message: error.message, stack: error.stack};
} finally {
  Math.random = previous;
  try { clean(); } catch (error) { failure ??= error; packet.readbackFailure = {message: error.message, stack: error.stack}; }
  const bytes = gzipSync(JSON.stringify(packet) + '\n', {level: 9});
  if (failure) writeFileSync(checking ? `${target}.failed.gz` : target, bytes);
  else if (checking) {
    if (!readFileSync(target).equals(bytes)) writeFileSync(`${target}.different.gz`, bytes);
    assert.deepEqual(readFileSync(target), bytes);
  } else writeFileSync(target, bytes);
}
if (failure) throw failure;
console.log('Complete original MOV-13 callback: 244 line-of-sight queries; native proof remains separate');
