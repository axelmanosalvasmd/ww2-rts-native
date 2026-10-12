// Builds the game states issue #2 measures and compares. It plays the reference's Conquest bench match
// (tools/bench.mjs: Six Fronts, six Massive AI seats, mulberry32 seed 0x3c1055) to tick 6,000, checks that
// the match ends where the bench's does, then writes:
//   bench-6000.json            the match as it stands, every field of the game
//   frozen-400.json, frozen-1000.json, frozen-2000.json
//                              the same match frozen (nobody fires, no planes, nothing in flight) and grown
//                              to that many units by copying ground units beside themselves, routes kept
// Run from the repo root with x64 Node 24.21.0 (DESIGN.md, "The core's math"):
//   node harness/sim-state.mjs [--out harness/out]
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { encode, decode } from './state-codec.mjs';

const REF = new URL('../reference/ww2-rts/', import.meta.url);
const sim = await import(new URL('shared/sim.js', REF));
const ai = await import(new URL('shared/ai.js', REF));
const { aiTick, commander } = await import(new URL('shared/ai-schedule.js', REF));

const args = process.argv.slice(2), opt = (name, fallback) => (args.includes(name) ? args[args.indexOf(name) + 1] : fallback);
const OUT = opt('--out', new URL('out/', import.meta.url).pathname), TICKS = 6000, SIZES = [400, 1000, 2000];
// tools/bench.mjs --scenario conquest --ticks 6000 at the pin, x64 Node 24.21.0
// (arm64 Node ends on the same hash: the last-bit differences in its math do not reach this match)
const BENCH_HASH = '10267b847db63f39266a7e906199c9e81216a18c2c66d6bad0c6141a7e37902e';

// ---------- the bench match, as tools/bench.mjs plays it ----------
function seededRandom(initial) {
  let value = initial;
  return () => {
    let t = value += 0x6D2B79F5;
    t = Math.imul(t ^ t >>> 15, t | 1);
    t ^= t + Math.imul(t ^ t >>> 7, t | 61);
    return ((t ^ t >>> 14) >>> 0) / 4294967296;
  };
}
function playBench() {
  const map = JSON.parse(readFileSync(new URL('maps/six-fronts.json', REF), 'utf8'));
  const original = Math.random, random = seededRandom(0x3c1055);
  let randomCalls = 0;
  Math.random = () => { randomCalls++; return random(); };
  try {
    const names = Array.from({ length: 6 }, (_, i) => 'AI ' + (i + 1));
    const g = sim.createGame(map, names, true, names.map((_, i) => i), names.map((_, i) => i % 3), { mode: 'conquest', army: 'massive' });
    const seats = names.map((_, slot) => ({ slot, level: 'normal', brain: commander })), views = [], first = sim.snapshotCache(g);
    for (const { slot } of seats) views[slot] = ai.observe(g, slot, first);
    for (let t = 0; t < TICKS; t++) {
      g.players.forEach(p => { p.away = false; });
      sim.step(g);
      aiTick(g, seats, views, { sim });
      // the bench builds every seat's snapshot on even ticks, and that touches the state (grid rebuilds)
      if (g.tick % 2 === 0 || g.winner !== null) {
        const shots = g.shots, cells = g.newCells;
        g.shots = []; g.newCells = [];
        const cache = sim.snapshotCache(g);
        for (let i = 0; i < 6; i++) JSON.stringify({ ...sim.snapshotFor(g, i, shots, cells, cache) });
      }
      if (g.winner !== null) break;
    }
    const state = {
      tick: g.tick, winner: g.winner, vp: g.players.map(p => p.vp), mp: g.players.map(p => p.mp),
      units: [...g.units.values()].sort((a, b) => a.id - b.id).map(u => ({ id: u.id, type: u.type, owner: u.owner, x: u.x, z: u.z, hp: u.hp })),
    };
    return { g, randomCalls, hash: createHash('sha256').update(JSON.stringify(state)).digest('hex') };
  } finally {
    Math.random = original;
  }
}

