// Compare ordinary mode aliases through original commands and full source steps.
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

if (process.version !== 'v24.21.0' || process.arch !== 'x64') throw Error('Use x64 Node 24.21.0');
if (!process.argv.slice(2).every(arg => arg === '--check')) throw Error('Only --check is supported');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => {
  if (git(['rev-parse', 'HEAD']) !== pin || git(['status', '--porcelain', '--untracked-files=no'])) throw Error('Use clean pinned reference');
};
clean();
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const buffer = new DataView(new ArrayBuffer(8));
const bits = value => { buffer.setFloat64(0, value); return buffer.getBigUint64(0).toString(16).padStart(16, '0'); };
const point = row => row ? { x: bits(row.x), z: bits(row.z) } : null;
const encode = value => JSON.stringify(value, (_key, row) => {
  if (typeof row === 'number') return bits(row);
  if (row instanceof Map) return [...row];
  if (row instanceof Set || ArrayBuffer.isView(row)) return [...row];
  return row;
});
const fullHash = game => createHash('sha256').update(encode(game)).digest('hex');
const projection = game => ({
  tick: game.tick, mode: game.mode ?? null, tech: game.tech, logisticsEnabled: game.logisticsEnabled,
  supply: game.supply, winVp: bits(game.winVp), winner: game.winner, endReason: game.endReason ?? null,
  players: game.players.map(p => ({ slot: p.slot, mp: bits(p.mp), vp: bits(p.vp), inc: bits(p.inc),
    tier: p.tier ?? null, armory: p.armory ?? null, lab: p.lab ?? null })),
  points: game.points.map(p => ({ ...point(p), owner: p.owner, capper: p.capper, progress: bits(p.progress),
    cut: p.cut, contested: !!p.contested, onPoint: p.onPoint ?? [] })),
  units: [...game.units.values()].map(u => ({ id: u.id, type: u.type, owner: u.owner, ...point(u),
    hp: bits(u.hp), path: u.path.map(point), worldGoal: point(u.worldGoal), routeEnd: point(u.routeEnd),
    targetId: u.targetId, attackId: u.attackId, holdPos: !!u.holdPos, garrison: u.garrison,
    reinf: bits(u.reinf), cd: bits(u.cd), moveOutcome: u.moveOutcome ?? null })),
});
const map = { name: 'Ordinary mode aliases', w: 48, h: 48, rows: Array(48).fill('.'.repeat(48)),
  spawns: [{ x: 4, y: 4 }, { x: 43, y: 43 }], points: [{ x: 27, y: 10 }] };
const players = [{ name: 'A', team: 0, faction: 0 }, { name: 'B', team: 1, faction: 1 }];
const cases = [];
for (const [name, mode] of [['omitted', undefined], ['explicit-conquest', 'conquest'], ['literal-empty-mode', '']]) {
  const options = { ...(mode === undefined ? {} : { mode }), tech: true, logistics: true, weather: false };
  const prior = Math.random;
  let game, startupDraws = 0, stepDraws = 0;
  Math.random = () => { startupDraws++; return 0.5; };
  try { game = sim.createGame(map, players.map(p => p.name), false, players.map(p => p.team), players.map(p => p.faction), options); }
  finally { Math.random = prior; }
  // Start just before a real infantry capture and a VP victory, without an enemy nearby.
  const u = game.units.get(1), p = game.points[0];
  Object.assign(u, { x: p.x, z: p.z });
  Object.assign(p, { capper: 0, progress: 1 - sim.TICK / sim.CFG.captureTime / 2 });
  game.players[0].vp = game.winVp - sim.TICK / 2;
  const initial = { unit: { id: 1, ...point(u) }, point: { index: 0, capper: p.capper, progress: bits(p.progress) },
    player: { slot: 0, vp: bits(game.players[0].vp) } };
  const command = { t: 'stance', ids: [1], key: 'holdPos', on: true };
  const denial = sim.command(game, 0, command) ?? null;
  if (denial) throw Error(`Ordinary stance refused: ${name}: ${denial}`);
  const steps = [];
  Math.random = () => { stepDraws++; return 0.5; };
  try {
    for (let i = 0; i < 2; i++) {
      sim.step(game);
      steps.push({ expected: projection(game), fullStateHash: fullHash(game) });
    }
  } finally { Math.random = prior; }
  if (game.points[0].owner !== 0 || game.winner !== 0 || game.endReason !== 'vp') throw Error('Capture and VP victory must execute');
  cases.push({ name, options, startup: { draws: startupDraws, value: 0.5 }, initial,
    command: { command, denial }, stepRandom: { draws: stepDraws, value: 0.5 }, steps });
}
if (JSON.stringify(cases[0].steps) !== JSON.stringify(cases[1].steps)) throw Error('Omitted and explicit Conquest full states differ');
for (const row of cases) {
  const expectedEnabled = row.name !== 'literal-empty-mode';
  if (row.steps.some(s => s.expected.mode !== null || s.expected.tech !== expectedEnabled || s.expected.logisticsEnabled !== expectedEnabled)) throw Error('Ordinary mode feature boundary changed');
}
clean();
const output = JSON.stringify({ reference: pin, node: process.version, arch: process.arch, map, players,
  scope: 'Omitted mode and explicit Conquest retain absent runtime mode. Empty-string mode is a distinct optional-feature boundary.',
  fullStateHash: 'SHA256 of serializable enumerable game data, including Map, Set and typed-array contents, with numeric values encoded as f64 bits.',
  cases }) + '\n';
const path = `${root}/rust/crates/core/tests/fixtures/ordinary-mode-alias.json`;
if (process.argv.includes('--check')) {
  if (readFileSync(path, 'utf8') !== output) throw Error('Ordinary mode alias fixture differs');
} else writeFileSync(path, output);
console.log(`Verified ${cases.length} mode cases, ${cases.reduce((n, c) => n + c.steps.length, 0)} full source steps`);
