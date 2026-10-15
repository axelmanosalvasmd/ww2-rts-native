// Six selected predicates. Every simulation call uses the unchanged pinned Source.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync, writeFileSync} from 'node:fs';
import {join, resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {gzipSync, gunzipSync} from 'node:zlib';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? join(root, 'reference/ww2-rts'));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], {encoding: 'utf8'}).trim();
function clean() {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
}
clean();
const sim = await import(pathToFileURL(join(reference, 'shared/sim.js')));
const {rebuildGrid} = await import(pathToFileURL(join(reference, 'shared/grid.js')));
const view = new DataView(new ArrayBuffer(8));
const bits = value => {
  view.setFloat64(0, value, false);
  return view.getBigUint64(0, false).toString(16).padStart(16, '0');
};
const number = value => {
  view.setBigUint64(0, BigInt('0x' + value), false);
  return view.getFloat64(0, false);
};
const encode = value => value === undefined ? null : typeof value === 'number' ? {f64: bits(value)}
  : Array.isArray(value) ? value.map(encode) : value && typeof value === 'object'
    ? Object.fromEntries(Object.entries(value).map(([key, item]) => [key, encode(item)])) : value;
const point = value => value ? {x: bits(value.x), z: bits(value.z)} : null;
const paid = value => value ? {...point(value), kind: value.kind, cells: structuredClone(value.cells),
  t: bits(value.t), dir: bits(value.dir), front: value.front === undefined ? null : bits(value.front),
  n: value.n ?? null, reach: value.reach === undefined ? null : bits(value.reach), project: value.project?.id ?? null} : null;
function changes(before, after, path = [], output = []) {
  if (Object.is(before, after)) return output;
  if (Array.isArray(before) && Array.isArray(after) && before.length === after.length) {
    for (let i = 0; i < before.length; i++) changes(before[i], after[i], [...path, i], output);
    return output;
  }
  if (before && after && typeof before === 'object' && typeof after === 'object'
      && !Array.isArray(before) && !Array.isArray(after)) {
    for (const key of new Set([...Object.keys(before), ...Object.keys(after)])) {
      if (!Object.hasOwn(after, key)) output.push([[...path, key]]);
      else changes(before[key], after[key], [...path, key], output);
    }
    return output;
  }
  output.push([path, after]);
  return output;
}
// Same declared constructor input as the existing construction fixture. Never reset native streams.
const u64 = value => BigInt.asUintN(64, value), gamma = 0x9e3779b97f4a7c15n;
const mix = value => {
  value = u64((value ^ (value >> 30n)) * 0xbf58476d1ce4e5b9n);
  value = u64((value ^ (value >> 27n)) * 0x94d049bb133111ebn);
  return value ^ (value >> 31n);
};
const bootstrap = Number(mix(u64(mix(701n ^ u64(2n * gamma)) + gamma)) & 0xffffffffn);
const constructorDraw = bootstrap / 2 ** 32;
const originalRandom = Math.random;
const cases = [];
let totalSteps = 0, totalSnapshots = 0;

