// Run the unchanged seat thinker against detached source observations and explicit public overlays.
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

if (process.version !== 'v24.21.0' || process.arch !== 'x64') throw Error('Use x64 Node 24.21.0');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => {
  if (git(['rev-parse', 'HEAD']) !== pin || git(['status', '--porcelain', '--untracked-files=no'])) throw Error('Use clean pinned reference');
};
clean();
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const ai = await import(pathToFileURL(`${reference}/shared/ai.js`));
const { planner } = await import(pathToFileURL(`${reference}/shared/ai-schedule.js`));
const runs = values => {
  const out = [];
  for (const value of values) {
    if (out.length && out.at(-1)[0] === value) out.at(-1)[1]++;
    else out.push([value, 1]);
  }
  return out;
};
const types = ['rifle', 'mg', 'at', 'tank', 'armoredcar', 'engineer'];
const unit = (id, type = 'rifle', extra = {}) => ({ id, type, x: 17 + id % 5 * 2, z: 17 + Math.floor(id / 5) * 2, ...extra });
const point = (x, z, extra = {}) => ({ x, z, vp: 1, mp: 1, owner: -1, capper: -1, progress: 0, contested: false, cut: false, locked: false, ...extra });
const defaults = [point(55, 21), point(27, 55), point(65, 65, { vp: 2 })];
// A neutral locked objective keeps these frames inside the source opening gate.
const openingSentinel = () => point(89, 89, { locked: true });
const cases = [];

function record(name, options = {}) {
  const seats = options.seats ?? 2, slot = options.slot ?? 0, level = options.level ?? 'normal';
  const map = {
    name: 'Quiet commander oracle', w: 48, h: 48, rows: Array(48).fill('.'.repeat(48)),
    spawns: [{ x: 4, y: 4 }, { x: 43, y: 43 }, { x: 4, y: 43 }, { x: 43, y: 4 }, { x: 23, y: 4 }, { x: 23, y: 43 }],
    points: [{ x: 27, y: 10 }, { x: 13, y: 27 }, { x: 32, y: 32 }],
  };
  const oldRandom = Math.random;
  let game, startupDraws = 0;
  Math.random = () => { startupDraws++; return 0.5; };
  try {
    game = sim.createGame(map, Array.from({ length: seats }, (_, i) => `Seat ${i}`), false,
      Array.from({ length: seats }, (_, i) => i), Array.from({ length: seats }, (_, i) => i % 4),
      { mode: 'conquest', weather: false, tech: false, logistics: false, supply: false });
    game.units.clear();
    for (const [index, type] of types.entries()) sim.internals.spawnUnit(game, slot, type, index);
  } finally { Math.random = oldRandom; }
  const view = ai.observe(game, slot, undefined);
  // Every template was decoded by viewFor from a real source snapshot. The following
  // inputs are a defined component frame, not inferred from native policy output.
  const templates = new Map([...view.units.values()].map(u => [u.type, u]));
  const specs = options.units ?? Array.from({ length: 12 }, (_, i) => unit(i + 1, types[i % 5]));
  const units = specs.map(spec => {
    const template = templates.get(spec.type);
    if (!template) throw Error(`Missing decoded ${spec.type}`);
    const u = { ...structuredClone(template), ...structuredClone(spec), owner: slot, hp: sim.UNITS[spec.type].models * sim.UNITS[spec.type].hpPer };
    for (const key of ['air', 'orderPlan']) u[key] ??= null;
    return u;
  }).sort((a, b) => a.id - b.id);
  view.units = new Map(units.map(u => [u.id, structuredClone(u)]));
  view.points = structuredClone(options.points ?? defaults);
  view.tick = options.tick ?? 40;
  const me = view.players[slot];
  Object.assign(me, { mp: 0, mun: 0, fuel: 0, visible: new Set(), out: !!options.out });
  const chars = [...view.chars], flags = [...view.flags], height = [...(view.height ?? Array(view.w * view.h).fill(0))];
  for (const [cell, ch, level = 0] of options.terrain ?? []) {
    chars[cell] = ch; flags[cell] = sim.TERRAIN[ch]; height[cell] = level;
  }
  Object.assign(view, { chars, flags, height, mapChars: chars, mapHeight: height, sightings: [] });
  const memory = { aim: options.aim ?? null, pointFailures: options.pointFailures ?? [] };
  const mem = { mind: { roster: new Map(), respect: {}, points: memory.pointFailures.map(failed => ({ failed })),
    owned: [], noticed: new Map(), lastSeen: new Map(), pointSeen: [], aim: structuredClone(memory.aim), decision: null } };
  // The oracle RNG is an explicit replay tape. Match stream isolation is tested separately.
  let seed = (0x516e9200 + cases.length) >>> 0;
  const random = Array.from({ length: 128 }, () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed / 4294967296; });
  let draws = 0;
  const commands = [], otherCommands = [];
  Math.random = () => { if (draws === random.length) throw Error('Thinker exhausted oracle RNG tape'); return random[draws++]; };
  try {
    ai.think(game, slot, { level, view, memory: mem, submit: command => {
      (['move', 'amove'].includes(command.t) ? commands : otherCommands).push(structuredClone(command));
      return undefined;
    } });
  } finally { Math.random = oldRandom; }
  // Holder behavior remains pending in native, but its actual source commands stay in the record.
  if (otherCommands.some(c => !['cover', 'garrison'].includes(c.t))) throw Error(`Unsupported full-thinker branch in ${name}: ${JSON.stringify(otherCommands)}`);
  const emitted = commands.reduce((n, c) => n + c.orders.length, 0);
  if (draws !== emitted) throw Error(`Unexpected non-waypoint randomness in ${name}`);
  cases.push({ name, slot, level, startupDraws, frame: {
    w: view.w, h: view.h, tick: view.tick, chars: runs(chars), flags: runs(flags), height: runs(height),
    players: view.players.map(p => ({ slot: p.slot, team: p.team, faction: p.faction, spawn: p.spawn, out: p.out })),
    points: view.points, units,
    me: { mp: 0, mun: 0, fuel: 0, visible: [], out: me.out },
  }, memory, random: random.slice(0, draws), expected: { draws, commands, pendingCommands: otherCommands, aim: mem.mind.aim } });
}

