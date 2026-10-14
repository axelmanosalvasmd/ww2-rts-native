// Module hooks for running the pinned reference infantry builders under Node, unmodified on disk.
// 1. Bare 'three' resolves from INFANTRY_DEPS (a directory where `npm ci` ran on a copy of the pin's own
//    package.json and package-lock.json; see export.sh). reference/ww2-rts has no node_modules and is read only.
// 2. client/models/infantry.js and infantry-authored.js keep their internals module-private. When they load,
//    one `export { ... }` line naming existing internal bindings is appended to the source text in memory, so the
//    tools call the exact reference functions instead of a copy. Nothing else in the source changes.
import { pathToFileURL } from 'node:url';
import { join } from 'node:path';

const deps = process.env.INFANTRY_DEPS;
if (!deps) throw new Error('INFANTRY_DEPS is not set (the directory holding node_modules/three)');
const parent = pathToFileURL(join(deps, 'package.json')).href;

const EXPOSE = {
  '/client/models/infantry.js': 'solve, spec, fallenSpec, weapon, item, handGeo, helmetGeo, pilotkaGeo, beretGeo, bagGeo, fuelTank, '
    + 'HELMETS, FACTIONS, makeBody, torsoAt, onTorso, tube, box, chain, T, RX, RY, RZ, SC, basis, DIG, DIGGERS, STANCES, '
    + 'authoredParts, farParts, mergeFigure, waist, headMatrix, ANCHOR',
  '/client/models/infantry-authored.js': 'transforms, frame, dressedBody, bareHead, source as authoredSource, inverse as authoredInverse',
};

export function resolve(specifier, context, next) {
  if (specifier === 'three' || specifier.startsWith('three/')) return next(specifier, { ...context, parentURL: parent });
  return next(specifier, context);
}

export function load(url, context, next) {
  const result = next(url, context);
  const key = Object.keys(EXPOSE).find((suffix) => url.endsWith('/reference/ww2-rts' + suffix));
  if (!key) return result;
  const text = typeof result.source === 'string' ? result.source : Buffer.from(result.source).toString('utf8');
  return { ...result, format: 'module', source: `${text}\nexport { ${EXPOSE[key]} };\n` };
}
