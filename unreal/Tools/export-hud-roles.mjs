// Reproduce the original unit-roles.js text in TCRecruitment.cpp. Reads the reference only.
import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL,fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import assert from 'node:assert/strict';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const ref=path.resolve(process.argv.slice(2).find(v=>!v.startsWith('--')) ?? root+'/reference/ww2-rts');
const pin=execFileSync('git',['-C',ref,'rev-parse','HEAD'],{encoding:'utf8'}).trim();
assert.equal(pin,'e4e579a838aabae3b1d9dadbc17a0a87147bc52d');
const {UNITS,UNIT_TYPES}=await import(pathToFileURL(ref+'/shared/sim.js'));
const {unitRole}=await import(pathToFileURL(ref+'/client/unit-roles.js'));
const quote=s=>'TEXT('+JSON.stringify(s)+')';
const rows=UNIT_TYPES.filter(type=>unitRole(type,UNITS[type].name)!==UNITS[type].name)
  .map(type=>'\t\t{ '+quote(type)+', '+quote(unitRole(type,UNITS[type].name))+' },');
const block='\t// BEGIN GENERATED SOURCE ROLES e4e579a\n'+
  '\tstruct FRole { const TCHAR* Type; const TCHAR* Text; };\n\tconst FRole SourceRoles[] = {\n'+rows.join('\n')+
  '\n\t};\n\t// END GENERATED SOURCE ROLES';
if(process.argv.includes('--check')){
  const native=fs.readFileSync(root+'/unreal/Source/ThreeCrossroads/Client/TCRecruitment.cpp','utf8');
  assert.equal(native.match(/\t\/\/ BEGIN GENERATED SOURCE ROLES e4e579a[\s\S]*?\t\/\/ END GENERATED SOURCE ROLES/)?.[0],block);
  console.log(JSON.stringify({reference:pin,roles:rows.length,sourceTextMatches:true}));
}else process.stdout.write(block+'\n');
