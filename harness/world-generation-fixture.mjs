// Original bare geography, without biome dressing. Rails read geography but do not change these fields.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');
const ref=resolve(process.env.WW2_REFERENCE??fileURLToPath(new URL('../reference/ww2-rts/',import.meta.url)));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',ref,...a],{encoding:'utf8'}).trim();
if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Use clean pinned reference');
const {generateWorldMap}=await import(pathToFileURL(resolve(ref,'shared/world-conquest.js')));
const {relief,waterways}=await import(pathToFileURL(resolve(ref,'shared/world-landforms.js')));
const fnv=bytes=>{let h=0xcbf29ce484222325n;for(const b of bytes)h=BigInt.asUintN(64,(h^BigInt(b))*0x100000001b3n);return h.toString(16).padStart(16,'0');};
const u32=values=>{const b=Buffer.alloc(values.length*4);values.forEach((v,i)=>b.writeUInt32LE(v,i*4));return b;};
const cases=[];
for(const size of ['huge','massive'])for(const seed of [0,1,42,20261003,4294967295]){
 const players=6,teams=[0,0,1,1,2,2];const map=generateWorldMap({size,seed,players,teams,scenery:false});
 const original=relief(map.w,map.h,seed);const ground=Array.from({length:map.h},()=>Array(map.w).fill('.'));
 const heights=Array.from({length:map.h},(_,y)=>[...original.subarray(y*map.w,(y+1)*map.w)].map(String));
 const features=waterways(map.w,map.h,seed,ground,heights);
 const membership=Buffer.alloc(map.world.regionMap.length*2);map.world.regionMap.forEach((v,i)=>membership.writeInt16LE(v,i*2));
 const channels=features.rivers.map(r=>({id:r.id,parent:r.parent,pathHash:fnv(u32(r.path.flatMap(p=>[p.x,p.y,p.radius])))}));
 cases.push({size,seed,teams,charsHash:fnv(Buffer.from(map.rows.join(''))),heightsHash:fnv(Buffer.from(map.heights.join(''), 'ascii').map(v=>v-48)),regionMapHash:fnv(membership),
  regions:map.world.regions,spawns:map.spawns,channels,crossings:features.crossings,lake:features.lake,passes:features.passes});
}
const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,scenery:false,cases})+'\n';
const path=fileURLToPath(new URL('../rust/crates/core/tests/fixtures/world-generation.json',import.meta.url));
if(process.argv.includes('--check')){if(output!==readFileSync(path,'utf8'))throw Error('World generation fixture differs');}else writeFileSync(path,output);
console.log(`Verified ${cases.length} bare-geography maps`);
