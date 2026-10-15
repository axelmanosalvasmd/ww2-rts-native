// Run the complete original acceptance body. Only read-only observers are added.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const ref = process.env.WW2_REFERENCE ?? resolve(root, 'reference/ww2-rts');
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const title = "Supply reconnection remains immediate at every level, including Easy's opening grace period";
if (process.version !== 'v24.21.0' || process.arch !== 'x64') throw Error('Use exact x64 Node 24.21.0');
const paths = ['test.js', 'test-fixtures.js', 'shared/sim.js', 'shared/ai.js', 'shared/ai-view.js', 'shared/ai-human.js'];
const original = new Map(paths.map(p => [p, readFileSync(resolve(ref, p), 'utf8')]));
const hash = text => createHash('sha256').update(text).digest('hex');
const clean = () => {
  const git = args => execFileSync('git', ['-C', ref, ...args], { encoding: 'utf8' }).trim();
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
  for (const [path, bytes] of original) assert.equal(readFileSync(resolve(ref, path), 'utf8'), bytes);
};
clean();
const humanUrl = pathToFileURL(resolve(ref, 'shared/ai-human.js')).href;
registerHooks({ load(url, context, next) {
  const loaded = next(url, context);
  if (url !== humanUrl) return loaded;
  assert.equal(String(loaded.source), original.get('shared/ai-human.js'));
  return { ...loaded, source: String(loaded.source) + '\nexport {news as __news, formationMoves as __formationMoves, inputsFor as __inputsFor};\n' };
} });
const sim = await import(pathToFileURL(resolve(ref, 'shared/sim.js')));
const ai = await import(pathToFileURL(resolve(ref, 'shared/ai.js')));
const { viewFor } = await import(pathToFileURL(resolve(ref, 'shared/ai-view.js')));
const { clearFixtureUnits } = await import(pathToFileURL(resolve(ref, 'test-fixtures.js')));
const human = await import(humanUrl);
const test = original.get('test.js');
const between = (start, end) => {
  const a = test.indexOf(start), b = test.indexOf(end, a + start.length);
  assert.ok(a >= 0 && b > a, `Original source range ${start}`);
  return test.slice(a, b);
};
const helperSource = between('const aiRandom = seed =>', 'const aiEquivalent =');
const mapSource = between('const aiMap = () =>', '// Supply reconnection remains');
const checkSource = between(`await check("${title}"`, '// Hard keeps onward orders');
const clone = value => JSON.parse(JSON.stringify(value, (_key, v) => {
  if (v instanceof Map) return [...v];
  if (v instanceof Set || ArrayBuffer.isView(v)) return [...v];
  return v;
}));
const runs = values => {
  const rows = [];
  for (const value of values) {
    if (rows.length && rows.at(-1)[0] === value) rows.at(-1)[1]++;
    else rows.push([value, 1]);
  }
  return rows;
};
const frame = (view, slot) => clone({ ...view, chars: runs(view.chars), flags: runs(view.flags),
  height: view.height ? runs(view.height) : null, mapChars: runs(view.mapChars),
  mapHeight: view.mapHeight ? runs(view.mapHeight) : null, units: [...view.units.values()], me: view.players[slot] });
const cases = [], constructors = [];
const observedThink = (g, slot, opts) => {
  const input = frame(opts.view, slot), memoryBefore = clone(opts.memory), commands = [], fractions = [];
  const random = Math.random, submit = opts.submit;
  Math.random = () => { const n = random(); fractions.push(n); return n; };
  try {
    ai.think(g, slot, { ...opts, submit: command => {
      commands.push(clone(command));
      return submit(command);
    } });
  } finally { Math.random = random; }
  const news = commands.map(command => human.__news(command, opts.view));
  const formation = human.__formationMoves(news.filter(Boolean), opts.view);
  const initialCursor = { camera: clone(opts.view.players[slot].spawn), selection: '' };
  const cursor = clone(initialCursor);
  const inputs = formation.map(command => ({ command: clone(command), emitted: human.__inputsFor(cursor, command, opts.view) }));
  cases.push({ slot, level: opts.level, frame: input, memoryBefore, memoryAfter: clone(opts.memory), fractions,
    commands, human: clone({ news, formation, initialCursor, inputs, finalCursor: cursor }) });
};
// These function bodies are copied verbatim from the pinned check and helpers.
const aiCommands = new Function('createHash', 'snapshotFor', 'think', 'assert', helperSource + '\nreturn aiCommands;')(createHash, sim.snapshotFor, observedThink, assert);
const aiMap = new Function(mapSource + '\nreturn aiMap;')();
const createGame = (...args) => { constructors.push(clone(args)); return sim.createGame(...args); };
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
let assertionsRan = 0;
const check = async (name, run) => { assert.equal(name, title); await run(); assertionsRan++; };
const previousRandom = Math.random;
let seed = 0x516e9200;
const startupFractions = [];
Math.random = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; const n = seed / 2 ** 32; startupFractions.push(n); return n; };
try {
  await new AsyncFunction('check', 'createGame', 'aiMap', 'clearFixtureUnits', 'massiveInternals', 'viewFor', 'AI_LEVEL_NAMES', 'aiCommands', 'assert', checkSource)(
    check, createGame, aiMap, clearFixtureUnits, { ...sim, ...sim.internals }, viewFor, ai.AI_LEVEL_NAMES, aiCommands, assert);
} finally { Math.random = previousRandom; }
assert.equal(assertionsRan, 1);
assert.equal(cases.length, 3);
assert.deepEqual(cases.map(c => c.level), ['easy', 'normal', 'hard']);
clean();
const output = JSON.stringify({ reference: pin, node: process.version, arch: process.arch, title,
  sourceHashes: Object.fromEntries([...original].map(([p, bytes]) => [p, hash(bytes)])),
  exactOriginal: { helperSource, mapSource, checkSource, checkSha256: hash(checkSource) },
  scope: 'Full original reconnection acceptance body and assertions. Complete supplied views and memory are recorded. Human helpers are component calls, not a natural commander schedule or canonical native admission. The original check supplies owner/cut as a view overlay.',
  constructors, startupFractions, cases }) + '\n';
const outputPath = resolve(root, 'rust/crates/core/tests/fixtures/ai-cut-reconnection.json');
if (process.argv.includes('--check')) assert.equal(readFileSync(outputPath, 'utf8'), output);
else writeFileSync(outputPath, output);
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} original acceptance: ${assertionsRan} check, ${cases.length} levels`);
