// Node oracle for the haze per mood and weather (round 14, scope edge): the pinned reference's client/moods.js
// imported unmodified (MOODS, WEATHER_LOOK); atmosphere.js blend() and applyMood() and light.js followView() are three
// lines each and are restated here with three r186's own Color (they need a WebGL scene to import). For each mood and
// match weather (no shower): hazeK, the haze hex light.js receives, the fog's near and far at camera distances 85 and
// 140 m, and the haze as the canvas shows it on High (composer: tone-mapped) and on Low (its sRGB bytes).
//   node haze-oracle.mjs <reference copy with node_modules> <out.json>
import fs from 'node:fs';
import path from 'node:path';
const [ref, outFile] = process.argv.slice(2);
globalThis.location = { search: '' };
const { MOODS, WEATHER_LOOK } = await import(path.join(ref, 'client/moods.js'));
const { screen } = await import('./tone.mjs');
const THREE = await import(path.join(ref, 'node_modules/three/build/three.module.js'));
const rows = [];
for (const [mood, M] of Object.entries(MOODS)) for (const kind of Object.keys(WEATHER_LOOK)) {
  // atmosphere.js blend() with shower.rain = 0: goal.haze = base.haze; hazeGoal = M.haze toward rain's, then back to
  // hazeBase by 1 - k = 1
  const base = WEATHER_LOOK[kind], r = WEATHER_LOOK.rain, k = 0;
  const hazeBase = new THREE.Color().setHex(M.haze).lerp(new THREE.Color().setHex(base.hazeTo), base.hazeMix);
  const hazeGoal = new THREE.Color().setHex(M.haze).lerp(new THREE.Color().setHex(r.hazeTo), r.hazeMix).lerp(hazeBase, 1 - k);
  const haze = base.haze + (r.haze - base.haze) * k;
  // applyMood: setMood({ ...M, hazeK: M.hazeK * cur.haze, haze: hazeNow.getHex() })
  const hazeK = M.hazeK * haze, hex = hazeGoal.getHex(), lin = new THREE.Color().setHex(hex);
  const fog = (dist) => [dist * 0.6 / hazeK, dist * 3.8 / hazeK];
  rows.push({ mood, weather: kind, hazeK, hex, exposure: M.exposure, fog85: fog(85), fog140: fog(140),
    screenHigh: screen([lin.r, lin.g, lin.b], M.exposure), screenLow: [hex >> 16, (hex >> 8) & 255, hex & 255] });
}
fs.writeFileSync(outFile, JSON.stringify({ reference: fs.readFileSync(path.join(ref, '..', 'ref.commit'), 'utf8').trim(), rows }, null, 1));
for (const r of rows.filter((x) => x.weather === 'clear' || (x.mood === 'snow' && x.weather === 'snow') || x.weather === 'fog'))
  console.log(r.mood.padEnd(9), r.weather.padEnd(6), 'hazeK', r.hazeK.toFixed(3), 'hex', r.hex.toString(16), 'fog@140', r.fog140.map((v) => v.toFixed(1)).join('-'), 'High', r.screenHigh.map((v) => v.toFixed(1)).join(','), 'Low', r.screenLow.join(','));
