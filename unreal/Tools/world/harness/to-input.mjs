// The oracle fixture's inputs as plain lines for the standalone harness (scenery-harness.cpp).
import { readFileSync, writeFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
const [fixture, out] = process.argv.slice(2);
const raw = readFileSync(fixture);
const f = JSON.parse((fixture.endsWith('.gz') ? gunzipSync(raw) : raw).toString('utf8'));
const L = [];
f.cases.forEach((c, i) => {
  const n = c.input;
  L.push(`CASE ${i}`);
  L.push(`DIM ${n.w} ${n.h} ${c.low ? 1 : 0} ${c.pieces ? 1 : 0} ${c.props ? 1 : 0} ${n.world ? 1 : 0}`);
  for (const r of n.live) L.push('LIVE ' + r);
  for (const r of n.orig) L.push('ORIG ' + r);
  for (const r of n.rows) L.push('ROWS ' + r);
  if (n.heights) for (const r of n.heights) L.push('HEI ' + r);
  if (n.cells) L.push('CELLS ' + n.cells.join(' '));
  for (const b of n.buildingsUsed ?? []) L.push(`BLD ${b.x} ${b.y} ${b.kind.replace(/ /g, '_')}`);
  if (n.mines.length) L.push('MINE ' + n.mines.join(' '));
  for (const [x, z] of n.nodes) L.push(`NODE ${x} ${z}`);
  if (n.skip.length) L.push('SKIP ' + n.skip.join(' '));
  for (const s of n.spawns) if (s) L.push(`SPAWN ${s.x} ${s.y}`);
  for (const p of n.points) L.push(`POINT ${p.x} ${p.y}`);
  if (n.biome) L.push('BIOME ' + n.biome.join(' '));
  if (n.discovered) L.push('DISC ' + n.discovered);
  L.push('END');
});
writeFileSync(out, L.join('\n') + '\n');
console.log(`${f.cases.length} cases -> ${out}`);
