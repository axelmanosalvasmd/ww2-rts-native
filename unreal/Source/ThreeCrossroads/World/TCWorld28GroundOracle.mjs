// Source and standalone C++ only. Runs the original public-ground oracle anew,
// then probes production bytes and extracted HLSL. No browser, engine or server.
// WW2_MODEL_DEPS=<retained deps> node --import <models/register.mjs> <this> --stream <public jsonl>
//   --camera-log <exact source camera log> --graphics high|low
//   --scratch <fresh round28-world directory> --out <fresh json>
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { homedir } from 'node:os';

const here = dirname(fileURLToPath(import.meta.url)), repo = resolve(here, '../../../..');
const opt = n => { const i = process.argv.indexOf('--' + n); return i < 0 ? null : process.argv[i + 1]; };
for (const n of ['stream', 'camera-log', 'graphics', 'scratch', 'out']) assert.ok(opt(n), '--' + n + ' required');
const scratch = resolve(opt('scratch')), out = resolve(opt('out'));
assert.ok(scratch.startsWith(join(homedir(), '.hermes/cache/scratch/round28-world') + '/'), 'use owned ordinary scratch');
assert.ok(!existsSync(scratch) && !existsSync(out), 'fresh scratch/output required, retain earlier tries');
mkdirSync(scratch, { recursive: true });
const hash = b => createHash('sha256').update(b).digest('hex');
const sha = p => hash(readFileSync(p));
const record = p => ({ path: resolve(p), sha256: sha(p) });
const commands = [];
function run(label, command, args) {
  const r = spawnSync(command, args, { cwd: repo, env: process.env, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 });
  writeFileSync(join(scratch, label + '.log'), (r.stdout ?? '') + (r.stderr ?? ''));
  commands.push({ label, command, args, exit: r.status, error: r.error?.message ?? null,
    log: record(join(scratch, label + '.log')) });
  writeFileSync(join(scratch, 'commands.json'), JSON.stringify(commands, null, 2) + '\n');
  assert.equal(r.status, 0, label + ' failed; preserve its log');
}
assert.equal(spawnSync('git', ['-C', join(repo, 'reference/ww2-rts'), 'rev-parse', 'HEAD'], { encoding: 'utf8' }).stdout.trim(),
  'e4e579a838aabae3b1d9dadbc17a0a87147bc52d');
const originalOut = join(scratch, 'source-ground.json');
run('original-public-ground', process.execPath, ['--import', join(repo, 'unreal/Tools/models/register.mjs'),
  join(here, 'TCWorld27GroundOracle.mjs'), '--stream', resolve(opt('stream')), '--camera-log', resolve(opt('camera-log')),
  '--graphics', opt('graphics'), '--scratch', join(scratch, 'original'), '--out', originalOut]);
const original = JSON.parse(readFileSync(originalOut));
assert.equal(original.provenance.mapName, 'Ardennes Crossing', 'bounded snow control requires retained public Ardennes');
const start = readFileSync(opt('stream'), 'utf8').split('\n').filter(Boolean).map(JSON.parse).find(m => m.t === 'start');
assert.ok(!start.map.world, 'never invent or discover unknown World ground');
const generator = join(repo, 'unreal/Tools/atmosphere/make-atmosphere-assets.py');
const code = readFileSync(generator, 'utf8');
const extract = name => { const m = code.match(new RegExp('^' + name + " = '''([\\s\\S]*?)'''", 'm')); assert.ok(m, name); return m[1]; };
const display = extract('DISPLAY_HLSL').replaceAll('[unroll]', '');
const expression = extract('GROUND_HLSL').match(/float snow = ([^;]+);/)[1];
const header = `// Extracted from the production generator, never a compiled GPU claim.\n
static float World28Snow(float Cover, float dry, float ny, float small, float road) {
 return ${expression.replaceAll('m.b', 'road')};
}
static float World28LowAlpha(float3 Background, float3 Col, float A, float Exposure, float PreExposure, float LinearBlend) {
 const struct { float3 rgb; } Scene{Background};
 const struct { float PreExposure; } View{PreExposure};
 ${display}
 return alphaLin;
}
`;
const expressionFile = join(scratch, 'TCWorld28GroundExpressions.inl');
writeFileSync(expressionFile, header);
const input = join(scratch, 'native-input.txt');
writeFileSync(input, [start.map.w + ' ' + start.map.h, ...start.map.rows, original.samples.length,
  ...original.samples.map(s => [s.position[0], s.position[2], s.normalUp, original.source.cover].join(' '))].join('\n') + '\n');

