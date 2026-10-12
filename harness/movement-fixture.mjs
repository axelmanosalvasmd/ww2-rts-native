// Writes the movement fixture the Rust test replays (rust/crates/simtest/tests/movement.rs): two corners of the frozen
// 1,000-unit state (harness/sim-state.mjs) recorded call by call (harness/movement-trace.mjs) for a few ticks. Run
// from the repo root with x64 Node 24.21.0 (movement goes through Math.atan2, sin, cos and tan):
//   node harness/movement-fixture.mjs [--state harness/out/frozen-1000.json] [--ticks 3]
//     [--out rust/crates/simtest/tests/fixtures/movement.json]
//
// The frozen workload never reaches some rules: the Six Fronts map has no roads, mud, fords or woods, nothing is
// wet, no vehicle gets walled in, nobody sprints or backs up. cover() sets up cases on open ground near the corners
// so each of those rules decides where a unit ends up (CASES lists them), and the fixture records the reference's
// answers. Per-cell arrays are written as runs, which keeps the file small.
import { readFileSync, writeFileSync } from 'node:fs';
import { decode } from './state-codec.mjs';
import { traceSim, trace } from './movement-trace.mjs';

const sim = await traceSim();
const { sweptVehicleClear, vehiclePositionClear, turnAngle, movementProfile, bodyRadius } = await import('../reference/ww2-rts/shared/vehicle-motion.js');
const { UNITS, CELL, TICK, MOVE, VBLOCK, COVER, FORD, WIRE, ROAD, MUD, WOOD, blockOf } = sim;

const args = process.argv.slice(2), opt = (name, fallback) => (args.includes(name) ? args[args.indexOf(name) + 1] : fallback);
const STATE = opt('--state', new URL('out/frozen-1000.json', import.meta.url).pathname);
const OUT = opt('--out', new URL('../rust/crates/simtest/tests/fixtures/movement.json', import.meta.url).pathname);
const TICKS = Number(opt('--ticks', 3));
// the corners kept: about 150 units of player 5's, mostly conscripts, with tanks, flak tracks and rocket trucks, and
// about 80 of player 1's, squads among tanks and armoured cars, some in a one-lane stretch or passing through friends
const AREAS = [[25, 50, 100, 100], [125, 225, 175, 275]];

export const CASES = [];
const cellAt = (g, x, z) => Math.floor(z / CELL) * g.w + Math.floor(x / CELL);
const center = (g, c) => ({ x: (c % g.w + 0.5) * CELL, z: (Math.floor(c / g.w) + 0.5) * CELL });
// adds terrain bits to the open cells in a square of `r` cells around a point
function paint(g, at, r, bits) {
  const cx = Math.floor(at.x / CELL), cz = Math.floor(at.z / CELL);
  for (let z = cz - r; z <= cz + r; z++) for (let x = cx - r; x <= cx + r; x++) {
    const c = z * g.w + x;
    if (x >= 0 && z >= 0 && x < g.w && z < g.h && !(g.flags[c] & (MOVE | VBLOCK))) g.flags[c] |= bits;
  }
}

