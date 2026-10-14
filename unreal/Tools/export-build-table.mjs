// Writes unreal/Source/ThreeCrossroads/Client/TCConstructionData.inl, the construction facts FTCConstruction
// (Client/TCConstruction.cpp) reads, from the pinned reference, which it only reads:
//   shared/sim.js        UNITS (the buildable kinds; every type's sight), BUILDABLE, WORLD_BUILDS, FIELD_BUILDS,
//                        SKIRMISH_BUILDS, CFG (fortBuilders, levelHeight, eye, smokeSight, wood.sight), CELL, TERRAIN,
//                        the terrain flag bits, STATION_YARD, RIDING_FLAG, the FORTS kinds;
//   shared/tech.js       tierOf;
//   shared/world-layers.js BASE_SURFACES;
//   client/keys.js       BUILD_KEYS;
//   client/hud.js        BUILD_ROLE (module-private, read from the source text).
// It also fingerprints the reference functions TCConstruction.cpp ports by hand (PORTED below). When the pin moves and
// one of them changed, it stops: port the change into TCConstruction.cpp, then update that fingerprint.
// Usage (from the repo root):
//   node unreal/Tools/export-build-table.mjs [reference/ww2-rts]           write the file
//   node unreal/Tools/export-build-table.mjs [reference/ww2-rts] --check   exit 1 if the file is not what it would write
import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';

const here = import.meta.dirname;
const args = process.argv.slice(2);
const check = args.includes('--check'), printHashes = args.includes('--print-hashes');
const reference = resolve(args.find((a) => !a.startsWith('--')) ?? join(here, '../../reference/ww2-rts'));
const out = join(here, '../Source/ThreeCrossroads/Client/TCConstructionData.inl');
const fail = (msg) => { console.error(`export-build-table: ${msg}`); process.exit(1); };

const git = (...a) => execFileSync('git', ['-C', reference, ...a], { encoding: 'utf8' }).trim();
const pin = git('rev-parse', '--short', 'HEAD');
if (git('status', '--porcelain', '--untracked-files=no')) fail(`${reference} has local changes: generate from a clean pin only`);

const load = (p) => import(pathToFileURL(join(reference, p)).href);
const sim = await load('shared/sim.js');
const { tierOf } = await load('shared/tech.js');
const { BASE_SURFACES } = await load('shared/world-layers.js');
const { BUILD_KEYS } = await load('client/keys.js');
const { UNITS, UNIT_TYPES, BUILDABLE, WORLD_BUILDS, FIELD_BUILDS, SKIRMISH_BUILDS, CFG, CELL, TERRAIN, MOVE, SIGHT, WOOD, LAND, STATION_YARD, RIDING_FLAG, FORTS } = sim;

// client/hud.js BUILD_ROLE is not exported: read its object literal of plain strings
const hud = readFileSync(join(reference, 'client/hud.js'), 'utf8');
const roleSrc = hud.match(/^const BUILD_ROLE = (\{[\s\S]*?\});\n/m)?.[1];
if (!roleSrc) fail('client/hud.js BUILD_ROLE is no longer an object literal');
const BUILD_ROLE = new Function(`return (${roleSrc});`)();
if (Object.values(BUILD_ROLE).some((v) => typeof v !== 'string')) fail('BUILD_ROLE holds a non-string');

