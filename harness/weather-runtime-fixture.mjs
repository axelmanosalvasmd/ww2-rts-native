// Execute the pinned private weather body; the loader only adds exports.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
assert.equal(process.arch, 'x64'); assert.equal(process.version, 'v24.21.0');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
assert.equal(git(['rev-parse', 'HEAD']), pin);
assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
const url = pathToFileURL(`${reference}/shared/sim.js`).href;
registerHooks({ load(path, context, next) { const result = next(path, context); return path === url ? { ...result, source: `${result.source}\nexport { weather, rng };\n` } : result; } });
const sim = await import(url);
const { planWeather, stepWeather } = await import(pathToFileURL(`${reference}/shared/weather.js`));
const bits = n => { const buffer = Buffer.alloc(8); buffer.writeDoubleBE(n); return buffer.toString('hex'); };
const snapshot = g => ({ seed: g.seed >>> 0, wind: [g.wind.a, g.wind.v].map(bits),
  wx: g.wx && [g.wx.raining, ...[g.wx.next, g.wx.rain, g.wx.wet].map(bits)],
  plan: structuredClone(g.weather), smoke: g.smokes.map(s => [s.id, bits(s.x), bits(s.z), bits(s.t)]) });
const initial = (kind, overrides = {}) => ({ seed: 0x3c1055, wind: { a: 0.2, v: 0.4 },
  wx: { raining: false, next: 0.1, rain: 0, wet: 0.25 }, weather: kind && { now: kind, next: null, at: 0 },
  smokes: [{ id: 1, x: 1.25, z: -2.5, t: 0.01 }, { id: 2, x: -40, z: 10, t: 100 }], ...overrides });
const cases = [];
function record(name, input, steps) {
  const g = structuredClone(input), expected = [];
  for (const [tick, dt] of steps) {
    g.tick = tick;
    stepWeather(g);
    // This exact statement precedes weather() in step(); expired smoke still drifts.
    for (const s of g.smokes) s.t -= dt * (1 + 0.5 * (g.wx?.rain ?? 0));
    sim.weather(g, dt);
    expected.push(snapshot(g));
  }
  cases.push({ name, input, steps, expected });
}
const steps = [0, 0.05, 0.05, 0, 0.001, 0.2, 1, 10, 20, 90, 250, 0.05, 600, 0.125].map((dt, i) => [i + 1, dt]);
for (const kind of ['clear', 'fog', 'rain', 'mud', 'snow', null]) {
  record(`fixed-${kind}`, initial(kind), steps);
  record(`disabled-${kind}`, initial(kind, { wx: null }), steps);
}
record('no-smoke-clouds', initial('clear', { smokes: [] }), steps);
for (const rain of [0.499999, 0.5, 0.500001, 1]) for (const raining of [false, true]) {
  record(`threshold-${rain}-${raining}`, initial('clear', { wx: { raining, next: 1000, rain, wet: 0.5 } }), [[9, 0], [10, 0.05], [11, 0.05]]);
}
for (const [a, v] of [[0, 0.15], [-Math.PI, 1], [1e4, -1], [-1e4, 2]]) {
  record(`wind-${a}-${v}`, initial('clear', { wind: { a, v } }), [[0, 0], [1, 0.05], [10, 400]]);
}
for (let seed = 0; seed < 32; seed++) {
  const plan = planWeather('random', {}, '', seed);
  const at = plan.at || 600;
  record(`random-plan-${seed}`, initial(plan.now, { seed, weather: plan }), [[1, 0.05], [at - 1, 0.05], [at, 0.05], [at + 1, 0.05], [at + 100, 100]]);
}
for (const now of ['rain', 'fog']) record(`${now}-transition`, initial(now, { weather: { now, next: now === 'rain' ? 'mud' : 'clear', at: 10 },
  wx: { raining: true, next: 0, rain: 1, wet: 1 } }), [[9, 0.05], [10, 0.05], [11, 0.05], [20, 20]]);
const rng = [0, 1, 0x3c1055, 0xffffffff].map(seed => {
  const g = { seed }; return { seed, draws: Array.from({ length: 100 }, () => [g.seed >>> 0, bits(sim.rng(g)), g.seed >>> 0]) };
});
const text = JSON.stringify({ reference: pin, node: process.version, arch: process.arch, cases, rng }) + '\n';
const destination = `${root}/rust/crates/core/tests/fixtures/weather-runtime.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(destination, 'utf8'), text);
else writeFileSync(destination, text);
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} ${cases.length} weather sequences, ${cases.reduce((n, c) => n + c.steps.length, 0)} steps and 400 environment draws`);
