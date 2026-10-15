// Original MOV evidence only. No Source execution occurs when this module is imported.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {closeSync, createReadStream, openSync, readSync, renameSync, unlinkSync, writeSync} from 'node:fs';
import {pipeline} from 'node:stream/promises';
import {StringDecoder} from 'node:string_decoder';
import {createGunzip, gzipSync} from 'node:zlib';

export const format = 'mov-original-jsonl-gzip-v1';

// Each line is one complete gzip member. Compression boundaries follow record
// boundaries, never asynchronous stream chunking or wall time.
export function captureWriter(path) {
  const fd = openSync(path, 'wx');
  let sequence = 0, closed = false;
  return {
    write(record) {
      assert.equal(closed, false);
      const bytes = gzipSync(JSON.stringify({sequence: sequence++, ...record}) + '\n', {level: 9});
      for (let offset = 0; offset < bytes.length;) {
        const written = writeSync(fd, bytes, offset, bytes.length - offset);
        assert.ok(written > 0, 'capture write advances');
        offset += written;
      }
    },
    close() { if (!closed) { closed = true; closeSync(fd); } },
  };
}

function equalFiles(left, right) {
  const a = openSync(left, 'r');
  let b;
  try {
    b = openSync(right, 'r');
    const x = Buffer.alloc(65536), y = Buffer.alloc(65536);
    // Positional reads compare equal offsets even if a filesystem read is short.
    let position = 0;
    while (true) {
      const n = readSync(a, x, 0, x.length, position);
      let m = 0;
      while (m < n) {
        const count = readSync(b, y, m, n - m, position + m);
        if (count === 0) return false;
        m += count;
      }
      if (!x.subarray(0, n).equals(y.subarray(0, m))) return false;
      if (n === 0) return readSync(b, y, 0, 1, position) === 0;
      position += n;
    }
  } finally { closeSync(a); if (b !== undefined) closeSync(b); }
}

export function finishCapture(partial, output, checking, failed) {
  if (failed) renameSync(partial, checking ? `${output}.failed.gz` : output);
  else if (!checking) renameSync(partial, output);
  else {
    if (equalFiles(partial, output)) unlinkSync(partial);
    else {
      renameSync(partial, `${output}.different.gz`);
      assert.fail('original full callback recording is reproducible');
    }
  }
}

// Shapes follow state-codec.mjs and the pinned producer, not decoded defaults.
function object(value) {
  assert.ok(value !== null && typeof value === 'object' && !Array.isArray(value), 'record object required');
}
function own(value, fields) {
  object(value);
  for (const key of fields.split(' ')) assert.ok(Object.hasOwn(value, key), `required own field: ${key}`);
}
function integer(value, minimum = 0) { assert.ok(Number.isSafeInteger(value) && !Object.is(value, -0) && value >= minimum, 'integer identity/count required'); }
function text(value) { assert.equal(typeof value, 'string'); }
function array(value) { assert.ok(Array.isArray(value), 'encoded array required'); }
function marker(value, key) {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    && Object.keys(value).length === 1 && Object.hasOwn(value, key);
}
function undefinedValue(value) { return marker(value, '$undefined') && value.$undefined === true; }
function number(value) {
  assert.ok((typeof value === 'number' && Number.isFinite(value) && !Object.is(value, -0))
    || (marker(value, '$num') && ['-0', 'Infinity', '-Infinity', 'NaN'].includes(value.$num)), 'encoded number required');
}
const typedKinds = ['Float32Array', 'Float64Array', 'Int8Array', 'Uint8Array', 'Int16Array', 'Uint16Array', 'Int32Array', 'Uint32Array'];
const integerRanges = {Int8Array: [-128, 127], Uint8Array: [0, 255], Int16Array: [-32768, 32767],
  Uint16Array: [0, 65535], Int32Array: [-2147483648, 2147483647], Uint32Array: [0, 4294967295]};
