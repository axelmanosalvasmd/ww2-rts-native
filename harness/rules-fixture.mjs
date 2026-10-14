// Compile post-initialization reference tables and record pure-helper outputs.
// Run with x64 Node 24.21.0, matching harness/math-fixture.mjs. --check compares without writing.
// --reference=/path/to/ww2-rts selects an already pinned read-only checkout.
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { setFlagsFromString } from 'node:v8';

if (process.version !== 'v24.21.0' || process.arch !== 'x64') throw new Error('Use x64 Node 24.21.0');
// The core uses V8 fdlibm pow on every platform (DESIGN.md, The core\'s math).
setFlagsFromString('--no-use-std-math-pow');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.argv.find(a => a.startsWith('--reference='))?.slice(12) ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
if (execFileSync('git', ['rev-parse', 'HEAD'], { cwd: reference, encoding: 'utf8' }).trim() !== pin) throw new Error('Wrong reference pin');
execFileSync('git', ['diff', '--exit-code', pin, '--', 'shared'], { cwd: reference, stdio: 'pipe' });
const load = name => import(pathToFileURL(`${reference}/shared/${name}.js`));
const [sim, tech, weather, formation, men, vehicle, traffic] = await Promise.all(['sim', 'tech', 'weather', 'formation', 'squad-men', 'vehicle-motion', 'local-traffic'].map(load));

// Preserve all values. JSON's default conversion silently replaces NaN and Infinity with null.
function encode(value) {
  if (typeof value === 'number') {
    if (Number.isNaN(value)) return { $number: 'NaN' };
    if (value === Infinity) return { $number: 'Infinity' };
    if (value === -Infinity) return { $number: '-Infinity' };
    if (Object.is(value, -0)) return { $number: '-0' };
    return value;
  }
  if (value === null || typeof value === 'boolean' || typeof value === 'string') return value;
  if (Array.isArray(value)) return value.map(encode);
  if (typeof value !== 'object' || Object.hasOwn(value, '$number')) throw new Error('Unsupported source value or reserved tag');
  return Object.fromEntries(Object.keys(value).sort().map(key => [key, encode(value[key])]));
}
const text = value => `${JSON.stringify(encode(value))}\n`;
const fnv = bytes => {
  let hash = 0xcbf29ce484222325n;
  for (const byte of bytes) hash = BigInt.asUintN(64, (hash ^ BigInt(byte)) * 0x100000001b3n);
  return hash.toString(16).padStart(16, '0');
};
const bits = value => { const b = Buffer.alloc(8); b.writeDoubleBE(value); return b.toString('hex'); };
const tables = {};
for (const key of ['UNITS', 'UNIT_TYPES', 'CFG', 'SUPPORT', 'SUPPORT_TYPES', 'FORTS', 'TERRAIN', 'ENTRENCH', 'ENTRENCH_TYPES', 'BUILDABLE', 'WORLD_BUILDS', 'FIELD_BUILDS', 'SKIRMISH_BUILDS', 'AB_MUN', 'DOCTRINE', 'CELL', 'TICK', 'FACTION_COUNT', 'MAX_PLAYERS', 'MOVE', 'SIGHT', 'COVER', 'TRENCH', 'FORD', 'WIRE', 'VBLOCK', 'ROAD', 'MUD', 'WOOD', 'LAND', 'SHOAL']) tables[key] = sim[key];
for (const key of ['TIER', 'TECH', 'TIER_NAMES']) tables[key] = tech[key];
for (const key of ['WEATHER', 'WEATHER_KINDS', 'WEATHER_CHOICES', 'WEATHER_WARN']) tables[key] = weather[key];
tables.WORLD_TUNING = (await load('world-conquest')).WORLD_TUNING;
tables.SHAPES = formation.SHAPES;
tables.TRAFFIC = traffic.TRAFFIC;
tables.LANE = traffic.LANE;
tables.CORNER_BULGE = vehicle.CORNER_BULGE;
// Resolve the private fallback through the source helper, including null for nonground types.
tables.MOVEMENT_PROFILES = Object.fromEntries(sim.UNIT_TYPES.map(type => [type, vehicle.movementProfile(type, sim.UNITS[type])]));
for (const key of ['SLOTS', 'GUN_SLOTS', 'SLOT_SCALE']) tables[key] = men[key];
if (sim.UNIT_TYPES.length !== 47) throw new Error('Expected all 47 unit definitions');
const catalog = { format: 1, reference: pin, tables };
const catalogText = text(catalog);

