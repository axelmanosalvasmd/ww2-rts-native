// node --import ./register.mjs: loads the pinned reference sources in place, unmodified on disk.
//  - the bare 'three' specifier resolves from WW2_MODEL_DEPS (unreal/Tools/models/three-resolve.mjs, the three.js the
//    models exporter installed from the pin's own package-lock);
//  - shared/sim.js keeps heightAt private, so when Node loads that file one line is appended to the module text it
//    read from disk: `export { heightAt as __oracleHeightAt };`. Nothing else changes; the file itself is not touched.
import { registerHooks } from 'node:module';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolveThree } from '../models/three-resolve.mjs';

export const SIM_EXPORT = '\nexport { heightAt as __oracleHeightAt };\n';
registerHooks({
  resolve: resolveThree,
  load(url, context, next) {
    if (url.startsWith('file:') && url.endsWith('/shared/sim.js')) {
      const source = readFileSync(fileURLToPath(url), 'utf8') + SIM_EXPORT;
      return { format: 'module', source, shortCircuit: true };
    }
    return next(url, context);
  },
});
