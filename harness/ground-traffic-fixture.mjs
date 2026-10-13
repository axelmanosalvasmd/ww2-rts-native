// Original traffic steering and movement phases, with physical effects excluded.
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const reference = resolve(process.env.WW2_REFERENCE ?? fileURLToPath(new URL('../reference/ww2-rts/', import.meta.url)));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => { if (git(['rev-parse', 'HEAD']) !== pin || git(['status', '--porcelain', '--untracked-files=no'])) throw Error('Use clean pinned reference'); };
if (process.arch !== 'x64' || process.version !== 'v24.21.0') throw Error('Use x64 Node v24.21.0');
clean();
registerHooks({ load(url, context, next) {
  const result = next(url, context);
  if (url !== pathToFileURL(resolve(reference, 'shared/sim.js')).href) return result;
  const source = String(result.source);
  const anchors = ['    // Global waypoints, voluntary traffic steering and swept contact are separate steps.', '    if (def.crushes && moved > 0)', '    u.still = moving ? 0 : u.still + dt;', '    if (u.logistics?.forced && u.path.length && !u.supplyHold)'];
  for (const a of anchors) if (source.split(a).length !== 2) throw Error('Movement phase anchors changed');
  const movement = source.slice(source.indexOf(anchors[0]), source.indexOf(anchors[1]));
  const progress = source.slice(source.indexOf(anchors[2]), source.indexOf(anchors[3]));
  return { ...result, source: source + '\nexport { placeUnit as __placeUnit, knownTerrainFor as __knownTerrainFor, beginPathTick as __beginPathTick };\nexport function __groundTraffic(g,id){const dt=TICK,infantryProgress=new Map();for(const u of [g.units.get(id)]){const def=UNITS[u.type],sm=suppMul(u);\n' + movement + progress + '\nreturn {traffic,speed,moved,progressSpeed,turningInPlace,infantryProgress:infantryProgress.get(u)??null};}}\n' };
} });
const sim = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const buffer = new DataView(new ArrayBuffer(8));
const bits = n => { buffer.setFloat64(0, n); return buffer.getBigUint64(0).toString(16).padStart(16, '0'); };
const pt = p => p ? [bits(p.x), bits(p.z)] : null;
const pose = p => p ? [...pt(p), bits(p.rot)] : null;
const seed = 0x71aff1;
function randomStream(initial) { let value = initial >>> 0; return () => { value = (value + 0x6D2B79F5) >>> 0; let t = value; t = Math.imul(t ^ t >>> 15, t | 1); t ^= t + Math.imul(t ^ t >>> 7, t | 61); return ((t ^ t >>> 14) >>> 0) / 4294967296; }; }
function unitRow(u) {
  return { id: u.id, kind: u.type, owner: u.owner, x: bits(u.x), z: bits(u.z), rot: bits(u.rot), hp: bits(u.hp), path: u.path.map(pt), reverse: pt(u.reverse), pace: bits(u.pace ?? NaN), attack: !!u.attackId, retreating: !!u.retreating, sprint: bits(u.sprint), supp: bits(u.supp), hitFrom: pt(u.hitFrom), stuck: bits(u.stuck), repath: bits(u.repath), worldGoal: pt(u.worldGoal), amove: pt(u.amove), moveSpeed: bits(u.moveSpeed ?? 0), travelDir: u.travelDir === undefined ? null : bits(u.travelDir), vx: u.vx === undefined ? null : bits(u.vx), vz: u.vz === undefined ? null : bits(u.vz), drift: !!u.drift, motionBefore: pose(u.motionBefore), motionClearance: u.motionClearance ? { goal: pose(u.motionClearance.goal), reverse: !!u.motionClearance.reverse, matchesPath: u.motionClearance.path === u.path, until: bits(u.motionClearance.until) } : null, traffic: u.traffic ? { goal: pt(u.traffic.goal), legs: (u.traffic.legs ?? []).map(pt), passing: !!u.traffic.passing, by: u.traffic.by, until: bits(u.traffic.until) } : null, trafficWait: bits(u.trafficWait ?? 0), trafficTrail: (u.trafficTrail ?? []).map(pose), trafficEntered: !!u.trafficEntered, queuedBehind: u.queuedBehind ?? 0, laneScan: u.laneScan ? { matchesPath: u.laneScan.path === u.path, until: bits(u.laneScan.until), lane: u.laneScan.lane ? { entry: pt(u.laneScan.lane.entry), exit: pt(u.laneScan.lane.exit) } : null } : null, ghost: bits(u.ghost ?? 0), still: bits(u.still), garrison: u.garrison, riding: !!u.riding, holdPos: !!u.holdPos, build: !!u.build, dig: !!u.dig, entrench: !!u.entrench, board: u.board, rail: !!u.rail };
}
const terrains = [], terrainIndices = new Map();
function terrainRow(view) {
  return { w: view.w, h: view.h, chars: view.chars.join(''), ground: view.ground?.join('') ?? null, objects: view.objects?.join('') ?? null, flags: [...view.flags], height: view.height ? [...view.height] : null, wear: view.wear ? [...view.wear].map(bits) : null, cellHp: [...view.cellHp].map(bits), trenchFront: view.trenchFront ? [...view.trenchFront].map(bits) : null, tough: view.tough ? [...view.tough].map(bits) : null, house: view.house ? [...view.house] : null, wrecks: (view.wrecks ?? []).map(w => ({ x: bits(w.x), z: bits(w.z), moving: !!w.motion, cell: w.c ?? null })), version: view.navigationKnowledgeVersion ?? 0 };
}
function terrainId(view) { const row = terrainRow(view), key = JSON.stringify(row); if (!terrainIndices.has(key)) { terrainIndices.set(key, terrains.length); terrains.push(row); } return terrainIndices.get(key); }
const map = (name, rows, heights) => ({ name, w: 32, h: 24, rows, spawns: [{ x: 0, y: 0 }, { x: 31, y: 23 }, { x: 31, y: 0 }], points: [{ x: 16, y: 12 }], ...(heights ? { heights } : {}) });
const blank = () => Array(24).fill('.'.repeat(32));
const flat = map('flat', blank());
const lanes = blank().map((r, y) => (y === 9 || y === 13) ? r.slice(0, 12) + 'BBBBBBBBBBBB' + r.slice(24) : r);
const narrow = map('narrow', lanes);
const covers = blank().map((r, y) => y === 11 ? r.slice(0, 7) + 'TTTTTTTTTTTT' + r.slice(19) : r);
const cover = map('cover', covers);
const heights = blank().map(() => '0'.repeat(16) + '2'.repeat(16));
const cliff = map('cliff', blank(), heights);
const cases = [];
function scenario(name, authored, initial, actions) {
  if (sim.validateMap(authored) !== null) throw Error(`Invalid map: ${name}`);
  const saved = Math.random; Math.random = randomStream(seed + cases.length); let g, players;
  try { g = sim.createGame(authored, [], false, [], [], { weather: false }); players = sim.createGame(authored, ['A', 'Ally', 'Enemy'], false, [0, 0, 1], [0, 1, 2], { weather: false }).players; } finally { Math.random = saved; }
  g.players = players; g.units = new Map(); g.nextId = 1; g.tick = 1; g.pathBudget = 4096; g.winner = null; g.supply = false; g.logistics = false;
  for (const row of initial) { const u = sim.internals.spawnUnit(g, row.owner ?? 0, row.kind); Object.assign(u, { x: row.x, z: row.z, ghost: 0, ...row.fields }); sim.__placeUnit(g, u, { x: u.x, z: u.z }); }
  for (const p of players) p.visible = new Set([...g.units.values()].filter(u => u.owner !== p.slot).map(u => u.id));
  const sourceTerrain = terrainId(g), starting = [...g.units.values()].map(unitRow), steps = [];
  for (const action of actions) {
    g.tick = action.tick ?? g.tick + 1;
    for (const [id, fields] of action.set ?? []) { const u = g.units.get(id); Object.assign(u, structuredClone(fields)); sim.__placeUnit(g, u, { x: u.x, z: u.z }); }
    for (const [c, ch] of action.cells ?? []) sim.internals.setCell(g, c, ch);
    for (const [owner, ids] of action.visible ?? []) players[owner].visible = new Set(ids);
    for (const id of action.arriveTraffic ?? []) { const u = g.units.get(id); if (!u.traffic) throw Error('No traffic goal to reach'); sim.__placeUnit(g, u, u.traffic.goal); }
    if (action.begin) sim.__beginPathTick(g);
    const knowledge = players.map(p => terrainId(sim.__knownTerrainFor(g, p.slot)));
    const memories = players.map(p => [...(p.terrainMemory?.values() ?? [])].map(row => structuredClone(row)));
    const before = [...g.units.values()].map(unitRow), results = [];
    for (const id of action.ids ?? [1]) {
      const u = g.units.get(id), oldPath = u.path, result = sim.__groundTraffic(g, id);
      results.push({ id, goal: pt(result.traffic.goal), blocked: !!result.traffic.blocked, yielding: !!result.traffic.yielding, waiting: !!result.traffic.waiting, replan: !!result.traffic.replan, speed: bits(result.speed), moved: bits(result.moved), progressSpeed: bits(result.progressSpeed), turning: result.turningInPlace, infantryProgress: result.infantryProgress === null ? null : bits(result.infantryProgress), replaced: oldPath !== u.path });
    }
    steps.push({ action, tick: g.tick, visionTick: g.visionTick ?? null, knowledge, memories, visible: players.map(p => [...p.visible]), before, results, expected: [...g.units.values()].map(unitRow) });
  }
  cases.push({ name, map: authored, seed: seed + cases.length, players: players.map(p => ({ team: p.team, faction: p.faction, spawn: pt(p.spawn) })), initial, sourceTerrain, starting, steps });
}
const route = (x, z) => [{ x, z }];
scenario('same-direction-queue', flat, [{ kind: 'tank', x: 17, z: 23, fields: { path: route(51, 23) } }, { kind: 'tank', x: 23, z: 23, fields: { path: route(51, 23), vx: 0, vz: 0 } }], [{}, {}, { set: [[2, { x: 31, z: 23 }]] }, {}]);
scenario('one-lane-oncoming-cache-and-replacement', narrow, [{ kind: 'tank', x: 19, z: 23, fields: { path: route(51, 23) } }, { kind: 'tank', owner: 1, x: 47, z: 23, fields: { path: route(15, 23), rot: Math.PI } }], [{ ids: [1, 2] }, { ids: [1, 2] }, { tick: 4, ids: [1, 2] }, { tick: 15, ids: [1, 2] }, { set: [[1, { path: route(19, 35) }]], ids: [1] }]);
scenario('stationary-pass-and-resume', flat, [{ kind: 'tank', x: 17, z: 23, fields: { path: [{ x: 21, z: 23 }, { x: 33, z: 23 }, { x: 51, z: 23 }], trafficWait: 0.4 } }, { kind: 'lorry', x: 23, z: 23 }], [{}, ...Array.from({ length: 4 }, () => ({ arriveTraffic: [1] })), {}]);
scenario('visible-allied-and-unseen-contact', flat, [{ kind: 'tank', x: 17, z: 23, fields: { path: route(51, 23), trafficWait: 0.4, moveSpeed: 6.5 } }, { kind: 'tank', owner: 2, x: 21.1, z: 23 }], [{ visible: [[0, []]] }, { visible: [[0, [2]]] }, { set: [[2, { owner: 1 }]], visible: [[0, []]] }, { set: [[1, { trafficWait: 3.1, stuck: 3.1 }]] }]);
scenario('covered-infantry-preserves-shelter', cover, [{ kind: 'rifle', x: 19, z: 23, fields: { path: route(43, 23), trafficWait: 0.4, hitFrom: { x: 43, z: 23 } } }, { kind: 'tank', x: 22, z: 23 }], [{}, {}, { set: [[1, { trafficWait: 3.1 }]] }]);
scenario('visible-cover-edit-and-memory', cover, [{ kind: 'rifle', x: 19, z: 23, fields: { path: route(43, 23), trafficWait: 0.4, hitFrom: { x: 43, z: 23 } } }, { kind: 'tank', x: 22, z: 23 }], [{}, { cells: [[9 + 11 * 32, '+']] }]);
scenario('trail-limit-and-cache-route', flat, [{ kind: 'tank', x: 5, z: 23, fields: { path: route(55, 23) } }], [...Array.from({ length: 23 }, (_, i) => ({ set: [[1, { x: 5 + i * 1.7, z: 23 }]] })), { set: [[1, { path: route(55, 35), stuck: 1, repath: 0 }]], begin: true }, {}]);
scenario('infantry-corner-skip', flat, [{ kind: 'rifle', x: 19.5, z: 23, fields: { path: [{ x: 20, z: 23 }, { x: 23, z: 23 }, { x: 31, z: 23 }] } }], [{}, {}]);
scenario('hidden-edit-known-skip-physical-stop', flat, [{ kind: 'rifle', x: 3, z: 3, fields: { path: route(55, 23) } }], [{ ids: [] }, { cells: [[24 + 11 * 32, 'B']], ids: [] }, { tick: 3, set: [[1, { x: 47.7, z: 23, path: [{ x: 48.1, z: 23 }, { x: 51, z: 23 }] }]] }, {}]);
scenario('hidden-edit-vehicle-physical-hull', flat, [{ kind: 'tank', x: 3, z: 3, fields: { path: route(55, 23) } }], [{ ids: [] }, { cells: [[27 + 11 * 32, 'B']], ids: [] }, { tick: 3, set: [[1, { x: 51.7, z: 23, path: route(61, 23), moveSpeed: 6.5 }]] }]);
const turnMap = map('turn-clearance', blank().map((r, y) => (y === 10 || y === 13) ? r.slice(0, 12) + 'BBBBBBBBBBBB' + r.slice(24) : r));
scenario('motion-clearance-and-route-invalidation', turnMap, [{ kind: 'tank', x: 49.8, z: 23.25, fields: { path: route(55, 35), stuck: 0.5, repath: 1 } }], [{}, {}, { set: [[1, { path: route(55, 23.25) }]] }]);
scenario('cliff-and-off-map', cliff, [{ kind: 'rifle', x: 31.7, z: 23, fields: { path: route(35, 23) } }, { kind: 'tank', x: 2.1, z: 23, fields: { path: route(-3, 23), rot: Math.PI, moveSpeed: 6.5 } }], [{ ids: [1, 2] }, { ids: [1, 2] }]);
// Guard the named branches so a recorder change cannot silently weaken coverage.
const branch = name => cases.find(c => c.name === name);
const requireBranch = (test, name) => { if (!test) throw Error(`Traffic branch not reached: ${name}`); };
requireBranch(branch('same-direction-queue').steps[0].results[0].waiting, 'queue');
requireBranch(branch('one-lane-oncoming-cache-and-replacement').steps[0].results[0].waiting, 'oncoming lane');
const passing = branch('stationary-pass-and-resume').steps;
requireBranch(passing[0].expected[0].traffic?.passing && !passing[4].expected[0].traffic && passing[4].expected[0].path.length === 1, 'pass and resume');
const contacts = branch('visible-allied-and-unseen-contact').steps;
requireBranch(!contacts[0].results[0].yielding && contacts[0].results[0].moved === bits(0) && contacts[1].results[0].yielding, 'unseen physical contact');
requireBranch(branch('covered-infantry-preserves-shelter').steps[0].results[0].blocked, 'shelter constraint');
requireBranch(branch('visible-cover-edit-and-memory').steps[1].memories[0].some(row => row[0] === 9 + 11 * 32 && row[1] === '+'), 'remembered cover edit');
const trail = branch('trail-limit-and-cache-route').steps;
requireBranch(trail[22].expected[0].trafficTrail.length === 20 && trail[23].results[0].replaced && !trail[23].expected[0].laneScan.matchesPath && trail[24].expected[0].laneScan.matchesPath, 'trail and route cache');
requireBranch(branch('infantry-corner-skip').steps[0].expected[0].path.length === 2, 'waypoint skip');
for (const [name, c] of [['hidden-edit-known-skip-physical-stop', 24 + 11 * 32], ['hidden-edit-vehicle-physical-hull', 27 + 11 * 32]]) {
  const steps = branch(name).steps;
  requireBranch(steps.every(s => terrains[s.knowledge[0]].chars[c] === '.' && !s.memories[0].length), name + ' stays hidden');
  requireBranch(steps.at(-1).results[0].moved === bits(0), name + ' physical stop');
}
const clearance = branch('motion-clearance-and-route-invalidation').steps;
requireBranch(clearance[0].expected[0].motionClearance?.matchesPath && !clearance.at(-1).expected[0].motionClearance, 'motion clearance cache');
requireBranch(branch('cliff-and-off-map').steps[0].results[1].moved === bits(0), 'off-map hull');
clean();
const output = JSON.stringify({ reference: pin, node: process.version, arch: process.arch, seed, excluded: ['crushing', 'ground wear and dust', 'mines and damage', 'combat', 'crowd separation', 'infantry post-separation progress'], terrains, cases }) + '\n';
const path = fileURLToPath(new URL('../rust/crates/core/tests/fixtures/ground-traffic.json', import.meta.url));
if (process.argv.includes('--check')) { if (readFileSync(path, 'utf8') !== output) throw Error('Ground traffic fixture differs'); } else writeFileSync(path, output);
console.log(`Verified ${cases.length} traffic traces, ${cases.reduce((n, c) => n + c.steps.length, 0)} steps, ${terrains.length} terrain snapshots`);
