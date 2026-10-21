// Executes pinned original texture constructors and audits native source policy.
// This is a source prerequisite, never compiled/resource/GPU sampler evidence.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url)), repo = resolve(here, '../../../..');
const opt = n => { const i = process.argv.indexOf('--' + n); return i < 0 ? null : process.argv[i + 1]; };
assert.ok(opt('out'), '--out required');
const { installShims, settle } = await import(pathToFileURL(join(repo, 'unreal/Tools/models/shims.mjs')));
installShims();
const THREE = await import('three');
const ground = await import(pathToFileURL(join(repo, 'reference/ww2-rts/client/ground.js')));
await settle();
const sourceFile = join(repo, 'reference/ww2-rts/client/apron.js');
const source = readFileSync(sourceFile, 'utf8');
const begin = source.indexOf('function tileTextures(look) {'), end = source.indexOf('// ---------- shared GLSL');
assert.ok(begin > 0 && end > begin, 'pinned original tile constructor seam');
const tileTextures = new Function('THREE', 'let looks = null;\n' + source.slice(begin, end) + '\nreturn tileTextures;')(THREE);
const tiles = tileTextures(ground.groundLook());
const policies = tiles.map(t => ({ repeatU: t.wrapS === THREE.RepeatWrapping, repeatV: t.wrapT === THREE.RepeatWrapping,
  linearMag: t.magFilter === THREE.LinearFilter, linearMipMin: t.minFilter === THREE.LinearMipmapLinearFilter,
  generateMipmaps: t.generateMipmaps, anisotropy: t.anisotropy, rawBytes: t.colorSpace === THREE.NoColorSpace }));
assert.equal(tiles.length, 6);
assert.ok(policies.every(p => Object.entries(p).every(([k, v]) => k === 'anisotropy' ? v === 8 : v === true)),
  'actual source constructor must request raw repeat/linear-mip/anisotropy 8');
tiles.forEach(t => t.dispose());
const apronFile = resolve(opt('native-apron') ?? join(here, 'TCApron.cpp'));
const native = readFileSync(apronFile, 'utf8');
const factoryFile = join(here, 'TCWater.cpp'), factory = readFileSync(factoryFile, 'utf8');
const setBytes = native.slice(native.indexOf('void FTCApron::SetTileBytes('), native.indexOf('float FTCApron::Height('));
assert.ok(setBytes.includes('TCWaterView::MakeTexture(Sizes[M], Sizes[M], Rgba[M].GetData(), true, true)'),
  'tile factory and original mip/byte path must stay unchanged');
assert.ok(factory.includes('T->Filter = bMips ? TF_Trilinear : TF_Bilinear;'), 'audit the actual factory, not a group default');
const candidatePolicy = /GetOrCreateSamplerState\(FSamplerStateInitializerRHI\(SF_AnisotropicLinear, AM_Wrap, AM_Wrap, AM_Wrap, 0\.f, 8\)\)/.test(native) &&
  native.includes('Resource->SamplerStateRHI = Sampler;') && native.includes('Resource->DeferredPassSamplerStateRHI = Sampler;') &&
  setBytes.includes('TCApronView::FOriginalTileSampler::Apply(T);');
const record = path => ({ path, sha256: createHash('sha256').update(readFileSync(path)).digest('hex') });
const result = { format: 'tc-world28-apron-policy-v1', reference: 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d',
  sourceConstructors: policies, nativeFactoryFilter: 'TF_Trilinear for mipmapped data textures',
  candidatePolicyPresentInSource: candidatePolicy, candidateIsUnappliedScratch: apronFile !== join(here, 'TCApron.cpp'),
  provenance: { source: record(sourceFile), nativeApron: record(apronFile), factory: record(factoryFile), tool: record(fileURLToPath(import.meta.url)) },
  engineRegression: 'ThreeCrossroads.World.World28.ApronOriginalSampler',
  limits: 'Original constructors execute now. Candidate policy detection is textual source evidence only. Actual engine RED/GREEN, cached resource bindings, device anisotropy limit, derivative-selected mips, cooked and sampled GPU evidence are lead prerequisites. No Low image repair is claimed.' };
writeFileSync(opt('out'), JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
console.log(JSON.stringify({ originalAnisotropy: policies.map(p => p.anisotropy),
  candidatePolicyPresentInSource: candidatePolicy, candidateIsUnappliedScratch: result.candidateIsUnappliedScratch, compiledOrRendered: false }));
if (process.argv.includes('--require-native-policy')) assert.ok(candidatePolicy,
  'unchanged native apron only requests trilinear sampling; original anisotropy-8 policy is absent');
