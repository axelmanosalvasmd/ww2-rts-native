// Record live shopping decisions from the unchanged thinker and delivered seat view.
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
const { viewFor } = await import(pathToFileURL(`${reference}/shared/ai-view.js`));
const copy = value => JSON.parse(JSON.stringify(value, (_key, row) => row instanceof Map ? [...row] : row instanceof Set ? [...row] : row));
const delivered = view => copy({ ...view, units: [...view.units.values()].sort((a, b) => a.id - b.id) });
const live = game => copy({
  players: game.players.map(p => ({ slot: p.slot, mp: p.mp, mun: p.mun, fuel: p.fuel, away: !!p.away, tier: p.tier, armory: p.armory, lab: p.lab })),
  units: [...game.units.values()].sort((a, b) => a.id - b.id).map(u => ({
    id: u.id, type: u.type, owner: u.owner, x: u.x, z: u.z, rot: u.rot, aim: u.aim, hp: u.hp,
    path: u.path, orders: u.orders, worldGoal: u.worldGoal, routeEnd: u.routeEnd,
    moveOutcome: u.moveOutcome, moveOutcomeTick: u.moveOutcomeTick, attackId: u.attackId,
    targetId: u.targetId, follow: u.follow, amove: u.amove, retreating: u.retreating,
    dig: u.dig, entrench: u.entrench, nade: u.nade, enter: u.enter, garrison: u.garrison,
    fireAt: u.fireAt, board: u.board, build: u.build, face: u.face, reverse: u.reverse,
    holdFire: u.holdFire, holdPos: u.holdPos, autoRetreat: u.autoRetreat, auto: u.auto,
  })),
});
const cases = [];
for (const spec of [
  { name: 'germany-mg-accepted', away: false },
  { name: 'germany-mg-seat-away-after-delivery', away: true },
]) {
  const map = { name: 'Quiet live shopping', w: 64, h: 64, rows: Array(64).fill('.'.repeat(64)),
    spawns: [{ x: 4, y: 4 }, { x: 59, y: 59 }], points: [{ x: 27, y: 10 }, { x: 13, y: 27 }, { x: 32, y: 32 }] };
  const players = [{ name: 'A', team: 0, faction: 1 }, { name: 'B', team: 1, faction: 1 }];
  const options = { mode: 'conquest', weather: false, tech: true, logistics: false, supply: false };
  const prior = Math.random;
  let game, startupDraws = 0;
  Math.random = () => { startupDraws++; return 0.5; };
  try { game = sim.createGame(map, players.map(p => p.name), false, players.map(p => p.team), players.map(p => p.faction), options); }
  finally { Math.random = prior; }
  const initial = { player: { tier: 3, armory: { inf: 3, guns: 3, armor: 3 }, mp: 150, mun: 0, fuel: 0 }, visible: [], removeUnits: [3] };
  Object.assign(game.players[0], initial.player, { visible: new Set(initial.visible) });
  for (const id of initial.removeUnits) {
    const unit = game.units.get(id);
    if (!unit || unit.owner !== 0 || unit.type !== 'mg') throw Error('Expected opening MG for shopping setup');
    game.units.delete(id);
  }
  const memory = {}, frames = [];
  let seed = 0x516e9200;
  for (const tick of [3600, 3640]) {
    // This bounded component advances delivery time, not the full match simulation.
    game.tick = tick;
    const view = viewFor(game, 0, memory);
    const frame = delivered(view), random = [], sourceRandom = [], attempts = [];
    if (view.players[0].visible.size) throw Error('Shopping fixture must have no enemy contact');
    const afterDelivery = spec.away && tick === 3600 ? { players: [[0, { away: true }]] } : null;
    for (const [slot, fields] of afterDelivery?.players ?? []) Object.assign(game.players[slot], fields);
    let phase = 'thinker';
    Math.random = () => {
      seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
      const value = seed / 4294967296;
      (phase === 'thinker' ? random : sourceRandom).push(value);
      return value;
    };
    try {
      ai.think(game, 0, { level: 'normal', view, memory, submit: command => {
        if (!['buy', 'move', 'amove'].includes(command.t)) throw Error(`Unbounded command: ${JSON.stringify(command)}`);
        phase = 'command';
        let result;
        try { result = sim.command(game, 0, command); }
        finally { phase = 'thinker'; }
        attempts.push({ command: copy(command), denial: result ?? null });
        return result;
      } });
    } finally { Math.random = prior; }
    if (sourceRandom.length) throw Error('Shopping admission unexpectedly consumed source randomness');
    frames.push({ tick, delivered: frame, ...(afterDelivery ? { afterDelivery } : {}), random, sourceRandom, expected: {
      draws: random.length, sourceDraws: sourceRandom.length, attempts, live: live(game), mind: copy(memory.mind),
    } });
  }
  cases.push({ name: spec.name, map, players, options, shuffle: false, slot: 0, level: 'normal',
    startup: { draws: startupDraws, value: 0.5 }, initial, frames });
}
clean();
const output = JSON.stringify({ reference: pin, node: process.version, arch: process.arch,
  scope: 'Delivered shopping decisions and synchronous admissions; no match steps between frames',
  rng: 'The unchanged source uses global Math.random. Record thinker and command draws separately; native stream isolation needs its own test.',
  cases }) + '\n';
const path = `${root}/rust/crates/core/tests/fixtures/commander-shopping.json`;
if (process.argv.includes('--check')) {
  if (readFileSync(path, 'utf8') !== output) throw Error('Commander shopping fixture differs');
} else writeFileSync(path, output);
console.log(`Verified ${cases.length} live shopping cases, ${cases.reduce((n, c) => n + c.frames.length, 0)} delivered frames`);