// The reference code TCConstruction.cpp ports by hand: [file, the line that starts it]. A fingerprint covers that
// line and what belongs to it (deeper-indented lines, then a closing bracket at its own indent).
const PORTED = [
  ['shared/sim.js', 'export const BUILDABLE = ', '6d10bf31c339'],
  ['shared/sim.js', 'export const buildKinds = ', 'b647cdaff9ed'],
  ['shared/sim.js', 'export const builderTypes = ', '247884a4d8eb'],
  ['shared/sim.js', 'export const isSkirmishBaseMode = ', 'ae4c199bbd5f'],
  ['shared/sim.js', 'export const productionAccess = ', '946c113e0493'],
  ['shared/sim.js', 'export const allied = ', '0180d5bee88e'],
  ['shared/sim.js', 'const footprint = ', '5975229dc833'],
  ['shared/sim.js', 'function stampOccupied(', 'a9bf19daf799'],
  ['shared/sim.js', 'const canStamp = ', '6bf8336273d5'],
  ['shared/sim.js', 'const footCenter = ', '24a25ca3305a'],
  ['shared/sim.js', 'function worldRegionAt(', '81838637ceed'],
  ['shared/sim.js', 'export function railYard(', '014cbeb15cdf'],
  ['shared/sim.js', 'const visionOf = ', '6e88d49d9529'],
  ['shared/sim.js', 'export function teamSees(', 'a4076401b60b'],
  ['shared/sim.js', 'const cellOf = ', 'cd4b275867fd'],
  ['shared/sim.js', 'const cellCenter = ', 'd31a024c68af'],
  ['shared/sim.js', 'function blocker(', 'c75597097315'],
  ['shared/sim.js', 'function segHits(', '1ba0c94251df'],
  ['shared/sim.js', 'const level = ', '63afbadc577a'],
  ['shared/sim.js', 'export const levelAt = ', '9d11ba38a81d'],
  ['shared/sim.js', 'function hillsClear(', '7a44141c05d6'],
  ['shared/sim.js', 'const smoked = ', 'e939e33f3b20'],
  ['shared/sim.js', 'export const los = ', 'e4c31ca9c952'],
  ['shared/sim.js', 'export function placementCheck(', 'a8a4559d073e'],
  ['shared/sim.js', 'function onCoast(', 'c4bcc541f699'],
  ['shared/weather.js', 'export const sightMul = ', 'c5d50f1febfc'],
  ['shared/world-layers.js', 'export const composeWorldCell = ', '380374b9db58'],
  ['shared/world-layers.js', 'export function migrateWorldMap(', 'e7c9265ee87e'],
  ['shared/world-territories.js', 'export function insideKnownRegion(', '647e4493bcf9'],
  ['client/world-state.js', 'apply(s) {', '7ec3f641704a'],
  ['client/logistics.js', 'const wire = snapshot?.logistics', '56712ed8926a'],
  ['client/availability.js', 'const resources = ', '0505b8bf95a2'],
  ['client/availability.js', 'export function availability(', 'edce3b256bee'],
  ['client/availability.js', 'export function rememberPlacementTerrain(', '05c83ed90a90'],
  ['client/availability.js', 'export function placementState(', '0f0607229a6c'],
  ['client/main.js', 'const classicMode = ', '2c8a484cd8bd'],
  ['client/main.js', 'const layers = worldLayers(map);', '3c667fe9e342'],
  ['client/main.js', '  for (const [cell, ch, lv, st, data] of m.cells || [])', '3b09278c82ed'],
  ['client/main.js', '    if (data) { terrain.groundGrid[y][x] = data.ground;', '460488966264'],
  ['client/main.js', 'function placementView(', '796da51c2792'],
  ['client/main.js', 'const builders = ', 'a81bdeb7fd4b'],
  ['client/main.js', 'const owns = ', 'bf822bc9ad80'],
  ['client/main.js', 'function canPlace(', '5e8c8170aa1c'],
  ['client/main.js', 'function startBuild(', 'e618a2428bb1'],
  ['client/main.js', 'function footAt(', '01a58b774388'],
  ['client/main.js', 'function setAim(', 'd61cbc199f07'],
  ['client/main.js', '    if (UNITS[kind]?.building) {', 'ec3b55189ad6'],
  ['client/main.js', '    } else if (g && UNITS[targeting]?.building) {', '9186419b4831'],
  ['client/hud.js', '    if (m === \'build\') {', '8d9f1958aac3'],
  ['client/hud.js', '    const types = ctx.PRIORITY.filter(', 'baddb7603bac'],
  ['client/hud.js', '      else if (a.startsWith(\'bld:\'))', '0d7021f9f77e'],
  ['client/hud.js', 'function drawClassicCard(', 'a6f8e879d780'],
  ['client/hud.js', '    if (!sel.length || !sel.every(v => v.type === \'truck\')) {', 'da8ae57dadb6'],
];
const sources = new Map();
function excerpt(file, anchor) {
  if (!sources.has(file)) sources.set(file, readFileSync(join(reference, file), 'utf8').split('\n'));
  const lines = sources.get(file), hits = [];
  lines.forEach((l, i) => { if (l.trimStart().startsWith(anchor.trimStart()) && (anchor !== anchor.trimStart() ? l.startsWith(anchor) : true)) hits.push(i); });
  if (hits.length !== 1) fail(`${file}: ${hits.length} lines start with ${JSON.stringify(anchor)} (want exactly one)`);
  const indent = (l) => l.length - l.trimStart().length, i0 = hits[0], ind = indent(lines[i0]), body = [lines[i0]];
  for (let i = i0 + 1; i < lines.length; i++) {
    const l = lines[i];
    if (!l.trim() || indent(l) > ind) { body.push(l); continue; }
    if (indent(l) === ind && /^[}\])]/.test(l.trimStart())) body.push(l);
    break;
  }
  while (body.length && !body.at(-1).trim()) body.pop();
  return body.join('\n');
}
const stale = [];
for (const [file, anchor, want] of PORTED) {
  const got = createHash('sha256').update(excerpt(file, anchor)).digest('hex').slice(0, 12);
  if (printHashes) console.log(`${got}  ${file}  ${anchor.trim()}`);
  else if (got !== want) stale.push(`${file} ${JSON.stringify(anchor.trim())}: ${want} -> ${got}`);
}
if (printHashes) process.exit(0);
if (stale.length) fail(`reference code ported by hand changed since the port (re-port it into TCConstruction.cpp, then update PORTED):\n  ${stale.join('\n  ')}`);

