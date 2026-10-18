// Live outcome comparison for AI-65. No fixture files or reference edits.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const [referencePath, mode, level, profile, seedText, ticksText] = process.argv.slice(2);
assert.ok(referencePath);
assert.ok(['conquest', 'classic', 'assault', 'annihilation', 'horde'].includes(mode));
assert.ok(['easy', 'normal', 'hard'].includes(level));
assert.ok(['off', 'on'].includes(profile));
const seed = Number(seedText), maxTicks = Number(ticksText);
assert.ok(Number.isInteger(seed) && seed >= 0 && seed <= 0xffffffff);
assert.ok(Number.isInteger(maxTicks) && maxTicks >= 3600 && maxTicks <= 48000);
const reference = resolve(referencePath);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = (...args) => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => {
  assert.equal(git('rev-parse', 'HEAD'), pin);
  assert.equal(git('status', '--porcelain', '--untracked-files=no'), '');
};
clean();
const { CFG } = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const { playMatch } = await import(pathToFileURL(resolve(reference, 'shared/ai-schedule.js')));
const mapKey = mode === 'horde' ? 'bocage' : 'default';
const run = () => {
  let heldAtThreeMinutes = null, startup;
  const map = JSON.parse(readFileSync(resolve(reference, `maps/${mapKey}.json`)));
  const seats = [0, 1, 2].map(slot => ({ slot, level }));
  if (mode === 'horde') seats.push({ slot: 3, level: 'normal' });
  const g = playMatch({ map, names: ['AI 0', 'AI 1', 'AI 2'], teams: [0, 1, 2],
    factions: [0, 1, 2], shuffle: true, seats, seed, maxTicks,
    options: { mode, defenderTeam: 0, army: 'standard', weather: 'map', mapKey,
      tech: profile === 'on', logistics: profile === 'on', supply: false, sandbox: false },
    setup: g => {
      assert.equal(g.mode?.kind ?? 'conquest', mode);
      assert.equal(g.tech, profile === 'on');
      assert.equal(g.logisticsEnabled, profile === 'on' && ['conquest', 'classic', 'annihilation'].includes(mode));
      assert.equal(g.supply, false);
      assert.equal(g.sandbox, false);
      assert.deepEqual(g.players.map(p => p.team), mode === 'horde' ? [0, 0, 0, 1] : mode === 'assault' ? [0, 1, 1] : [0, 1, 2]);
      assert.deepEqual(g.army, CFG.armies.standard);
      startup = { map, mode: g.mode?.kind ?? 'conquest', teams: g.players.map(p => p.team),
        factions: g.players.map(p => p.faction), army: 'standard', tech: g.tech,
        logistics: g.logisticsEnabled, supply: g.supply, sandbox: g.sandbox,
        weather: g.weather.now, livingWeather: g.wx !== null,
        defenderTeam: g.mode?.defenderTeam ?? null,
        ais: seats.map(s => ({ slot: s.slot, level: s.level,
          brain: s.slot === g.mode?.slot && mode === 'horde' ? 'horde' : 'commander' })),
      };
    },
    onTick: g => {
      if (g.tick === 3600) heldAtThreeMinutes = g.points.filter(p => p.owner >= 0).length;
    },
  });
  return { startup, outcome: {
    tick: g.tick, winner: g.winner, reason: g.endReason ?? null,
    teams: g.players.map(p => p.team),
    units: g.players.map(p => [...g.units.values()].filter(u => u.owner === p.slot).length),
    vp: g.players.map(p => p.vp), points: g.points.length, heldAtThreeMinutes,
    losses: g.story.reduce((n, s) => n + s.losses, 0),
    kills: g.story.reduce((n, s) => n + s.kills, 0),
    spent: g.story.map(s => s.mpSpent),
    fighterCaptured: g.story.some(s => s.kills > 0 && s.captures > 0),
    samples: g.timeline.length,
  } };
};
const result = run();
assert.deepEqual(run(), result, 'the pinned prototype must replay its seeded outcome');
clean();
console.log(JSON.stringify({ reference: pin, node: process.version, mapKey, mode, level, profile, seed, ...result }));