// Capture additions from initialized modules, separately from the existing catalog.
const [debris, structures, layers] = await Promise.all(['debris-motion', 'structures', 'world-layers'].map(load));
const additions = {};
for (const [module, names] of [
  [sim, ['RAIL_DIRS', 'POINT_KINDS', 'UNIT_ROW', 'GATE_OPEN_FLAG', 'OPERATIVE_FLAG', 'AUTO_FLAG', 'RIDING_FLAG', 'CARGO_FLAG', 'CARGO_SHIFT']],
  [debris, ['DEBRIS_LIMITS', 'SECTION_MASS']],
  [structures, ['STRUCTURE_LIMITS']],
  [layers, ['WORLD_VERSION', 'BASE_SURFACES', 'OBJECT_TYPES', 'BIOMES', 'MATERIALS']],
]) for (const name of names) {
  if (!Object.hasOwn(module, name) || Object.hasOwn(tables, name)) throw new Error(`Invalid startup addition: ${name}`);
  additions[name] = module[name];
}
if (Object.keys(tables).length !== 47 || Object.keys(additions).length !== 17) throw new Error('Expected 47 catalog tables and 17 additions');

// The pin keeps these literals private. Extract from the actual functions and source,
// failing if their expression changes instead of retaining a copied fallback value.
const formationSource = readFileSync(`${reference}/shared/formation.js`, 'utf8');
const numberFrom = (source, pattern, label, group = 1) => {
  const match = source.match(pattern);
  if (!match) throw new Error(`Formation expression changed: ${label}`);
  const value = Number(match[group]);
  if (!Number.isFinite(value)) throw new Error(`Invalid formation number: ${label}`);
  return value;
};
const slotSource = formation.slotSize.toString(), spotsSource = formation.facingSpots.toString();
const slotPattern = /def\.infantry\s*\?\s*(\d+(?:\.\d+)?)\s*:\s*(\d+(?:\.\d+)?)/;
const formationNumbers = {
  infantrySlot: numberFrom(slotSource, slotPattern, 'infantrySlot'),
  vehicleSlot: numberFrom(slotSource, slotPattern, 'vehicleSlot', 2),
  slotMargin: numberFrom(slotSource, /2\s*\*\s*def\.radius\s*\+\s*(\d+(?:\.\d+)?)/, 'slotMargin'),
  rankGap: numberFrom(spotsSource, /Math\.max\(\.\.\.rank\.map\(p => p\.u\.size\)\)\s*\+\s*(\d+(?:\.\d+)?)/, 'rankGap'),
  lineRank: numberFrom(formationSource, /Math\.ceil\(n \/ Math\.ceil\(n \/ (\d+(?:\.\d+)?)\)\)/, 'lineRank'),
  maxStretch: numberFrom(spotsSource, /Math\.min\((\d+(?:\.\d+)?)\s*\*\s*tight, far\)/, 'maxStretch'),
  depthSteps: numberFrom(spotsSource, /\(b\.u\.x - a\.u\.x\) \* dx \* (\d+(?:\.\d+)?) \+ \(b\.u\.z - a\.u\.z\) \* dz \* \1/, 'depthSteps'),
};
const formationBytes = Buffer.concat([
  ...formation.SHAPES.map(shape => Buffer.from(`${shape}\0`)),
  ...Object.values(formationNumbers).map(value => { const bytes = Buffer.alloc(8); bytes.writeDoubleLE(value); return bytes; }),
]);
const startup = {
  schemaVersion: 1, reference: pin,
  formation: { hash: fnv(formationBytes), scope: 'formation-only', shapes: formation.SHAPES, numbers: formationNumbers },
  tables: { ...tables, ...additions },
};
const startupText = text(startup);
const sha256 = data => createHash('sha256').update(data).digest('hex');
const acceptedContent = readFileSync(`${root}/docs/contracts/startup-tables/proposed-content.json`, 'utf8');
const acceptedSha256 = '5de90a7990ec5e8a2903436ac4809302464be29b546d79591a2adc08c68a2efd';
if (sha256(acceptedContent) !== acceptedSha256) throw new Error('Accepted candidate bytes changed');
// Canonicalize both through this encoder: encoding differences must not hide value changes.
function ordered(value) {
  if (Array.isArray(value)) return value.map(ordered);
  if (value !== null && typeof value === 'object') return Object.fromEntries(Object.keys(value).sort().map(key => [key, ordered(value[key])]));
  return value;
}
if (JSON.stringify(encode(startup)) !== JSON.stringify(ordered(JSON.parse(acceptedContent)))) throw new Error('Startup values differ from accepted candidate');

// Every numeric leaf comes from the raw initialized exports, before tag encoding.
// Paths use RFC 6901; bits are big-endian IEEE754 hex, including tagged numbers.
const numbers = [];
function recordNumbers(value, path = '') {
  if (typeof value === 'number') numbers.push({ path, bits: bits(value) });
  else if (Array.isArray(value)) value.forEach((item, index) => recordNumbers(item, `${path}/${index}`));
  else if (value !== null && typeof value === 'object') for (const key of Object.keys(value).sort()) {
    recordNumbers(value[key], `${path}/${key.replaceAll('~', '~0').replaceAll('/', '~1')}`);
  }
}
recordNumbers(startup);
const startupValues = { format: 1, reference: pin, contentSha256: sha256(startupText), numbers };

