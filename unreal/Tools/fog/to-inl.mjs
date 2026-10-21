// fog-oracle.json -> World/TCFogFrontier.Oracle.inl: two of the oracle's scenarios (an authored map's player, World's
// frontier) for ThreeCrossroads.World.FogFrontier, run-length encoded ("value" or "value*count" tokens). Per step: the
// flips, the known ground units (type x z), lift, the alpha after a 0.05 s frame, settled alpha and minimap alpha.
import fs from 'node:fs';
import path from 'node:path';
const [oracle, ref, out] = process.argv.slice(2);
const { UNITS } = await import(path.join(ref, 'shared/sim.js'));
const d = JSON.parse(fs.readFileSync(oracle, 'utf8'));
const rle = (a) => { const t = []; for (let i = 0; i < a.length;) { let j = i; while (j < a.length && a[j] === a[i]) j++; t.push(j - i > 1 ? `${a[i]}*${j - i}` : `${a[i]}`); i = j; } return `${a.length}:${t.join(' ')}`; };
const lines = [];
lines.push(`B ${Object.entries(d.meta.buildingSizes).map(([k, v]) => k + '=' + v).join(' ')}`);
for (const s of d.scenarios.filter((x) => ['authored-player', 'world-player-frontier', 'authored-player-lifted-at-end'].includes(x.name))) {
  lines.push(`S ${s.name} ${s.w} ${s.h} ${s.opaque ? 1 : 0} ${s.off ? 1 : 0} ${s.key ? 1 : 0}`);
  if (s.key) { lines.push('V ' + s.key.v.join(' ')); lines.push('E ' + s.key.e.join(' ')); }
  lines.push('I ' + rle(s.initial.alpha));
  for (const st of s.steps) {
    lines.push('F ' + st.flips.join(' '));
    lines.push('U ' + st.units.filter(([t]) => UNITS[t] && !UNITS[t].air).map(([t, x, z]) => `${t} ${x.toPrecision(17)} ${z.toPrecision(17)}`).join(' '));
    const f = st.frames.find((q) => q.dt === 0.05);
    lines.push(`L ${st.lift ? 1 : 0} ${st.frames.map((q) => q.dt).join(' ')}`);
    lines.push('A ' + rle(f.alpha));
    lines.push('T ' + rle(st.settled));
    lines.push('M ' + rle(st.minimap));
  }
}
const body = lines.join('\n');
const sha = (await import('node:crypto')).createHash('sha256').update(fs.readFileSync(path.join(ref, 'client/fog.js'))).digest('hex');
let txt = `// Generated from the Node oracle that runs the pinned reference's unmodified client/fog.js (${d.meta.reference},\n// fog.js sha256 ${sha}) on scripted play; see TCFogFrontier.h. Do not edit by hand.\n`;
txt += `// ${lines.length} lines; read by ThreeCrossroads.World.FogFrontier (TCFogFrontier.cpp).\n#pragma once\nstatic const TCHAR* const TCFogFrontierOracle[] = {\n`;
for (const l of lines) txt += `\tTEXT("${l}"),\n`;
txt += '};\n';
fs.writeFileSync(out, txt);
console.log('lines', lines.length, 'bytes', txt.length, 'longest line', Math.max(...lines.map((l) => l.length)));
