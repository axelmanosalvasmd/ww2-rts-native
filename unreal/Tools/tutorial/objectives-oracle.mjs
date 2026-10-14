// Oracle for the Client/TCScenarioText and Client/TCTutorial tests: runs the pinned reference client's own modules under
// node (client/objectives.js with stub hooks and a minimal document, client/i18n.js spanish()) and prints JSON.
//   node objectives-oracle.mjs <reference copy with node_modules (three)>
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
const ref = process.argv[2];
let clock = 100;
globalThis.performance = { now: () => clock * 1000 };
// a minimal document: objectives.js init() appends its banner, then its goal box, to #hud
const box = (id) => ({ id, hidden: true, textContent: '', style: {}, dataset: {}, offsetHeight: 30, setAttribute() {},
  classList: { on: new Set(), toggle(c, v) { v ? this.on.add(c) : this.on.delete(c); }, contains(c) { return this.on.has(c); } } });
const boxes = [];
const hud = { ...box('hud'), append: (e) => boxes.push(e), currentCSSZoom: 1 };
globalThis.document = { querySelector: () => ({}), createElement: () => box(''), head: { append() {} },
  getElementById: (id) => id === 'hud' ? hud : id === 'top' ? { getBoundingClientRect: () => ({ bottom: 100 }) } : null };
const { createObjectives } = await import(pathToFileURL(join(ref, 'client/objectives.js')));
const { spanish } = await import(pathToFileURL(join(ref, 'client/i18n.js')));
const { STEPS } = await import(pathToFileURL(join(ref, 'shared/tutorial.js')));
const out = {};
const bursts = [];
const units = new Map(); // main.js units: type, owner, x, z and the newest snapshot position tx, tz
const hooks = { points: () => [], units, effects: { plume: (k, x, y, z, r) => bursts.push([k, +x.toFixed(3), +y.toFixed(3), +z.toFixed(3), r]) },
  hAt: () => 0, camera: null, cam: null, colorOf: () => 0, me: () => 0, friend: (s) => s === 0 || s === 2 };
const o = createObjectives(hooks); o.init();
const [bannerBox, goalBox] = boxes;

// the goal box: text, hidden, scenario-goals class
const cases = [
  { mode: { kind: 'tutorial', step: 2, steps: 9, goal: 'Take the crossroads: keep a squad by the flag' } },
  { scenario: { objectives: [{ id: 'a', state: 'active', text: 'Hold the ridge' }, { id: 'b', state: 'complete', text: { en: 'Take the village', es: 'Toma el pueblo' } }, { id: 'c', state: 'hidden', text: 'Secret' }, { id: 'd', state: 'failed', text: 'Lost cause' }, { id: 'e', state: 'active', text: { es: 'Solo espanol' } }] } },
  { mode: { kind: 'tutorial', goal: 'Cross the bridge and take the bridgehead' }, scenario: { objectives: [{ id: 'a', state: 'active', text: 'Both' }] } },
  { scenario: { objectives: [] } },
  {},
];
out.goalModule = cases.map(c => { o.snapshot({ units: [], points: [], shots: [], ...c }); return { text: goalBox.textContent, hidden: goalBox.hidden, scenario: goalBox.classList.contains('scenario-goals') }; });

// collapse(): each case 10 s after the last, so no banner is showing
units.set(10, { type: 'barracks', owner: 0, x: 50, z: 50, tx: 50, tz: 50 });
units.set(11, { type: 'depot', owner: 1, x: 80, z: 80, tx: 80, tz: 80 });
units.set(12, { type: 'hq', owner: 2, x: 120, z: 40, tx: 120, tz: 40 });
units.set(13, { type: 'rifle', owner: 1, x: 81, z: 81, tx: 81, tz: 81 });
units.set(14, { type: 'bunker', owner: 1, x: 200, z: 200, tx: 200, tz: 200 });
const collapseCases = [
  { k: 'collapse', name: 'church', x: 1, z: 1 },
  { k: 'collapse', t: 10, x: 0, z: 0 },
  { k: 'collapse', t: 13, x: 82, z: 81 },
  { k: 'collapse', x: 122, z: 41 },
  { k: 'collapse', x: 300, z: 300 },
  { k: 'collapse', t: 14, x: 200, z: 200 },
  { k: 'collapse', x: 83.9, z: 80 },
];
out.collapse = collapseCases.map(sh => { clock += 10; bannerBox.textContent = ''; bannerBox.hidden = true; o.collapse(sh); return { sh, text: bannerBox.hidden ? '' : bannerBox.textContent }; });
// a second collapse 2 s later keeps the first banner; frame() hides it after 4 s
clock += 10; bannerBox.hidden = true;
o.collapse({ k: 'collapse', name: 'mill', x: 0, z: 0 }); const first = bannerBox.textContent;
clock += 2; o.collapse({ k: 'collapse', name: 'barn', x: 0, z: 0 }); const second = bannerBox.textContent;
clock += 1.9; o.frame(0); const at39 = bannerBox.hidden;
clock += 0.2; o.frame(0); const at41 = bannerBox.hidden;
out.bannerTiming = { first, second, hiddenAt3_9: at39, hiddenAt4_1: at41 };

// damaged buildings and the smoke cadence (no camera: no ranking; High graphics)
const row = (id, type, owner, x, z, hp, built = 1) => [id, type, owner, x, z, 0, 0, hp, 0, 0, 0, 0, 0, 0, built];
o.snapshot({ points: [], shots: [], units: [row(1, 'hq', 0, 10, 10, 3000), row(2, 'hq', 0, 20, 10, 1980), row(3, 'hq', 0, 30, 10, 1981), row(4, 'depot', 1, 40, 10, 198), row(5, 'barracks', 0, 50, 10, 400, 0.5), row(6, 'rifle', 0, 60, 10, 10), row(7, 'pillbox', 1, 70, 10, 0), row(8, 'tower', 0, 80, 10, 99)] });
out.emitters = o.emitters;
out.frames = [0.1, 0.1, 0.1, 0.05, 0.2, 0.3].map(dt => { bursts.length = 0; o.frame(dt); return bursts.slice(); });
// the depot heals past 66 %: it leaves; the tower burns on with its timers
o.snapshot({ points: [], shots: [], units: [row(4, 'depot', 1, 40, 10, 500), row(8, 'tower', 0, 80, 10, 99)] });
out.emittersAfter = o.emitters;
bursts.length = 0; o.frame(0.3); out.frameAfter = bursts.slice();

// the Spanish of the Tutorial's texts through the reference dictionary (what the page shows in Spanish)
out.spanish = {};
for (const s of STEPS) for (const t of [`Objective: ${s.goal}`, s.goal, ...s.say]) out.spanish[t] = spanish(t);
for (const t of ['The church has collapsed', 'Your Barracks has collapsed', 'The enemy Supply Depot has collapsed', 'The allied HQ has collapsed', 'Tutorial',
  'The bridge is ours. Training complete: you are ready for a real match.', 'New here? Learn the basics in about ten minutes',
  'Learn to play: Sergeant Hollis walks you from a glider landing to a bridge, one order at a time. Friends can join as co-op.']) out.spanish[t] = spanish(t);
console.log(JSON.stringify(out, null, 1));