const modes = [null, '', 'classic', 'world', 'conquest', 'assault', 'annihilation', 'horde', 'tutorial', 'unknown'];
const modeResults = modes.map(kind => {
  const g = kind === null ? {} : { mode: { kind } };
  return {
    kind,
    builds: sim.buildKinds(sim.isConstructionMode(g), sim.isSkirmishBaseMode(g), kind === 'world'),
    builders: sim.builderTypes(sim.isConstructionMode(g)),
    units: sim.UNIT_TYPES.map(type => [type, sim.priceOf(g, type), sim.popUse(type), [0, 1, 2, 3].map(faction => sim.canBuild(type, faction)), tech.tierOf(type)]),
    supports: sim.SUPPORT_TYPES.map(type => [type, sim.supCost(g, type), sim.dropPop(type)]),
  };
});
const abilities = [
  ...Object.values(sim.UNITS).filter(d => d.ab).map(d => d.ab),
  ...Object.keys(sim.AB_MUN).map(id => ({ id })),
  {}, { id: 'unknown' }, { id: 'grenade', mun: null }, { id: 'grenade', mun: 0 },
  { id: 'unknown', mun: 12.5 }, { id: 'grenade', mun: -1 }, { id: 'smoke', mun: NaN },
  { id: 'smoke', mun: Infinity }, { id: 'smoke', mun: -Infinity }, { id: 'smoke', mun: -0 },
];
const abilityResults = modes.flatMap(kind => abilities.map(ability => ({ kind, ability, expected: bits(sim.abCost({ mode: { kind } }, ability)) })));
const player = (tier, inf, guns, armor) => ({ tier, armory: { inf, guns, armor } });
const research = modes.flatMap(kind => [1, 2, 3].flatMap(tier => [0, 1, 2, 3].flatMap(level => ['tier', 'inf', 'guns', 'armor'].map(technology => {
  const p = player(tier, level, (level + 1) % 4, (level + 2) % 4);
  return { kind, player: p, technology, expected: tech.techCost(p, technology, sim.isConstructionMode({ mode: { kind } })) };
}))));
// All 47 x 47 unit pairs, every effective level, mixed research lines, and absent players/attackers.
// Rows store indices into player/type lists to keep the fixture small and readable by the test.
const players = [null, ...[0, 1, 2, 3].map(n => player(1, n, (n + 1) % 4, (n + 2) % 4))];
const types = [null, ...sim.UNIT_TYPES];
const multipliers = [], multiplierSamples = [];
let multiplierCases = 0;
for (let a = 0; a < players.length; a++) for (let d = 0; d < players.length; d++) {
  for (let by = 0; by < types.length; by++) {
    let hash = 0xcbf29ce484222325n;
    for (let to = 1; to < types.length; to++) {
      const expected = bits(tech.techMul(players[a], sim.UNITS[types[by]], players[d], sim.UNITS[types[to]]));
      for (const byte of Buffer.from(expected, 'hex')) hash = BigInt.asUintN(64, (hash ^ BigInt(byte)) * 0x100000001b3n);
      // Direct bit assertions for every attacker against infantry, vehicles and structures.
      if (['rifle', 'tank', 'hq'].includes(types[to])) multiplierSamples.push([a, by, d, to, expected]);
      multiplierCases++;
    }
    multipliers.push([a, by, d, hash.toString(16).padStart(16, '0')]);
  }
}
const fixture = { format: 1, reference: pin, catalogFnv: fnv(Buffer.from(catalogText)), pow: 'V8 fdlibm', modes: modeResults, abilities: abilityResults, research, players, types, multipliers, multiplierSamples, multiplierCases };
const files = [
  ['rust/crates/core/data/rules.json', catalogText],
  ['rust/crates/core/tests/fixtures/economy.json', text(fixture)],
  ['rust/crates/core/data/startup-table-additions.json', text({ format: 1, reference: pin, tables: additions })],
  ['rust/crates/core/tests/fixtures/startup-tables.json', startupText],
  ['rust/crates/core/tests/fixtures/startup-table-values.json', text(startupValues)],
];
for (const [path, data] of files) {
  const dest = `${root}/${path}`;
  if (process.argv.includes('--check')) {
    if (readFileSync(dest, 'utf8') !== data) throw new Error(`Recorded data differs: ${path}`);
  } else writeFileSync(dest, data);
}
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} ${Object.keys(tables).length} tables, 47 units, ${modeResults.length} modes, ${abilityResults.length} abilities, ${research.length} research cases, ${multiplierCases} damage multipliers`);

console.log(`Startup: 64 tables, ${numbers.length} numeric leaves, formation ${startup.formation.hash}, SHA256 ${sha256(startupText)}; accepted bytes ${startupText === acceptedContent ? 'identical' : 'different'}, values equal`);
