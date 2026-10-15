// One pinned-Source component batch for MODE-12/13/18/78/87. No live-mode admission.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', args, { cwd: reference, encoding: 'utf8' }).trim();
function clean() {
  assert.equal(process.version, 'v24.21.0');
  assert.equal(process.arch, 'x64');
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
}
clean();
assert(process.argv.slice(2).every(v => v === '--check'));
const original = readFileSync(`${reference}/shared/sim.js`, 'utf8');
const start = '  for (const p of g.points) {\n    const on = list.filter(u => u.hp > 0';
const end = '  // victory points count per team:';
assert.equal(original.split(start).length, 2);
const capture = original.slice(original.indexOf(start), original.indexOf(end, original.indexOf(start)));
assert(capture.endsWith('\n\n'));
registerHooks({ load(url, context, next) {
  const loaded = next(url, context);
  if (url !== pathToFileURL(`${reference}/shared/sim.js`).href) return loaded;
  // Append exports and an unchanged source statement block. No function body is replaced.
  return { ...loaded, source: `${String(loaded.source)}\nexport { updateVision as __modeVision, rebuildGrid as __modeGrid, stepLab as __modeLab };\nexport function __modeCapture(g, list, dt) {\n${capture}}\n` };
} });
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const scenarios = await import(pathToFileURL(`${reference}/shared/scenarios.js`));
const server = readFileSync(`${reference}/server.js`, 'utf8');
const seats = server.split('\n').find(line => line.startsWith('const seats = (room) => '));
assert(seats?.endsWith(';'));
// Evaluate the original pure seats expression, without importing or starting the server.
const capacity = new Function('MAX_PLAYERS', 'spawnsFor', `return (${seats.slice('const seats = '.length, -1)});`)(sim.MAX_PLAYERS, sim.spawnsFor);
const hash = s => createHash('sha256').update(s).digest('hex');
const bits = n => { const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex'); };
const encode = v => typeof v === 'number' ? bits(v) : Array.isArray(v) ? v.map(encode) : v && typeof v === 'object' ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, encode(x)])) : v;
const map = { name: 'Mode predicate components', w: 64, h: 64, rows: Array(64).fill('.'.repeat(64)), spawns: [{ x: 8, y: 8 }, { x: 20, y: 8 }, { x: 53, y: 53 }, { x: 53, y: 8 }], points: [{ x: 32, y: 32 }] };
const roster = [2, 5, 2, 5];
function make(authored = map, teams = roster, mode = 'conquest', defender = 5, factions = teams.map((_, i) => i % 4)) {
  assert.equal(sim.validateMap(authored), null);
  const old = Math.random;
  let g;
  // Explicit constructor input, not a claim of shared native/Source random streams.
  Math.random = () => 0.5;
  try {
    g = sim.createGame(authored, teams.map((_, i) => `P${i}`), false, teams, factions, { mode, defenderTeam: defender, army: 'standard', weather: 'clear', weatherSeed: 31, tech: true, supply: false, logistics: false, sandbox: false });
  } finally { Math.random = old; }
  for (const u of g.units.values()) { u.holdPos = true; u.holdFire = true; }
  return g;
}
const anchors = g => [...g.units.values()].map(u => [u.id, u.owner, u.type, bits(u.x), bits(u.z)]);
function recipients(g) {
  return g.players.map((_, slot) => {
    const s = sim.snapshotFor(g, slot, []);
    return { slot, tick: s.tick, winner: s.winner ?? null, end: s.end ?? null, units: s.units.map(u => u.slice(0, 3)).sort((a, b) => a[0] - b[0]) };
  });
}
function finishProjection(g) {
  return encode({ tick: g.tick, winner: g.winner, reason: g.endReason ?? null, at: g.endAt ?? null, endTick: g.endTick ?? null, reveal: !!g.reveal, timeline: g.timeline ?? [], recipients: recipients(g), allUnits: [...g.units.values()].map(u => [u.id, u.type, u.owner]).sort((a, b) => a[0] - b[0]) });
}

