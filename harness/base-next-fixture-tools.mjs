// Shared original-source recorder setup. No live match state is installed.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {readFileSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
export const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
export const root = fileURLToPath(new URL('../', import.meta.url));
export const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const git = args => execFileSync('git', ['-C', reference, ...args], {encoding: 'utf8'}).trim();
export function clean() {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
}
clean();
const simUrl = pathToFileURL(`${reference}/shared/sim.js`).href;
registerHooks({load(url, context, next) {
  const loaded = next(url, context);
  if (url !== simUrl) return loaded;
  // Observation exports only. Every original function body remains byte-identical.
  return {...loaded, source: loaded.source.toString() +
    '\nexport {canStamp as __baseNextCanStamp, onCoast as __baseNextOnCoast};\n'};
}});
export const sim = await import(simUrl);
export function bits(value) {
  assert.ok(Number.isFinite(value));
  const bytes = Buffer.alloc(8);
  bytes.writeDoubleBE(value);
  return bytes.toString('hex');
}
export const point = value => value ? [bits(value.x), bits(value.z)] : null;
// Same declared constructor bootstrap as BASE05, from match seed 701's Weather stream.
// The original constructor consumes this once; its live LCG is never reset.
const mask = (1n << 64n) - 1n, gamma = 0x9e3779b97f4a7c15n;
const mix = value => {
  value = ((value ^ (value >> 30n)) * 0xbf58476d1ce4e5b9n) & mask;
  value = ((value ^ (value >> 27n)) * 0x94d049bb133111ebn) & mask;
  return value ^ (value >> 31n);
};
const bootstrap = Number(mix((mix(701n ^ ((2n * gamma) & mask)) + gamma) & mask) & 0xffffffffn) / 2 ** 32;
export function construct(input) {
  let draws = 0;
  const original = Math.random;
  Math.random = () => { draws++; return bootstrap; };
  try {
    const game = sim.createGame(structuredClone(input.map), input.names, false,
      input.teams, input.factions, input.settings);
    assert.equal(draws, 1, 'constructor must consume only the declared bootstrap draw');
    return game;
  } finally { Math.random = original; }
}
export function save(path, value) {
  clean();
  const text = JSON.stringify(value) + '\n';
  if (process.argv.includes('--check')) assert.equal(readFileSync(`${root}/${path}`, 'utf8'), text);
  else writeFileSync(`${root}/${path}`, text);
}
export function failure(variable, error, value) {
  if (process.env[variable]) writeFileSync(process.env[variable],
    JSON.stringify({error: String(error), observed: value}) + '\n', {flag: 'wx'});
}
