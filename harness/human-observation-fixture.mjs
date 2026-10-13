// Record unchanged ordinary ai.observe/viewFor readers for the detached human seam.
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

if (process.version !== 'v24.21.0' || process.arch !== 'x64') throw Error('Use x64 Node 24.21.0');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
if (git(['rev-parse', 'HEAD']) !== pin || git(['status', '--porcelain', '--untracked-files=no'])) throw Error('Use clean pinned reference');
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const ai = await import(pathToFileURL(`${reference}/shared/ai.js`));
const { aiTick } = await import(pathToFileURL(`${reference}/shared/ai-schedule.js`));
const { rebuildGrid } = await import(pathToFileURL(`${reference}/shared/grid.js`));
const humanSource = readFileSync(`${reference}/shared/ai-human.js`, 'utf8');
const hurtGuard = humanSource.match(/if \((e\.k !== 'hurt'.*?)\) continue;/)?.[1];
if (!hurtGuard) throw Error('Original hurt admission guard not found');
// Execute the unchanged original guard expression, before camera/area policy.
const admittedHurt = new Function('e', 'slot', `return !(${hurtGuard});`);
const map = { name: 'Human detached observations', w: 48, h: 48,
  rows: Array.from({ length: 48 }, (_, z) => '.'.repeat(48).split('').map((c, x) => z === 15 && x === 15 ? 'N' : c).join('')),
  spawns: [{ x: 3, y: 3 }, { x: 42, y: 42 }, { x: 8, y: 3 }],
  points: [{ x: 23, y: 15 }, { x: 30, y: 30 }, { x: 18, y: 35 }] };
