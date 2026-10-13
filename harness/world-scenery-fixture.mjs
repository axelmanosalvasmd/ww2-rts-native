// Original full World generator, including dressing and ordered rail networks.
import {readFileSync,writeFileSync,mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');
const ref=resolve(process.env.WW2_REFERENCE??fileURLToPath(new URL('../reference/ww2-rts/',import.meta.url)));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',ref,...a],{encoding:'utf8'}).trim();
if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Use clean pinned reference');
const {generateWorldMap}=await import(pathToFileURL(resolve(ref,'shared/world-conquest.js')));

const fnv=bytes=>{let h=0xcbf29ce484222325n;for(const b of bytes)h=BigInt.asUintN(64,(h^BigInt(b))*0x100000001b3n);return h.toString(16).padStart(16,'0');};
const u32=values=>{const b=Buffer.alloc(values.length*4);values.forEach((v,i)=>b.writeUInt32LE(v,i*4));return b;};
const sim=await import(pathToFileURL(resolve(ref,'shared/sim.js')));
const directory=mkdtempSync(resolve(tmpdir(),'ww2-generated-world-'));
const marker='  // The map clients download stays separate from later building footprints and terrain edits.';
let source=readFileSync(resolve(ref,'shared/sim.js'),'utf8');
if(source.split(marker).length!==2)throw Error('Static terrain capture point must be unique');
source=source.replace(marker,'  return g;\n'+marker).replace(/from '(\.\/[^']+)'/g,(_,path)=>`from '${pathToFileURL(resolve(ref,'shared',path)).href}'`);
writeFileSync(resolve(directory,'sim.mjs'),source);
const capture=await import(pathToFileURL(resolve(directory,'sim.mjs')));
const u16=values=>{const b=Buffer.alloc(values.length*2);values.forEach((v,i)=>b.writeUInt16LE(v,i*2));return b;};
const floatBytes=array=>Buffer.from(array.buffer,array.byteOffset,array.byteLength);
const previousRandom=Math.random;
Math.random=()=>0.5;
try {
const cases=[];
for(const size of ['huge','massive'])for(const seed of [0,1,42,20261003,4294967295])for(const scenery of [false,true]){
 const players=6,teams=[0,0,1,1,2,2];const map=generateWorldMap({size,seed,players,teams,scenery});
 if(sim.validateMap(map)!==null)throw Error('Generated source map did not validate');
 const g=capture.createGame(map,[],false,[],[],{weather:false});
 const terrain={flagsHash:fnv(u16([...g.flags])),houseHash:fnv(g.house),cellHpHash:fnv(floatBytes(g.cellHp)),wearHash:fnv(floatBytes(g.wear))};
 const membership=Buffer.alloc(map.world.regionMap.length*2);map.world.regionMap.forEach((v,i)=>membership.writeInt16LE(v,i*2));
 const rails={stations:map.world.rails.stations,lines:map.world.rails.lines.map(l=>({a:l.a,b:l.b,regions:l.regions,length:l.cells.length,cellsHash:fnv(u32(l.cells))}))};
 cases.push({size,seed,teams,scenery,charsHash:fnv(Buffer.from(map.rows.join(''))),heightsHash:fnv(Buffer.from(map.heights.join(''), 'ascii').map(v=>v-48)),regionMapHash:fnv(membership),
  terrain,regions:map.world.regions,spawns:map.spawns,buildings:map.buildings,biomesHash:map.world.biomes?fnv(map.world.biomes):null,rails});
}
const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases})+'\n';
const path=fileURLToPath(new URL('../rust/crates/core/tests/fixtures/world-scenery.json',import.meta.url));
if(process.argv.includes('--check')){if(output!==readFileSync(path,'utf8'))throw Error('World generation fixture differs');}else writeFileSync(path,output);
console.log(`Verified ${cases.length} full World maps (with/without scenery)`);

}finally {Math.random=previousRandom;rmSync(directory,{recursive:true,force:true});}
