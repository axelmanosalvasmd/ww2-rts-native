// Original spawnUnit and region/nearestFree bodies, projected through the
// existing native Unit loader. Air bases, logistics and index insertion are out
// of scope. --check compares the committed fixture without changing it.
import { fileURLToPath, pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { compileFunction } from 'node:vm';
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');
const root=resolve(process.env.WW2_REFERENCE??fileURLToPath(new URL('../reference/ww2-rts/',import.meta.url)));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',['-C',root,...args],{encoding:'utf8'}).trim();
if(git(['rev-parse','HEAD'])!==pin)throw Error(`Reference HEAD must be ${pin}`);
if(git(['status','--porcelain','--untracked-files=no']))throw Error('Reference has tracked changes');
const sim=await import(pathToFileURL(resolve(root,'shared/sim.js')));
const source=readFileSync(resolve(root,'shared/sim.js'),'utf8');
const between=(start,end)=>{const a=source.indexOf(start),b=source.indexOf(end,a);if(a<0||b<a)throw Error('Source boundaries changed');return source.slice(a,b);};
// Drop only the module export keyword so the unchanged bodies run in the fixture closure.
const code=between('// Shared scratch buffers','// Cells touching a wall').replace('export function regionsFor', 'function regionsFor');
const factory=compileFunction(code+'\nreturn nearestFree;', ['CELL','MOVE','LAND'],{filename:'reference-unit-spawn.js'});
const api={nearestFree:factory(sim.CELL,sim.MOVE,sim.LAND),spawnUnit:sim.internals.spawnUnit};
let state=0x51a7e;
function random(){state=(state+0x6d2b79f5)>>>0;let t=state;t=Math.imul(t^t>>>15,t|1);t^=t+Math.imul(t^t>>>7,t|61);return((t^t>>>14)>>>0)/4294967296;}
const pick=n=>Math.floor(random()*n),point=(x,z)=>({x,z});
const runs=array=>{const out=[];for(const v of array){if(out.length&&out.at(-1)[0]===v)out.at(-1)[1]++;else out.push([v,1]);}return out;};
const scenarios=[];
function scenario(name,w,h,flags,height,spawn=point(1,1),mode=null){
 const g={w,h,flags:Uint16Array.from(flags),height:height&&Int8Array.from(height),players:[{spawn},{spawn:point(w*2-1,h*2-1)}],units:new Map(),nextId:1,
  logisticsEnabled:false,infantryRegionVersion:0,vehicleRegionVersion:0,navalRegionVersion:0};
 if(mode)g.mode={kind:mode};
 const data={name,w,h,flags:runs(flags),height:height&&runs(height),spawns:g.players.map(p=>p.spawn),actions:[]};scenarios.push(data);
 if(mode)data.mode=mode;
 return {
  nearest(at,block,origin=null){data.actions.push({op:'nearest',at,block,origin,cell:api.nearestFree(g,at.x,at.z,block,origin)});},
  spawn(type,index,owner=0){const effective=index??g.units.size;const unit=api.spawnUnit(g,owner,type,index);data.actions.push({op:'spawn',type,index:effective,owner,unit});},
  patch(cells){for(const[c,flag,level]of cells){g.flags[c]=flag;if(g.height)g.height[c]=level;}g.infantryRegionVersion++;g.vehicleRegionVersion++;g.navalRegionVersion++;data.actions.push({op:'patch',cells});},
 };
}
const all=scenario('all-represented-non-air-defaults',17,17,Array(289).fill(0),Array(289).fill(0),point(16,16));
const supported=Object.keys(sim.UNITS).filter(k=>!sim.UNITS[k].air);
for(const type of supported){all.spawn(type);all.spawn(type,7,1);}
const modeUnits=Object.entries(sim.UNITS).filter(([,d])=>!d.air);
const paid=modeUnits.find(([,d])=>d.ab.id!=='none'&&sim.abCost({mode:{kind:'world'}},d.ab)>0)?.[0];
const free=modeUnits.find(([,d])=>d.ab.id!=='none'&&sim.abCost({mode:{kind:'world'}},d.ab)===0)?.[0];
const none=modeUnits.find(([,d])=>d.ab.id==='none')?.[0];
if(!paid||!free||!none)throw Error('Missing source ability-cost representatives');
for(const mode of['classic','world']){
 const current=scenario(`mode-control-defaults:${mode}`,17,17,Array(289).fill(0),Array(289).fill(0),point(16,16),mode);
 for(const [index,type]of[none,paid,free].entries())current.spawn(type,index);
}
for(const [name,w,h,flags,height,spawn]of[
 ['one-cell',1,1,[0],null,point(1,1)],
 ['fully-blocked',3,3,Array(9).fill(1|64|1024|2048),Array(9).fill(0),point(3,3)],
 ['disconnected-islands',7,5,Array.from({length:35},(_,c)=>c%7===3?1|64|1024:0),Array(35).fill(0),point(3,5)],
 ['diagonal-height-connectivity',2,2,[0,0,0,0],[0,2,2,1],point(1,1)],
 ['single-row',9,1,[0,0,1,1,0,0,0,0,0],null,point(1,1)],
 ['naval-origin-bypass',5,5,Array.from({length:25},(_,c)=>c%5===2?1024:1),null,point(1,5)],
]){
 const current=scenario(name,w,h,flags,height,spawn);
 for(const block of[0,1,1|64,1024,1024|2048])for(const at of[point(-5,-5),point(w*2+5,h*2+5),point(1,1),point(w*2-1,h*2-1)]){
  current.nearest(at,block);current.nearest(at,block,spawn);
 }
 for(const type of['rifle','tank','gunboat','destroyer'])for(const index of[0,1,2,7,99,1000])current.spawn(type,index);
 if(height){current.patch([[0,0,0],[flags.length-1,0,0]]);current.nearest(point(w*2-1,h*2-1),1,spawn);current.spawn('rifle',7);}
}
for(let i=0;i<15;i++){
 const w=7+pick(16),h=7+pick(16),n=w*h;
 const flags=Array.from({length:n},()=>random()<.2?[1,64,1024,2048][pick(4)]:0);
 const height=i%3?Array.from({length:n},()=>pick(4)-1):null;
 const spawn=point(random()*w*2,random()*h*2);const current=scenario(`seeded-${i}`,w,h,flags,height,spawn);
 for(let q=0;q<18;q++){
  current.nearest(point(random()*w*2,random()*h*2),[1,65,1024,3072][q%4],q%3?spawn:null);
  current.spawn(['rifle','tank','gunboat','destroyer'][q%4],pick(500));
 }
}
for(const filename of readdirSync(resolve(root,'maps')).filter(f=>f.endsWith('.json')).sort()){
 const map=JSON.parse(readFileSync(resolve(root,'maps',filename),'utf8'));let g;const old=Math.random;Math.random=random;
 try{g=sim.createGame(map,['One','Two'],false,[0,1],[0,1],{mode:'conquest',weather:false});}finally{Math.random=old;}
 const current=scenario(`map:${filename}`,g.w,g.h,g.flags,g.height,g.players[0].spawn);
 for(const type of['rifle','tank','gunboat','destroyer'])for(const index of[0,7])current.spawn(type,index);
}
const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,seed:'0x51a7e',supported,scenarios})+'\n';
if(process.argv.includes('--check')){if(output!==readFileSync(new URL('../rust/crates/simtest/tests/fixtures/unit-spawn.json',import.meta.url),'utf8'))throw Error('Spawn fixture differs');console.log('Spawn fixture matches');}
else process.stdout.write(output);
