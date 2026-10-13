// Original observed memory and route collectors. Visibility is supplied as
// explicit team facts; no fog implementation is copied into this oracle.
import { readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath,pathToFileURL } from 'node:url';
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');
const ref=resolve(process.env.WW2_REFERENCE??fileURLToPath(new URL('../reference/ww2-rts/',import.meta.url)));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',ref,...a],{encoding:'utf8'}).trim();
if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Use clean pinned reference');
const path=resolve(ref,'shared/sim.js');let source=readFileSync(path,'utf8');
// Stop only after the real map/structural initialization, before unrelated units
// and match-mode setup. Expose private functions without changing their bodies.
const marker='  setupStructures(g, map);';if(source.split(marker).length!==2)throw Error('Initialization marker changed');
source=source.replace(marker,marker+'\n  return g;').replace(/from (['"])(\.\/[^'"]+)\1/g,(_,q,p)=>`from ${q}${new URL(p,pathToFileURL(path)).href}${q}`);
source+='\nexport {observedTerrainFor,observedPathView,logCell};\nexport const setOracleVisibility=(callback)=>{teamSees=callback;};\n';
const sim=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
const cases=[];let visibility=[[],[]];
sim.setOracleVisibility((g,team,at)=>visibility[team]?.includes(Math.floor(at.z/2)*g.w+Math.floor(at.x/2))??false);
const bit=x=>{const a=Buffer.alloc(8);a.writeDoubleBE(x);return a.toString('hex');};
function scenario(name,map,actions){
 const old=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(map,[],false,[],[],{weather:false});}finally{Math.random=old;}
 g.players=[0,1].map((slot)=>({slot,team:slot,visible:new Set(),wreckMemory:new Map()}));
 g.visionTick=0;g.tick=0;g.units.clear();g.buildingCells=new Map();visibility=[[],[]];
 const snapshots=[],revisions=new WeakMap();let nextRevision=[0,0];
 function snapshot(){return {memories:g.players.map((p,slot)=>{
  const rows=[...(p.terrainMemory?.values()??[])];for(const r of rows)if(!revisions.has(r))revisions.set(r,++nextRevision[slot]);
  return {rows:rows.map(r=>({row:r,revision:revisions.get(r)})),pending:[...(p.terrainPending??[])].sort((a,b)=>a-b)};
 }),versions:[g.tick,g.terrainVersion??0,g.visionTick],reveal:!!g.reveal};}
 for(const a of actions){let result=null;
  switch(a.op){
   case 'set':sim.internals.setCell(g,a.c,a.ch);break;
   case 'mutate':sim.mutateWorldCell(g,a.c,a.change);break;
   case 'vision':g.visionTick=a.tick;visibility[a.team]=a.cells;break;
   case 'tick':g.tick=a.tick;break;
   case 'log':sim.logCell(g,a.c,[],a.look??false);break;
   case 'mine':if(a.owner!==undefined)g.mines.set(a.c,a.owner);if(a.seen!==undefined)g.mineSeen.set(a.c,a.seen);break;
   case 'footprint':{
    const u={id:a.identity,owner:a.owner,type:'hq',hp:1,x:(a.cells[0]%g.w+.5)*2,z:(Math.floor(a.cells[0]/g.w)+.5)*2};
    if(a.live)g.units.set(u.id,u);else g.units.delete(u.id);
    for(const c of a.cells)g.buildingCells.set(c,u);
    for(let i=0;i<2;i++)if(a.visibleSlots?.includes(i))g.players[i].visible.add(u.id);else g.players[i].visible.delete(u.id);
    break;
   }
   case 'clearFootprint':for(const c of a.cells)g.buildingCells.delete(c);break;
   case 'reveal':g.reveal=a.value;break;
   case 'skipFog':g.skipFog=a.value;break;
   case 'team':g.players[a.slot].team=a.team;break;
   case 'read':result=sim.observedTerrainFor(g,a.slot,a.full);break;
   case 'route':{
    for(const p of g.players)g.fog??=new Map();
    for(const p of g.players)g.fog.set(p.team,{key:g.visionTick,vis:new Uint8Array(g.w*g.h),visibleCells:[]});
    const view=sim.observedPathView(g,a.slot);
    result={chars:view.chars.join(''),flags:[...view.flags],height:[...view.height],mines:[...view.mineSeen.keys()],version:view.navigationKnowledgeVersion,
      changes:view.navigationChanges.map(v=>[v.from,v.to,v.cells]),hp:[...view.cellHp].map(bit)};
    break;
   }
   case 'reconnect':result=sim.observedTerrainFor(g,a.slot,true);break;
   default:throw Error(a.op);
  }
  snapshots.push({result:result===null?null:structuredClone(result),...snapshot()});
 }
 cases.push({name,map,actions,snapshots});
}
const blank=()=>({name:'Memory',w:20,h:20,rows:Array(20).fill('.'.repeat(20)),spawns:[{x:0,y:0},{x:19,y:19}],points:[{x:10,y:10}]});
scenario('ordered-outbox-and-hidden-mines',blank(),[
 {op:'route',slot:0},{op:'read',slot:1,full:true},
 {op:'set',c:30,ch:'T'},{op:'set',c:2,ch:'M'},{op:'vision',team:0,tick:1,cells:[2,30]},
 {op:'route',slot:0},{op:'read',slot:0,full:false},{op:'read',slot:0,full:false},
 {op:'set',c:30,ch:'B'},{op:'set',c:2,ch:'D'},{op:'set',c:40,ch:'X'},{op:'vision',team:0,tick:2,cells:[2,30,40]},
 {op:'read',slot:0,full:true},{op:'read',slot:0,full:false},{op:'route',slot:0},
 {op:'set',c:2,ch:'T'},{op:'read',slot:0,full:true},{op:'set',c:30,ch:'.'},{op:'read',slot:0,full:true},{op:'read',slot:0,full:false},
 {op:'mutate',c:10,change:{mine:true,material:'steel'}},{op:'vision',team:0,tick:3,cells:[10]},
 {op:'route',slot:0},{op:'mine',c:10,seen:1},{op:'log',c:10,look:true},{op:'route',slot:0},
 {op:'vision',team:0,tick:4,cells:[10]},{op:'tick',tick:1},{op:'route',slot:0},{op:'read',slot:0,full:false},
 {op:'vision',team:0,tick:5,cells:[]},{op:'mutate',c:10,change:{mine:false}},{op:'route',slot:0},{op:'read',slot:0,full:false},
 {op:'vision',team:0,tick:6,cells:[10]},{op:'tick',tick:2},{op:'route',slot:0},{op:'read',slot:0,full:false},
 {op:'mutate',c:11,change:{mine:true}},{op:'mine',c:11,owner:0},{op:'vision',team:0,tick:7,cells:[11]},
 {op:'tick',tick:3},{op:'route',slot:0},{op:'reconnect',slot:0},{op:'read',slot:1,full:true},
 {op:'vision',team:1,tick:8,cells:[2,30,10,11]},{op:'route',slot:1},{op:'read',slot:1,full:false},
 {op:'team',slot:1,team:0},{op:'log',c:11,look:false},{op:'tick',tick:4},{op:'route',slot:1},
 {op:'reveal',value:true},{op:'log',c:40,look:false},{op:'route',slot:1},{op:'read',slot:1,full:false},
]);
scenario('footprints-allied-unseen-removal-and-cancel',blank(),[
 {op:'read',slot:0,full:true},{op:'set',c:17,ch:'K'},{op:'set',c:18,ch:'K'},
 {op:'footprint',identity:5,owner:1,live:true,cells:[17,18],visibleSlots:[]},{op:'vision',team:0,tick:1,cells:[17,18]},
 {op:'read',slot:0,full:true},{op:'footprint',identity:5,owner:1,live:true,cells:[17,18],visibleSlots:[0]},
 {op:'read',slot:0,full:true},{op:'vision',team:0,tick:2,cells:[17,18]},{op:'route',slot:0},{op:'read',slot:0,full:false},
 {op:'footprint',identity:5,owner:1,live:false,cells:[17,18],visibleSlots:[]},{op:'vision',team:0,tick:3,cells:[]},
 {op:'set',c:17,ch:'.'},{op:'set',c:18,ch:'.'},{op:'read',slot:0,full:true},
 {op:'reveal',value:true},{op:'log',c:17,look:false},{op:'read',slot:0,full:true},
 {op:'vision',team:0,tick:4,cells:[17]},{op:'route',slot:0},{op:'read',slot:0,full:false},
 {op:'set',c:22,ch:'K'},{op:'footprint',identity:6,owner:0,live:true,cells:[22],visibleSlots:[]},
 {op:'vision',team:0,tick:5,cells:[]},{op:'route',slot:0},{op:'read',slot:0,full:false},
 {op:'reveal',value:false},{op:'set',c:25,ch:'K'},{op:'footprint',identity:7,owner:1,live:true,cells:[25],visibleSlots:[0]},
 {op:'tick',tick:1},{op:'route',slot:0},{op:'set',c:25,ch:'.'},{op:'clearFootprint',cells:[25]},
 {op:'tick',tick:2},{op:'route',slot:0},{op:'skipFog',value:true},{op:'log',c:25,look:false},{op:'route',slot:0},{op:'read',slot:0,full:false},
]);
scenario('hidden-mine-physical-change-and-metadata',blank(),[
 {op:'read',slot:0,full:true},{op:'mutate',c:51,change:{ground:'M',mine:true,material:'steel'}},
 {op:'vision',team:0,tick:1,cells:[51]},{op:'route',slot:0},{op:'read',slot:0,full:false},
 {op:'log',c:51,look:false},{op:'read',slot:0,full:true},
 {op:'mutate',c:51,change:{material:'wood'}},{op:'read',slot:0,full:true},
 {op:'mutate',c:51,change:{height:2}},{op:'read',slot:0,full:true},{op:'route',slot:0},
 {op:'mine',c:51,seen:1},{op:'log',c:51,look:true},{op:'vision',team:0,tick:2,cells:[51]},
 {op:'route',slot:0},{op:'read',slot:0,full:false},{op:'read',slot:0,full:true},
 {op:'mutate',c:51,change:{mine:false}},{op:'route',slot:0},{op:'read',slot:0,full:false},
]);
const out=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases})+'\n';
if(process.argv.includes('--check')){if(out!==readFileSync(new URL('../rust/crates/core/tests/fixtures/observed-memory.json',import.meta.url),'utf8'))throw Error('Observed-memory fixture differs');console.log('Observed-memory fixture matches');}else process.stdout.write(out);
