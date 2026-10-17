// Module resolve hook: the pinned reference imports the bare specifier 'three', but reference/ww2-rts is read only and
// has no node_modules. Bare 'three' (and 'three/...') resolve from the directory in WW2_MODEL_DEPS instead, where
// export-reference-models.mjs ran `npm ci --ignore-scripts` on the pin's own package.json and package-lock.json.
// Every other specifier resolves normally, so the reference sources load in place, unmodified.
import { pathToFileURL } from 'node:url';
import { join } from 'node:path';

const deps = process.env.WW2_MODEL_DEPS;
if (!deps) throw new Error('WW2_MODEL_DEPS is not set (the directory holding node_modules/three)');
const parent = pathToFileURL(join(deps, 'package.json')).href;

export function resolveThree(specifier, context, next) {
  if (specifier === 'three' || specifier.startsWith('three/')) return next(specifier, { ...context, parentURL: parent });
  return next(specifier, context);
}
