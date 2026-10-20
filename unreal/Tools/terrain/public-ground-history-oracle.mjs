// Public-placeholder history component. Hidden values never enter the reference.
import { readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { resolve, join } from 'node:path';
import assert from 'node:assert/strict';
const repo=process.cwd(), ref=join(repo,'reference/ww2-rts'), evidence=resolve(process.argv[2]), fixture=resolve(process.argv[3]);
const {installShims}=await import(pathToFileURL(join(repo,'unreal/Tools/models/shims.mjs'))); installShims();
globalThis.localStorage={getItem:()=>null,setItem:()=>{}}; globalThis.location={search:''};
const THREE=await import('three');
const {createRelief}=await import(pathToFileURL(join(ref,'client/relief.js')));
const W=9,H=9,N=W*H, known=Array.from({length:N},(_,c)=>c%W<4?1:0);
const grid=Array.from({length:H},()=>[...'.........']);
const map={w:W,h:H,world:true,rows:grid.map(r=>r.join('')),heights:Array(H).fill('111100000')};
const material=new THREE.MeshBasicMaterial();
const relief=createRelief(map,grid,{low:true,isRoad:(x,y)=>grid[y][x]==='D',material});
const samples=[];for(const z of [7,9,11])for(const x of [3,5,6,7,7.5,8,8.5,9,10,11,13,17])samples.push([x,z]);
const stages=[];
const capture=(name,changed)=>stages.push({name,changed,grid:grid.flat().join(''),levels:map.heights.join(''),known:known.join(''),samples:samples.map(([x,z])=>relief.hAt(x,z))});
const edit=(c,ch,level)=>{const x=c%W,y=Math.floor(c/W);grid[y][x]=ch;map.rows[y]=grid[y].join('');map.heights[y]=map.heights[y].slice(0,x)+level+map.heights[y].slice(x+1);};
capture('public-start',[]);
const revealed=[];for(let y=0;y<H;y++){const c=y*W+4;known[c]=1;edit(c,'.','1');revealed.push(c);}relief.update(revealed);capture('delivered-discovery',revealed);
for(const [name,c,ch,level] of [['crater',38,'+','0'],['trench',39,'T','1'],['road',29,'D','1'],['filled-crater',38,'.','1'],['flooded-trench',39,'F','a']]) {
  edit(c,ch,level); relief.update([c]); capture(name,[c]);
}
relief.update([]);capture('remembered-no-terrain-change',[]);
const sha=p=>createHash('sha256').update(readFileSync(p)).digest('hex');
const result={reference:'e4e579a838aabae3b1d9dadbc17a0a87147bc52d',sourceSHA256:sha(join(ref,'client/relief.js')),W,H,
  boundary:'Synthetic sanitized recipient component. Known cells plus public open level-zero placeholders only. No hidden input is supplied to source.',
  frozenHeights:'111100000'.repeat(H),samples,stages,
  limits:'Source HAt fixtures do not establish production known-only boundary equality, native actor reconnect, GPU geometry, actual World delivery or Rust World admission.'};
writeFileSync(join(evidence,'public-ground-history-source.json'),JSON.stringify(result,null,2)+'\n');
const q=JSON.stringify,num=n=>Number(n).toPrecision(17);
let table='// Generated from pinned relief.js by Tools/terrain/public-ground-history-oracle.mjs.\n// Public sanitized component only; production unknown relief remains a separate diagnostic.\n';
table+='static const double World26PublicSamples[][2] = {\n'+samples.map(s=>'    { '+s.map(num).join(', ')+' },').join('\n')+'\n};\n';
table+='struct FWorld26PublicStage { const char* Name; const char* Grid; const char* Levels; const char* Known; double Height[36]; };\nstatic const FWorld26PublicStage World26PublicStages[] = {\n';
for(const s of stages)table+=`    { ${q(s.name)}, ${q(s.grid)}, ${q(s.levels)}, ${q(s.known)}, { ${s.samples.map(num).join(', ')} } },\n`;
table+='};\n';
if(process.argv.includes('--check'))assert.equal(readFileSync(fixture,'utf8'),table);else writeFileSync(fixture,table);
relief.dispose();material.dispose();
console.log(JSON.stringify({stages:stages.length,publicHeightSamples:stages.length*samples.length,fixture:process.argv.includes('--check')?'current':'written',sourceOnly:true}));
