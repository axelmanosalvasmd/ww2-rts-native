// The model texture layers (client/model-textures.js LAYERS, in client/models/geom.js MATS order): _MATID's integer
// part indexes this list. Read from the source text so the orchestrator needs no three.js.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

export function readLayers(reference) {
  const geom = readFileSync(join(reference, 'client/models/geom.js'), 'utf8'), tex = readFileSync(join(reference, 'client/model-textures.js'), 'utf8');
  const mats = [...geom.match(/export const MATS = \[([^\]]+)\]/)[1].matchAll(/'([^']+)'/g)].map((m) => m[1]);
  const body = tex.match(/export const LAYERS = \{([\s\S]*?)\n\};/)[1];
  return mats.map((name, index) => {
    const line = body.match(new RegExp(`^\\s+'?${name}'?: \\{([^}]*)\\}`, 'm'));
    if (!line) throw new Error(`model-textures.js LAYERS has no ${name}`);
    const num = (k) => { const m = line[1].match(new RegExp(`${k}: ([\\d.]+)`)); return m ? Number(m[1]) : null; };
    const file = line[1].match(/file: '([^']+)'/)?.[1] ?? `${name}.jpg`;
    return { index, name, file: `client/textures/models/${file}`.replace('models/../', ''), texelsPerMetre: num('texels'), strength: num('strength'), hue: num('hue'), fade: num('fade'), film: num('film'), roughness: num('roughness'), metalness: num('metalness'), relief: num('relief') };
  }).concat([{ index: -1, name: 'plain', note: 'PLAIN: no texture' }, { index: -2, name: 'unset', note: "UNSET: the material's default (material extras unsetMatId)" }]);
}