function typedLength(value, kind) {
  assert.ok(marker(value, `$${kind}`), `encoded ${kind} required`);
  const raw = value[`$${kind}`];
  const itemShape = item => {
    number(item);
    if (Object.hasOwn(integerRanges, kind)) {
      const [minimum, maximum] = integerRanges[kind];
      assert.ok(Number.isInteger(item) && item >= minimum && item <= maximum, 'integer typed-array element');
    }
  };
  if (Array.isArray(raw)) { for (const item of raw) itemShape(item); return raw.length; }
  assert.ok(marker(raw, 'runs'));
  array(raw.runs); assert.equal(raw.runs.length % 2, 0);
  let length = 0;
  for (let i = 0; i < raw.runs.length; i += 2) {
    itemShape(raw.runs[i]); integer(raw.runs[i + 1], 1);
    length += raw.runs[i + 1]; integer(length);
  }
  return length;
}
function encoded(value) {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return;
  if (typeof value === 'number') { number(value); return; }
  if (Array.isArray(value)) { for (const item of value) encoded(item); return; }
  object(value);
  if (Object.hasOwn(value, '$num')) { number(value); return; }
  if (Object.hasOwn(value, '$undefined')) { assert.ok(undefinedValue(value)); return; }
  if (Object.hasOwn(value, '$map')) {
    assert.ok(marker(value, '$map')); array(value.$map);
    for (const entry of value.$map) { array(entry); assert.equal(entry.length, 2); encoded(entry[0]); encoded(entry[1]); }
    return;
  }
  if (Object.hasOwn(value, '$set')) {
    assert.ok(marker(value, '$set')); array(value.$set);
    for (const item of value.$set) encoded(item);
    return;
  }
  for (const kind of typedKinds) if (Object.hasOwn(value, `$${kind}`)) { typedLength(value, kind); return; }
  for (const [key, item] of Object.entries(value)) {
    assert.ok(!key.startsWith('$'), 'unknown codec marker');
    encoded(item);
  }
}
function errorShape(value) {
  own(value, 'name message stack');
  text(value.name); text(value.message); text(value.stack);
}
function point(value) { own(value, 'x z'); number(value.x); number(value.z); }
function weatherAndWind(value) {
  own(value.wind, 'a v'); number(value.wind.a); number(value.wind.v);
  if (value.wx !== null) {
    own(value.wx, 'rain raining wet next');
    number(value.wx.rain); number(value.wx.wet); number(value.wx.next);
    assert.equal(typeof value.wx.raining, 'boolean');
  }
}
// sim.js:1776-1807 declares these on every spawned actor, including buildings.
const unitFields = 'id type owner x z rot aim moveSpeed vx vz travelDir hp supp path orders attackId targetId cooldown still retarget repath stuck cd buff ap nade retreating reinf dig garrison enter board amove face xp fireAt sprint riding cargo hitBy hitAt hitFrom drift reverse react auto autoRetreat motionBefore motionClearance traffic trafficWait trafficTrail trafficEntered queuedBehind laneScan follow routeEnd moveOutcome moveOutcomeTick moveResult worldGoal pace ghost build entrench garrisonEntry holdFire holdPos guardHome shotAt lastHit deathImpulse hpWas woundAt healAt attackPick atHit aaShot aaReserve dust cells built queue productionJobs prog rally structureId region gateT open logistics convoy supplyStarted supplyEscape supplyHold supplyRetry supplyRouteReason air';
function unitShape(unit) {
  own(unit, unitFields); integer(unit.id, 1); integer(unit.owner, -1); text(unit.type);
  for (const key of ['x', 'z', 'rot', 'aim', 'moveSpeed', 'vx', 'vz', 'hp', 'supp']) number(unit[key]);
  array(unit.path); array(unit.orders); array(unit.cargo);
}
function characters(value, cells) {
  array(value); assert.equal(value.length, cells);
  for (const ch of value) { text(ch); assert.equal(ch.length, 1); }
}
function regions(value, world) {
  if (!world) { assert.ok(undefinedValue(value), 'absent World regions use the codec undefined marker'); return; }
  array(value);
  for (const region of value) {
    own(region, 'id kind x z bounds team progress capper'); text(region.kind);
    integer(region.id); integer(region.team, -1); integer(region.capper, -1);
    number(region.x); number(region.z); number(region.progress);
    array(region.bounds); assert.equal(region.bounds.length, 4);
    for (const bound of region.bounds) number(bound);
  }
}
function witnessShape(value, game) {
  own(value, 'tick seed wx wind units players motionBits chars flags height wear regions winner');
  integer(value.tick); integer(value.seed, -2147483648); weatherAndWind(value);
  array(value.units); array(value.players); array(value.motionBits);
  assert.equal(value.players.length, game.players);
  for (const [index, player] of value.players.entries()) {
    own(player, 'slot team spawn mp mun out');
    assert.equal(player.slot, index); integer(player.team); point(player.spawn); number(player.mp);
    if (!undefinedValue(player.mun)) number(player.mun);
    assert.ok(undefinedValue(player.out) || typeof player.out === 'boolean');
  }
  const ids = new Set();
  assert.equal(value.motionBits.length, value.units.length);
  for (const [index, unit] of value.units.entries()) {
    unitShape(unit); assert.ok(!ids.has(unit.id)); ids.add(unit.id);
    const bits = value.motionBits[index]; array(bits); assert.equal(bits.length, 5); assert.equal(bits[0], unit.id);
    for (const word of bits.slice(1)) assert.ok(typeof word === 'string' && /^[0-9a-f]{16}$/.test(word));
  }
  characters(value.chars, game.cells);
  assert.equal(typedLength(value.flags, 'Uint16Array'), game.cells);
  assert.equal(typedLength(value.wear, 'Float32Array'), game.cells);
  if (value.height !== null) assert.equal(typedLength(value.height, 'Int8Array'), game.cells);
  regions(value.regions, game.world);
  if (value.winner !== null) integer(value.winner, -1);
}
// sim.js:709-824, world-layers.js:53-66 and setupStructures declare these
// initial own fields. Additional conditional state remains present and validated.
const initialFields = 'w h flags tick nextId winVp units shots flights nextFlight nades salvos smokes strikes wrecks winner cellLog newCells claims visionTick fog fogTerrain fogSources timeline story fallen pathStats navalRegionVersion worldFootprintRadius ghosts nextProductionJob projectSeq projects covers convoys operatives pits tough supplyGraph supplyNet trenchFront shoal scenario world players points logisticsEnabled chars ground objects mineLayer materials baseMaterials objectMaterials mineMaterials roads naval mines mineSeen aid supply sandbox house cellHp surfaceWear wear burnt cellState fires scar soak seed wind wx height initialTerrain structures structuralCells fallingSections worldContacts tech army weather';
function initialShape(value, args) {
  own(value, initialFields); integer(value.w, 1); integer(value.h, 1);
  const cells = value.w * value.h; integer(cells, 1);
  array(args); assert.ok(args.length >= 2 && args.length <= 6);
  own(args[0], 'w h rows spawns points');
  assert.equal(value.w, args[0].w); assert.equal(value.h, args[0].h);
  array(args[0].rows); array(args[0].spawns); array(args[0].points); array(args[1]);
  assert.equal(args[0].rows.length, value.h);
  for (const row of args[0].rows) { text(row); assert.equal(row.length, value.w); }
  for (const spawn of args[0].spawns) { own(spawn, 'x y'); number(spawn.x); number(spawn.y); }
  for (const name of args[1]) text(name);
  // An explicit undefined array argument becomes null under the unchanged codec.
  if (args.length > 2 && args[2] !== null) assert.equal(typeof args[2], 'boolean');
  for (const index of [3, 4]) if (args.length > index && args[index] !== null) {
    array(args[index]); for (const id of args[index]) integer(id);
  }
  if (args.length > 5 && args[5] !== null) object(args[5]);
  array(value.players); assert.equal(value.players.length, args[1].length);
  for (const [index, player] of value.players.entries()) {
    own(player, 'slot name team faction vp mp inc sup spawn visible');
    assert.equal(player.slot, index); text(player.name); integer(player.team); integer(player.faction); point(player.spawn);
    number(player.vp); number(player.mp); number(player.inc); object(player.sup);
    assert.ok(marker(player.visible, '$set'));
  }
  const world = !undefinedValue(value.world);
  assert.equal(world, args[5]?.mode === 'world');
  if (world) {
    own(value.world, 'size total seed regionMap biomes regionCells memory regions');
    own(value.mode, 'kind teams total'); assert.equal(value.mode.kind, 'world');
    own(value, 'cellJournal journalBase nodes buildingCells');
    regions(value.world.regions, true);
  }
  assert.ok(marker(value.units, '$map'));
  const ids = new Set();
  for (const entry of value.units.$map) {
    unitShape(entry[1]); assert.equal(entry[0], entry[1].id);
    assert.ok(!ids.has(entry[0])); ids.add(entry[0]);
    if (Array.isArray(entry[1].cells)) { own(value, 'buildingCells'); assert.ok(marker(value.buildingCells, '$map')); }
  }
  characters(value.chars, cells); characters(value.ground, cells); characters(value.objects, cells);
  for (const [key, kind] of [['flags', 'Uint16Array'], ['mineLayer', 'Uint8Array'], ['cellHp', 'Float32Array'],
    ['surfaceWear', 'Float32Array'], ['wear', 'Float32Array'], ['burnt', 'Uint8Array'], ['cellState', 'Uint8Array'], ['scar', 'Float32Array']]) {
    assert.equal(typedLength(value[key], kind), cells);
  }
  if (value.height !== null) assert.equal(typedLength(value.height, 'Int8Array'), cells);
  own(value.initialTerrain, 'chars ground objects mineLayer height trenchFront');
  for (const key of ['chars', 'ground', 'objects']) characters(value.initialTerrain[key], cells);
  for (const key of ['mineLayer', 'height', 'trenchFront']) {
    if (value.initialTerrain[key] === null && key !== 'mineLayer') continue;
    array(value.initialTerrain[key]); assert.equal(value.initialTerrain[key].length, cells);
    for (const item of value.initialTerrain[key]) number(item);
  }
  for (const key of ['flights', 'mines', 'mineSeen', 'aid', 'materials', 'baseMaterials', 'objectMaterials', 'mineMaterials', 'fires', 'structures', 'structuralCells']) assert.ok(marker(value[key], '$map'));
  assert.ok(marker(value.soak, '$set')); assert.ok(marker(value.worldContacts, '$set'));
  for (const key of ['shots', 'nades', 'salvos', 'smokes', 'strikes', 'wrecks', 'cellLog', 'newCells', 'points', 'fallingSections']) array(value[key]);
  for (const key of ['logisticsEnabled', 'roads', 'naval', 'supply', 'sandbox', 'tech']) assert.equal(typeof value[key], 'boolean');
  own(value.weather, 'now next at'); text(value.weather.now); number(value.weather.at);
  if (value.weather.next !== null) text(value.weather.next);
  object(value.army); number(value.winVp);
  integer(value.tick); integer(value.nextId, 1); integer(value.nextFlight, 1); number(value.seed); weatherAndWind(value);
  if (value.winner !== null) integer(value.winner, -1);
  return {cells, players: value.players.length, world};
}
function actionInput(op, value) {
  switch (op) {
    case 'step': assert.equal(value, null); break;
    case 'findPath': own(value, 'from to'); point(value.from); point(value.to); break;
    case 'snapshotFor': array(value); assert.ok(value.length >= 1); integer(value[0], -1); break;
    case 'fixtureCommand':
    case 'command': own(value, 'owner cmd'); integer(value.owner, -1); own(value.cmd, 't'); text(value.cmd.t); break;
    default: assert.fail(`invalid observed operation: ${op}`);
  }
}

