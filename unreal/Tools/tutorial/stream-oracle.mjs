// Feeds a recorded stream (player.mjs) through the reference client's own client/world-state.js and client/objectives.js
// (stub hooks: main.js's units map rebuilt from the rows) and prints what objectives.js shows: the goal box text at each
// change, the collapse shots and the banners they raised, and the damaged-building emitters seen.
//   node stream-oracle.mjs <reference copy with node_modules> <stream.jsonl>
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
const [ref, file] = process.argv.slice(2);
let clock = 0;
globalThis.performance = { now: () => clock * 1000 };
const box = (id) => ({ id, hidden: true, textContent: '', style: {}, dataset: {}, offsetHeight: 30, setAttribute() {},
  classList: { on: new Set(), toggle(c, v) { v ? this.on.add(c) : this.on.delete(c); }, contains(c) { return this.on.has(c); } } });
const boxes = [];
const hud = { ...box('hud'), append: (e) => boxes.push(e), currentCSSZoom: 1 };
globalThis.document = { querySelector: () => ({}), createElement: () => box(''), head: { append() {} },
  getElementById: (id) => id === 'hud' ? hud : id === 'top' ? { getBoundingClientRect: () => ({ bottom: 100 }) } : null };
const { createObjectives } = await import(pathToFileURL(join(ref, 'client/objectives.js')));
const { createWorldState } = await import(pathToFileURL(join(ref, 'client/world-state.js')));
const units = new Map();
let me = 0, teams = [];
const o = createObjectives({ points: () => [], units, effects: { plume() {} }, hAt: () => 0, camera: null, cam: null, colorOf: () => 0,
  me: () => me, friend: (s) => (teams[s] ?? s) === (teams[me] ?? me) });
o.init();
const [bannerBox, goalBox] = boxes;
const world = createWorldState();
const out = { goals: [], collapses: 0, banners: [], emittersMax: 0, snapshots: 0 };
let lastGoal = null;
for (const line of readFileSync(file, 'utf8').split('\n')) {
  if (!line) continue;
  const m = JSON.parse(line);
  if (m.t === 'start') { me = m.you; teams = m.teams ?? []; world.reset(); units.clear(); o.reset(); continue; }
  if (m.t !== 's') continue;
  out.snapshots++;
  clock = m.tick / 20;
  world.apply(m);
  const seen = new Set();
  for (const [id, type, owner, x, z] of m.units) { seen.add(id); const v = units.get(id) ?? { type, owner }; Object.assign(v, { type, owner, x, z, tx: x, tz: z }); units.set(id, v); }
  for (const id of units.keys()) if (!seen.has(id)) units.delete(id);
  const before = bannerBox.hidden ? '' : bannerBox.textContent;
  out.collapses += (m.shots ?? []).filter(s => s.k === 'collapse').length;
  o.snapshot(m);
  const after = bannerBox.hidden ? '' : bannerBox.textContent;
  if (after && after !== before) out.banners.push([m.tick, after]);
  o.frame(0.05);
  out.emittersMax = Math.max(out.emittersMax, o.emitters.length);
  if (goalBox.textContent !== lastGoal) { lastGoal = goalBox.textContent; out.goals.push([m.tick, lastGoal]); }
}
console.log(JSON.stringify(out, null, 1));
