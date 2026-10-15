// Run each complete original benchmark once. Source execution needs Root's slot.
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, chmodSync, fstatSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
const args = process.argv.slice(2);
const option = key => { const i = args.indexOf(key); assert(i >= 0 && args[i+1], key); return args[i+1]; };
const reference = resolve(option('--reference'));
const checking = args.includes('--check-dir');
const output = resolve(option(checking ? '--check-dir' : '--output-dir'));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim();
const clean = () => {
  assert.equal(git('-C', reference, 'rev-parse', 'HEAD'), pin);
  assert.equal(git('-C', reference, 'status', '--porcelain', '--untracked-files=no'), '');
  assert.equal(git('status', '--porcelain'), '', 'freeze this source-only child first');
};
clean();
if (!checking) mkdirSync(output, { mode: 0o700 });
// The caller can pass its already-held slot descriptor through both workers.
const lockFd = process.env.WW2_MOV33_LOCK_FD === undefined ? null : Number(process.env.WW2_MOV33_LOCK_FD);
if (lockFd !== null) {
  assert(Number.isInteger(lockFd) && lockFd >= 3 && lockFd <= 64);
  assert(fstatSync(lockFd).isFile(), 'caller slot lock descriptor must be open');
}
const workerStdio = lockFd === null ? undefined : Array.from({length:lockFd+1}, (_,i) =>
  i === 0 ? 'ignore' : i === 1 || i === 2 ? 'pipe' : i === lockFd ? lockFd : 'ignore');
const observer = fileURLToPath(new URL('./mov33-benchmark-observer.mjs', import.meta.url));
const manifest = { format: 'mov33-original-benchmark-v1', reference: pin,
  sourceHead: git('rev-parse', 'HEAD'), node: process.version, cases: [], failure: null };
for (const bunkers of [false, true]) {
  const name = bunkers ? 'six-bunkers' : 'no-bunkers';
  const file = `${name}.json`;
  const destination = resolve(output, checking ? `${name}.check-actual.json` : file);
  const command = [ '--import', observer, resolve(reference, 'tools/bench-move.mjs'), '--root', reference, ...(bunkers ? ['--bunkers'] : []) ];
  const result = spawnSync(process.execPath, command, { encoding: 'utf8', maxBuffer: 64*1024*1024, stdio: workerStdio,
    env: { ...process.env, WW2_REFERENCE: reference, WW2_MOV33_CASE: destination } });
  writeFileSync(resolve(output, checking ? `${name}.check.log` : `${name}.log`),
    `${result.stdout ?? ''}${result.stderr ?? ''}`, { flag: 'wx', mode: 0o600 });
  let trace;
  try { trace = JSON.parse(readFileSync(destination, 'utf8')); }
  catch (error) { manifest.failure = { name, stage: 'capture', message: String(error), processError: result.error?.message, exit: result.status, signal: result.signal }; break; }
  manifest.cases.push({ name, file, commands: trace.commands.length,
    queries: trace.commands.reduce((n,c) => n+c.queries.length, 0), failure: trace.failure });
  if (result.status !== 0 || result.signal || trace.failure) {
    manifest.failure = { name, exit: result.status, signal: result.signal, failure: trace.failure }; break;
  }
  if (checking && readFileSync(resolve(output,file),'utf8') !== readFileSync(destination,'utf8')) {
    manifest.failure = { name, stage: 'exact-reproduction', actual: destination }; break;
  }
}
clean();
const path = resolve(output, checking ? 'manifest.check.json' : 'manifest.json');
writeFileSync(path, JSON.stringify(manifest)+'\n', { flag: 'wx', mode: 0o400 });
chmodSync(path, 0o400);
if (checking && !manifest.failure) assert.equal(readFileSync(path,'utf8'), readFileSync(resolve(output,'manifest.json'),'utf8'));
console.log(JSON.stringify(manifest));
if (manifest.failure) process.exitCode = 2;
