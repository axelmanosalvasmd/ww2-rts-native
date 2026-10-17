// fog-oracle.json -> harness-input.txt (plain integers for the C++ harness). A unit is passed as "known" when the
// pinned UNITS has its type and it is not an aircraft (what the native unit table tells FTCFogFrontier); its footprint
// size is left to the C++ (TCFogFrontierCore::BuildingSize) so the table is checked too.
import fs from 'node:fs';
import path from 'node:path';
const [oracle, ref, out] = process.argv.slice(2);
const { UNITS } = await import(path.join(ref, 'shared/sim.js'));
const d = JSON.parse(fs.readFileSync(oracle, 'utf8'));
const L = [];
const arr = (a) => L.push(a.length + ' ' + a.join(' '));
L.push(String(Object.keys(d.meta.buildingSizes).length));
for (const [k, v] of Object.entries(d.meta.buildingSizes)) L.push(`${k} ${v}`);
L.push(String(d.scenarios.length));
for (const s of d.scenarios) {
  L.push(`${s.name} ${s.w} ${s.h} ${s.opaque ? 1 : 0} ${s.off ? 1 : 0} ${s.key ? 1 : 0}`);
  if (s.key) { arr(s.key.v); arr(s.key.e); }
  arr(s.initial.alpha); arr(s.initial.looks); arr(s.initial.minimap);
  L.push(String(s.steps.length));
  for (const st of s.steps) {
    arr(st.flips);
    const known = st.units.filter(([t]) => UNITS[t] && !UNITS[t].air);
    L.push(String(known.length));
    for (const [t, x, z] of known) L.push(`${t} ${x.toPrecision(17)} ${z.toPrecision(17)}`);
    L.push(`${st.lift ? 1 : 0} ${st.frames.length}`);
    for (const f of st.frames) { L.push(f.dt.toPrecision(17)); arr(f.alpha); }
    arr(st.settled); arr(st.looks); arr(st.minimap);
  }
}
fs.writeFileSync(out, L.join('\n') + '\n');
