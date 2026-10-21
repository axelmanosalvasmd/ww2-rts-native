// Can an honest scripted player finish the reference tutorial? Runs the pinned sim (read only) with a policy that sees
// only what one seat's snapshot carries (own rows, seen enemy rows, mode, mp). Usage: node drive.mjs <reference dir> (the sim seeds itself; each run differs)
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
const ref = process.argv[2];
const sim = await import(pathToFileURL(join(ref, 'shared/sim.js')));
const { decide } = await import('./policy.mjs');
const { createGame, step, command, snapshotFor, snapshotCache, CELL } = sim;
const map = JSON.parse(readFileSync(join(ref, 'maps/tutorial.json'), 'utf8'));
const g = createGame(map, ['me'], false, [0], [1], { mode: 'tutorial' });
const memo = {}; let said = 0, last = -1;
const log = [];
for (let i = 0; i < 20 * 60 * 25 && g.winner === null; i++) {
  step(g);
  said += g.shots.filter(s => s.k === 'say').length;
  if (i % 4 === 0) { // the server's snapshot cadence (every 4th tick at 20 Hz: 5 a second)
    const s = snapshotFor(g, 0, g.shots, [], snapshotCache(g));
    for (const c of decide(s, memo, CELL)) { const r = command(g, 0, c); if (r && r !== true) log.push(`${g.tick} ${c.t} -> ${r}`); }
  }
  g.shots = [];
  if (g.mode.step !== last) { last = g.mode.step; console.log(`tick ${g.tick} (${(g.tick / 20).toFixed(0)} s) step ${g.mode.step + 1}/${g.mode.steps}: ${g.mode.goal}`); }
}
console.log('winner', g.winner, g.endReason, 'tick', g.tick, 'said', said);
console.log('refused commands:', log.length, log.slice(0, 15).join('\n'));
