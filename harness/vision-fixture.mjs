// Writes the vision and fog fixture the Rust test replays (rust/crates/simtest/tests/vision.rs): the bench match
// at tick 6,000 (harness/sim-state.mjs) cut down to the fields the vision and fog passes read, with the
// reference's answers from harness/vision-check.mjs. Run from the repo root with x64 Node (the recon flight and
// the ship below go through Math.cos and Math.sin):
//   node harness/vision-fixture.mjs [--state harness/out/bench-6000.json] [--out rust/crates/simtest/tests/fixtures/vision.json]
//
// The fog caches are left out (the passes rebuild them; the four harness states exercise saved ones), and per-cell
// arrays are written as runs, which keeps the file small. A few changes reach rules the bench match does not (see
// cover below): snow and rain, a live recon flight, camouflaged snipers, woods, an unfinished Scout Tower, a church
// garrison, a squad riding a carrier, one down in a trench, one in smoke, a building seen through its aim point, and
// a ship seen by its hull on a naval map.
import { readFileSync, writeFileSync } from 'node:fs';
import { encode, decode } from './state-codec.mjs';
import { tables } from './tables.mjs';
import { check } from './vision-check.mjs';

const sim = await import('../reference/ww2-rts/shared/sim.js');
const { rebuildGrid } = await import('../reference/ww2-rts/shared/grid.js');
const { updateVision } = sim.internals;
const args = process.argv.slice(2), opt = (name, fallback) => (args.includes(name) ? args[args.indexOf(name) + 1] : fallback);
const STATE = opt('--state', new URL('out/bench-6000.json', import.meta.url).pathname);
const OUT = opt('--out', new URL('../rust/crates/simtest/tests/fixtures/vision.json', import.meta.url).pathname);

const UNIT_FIELDS = ['id', 'type', 'owner', 'x', 'z', 'rot', 'garrison', 'riding', 'still', 'shotAt', 'dust', 'built', 'cells'];
// the fields of the game the passes read, each unit cut down to UNIT_FIELDS (fields left undefined are left out)
export function trim(g) {
  const pick = (o, keys) => Object.fromEntries(keys.filter(k => o[k] !== undefined).map(k => [k, o[k]]));
  const units = new Map([...g.units].map(([id, u]) => [id, { ...pick(u, UNIT_FIELDS), ...(u.air ? { air: { state: u.air.state } } : {}) }]));
  return {
    w: g.w, h: g.h, flags: g.flags, height: g.height, house: g.house, tick: g.tick, visionTick: g.visionTick, terrainVersion: g.terrainVersion,
    mode: g.mode, naval: g.naval, weather: { now: g.weather?.now }, wx: { rain: g.wx?.rain ?? 0 },
    players: g.players.map(p => ({ slot: p.slot, team: p.team, visible: p.visible })),
    units, smokes: g.smokes.map(s => ({ x: s.x, z: s.z, r: s.r })), strikes: g.strikes.map(s => pick(s, ['live', 'kind', 'owner', 'x', 'z', 'dir'])),
    structuralCells: new Map([...g.structuralCells].map(([c, s]) => [c, { state: s.state, hp: s.hp }])),
    fog: g.fog,
  };
}