// Blocks for vehicles (VBLOCK) the fewest cells near vehicle u, in a fixed order, so that `wanted` holds: the walled-in
// tests of driveVehicle. Returns the cells, or null.
function blockFor(g, u, wanted) {
  const def = UNITS[u.type], block = blockOf(def), profile = movementProfile(u.type, def);
  const before = { x: u.x, z: u.z, rot: u.rot }, wp = u.path[0];
  const rot = turnAngle(u.rot, Math.atan2(wp.z - u.z, wp.x - u.x), profile.hullTurn * TICK);
  const swept = (a, b) => sweptVehicleClear(g, a, b, def, block);
  const turns = at => swept(at, { ...at, rot: Math.atan2(wp.z - at.z, wp.x - at.x) });
  const along = dir => ({ x: before.x + Math.cos(before.rot) * 2 * dir, z: before.z + Math.sin(before.rot) * 2 * dir, rot: before.rot });
  const side = s => ({ x: before.x - Math.sin(before.rot) * s, z: before.z + Math.cos(before.rot) * s, rot: before.rot });
  const test = {
    // it cannot turn on the spot, but backing up or pulling forward 2 m leaves room to turn
    clearance: () => !swept(before, { ...before, rot }) && [-1, 1].some(d => swept(before, along(d)) && turns(along(d))),
    // nor that, but sliding sideways does
    slide: () => !swept(before, { ...before, rot }) && ![-1, 1].some(d => swept(before, along(d)) && turns(along(d)))
      && [0.5, -0.5, 1, -1, 1.5, -1.5].some(s => swept(before, side(s)) && turns(side(s))),
  }[wanted];
  const cx = Math.floor(u.x / CELL), cz = Math.floor(u.z / CELL), near = [];
  for (let z = cz - 3; z <= cz + 3; z++) for (let x = cx - 3; x <= cx + 3; x++) if (!(g.flags[z * g.w + x] & block)) near.push(z * g.w + x);
  const tryCells = (cells) => {
    for (const c of cells) g.flags[c] |= VBLOCK;
    if (vehiclePositionClear(g, before, def, block) && test()) return true;
    for (const c of cells) g.flags[c] &= ~VBLOCK;
    return false;
  };
  for (const c of near) if (tryCells([c])) return [c];
  for (let i = 0; i < near.length; i++) for (let j = i + 1; j < near.length; j++) if (tryCells([near[i], near[j]])) return [near[i], near[j]];
  for (let i = 0; i < near.length; i++) for (let j = i + 1; j < near.length; j++) for (let k = j + 1; k < near.length; k++) {
    if (tryCells([near[i], near[j], near[k]])) return [near[i], near[j], near[k]];
  }
  return null;
}

// Case sites: open ground (no terrain bits within 4 cells, one level within 3) with nobody within 12 m and no other site
// within 18 m, searched outward from a point in a fixed order. Each case that depends on the ground gets its own, so
// the cases do not meet.
const sites = [];
function site(g, from) {
  const cx0 = Math.floor(from.x / CELL), cz0 = Math.floor(from.z / CELL), units = [...g.units.values()];
  for (let r = 0; r < 120; r++) for (let kz = cz0 - r; kz <= cz0 + r; kz++) for (let kx = cx0 - r; kx <= cx0 + r; kx++) {
    if (Math.max(Math.abs(kx - cx0), Math.abs(kz - cz0)) !== r || kx < 6 || kz < 6 || kx >= g.w - 6 || kz >= g.h - 6) continue;
    const c = kz * g.w + kx, at = center(g, c);
    if (units.some(v => Math.hypot(v.x - at.x, v.z - at.z) < 12) || sites.some(s => Math.hypot(s.x - at.x, s.z - at.z) < 18)) continue;
    let open = true;
    for (let z = kz - 4; open && z <= kz + 4; z++) for (let x = kx - 4; open && x <= kx + 4; x++) {
      open = g.flags[z * g.w + x] === (g.flags[z * g.w + x] & 1024) && (Math.max(Math.abs(x - kx), Math.abs(z - kz)) > 3 || g.height[z * g.w + x] === g.height[c]);
    }
    if (!open) continue;
    sites.push(at);
    return at;
  }
  throw new Error('no open site left');
}

// Puts unit u at `at`, heading `a` with a waypoint `far` metres on, and clears what it was doing (a badly hurt unit would
// otherwise run for home before its movement block: auto-retreat).
function send(u, at, a = 0, far = 10) {
  Object.assign(u, {
    x: at.x, z: at.z, rot: a, travelDir: a, path: [{ x: at.x + Math.cos(a) * far, z: at.z + Math.sin(a) * far }], worldGoal: null, amove: null,
    retreating: false, reverse: null, traffic: null, trafficWait: 0, trafficTrail: [], trafficEntered: false, queuedBehind: 0, laneScan: null,
    motionClearance: null, stuck: 0, repath: 0, moveSpeed: 0, garrison: -1, autoRetreat: false,
  });
  return u;
}

