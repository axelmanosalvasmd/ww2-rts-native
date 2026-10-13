// Record the original population helpers and income phase at the pinned reference.
// The source slices are executed unchanged, without the rest of step() altering the setup.
// x64 Node 24.21.0 harness/economy-state-fixture.mjs --reference=/path/to/ww2-rts [--check]
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
if (process.version !== 'v24.21.0' || process.arch !== 'x64') throw Error('Use x64 Node 24.21.0');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.argv.find(a => a.startsWith('--reference='))?.slice(12) ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
if (execFileSync('git', ['rev-parse', 'HEAD'], { cwd: reference, encoding: 'utf8' }).trim() !== pin) throw Error('Wrong reference pin');
execFileSync('git', ['diff', '--exit-code', pin, '--', 'shared'], { cwd: reference, stdio: 'pipe' });
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const { WORLD_TUNING } = await import(pathToFileURL(`${reference}/shared/world-conquest.js`));
const src = readFileSync(`${reference}/shared/sim.js`, 'utf8');
const slice = (start, end) => {
  const a = src.indexOf(start), b = src.indexOf(end, a);
  if (a < 0 || b < 0) throw Error(`Missing source marker ${start}`);
  return src.slice(a, b);
};
const world = slice('function worldIncome(g, p, list, dt)', 'function recoverWorld(');
const region = slice('function worldRegionAt(g, at)', 'const worldOwned =');
const income = slice('  const teamVp = (t) =>', '  stepConvoys(g, dt, supplyHooks(g));');
// Fixtures use the source's regular region grid, so insideKnownRegion is never needed.
const run = new Function('CFG', 'UNITS', 'WORLD_TUNING', 'allied', 'CELL', `${region}\n${world}\nreturn (g, dt) => { const list = [...g.units.values()]; ${income}\n};`)(sim.CFG, sim.UNITS, WORLD_TUNING, sim.allied, sim.CELL);
const bits = n => { const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex'); };
const clone = v => structuredClone(v);
const player = (team, vp, extra = {}) => ({ team, vp, mp: 103.25, mun: 9.5, fuel: 8.75, inc: 123, ...extra });
const unit = (id, owner, type, extra = {}) => ({ id, owner, type, hp: 100, built: 1, queue: [], ...extra });
const base = {
  players: [player(0, 40), player(0, 20, { away: true }), player(1, 780), player(2, 10, { out: true })],
  units: [unit(1, 0, 'rifle', { queue: ['tiger'] }), unit(2, 0, 'conscript', { hp: 0 }), unit(3, 0, 'hq', { queue: ['rifle', 'conscript', 'tank'] }), unit(4, 1, 'engineer'), unit(5, 0, 'depot'), unit(6, 0, 'depot', { built: .5 }), unit(7, 2, 'depot'), unit(8, -1, 'worldbase'), unit(30, 0, 'fighter'), ...Array.from({length: 10}, (_, i) => unit(i + 9, 2, 'tiger'))],
  strikes: [{ owner: 0, kind: 'para', live: false }, { owner: 0, kind: 'para', live: true }, { owner: 1, kind: 'smoke', live: false }],
  points: [{ owner: 0, mp: 1.5, vp: 2, cut: false }, { owner: 1, mp: 4, vp: 3, cut: false }, { owner: 0, mp: 19, vp: 5, cut: true }, { owner: 2, mp: 1, vp: 1, cut: false }, { owner: -1, mp: 50, vp: 9, cut: false }],
  nodes: [{ depot: 5, rate: 2.5, fuel: false, region: 0 }, { depot: 5, rate: 1.5, fuel: true, region: 1 }, { depot: 6, rate: 8, fuel: false, region: 0 }, { depot: 7, rate: .4, fuel: true, region: 3 }, { depot: 999, rate: 40, fuel: false, region: null }, { depot: 5, rate: 40, fuel: false, region: null }],
  regions: [{team: 0, kind: 'city'}, {team: 0, kind: 'industrial'}, {team: 0, kind: 'resource'}, {team: 1, kind: 'other'}, {team: 0, kind: 'other'}, {team: 0, kind: 'city'}],
};
const set = (root, path) => { const value = path.at(-1), keys = path.slice(0,-1), key = keys.pop(); let at = root; for (const k of keys) at = at[k]; at[key] = clone(value); };
const steps = [
  {dt: .05, set: []},
  {dt: .3, set: [['players',1,'away',false], ['players',3,'out',false], ['units',5,'built',1], ['points',2,'cut',false]]},
  {dt: 1.25, set: [['players',1,'out',true], ['regions',1,'team',1], ['units',4,'hp',0], ['units',2,'queue',[]], ['strikes',0,'live',true]]},
  {dt: .05, set: [['players',0,'away',true], ['players',2,'out',true], ['points',0,'owner',2]]},
  {dt: .8, winner: true, set: [['players',0,'away',false], ['players',0,'out',true]]},
  {dt: 0, winner: true, set: []},
];
const modes = [null, {kind:'conquest'}, {kind:'assault',defenderTeam:0}, {kind:'annihilation'}, {kind:'horde',defenderTeam:0,slot:2}, {kind:'tutorial',slot:3}, {kind:'classic'}, {kind:'world'}, {kind:'other'}];
const scenarios = [];
for (const mode of modes) for (const army of Object.keys(sim.CFG.armies)) {
  const input = clone(base);
  const players = input.players.map((p,slot) => ({...p,slot}));
  const makeGame = winner => ({players, units:new Map(input.units.map(u=>[u.id,u])),strikes:input.strikes,points:input.points,nodes:input.nodes.map(n => ({...n, x:n.region === null ? -1 : n.region * sim.CELL + .1,z:.1})), mode, army:sim.CFG.armies[army], winner:winner ? 0 : null,w:input.regions.length,h:1,world:{regions:input.regions,regionCells:1}});
  const trace = [];
  for (const step of steps) {
    for (const path of step.set) { set(input,path); if (path[0] === 'players') set({players},path); }
    const g = makeGame(step.winner);
    run(g, step.dt);
    trace.push({set:step.set,dt:step.dt,winner:!!step.winner, expected:players.map((p,slot) => ({wallet:['mp','mun','fuel','vp','inc','fuelInc','upkeep'].map(k => p[k] === undefined ? null : bits(p[k])),population:bits(sim.popOf(g,slot)),cap:bits(sim.popCap(g,slot))}))});
  }
  scenarios.push({mode,army,trace});
}
// Exercise the World cap clamp before army scaling, all teammates out, and zero seats gained.
const capCases = [
  {teams:[0,0,1], out:[false,false,false], away:[false,true,false], owned:100, army:'large', slot:0},
  {teams:[0,0,1], out:[true,true,false], away:[false,false,false], owned:8, army:'massive', slot:0},
  {teams:[0,0,1], out:[false,false,false], away:[false,false,false], owned:1, army:'standard', slot:0},
].map(c => ({...c,expected:bits(sim.popCap({mode:{kind:'world'},players:c.teams.map((team,i)=>({team,out:c.out[i],away:c.away[i]})),world:{regions:Array.from({length:c.owned},()=>({team:0}))},army:sim.CFG.armies[c.army]},c.slot))}));
const preview = [0, 37.25, -1].map(cap=>({cap,expected:bits(sim.popCap({mode:{kind:'world'},world:{cap},army:sim.CFG.armies.endless}))}));
const data = JSON.stringify({reference:pin,base,scenarios,capCases,preview})+'\n';
const dest = `${root}/rust/crates/core/tests/fixtures/economy-state.json`;
if (process.argv.includes('--check')) { if (readFileSync(dest,'utf8') !== data) throw Error('Economy fixture differs'); }
else writeFileSync(dest,data);
console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} ${scenarios.length} traces, ${scenarios.length * steps.length} income steps, ${capCases.length} World cap cases`);
