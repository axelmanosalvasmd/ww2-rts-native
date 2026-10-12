// Runs issue #2's workload in the reference JavaScript: a frozen state from harness/sim-state.mjs stepped with
// no AI, timing the parts the Rust core ports, and printing what the Rust side must reproduce.
//   node harness/sim-workload.mjs harness/out/frozen-2000.json [--ticks 200]
// Prints one RESULT line of JSON: per-tick milliseconds (mean, p95) for movement, crowd separation, the vision
// pass and the fog pass, the whole step for context, and hashes of positions and fog after the run.
//
// Module hooks rewrite shared/sim.js as it loads, without touching reference/:
// - timers around the movement block of step()'s unit loop, the separation pass and the vision pass;
// - a path request keeps the current route (and waits a second before asking again), so no path search runs;
// - the weather and falling debris stand still;
// - units steer by the real map, not each player's remembered view of it, which the Rust port leaves out.
// The frozen state makes everything else quiet: nobody fires, nothing is in flight, and with crushing and
// ground wear off the terrain never changes. The run checks that: no random draw and no terrain change.
import { registerHooks } from 'node:module';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { decode } from './state-codec.mjs';

const PATCHES = [
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
];
registerHooks({
  load(url, context, nextLoad) {
    const result = nextLoad(url, context);
    if (!url.endsWith('/reference/ww2-rts/shared/sim.js')) return result;
    let source = typeof result.source === 'string' ? result.source : Buffer.from(result.source).toString('utf8');
    for (const [from, to] of PATCHES) {
      // each target must occur exactly once, so a moved pin fails here instead of timing the wrong code
      if (source.split(from).length !== 2) throw new Error(`patch target not found exactly once: ${from.slice(0, 60)}`);
      source = source.replace(from, to);
    }
    return { ...result, source };
  },
});
const sim = await import('../reference/ww2-rts/shared/sim.js');

const args = process.argv.slice(2), file = args[0], TICKS = Number(args.includes('--ticks') ? args[args.indexOf('--ticks') + 1] : 200);
if (!file) throw new Error('usage: node harness/sim-workload.mjs <state.json> [--ticks N]');
const saved = JSON.parse(readFileSync(file, 'utf8')), g = decode(saved.game);
sim.CFG.crush = {};
sim.CFG.traffic = 0;

let draws = 0;
const random = Math.random;
Math.random = () => { draws++; return random(); };
const terrainBefore = g.terrainVersion, teams = [...new Set(g.players.map(p => p.team))];
const samples = { step: [], move: [], separate: [], vision: [], fog: [] };
for (let t = 0; t < TICKS; t++) {
  const before = { ...sim.__timing }, start = performance.now();
  sim.step(g);
  const stepped = performance.now();
  let fog = 0;
  // the fog pass runs when a snapshot asks for it after a vision tick; here every team asks every vision tick
  if (g.tick % 4 === 1) { for (const team of teams) sim.teamFog(g, team); fog = performance.now() - stepped; }
  samples.step.push(stepped - start);
  for (const k of ['move', 'separate', 'vision']) samples[k].push(sim.__timing[k] - before[k]);
  samples.fog.push(fog);
}
Math.random = random;
if (draws) throw new Error(`the frozen workload drew ${draws} random numbers`);
if (g.terrainVersion !== terrainBefore) throw new Error('the frozen workload changed the terrain');

const stats = values => {
  const sorted = values.toSorted((a, b) => a - b), mean = values.reduce((a, b) => a + b, 0) / values.length;
  return { mean: +mean.toFixed(3), p95: +sorted[Math.ceil(sorted.length * 0.95) - 1].toFixed(3) };
};
const visionTicks = k => samples[k].filter((_, i) => (i + 1 + saved.game.tick) % 4 === 1);
const sha = value => createHash('sha256').update(JSON.stringify(value)).digest('hex');
const units = [...g.units.values()].sort((a, b) => a.id - b.id).map(u => [u.id, u.x, u.z, u.rot, u.moveSpeed ?? 0]);
console.log('RESULT ' + JSON.stringify({
  state: file, units: g.units.size, ticks: TICKS, node: process.version, arch: process.arch,
  ms: { step: stats(samples.step), move: stats(samples.move), separate: stats(samples.separate), vision: stats(visionTicks('vision')), fog: stats(visionTicks('fog')) },
  hashes: { units: sha(units), fog: sha(teams.map(team => Array.from(sim.teamFog(g, team).vis))) },
}));
