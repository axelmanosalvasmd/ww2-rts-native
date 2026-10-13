// Record original findPath results for explicit recipient-owned terrain views.
// Use x64 Node 24.21.0. --check compares the committed fixture without writing it.
import { fileURLToPath, pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
if (process.arch !== 'x64' || process.version !== 'v24.21.0') throw Error('Use x64 Node v24.21.0');
const reference = resolve(process.env.WW2_REFERENCE ?? fileURLToPath(new URL('../reference/ww2-rts/', import.meta.url)));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
if (git(['rev-parse', 'HEAD']) !== pin) throw Error(`Reference HEAD must be ${pin}`);
if (git(['status', '--porcelain', '--untracked-files=no'])) throw Error('Reference has tracked changes');
const sim = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const vehicleMotion = await import(pathToFileURL(resolve(reference, 'shared/vehicle-motion.js')));
let seed = 0xface17;
function random() {
  seed = (seed + 0x6d2b79f5) >>> 0;
  let t = seed; t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}
const pick = n => Math.floor(random() * n);
function runs(values) {
  const out = [];
  for (const value of values) {
    if (out.length && out.at(-1)[0] === value) out.at(-1)[1]++;
    else out.push([value, 1]);
  }
  return out;
}
const bitsBuffer = new ArrayBuffer(8), bitsView = new DataView(bitsBuffer);
const bits = x => { bitsView.setFloat64(0, x); return bitsView.getBigUint64(0).toString(16).padStart(16, '0'); };
const pointBits = p => [bits(p.x), bits(p.z)];
const scenarios = [];
const profiles = ['rifle', 'engineer', 'sniper', 'mg', 'at', 'lcvp', 'gunboat', 'destroyer'];
function makeScenario(name, w, h, flags, height, { worldKnown = true, mines = [], fires = [], bunkers = [], wear = null, wetness = 0, roads = false } = {}) {
  const near = new Uint8Array(w * h);
  for (let c = 0; c < flags.length; c++) if ((flags[c] & (sim.MOVE | sim.SIGHT)) === (sim.MOVE | sim.SIGHT)) {
    const x = c % w;
    for (let dy = -w; dy <= w; dy += w) for (let dx = x > 0 ? -1 : 0; dx <= (x < w - 1 ? 1 : 0); dx++) {
      const n = c + dy + dx; if (n >= 0 && n < flags.length) near[n] = 1;
    }
  }
  const g = { w, h, flags: Uint16Array.from(flags), height: height && Int8Array.from(height),
    navigationObserved: true, worldKnown, worldNearWalls: near, wear: wear && Float32Array.from(wear), wx: { wet: wetness }, roads, players: [{ team: 0 }],
    infantryRegionVersion: 0, vehicleRegionVersion: 0, navalRegionVersion: 0, navigationKnowledgeVersion: 0,
    mineSeen: new Map(mines.map(c => [c, 1])), fires: new Map(fires.map(c => [c, 20])),
    units: new Map(bunkers.map((p, i) => [i + 1, { ...p, type: 'bunker', hp: 100 }])) };
  const data = { name, w, h, flags: runs(flags), height: height && runs(height), near: runs(near), worldKnown, mines, fires, bunkers, wear: wear && runs(Float32Array.from(wear)), wetness, roads, queries: [] };
  scenarios.push(data);
  return (type, from, to, limit = null, hazards = null, update = null) => {
    if (update) {
      if (update.replace) g.flags = Uint16Array.from(g.flags);
      for (const [c, value] of update.flags ?? []) g.flags[c] = value;
      for (const [c, value] of update.heights ?? []) g.height[c] = value;
      g.vehicleRegionVersion = update.version ?? g.vehicleRegionVersion;
      g.navigationKnowledgeVersion = update.version ?? g.navigationKnowledgeVersion;
      g.navigationChangedFromVersion = update.from;
      g.navigationChangedCells = update.changed && new Set(update.changed);
      if (update.wetness !== undefined) g.wx.wet = update.wetness;
      for (const [c, value] of update.wear ?? []) g.wear[c] = value;
      // Supply the current near-wall view too, just as an observed view would.
      near.fill(0);
      for (let c=0;c<g.flags.length;c++) if ((g.flags[c] & (sim.MOVE|sim.SIGHT)) === (sim.MOVE|sim.SIGHT)) {
        const x=c%w;
        for(let dy=-w;dy<=w;dy+=w) for(let dx=x>0?-1:0;dx<=(x<w-1?1:0);dx++) {
          const n=c+dy+dx;if(n>=0&&n<near.length)near[n]=1;
        }
      }
    }
    const selected = hazards ?? { mines, fires, bunkers };
    g.mineSeen = new Map(selected.mines.map(c => [c, 1]));
    g.fires = new Map(selected.fires.map(c => [c, 20]));
    g.units = new Map(selected.bunkers.map((p, i) => [i + 1, { ...p, type: 'bunker', hp: 100 }]));
    g.finePathLimit = limit ?? undefined;
    g.pathStats = { calls: 0, deferred: 0, failed: 0, dropped: 0, expansions: 0, regionRejected: 0 };
    const unit = { id: 1, type, owner: 0, ...from };
    const def = sim.UNITS[type];
    const view = vehicleMotion.isGroundVehicle(def) ? vehicleMotion.vehicleNavigationView(g, def, sim.blockOf(def)) : g;
    // The reference cached vehicle view only refreshes selected live fields.
    // Attach per-query instrumentation and budget to the exact search view.
    view.pathStats = g.pathStats;
    view.finePathLimit = g.finePathLimit;
    const path = sim.findPath(view, unit, to);
    data.queries.push({ type, from, to, limit, ...(hazards ? { hazards } : {}), ...(update ? { update } : {}), expected: {
      points: path.map(pointBits), routeEnd: pointBits(unit.routeEnd), expansions: g.pathStats.expansions,
      status: g.pathStats.deferred ? 'ExpansionLimit' : g.pathStats.failed ? 'Unreachable' : 'Found',
    } });
  };
}
const pt = (x, z) => ({ x, z });
for (const worldKnown of [false, true]) {
  const flags = Array(19 * 15).fill(0), heights = Array(flags.length).fill(0);
  for (let y = 1; y < 14; y++) if (y !== 5 && y !== 12) flags[y * 19 + 9] = sim.MOVE | sim.SIGHT;
  for (let x = 2; x < 9; x++) flags[6 * 19 + x] = sim.WIRE;
  for (let x = 4; x < 15; x++) flags[9 * 19 + x] = sim.MUD | sim.WOOD;
  for (let y = 3; y < 8; y++) for (let x = 13; x < 17; x++) heights[y * 19 + x] = 2;
  const query = makeScenario(`walls-wire-cliffs-${worldKnown}`, 19, 15, flags, heights, { worldKnown, mines: [79, 99, 118], fires: [83, 102, 140], bunkers: [pt(14, 18), pt(28, 23)] });
  for (let i = 0; i < 60; i++) query(profiles[i % 5], pt(random() * 38, random() * 30), pt(random() * 44 - 3, random() * 36 - 3), i % 11 === 0 ? pick(8) : null);
}
for (const [name, flags, height] of [
  ['directed-cliff', [0, 0, 0, 0], [0, 2, 2, 1]],
  ['blocked-start-and-goal', [1, 1, 0, 0, 0, 1, 1, 1, 1], null],
  ['fully-blocked', [1, 1, 1, 1], null],
  ['flat-small', [0, 0, 0, 0], null],
]) {
  const w = name === 'blocked-start-and-goal' ? 3 : 2, h = flags.length / w;
  const query = makeScenario(name, w, h, flags, height);
  for (const type of ['rifle', 'gunboat', 'destroyer']) for (const from of [pt(1, 1), pt(3, 3), pt(0.2, 1.7), pt(-1, 3)])
    for (const to of [pt(1, 1), pt(3, 3), pt(1.7, 0.2), pt(30, -10)]) query(type, from, to);
}
const fineBlocked = makeScenario('coarse-connected-mine-corners-no-fine-path', 2, 2, [0,0,0,0], [0,2,2,1], { mines: [1,2] });
fineBlocked('rifle', pt(3,3), pt(1,1));
const live = makeScenario('live-hazards-refresh-between-queries', 15, 9, Array(135).fill(0), Array(135).fill(0));
for (const hazards of [
  { mines: [], fires: [], bunkers: [] },
  { mines: [64,65,66,67], fires: [], bunkers: [] },
  { mines: [], fires: [64,65,66,67], bunkers: [] },
  { mines: [], fires: [], bunkers: [pt(14,9)] },
  { mines: [], fires: [], bunkers: [pt(14,3)] },
  { mines: [], fires: [], bunkers: [] },
]) live('rifle',pt(1,9),pt(29,9),null,hazards);
const ocean = Array(29 * 17).fill(sim.MOVE), oceanHeight = Array(ocean.length).fill(0);
for (let y = 2; y < 15; y++) if (y !== 8) for (let x = 12; x < 17; x++) ocean[y * 29 + x] = sim.LAND;
for (let x = 0; x < 29; x++) ocean[9 * 29 + x] |= sim.SHOAL;
const navy = makeScenario('naval-land-shoals', 29, 17, ocean, oceanHeight, { mines: [88, 117, 146], fires: [175], bunkers: [pt(31, 18)] });
for (let i = 0; i < 60; i++) navy(['gunboat', 'destroyer', 'lcvp'][i % 3], pt(random() * 58, random() * 34), pt(random() * 58, random() * 34), i % 13 === 0 ? 1 : null);
for (let m = 0; m < 16; m++) {
  const w = 8 + pick(17), h = 8 + pick(17), n = w*h;
  const flags = Array.from({length:n}, () => random() < .18 ? [1,3,32,256,512][pick(5)] : 0);
  const heights = m % 4 ? Array.from({length:n}, () => random() < .1 ? pick(4)-1 : 0) : null;
  const query = makeScenario(`seeded-${m}`, w,h,flags,heights,{worldKnown:m%2===0});
  for(let i=0;i<25;i++) query(profiles[i%5],pt(random()*w*2,random()*h*2),pt(random()*w*2,random()*h*2),i%7===0?pick(12):null);
}
// Use the reference map loader for all authored maps. Only terrain and explicit
// recipient-local hazards cross into the test views passed to findPath.
for (const filename of readdirSync(resolve(reference, 'maps')).filter(f => f.endsWith('.json')).sort()) {
  const map = JSON.parse(readFileSync(resolve(reference, 'maps', filename), 'utf8'));
  const saved = Math.random; Math.random = random;
  let game;
  try { game = sim.createGame(map, ['One', 'Two'], false, [0, 1], [0, 1], { mode: 'conquest', weather: false }); }
  finally { Math.random = saved; }
  const query = makeScenario(`map:${filename}`, game.w, game.h, game.flags, game.height, { worldKnown: filename.length%2===0 });
  for (let i=0;i<12;i++) {
    const type = profiles[i%profiles.length]; const block = sim.blockOf(sim.UNITS[type]);
    const free = Array.from(game.flags.keys()).filter(c => !(game.flags[c]&block));
    const a=free.length?free[pick(free.length)]:0, b=free.length?free[pick(free.length)]:game.flags.length-1;
    query(type, pt((a%game.w+.5)*2,(Math.floor(a/game.w)+.5)*2),pt((b%game.w+.5)*2,(Math.floor(b/game.w)+.5)*2),i===0?0:null);
  }
}

// Record every road-vehicle definition, including the private motion fallback.
// Aircraft, trains and structures are deliberately outside this search slice.
const vehicles = Object.keys(sim.UNITS).filter(type => {
  const d=sim.UNITS[type]; return !d.infantry && !d.naval && !d.air && !d.rail && !d.structure;
});
const allProfiles = [...profiles, ...vehicles];
for (const worldKnown of [true, false]) {
  const w=43,h=27,n=w*h, flags=Array(n).fill(0), heights=Array(n).fill(0), wear=Array(n).fill(0);
  // A road detour competes with slow mud and forest. A bridge spans the water.
  for(let y=2;y<25;y++) for(let x=17;x<22;x++) flags[y*w+x]=sim.MOVE;
  for(let x=1;x<42;x++) for(let y=5;y<=9;y++) flags[y*w+x]=sim.ROAD;
  for(let y=13;y<25;y++) for(let x=4;x<16;x++) flags[y*w+x]=sim.MUD;
  for(let y=13;y<25;y++) for(let x=24;x<38;x++) flags[y*w+x]=sim.WOOD;
  for(let y=17;y<22;y++) for(let x=7;x<12;x++) {flags[y*w+x]|=sim.FORD;heights[y*w+x]=-1;}
  for(let y=12;y<19;y++) for(let x=27;x<31;x++) wear[y*w+x]=.65;
  for(let x=5;x<15;x++) wear[7*w+x]=.65;
  for(let x=7;x<13;x++) wear[22*w+x]=.4;
  const query=makeScenario(`vehicles-road-bridge-costs-${worldKnown}`,w,h,flags,heights,{worldKnown,wear,wetness:.7,roads:true,mines:[19*w+12],fires:[18*w+26],bunkers:[pt(50,20)]});
  for(const type of vehicles) {
    query(type,pt(9,43),pt(77,43));
    query(type,pt(5,15),pt(80.7,15.1));
    query(type,pt(39,15),pt(45.1,19.1));
    query(type,pt(1,1),pt(84,52),0);
  }
}
for(const gap of [1,2,3,5]) {
  const w=25,h=21,flags=Array(w*h).fill(0), heights=Array(w*h).fill(0);
  for(let y=0;y<h;y++) if(y<10-Math.floor(gap/2)||y>=10-Math.floor(gap/2)+gap) flags[y*w+12]=sim.MOVE|sim.SIGHT;
  for(let y=2;y<8;y++)for(let x=3;x<9;x++)heights[y*w+x]=2;
  const query=makeScenario(`vehicles-narrow-gap-${gap}`,w,h,flags,heights);
  for(const type of vehicles) {
    query(type,pt(11,21),pt(39,21));
    query(type,pt(23.7,5),pt(31,8));
    query(type,pt(7,7),pt(35,29));
  }
}
const changedWidth=31,changedHeight=19,changedFlags=Array(changedWidth*changedHeight).fill(0);
const updates=makeScenario('vehicles-changing-terrain',changedWidth,changedHeight,changedFlags,Array(changedFlags.length).fill(0),{wear:Array(changedFlags.length).fill(0),roads:true});
let version=0;
for(const type of vehicles) {
  updates(type,pt(9,19),pt(51,19));
  const wall=Array.from({length:changedHeight},(_,y)=>[y*changedWidth+15,sim.MOVE|sim.SIGHT]);
  updates(type,pt(9,19),pt(51,19),null,null,{flags:wall,from:version,version:++version,changed:wall.map(p=>p[0])});
  const opening=[8,9,10].map(y=>[y*changedWidth+15,sim.ROAD]);
  updates(type,pt(9,19),pt(51,19),null,null,{flags:opening,from:version,version:++version,changed:opening.map(p=>p[0])});
  updates(type,pt(9,19),pt(51,19),null,null,{wetness:.8,wear:[[9*changedWidth+14,.9]]});
  updates(type,pt(9,19),pt(51,19),null,null,{flags:wall.map(([c])=>[c,0]),version:++version});
  updates(type,pt(9,19),pt(51,19),null,null,{replace:true,flags:wall,version:++version});
  updates(type,pt(9,19),pt(51,19),null,null,{flags:wall.map(([c])=>[c,0]),version:++version});
  updates(type,pt(9,19),pt(51,19),null,null,{replace:true,flags:wall,version});
  updates(type,pt(9,19),pt(51,19),null,null,{replace:true,flags:wall.map(([c])=>[c,0]),version});
}

// Change distant chunks in reverse spatial order to preserve the original
// insertion order through expanded footprints and coarse region updates.
const wideW=91,wideH=73,wideFlags=Array(wideW*wideH).fill(0);
for(let y=0;y<wideH;y++) if(y<29||y>38)wideFlags[y*wideW+45]=sim.MOVE|sim.SIGHT;
const wide=makeScenario('vehicles-unsorted-distant-changes',wideW,wideH,wideFlags,Array(wideFlags.length).fill(0));
let wideVersion=0;
for(const type of ['tank','lorry']) {
  wide(type,pt(11,19),pt(161,121));
  const cells=[61*wideW+76,45*wideW+62,29*wideW+46,13*wideW+14];
  wide(type,pt(11,19),pt(161,121),null,null,{flags:cells.map(c=>[c,sim.MOVE|sim.SIGHT]),from:wideVersion,version:++wideVersion,changed:cells});
  wide(type,pt(11,19),pt(161,121),null,null,{flags:cells.map(c=>[c,0]),from:wideVersion,version:++wideVersion,changed:cells});
}

const output = JSON.stringify({ reference: pin, node: process.version, arch: process.arch, seed: '0xface17',
  rules: { wallHug: sim.CFG.wallHug, bunkerRadius: sim.UNITS.bunker.radius, roadSpeed:sim.CFG.roadSpeed, fordSpeed:sim.CFG.fordSpeed,
    mudSpeed:sim.CFG.mudSpeed, woodSpeed:sim.CFG.wood.speed, churn:sim.CFG.churn, wetFord:sim.CFG.weather.wetFord, wetGround:sim.CFG.weather.wetGround },
  profiles: Object.fromEntries(allProfiles.map(type => { const d=sim.UNITS[type];return [type,{infantry:!!d.infantry,naval:!!d.naval,deep:!!d.deep,radius:d.radius,speed:d.speed}]; })),scenarios })+'\n';
if (process.argv.includes('--check')) {
  const expected = readFileSync(new URL('../rust/crates/simtest/tests/fixtures/pathfinding.json', import.meta.url), 'utf8');
  if (output !== expected) throw Error('Pathfinding fixture differs');
  console.log('Pathfinding fixture matches');
} else process.stdout.write(output);
