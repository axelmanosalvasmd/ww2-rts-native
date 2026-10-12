// Dumps the rules data the vision and fog passes read (`updateVision`, `teamFog` and what they call in
// shared/sim.js at the pin) from the reference game's own tables, so the Rust side loads the same numbers
// instead of copies typed by hand. The core will own these tables later (docs/parity/interface.md, IF-01).
//   node harness/tables.mjs [--out harness/out/tables.json]
// TERRAIN is not dumped: the passes read each cell's flag bits from the map, never the terrain letters.
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';

const REF = new URL('../reference/ww2-rts/', import.meta.url);
const sim = await import(new URL('shared/sim.js', REF));
const { WEATHER } = await import(new URL('shared/weather.js', REF));

export function tables() {
  const { CFG, UNITS, SUPPORT } = sim;
  const units = Object.entries(UNITS).map(([name, d]) => ({
    name, vision: d.vision, lookout: !!d.lookout, camo: !!d.camo, structure: !!d.structure, building: !!d.building,
    infantry: !!d.infantry, hull: d.hull ?? 0,
  }));
  for (const u of units) if (!Number.isFinite(u.vision)) throw new Error(`${u.name} has no vision number`);
  return {
    reference: 'e4e579a',
    cell: sim.CELL,
    flags: { move: sim.MOVE, sight: sim.SIGHT, trench: sim.TRENCH, wood: sim.WOOD },
    units,
    vision: {
      camoRange: CFG.camoRange, dustSeen: CFG.dustSeen, airSeeRange: CFG.air.seeRange, trenchHide: CFG.trench.hide,
      smokeSight: CFG.smokeSight, levelHeight: CFG.levelHeight, eye: CFG.eye, minLevel: CFG.minLevel, woodSight: CFG.wood.sight,
      highGroundVision: CFG.highGroundVision, garrisonVision: CFG.garrisonVision, houseVision: CFG.houses.map(h => h.vision ?? null),
      rainSight: CFG.weather.sight, reconLen: SUPPORT.recon.len, reconWidth: SUPPORT.recon.width,
    },
    weather: Object.entries(WEATHER).map(([name, w]) => ({ name, sight: w.sight, rains: !!w.rains })),
  };
}

if (import.meta.main) {
  const args = process.argv.slice(2), out = args.includes('--out') ? args[args.indexOf('--out') + 1] : new URL('out/tables.json', import.meta.url).pathname;
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, JSON.stringify(tables(), null, 1) + '\n');
  console.log(`wrote ${out}`);
}
