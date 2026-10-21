// Prints the rows of the recruit table in unreal/Source/ThreeCrossroads/Client/TCRecruitment.cpp (const FRow Rows[]) from
// the pinned reference, which it only reads: shared/sim.js UNITS and UNIT_TYPES, shared/tech.js tierOf. When the pin
// moves, run it and replace the rows (and FTCRecruitment::ReferencePin) rather than editing them by hand:
//   node unreal/Tools/export-recruit-table.mjs reference/ww2-rts
import { pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
const ref = process.argv[2];
const { UNITS, UNIT_TYPES } = await import(pathToFileURL(ref + '/shared/sim.js').href);
const { tierOf } = await import(pathToFileURL(ref + '/shared/tech.js').href);
const pin = execFileSync('git', ['-C', ref, 'rev-parse', '--short', 'HEAD'], { encoding: 'utf8' }).trim();
const s = (v) => `TEXT("${String(v).replace(/"/g, '\\"')}")`;
const b = (v) => (v ? 'true' : 'false');
const num = (v) => { if (!Number.isFinite(v)) throw new Error('not finite ' + v); return String(v); };
console.log(`// generated from reference ${pin}`);
for (const t of UNIT_TYPES) {
  const d = UNITS[t];
  for (const k of ['cost']) if (d[k] !== undefined && !Number.isInteger(d[k])) throw new Error(t + k);
  if (d.faction !== undefined && !Number.isInteger(d.faction)) throw new Error(t + ' faction');
  const fields = [s(t), s(d.name ?? t), num(d.cost ?? 0), num(d.classicCost ?? -1), num(d.fuel ?? 0), num(d.pop ?? 1), num(d.max ?? -1),
    d.faction === undefined ? 'AnyFaction' : num(d.faction), b(d.classic), b(d.naval), b(d.structure), b(d.building), b(d.air), b(d.infantry),
    num(d.train ?? 0), num(tierOf(t)), s((d.makes ?? []).join(','))];
  console.log(`\t{ ${fields.join(', ')} },`);
}
