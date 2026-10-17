// Writes unreal/Content/Data/units.json: the client-safe presentation fields of the pinned reference's UNITS table.
// Interim until the Rust core exposes the table through the C ABI (DESIGN.md). It reads the reference read-only.
// Usage: node unreal/Tools/export-unit-table.mjs [path/to/ww2-rts]
import { writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';

const here = import.meta.dirname;
const reference = resolve(process.argv[2] ?? join(here, '../../reference/ww2-rts'));
const { UNITS } = await import(pathToFileURL(join(reference, 'shared/sim.js')).href);
let pin = 'unknown';
try { pin = execFileSync('git', ['-C', reference, 'rev-parse', '--short', 'HEAD'], { encoding: 'utf8' }).trim(); } catch {}
// Only what the client draws, picks and lays out with (shared/formation.js slotSize, client/main.js rearRank and isAir,
// client/selection.js: a click falls back to buildings, never to other structures such as the bunker; client/orders.js
// kind: a right-click on an own carrier with infantry selected boards it).
const units = Object.fromEntries(Object.entries(UNITS).map(([type, d]) => [type, {
  name: d.name ?? type,
  radius: d.radius ?? 1,
  infantry: !!d.infantry,
  structure: !!d.structure,
  building: !!d.building,
  air: !!d.air,
  rail: !!d.rail,
  models: d.models ?? 1,
  hpPer: d.hpPer ?? 0,
  hp: d.hp ?? 0,
  carries: d.carries ?? 0,
  back: !!d.w?.minRange || !d.w?.range,
}]));
const out = join(here, '../Content/Data/units.json');
writeFileSync(out, JSON.stringify({ reference: pin, units }, null, 1) + '\n');
console.log(`wrote ${Object.keys(units).length} unit types from ${pin} to ${out}`);
