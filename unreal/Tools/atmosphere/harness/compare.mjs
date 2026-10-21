// Compares atmos-harness.cpp's output with the oracle fixture (unreal/Tools/atmosphere/oracle.mjs). Tolerance: 2e-4
// absolute plus 1e-5 relative (three keeps instance matrices and attributes in Float32, the fixture rounds to 1e-6).
import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
const [fix, out] = process.argv.slice(2);
const raw = readFileSync(fix);
const f = JSON.parse((fix.endsWith('.gz') ? gunzipSync(raw) : raw).toString());
const h = JSON.parse(readFileSync(out, 'utf8'));
let checked = 0, bad = 0, worst = 0;
const fails = [];
const near = (a, b) => Math.abs(a - b) <= 2e-4 + 1e-5 * Math.abs(b);
function cmp(name, a, b, exact = false) {
  if (a.length !== b.length) { bad++; fails.push(`${name}: length ${a.length} vs ${b.length}`); return; }
  for (let i = 0; i < a.length; i++) {
    checked++;
    const d = Math.abs(a[i] - b[i]);
    if (d > worst && !exact) worst = d;
    if (exact ? a[i] !== b[i] : !near(a[i], b[i])) { bad++; if (fails.length < 30) fails.push(`${name}[${i}]: port ${a[i]} reference ${b[i]}`); }
  }
}
f.rail.out.forEach((o, i) => {
  const p = h.rail[i];
  cmp(`rail step ${i} ballast`, p.ballast, o.ballast); cmp(`rail step ${i} ties`, p.ties, o.ties); cmp(`rail step ${i} rails`, p.rails, o.rails);
  cmp(`rail step ${i} station count`, [p.stations.length], [o.stations.length], true);
  o.stations.forEach((s, k) => { cmp(`rail step ${i} station ${k} region`, [p.stations[k]?.region], [s.region], true); cmp(`rail step ${i} station ${k} parts`, (p.stations[k]?.parts ?? []).flat(), s.parts.flat()); });
  cmp(`rail step ${i} track cells`, [p.has], [o.has], true);
});
cmp('noise', h.noise, f.noise, true);
for (const k of ['rain', 'snow', 'dust']) { cmp(`fall ${k} position`, h.falls[k].position, f.falls[k].position); cmp(`fall ${k} rand`, h.falls[k].rand, f.falls[k].rand); }
f.maps.forEach((m, i) => {
  cmp(`${m.file} groundMask`, h.maps[i].mask, m.mask, true);
  cmp(`${m.file} mistSites`, h.maps[i].mist.flat(), m.mist.flat());
  cmp(`${m.file} bankSites`, h.maps[i].banks.flat(), m.banks.flat());
});
cmp('wind', h.wind.flat(), f.wind.flat());
f.running.forEach((r, i) => {
  r.frames.forEach((fr, k) => {
    const p = h.running[i].frames[k], n = `${r.file} ${r.weather} t=${fr.t}`;
    if (fr.cloudVisible) cmp(`${n} drift`, p.drift, fr.drift);
    cmp(`${n} birds`, p.birds.flat(), fr.birds);
    if (fr.mistVisible) cmp(`${n} mist`, p.mist.flat(), fr.mist);
    if (fr.banksVisible) cmp(`${n} banks`, p.banks.flat(), fr.banks);
    // the fall drawn at start keeps the default wind's slant; compare while it is still the first one
    if (fr.fall && k < 6) cmp(`${n} ${fr.fall.kind} velocity`, p.vel[fr.fall.kind], fr.fall.vel);
  });
});
console.log(`compare: ${checked} values checked, ${bad} outside tolerance, worst float difference ${worst.toExponential(2)}`);
for (const l of fails) console.log('  ' + l);
process.exit(bad ? 1 : 0);
