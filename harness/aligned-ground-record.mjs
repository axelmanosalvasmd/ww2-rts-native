// Run one unchanged Source case and retain its actual process result and raw prefix.
import {openSync, closeSync, readFileSync, writeFileSync} from 'node:fs';
import {spawnSync, execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {resolve, dirname} from 'node:path';
import {fileURLToPath} from 'node:url';

const args = process.argv.slice(2);
const option = name => {
  const i = args.indexOf(name);
  if (i < 0 || !args[i + 1] || args[i + 1].startsWith('--')) throw Error(`Required ${name}`);
  return args[i + 1];
};
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const output = resolve(option('--output'));
const phase = args.includes('--check') ? 'check' : 'generate';
const resultPath = `${output}.${phase}.result.json`;
// Reserve every artifact exclusively. A second attempt must use new paths.
const resultFd = openSync(resultPath, 'wx', 0o600);
const result = {schema: 'ground-source-process-v1', phase, result: 'FAIL',
  cwd: root, output, case: null, sourceHead: null, reference: null,
  input: null, capture: null, process: null, error: null,
  stdout: `${output}.${phase}.stdout.log`, stderr: `${output}.${phase}.stderr.log`,
  prefixFailurePath: `${output}.${phase}.failure.json`,
  slotRelease: 'EXTERNAL_ROOT_RECEIPT_REQUIRED'};
const describe = path => {
  const bytes = readFileSync(path);
  return {path, bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex')};
};
let stdoutFd, stderrFd;
try {
  result.case = option('--case');
  if (process.cwd() !== root) throw Error('run from the assigned physical repository root');
  result.sourceHead = execFileSync('git', ['rev-parse', 'HEAD'], {cwd: root, encoding: 'utf8'}).trim();
  const reference = resolve(option('--reference'));
  result.reference = execFileSync('git', ['rev-parse', 'HEAD'], {cwd: reference, encoding: 'utf8'}).trim();
  const inputPath = resolve(option('--input'));
  result.input = describe(inputPath);
  // Keep input bytes too, so the native reader can bind them without a hash dependency.
  const inputBytes = readFileSync(inputPath);
  const before = phase === 'check' ? readFileSync(output) : null;
  stdoutFd = openSync(result.stdout, 'wx', 0o600);
  stderrFd = openSync(result.stderr, 'wx', 0o600);
  const argv = [...process.execArgv, resolve(root, 'harness/aligned-retreat-scuttle.mjs'), ...args];
  result.argv = [process.execPath, ...argv];
  const child = spawnSync(process.execPath, argv, {cwd: root,
    env: process.env, stdio: ['ignore', stdoutFd, stderrFd]});
  result.process = {pid: child.pid ?? null, exitCode: child.status, signal: child.signal,
    error: child.error ? String(child.error) : null, terminal: true};
  try { result.capture = describe(output); } catch (error) { result.captureError = String(error); }
  if (child.error || child.status !== 0 || child.signal !== null) throw Error('Source process failed; retain raw stdout, stderr and any capture prefix');
  const capture = readFileSync(output);
  if (before !== null && !capture.equals(before)) throw Error('authentic recheck changed its raw capture');
  if (!readFileSync(inputPath).equals(inputBytes)) throw Error('authored input changed during Source execution');
  const rows = capture.toString('utf8').trimEnd().split('\n').map(line => JSON.parse(line));
  if (rows.length < 3 || rows[0].evidence?.stage !== 'startup'
      || rows.at(-2).evidence?.stage !== 'complete' || rows.at(-1).evidence?.stage !== 'inputReceipt'
      || rows.some(row => Object.hasOwn(row, 'stateProviderError')
        || ['failure', 'predicateFailure', 'missingMilestone', 'commandException', 'tickException'].includes(row.evidence?.stage))) {
    throw Error('Source did not retain a successful complete prefix and terminal input receipt');
  }
  const first = rows[0].evidence;
  if (first.sourceHead !== result.sourceHead || first.reference !== result.reference
      || first.case !== result.case) throw Error('Source process/capture identity differs');
  const last = rows.at(-1), complete = rows.at(-2);
  if (!Number.isSafeInteger(last.state?.tick) || !Number.isSafeInteger(last.state?.drawCount)
      || last.state.tick !== complete.state?.tick || last.state.drawCount !== complete.state?.drawCount
      || last.evidence.consumed !== last.state.drawCount) throw Error('Source terminal boundary differs');
  result.terminal = {tick: last.state.tick, drawCount: last.state.drawCount, rows: rows.length};
  if (phase === 'check') {
    // This copy is written only after the real --check child exited successfully.
    writeFileSync(`${output}.checked.jsonl`, capture, {flag: 'wx', mode: 0o600});
    writeFileSync(`${output}.checked.input`, inputBytes, {flag: 'wx', mode: 0o600});
  }
  result.result = 'PASS';
} catch (error) {
  result.error = {message: String(error), stack: error.stack ?? null};
  if (result.capture === null) {
    try { result.capture = describe(output); } catch (error) { result.captureError = String(error); }
  }
  process.exitCode = 2;
} finally {
  try { result.prefixFailure = describe(result.prefixFailurePath); }
  catch (error) { if (error.code !== 'ENOENT') result.prefixFailureReadError = String(error); }
  if (stdoutFd !== undefined) closeSync(stdoutFd);
  if (stderrFd !== undefined) closeSync(stderrFd);
  writeFileSync(resultFd, JSON.stringify(result, null, 2) + '\n');
  closeSync(resultFd);
}
console.log(JSON.stringify({resultPath, result: result.result, process: result.process}));