function record(name, map, tech, actions) {
  const draws = [];
  Math.random = () => { draws.push(constructorDraw); return constructorDraw; };
  assert.equal(sim.validateMap(map), null);
  const opts = {weather: false, tech, supply: false, logistics: false};
  const g = sim.createGame(map, ['Crew', 'Opposition'], false, [0, 1], [0, 1], opts);
  assert.equal(g.mode, undefined);
  assert.equal(sim.isConstructionMode(g), false);
  assert.equal(sim.isSkirmishBaseMode(g), true);
  assert.equal(draws.length, 1);
  Math.random = () => { throw Error(`${name}: unexpected runtime Math.random`); };
  const initialActorOrder = [...g.units.keys()];
  const owned = type => [...g.units.values()].find(unit => unit.owner === 0 && unit.type === type);
  const aliases = {worker: owned('rifle').id, hq: owned('hq').id, barracks: owned('barracks').id};
  for (const unit of g.units.values()) Object.assign(unit, {holdFire: true, holdPos: true, auto: false, autoRetreat: false});
  g.players[0].mp = 5000;
  g.pathRetryLimit = 1;
  g.cellJournal = [];
  rebuildGrid(g);
  sim.internals.updateVision(g);
  const setup = {map, names: ['Crew', 'Opposition'], shuffle: false, factions: [0, 1], teams: [0, 1], opts,
    initialActorOrder, spawn: [], initialAliases: structuredClone(aliases),
    prime: {allUnits: {holdFire: true, holdPos: true, auto: false, autoRetreat: false},
      player0: {mp: 5000}, pathRetryLimit: 1, cellJournal: []}, creationRandomDraws: 1, setupRandomDraws: 0};
  let previous = null, newAt = 0, journalAt = 0, index = 0;
  const patches = [], records = [], checkpoints = new Map();
  const sites=new Map();
  const snapshot=()=>{
   const units=[...g.units.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,hp:bits(u.hp),...point(u),rot:bits(u.rot),aim:bits(u.aim),built:u.built===undefined?null:bits(u.built),cells:u.cells??null,structureId:u.structureId??null,build:u.build??null,dig:paid(u.dig),entrench:u.entrench?.id??null,path:u.path.map(point),orders:encode(u.orders),follow:u.follow??null,garrison:u.garrison,riding:u.riding??null,holdFire:u.holdFire??null,holdPos:u.holdPos??null,auto:u.auto??null,autoRetreat:u.autoRetreat??null,retreating:u.retreating,repath:bits(u.repath),still:u.still===undefined?null:bits(u.still),moveSpeed:bits(u.moveSpeed),moveOutcome:u.moveOutcome??null,moveOutcomeTick:u.moveOutcomeTick??null,moveResult:encode(u.moveResult),queue:encode(u.queue),productionJobs:encode(u.productionJobs),prog:u.prog===undefined?null:bits(u.prog),cooldown:bits(u.cooldown)}));
   for(const u of g.units.values())if(u.cells)sites.set(u.id,u);
   const cells=new Set(g.cellLog.map(row=>row[0]));for(const u of sites.values())for(const c of u.cells)cells.add(c);
   const cellRows=[...cells].sort((a,b)=>a-b).map(c=>({c,ch:g.chars[c],ground:g.ground[c],object:g.objects[c],mine:g.mineLayer[c],flags:g.flags[c],height:g.height?.[c]??0,hpBits:new Uint32Array(g.cellHp.buffer)[c],wearBits:new Uint32Array(g.wear.buffer)[c],state:g.cellState[c],burnt:g.burnt[c],scarBits:new Uint32Array(g.scar.buffer)[c],frontBits:g.trenchFront?new Uint32Array(g.trenchFront.buffer)[c]:null}));
   const batch=g.newCells.slice(newAt),journal=g.cellJournal.slice(journalAt);newAt=g.newCells.length;journalAt=g.cellJournal.length;
   return {tick:g.tick,mode:encode(g.mode),constructionMode:sim.isConstructionMode(g),skirmishBaseMode:sim.isSkirmishBaseMode(g),actorOrder:[...g.units.keys()],aliases:structuredClone(aliases),units,sites:[...sites.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,live:g.units.get(u.id)===u,hp:bits(u.hp),built:bits(u.built),cells:u.cells,structureId:u.structureId,at:point(u),rot:bits(u.rot),cost:bits(sim.UNITS[u.type].cost),buildTime:bits(sim.UNITS[u.type].buildTime),maxHp:bits(sim.UNITS[u.type].hpPer)})),structures:[...g.structures.values()].map(s=>({id:s.id,kind:s.kind,entityId:s.entityId??null,sections:s.sections.map(t=>encode(t))})),structuralCellOrder:[...g.structuralCells.keys()],buildingCells:encode([...g.buildingCells]),players:g.players.map(p=>({slot:p.slot,team:p.team,mp:bits(p.mp),inc:bits(p.inc),vp:bits(p.vp),tier:p.tier??null,mun:p.mun===undefined?null:bits(p.mun),fuel:p.fuel===undefined?null:bits(p.fuel),out:p.out??false,away:p.away??false})),story:encode(g.story),timeline:encode(g.timeline),terrain:{cells:cellRows,newCells:encode(batch),newCellsTotal:g.newCells.length,journal,journalTotal:g.cellJournal.length,cellLog:encode(g.cellLog),versions:[g.terrainVersion??0,g.obstructionVersion??0,g.infantryRegionVersion??0,g.vehicleRegionVersion??0,g.navalRegionVersion??0],mines:[...g.mines],mineSeen:[...g.mineSeen]},pathStats:encode(g.pathStats),seed:g.seed>>>0,wind:{a:bits(g.wind.a),v:bits(g.wind.v)},weather:encode(g.weather),wx:encode(g.wx),nextId:g.nextId,winner:g.winner,shots:encode(g.shots),falling:encode(g.fallingSections),randomDraws:draws.length,population:g.players.map(p=>bits(sim.popOf(g,p.slot)))};
  };
  function save() {
    const state = snapshot();
    if (previous === null) checkpoints.set('initial', state);
    else patches.push(changes(previous, state));
    previous = state;
    index++;
    totalSnapshots++;
  }
  save();
  const initial = previous;
  for (const input of actions) {
    const action = structuredClone(input), beforeSeed = g.seed >>> 0;
    const beforeIds = new Set(g.units.keys());
    let result = null, count = 0;
    if (action.op === 'command') {
      if (action.cmd.ids) action.cmd.ids = action.cmd.ids.map(id => typeof id === 'string' ? aliases[id] : id);
      if (action.cmd.orders) action.cmd.orders = action.cmd.orders.map(([id, x, z]) => [typeof id === 'string' ? aliases[id] : id, x, z]);
      for (const key of ['id', 'from']) if (typeof action.cmd[key] === 'string') action.cmd[key] = aliases[action.cmd[key]];
      if (action.siteNear) {
        const worker = g.units.get(aliases.worker);
        const at = sim.siteNear(g, worker.x, worker.z, sim.UNITS[action.cmd.kind].size);
        assert.ok(at, 'original siteNear must find a real site');
        Object.assign(action.cmd, at);
      }
      result = sim.command(g, action.slot ?? 0, action.cmd) ?? null;
      assert.equal(result, action.expect ?? null, `${name}: ${action.label}`);
      if (action.as) {
        const born = [...g.units.values()].filter(unit => !beforeIds.has(unit.id));
        assert.equal(born.length, 1);
        aliases[action.as] = born[0].id;
        if (action.cmd.t === 'build') {
          const site = born[0], def = sim.UNITS[site.type];
          assert.equal(site.built, 0);
          assert.equal(site.hp, def.hpPer * 0.25);
          assert.equal(site.cells.length, def.size ** 2);
          assert.equal(g.structures.get(site.structureId).entityId, site.id);
          assert.ok(site.cells.every(cell => g.structuralCells.get(cell).entityId === site.id
            && g.buildingCells.get(cell).id === site.id));
        } else {
          assert.equal(action.cmd.t, 'buy');
          const maker = g.units.get(action.cmd.from);
          assert.ok(maker && maker.owner === 0 && maker.built === 1 && maker.hp > 0);
          assert.ok(sim.UNITS[maker.type].makes.includes(born[0].type));
          assert.ok(maker.cells.length && g.structures.get(maker.structureId).entityId === maker.id);
        }
      }
      assert.equal(g.seed >>> 0, beforeSeed, 'command cannot reset environment seed');
      save();
    } else if (action.op === 'prime') {
      for (const [who, values] of Object.entries(action.units ?? {})) {
        Object.assign(g.units.get(aliases[who]), structuredClone(values));
      }
      for (const [slot, values] of Object.entries(action.players ?? {})) {
        Object.assign(g.players[Number(slot)], structuredClone(values));
      }
      rebuildGrid(g);
      sim.internals.updateVision(g);
      assert.equal(g.seed >>> 0, beforeSeed);
      save();
    } else if (action.op === 'steps' || action.op === 'until') {
      const limit = action.op === 'steps' ? action.count : action.max;
      const done = () => g.units.get(aliases[action.target])?.built === 1;
      while (count < limit && (action.op === 'steps' || !done())) {
        sim.step(g);
        count++;
        totalSteps++;
        save();
      }
      if (action.op === 'until') assert.ok(done(), `${name}: construction completes within original bound`);
      else assert.equal(count, action.count);
    } else throw Error(`unrepresented action ${action.op}`);
    assert.equal(draws.length, 1);
    if (action.label) checkpoints.set(action.label, previous);
    records.push({action, result, steps: count, lastSnapshot: index - 1,
      randomDraws: 0, seedBefore: beforeSeed, seedAfter: g.seed >>> 0});
  }
  // Retain the constructor actors and their canonical building identities throughout every scene.
  for (const id of initialActorOrder) assert.ok(g.units.has(id), `${name}: retained constructor actor ${id}`);
  const at = label => checkpoints.get(label);
  const unit = (label, alias) => at(label).units.find(row => row.id === aliases[alias]);
  const mp = label => number(at(label).players[0].mp);
  const predicateNames = [];
  if (name === 'pillbox-facing-and-tier') {
    assert.equal(records.find(row => row.action.label === 'tier-refusal').result, 'tier', 'a pillbox needs Company HQ');
    assert.equal(records.find(row => row.action.label === 'placed').result, null);
    assert.ok(number(unit('placed', 'site').built) < 1, 'a construction site goes up');
    assert.ok(Math.abs(number(unit('placed', 'site').rot)) < 0.2, 'the slit faces out from home, toward the enemy side');
    assert.equal(mp('placed'), mp('tier-two') - sim.UNITS.pillbox.cost);
    predicateNames.push('base26_pillbox_slit_faces_out_from_home', 'base48_pillbox_needs_company_hq');
  } else if (name === 'b05-real-producer-completion-and-escape') {
    const maker = unit('purchased', 'hq'), born = unit('purchased', 'worker');
    assert.equal(born.type, 'rifle');
    assert.equal(born.owner, 0);
    assert.equal(mp('purchased'), mp('funded') - sim.UNITS.rifle.cost, 'actual HQ purchase pays exactly once');
    assert.ok(initialActorOrder.includes(maker.id));
    assert.equal(records.find(row => row.action.label === 'occupied').result, 'blocked');
    assert.equal(mp('occupied'), mp('ready'), 'refusal spends no construction MP');
    const builder = unit('occupied', 'worker');
    const cell = Math.floor(number(builder.z) / sim.CELL) * g.w + Math.floor(number(builder.x) / sim.CELL);
    assert.equal(at('occupied').terrain.cells.find(row => row.c === cell)?.ch
      ?? map.rows[Math.floor(cell / g.w)][cell % g.w], '.', 'refusal leaves the occupant on open ground');
    assert.ok(mp('occupied') >= mp('posed'));
    assert.equal(records.find(row => row.action.label === 'adjacent').result, null);
    assert.equal(unit('complete', 'site').built, bits(1), 'an ordinary adjacent builder completes construction');
    assert.equal(records.find(row => row.action.label === 'escape-order').result, null);
    const escaped = unit('escaped', 'worker');
    assert.ok(Math.hypot(number(escaped.x) - 81, number(escaped.z) - 31) < 0.5, 'the builder can leave the completed site');
    predicateNames.push('base28_b05_occupied_adjacent_complete_and_escape');
  } else if (name === 'conquest-motorpool-completion-and-repair') {
    assert.equal(records.find(row => row.action.label === 'placed').result, null, 'conquest rifle builds Motor Pool');
    assert.equal(records.find(row => row.action.label === 'unfinished-buy').result, 'needs');
    assert.equal(unit('complete', 'site').built, bits(1), 'conquest construction completes');
    assert.equal(records.find(row => row.action.label === 'tank-born').result, null);
    assert.equal(unit('tank-born', 'tank').type, 'tank');
    assert.equal(unit('tank-born', 'tank').owner, 0);
    assert.equal(mp('tank-born'), mp('complete') - sim.UNITS.tank.cost);
    assert.equal(records.find(row => row.action.label === 'assist').result, null);
    assert.equal(unit('damaged', 'site').hp, bits(500));
    assert.ok(number(unit('repaired', 'site').hp) > 500, 'builders repair');
    assert.equal(unit('repaired', 'tank').id, unit('tank-born', 'tank').id);
    predicateNames.push('base29_conquest_construction_completes_and_buys_from_real_motorpool', 'base30_conquest_builders_repair_after_real_tank_birth');
  } else if (name === 'mortar-and-motorpool-tier') {
    assert.equal(at('funded').players[0].tier, 1);
    assert.equal(records.find(row => row.action.label === 'mortar-refusal').result, 'tier', 'mortar needs Company HQ');
    assert.equal(records.find(row => row.action.label === 'motorpool-refusal').result, 'tier');
    for (const label of ['mortar-refusal', 'motorpool-refusal']) {
      assert.equal(mp(label), mp('funded'));
      assert.equal(at(label).nextId, at('funded').nextId);
      assert.deepEqual(at(label).actorOrder, at('funded').actorOrder);
      assert.deepEqual(at(label).population, at('funded').population);
      assert.deepEqual(unit(label, 'barracks').queue, unit('funded', 'barracks').queue);
    }
    predicateNames.push('base48_mortar_needs_company_hq');
  } else throw Error(name);
  cases.push({name, setup, initial, records, patches, snapshots: index,
    finalAliases: structuredClone(aliases), predicates: predicateNames,
    checkpoints: Object.fromEntries([...checkpoints].map(([label, state]) => [label, state.tick]))});
}