const alliance = [];
for (const teams of [[0, 1], roster]) for (const mode of ['conquest', 'world']) {
  const g = { players: teams.map((team, slot) => ({ team, slot })), ...(mode === 'world' ? { mode: { kind: 'world' } } : {}) };
  const slots = [-2, -1, ...teams.map((_, i) => i), teams.length, 99];
  alliance.push({ teams, mode, slots, expected: slots.map(a => slots.map(b => sim.allied(g, a, b))) });
}
const consumerMap = { ...structuredClone(map), name: 'Canonical alliance vision', spawns: [{ x: 16, y: 16 }, { x: 28, y: 16 }, { x: 16, y: 28 }, { x: 28, y: 28 }] };
const consumers = make(consumerMap);
const consumerAnchors = anchors(consumers);
sim.__modeGrid(consumers);
sim.__modeVision(consumers);
const own = [...consumers.units.values()].find(u => u.owner === 0 && u.type === 'rifle');
const ally = [...consumers.units.values()].find(u => u.owner === 2 && u.type === 'rifle');
assert(own && ally);
const shared = [...consumers.players[0].visible].filter(id => !sim.allied(consumers, consumers.units.get(id).owner, 0));
assert(shared.length > 0, 'A genuine enemy is seen at the first allied spawn');
assert(shared.every(id => consumers.players[2].visible.has(id)), 'Teammate shares enemy vision');
assert(!consumers.players[0].visible.has(ally.id));
assert(sim.snapshotFor(consumers, 0, []).units.some(u => u[0] === ally.id));
sim.command(consumers, 0, { t: 'attack', ids: [own.id], target: ally.id });
assert.equal(own.attackId, 0);
assert.deepEqual(anchors(consumers), consumerAnchors);
const allianceConsumers = { map: consumerMap, allocatedSpawns: consumers.players.map(p => ({ slot: p.slot, team: p.team, spawn: p.spawn })), canonicalAnchors: consumerAnchors, shooter: own.id, ally: ally.id, expected: encode({ visible: consumers.players.map(p => [...p.visible].sort((a, b) => a - b)), recipients: recipients(consumers), attackId: own.attackId }) };
// A declared finished producer uses original canonical placement, not the test.js
// helper's temporary factory. Purchase and research still use real command admission.
const factionInput = { factions: [0, 1, 1, 2], slot: 2, cell: 42 * map.w + 40, mp: 5000, researchSeconds: [60, 90] };
const purchaseGame = make(map, roster, 'conquest', 5, factionInput.factions);
const purchaseAnchors = anchors(purchaseGame), constructorIds = new Set(purchaseAnchors.map(u => u[0]));
assert.equal(purchaseGame.players[2].faction, 1, 'factions per player, not slot');
const factoryCells = Array.from({ length: 9 }, (_, i) => factionInput.cell + Math.floor(i / 3) * map.w + i % 3);
assert(factoryCells.every(c => purchaseGame.chars[c] === '.'));
assert([...purchaseGame.units.values()].every(u => Math.hypot(u.x - 83, u.z - 87) > 6));
const factory = sim.internals.placeBuilding(purchaseGame, factionInput.slot, 'motorpool', factionInput.cell, true);
assert(factory.id > 0 && !constructorIds.has(factory.id));
assert.deepEqual(factory.cells, factoryCells);
const producerProjection = () => {
  const structure = purchaseGame.structures.get(factory.structureId);
  assert(structure && structure.entityId === factory.id);
  assert(factory.cells.every(c => purchaseGame.buildingCells.get(c)?.id === factory.id && purchaseGame.structuralCells.get(c)?.structureId === factory.structureId));
  return { id: factory.id, owner: factory.owner, type: factory.type, x: factory.x, z: factory.z, cells: factory.cells, built: factory.built, hp: factory.hp, queue: factory.queue, jobs: factory.productionJobs, progress: factory.prog, rally: factory.rally, structure: { id: structure.id, entityId: structure.entityId, cells: structure.sections.map(s => s.c), sections: structure.sections.map(s => s.id) } };
};
const producer = structuredClone(producerProjection());
const purchaser = purchaseGame.players[factionInput.slot];
purchaser.mp = factionInput.mp;
for (const [i, seconds] of factionInput.researchSeconds.entries()) {
  assert.equal(sim.command(purchaseGame, factionInput.slot, { t: 'tech', kind: 'tier' }), undefined);
  assert.equal(purchaser.lab.length, 1);
  sim.__modeLab(purchaseGame, purchaser, seconds);
  assert.equal(purchaser.tier, i + 2);
  assert.equal(purchaser.lab.length, 0);
}
const beforeIds = new Set(purchaseGame.units.keys()), beforeMp = purchaser.mp;
assert.equal(sim.command(purchaseGame, factionInput.slot, { t: 'buy', unit: 'tiger', from: factory.id }), undefined);
const purchased = [...purchaseGame.units.values()].filter(u => !beforeIds.has(u.id));
assert.equal(purchased.length, 1);
assert.equal(purchased[0].type, 'tiger', 'actual purchase uses player faction, not slot');
assert.equal(purchased[0].owner, factionInput.slot);
assert(purchaser.mp < beforeMp);
assert.deepEqual(producerProjection(), producer);
assert.deepEqual(anchors(purchaseGame).filter(u => constructorIds.has(u[0])), purchaseAnchors);
const factionPurchase = { ...factionInput, expected: encode({ factions: purchaseGame.players.map(p => p.faction), tier: purchaser.tier, beforeMp, afterMp: purchaser.mp, purchased: purchased.map(u => [u.id, u.type, u.owner]), countBefore: beforeIds.size, countAfter: purchaseGame.units.size, producer }) };

