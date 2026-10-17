// Converts the reference's combat effect textures to PNG for the Unreal import (unreal/Tools/effects/import-effects.py).
// Reads only the pinned reference (reference/ww2-rts, clean) and writes unreal/Saved/EffectsExport:
//   fx-atlas.png               client/textures/fx-atlas.webp, premultiplied by its alpha. client/fx.js loads the atlas
//                              with premultiplyAlpha (WebGL premultiplies the 8-bit texels on upload) and its shader
//                              un-premultiplies (t.rb / t.a, t.g / t.a, t.rgb / t.a), so the Unreal texture holds the
//                              same premultiplied texels and the material ports the shader's arithmetic unchanged.
//                              Linear data (fx.js: no colour space; Unreal: sRGB off).
//   fx-crater.png              client/textures/fx-crater.webp unchanged (sRGB colour, straight alpha).
//   fx-rubble-materials-v1.png client/textures/fx-rubble-materials-v1.webp unchanged (sRGB colour, straight alpha).
//   manifest.json              the pin, every input's and output's sha256 and the ImageMagick version.
// Deterministic: pixels go through ImageMagick as raw 8-bit RGBA (as the reference's own tools/build-fx-atlas.mjs reads
// them), the premultiply is integer arithmetic, and the PNGs are written without time chunks, so two runs give the same
// bytes. Usage (repo root): node unreal/Tools/effects/convert-fx-textures.mjs [reference/ww2-rts] [--check]
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join, resolve } from 'node:path';

const here = import.meta.dirname;
const args = process.argv.slice(2);
const check = args.includes('--check');
const reference = resolve(args.find((a) => !a.startsWith('--')) ?? join(here, '../../../reference/ww2-rts'));
const out = resolve(join(here, '../../Saved/EffectsExport'));
const PIN = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const fail = (m) => { console.error(`convert-fx-textures: ${m}`); process.exit(1); };

const git = (...a) => execFileSync('git', ['-C', reference, ...a], { encoding: 'utf8' }).trim();
const head = git('rev-parse', 'HEAD');
if (head !== PIN) fail(`${reference} is at ${head}, not the pin ${PIN}`);
if (git('status', '--porcelain', '--untracked-files=no')) fail(`${reference} has local changes: convert from a clean pin only`);

const sha = (buf) => createHash('sha256').update(buf).digest('hex');
const magick = (argv, input) => execFileSync('magick', argv, { input, maxBuffer: 1 << 28 });
const version = magick(['-version']).toString().split('\n')[0].trim();
const PNG = ['-define', 'png:exclude-chunks=date,time,tIME', '-define', 'png:compression-level=9', '-strip'];

function rgba(file) {
  const [w, h] = magick(['identify', '-format', '%w %h', file]).toString().trim().split(' ').map(Number);
  const raw = magick([file, '-alpha', 'on', '-depth', '8', 'rgba:-']);
  if (raw.length !== w * h * 4) fail(`${file}: ${raw.length} bytes of RGBA for ${w}x${h}`);
  return { w, h, raw };
}
function png(w, h, raw) {
  return magick(['-size', `${w}x${h}`, '-depth', '8', 'rgba:-', ...PNG, 'png32:-'], raw);
}

const jobs = [
  { src: 'client/textures/fx-atlas.webp', dst: 'fx-atlas.png', premultiply: true },
  { src: 'client/textures/fx-crater.webp', dst: 'fx-crater.png', premultiply: false },
  { src: 'client/textures/fx-rubble-materials-v1.webp', dst: 'fx-rubble-materials-v1.png', premultiply: false },
];
const manifest = { reference: PIN, tool: 'unreal/Tools/effects/convert-fx-textures.mjs', toolSha256: sha(readFileSync(import.meta.filename)), imagemagick: version, files: [] };
const outputs = new Map();
for (const j of jobs) {
  const file = join(reference, j.src);
  const source = readFileSync(file);
  const { w, h, raw } = rgba(file);
  if (j.premultiply) {
    // WebGL UNPACK_PREMULTIPLY_ALPHA_WEBGL on 8-bit texels: c * a / 255, rounded
    for (let i = 0; i < raw.length; i += 4) {
      const a = raw[i + 3];
      raw[i] = Math.round(raw[i] * a / 255); raw[i + 1] = Math.round(raw[i + 1] * a / 255); raw[i + 2] = Math.round(raw[i + 2] * a / 255);
    }
  }
  const bytes = png(w, h, raw);
  // read back: the PNG holds exactly the pixels written
  const back = magick(['png:-', '-alpha', 'on', '-depth', '8', 'rgba:-'], bytes);
  if (!back.equals(raw)) fail(`${j.dst}: the PNG does not read back the pixels written`);
  outputs.set(j.dst, bytes);
  manifest.files.push({ source: j.src, sourceSha256: sha(source), output: j.dst, outputSha256: sha(bytes), width: w, height: h, premultiplied: j.premultiply, pixelsSha256: sha(raw) });
}
const text = JSON.stringify(manifest, null, 2) + '\n';
if (check) {
  for (const [name, bytes] of outputs) if (!existsSync(join(out, name)) || !readFileSync(join(out, name)).equals(bytes)) fail(`${name} differs from what this pin writes`);
  if (readFileSync(join(out, 'manifest.json'), 'utf8') !== text) fail('manifest.json differs');
  console.log(`convert-fx-textures: ${out} is current`);
} else {
  mkdirSync(out, { recursive: true });
  for (const [name, bytes] of outputs) writeFileSync(join(out, name), bytes);
  writeFileSync(join(out, 'manifest.json'), text);
  for (const f of manifest.files) console.log(`${f.output}  ${f.width}x${f.height}  ${f.outputSha256}${f.premultiplied ? '  (premultiplied)' : ''}`);
}