// ---------- the data ----------
const s = (v) => `TEXT("${String(v).replace(/\\/g, '\\\\').replace(/"/g, '\\"')}")`;
const b = (v) => (v ? 'true' : 'false');
const int = (v, what) => { if (!Number.isInteger(v)) fail(`${what} is not an integer: ${v}`); return String(v); };
const real = (v, what) => { if (!Number.isFinite(v)) fail(`${what} is not finite: ${v}`); return Number.isInteger(v) ? `${v}.0` : String(v); };
const list = (name, items, why) => `\t// ${why}\n\tinline const TCHAR* const ${name}[] = { ${items.map(s).join(', ')} };`;

const kinds = [...new Set([...WORLD_BUILDS, ...SKIRMISH_BUILDS, ...FIELD_BUILDS, ...BUILDABLE])];
for (const k of kinds) {
  const d = UNITS[k];
  if (!d?.building || !d.structure) fail(`${k} is not a building`);
  if (d.needs !== undefined && !UNITS[d.needs]) fail(`${k} needs unknown ${d.needs}`);
  for (const key of Object.keys(d)) if (!['name', 'cost', 'buildTime', 'size', 'needs', 'coast', 'building', 'structure'].includes(key) && /coast|needs|place|foot/i.test(key)) fail(`${k}.${key}: a placement field the port does not know`);
}
const terrain = Object.entries(TERRAIN);
for (const [ch] of terrain) if (ch.length !== 1 || ch.charCodeAt(0) > 127) fail(`TERRAIN key ${ch}`);
const walls = /const canStamp = [^\n]*'([^']*)'\.includes\(g\.chars\[c\]\)/.exec(readFileSync(join(reference, 'shared/sim.js'), 'utf8'))?.[1];
if (walls === undefined) fail('shared/sim.js canStamp: the characters nothing is built on were not found');