const prime = (label, units = {}, players = {}) => ({op: 'prime', label, units, players});
const command = (label, cmd, expect = null, extra = {}) => ({op: 'command', label, cmd, expect, ...extra});
const steps = (label, count) => ({op: 'steps', label, count});
const map80 = (thirdSpawn = false) => ({w: 80, h: 80, rows: Array(80).fill('.'.repeat(80)),
  spawns: [{x: 12, y: 40}, {x: 68, y: 40}, ...(thirdSpawn ? [{x: 40, y: 68}] : [])], points: [], defend: [0]});
const quiet = {holdFire: true, holdPos: true, auto: false, autoRetreat: false};
try {
  record('pillbox-facing-and-tier', map80(), true, [
    prime('funded', {}, {0: {mp: 10000}}),
    command('tier-refusal', {t: 'build', kind: 'pillbox', ids: ['worker'], x: 50, z: 80}, 'tier'),
    prime('tier-two', {}, {0: {tier: 2}}),
    command('placed', {t: 'build', kind: 'pillbox', ids: ['worker'], x: 50, z: 80}, null, {as: 'site'}),
  ]);
  record('b05-real-producer-completion-and-escape', {w: 64, h: 64, rows: Array(64).fill('.'.repeat(64)),
    spawns: [{x: 2, y: 2}, {x: 60, y: 60}], points: []}, false, [
    prime('funded', {}, {0: {mp: 10000}, 1: {mp: 10000}}),
    command('purchased', {t: 'buy', unit: 'rifle', from: 'hq'}, null, {as: 'worker'}),
    prime('posed', {worker: {...quiet, x: 31, z: 31, still: 5, path: [], orders: [], cooldown: 1e9}}),
    steps('ready', 1),
    command('occupied', {t: 'build', ids: ['worker'], kind: 'barracks', x: 31, z: 31}, 'blocked'),
    command('adjacent', {t: 'build', ids: ['worker'], kind: 'barracks', x: 35, z: 31}, null, {as: 'site'}),
    steps('complete', 700),
    command('escape-order', {t: 'move', orders: [['worker', 81, 31]]}),
    steps('escaped', 600),
  ]);
  record('conquest-motorpool-completion-and-repair', map80(true), false, [
    prime('funded', {}, {0: {mp: 10000}}),
    command('placed', {t: 'build', kind: 'motorpool', ids: ['worker']}, null, {as: 'site', siteNear: true}),
    command('unfinished-buy', {t: 'buy', unit: 'tank'}, 'needs'),
    {op: 'until', label: 'complete', target: 'site', condition: 'finished', max: 1600},
    command('tank-born', {t: 'buy', unit: 'tank', from: 'site'}, null, {as: 'tank'}),
    prime('damaged', {site: {hp: 500}, tank: quiet}),
    command('assist', {t: 'assist', ids: ['worker'], id: 'site'}),
    steps('repaired', 100),
  ]);
  record('mortar-and-motorpool-tier', map80(), true, [
    prime('funded', {}, {0: {mp: 10000}}),
    command('mortar-refusal', {t: 'buy', unit: 'mortar'}, 'tier'),
    command('motorpool-refusal', {t: 'build', kind: 'motorpool', ids: ['worker'], x: 200, z: 200}, 'tier'),
  ]);
  assert.equal(cases.length, 4);
  assert.equal(cases.reduce((count, item) => count + item.predicates.length, 0), 6);
  clean();
  const content = JSON.stringify({reference: pin, node: process.version, arch: process.arch,
    boundary: 'unmodified createGame + command + full step; selected ordinary Conquest predicates only',
    random: {nativeMatchSeed: 701, bootstrap, constructorDraw: bits(constructorDraw), runtimeMathRandomDraws: 0},
    cases, steps: totalSteps, snapshots: totalSnapshots}) + '\n';
  const target = join(root, 'rust/crates/simtest/tests/fixtures/base-construction-selected.json.gz');
  if (process.argv.includes('--check')) assert.equal(gunzipSync(readFileSync(target)).toString('utf8'), content);
  else writeFileSync(target, gzipSync(content, {level: 9}));
  console.error(`${process.argv.includes('--check') ? 'Verified' : 'Recorded'} 4 cases, 6 predicates, ${totalSteps} full steps, ${totalSnapshots} snapshots`);
} finally {
  Math.random = originalRandom;
  clean();
}