const captureCases = [];
for (const input of [
  { name: 'allies-first-slot', owners: [0, 2], owner: -1, capper: 0, progress: 1 - 1 / 160 },
  { name: 'allies-reversed-order', owners: [2, 0], owner: -1, capper: 2, progress: 1 - 1 / 160 },
  { name: 'ally-held-stays-full', owners: [0, 2], owner: 2, capper: 2, progress: 0.2 },
  { name: 'enemy-contests-allies', owners: [0, 2, 1], owner: -1, capper: 0, progress: 1 - 1 / 160 },
]) {
  const g = { tick: 3, winner: null, players: roster.map((team, slot) => ({ team, slot, out: false })), points: [{ x: 40, z: 40, owner: input.owner, capper: input.capper, progress: input.progress, needs: -1 }] };
  const units = input.owners.map((owner, i) => ({ id: i + 1, owner, type: 'rifle', x: 40, z: 40, hp: 100, retreating: false, riding: false }));
  sim.__modeCapture(g, units, sim.TICK);
  const p = g.points[0];
  captureCases.push({ ...input, expected: encode({ owner: p.owner, capper: p.capper, progress: p.progress, contested: p.contested, onPoint: p.onPoint }) });
}
const alliedVictory = make();
alliedVictory.players[0].vp = 1200.125;
alliedVictory.players[2].vp = 1200.125;
sim.step(alliedVictory);
assert.equal(alliedVictory.winner, 2);
assert(alliedVictory.players.every(p => p.vp < alliedVictory.winVp));
const teamVictory = { scores: [1200.125, 0, 1200.125, 0], expected: finishProjection(alliedVictory) };

