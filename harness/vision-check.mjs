// The JavaScript side of the vision and fog parity check: loads a game state (harness/sim-state.mjs, or a
// fixture from harness/vision-fixture.mjs), runs the reference's vision pass (internals.updateVision) and fog
// pass (teamFog) for every team, and prints what the Rust side must reproduce. `simtest <state>` in rust/
// prints the same lines from the same file; the two outputs must be identical.
//   node harness/vision-check.mjs <state.json> [--bench N]
//
// Four passes, each followed by every team's fog:
//   1. the state as loaded, with the fog caches it was saved with (kept cells of still sources, fog terrain);
//   2. and 3. four ticks later each, nothing moved: still sources work their kept cells out, then reuse them;
//   4. four ticks later, with the kept cells and the fog terrain dropped, so every source is worked out again.
// Per pass it prints each player's visible unit ids (sorted), each team's fog (FNV-1a hashes of `vis`, `explored`
// and the latest packed change, and the version) and a hash of the kept sources.
//
// The passes use Math.cos and Math.sin only for ships (hullPoint) and live recon flights (inStrip); everything
// else is hypot, floor, ceil, sqrt and comparisons, which arm64 and x64 Node compute alike. A state with ships or
// live recon flights must be checked with x64 Node (DESIGN.md, "The core's math").
//
// --bench N times N passes with every source worked out again (as when every unit moves) and N with every source
// standing still, after a few untimed ones, and prints mean and p95 milliseconds for the vision pass, the fog pass
// (all teams) and both. Timing is fine on arm64 Node.
import { readFileSync } from 'node:fs';
import { decode } from './state-codec.mjs';

const sim = await import('../reference/ww2-rts/shared/sim.js');
const { updateVision } = sim.internals;

// FNV-1a, 32 bits, over bytes
function fnv(bytes) {
  let h = 0x811c9dc5;
  for (let i = 0; i < bytes.length; i++) h = Math.imul(h ^ bytes[i], 0x01000193) >>> 0;
  return h.toString(16).padStart(8, '0');
}
// the kept sources, in id order: id, x, z, garrison, then the cells (or 0xffffffff while none are kept), the terrain
// version, the box, reach, range and how many smoke clouds were near
function keptHash(kept) {
  const parts = [];
  for (const id of [...kept.keys()].sort((a, b) => a - b)) {
    const e = kept.get(id), cells = e.cells ? Array.from(e.cells) : null;
    const b = new DataView(new ArrayBuffer(24 + (cells ? 4 + cells.length * 4 + 4 + 16 + 16 + 4 : 4)));
    let o = 0;
    const u32 = v => { b.setUint32(o, v, true); o += 4; }, i32 = v => { b.setInt32(o, v, true); o += 4; }, f64 = v => { b.setFloat64(o, v, true); o += 8; };
    u32(id); f64(e.x); f64(e.z); i32(e.garrison);
    if (!cells) u32(0xffffffff);
    else { u32(cells.length); for (const c of cells) u32(c); u32(e.version); for (const v of e.box) i32(v); f64(e.reach); f64(e.range); u32(e.smokes.length); }
    parts.push(new Uint8Array(b.buffer));
  }
  const all = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) { all.set(p, o); o += p.length; }
  return fnv(all);
}
const teamsOf = g => [...new Set(g.players.map(p => p.team))];
const text = s => fnv(new TextEncoder().encode(s));

export function needsX64(g) {
  return [...g.units.values()].some(u => sim.UNITS[u.type].hull) || g.strikes.some(s => s.live && s.kind === 'recon');
}

export function check(g) {
  const lines = [];
  for (let pass = 1; pass <= 4; pass++) {
    if (pass > 1) g.tick += 4;
    if (pass === 4) { g.fogSources = undefined; g.fogTerrain = undefined; }
    updateVision(g);
    lines.push(`pass ${pass} tick ${g.tick}`);
    for (const p of g.players) lines.push(['visible', p.slot, p.visible.size, ...[...p.visible].sort((a, b) => a - b)].join(' '));
    for (const team of teamsOf(g)) {
      const f = sim.teamFog(g, team), delta = f.deltas.get(f.version - 1);
      lines.push(`fog ${team} vis ${fnv(f.vis)} explored ${fnv(f.explored)} version ${f.version} delta ${delta === undefined ? '-' : text(delta)}`);
    }
    lines.push(`kept ${g.fogSources.size} ${keptHash(g.fogSources)}`);
  }
  return lines;
}

export function bench(g, n) {
  const teams = teamsOf(g), stats = values => {
    const sorted = values.toSorted((a, b) => a - b), mean = values.reduce((a, b) => a + b, 0) / values.length;
    return `${mean.toFixed(3)} ${sorted[Math.ceil(sorted.length * 0.95) - 1].toFixed(3)}`;
  };
  const lines = [];
  for (const [name, cold] of [['moving', true], ['still', false]]) {
    const vision = [], fog = [], both = [];
    for (let i = -5; i < n; i++) {
      g.tick += 4;
      if (cold) g.fogSources = undefined;
      const t0 = performance.now();
      updateVision(g);
      const t1 = performance.now();
      for (const team of teams) sim.teamFog(g, team);
      const t2 = performance.now();
      if (i >= 0) { vision.push(t1 - t0); fog.push(t2 - t1); both.push(t2 - t0); }
    }
    lines.push(`bench ${name} units ${g.units.size} passes ${n} vision ${stats(vision)} fog ${stats(fog)} both ${stats(both)} (ms mean p95)`);
  }
  return lines;
}

if (import.meta.main) {
  const args = process.argv.slice(2), file = args[0];
  if (!file) throw new Error('usage: node harness/vision-check.mjs <state.json> [--bench N]');
  const g = decode(JSON.parse(readFileSync(file, 'utf8')).game);
  if (needsX64(g) && process.arch !== 'x64') throw new Error('this state has ships or live recon flights, whose Math.cos and Math.sin differ on arm64 Node: run it with x64 Node');
  const ids = [...g.units.keys()];
  if (ids.some((id, i) => i > 0 && id <= ids[i - 1])) throw new Error('units are not in ascending id order, which the Rust side assumes');
  for (const line of check(g)) console.log(line);
  if (args.includes('--bench')) for (const line of bench(g, Number(args[args.indexOf('--bench') + 1]))) console.log(line);
}