// Changes that reach rules the bench match leaves alone. The last few are placed where the reference's own vision
// pass says they decide an answer, so a port that gets the rule wrong fails the test.
function cover(g) {
  const list = [...g.units.values()], first = (owner, type) => list.find(u => u.owner === owner && u.type === type);
  g.weather = { now: 'snow' };
  g.wx = { rain: 0.25 };
  // a recon flight of player 0's over player 3's first squad
  const target = list.find(u => u.owner === 3 && !sim.UNITS[u.type].structure);
  g.strikes.push({ live: true, kind: 'recon', owner: 0, x: target.x + 5, z: target.z - 3, dir: 0.7 });
  // player 4's first three rifle squads become snipers: hidden, still too briefly, and quiet but firing lately
  const rifles = list.filter(u => u.owner === 4 && u.type === 'rifle').slice(0, 3);
  Object.assign(rifles[0], { type: 'sniper', still: 5, shotAt: undefined });
  Object.assign(rifles[1], { type: 'sniper', still: 1, shotAt: undefined });
  Object.assign(rifles[2], { type: 'sniper', still: 5, shotAt: g.tick - 40 });
  // a 12 x 12 woods patch around the middle of the ground units
  const ground = list.filter(u => !u.air && !sim.UNITS[u.type].structure);
  const cx = Math.floor(ground.reduce((s, u) => s + u.x, 0) / ground.length / sim.CELL), cz = Math.floor(ground.reduce((s, u) => s + u.z, 0) / ground.length / sim.CELL);
  for (let y = cz - 6; y < cz + 6; y++) for (let x = cx - 6; x < cx + 6; x++) if (!(g.flags[y * g.w + x] & sim.SIGHT)) g.flags[y * g.w + x] |= sim.WOOD | sim.COVER;
  // an unfinished Scout Tower sees only its builders' few metres
  list.find(u => u.type === 'tower').built = 0.5;
  // the garrisoned squad's house becomes a church, which sees farther than garrisonVision
  for (const u of list) if (u.garrison >= 0) g.house[u.garrison] = 3;
  g.naval = true;

  const saved = { visionTick: g.visionTick, visible: g.players.map(p => p.visible) };
  const teamOf = u => g.players[u.owner].team;
  const sees = (team, id) => { rebuildGrid(g); updateVision(g); return g.players.some(p => p.team === team && p.visible.has(id)); };
  const seenBy = id => { rebuildGrid(g); updateVision(g); return g.players.filter(p => p.visible.has(id)).map(p => p.team).join(); };
  // keeps `change` on the first unit whose change alters which teams see it, and undoes it on the others
  const decisive = (what, units, change) => {
    for (const u of units) {
      const before = seenBy(u.id), undo = change(u);
      if (seenBy(u.id) !== before) return u;
      undo();
    }
    throw new Error(`no unit where ${what} decides who sees it`);
  };
  let nextId = Math.max(...g.units.keys()) + 1;
  // a new squad of player `owner`'s `dist` metres from `target`, kept on the first of eight sides where `works()`
  // says so; false, and no squad, when no side works
  const scout = (owner, target, dist, works) => {
    const u = { ...first(owner, 'rifle'), id: nextId, still: 0, shotAt: undefined, dust: undefined };
    for (let k = 0; k < 8; k++) {
      Object.assign(u, { x: target.x + dist * Math.cos(k * Math.PI / 4), z: target.z + dist * Math.sin(k * Math.PI / 4) });
      g.units.set(u.id, u);
      if (works()) { nextId++; return true; }
    }
    g.units.delete(u.id);
    return false;
  };
  const isSquad = u => !u.air && sim.UNITS[u.type].infantry && !sim.UNITS[u.type].camo && u.garrison < 0 && !u.riding;
  updateVision(g);
  const seen = new Set(g.players.flatMap(p => [...p.visible])), squads = () => list.filter(u => seen.has(u.id) && isSquad(u));
  // a squad someone sees rides a carrier
  squads()[0].riding = 1;
  // one goes down in a trench and holds fire
  decisive('a trench', squads(), u => {
    const c = Math.floor(u.z / sim.CELL) * g.w + Math.floor(u.x / sim.CELL), flags = g.flags[c], shotAt = u.shotAt;
    g.flags[c] |= sim.TRENCH | sim.COVER; u.shotAt = undefined;
    return () => { g.flags[c] = flags; u.shotAt = shotAt; };
  });
  // a squad of player 0's 12 m from player 4's HQ that sees it alone, only through the aim point (a line to the HQ's
  // middle crosses its own walls)
  const hq = first(4, 'hq'), team0 = g.players[0].team, team1 = g.players[1].team;
  if (sees(team0, hq.id) || !scout(0, hq, 12, () => sees(team0, hq.id))) throw new Error('no side of the HQ lets a scout see it');
  // squads of player 1's: one 20 m from an enemy squad that only it sees, with a smoke cloud over that squad; one 36 m
  // from an enemy vehicle, beyond its sight unless the vehicle trails dust, and the dust
  const smoked = list.filter(t => teamOf(t) !== team1 && isSquad(t) && !sees(team1, t.id)).some(t => scout(1, t, 20, () => {
    if (!sees(team1, t.id)) return false;
    g.smokes.push({ x: t.x, z: t.z, r: 5 });
    if (!sees(team1, t.id)) return true;
    g.smokes.pop();
    return false;
  }));
  const vehicles = list.filter(t => teamOf(t) !== team1 && !t.air && !sim.UNITS[t.type].infantry && !sim.UNITS[t.type].structure && !sees(team1, t.id));
  const dusty = vehicles.some(t => {
    const dust = t.dust;
    return scout(1, t, 36, () => {
      t.dust = dust;
      if (sees(team1, t.id)) return false;
      t.dust = g.tick;
      if (sees(team1, t.id)) return true;
      t.dust = dust;
      return false;
    });
  });
  if (!smoked || !dusty) throw new Error(`found no squad that smoke hides (${smoked}) or no vehicle that dust shows (${dusty})`);
  // a gunboat of player 2's beside a squad of another team, end on, where its hull's near end is in that squad's sight
  // and its middle is not: on the first heading where it is seen as a ship and would not be as a point
  const ship = { id: nextId++, type: 'gunboat', owner: 2, x: 0, z: 0, rot: 0, garrison: -1, riding: 0, still: 0 };
  const placed = list.filter(u => !u.air && sim.UNITS[u.type].infantry && teamOf(u) !== teamOf(ship) && u.garrison < 0).some(eye => [0, 1, 2, 3, 4, 5, 6, 7].some(k => {
    const a = k * Math.PI / 4, r = sim.UNITS[eye.type].vision * 0.9 + 4;
    Object.assign(ship, { type: 'gunboat', x: eye.x + r * Math.cos(a), z: eye.z + r * Math.sin(a), rot: a });
    if (ship.x < 0 || ship.z < 0 || ship.x >= g.w * sim.CELL || ship.z >= g.h * sim.CELL) return false;
    g.units.set(ship.id, ship);
    if (!sees(teamOf(eye), ship.id)) return false;
    ship.type = 'mg';
    const asPoint = sees(teamOf(eye), ship.id);
    ship.type = 'gunboat';
    return !asPoint;
  }));
  if (!placed) throw new Error('found no place where the gunboat is seen only by its hull');
  rebuildGrid(g);
  g.visionTick = saved.visionTick;
  g.players.forEach((p, i) => { p.visible = saved.visible[i]; });
}

if (import.meta.main) {
  if (process.arch !== 'x64') throw new Error(`run this with an x64 Node, not ${process.arch}: the recon flight and the ship go through Math.cos and Math.sin`);
  const saved = JSON.parse(readFileSync(STATE, 'utf8')), g = trim(decode(saved.game));
  cover(g);
  const game = encode(g, { runs: true }), expected = check(decode(JSON.parse(JSON.stringify(game))));
  const fixture = { reference: 'e4e579a', node: process.version, v8: process.versions.v8, arch: process.arch, tables: tables(), game, expected };
  writeFileSync(OUT, JSON.stringify(fixture) + '\n');
  console.log(`wrote ${OUT}: ${g.units.size} units, ${expected.length} lines, ${JSON.stringify(fixture).length} bytes`);
}