// Actual source flat material averages isolate colour conversion without
// claiming that these unlit controls are the observed lit opaque scene.
const { installShims, settle } = await import(pathToFileURL(join(repo, 'unreal/Tools/models/shims.mjs')));
installShims(); globalThis.localStorage = { getItem: () => null, setItem: () => {} }; globalThis.location = { search: '' };
const THREE = await import('three');
const ground = await import(pathToFileURL(join(repo, 'reference/ww2-rts/client/ground.js')));
const { MOODS, moodFor } = await import(pathToFileURL(join(repo, 'reference/ww2-rts/client/moods.js')));
const { screen, oetf } = await import(pathToFileURL(join(repo, 'unreal/Tools/fog/tone.mjs')));
await settle();
const look = ground.groundLook(), exposure = MOODS[moodFor(start.map, 'ardennes-crossing', 'snow')].exposure;
const snow = new THREE.Color(0xeef1f4).toArray();
const cameras = readFileSync(opt('camera-log'), 'utf8').split('\n').flatMap(l => { try { return [JSON.parse(l)]; } catch { return []; } }).filter(r => r.exactCamera);
const mix = (a, b, t) => a.map((v, i) => v * (1 - t) + b[i] * t);
const controls = [];
for (const s of original.samples) {
  const background = new THREE.Color().setRGB(...look.flat[s.material.primary].map(v => v / 255), THREE.SRGBColorSpace).toArray();
  const haze = cameras.find(c => c.exactCamera === s.pose).fog[2];
  for (const f of [0, s.sceneHaze]) {
    const alpha = s.snowAlphaBeforeHaze * (1 - f);
    controls.push({ sourcePixel: s.pixel, pose: s.pose, carrier: s.carrier, background,
      haze, fog: f, alpha, snow, exposure });
  }
}
const alphaInput = join(scratch, 'alpha-input.txt');
writeFileSync(alphaInput, [controls.length, ...controls.map(c => [...c.background, ...snow, c.alpha, exposure, 1, opt('graphics') === 'high' ? 1 : 0].join(' '))].join('\n') + '\n');
const executable = join(scratch, 'ground-native');
run('compile-standalone', process.env.CXX ?? 'g++', ['-std=c++17', '-O2', '-Wall', '-Wextra', '-DTC_WORLD28_GROUND_STANDALONE',
  '-I' + join(repo, 'unreal/Source/ThreeCrossroads'), '-I' + scratch, join(here, 'TCWorld28GroundHarness.cpp'), '-o', executable]);
const nativeOut = join(scratch, 'native-ground.json'), nativeNoise = join(scratch, 'native-noise.bin'), nativeMask = join(scratch, 'native-mask.bin');
run('native-production-kernels', executable, [input, nativeOut, nativeNoise, nativeMask, alphaInput]);
const native = JSON.parse(readFileSync(nativeOut));
const noise = readFileSync(nativeNoise), rgba = Buffer.alloc(noise.length * 4);
noise.forEach((v, i) => rgba.fill(v, i * 4, i * 4 + 4));
assert.equal(hash(rgba), original.source.noise.bytesSHA256, 'production seeded noise bytes must match the original');
assert.equal(sha(nativeMask), original.source.mask.bytesSHA256, 'compare constructor mask with the same constructor input');
let maxSmallError = 0, maxSnowError = 0;
native.samples.forEach((s, i) => {
  maxSmallError = Math.max(maxSmallError, Math.abs(s.small - original.samples[i].small));
  maxSnowError = Math.max(maxSnowError, Math.abs(s.snowAlpha - original.samples[i].snowAlphaBeforeHaze));
});
assert.ok(maxSmallError < 1e-12, 'level-zero repeat/linear sampling differs');
assert.ok(maxSnowError < 2e-6, 'extracted native snow expression differs beyond standalone float precision');