const text = `// Generated by unreal/Tools/export-build-table.mjs from the reference at ${pin}. Do not edit: when the pin moves,
// run the tool again (node unreal/Tools/export-build-table.mjs reference/ww2-rts). Included by TCConstruction.cpp only.
#pragma once

namespace TCConstructionData
{
	inline const TCHAR* const ReferencePin = ${s(pin)};

	// shared/sim.js CELL, CFG.levelHeight, CFG.eye, CFG.smokeSight, CFG.wood.sight, STATION_YARD
	constexpr double Cell = ${real(CELL, 'CELL')};
	constexpr double LevelHeight = ${real(CFG.levelHeight, 'CFG.levelHeight')};
	constexpr double Eye = ${real(CFG.eye, 'CFG.eye')};
	constexpr double SmokeSight = ${real(CFG.smokeSight, 'CFG.smokeSight')};
	constexpr int32 WoodSight = ${int(CFG.wood.sight, 'CFG.wood.sight')};
	constexpr int32 StationYard = ${int(STATION_YARD, 'STATION_YARD')};
	// shared/sim.js terrain flag bits
	constexpr int32 MoveFlag = ${int(MOVE, 'MOVE')}, SightFlag = ${int(SIGHT, 'SIGHT')}, WoodFlag = ${int(WOOD, 'WOOD')}, LandFlag = ${int(LAND, 'LAND')};
	// shared/sim.js RIDING_FLAG: a unit row's flag for a squad riding a carrier
	constexpr int32 RidingFlag = ${int(RIDING_FLAG, 'RIDING_FLAG')};

	// shared/sim.js TERRAIN (with the LAND bits it adds): a cell character's flags; any other character has none
	struct FTerrainRow { TCHAR Ch; int32 Flags; };
	inline const FTerrainRow Terrain[] = {
${terrain.map(([ch, f]) => `\t\t{ TEXT('${ch === "'" || ch === '\\' ? '\\' + ch : ch}'), ${int(f, 'TERRAIN ' + ch)} },`).join('\n')}
	};
	// shared/sim.js canStamp: the cell characters nothing is built on
	inline const TCHAR* const NoStamp = ${s(walls)};
	// shared/world-layers.js BASE_SURFACES: the ground characters (the rest of a cell character is an object)
	inline const TCHAR* const BaseSurfaces = ${s(BASE_SURFACES)};

	// shared/sim.js buildKinds: what builders put up in Classic, World, outside Classic and in the skirmish base modes
${list('Buildable', BUILDABLE, 'BUILDABLE')}
${list('WorldBuilds', WORLD_BUILDS, 'WORLD_BUILDS')}
${list('FieldBuilds', FIELD_BUILDS, 'FIELD_BUILDS')}
${list('SkirmishBuilds', SKIRMISH_BUILDS, 'SKIRMISH_BUILDS')}
${list('FortBuilders', CFG.fortBuilders, 'CFG.fortBuilders: the builder squads outside Classic (Classic and World: Engineers)')}
${list('Forts', Object.keys(FORTS), 'FORTS: the fortification kinds (placementCheck places these another way)')}

	// One buildable kind: shared/sim.js UNITS name, cost, buildTime, size, needs (empty: none), coast; shared/tech.js
	// tierOf; client/keys.js BUILD_KEYS (empty: no key); client/hud.js BUILD_ROLE (empty: none).
	struct FBuildRow { const TCHAR* Kind; const TCHAR* Name; int32 Cost, BuildTime, Size, Tier; const TCHAR* Needs; bool bCoast; const TCHAR* Key; const TCHAR* Role; };
	inline const FBuildRow Builds[] = {
${kinds.map((k) => { const d = UNITS[k]; return `\t\t{ ${[s(k), s(d.name), int(d.cost, k + '.cost'), int(d.buildTime, k + '.buildTime'), int(d.size, k + '.size'), int(tierOf(k), 'tierOf ' + k), s(d.needs ?? ''), b(d.coast), s(BUILD_KEYS[k] ?? ''), s(BUILD_ROLE[k] ?? '')].join(', ')} },`; }).join('\n')}
	};
	// Every unit type's sight and the flags placement reads (shared/sim.js UNITS vision, lookout, building, structure, air,
	// and name for the needs sentence), in UNIT_TYPES order.
	struct FSightRow { const TCHAR* Type; const TCHAR* Name; double Vision; bool bLookout, bBuilding, bStructure, bAir; };
	inline const FSightRow Sight[] = {
${UNIT_TYPES.map((t) => { const d = UNITS[t]; return `\t\t{ ${[s(t), s(d.name ?? t), real(d.vision, t + '.vision'), b(d.lookout), b(d.building), b(d.structure), b(d.air)].join(', ')} },`; }).join('\n')}
	};
}
`;
if (check) {
  let have = '';
  try { have = readFileSync(out, 'utf8'); } catch {}
  if (have !== text) fail(`${out} is not what the reference at ${pin} gives: run the tool without --check`);
  console.log(`export-build-table: ${out} matches the reference at ${pin}`);
} else {
  writeFileSync(out, text);
  console.log(`export-build-table: wrote ${out} from the reference at ${pin} (${kinds.length} buildable kinds, ${UNIT_TYPES.length} unit types)`);
}
