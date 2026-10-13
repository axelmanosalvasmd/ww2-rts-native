// Capture unchanged source wreck collectors and snapshot list expressions.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
const root=fileURLToPath(new URL('../',import.meta.url)),ref=resolve(process.env.WW2_REFERENCE??resolve(root,'reference/ww2-rts'));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d',git=a=>execFileSync('git',['-C',ref,...a],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'')};clean();
const path=resolve(ref,'shared/sim.js');let code=readFileSync(path,'utf8');
const snapshot=code.slice(code.indexOf('export function snapshotFor('));
const found=snapshot.match(/foundMines: (\[\.\.\.g\.mineSeen\].*?),\n/)[1];
const ordinaryNodes=snapshot.match(/nodes: g\.mode\?\.kind === 'world' \? worldNodesFor\(g, slot\) : cache \? cache.nodes : (.*?), upkeep:/)[1];
code=code.replace(/from (['"])(\.\/[^'"]+)\1/g,(_,q,p)=>`from ${q}${new URL(p,pathToFileURL(path)).href}${q}`);
code+=`\nexport {rebuildGrid,observedPathView,worldPathView};\nexport function oracleLists(g,slot){const p=g.players[slot],r=rounded;return {wrecks:rememberedWrecksFor(g,slot),foundMines:${found},...(g.mode?.kind==='world'?{nodes:worldNodesFor(g,slot)}:g.nodes===undefined?{}:{nodes:${ordinaryNodes}})}};\n`;
const sim=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
const starts=JSON.parse(readFileSync(resolve(root,'rust/crates/simtest/tests/fixtures/world-start.json'),'utf8'));
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n,mix=x=>{x=((x^(x>>30n))*0xbf58476d1ce4e5b9n)&mask;x=((x^(x>>27n))*0x94d049bb133111ebn)&mask;return x^(x>>31n)};
const environmentSeed=seed=>Number(mix((mix(BigInt(seed)^((2n*gamma)&mask))+gamma)&mask)&0xffffffffn);
const plain={name:'Recipient lists',w:100,h:60,rows:Array(60).fill('.'.repeat(100)),spawns:[{x:0,y:0},{x:99,y:59}],points:[{x:50,y:30}]};
const cases=[];
function record(name,world,actions){
 const c=world?starts.cases[1]:{map:plain,seed:711,players:[{name:'P0',team:0,faction:0},{name:'P1',team:1,faction:1}],settings:{shuffle:false,army:'standard',living:false,weather:'map',tech:false},weatherSeed:1};
 const map=structuredClone(c.map);if(map.world?.regionMapRuns){map.world.regionMap=map.world.regionMapRuns.flatMap(([v,n])=>Array(n).fill(v));delete map.world.regionMapRuns}
 const old=Math.random;Math.random=()=>environmentSeed(c.seed)/4294967296;let g;
 try{g=sim.createGame(map,c.players.map(p=>p.name),c.settings.shuffle,c.players.map(p=>p.team),c.players.map(p=>p.faction),{mode:world?'world':undefined,army:c.settings.army,weather:c.settings.living?c.settings.weather:false,weatherSeed:c.weatherSeed,tech:c.settings.tech,logistics:false})}finally{Math.random=old}
 g.units.clear();sim.rebuildGrid(g);g.wrecks=[];g.mineSeen.clear();const aliases=new Map(),rows=[];
 for(const a of actions){let result=null;
  switch(a.op){
   case 'eye':{const u=sim.internals.spawnUnit(g,a.owner??0,'rifle');a.id=u.id;aliases.set(a.alias,u.id);Object.assign(u,{x:a.x,z:a.z});sim.rebuildGrid(g);break}
   case 'move':Object.assign(g.units.get(aliases.get(a.alias)),{x:a.x,z:a.z});sim.rebuildGrid(g);break;
   case 'vision':g.tick=a.tick;sim.internals.updateVision(g);break;
   case 'wreck':{const w={id:a.id,type:a.kind??'tank',owner:a.owner??1,rot:a.rot??-.26,x:a.x,z:a.z};if(a.moving)w.motion={id:`wreck:${a.id}`,kind:'wreck',x:a.x,y:.08,z:a.z,dir:.35,material:'steel',mass:8500,vx:.13,vy:-.02,vz:.27,gravity:9.81,drag:3.2,stamp:1.25,born:.5,tilt:.007,settled:false};const old=g.wrecks.find(w=>w.id===a.id);if(old)Object.assign(old,w,{motion:w.motion});else g.wrecks.push(w);break}
   case 'removeWreck':g.wrecks=g.wrecks.filter(w=>w.id!==a.id);break;
   case 'seen':g.mineSeen.set(a.cell,a.bits);break;
   case 'forget':g.mineSeen.delete(a.cell);break;
   case 'mine':sim.mutateWorldCell(g,a.cell,{mine:a.value});break;
   case 'reveal':g.reveal=a.value;break;
   case 'nodes':g.nodes=structuredClone(a.rows);break;
   case 'observe':world?sim.worldPathView(g,a.slot):sim.observedPathView(g,a.slot);break;
   case 'read':result=sim.oracleLists(g,a.slot);break;
   default:throw Error(a.op)
  }
  rows.push({action:a,result:structuredClone(result)});
 }
 cases.push({name,world,startIndex:world?1:null,...(world?{}:{start:c}),rows});
}
const eye={op:'eye',alias:'eye',x:41,z:41},vision=t=>({op:'vision',tick:t}),read=slot=>({op:'read',slot}),wreck=(id,x,z,moving=false)=>({op:'wreck',id,x,z,moving});
for(const world of [false,true]){
 record(`${world?'world':'ordinary'}-full-memory-motion-order`,world,[eye,vision(1),wreck(9002,43.14,41.26,true),wreck(9001,45.15,41.34),{op:'observe',slot:0},{op:'move',alias:'eye',x:world?901:181,z:world?901:101},vision(2),read(0),read(1),{op:'wreck',id:9002,x:43.44,z:41.56,moving:true,kind:'halftrack',owner:0,rot:1.36},read(0),{op:'wreck',id:9002,x:world?905:185,z:world?905:105,moving:true,kind:'halftrack',rot:1.36},read(0),{op:'removeWreck',id:9001},read(0),{op:'reveal',value:true},read(0),{op:'reveal',value:false},{op:'move',alias:'eye',x:41,z:41},vision(3),read(0),{op:'removeWreck',id:9002},read(0),wreck(9001,44.14,42.24),read(0),...(world?[{op:'wreck',id:9003,x:43.04,z:41.05,kind:'armoredcar',owner:-1,rot:-.04},read(0)]:[])]);
 const cells=world?[10262,10260,10261]:[422,420,421];
 record(`${world?'world':'ordinary'}-mine-discovery-privacy`,world,[...cells.map(cell=>({op:'mine',cell,value:true})),{op:'seen',cell:cells[0],bits:1},{op:'seen',cell:cells[1],bits:world?4:2},{op:'seen',cell:cells[2],bits:1},read(0),read(world?2:1),{op:'seen',cell:cells[0],bits:world?5:3},read(0),{op:'forget',cell:cells[0]},{op:'seen',cell:cells[0],bits:1},read(0),{op:'mine',cell:cells[2],value:false},read(0)]);
}
record('world-nodes-known-cells-reveal-order',true,[eye,vision(1),{op:'nodes',rows:[{c:10260,x:41.14,z:41.26,rate:7.125,fuel:false},{c:200000,x:worldX(200000),z:worldZ(200000),rate:3.5,fuel:true}]},read(0),read(2),{op:'move',alias:'eye',x:901,z:901},vision(2),read(0),{op:'reveal',value:true},read(0)]);
function worldX(c){return(c%512)*2+1}function worldZ(c){return Math.floor(c/512)*2+1}
clean();const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases},null,2)+'\n',out=resolve(root,'rust/crates/core/tests/fixtures/recipient-lists.json');
if(process.argv.includes('--check'))assert.equal(readFileSync(out,'utf8'),output);else writeFileSync(out,output);
console.log(`${process.argv.includes('--check')?'Verified':'Recorded'} ${cases.length} list traces, ${cases.reduce((n,c)=>n+c.rows.length,0)} actions`);