const pixels = controls.map((c, i) => {
  const baseDisplay = screen(c.background, exposure).map(v => v / 255), tintDisplay = screen(snow, exposure).map(v => v / 255);
  // ShaderMaterial OUT has no fog-colour mix. It only fades cover alpha.
  const expected = opt('graphics') === 'high'
    ? screen(mix(mix(c.background, c.haze, c.fog), snow, c.alpha), exposure).map(v => v / 255)
    : mix(mix(baseDisplay, c.haze.map(oetf), c.fog), tintDisplay, c.alpha);
  // This models the actual native shader/post ordering, not raw GPU samples.
  const linear = mix(c.background, snow, native.lowAlphas[i]);
  const actual = opt('graphics') === 'high'
    ? screen(mix(linear, c.haze, c.fog), exposure).map(v => v / 255)
    : mix(screen(linear, exposure).map(v => v / 255), c.haze.map(oetf), c.fog);
  return { ...c, nativeAlpha: native.lowAlphas[i], sourceDisplay: expected, nativeDisplayModel: actual,
    maximumChannelError255: 255 * Math.max(...actual.map((v, k) => Math.abs(v - expected[k]))) };
});
const selected = pixels.filter(p => p.alpha > 0.01);
const noHaze = selected.filter(p => p.fog === 0), withHaze = selected.filter(p => p.fog > 0);
const stats = rows => ({ count: rows.length, maximumChannelError255: Math.max(0, ...rows.map(p => p.maximumChannelError255)),
  meanMaximumChannelError255: rows.reduce((v, p) => v + p.maximumChannelError255, 0) / Math.max(1, rows.length) });
const result = { format: 'tc-world28-ground-numeric-v1', graphics: opt('graphics'),
  reference: original.reference, sourceKind: 'Fresh execution of pinned original at retained public input/camera, standalone production C++ and extracted HLSL',
  provenance: { stream: record(opt('stream')), camera: record(opt('camera-log')), originalTool: record(join(here, 'TCWorld27GroundOracle.mjs')),
    originalResult: record(originalOut), generator: record(generator), expressions: record(expressionFile), harness: record(join(here, 'TCWorld28GroundHarness.cpp')),
    sourceTextures: Object.fromEntries(['grass', 'dirt', 'mud', 'field', 'road', 'water'].map(n => [n, sha(join(repo, 'reference/ww2-rts/client/textures/' + n + '.jpg'))])), commands },
  kernels: { noiseTexels: noise.length, noiseRGBAHash: hash(rgba), constructorMaskBytes: readFileSync(nativeMask).length,
    sampledSourceRays: original.samples.length, maxSmallError, maxSnowAlphaError: maxSnowError, snowExpression: expression,
    sourceSampler: original.source.noise, sourcePaintPixelsPerCell: original.source.groundPixelsPerCell,
    sourceCloudVisible: original.source.cloudVisible, constructorVsLaterRecipientMaskDifferences: original.source.maskInputDifferences },
  colourModel: { noHaze: stats(noHaze), withHaze: stats(withHaze), pixels },
  conclusions: ['Noise bytes, constructor mask, 9 m snow coordinates and level-zero bilinear samples are directly compared.',
    'The native Low conversion solves one luminance alpha, then a global haze pass. It is an approximation for coloured backgrounds.',
    'No shader fix is applied from this model. Raw GPU filtering, pass versions, lit opaque radiance and full Low composition need lead controls.'],
  limits: 'No decoded painted canvas pixels, derivative-selected GPU mips, actual GPU texels, rendered symptom repair, cooked result, performance or accepted parity. Colour controls use original flat diffuse averages as unlit inputs. Earlier World unknown-ground/privacy limits are unchanged.' };
writeFileSync(out, JSON.stringify(result, null, 2) + '\n');
console.log(JSON.stringify({ graphics: result.graphics, kernels: { ...result.kernels, sourceSampler: undefined },
  colourModel: { noHaze: stats(noHaze), withHaze: stats(withHaze) }, changedProduction: false }));
if (process.argv.includes('--require-exact-colour')) assert.ok(selected.every(p => p.maximumChannelError255 <= 0.5),
  'original full-colour layer composition differs; this optional RED remains a diagnostic, never relax it to claim alignment');
