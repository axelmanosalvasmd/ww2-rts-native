// The frozen workload's changes to shared/sim.js (issue #2), shared by harness/sim-workload.mjs (timings) and
// harness/movement-trace.mjs (the movement port's call-by-call recordings), so both run the same game.
//
// Node module hooks rewrite shared/sim.js as it loads, without touching reference/:
// - timers around the movement block of step()'s unit loop, the separation pass and the vision pass;
// - a path request keeps the current route (and waits a second before asking again), and every path search
//   returns no route, so units only follow the routes they have; idle units do not space out or seek cover (react);
// - the weather and falling debris stand still;
// - units steer by the real map, not each player's remembered view of it, which the Rust port leaves out.
// freeze() turns off crushing and ground wear. The frozen state (harness/sim-state.mjs) makes everything else quiet:
// nobody fires and nothing is in flight, so with crushing and wear off the terrain never changes.
import { registerHooks } from 'node:module';

export const PATCHES = [
  ['export const CELL', 'export const __timing = { move: 0, separate: 0, vision: 0 };\nexport const CELL'],
  ['    // Global waypoints, voluntary traffic steering and swept contact are separate steps.\n',
    '    const __moveStart = performance.now();\n    // Global waypoints, voluntary traffic steering and swept contact are separate steps.\n'],
  ['    updateGrid(g, u);\n    if (def.crushes && moved > 0)', '    updateGrid(g, u);\n    __timing.move += performance.now() - __moveStart;\n    if (def.crushes && moved > 0)'],
  ['  separateCrowds(g, list);\n', '  { const t0 = performance.now(); separateCrowds(g, list); __timing.separate += performance.now() - t0; }\n'],
  ['  if (g.tick % 4 === 1) updateVision(g);\n', '  if (g.tick % 4 === 1) { const t0 = performance.now(); updateVision(g); __timing.vision += performance.now() - t0; }\n'],
  ['function requestStepPath(g, u, to, kind) {\n', 'function requestStepPath(g, u, to, kind) {\n  u.repath = 1; return;\n'],
  ['        else { const route = findPath(g, u, goal); if (route.length) u.path = route; u.repath = 1; }\n', '        else { u.repath = 1; }\n'],
  ['  stepWorldDebris(g, TICK);\n', ''],
  ['  stepWeather(g);\n', ''],
  // units steer by the real map instead of each player's remembered view of it (rebuilt every tick from the cell log)
  ['    const trafficView = () => trafficTerrain ??= knownTerrainFor(g, u.owner);\n', '    const trafficView = () => g;\n'],
  // units follow the routes they have: no new route from spacing or cover (react) or from medics and retreats
  ['    if (!def.structure && !u.convoy && !u.logistics?.forced) react(g, u, def);\n', ''],
  ['export function findPath(g, from, to) {\n', 'export function findPath(g, from, to) {\n  return [];\n'],
];

// Loads shared/sim.js with PATCHES and then `extra` applied. Each target must occur exactly once, so a moved pin
// fails here instead of running the wrong code. Call it once per process, before anything imports sim.js.
export async function loadSim(extra = []) {
  registerHooks({
    load(url, context, nextLoad) {
      const result = nextLoad(url, context);
      if (!url.endsWith('/reference/ww2-rts/shared/sim.js')) return result;
      let source = typeof result.source === 'string' ? result.source : Buffer.from(result.source).toString('utf8');
      for (const [from, to] of [...PATCHES, ...extra]) {
        if (source.split(from).length !== 2) throw new Error(`patch target not found exactly once: ${from.slice(0, 60)}`);
        source = source.replace(from, to);
      }
      return { ...result, source };
    },
  });
  return import('../reference/ww2-rts/shared/sim.js');
}

// The rules the frozen workload turns off: tanks crush nothing and vehicles wear no ground into mud.
export function freeze(sim) {
  sim.CFG.crush = {};
  sim.CFG.traffic = 0;
}