for (const level of ['easy', 'normal', 'hard']) for (let seats = 1; seats <= 6; seats++) {
  record(`opening-${level}-${seats}`, { level, seats, slot: seats - 1 });
}
for (const level of ['easy', 'normal', 'hard']) {
  record(`cut-bypasses-hands-${level}`, { level, points: [point(55, 55, { owner: 0, cut: true }), openingSentinel()] });
  record(`own-uncut-keeps-home-${level}`, { level, points: [point(55, 55, { owner: 0 }), openingSentinel()] });
  record(`locked-${level}`, { level, points: defaults.map(p => ({ ...p, locked: true })) });
  record(`equal-scores-${level}`, { level, points: [point(43, 21), point(19, 45)], units: [unit(1, 'tank', { x: 19, z: 21 })] });
  record(`income-${level}`, { level, points: [point(35, 21), point(65, 21, { mp: 6 }), point(69, 21, { vp: 3 })] });
  record(`load-path-${level}`, { level, units: [unit(1, 'rifle', { path: [{ x: 55, z: 21 }], orderPlan: { kind: 1, x: 55, z: 21 } }), unit(2), unit(3), unit(4)] });
  record(`loyalty-${level}`, { level, aim: { i: 1, until: 20 }, points: [point(45, 21), point(49, 21)] });
  record(`expired-aim-${level}`, { level, aim: { i: 1, until: 2 }, points: [point(45, 21), point(49, 21)] });
  record(`owned-aim-cleared-${level}`, { level, aim: { i: 0, until: 20 }, points: [point(45, 21, { owner: 0 }), point(49, 21)] });
  record(`private-failure-memory-${level}`, { level, pointFailures: [3, 0, 9] });
  const cover = [];
  for (const p of defaults) for (let dy = -3; dy <= 3; dy++) for (let dx = -3; dx <= 3; dx++) {
    const x = Math.floor(p.x / 2) + dx, y = Math.floor(p.z / 2) + dy;
    if ((x + y) % 3 === 0) cover.push([y * 48 + x, 'O']);
  }
  cover.push([10 * 48 + 27, 'W']);
  record(`cover-cells-${level}`, { level, terrain: cover });
}
record('busy-and-source-eligible-flags', { units: [
  unit(1, 'rifle', { path: [{ x: 55, z: 21 }] }),
  unit(2, 'rifle', { attackId: 12 }), unit(3, 'rifle', { targetId: 12 }),
  unit(4, 'rifle', { nade: { x: 3, z: 3 } }), unit(5, 'rifle', { dig: { x: 3, z: 3 } }),
  unit(6, 'rifle', { enter: 0 }), unit(7, 'rifle', { fireAt: 0 }),
  unit(8, 'rifle', { retreating: true, flags: 1 }), unit(9, 'rifle', { build: 1, flags: 128 }),
  unit(10, 'rifle', { flags: sim.RIDING_FLAG }), unit(11, 'engineer'),
  unit(12, 'rifle', { holdPos: true, flags: 4096 }),
  unit(13, 'rifle', { entrench: {}, flags: 1024 }), unit(14, 'rifle', { orders: [{ kind: 1, x: 55, z: 21 }] }),
] });
record('neutral-infantry-already-captures', { units: [unit(1, 'rifle', { x: 55, z: 21 }), unit(2, 'tank', { x: 55, z: 21 }), unit(3)] });
record('own-cut-infantry-already-holds', { points: [point(55, 21, { owner: 0, cut: true }), openingSentinel()], units: [unit(1, 'rifle', { x: 55, z: 21 }), unit(2, 'tank', { x: 55, z: 21 }), unit(3)] });
record('out-seat', { out: true });
for (const [name,fields] of [
 ['already-covered',{cover:1}], ['trench-covered',{cover:2}], ['entrenching',{entrench:{}}],
 ['garrisoned',{garrison:100}], ['two-holders',{}],
]) record(`holder-${name}`, { points:[point(55,21,{owner:0}),openingSentinel()],
 units:[unit(1,'rifle',{x:55,z:21,...fields}),unit(2,'rifle',{x:55,z:21})] });