const maps = ['bastogne', 'helms-deep', 'stalingrad-factory'].map(name => ({ name, map: JSON.parse(readFileSync(`${reference}/maps/${name}.json`, 'utf8')) }));
const modes = ['conquest', 'assault', 'annihilation', 'classic', 'horde', 'tutorial', 'world'];
const spawnCases = maps.flatMap(({ name, map }) => {
  assert.equal(map.spawns[0].assault, true);
  assert(map.defend.includes(0));
  return modes.map(mode => ({ name, mode, expected: { indices: sim.spawnsFor(map, mode), capacity: capacity({ mode, mapSpawns: map.spawns }) } }));
});
const spawnConstructors = maps.flatMap(({ name, map }) => ['conquest', 'assault'].map(mode => {
  const g = make(map, [0, 1], mode, 0);
  return { name, mode, expected: encode(g.players.map(p => ({ team: p.team, spawn: p.spawn }))) };
}));
const minimumCases = [
  { name: 'two-normal-one-assault', flags: [false, false, true] },
  { name: 'one-normal-two-assault', flags: [false, true, true] },
  { name: 'zero-normal-three-assault', flags: [true, true, true] },
].map(input => {
  const authored = structuredClone(map);
  authored.spawns = authored.spawns.slice(0, 3).map((p, i) => ({ ...p, assault: input.flags[i] }));
  return { ...input, map: authored, error: sim.validateMap(authored) };
});
const normalizationCases = [
  { teams: [2, 5, 3, 2], defender: 5 },
  { teams: [5, 2, 3, 2], defender: 5 },
  { teams: [2, 3, 2, 5], defender: 5 },
  { teams: [3, 2, 5, 2], defender: 5 },
  { teams: [5, 5, 5], defender: 5 },
  { teams: [2, 3, 2], defender: 5 },
  { teams: [5], defender: 5 },
  { teams: [2], defender: 5 },
  { teams: [0, 1, 2, 3, 4, 5], defender: 0 },
  { teams: [0, 1, 2, 3, 4, 5], defender: 5 },
].map(input => {
  const g = make(map, input.teams, 'assault', input.defender);
  return { ...input, expected: { teams: g.players.map(p => p.team), defender: g.mode.defenderTeam, attacker: g.mode.attackerTeam } };
});

const legacyBase = triggers => ({ ...structuredClone(map), triggers });
const legacyInputs = [
  { name: 'absent', map: structuredClone(map) },
  { name: 'empty', map: legacyBase([]) },
  { name: 'say', map: legacyBase([{ at: 0, say: 'Public' }]) },
  { name: 'destroy', map: legacyBase([{ at: 1, blow: [1, 2, 3, 4] }]) },
  { name: 'say-before-destroy', map: legacyBase([{ at: 0.025, say: 'Both', blow: [0, 0, 0, 0] }]) },
  { name: 'ordered-same-time', map: legacyBase([{ at: 0, say: 'First' }, { at: 0, say: 'Second' }]) },
  { name: 'twenty-boundaries', map: legacyBase(Array.from({ length: 20 }, (_, i) => ({ at: i === 19 ? 7200 : i, say: `Trigger ${i}` }))) },
  { name: 'utf16-140', map: legacyBase([{ at: 0, say: '😀'.repeat(70) }]) },
  { name: 'utf16-141', map: legacyBase([{ at: 0, say: '😀'.repeat(70) + 'x' }]) },
  { name: 'time-below-zero', map: legacyBase([{ at: -0.01, say: 'Bad' }]) },
  { name: 'time-above-7200', map: legacyBase([{ at: 7200.01, say: 'Bad' }]) },
  { name: 'twenty-one', map: legacyBase(Array.from({ length: 21 }, () => ({ at: 0, say: 'Bad' }))) },
  { name: 'empty-text', map: legacyBase([{ at: 0, say: '' }]) },
  { name: 'missing-action', map: legacyBase([{ at: 0 }]) },
  { name: 'reversed-box', map: legacyBase([{ at: 0, blow: [4, 1, 3, 2] }]) },
  { name: 'out-of-map-box', map: legacyBase([{ at: 0, blow: [0, 0, 64, 0] }]) },
];
for (const name of ['el-alamein', 'hurtgen-forest', 'prokhorovka', 'seelow-heights', 'the-great-bridge']) legacyInputs.push({ name: `authored:${name}`, map: JSON.parse(readFileSync(`${reference}/maps/${name}.json`, 'utf8')) });
const legacyCases = legacyInputs.map(input => ({ ...input, error: sim.validateMap(input.map), expected: sim.validateMap(input.map) === null ? scenarios.migrateScenario(input.map) : null }));