const save = (name, g) => writeFileSync(OUT + name, JSON.stringify({ reference: 'e4e579a', node: process.version, v8: process.versions.v8, game: encode(g) }));

// ---------- the frozen workload ----------
// Nobody fires, nothing is in flight or burning, nobody digs or builds, and no smoke drifts, so no random number
// is drawn and the terrain does not change. harness/sim-workload.mjs also holds the weather still and has units
// steer by the real map. Planes leave: they fly by other rules.
export function freeze(g) {
  for (const u of [...g.units.values()]) if (u.air) g.units.delete(u.id);
  for (const u of g.units.values()) Object.assign(u, { holdFire: true, attackId: 0, targetId: 0, amove: null, fireAt: -1, dig: null, build: undefined, entrench: undefined, orders: [] });
  Object.assign(g, { shots: [], nades: [], salvos: [], strikes: [], smokes: [], newCells: [], fallingSections: [] });
  g.flights?.clear?.();
  g.fires?.clear?.();
}
// Grows the match to `size` units: ground units that move (not structures, not riding, not garrisoned), taken
// round-robin by owner in id order, each copied onto the nearest cell beside it that its kind may stand on and
// no unit stands on, keeping its route.
export function grow(g, size) {
  const occupied = new Set([...g.units.values()].map(u => Math.floor(u.z / sim.CELL) * g.w + Math.floor(u.x / sim.CELL)));
  const movers = owner => [...g.units.values()].filter(u => u.owner === owner && !u.air && !u.riding && u.garrison < 0 && !sim.UNITS[u.type].structure && sim.UNITS[u.type].speed > 0);
  const byOwner = g.players.map((_, owner) => movers(owner)), next = byOwner.map(() => 0);
  for (let owner = 0; g.units.size < size; owner = (owner + 1) % byOwner.length) {
    const list = byOwner[owner];
    if (!list.length) continue;
    const source = list[next[owner]++ % list.length], block = sim.blockOf(sim.UNITS[source.type]);
    const sx = Math.floor(source.x / sim.CELL), sz = Math.floor(source.z / sim.CELL);
    let spot = -1;
    for (let r = 1; spot < 0 && r < 40; r++) {
      for (let dz = -r; dz <= r && spot < 0; dz++) for (let dx = -r; dx <= r && spot < 0; dx++) {
        if (Math.max(Math.abs(dx), Math.abs(dz)) !== r) continue;
        const x = sx + dx, z = sz + dz, c = z * g.w + x;
        if (x >= 0 && z >= 0 && x < g.w && z < g.h && !(g.flags[c] & block) && !occupied.has(c)) spot = c;
      }
    }
    if (spot < 0) continue;
    occupied.add(spot);
    const copy = structuredClone(source);
    Object.assign(copy, {
      id: g.nextId++, x: (spot % g.w + 0.5) * sim.CELL, z: (Math.floor(spot / g.w) + 0.5) * sim.CELL,
      traffic: null, trafficWait: 0, trafficTrail: [], trafficEntered: false, queuedBehind: 0, laneScan: null, motionClearance: null, ghost: 0, stuck: 0,
    });
    g.units.set(copy.id, copy);
  }
}

if (import.meta.main) {
  if (process.arch !== 'x64') throw new Error(`run this with an x64 Node, not ${process.arch}: arm64 builds compute other last bits in V8's math`);
  mkdirSync(OUT, { recursive: true });
  const { g, randomCalls, hash } = playBench();
  console.log(`bench match: tick ${g.tick}, ${g.units.size} units, ${randomCalls} random draws, final-state sha256 ${hash}`);
  if (hash !== BENCH_HASH) throw new Error(`the match did not end as tools/bench.mjs's does (${BENCH_HASH})`);
  save('bench-6000.json', g);
  for (const size of SIZES) {
    const copy = decode(JSON.parse(JSON.stringify(encode(g))));
    freeze(copy);
    grow(copy, size);
    save(`frozen-${size}.json`, copy);
    console.log(`frozen-${size}.json: ${copy.units.size} units`);
  }
}
