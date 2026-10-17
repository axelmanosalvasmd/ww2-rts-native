// Writes the oracle's answers into the generated block of Client/TCRoomBrowser.cpp (between the BEGIN and END marks).
// usage: node write-fixtures.mjs <directory-oracle.json> <TCRoomBrowser.cpp> [--check]
import { readFileSync, writeFileSync } from 'node:fs';
const [json, cpp, mode] = process.argv.slice(2);
const d = JSON.parse(readFileSync(json, 'utf8'));
const lit = (s) => 'TEXT("' + [...String(s)].map(ch => { const c = ch.codePointAt(0); if (ch === '"' || ch === '\\') return '\\' + ch; if (c < 0x20 || c > 0x7e) return c > 0xffff ? ch : '\\u' + c.toString(16).padStart(4, '0').toUpperCase(); return ch; }).join('') + '")';
const counts = d.rows.length;
const lines = [
  '// BEGIN GENERATED FIXTURES (round-15 lobby tools/write-fixtures.mjs from directory-oracle.json: the reference\'s own',
  '// client/public-lobby.js and client/room-session.js under node, fed by GET /api/rooms recorded from an isolated',
  `// reference server at e4e579a, run tag ${d.tag})`,
  'namespace TCRoomFx',
  '{',
  '\tstruct FAddress { const TCHAR* Hash; const TCHAR* Room; const TCHAR* Seat; const TCHAR* Out; };',
  '\tconst FAddress Addresses[] = {',
  ...d.addresses.map(a => `\t\t{ ${a.map(lit).join(', ')} },`),
  '\t};',
  `\tconst TCHAR* const Directory = ${lit(d.body)};`,
  `\tconst TCHAR* const DirectoryStatus = ${lit(d.status)};`,
  '\tstruct FRow { const TCHAR* Cells; };',
  '\tconst FRow DirectoryRows[] = {',
  ...d.rows.map(r => `\t\t{ ${lit(r)} },`),
  '\t};',
  `\tconst int32 HordeRows = ${d.hordeRows};`,
  `\tconst int32 OpenRows = ${d.openRows};`,
  `\tconst TCHAR* const QuickPlayRoom = ${lit(d.quickPlay.split('#')[1])};`,
  `\tconst TCHAR* const ServerTitleIn = ${lit(d.serverTitle[0])};`,
  `\tconst TCHAR* const ServerTitleOut = ${lit(d.serverTitle[1])};`,
  `\tconstexpr double RefreshMs = ${d.refreshEveryMs};`,
  '}',
  '// END GENERATED FIXTURES',
];
const src = readFileSync(cpp, 'utf8');
const re = /\/\/ BEGIN GENERATED FIXTURES[\s\S]*?\/\/ END GENERATED FIXTURES/;
if (!re.test(src)) { console.error('no generated block in ' + cpp); process.exit(2); }
const next = src.replace(re, lines.join('\n'));
if (mode === '--check') { if (next !== src) { console.error('fixtures differ'); process.exit(1); } console.log('fixtures up to date'); }
else { writeFileSync(cpp, next); console.log(`wrote ${counts} rows, ${d.addresses.length} addresses`); }
