// Original createGame startup state, plus an observational capture before base placement.
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
if (process.arch !== 'x64' || process.version !== 'v24.21.0') throw Error('Use x64 Node v24.21.0');
const reference = resolve(process.env.WW2_REFERENCE ?? fileURLToPath(new URL('../reference/ww2-rts/', import.meta.url)));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
if (git(['rev-parse', 'HEAD']) !== pin || git(['status', '--porcelain', '--untracked-files=no'])) throw Error('Reference must be clean at the named pin');
registerHooks({ load(url, context, next) {
  const result = next(url, context);
  if (url !== pathToFileURL(resolve(reference, 'shared/sim.js')).href) return result;
  let source = String(result.source);
  const anchor = '  if (horde) setupHorde(g, map);';
  if (source.split(anchor).length !== 2) throw Error('Startup probe anchor changed');
  source = 'export let __startingUnits;\n' + source.replace(anchor, '__startingUnits = [...g.units.values()].map(u => structuredClone(u));\n' + anchor);
  const baseAnchor = '  if (isSkirmishBaseMode(g)) setupSkirmishBases(g);';
  if (source.split(baseAnchor).length !== 2) throw Error('Base capture anchor changed');
  source = source.replace(baseAnchor, '  g.cellJournal = []; for (const p of g.players) p.terrainPending = new Set();\n' + baseAnchor);
  return { ...result, source };
} });
const sim = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const mask = (1n << 64n) - 1n, gamma = 0x9e3779b97f4a7c15n;
function mix(x) { x = ((x ^ (x >> 30n)) * 0xbf58476d1ce4e5b9n) & mask; x = ((x ^ (x >> 27n)) * 0x94d049bb133111ebn) & mask; return x ^ (x >> 31n); }
function stream(seed, purpose) { let state = mix((seed ^ ((BigInt(purpose) * gamma) & mask)) & mask); return () => mix(state = (state + gamma) & mask); }
const fraction = n => Number(n >> 11n) / 9007199254740992;
const buffer = new DataView(new ArrayBuffer(8));
const bits = n => { buffer.setFloat64(0, n); return buffer.getBigUint64(0).toString(16).padStart(16, '0'); };
function runs(values) { const out = []; for (const v of values) { if (out.length && out.at(-1)[0] === v) out.at(-1)[1]++; else out.push([v, 1]); } return out; }
const floatRuns = a => runs(new Uint32Array(a.buffer, a.byteOffset, a.length));
const cases = [], maps = [];
for (const file of readdirSync(resolve(reference, 'maps')).filter(n => n.endsWith('.json')).sort()) {
  const map = JSON.parse(readFileSync(resolve(reference, 'maps', file), 'utf8'));
  if (map.world || map.scenario) continue;
  maps.push({ name: file, map });
}
function record(mapIndex, count, army, weather, living, shuffle, teamMode, tech, explicitSeed = true) {
  const seed = BigInt(0x551200 + cases.length), terrain = stream(seed, 1), weatherRandom = stream(seed, 2);
  const layoutFraction = shuffle ? fraction(terrain()) : null;
  const initialLivingSeed = Number(weatherRandom() & 0xffffffffn);
  // These fixtures pass a weather seed explicitly, isolating the exact weather-plan function.
  const weatherSeed = explicitSeed ? 217 + cases.length : undefined;
  const drawnWeatherSeed = !explicitSeed && weather === 'random' ? Number(weatherRandom() % (1n << 31n)) : null;
  const players = Array.from({ length: count }, (_, i) => ({ name: `Player ${i + 1}`, team: teamMode ? i % 2 : i, faction: i % 4 }));
  const draws = [...(shuffle ? [layoutFraction] : []), initialLivingSeed / 4294967296, ...(drawnWeatherSeed === null ? [] : [drawnWeatherSeed / 2147483648])];
  const old = Math.random; let used = 0, game;
  Math.random = () => { if (used >= draws.length) throw Error('Unexpected unseeded startup draw'); return draws[used++]; };
  try { game = sim.createGame(maps[mapIndex].map, players.map(p => p.name), shuffle, players.map(p => p.team), players.map(p => p.faction), { mode: 'conquest', army, weather: living ? weather : false, weatherSeed, mapKey: maps[mapIndex].name, tech }); }
  finally { Math.random = old; }
  if (used !== draws.length) throw Error('Startup draw count changed');
  cases.push({ map: mapIndex, seed: seed.toString(), players, army, weather, living, shuffle, tech, weatherSeed,
    expected: {
      players: game.players.map(p => ({ name: p.name, team: p.team, faction: p.faction, spawn: [bits(p.spawn.x), bits(p.spawn.z)], mp: bits(p.mp), inc: bits(p.inc), vp: bits(p.vp), support: Object.fromEntries(Object.entries(p.sup).map(([k,v]) => [k,bits(v)])), tier: p.tier ?? null })),
      points: game.points.map(p => ({ id: p.scenarioId, at: [bits(p.x), bits(p.z)], vp: bits(p.vp), mp: bits(p.mp), kind: p.kind ?? null, needs: p.needs })),
      winVp: bits(game.winVp), weather: game.weather, wind: [bits(game.wind.a), bits(game.wind.v)],
      wet: game.wx ? bits(game.wx.wet) : null, rain: bits(game.wx?.rain ?? 0),
      living: game.wx ? { seed: game.seed >>> 0, raining: game.wx.raining, next: bits(game.wx.next) } : null,
      startingUnits: sim.__startingUnits,
      finalUnits: [...game.units.values()],
      environmentSeed: game.seed >>> 0,
      production:[...game.units.values()].filter(u=>u.queue).map(u=>[u.id,u.queue,u.productionJobs,bits(u.prog),u.rally]),
      tech:game.players.map(p=>p.tier===undefined?null:[p.tier,p.armory,p.lab]),
      scenario:game.scenario??null,
      finalTerrain: { chars:runs(game.chars), ground:runs(game.ground), objects:runs(game.objects), flags:runs(game.flags), height:game.height && runs(game.height), hp:floatRuns(game.cellHp), wear:game.cellLog.map(r=>r[0]).map(c=>[c,new Uint32Array(game.wear.buffer)[c]]), state:runs(game.cellState),
        versions:[game.terrainVersion??0,game.obstructionVersion??0,game.infantryRegionVersion??0,game.vehicleRegionVersion??0,game.navalRegionVersion??0],
        log:game.cellLog, newCells:game.newCells, journal:[...game.cellJournal],
        buildingCells:[...game.buildingCells??[]].sort((a,b)=>a[0]-b[0]).map(([c,b])=>[c,b.id,b.owner,bits(b.x),bits(b.z)]),
        pending:game.players.map(p=>[...(p.terrainPending??[])].sort((a,b)=>a-b)),
        buildings:[...game.structures.values()].filter(s=>s.kind==='building').map(s=>[s.id,s.entityId,s.sections.map(t=>t.c),s.sections.map(t=>[t.id,t.structureId,t.c,bits(t.hp),bits(t.maxHp),t.material,t.anchor,t.supports,t.state,t.rubble])]) },
    } });
}
for (let i = 0; i < maps.length; i++) record(i, 2, 'standard', 'map', true, true, false, false);
for (let i = 0; i < 4; i++) for (const weather of ['clear', 'fog', 'rain', 'mud', 'snow', 'random']) record(i, 6, ['standard', 'large', 'massive', 'endless'][i], weather, true, i % 2 === 0, true, true);
record(0, 1, 'standard', 'map', false, false, false, false);
record(0, 2, 'standard', 'random', true, false, false, false, false);
record(0, 2, 'standard', 'map', true, false, false, false, false);
for (const name of ['causeway', 'nearby-spawns', 'steep-pad']) {
  const rows = Array(20).fill('W'.repeat(20)); rows[10]='.'.repeat(20);
  const map = {name, w:20, h:20, rows, spawns:[{x:1,y:10},{x:name==='nearby-spawns'?3:18,y:10}], points:[{x:10,y:10}]};
  if(name==='steep-pad') { map.rows=Array(20).fill('.'.repeat(20)); map.heights=Array(20).fill('04040404040404040404'); }
  if(sim.validateMap(map)!==null) throw Error(`Invalid synthetic ${name}`);
  maps.push({name,map});record(maps.length-1,2,'standard','map',true,false,false,false);
}
const output = JSON.stringify({ reference: pin, node: process.version, arch: process.arch, maps, cases }) + '\n';
const path = fileURLToPath(new URL('../rust/crates/simtest/tests/fixtures/match-start.json', import.meta.url));
if (process.argv.includes('--check')) { if (readFileSync(path, 'utf8') !== output) throw Error('Match startup fixture differs'); }
else writeFileSync(path, output);
console.log(`Verified ${cases.length} startup cases across 33 authored maps and 3 synthetic maps`);