// Moves vehicle u near `from`, nobody within 12 m and no site within 18 m, heading 0 with its front and one side 5 cm
// short of a cell's edge (so one tick's turn can be blocked by a single cell), with a waypoint 10 m off at 0.6 rad, and
// blocks cells around it so that `wanted` holds. Returns the cells, or null (u unchanged).
function wallIn(g, u, wanted, from) {
  const def = UNITS[u.type], block = blockOf(def), long = bodyRadius(def), wide = def.radius * 0.48;
  const saved = { ...u }, others = [...g.units.values()].filter(v => v !== u);
  const cx0 = Math.floor(from.x / CELL), cz0 = Math.floor(from.z / CELL);
  for (let r = 0; r < 60; r++) for (let kz = cz0 - r; kz <= cz0 + r; kz++) for (let kx = cx0 - r; kx <= cx0 + r; kx++) {
    if (Math.max(Math.abs(kx - cx0), Math.abs(kz - cz0)) !== r || kx < 6 || kz < 6 || kx >= g.w - 6 || kz >= g.h - 6) continue;
    const x = kx * CELL - 0.05 - long, z = kz * CELL - 0.05 - wide;
    if (others.some(v => Math.hypot(v.x - x, v.z - z) < 12) || sites.some(s => Math.hypot(s.x - x, s.z - z) < 18)) continue;
    send(u, { x, z }, 0);
    u.path = [{ x: x + 10 * Math.cos(0.6), z: z + 10 * Math.sin(0.6) }];
    if (!vehiclePositionClear(g, u, def, block)) continue;
    const cells = blockFor(g, u, wanted);
    if (cells) {
      sites.push({ x, z });
      return cells;
    }
  }
  Object.assign(u, saved);
  return null;
}

