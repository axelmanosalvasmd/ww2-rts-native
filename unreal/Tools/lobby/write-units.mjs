// Writes shared/sim.js UNITS without structures (client/main.js #sbUnit: Object.keys(UNITS).filter(t => !UNITS[t].structure),
// option text UNITS[t].name) into the generated block of Client/TCMatchControls.cpp.
// usage: node write-units.mjs <reference dir> <TCMatchControls.cpp> [--check]
import { readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
const [ref, cpp, mode] = process.argv.slice(2);
const { UNITS } = await import(pathToFileURL(ref + '/shared/sim.js').href);
const list = Object.keys(UNITS).filter((t) => !UNITS[t].structure).map((t) => [t, UNITS[t].name]);
const lit = (s) => 'TEXT("' + String(s).replace(/[\\"]/g, '\\$&') + '")';
const block = ['// BEGIN GENERATED UNITS (round-15 lobby tools/write-units.mjs: shared/sim.js UNITS in Object.keys order without',
  '// structures, as client/main.js fills #sbUnit; reference e4e579a)',
  '\tconst FTCSandboxUnit ReferenceUnits[] = {', ...list.map(([t, n]) => `\t\t{ ${lit(t)}, ${lit(n)} },`), '\t};',
  '// END GENERATED UNITS'].join('\n');
const src = readFileSync(cpp, 'utf8');
const re = /\/\/ BEGIN GENERATED UNITS[\s\S]*?\/\/ END GENERATED UNITS/;
if (!re.test(src)) { console.error('no generated block'); process.exit(2); }
const next = src.replace(re, block);
if (mode === '--check') { if (next !== src) { console.error('units differ'); process.exit(1); } console.log('units up to date'); }
else { writeFileSync(cpp, next); console.log(`wrote ${list.length} units`); }
