// Recheck the ENG-59/74 named helper matrix against unmodified pinned Source bodies.
// This is a component check. It runs no match and writes no reference or fixture file.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
assert.equal(git(['rev-parse', 'HEAD']), 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d');
assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
const url = pathToFileURL(`${reference}/shared/sim.js`).href;
registerHooks({
  load(path, context, next) {
    const result = next(path, context);
    return path === url
      ? { ...result, source: `${result.source}\nexport { groundMul, speedMul, visionOf, visionRange };\n` }
      : result;
  },
});
const sim = await import(url);
const weather = await import(pathToFileURL(`${reference}/shared/weather.js`));
const bits = value => {
  const bytes = Buffer.alloc(8);
  bytes.writeDoubleBE(value);
  return bytes.toString('hex');
};
const same = (actual, expected, label) => assert.equal(bits(actual), bits(expected), label);
const g = {
  w: 20, h: 20, flags: new Uint16Array(400).fill(sim.LAND | sim.MUD),
  chars: Array(400).fill('M'), wear: new Float32Array(400),
};
const tank = { type: 'tank', x: 19, z: 19 };
const rifle = { type: 'rifle', x: 19, z: 19 };
const midpoint = Buffer.from('3fe0cccccccccccc', 'hex').readDoubleBE();
let mudCases = 0;
for (const [depth, drySpeed] of [[0, 0.7], [0.5, midpoint], [1, 0.35]]) {
  g.wear.fill(depth);
  for (const [wet, factor] of [[null, 1], [0, 1], [0.5, 0.9], [1, 0.8]]) {
    g.wx = wet === null ? undefined : { wet };
    const expected = drySpeed * factor;
    same(sim.groundMul(g, 189, true), expected, 'mud vehicle cell');
    same(sim.groundMul(g, 189, false), 1, 'mud infantry cell');
    same(sim.speedMul(g, tank), (expected + expected + expected + expected + expected) / 5, 'tank footprint');
    same(sim.speedMul(g, rifle), 1, 'rifle footprint');
    mudCases++;
  }
}
assert.equal(mudCases, 12);
const kinds = Object.entries(sim.UNITS);
assert.equal(kinds.length, 47);
let weatherCases = 0;
for (const [name, sight, infantry, vehicle, rain] of [
  ['clear', 1, 1, 1, 0], ['fog', 0.7, 1, 1, 0], ['rain', 1, 1, 1, 1],
  ['mud', 1, 0.9, 1, 0], ['snow', 0.9, 0.9, 0.85, 0],
]) {
  g.weather = { now: name };
  g.wx = { rain };
  same(weather.sightMul(g), sight, name);
  for (const [type, def] of kinds) {
    const unit = { type, x: 19, z: 19, built: 1, garrison: -1, air: def.air ? { state: 'station' } : undefined };
    same(weather.weatherSpeed(g, def), def.air ? 1 : def.infantry ? infantry : vehicle, type);
    const expectedSight = def.vision * (def.air ? 1 : sight);
    same(sim.visionOf(g, unit), expectedSight, type);
    if (!def.air) same(sim.visionRange(g, unit, def), expectedSight * (1 - 0.2 * rain), type);
    weatherCases++;
  }
}
assert.equal(weatherCases, 235);
g.weather = undefined;
same(weather.sightMul(g), 1, 'missing weather sight');
for (const [, def] of kinds) same(weather.weatherSpeed(g, def), 1, 'missing weather speed');
console.log(`Checked ${mudCases} mud cases and ${weatherCases} weather/kind cases against pinned Source`);