const legacyMap = legacyBase([{ at: 0.05, say: 'Mode batch public' }]);
const legacyGame = make(legacyMap);
const legacyAnchors = anchors(legacyGame);
const legacyFrames = [];
for (let tick = 1; tick <= 2; tick++) {
  sim.step(legacyGame);
  const rows = legacyGame.players.map((_, slot) => {
    const snapshot = sim.snapshotFor(legacyGame, slot, legacyGame.shots);
    return { slot, shots: snapshot.shots, scenario: snapshot.scenario };
  });
  const expectedSay = tick === 1 ? 1 : 0;
  assert(rows.every(row => row.shots.filter(s => s.k === 'say').length === expectedSay));
  assert.equal(legacyGame.scenario.triggers['legacy:0@match'].count, 1);
  assert.deepEqual(anchors(legacyGame), legacyAnchors);
  legacyFrames.push({ tick, scenario: structuredClone(legacyGame.scenario), recipients: structuredClone(rows) });
  legacyGame.shots.length = 0;
}
const legacyTrace = { map: legacyMap, frames: legacyFrames };

const finishCases = [];
for (const [i, reason] of ['vp', 'hq', 'bunkers', 'structures', 'timer', 'world', 'nations', 'tutorial', 'draw'].entries()) {
  const input = { name: `fresh-${reason}`, reason: reason === 'draw' ? 'timer' : reason, winner: reason === 'draw' ? -1 : i % 2 ? 5 : 2, at: i % 3 === 0 ? null : { x: i % 2 ? 17.25 : -17.25, z: i % 2 ? -0.05 : 0.15 } };
  const g = make(), before = anchors(g), unfinished = finishProjection(g);
  sim.finish(g, input.winner, input.reason, input.at ?? undefined);
  const expected = finishProjection(g);
  sim.finish(g, input.winner === 2 ? 5 : 2, 'hq', { x: 1, z: 1 });
  assert.deepEqual(finishProjection(g), expected);
  assert.deepEqual(anchors(g), before);
  finishCases.push({ ...input, unfinished, expected });
}
for (const finishTick of [1, 199, 200, 201]) {
  const g = make(), before = anchors(g);
  while (g.tick < finishTick - 1) sim.step(g);
  const unfinished = finishProjection(g);
  g.players[0].vp = 1200.125; g.players[2].vp = 1200.125;
  sim.step(g);
  assert.equal(g.winner, 2);
  assert.deepEqual(anchors(g), before);
  const expected = finishProjection(g);
  g.players[0].vp = 0; g.players[2].vp = 0; g.players[1].vp = g.winVp;
  sim.finish(g, 5, 'hq', { x: 1, z: 1 });
  assert.deepEqual(finishProjection(g), expected);
  finishCases.push({ name: `actual-vp-tick-${finishTick}`, finishTick, unfinished, expected });
}
clean();
const fixture = { format: 1, reference: pin, node: process.version, arch: process.arch, kind: 'isolated-components-not-whole-match', source: { captureBlockSha256: hash(capture), seatsExpressionSha256: hash(seats) }, map, roster, seed: 3936341, alliance, allianceConsumers, factionPurchase, captureCases, teamVictory, maps, spawnCases, spawnConstructors, minimumCases, normalizationCases, legacyCases, legacyTrace, finishCases };
const output = `${JSON.stringify(fixture)}\n`;
const path = `${root}/rust/crates/simtest/tests/fixtures/mode-predicates.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), output); else writeFileSync(path, output);
console.log(`MODE predicates: alliance=${alliance.length}, consumers=1, factionPurchases=1, captures=${captureCases.length}, alliedVictory=1, spawnFiltersAndCapacities=${spawnCases.length}, spawnConstructors=${spawnConstructors.length}, minimum=${minimumCases.length}, normalizations=${normalizationCases.length}, migrations=${legacyCases.length}, finishes=${finishCases.length}`);
