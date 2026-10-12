// Dumps the rules data the ported passes read (the vision and fog passes, `updateVision` and `teamFog`; the
// movement block of step()'s unit loop with shared/movement.js, vehicle-motion.js and local-traffic.js; and
// `separateCrowds`) from the reference game's own tables, so the Rust side loads the same numbers instead of
// copies typed by hand. The core will own these tables later (docs/parity/interface.md, IF-01).
//   node harness/tables.mjs [--out harness/out/tables.json]
// TERRAIN is not dumped: the passes read each cell's flag bits from the map. The few terrain letters movement
// reads (houses and walls for cover, bridges, woods) are fixed in the core.
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';

const REF = new URL('../reference/ww2-rts/', import.meta.url);
const sim = await import(new URL('shared/sim.js', REF));
const { WEATHER } = await import(new URL('shared/weather.js', REF));
const { movementProfile, isGroundVehicle, CORNER_BULGE } = await import(new URL('shared/vehicle-motion.js', REF));
const { TRAFFIC, LANE } = await import(new URL('shared/local-traffic.js', REF));

export function tables() {
  const { CFG, UNITS, SUPPORT } = sim;
  const units = Object.entries(UNITS).map(([name, d]) => ({
    name, vision: d.vision, lookout: !!d.lookout, camo: !!d.camo, structure: !!d.structure, building: !!d.building,
    infantry: !!d.infantry, hull: d.hull ?? 0,
    // movement: a unit kind's top speed and size, what it is, the sprint ability's speed factor (null: the kind
    // has none) and, for ground vehicles, its motion profile
    speed: d.speed, radius: d.radius ?? null, naval: !!d.naval, deep: !!d.deep, air: !!d.air, rail: !!d.rail, weapon: !!d.w,
    abilitySpeed: d.ab?.speed ?? null, vehicle: isGroundVehicle(d) ? movementProfile(name, d) : null,
  }));
  for (const u of units) if (!Number.isFinite(u.vision)) throw new Error(`${u.name} has no vision number`);
  for (const u of units) if (!Number.isFinite(u.speed)) throw new Error(`${u.name} has no speed`);
  return {
    reference: 'e4e579a',
    cell: sim.CELL,
    tick: sim.TICK,
    flags: {
      move: sim.MOVE, sight: sim.SIGHT, cover: sim.COVER, trench: sim.TRENCH, ford: sim.FORD, wire: sim.WIRE, vblock: sim.VBLOCK,
      road: sim.ROAD, mud: sim.MUD, wood: sim.WOOD, land: sim.LAND, shoal: sim.SHOAL,
    },
    units,
    vision: {
      camoRange: CFG.camoRange, dustSeen: CFG.dustSeen, airSeeRange: CFG.air.seeRange, trenchHide: CFG.trench.hide,
      smokeSight: CFG.smokeSight, levelHeight: CFG.levelHeight, eye: CFG.eye, minLevel: CFG.minLevel, woodSight: CFG.wood.sight,
      highGroundVision: CFG.highGroundVision, garrisonVision: CFG.garrisonVision, houseVision: CFG.houses.map(h => h.vision ?? null),
      rainSight: CFG.weather.sight, reconLen: SUPPORT.recon.len, reconWidth: SUPPORT.recon.width,
    },
    movement: {
      retreatSpeed: CFG.retreatSpeed, slope: CFG.slope, fordSpeed: CFG.fordSpeed, wetFord: CFG.weather.wetFord,
      wetGround: CFG.weather.wetGround, wireSpeed: CFG.wireSpeed, trenchSpeed: CFG.trench.speed, trenchRally: CFG.trench.rally,
      trenchArc: CFG.trench.arc, woodSpeed: CFG.wood.speed, woodCover: CFG.wood.cover, roadSpeed: CFG.roadSpeed,
      mudSpeed: CFG.mudSpeed, churn: CFG.churn, reverseSpeed: CFG.behavior.reverseSpeed, houseHp: CFG.houses.map(h => h.hp),
      terrainHp: CFG.terrainHp, cornerBulge: CORNER_BULGE, traffic: TRAFFIC, lane: LANE,
    },
    weather: Object.entries(WEATHER).map(([name, w]) => ({ name, sight: w.sight, rains: !!w.rains, inf: w.inf, veh: w.veh })),
  };
}

if (import.meta.main) {
  const args = process.argv.slice(2), out = args.includes('--out') ? args[args.indexOf('--out') + 1] : new URL('out/tables.json', import.meta.url).pathname;
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, JSON.stringify(tables(), null, 1) + '\n');
  console.log(`wrote ${out}`);
}