const baseUnits = [
  { id: 1, owner: 0, type: 'rifle', x: 20.049, z: 20.051 },
  { id: 2, owner: 0, type: 'tank', x: 24, z: 20 },
  { id: 3, owner: 0, type: 'engineer', x: 20, z: 25 },
  { id: 4, owner: 2, type: 'rifle', x: 17, z: 20 },
  { id: 5, owner: 1, type: 'rifle', x: 80, z: 80 },
  { id: 6, owner: 1, type: 'mg', x: 83, z: 80 },
  { id: 7, owner: 1, type: 'barracks', x: 80, z: 83 },
];
const tape = [
  { tick: 0 },
  { tick: 2, units: [{ id: 5, x: 38.049, z: 20.051, hp: 59.1 }, { id: 6, x: 43, z: 20 }, { id: 7, x: 40, z: 25 },
    { id: 1, worldGoal: { x: 60.049, z: 40.051 }, path: [{ x: 30.051, z: 25.049 }] }] },
  { tick: 4, units: [{ id: 1, amove: { x: 55.051, z: 42.049 } }, { id: 5, x: 80, z: 80 }] },
  { tick: 6, units: [{ id: 1, attackId: 6, targetId: 6 }, { id: 2, targetId: 5 }] },
  { tick: 8, units: [{ id: 1, attackId: 0, targetId: 0, nade: { x: 44, z: 25 } }], mp: 82.9 },
  { tick: 10, units: [{ id: 1, nade: null, board: 2 }, { id: 6, x: 43.06 }] },
  { tick: 12, units: [{ id: 1, board: 0, amove: null, worldGoal: null }, { id: 6, x: 43.2 }], knownMine: true },
  { tick: 1202 },
  { tick: 1204, units: [{ id: 6, x: 80, z: 82 }, { id: 7, x: 80, z: 83 }] },
  { tick: 1206 },
  { tick: 1210, units: [{ id: 5, x: 38, z: 20 }, { id: 6, x: 43, z: 20 }] },
  { tick: 1211, finish: true },
];
const runs = values => {
  const out = [];
  for (const value of values) {
    const last = out.at(-1);
    if (last && last[0] === value) last[1]++; else out.push([value, 1]);
  }
  return out;
};
function capture(view, slot, tech, memory) {
  const own = [...view.units.values()].filter(u => u.owner === slot && sim.UNITS[u.type].structure);
  const ready = kind => own.filter(u => u.type === kind && u.hp > 0 && (u.built ?? 1) >= 1).length;
  return { tick: view.tick, spawn: view.players[slot].spawn,
    units: [...view.units.values()].map(u => ({ id: u.id, owner: u.owner, type: u.type, x: u.x, z: u.z, hp: u.hp,
      worldGoal: u.worldGoal ?? null, amove: u.amove, target: u.targetId, retreating: u.retreating,
      air: !!u.air, build: !!u.build, dig: !!u.dig, entrench: !!u.entrench, garrison: u.garrison >= 0,
      supp: u.supp, cover: u.cover, path: u.path, attack: !!u.attackId, enter: u.enter >= 0, fire: u.fireAt >= 0, nade: !!u.nade })),
    visible: [...view.players[slot].visible], sightings: view.sightings,
    still: [...memory.still].map(([id, s]) => ({ id, x: s.x, z: s.z, since: s.since })),
    packet: { w: view.w, h: view.h, tick: view.tick, mp: view.players[slot].mp,
      players: view.players.map(p => [p.team, p.out, p.spawn]),
      points: view.points.map(p => [p.x, p.z, p.vp, p.mp, p.owner, p.cut, p.locked]),
      chars: view.chars.join(''), flags: runs(view.flags),
      research: tech ? { faction: view.players[slot].faction,
        tech: [view.players[slot].tech.tier, view.players[slot].tech.armory.inf,
          view.players[slot].tech.armory.guns, view.players[slot].tech.armory.armor],
        busy: view.players[slot].tech.lab.map(j => j[0]), standard: true,
        stock: own.length === 2 && ready('hq') === 1 && ready('barracks') === 1, armory: ready('armory') !== 0 } : null } };
}
const cases = [];
for (const mode of [undefined, 'conquest']) for (const tech of [false, true]) {
  const oldRandom = Math.random;
  let g;
  Math.random = () => 0.25;
  try { g = sim.createGame(map, ['A', 'B', 'Ally'], false, [0, 1, 0], [0, 1, 2],
    { ...(mode === undefined ? {} : { mode }), weather: false, tech, logistics: false, supply: false, shuffle: false }); }
  finally { Math.random = oldRandom; }
  g.seed = 841;
  g.units.clear(); g.nextId = 1;
  for (const spec of baseUnits) {
    sim.internals.spawnUnit(g, spec.owner, spec.type, g.units.size);
    Object.assign(g.units.get(spec.id), spec);
  }
  const frames = [];
  for (const action of tape) {
    for (const patch of action.units ?? []) Object.assign(g.units.get(patch.id), structuredClone(patch));
    if (action.mp !== undefined) g.players[0].mp = action.mp;
    if (action.knownMine) g.mineSeen.set(15 * g.w + 15, 1);
    rebuildGrid(g);
    if (action.tick > 0) { g.tick = action.tick - 1 - ((action.tick - 2) % 4); sim.internals.updateVision(g); }
    g.tick = action.tick;
    if (action.finish) { g.players[0].vp = g.winVp; sim.finish(g, 0, 'vp'); }
    const delivered = [];
    aiTick(g, [{ slot: 0, brain: { observe: ai.observe, wantsView: () => true, act: () => {} } }], delivered);
    const view = delivered[0];
    if (!view) throw Error('Original snapshot delivery did not admit the recorded frame');
    frames.push({ action, expected: capture(view, 0, tech, ai.aiMemories(g)[0]) });
  }
  cases.push({ name: `${mode ?? 'default'}-tech-${tech}`, mode: mode ?? 'default', tech, map, baseUnits, frames });
}
const shots = [ { k: 'flight' }, { k: 'hurt', to: 1, t: 5, x: 38, z: 20 },
  { k: 'hurt', to: 0, t: 1, x: 20.049, z: 20.051 }, { k: 'hurt', to: 0, t: 0, x: 20, z: 20 },
  { k: 'shell' }, { k: 'hurt', to: 0, t: 3, x: 20, z: 25 } ];
const hurtFrames = [ { generation: 41, shots: shots.slice(0, 4) }, { generation: 41, shots },
  { generation: 42, shots: [shots[5]] } ].map(f => ({ ...f, expected: f.shots.flatMap((e, ordinal) =>
    admittedHurt(e, 0) ? [{ ordinal, id: e.t, at: { x: e.x, z: e.z } }] : []) }));
const content = JSON.stringify({ reference: pin, cases, hurtFrames });
const output = `${root}/rust/crates/core/tests/fixtures/human-observation.json`;
if (process.argv.includes('--check')) { if (readFileSync(output, 'utf8') !== content) throw Error('Human observation fixture differs'); }
else writeFileSync(output, content);
if (git(['status', '--porcelain', '--untracked-files=no'])) throw Error('Reference changed');
console.log(`Human observations: ${cases.length} scenes, ${cases.reduce((n, c) => n + c.frames.length, 0)} frames, ${hurtFrames.length} hurt batches`);