// Changes that reach rules the frozen workload leaves alone, each set up so the rule decides where a unit ends up.
export function cover(g) {
  const list = [...g.units.values()], used = new Set();
  // the first unit not used yet that passes `test`, else a copy of one that does, with the next id
  const take = (what, test) => {
    let u = list.find(u => !used.has(u.id) && test(u, UNITS[u.type]));
    for (let i = 0; !u && i < list.length; i++) {
      const copy = Object.assign(structuredClone(list[i]), { id: g.nextId, cargo: [] });
      if (!test(copy, UNITS[copy.type])) continue;
      u = copy;
      g.units.set(g.nextId++, u);
      list.push(u);
    }
    if (!u) throw new Error(`no unit for ${what}`);
    used.add(u.id);
    CASES.push(`${what}: unit ${u.id} (${u.type})`);
    return u;
  };
  const vehicle = d => !d.infantry && !d.structure && !d.air && !d.naval;
  const squad = d => d.infantry;
  const moving = u => u.path.length > 0 && !u.traffic;
  const idle = u => !u.path.length && !u.worldGoal && !u.retreating && u.garrison < 0 && !u.riding;
  const copy = p => ({ x: p.x, z: p.z });
  // case sites are looked for outward from the middle of the first corner
  const middle = { x: (AREAS[0][0] + AREAS[0][2]) / 2, z: (AREAS[0][1] + AREAS[0][3]) / 2 };
  const level = at => g.height[cellAt(g, at.x, at.z)];
  // a cell's object letter and terrain bits (and the hit points it has taken: its cell hp)
  const object = (at, ch, bits, hp = 0) => { const k = cellAt(g, at.x, at.z); g.objects[k] = ch; g.flags[k] |= bits; g.cellHp[k] = hp; return k; };
  // a cell's wear; its look (cellState) follows, as the game keeps them together
  const wear = (k, w) => { g.wear[k] = w; g.cellState[k] = (g.cellState[k] & ~3) | Math.min(3, Math.floor(g.wear[k] * 4)); };

  // the weather: snow slows everyone, and the ground is a little wet (twice as wet in hollows); it dries as the ticks go
  g.weather = { now: 'snow', next: null, at: 0 };
  g.wx.wet = 0.2;
  CASES.push('snow, and wet ground drying out');

  // vehicles walled in by tank traps: one backs up or pulls forward to turn (motionClearance), one slides sideways
  for (const wanted of ['clearance', 'slide']) {
    let cells = null;
    // stuck long enough to look for room, not long enough to ask for a new route
    take(`a vehicle walled in (${wanted})`, (u, def) => vehicle(def) && !!(cells = wallIn(g, u, wanted, middle))).stuck = 0.5;
    CASES.push(`  tank traps at cells ${cells.join(', ')}`);
  }

  // ground the map lacks, each on a site of its own: vehicles on a worn road, in worn mud, in woods in a hollow (too
  // wet for dust) and in a worn ford; squads in wire, in a worn ford and along a trench
  for (const [what, bits, kind, worn, hollow] of [['a worn road', ROAD, vehicle, 0.4], ['worn mud', MUD, vehicle, 0.6],
    ['woods in a hollow', WOOD | COVER, vehicle, 0, true], ['a ford', FORD, vehicle, 0.5], ['wire', WIRE, squad], ['a ford', FORD, squad, 0.5],
    ['a trench', sim.TRENCH | COVER, squad]]) {
    const at = site(g, middle), u = send(take(what, (u, def) => kind(def)), at, 0.3);
    paint(g, at, 4, bits);
    if (worn) for (let z = -4; z <= 4; z++) for (let x = -4; x <= 4; x++) wear(cellAt(g, at.x + x * CELL, at.z + z * CELL), worn);
    if (hollow) for (let z = -5; z <= 5; z++) for (let x = -5; x <= 5; x++) g.height[cellAt(g, at.x + x * CELL, at.z + z * CELL)] = level(at) - 1;
    if (kind === vehicle) u.moveSpeed = 2;
  }

  // a step of two levels just ahead: a squad drops its route, a moving vehicle is held
  for (const kind of [squad, vehicle]) {
    const at = site(g, middle), c = cellAt(g, at.x, at.z);
    const u = send(take('a cliff just ahead', (u, def) => kind(def)), { x: (c % g.w + 1) * CELL - 0.1, z: at.z }, 0);
    g.height[c + 1] = g.height[c] + 2;
    if (kind === vehicle) u.moveSpeed = 3;
  }
  // two squads at a cell centre, their first waypoint half a metre east and the next one cell over diagonally, so the
  // line to it runs through a cell corner: over level ground (they skip the first waypoint), and with a step of two
  // levels beside the corner (they walk to the first)
  for (const step of [false, true]) {
    const at = site(g, middle), c = cellAt(g, at.x, at.z);
    send(take(`a squad heading through a cell corner${step ? ' beside a cliff' : ''}`, (u, def) => squad(def)), at).path = [{ x: at.x + 0.5, z: at.z }, { x: at.x + CELL, z: at.z + CELL }];
    if (step) g.height[c + 1] = g.height[c] + 2;
  }

  // traffic: a vehicle that has waited meets a stopped squad of its own and drives round it (passing legs); one that
  // has waited long passes through it (a ghost); a retreating vehicle outranks a squad holding its position, which
  // still does not give way, so it drives round
  for (const [what, wait, retreat, hold] of [['drives round a stopped squad', 0.4], ['passes through a friend', 3.5], ['retreating, meets a squad holding its position', 0.4, true, true]]) {
    const at = site(g, middle), v = send(take(`a vehicle that ${what}`, (u, def) => vehicle(def)), at, 0, 12);
    Object.assign(v, { trafficWait: wait, retreating: !!retreat, moveSpeed: 2 });
    Object.assign(send(take('  the squad', (u, def) => squad(def) && u.owner === v.owner), { x: at.x + (hold ? 3 : 4), z: at.z }), { path: [], holdPos: !!hold });
  }
  // squads stepping aside past a stopped unit: one reaching a leg with more to go, one reaching its last leg (it takes
  // up its route again at the furthest waypoint with a clear line), two stepping aside for a unit (one far off by now,
  // one still close); one whose time to pass ran out, one whose route ahead is walled off when its last leg ends, and
  // one still on its way to the point it steps aside to
  for (const [what, legs, passing, near, extra] of [['reaching a passing leg', 2, true, true], ['reaching its last passing leg', 1, true, true],
    ['stepping aside for a unit gone off', 1, false, false], ['stepping aside for a unit still close', 1, false, true],
    ['passing whose time ran out', 2, true, true, 'late'], ['reaching its last passing leg, its route walled off', 1, true, true, 'walled'],
    ['on its way to the point it steps aside to', 1, false, true, 'far']]) {
    const at = site(g, middle), u = send(take(`a squad ${what}`, (u, def) => squad(def)), at, 0, 12);
    const lastLeg = legs === 1 && passing;
    if (lastLeg && !extra) u.path = [{ x: at.x + 4, z: at.z }, { x: at.x + 8, z: at.z }, { x: at.x + 12, z: at.z }];
    const by = send(take('  the unit', (u, def) => (lastLeg ? vehicle : squad)(def)), { x: at.x + (near ? 3 : 20), z: at.z + (lastLeg && !extra ? 6 : 2) });
    by.path = [];
    const goal = extra === 'far' || extra === 'late' ? { x: at.x + 1, z: at.z + 0.5 } : { x: at.x + 0.2, z: at.z + 0.1 };
    const legPoints = [goal, { x: at.x + 2, z: at.z + 2 }].slice(0, legs);
    u.traffic = { goal, ...(passing ? { legs: legPoints, passing: true } : {}), by: by.id, until: (g.tick + (extra === 'late' ? -1 : 40)) * TICK };
    if (extra === 'walled') for (let z = -3; z <= 3; z++) g.flags[cellAt(g, at.x + 4, at.z + z * CELL)] |= MOVE;
  }
  // squads under fire from the south, sheltering behind a wall, that have waited for a friendly vehicle retreating
  // at them: each steps aside to the first place with cover as good, and the first place it looks at (0.8 m back,
  // about 4 m to its left, lined up so its sight line south crosses a cell edge 2.5 m out) gets one kind of cover: a
  // wall at 2.9 m, too far for its worn state to shelter it; a house corner off to one side; a wreck at the edge of its
  // reach; a dug trench facing the fire; a wall at 2.9 m whose structure cell has failed (its letter's hit points
  // count); and open ground, with the trail it came along to go back to
  for (const kind of ['worn wall', 'house corner', 'wreck', 'trench', 'failed wall', 'trail']) {
    const site0 = site(g, middle);
    const u = take(`a squad under fire stepping aside to cover: ${kind}`, (u, def) => squad(def));
    // (later in the units' order, so the squad looks to its left first)
    const v = take('  the vehicle', (v, def) => vehicle(def) && v.id > u.id && v.owner === u.owner);
    const d = 0.8 * (UNITS[u.type].radius + UNITS[v.type].radius) + 0.7;
    const at = { x: site0.x, z: site0.z + ((1.5 - (site0.z + d) % 2) + 4) % 2 }, first = { x: at.x - 0.8, z: at.z + d };
    send(u, at, 0, 12);
    Object.assign(send(v, { x: at.x + 3.5, z: at.z }, Math.PI, 12), { retreating: true, vx: 0, vz: 0 });
    Object.assign(u, { trafficWait: 0.4, hitFrom: { x: at.x, z: at.z + 30 } });
    object({ x: at.x, z: at.z + 1.5 }, '#', COVER);
    if (kind === 'worn wall') object({ x: first.x, z: first.z + 2.9 }, '#', COVER);
    if (kind === 'house corner') object({ x: first.x + CELL, z: first.z + CELL }, 'B', MOVE | sim.SIGHT);
    if (kind === 'wreck') g.wrecks.push({ id: 0, type: 'tank', owner: 0, x: first.x, z: first.z + 3.2, rot: 0, c: -1, motion: null });
    if (kind === 'trench') g.trenchFront[object(first, '.', sim.TRENCH)] = Math.PI / 2;
    if (kind === 'failed wall') g.structuralCells.set(object({ x: first.x, z: first.z + 2.9 }, '#', COVER, 100), { state: 'failed', hp: 0, maxHp: 400 });
    if (kind === 'trail') { u.trafficTrail = [{ x: at.x - 6, z: at.z, rot: 0 }]; object({ x: at.x - 6, z: at.z }, '+', COVER); }
  }
  // a vehicle and a squad meeting head on, the squad later in the units' order but already inside the crossing: the
  // squad keeps its way, the vehicle steps aside
  {
    const at = site(g, middle), v = send(take('a vehicle meeting a squad head on', (u, def) => vehicle(def)), at, 0, 12);
    v.moveSpeed = 2;
    Object.assign(send(take('  the squad', (u, def) => squad(def) && u.id > v.id && u.owner === v.owner), { x: at.x + 5, z: at.z }, Math.PI, 12), { trafficWait: 0.4, trafficEntered: true });
  }
  // a retreating vehicle meeting an enemy squad head on that has waited 6.5 s (which raises its rank, but not to a
  // retreat's): the vehicle keeps its way
  {
    const at = site(g, middle), v = send(take('a retreating vehicle meeting an enemy squad that has waited', (u, def) => vehicle(def)), at, 0, 12);
    Object.assign(v, { moveSpeed: 2, retreating: true });
    Object.assign(send(take('  the squad', (u, def) => squad(def) && u.id > v.id && u.owner !== v.owner), { x: at.x + 5, z: at.z }, Math.PI, 12), { trafficWait: 6.5 });
  }
  // two friendly vehicles on one route, the one in front stuck: the one behind gives way instead of queueing
  {
    const at = site(g, middle), back = send(take('a vehicle behind a stuck friend', (u, def) => vehicle(def)), at, 0, 16);
    back.moveSpeed = 2;
    Object.assign(send(take('  the stuck friend', (u, def) => vehicle(def) && u.owner === back.owner), { x: at.x + 5, z: at.z }, 0, 11), { stuck: 1.5 });
  }
  // two friends in a line, the one in front (later in the units' order) still waiting on the one behind from last tick:
  // the one behind does not queue behind it (no ring of waiting units)
  {
    const at = site(g, middle), behind = send(take('a vehicle behind a friend that waits on it', (u, def) => vehicle(def)), at, 0, 16);
    behind.moveSpeed = 2;
    send(take('  the friend in front', (u, def) => vehicle(def) && u.owner === behind.owner && u.id > behind.id), { x: at.x + 5, z: at.z }, 0, 11).queuedBehind = behind.id;
  }
  // a one-lane stretch between two lines of tank traps, 12 m long and a little narrower than two of these hulls need to
  // pass (but wider than one hull and its margin): a vehicle 8 m short of it holds for an oncoming friend nearer its
  // middle, which drives on
  {
    const at = site(g, middle), a = send(take('a vehicle short of a one-lane stretch', (u, def) => vehicle(def) && def.radius >= 2.5), at, 0, 30);
    // tank traps m cells to either side of the route leave 2m - 1.5 m open on that side (the width is looked at in half
    // metres): the widest odd width below what two hulls need, which is at least one hull and margin for these hulls
    const need = 0.96 * UNITS[a.type].radius + 2 * bodyRadius(UNITS[a.type]) + 1.5, rows = Math.floor((need + 3 - 1e-9) / 2);
    for (let x = at.x + 8; x < at.x + 20; x += CELL) for (const [side, m] of [[1, Math.ceil(rows / 2)], [-1, Math.floor(rows / 2)]]) g.flags[cellAt(g, x, at.z + side * m * CELL)] |= VBLOCK;
    a.moveSpeed = 2;
    Object.assign(send(take('  the oncoming friend', (u, def) => vehicle(def) && u.owner === a.owner), { x: at.x + 26, z: at.z }, Math.PI, 30), { moveSpeed: 2 });
  }
  // a vehicle short of a waypoint where its route bends 60 degrees, at 3 m/s: further than it turns early at that pace
  // (a hull's turning lead), but not by much
  {
    const at = site(g, middle), v = take('a vehicle short of a bend', (u, def) => vehicle(def)), turn = movementProfile(v.type, UNITS[v.type]).hullTurn;
    const lead = 3.1 / turn * Math.tan(Math.PI / 6), far = lead + 0.35;
    send(v, at, 0, far).path.push({ x: at.x + far + 4, z: at.z + 7 });
    v.moveSpeed = 3;
  }
  // a vehicle stuck a while, driving into a friend that stands in its way: it passes through it (a ghost)
  {
    const at = site(g, middle), v = send(take('a stuck vehicle driving into a friend', (u, def) => vehicle(def)), at, 0, 12);
    Object.assign(v, { moveSpeed: 3, stuck: 3.5 });
    Object.assign(send(take('  the friend', (u, def) => squad(def) && u.owner === v.owner), { x: at.x + 2.5, z: at.z }), { path: [] });
  }
  // two friendly vehicles overlapping, one passing through friends: they are pushed apart gently, not as hulls
  {
    const at = site(g, middle), v = send(take('a vehicle passing through friends', (u, def) => vehicle(def)), at, 0, 12);
    v.ghost = g.tick + 40;
    Object.assign(send(take('  a friend overlapping it', (u, def) => vehicle(def) && u.owner === v.owner), { x: at.x + 1, z: at.z + 3 }), { path: [] });
  }
  // a vehicle that was backing out of a tight spot and has got there (its clearance ends), and ones stalled long
  // enough to ask for a new route (a vehicle with a route winding on past 40 m, and a squad)
  {
    const at = site(g, middle), v = send(take('a vehicle at the end of its clearance', (u, def) => vehicle(def)), at, 0, 12);
    v.motionClearance = { goal: { x: at.x + 0.1, z: at.z, rot: 0 }, reverse: true, path: v.path, until: g.tick + 80 };
    v.stuck = 0.5;
  }
  {
    const at = site(g, middle), v = send(take('a vehicle stalled on its route', (u, def) => vehicle(def)), at, 0, 12);
    Object.assign(v, { stuck: 1, repath: -1 });
    for (let i = 1; i <= 6; i++) v.path.push({ x: at.x + 12 + 9 * i, z: at.z + (i % 2 ? 6 : 0) });
    Object.assign(send(take('a squad stalled on its route', (u, def) => squad(def)), site(g, middle), 0, 12), { stuck: 1, repath: -1 });
  }
  // backing up to the end of a short move: a vehicle facing away from its waypoint and still rolling forward a little
  // (it brakes to a stop before backing), and a squad
  {
    const at = site(g, middle), v = send(take('a vehicle backing up', (u, def) => vehicle(def)), at, Math.PI - 0.1, 12);
    Object.assign(v, { moveSpeed: 0.1, rot: -0.1, reverse: copy(v.path.at(-1)) });
    const u = send(take('a squad backing up', (u, def) => squad(def)), site(g, middle), 0.4, 12);
    u.reverse = copy(u.path.at(-1));
  }
  // a squad that has not moved yet this tick crossing 5 m ahead of a vehicle that moves first: the vehicle guesses its
  // velocity from its route, sees they would meet, and waits for it (the squad is inside the crossing already)
  {
    const at = site(g, middle), v = send(take('a vehicle on its way', (u, def) => vehicle(def)), at, 0, 12);
    v.moveSpeed = 2;
    const u = take('  a squad with no velocity yet crossing ahead of it', (u, def) => squad(def) && u.id > v.id && u.owner === v.owner);
    // how close the two come within the next 0.7 s, as trafficStep works it out, with the squad standing or moving
    const min = bodyRadius(UNITS[v.type]) + bodyRadius(UNITS[u.type]) + 0.15, vx = UNITS[v.type].speed, vs = UNITS[u.type].speed;
    const closest = (side, moving) => {
      const rvx = -vx, rvz = moving ? -vs : 0, t = Math.max(0, Math.min(0.7, -(4 * rvx + side * rvz) / (rvx * rvx + rvz * rvz)));
      return Math.hypot(4 + rvx * t, side + rvz * t);
    };
    let side = 2;
    while (!(closest(side, false) > min + 0.3 && closest(side, true) < min - 0.3)) if ((side += 0.25) > 8) throw new Error('no crossing fits');
    send(u, { x: at.x + 4, z: at.z + side }, -Math.PI / 2, 10);
    Object.assign(u, { vx: undefined, vz: undefined, trafficEntered: true });
  }
  // a squad walking past one holding its position, too close: the holding squad does not move and the walking one is
  // not pushed off its way by it
  {
    const at = site(g, middle), a = send(take('a squad walking past one holding its position', (u, def) => squad(def)), at, 0, 12);
    const holder = take('  the squad holding its position', (u, def) => squad(def) && u.id > a.id && u.owner === a.owner);
    Object.assign(send(holder, { x: at.x + 0.5, z: at.z + 1.5 }), { path: [], holdPos: true });
  }
  // a squad 10 cm short of the end of its route: it gets there
  {
    const at = site(g, middle);
    send(take('a squad reaching the end of its route', (u, def) => squad(def)), at).path = [{ x: at.x + 0.1, z: at.z }];
  }
  // two friendly squads crossing a ford the same way, one a little ahead beside the other and too close: they keep
  // their forward step
  {
    const at = site(g, middle);
    paint(g, at, 2, FORD);
    const lead = send(take('a squad crossing a ford', (u, def) => squad(def)), at, 0, 12);
    send(take('  another beside it, the same way', (u, def) => squad(def) && u.owner === lead.owner), { x: at.x - 0.8, z: at.z + 1.0 }, 0, 12);
  }

  // in place: pace and sprint (a squad held to a slower pace, an idle one whose pace lapses, a conscript sprinting)
  take('a squad on its way at a set pace', (u, def) => squad(def) && moving(u) && !u.retreating).pace = 2;
  take('an idle squad whose pace lapses', (u, def) => squad(def) && idle(u)).pace = 3;
  take('a conscript sprinting', (u, def) => u.type === 'conscript' && moving(u) && !u.retreating).sprint = 3;
  // a retreat that has ended: no route and nowhere to go
  take('a retreat with nowhere left to go', (u, def) => squad(def) && idle(u)).retreating = true;
  // a squad riding a vehicle: placed on it every tick, and skipped by traffic and separation
  const carrier = take('a carrier', (u, def) => vehicle(def) && moving(u));
  const rider = take('a squad riding it', (u, def) => squad(def) && idle(u) && u.id > carrier.id);
  rider.riding = carrier.id; carrier.cargo.push(rider.id);
  // a squad beside one inside a building (here, its own cell), which comes after it in the units' order
  const neighbour = take('a squad', (u, def) => squad(def) && idle(u));
  const inside = take('a squad in a building beside it', (u, def) => squad(def) && idle(u) && u.id > neighbour.id);
  inside.garrison = cellAt(g, inside.x, inside.z);
  Object.assign(neighbour, { x: inside.x + 1, z: inside.z });
  // two squads on exactly one spot, and two walls overlapping
  const a = take('a squad', (u, def) => squad(def) && idle(u)), b = take('a squad on the same spot', (u, def) => squad(def) && idle(u) && u.owner === a.owner);
  Object.assign(b, { x: a.x, z: a.z });
  const wall = take('a wall', u => u.type === 'wall'), other = take('a wall overlapping it', u => u.type === 'wall');
  Object.assign(other, { x: wall.x + 0.5, z: wall.z });
  // a vehicle up against a wall that comes before it in the units' order: it is pushed off hard
  const wall2 = take('a wall', u => u.type === 'wall');
  Object.assign(send(take('  a vehicle against it', (u, def) => vehicle(def) && u.id > wall2.id), { x: wall2.x + 1.5, z: wall2.z }, 0, 12), { path: [] });
  // a unit on its way whose trail is full: the oldest place drops off
  const full = take('a squad on its way with a full trail', (u, def) => squad(def) && moving(u) && !u.retreating);
  full.trafficTrail = Array.from({ length: 20 }, (_, i) => ({ x: full.x - 30 + i, z: full.z - 30, rot: 0 }));
  // at the map's edge: a squad whose next step leaves the map, and a vehicle whose hull pokes over the edge
  Object.assign(send(take('a squad stepping off the map', (u, def) => squad(def)), { x: 0.1, z: 61 }, Math.PI), { moveSpeed: 0 });
  Object.assign(send(take('a vehicle over the edge', (u, def) => vehicle(def)), { x: 1.5, z: 71 }, 0), { moveSpeed: 1 });
}