// Producer rows use actual initialization and observation without a synthetic
// decoded-frame overlay. Recorded raw intent edits are replayable by native Match.
const producer = [];
const visionSchedule = [];
for (const name of ['initial-source-observation', 'rounded-own-intents']) {
  const map = { name: 'Producer source', w: 64, h: 64, rows: Array(64).fill('.'.repeat(64)),
    spawns: [{ x: 4, y: 4 }, { x: 59, y: 59 }], points: [{ x: 27, y: 10 }, { x: 13, y: 27 }, { x: 32, y: 32, needs: 0 }] };
  // Authored mine outside friendly sight and clear of the initial base footprints.
  map.rows[60] = '.'.repeat(40) + 'N' + '.'.repeat(23);
  const players = [{ name: 'A', team: 0, faction: 0 }, { name: 'B', team: 1, faction: 1 }];
  const options = { mode: 'conquest', weather: false, tech: false, logistics: false, supply: false };
  const prior = Math.random;
  let game, startupDraws = 0;
  Math.random = () => { startupDraws++; return 0.5; };
  try { game = sim.createGame(map, players.map(p => p.name), false, players.map(p => p.team), players.map(p => p.faction), options); }
  finally { Math.random = prior; }
  const overrides = [], visible = [];
  if (name === 'rounded-own-intents') {
    const own = [...game.units.values()].filter(u => u.owner === 0 && !sim.UNITS[u.type].structure);
    if (own.length < 3) throw Error('Source opening roster changed');
    overrides.push({ id: own[0].id, x: 20.25, z: 21.15, hp: own[0].hp - 0.25, supp: 2.5,
      path: [{ x: 54.25, z: 21.15 }], worldGoal: { x: 55.25, z: 21.15 } });
    overrides.push({ id: own[1].id, targetId: 999999, fireAt: 27 * 64 + 27,
      nade: { x: 37.25, z: 32.15 }, enter: 22 * 64 + 22 });
    overrides.push({ id: own[2].id, nade: { x: 30.25, z: 31.15 }, enter: 24 * 64 + 24 });
    for (const override of overrides) Object.assign(game.units.get(override.id), structuredClone(override));
    // A target removed after the last vision update can remain in this source set.
    visible.push(999999); game.players[0].visible = new Set(visible);
  }
  const view = ai.observe(game, 0);
  const units = [...view.units.values()].sort((a, b) => a.id - b.id).map(u => ({ ...u, air: u.air ?? null, orderPlan: u.orderPlan ?? null }));
  producer.push({ name, map, players, options, shuffle: false, tick: game.tick, startupDraws, overrides, visible,
    expected: { frame: { w: view.w, h: view.h, tick: view.tick, chars: runs(view.chars), flags: runs(view.flags),
      height: view.height === null ? null : runs(view.height),
      players: view.players.map(p => ({ slot: p.slot, team: p.team, faction: p.faction, spawn: p.spawn, out: p.out })),
      points: view.points, units } } });
  if (name === 'initial-source-observation') {
    visionSchedule.push([game.tick, game.visionTick ?? null]);
    Math.random = () => 0.5;
    try {
      for (let i = 0; i < 8; i++) {
        sim.step(game);
        visionSchedule.push([game.tick, game.visionTick ?? null]);
      }
    } finally { Math.random = prior; }
  }
}
// Capture the source planner's dispatch guard separately from policy execution.
const schedule = [];
for (const level of ['easy', 'normal', 'hard']) for (let slot = 0; slot < 6; slot++) {
  const acts = [], views = [];
  const brain = planner({ ...ai, think: g => acts.push(g.tick) });
  for (let tick = 0; tick <= 240; tick++) {
    const g = { tick };
    if (brain.wantsView(g, slot, level)) views.push(tick);
    brain.act(g, slot, { view: null, level });
  }
  schedule.push({ slot, level, every: ai.thinkEvery(level), acts, views });
}
clean();
const data = JSON.stringify({ format: 1, reference: pin, node: process.version, arch: process.arch, schedule, visionSchedule, producer, cases }) + '\n';
const output = `${root}/rust/crates/core/tests/fixtures/commander-ai.json`;
if (process.argv.includes('--check')) {
  if (readFileSync(output, 'utf8') !== data) throw Error('Commander fixture differs');
} else writeFileSync(output, data);
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} ${cases.length} quiet commander cases, ${cases.reduce((n, c) => n + c.expected.draws, 0)} waypoint draws`);
