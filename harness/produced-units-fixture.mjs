// Execute original stepOut and produceUnit. The loader only adds exports.
import assert from 'node:assert/strict';
import { readFileSync,readdirSync,writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath,pathToFileURL } from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim();
assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');
const url=pathToFileURL(`${reference}/shared/sim.js`).href;
registerHooks({load(path,context,next){const result=next(path,context);return path===url?{...result,source:`${result.source}\nexport { stepOut, produceUnit, rebuildGrid };\n`}:result;}});
const sim=await import(url);
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const point=(x,z)=>({x,z});
const runs=values=>{const out=[];for(const v of values){if(out.length&&out.at(-1)[0]===v)out.at(-1)[1]++;else out.push([v,1]);}return out;};
let seed=0x7239ca;
Math.random=()=>{let t=seed=(seed+0x6d2b79f5)>>>0;t=Math.imul(t^t>>>15,t|1);t^=t+Math.imul(t^t>>>7,t|61);return((t^t>>>14)>>>0)/4294967296;};
const supported=sim.UNIT_TYPES.filter(k=>!sim.UNITS[k].air);
const scenarios=[];
function scenario(name,w,h,flags,height,spawn=point(3,3),occupants=[]){
 const input={name,w,h,flags:runs(flags),height:height&&runs(height),spawn,occupants,actions:[]};scenarios.push(input);
 function game(mode){
  const chars=Array.from(flags,flag=>flag&sim.LAND?'W':flag&sim.MOVE?'B':'.');
  const units=new Map(occupants.map(row=>[row.id,{...row,air:row.air?{state:'base'}:undefined}]));
  return{w,h,flags:Uint16Array.from(flags),height:height&&Int8Array.from(height),chars,ground:chars,objects:Array(w*h).fill('.'),house:new Uint8Array(w*h),wear:new Float32Array(w*h),cellHp:new Float32Array(w*h),
   players:[{spawn,rally:null,faction:0,team:0}],units,nextId:10000,tick:37,logisticsEnabled:false,tech:false,mode:mode&&{kind:mode},navigationObserved:true,
   worldNearWalls:new Uint8Array(w*h),fires:new Map(),mineSeen:new Map(),wrecks:[],smokes:[],infantryRegionVersion:0,vehicleRegionVersion:0,navalRegionVersion:0};
 }
 return{
  step(type,door){const g=game('conquest'),u=sim.internals.spawnUnit(g,0,type,7);const before=JSON.stringify(occupants);sim.stepOut(g,u,door.at,door.radius,door.heading,door.origin);
   assert.equal(JSON.stringify(occupants),before);input.actions.push({op:'step',type,door,expected:[u.x,u.z,u.rot,u.aim,u.ghost].map(bits)});},
  produce(type,building,playerRally=null,mode='conquest'){const g=game(mode);g.players[0].rally=playerRally;const b={id:9999,type:building.type,owner:0,...building.at,rally:building.rally};
   const u=sim.produceUnit(g,b,0,type);input.actions.push({op:'produce',type,building,playerRally,mode,scatterIndex:occupants.length,expected:{id:u.id,hp:bits(u.hp),pose:[u.x,u.z,u.rot,u.aim,u.ghost].map(bits),rally:u.worldGoal??null,drift:u.drift==='rally'}});},
 };
}
const open=scenario('all-ground-types',25,25,Array(625).fill(0),null,point(7,7));
for(const type of supported){open.step(type,{at:point(25,25),radius:4,heading:0.3,origin:null});open.produce(type,{type:'barracks',at:point(25,25),rally:null},point(40,35));}
for(const mode of ['conquest','assault','annihilation','horde','classic','world','tutorial',null])for(const type of ['rifle','tank','bunker'])for(const rally of[null,point(5,30)])open.produce(type,{type:'motorpool',at:point(20,20),rally},point(40,35),mode);
for(const[name,w,h,flags,height,spawn]of[
 ['one-cell',1,1,[0],null,point(1,1)],
 ['blocked',5,5,Array(25).fill(sim.MOVE|sim.VBLOCK|sim.LAND|sim.SHOAL),null,point(3,3)],
 ['river',15,15,Array.from({length:225},(_,c)=>c%15===7?sim.TERRAIN.W:sim.TERRAIN['.']),null,point(3,3)],
 ['islands',9,9,Array.from({length:81},(_,c)=>c%9===4?sim.MOVE|sim.VBLOCK:0),null,point(3,3)],
 ['cliff',7,7,Array(49).fill(0),Array.from({length:49},(_,c)=>c%7>=4?3:0),point(3,3)],
 ['diagonal',2,2,[0,0,0,0],[0,2,2,1],point(1,1)],
 ['narrow-door',13,13,Array.from({length:169},(_,c)=>c%13<5||c%13>7?sim.MOVE|sim.VBLOCK:0),null,point(13,13)],
]){
 const current=scenario(name,w,h,flags,height,spawn);
 for(const type of ['rifle','tank','medium','tiger','gunboat','destroyer','train','kaiju'])for(const origin of[null,spawn])for(const heading of[0,Math.PI,-1.2])current.step(type,{at:point(w,h),radius:3,heading,origin});
}
const occupants=[];
for(let id=1;id<=25;id++)occupants.push({id,type:id%3?'tank':'rifle',owner:0,x:25+(id%5-2)*2,z:25+(Math.floor(id/5)-2)*2,hp:100,riding:0,air:false});
occupants.push({id:40,type:'tank',owner:0,x:25,z:25,hp:0,riding:0,air:false},{id:41,type:'rifle',owner:0,x:25,z:25,hp:100,riding:1,air:false},{id:42,type:'rifle',owner:0,x:25,z:25,hp:100,riding:0,air:true});
const crowded=scenario('crowded-rings',25,25,Array(625).fill(0),null,point(5,5),occupants);
for(const type of supported)crowded.step(type,{at:point(25,25),radius:0,heading:-0.6,origin:point(5,5)});
const fromBuilding=scenario('occupied-production-building',25,25,Array(625).fill(0),null,point(5,5),[{id:9999,type:'hq',owner:0,x:25,z:25,hp:3000,riding:0,air:false}]);
for(const type of supported)for(const rally of[null,point(10,35)])fromBuilding.produce(type,{type:'hq',at:point(25,25),rally},point(40,35));
for(const filename of readdirSync(`${reference}/maps`).filter(f=>f.endsWith('.json')).sort()){
 const map=JSON.parse(readFileSync(`${reference}/maps/${filename}`,'utf8'));
 const g=sim.createGame(map,['One','Two'],false,[0,1],[0,1],{mode:'conquest',weather:false,logistics:false});
 const current=scenario(`map:${filename}`,g.w,g.h,g.flags,g.height,g.players[0].spawn);
 for(const type of['rifle','tank','medium','gunboat','destroyer']){current.step(type,{at:g.players[0].spawn,radius:4,heading:0.7,origin:g.players[0].spawn});current.produce(type,{type:'hq',at:g.players[0].spawn,rally:null});}
}
const text=JSON.stringify({reference:pin,node:process.version,arch:process.arch,supported,tick:37,scenarios})+'\n';
const destination=`${root}/rust/crates/core/tests/fixtures/produced-units.json`;
if(process.argv.includes('--check'))assert.equal(readFileSync(destination,'utf8'),text);else writeFileSync(destination,text);
console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} ${scenarios.length} terrains, ${scenarios.reduce((n,s)=>n+s.actions.length,0)} placement/production cases`);