// Clears what nothing in a few ticks reaches, to keep the file small: wear and structure cells more than 30 m from
// every unit (units move about a metre in three ticks; cover and lanes are looked for within about 35 m of a unit).
function trimFar(g, margin = 30) {
  const near = new Uint8Array(g.w * g.h), r = Math.ceil(margin / CELL);
  for (const u of g.units.values()) {
    const cx = Math.floor(u.x / CELL), cz = Math.floor(u.z / CELL);
    for (let z = Math.max(0, cz - r); z <= Math.min(g.h - 1, cz + r); z++) for (let x = Math.max(0, cx - r); x <= Math.min(g.w - 1, cx + r); x++) near[z * g.w + x] = 1;
  }
  for (let c = 0; c < near.length; c++) if (!near[c]) { g.wear[c] = 0; g.structuralCells.delete(c); }
}

if (import.meta.main) {
  if (process.arch !== 'x64') throw new Error(`run this with an x64 Node, not ${process.arch}: arm64 builds compute other last bits in V8's math`);
  const g = decode(JSON.parse(readFileSync(STATE, 'utf8')).game);
  for (const u of [...g.units.values()]) if (!AREAS.some(a => u.x >= a[0] && u.z >= a[1] && u.x < a[2] && u.z < a[3])) g.units.delete(u.id);
  cover(g);
  trimFar(g);
  const result = { state: STATE.split('/').slice(-2).join('/'), areas: AREAS, cases: CASES, ...await trace(sim, g, TICKS) };
  writeFileSync(OUT, JSON.stringify(result));
  const calls = result.steps.reduce((n, s) => n + s.events.filter(e => e[0] === 'move').length, 0);
  console.log(`wrote ${OUT}: ${result.steps.length} ticks, ${g.units.size} units, ${calls} movement calls, ${JSON.stringify(result).length} bytes`);
  for (const c of CASES) console.log('  ' + c);
}