// Validate every record and hash the entire compressed file. Retain only case
// metadata and unresolved action IDs, never decoded witnesses or action history.
export async function readOriginalCapture(path) {
  const digest = createHash('sha256');
  const decoder = new StringDecoder('utf8');
  const input = createReadStream(path);
  input.on('data', bytes => digest.update(bytes));
  let sequence = 0, header, footer, active, nextCall = 0;
  const cases = [], open = new Map(), games = new Map();
  function record(line) {
    const row = JSON.parse(line);
    own(row, 'sequence type'); encoded(row); integer(row.sequence); text(row.type);
    assert.equal(row.sequence, sequence++);
    assert.equal(footer, undefined, 'no records after footer');
    if (!header) {
      own(row, 'format caseCount packet');
      own(row.packet, 'reference node arch scope globalRandom sourceHashes');
      for (const key of ['reference', 'node', 'arch', 'scope', 'globalRandom']) text(row.packet[key]);
      own(row.packet.sourceHashes, 'test.js test-world-rules.mjs test-fixtures.js');
      for (const key of ['test.js', 'test-world-rules.mjs', 'test-fixtures.js']) assert.ok(/^[0-9a-f]{64}$/.test(row.packet.sourceHashes[key]));
      assert.equal(row.type, 'header');
      assert.equal(row.format, format, 'explicit original capture format required');
      header = row.packet;
      assert.equal(row.caseCount, 5);
      return;
    }
    if (row.type !== 'footer') { own(row, 'case'); integer(row.case); }
    switch (row.type) {
      case 'case-start':
        own(row, 'spec'); own(row.spec, 'name file helpers body sha256 completed');
        for (const key of ['name', 'file', 'helpers', 'body', 'sha256']) text(row.spec[key]);
        assert.equal(active, undefined);
        assert.ok(cases.length < 5, 'only the five selected callbacks');
        assert.equal(row.case, cases.length);
        assert.equal(row.spec.completed, false);
        assert.equal(row.spec.sha256, createHash('sha256').update(row.spec.helpers + '\n' + row.spec.body).digest('hex'));
        active = {...row.spec};
        cases.push(active); nextCall = 0; games.clear();
        break;
      case 'create':
        own(row, 'call event'); integer(row.call);
        own(row.event, 'op game input initial'); integer(row.event.game, 1);
        array(row.event.input);
        assert.ok(active);
        assert.equal(row.case, cases.length - 1);
        assert.equal(row.call, nextCall++);
        assert.equal(row.event.op, 'createGame');
        assert.equal(row.event.game, games.size + 1, 'case-local constructor game identity');
        games.set(row.event.game, initialShape(row.event.initial, row.event.input));
        break;
      case 'action-start':
        own(row, 'call event'); integer(row.call);
        own(row.event, 'op game input before'); integer(row.event.game, 1);
        actionInput(row.event.op, row.event.input);
        assert.ok(games.has(row.event.game), 'action game was created in this case');
        assert.ok(active);
        assert.equal(row.case, cases.length - 1);
        assert.equal(row.call, nextCall++);
        witnessShape(row.event.before, games.get(row.event.game));
        open.set(row.call, {op: row.event.op, game: row.event.game});
        break;
      case 'action-end':
        own(row, 'call op game outcome event'); integer(row.call); integer(row.game, 1);
        own(row.event, 'after');
        assert.ok(games.has(row.game), 'action game was created in this case');
        assert.ok(open.has(row.call), 'action end has a begun call ID');
        assert.ok(active);
        assert.equal(row.case, cases.length - 1);
        assert.deepEqual(open.get(row.call), {op: row.op, game: row.game});
        witnessShape(row.event.after, games.get(row.game));
        assert.ok(row.outcome === 'return' || row.outcome === 'throw');
        assert.equal(Object.hasOwn(row.event, 'failure'), row.outcome === 'throw');
        if (row.outcome === 'throw') {
          own(row.event, 'failure'); errorShape(row.event.failure);
          assert.equal(Object.hasOwn(row.event, 'result'), false);
        } else if (row.op === 'findPath') {
          own(row.event, 'result'); array(row.event.result);
        } else if (row.op === 'snapshotFor') {
          own(row.event, 'result'); object(row.event.result);
        } else {
          // Only these operations may genuinely omit an undefined return result.
          assert.ok(['step', 'fixtureCommand', 'command'].includes(row.op));
        }
        open.delete(row.call);
        break;
      case 'case-end':
        own(row, 'calls completed'); integer(row.calls); assert.equal(typeof row.completed, 'boolean');
        assert.ok(active);
        assert.equal(row.case, cases.length - 1);
        assert.equal(row.calls, nextCall);
        assert.equal(open.size, 0, 'every begun action has full after evidence');
        active.completed = row.completed;
        active = undefined;
        break;
      case 'footer':
        own(row, 'cases'); integer(row.cases);
        for (const key of ['failure', 'sourceReadbackFailure']) if (Object.hasOwn(row, key)) errorShape(row[key]);
        assert.equal(active, undefined);
        assert.equal(open.size, 0);
        assert.equal(row.cases, cases.length);
        assert.equal(cases.length, 5, 'all original callbacks required');
        assert.ok(cases.every(c => c.completed === true));
        assert.equal(row.failure, undefined, 'original callback failure retained, not admissible');
        assert.equal(row.sourceReadbackFailure, undefined);
        footer = row;
        break;
      default: assert.fail(`unknown original capture record: ${row.type}`);
    }
  }
  await pipeline(input, createGunzip(), async chunks => {
    let pending = '';
    for await (const bytes of chunks) {
      // JSON.stringify emits non-ASCII callback/source text. Decode incrementally
      // so a UTF-8 character split across decompressor chunks stays exact.
      pending += decoder.write(bytes);
      let end;
      while ((end = pending.indexOf('\n')) !== -1) {
        record(pending.slice(0, end));
        pending = pending.slice(end + 1);
      }
    }
    pending += decoder.end();
    assert.equal(pending, '', 'complete newline-terminated records required');
  });
  assert.ok(header && footer, 'complete original capture required');
  return {...header, cases, sha256Compressed: digest.digest('hex')};
}
