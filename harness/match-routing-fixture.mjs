// Actual Conquest collectors and visibility. The row probe only records identity.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');
const ref=resolve(process.env.WW2_REFERENCE??fileURLToPath(new URL('../reference/ww2-rts/',import.meta.url))),pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',ref,...a],{encoding:'utf8'}).trim();
if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Use clean pinned reference');
const path=resolve(ref,'shared/sim.js');let source=readFileSync(path,'utf8');
const probe='const known = [...cell]; memory.set(c, known);';
if(source.split(probe).length!==2)throw Error('Row probe changed');
source=source.replace(probe,'const known = [...cell]; oracleRow(slot, known); memory.set(c, known);');
source=source.replace(/from (['"])(\.\/[^'"]+)\1/g,(_,q,p)=>`from ${q}${new URL(p,pathToFileURL(path)).href}${q}`);
source+='\nexport {observedTerrainFor,observedPathView,rebuildGrid,logCell};\nexport let oracleRow=()=>{}; export const setRowProbe=f=>{oracleRow=f};\n';
const sim=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
const bits=x=>{const b=Buffer.alloc(8);b.writeDoubleBE(x);return b.toString('hex')};
const runs=vs=>{const out=[];for(const v of vs){if(out.length&&JSON.stringify(out.at(-1)[0])===JSON.stringify(v))out.at(-1)[1]++;else out.push([v,1]);}return out;};
const blank=()=>({name:'Observed routing',w:40,h:40,rows:Array(40).fill('.'.repeat(40)),spawns:[{x:3,y:3},{x:36,y:36}],points:[{x:20,y:20}]});
const cases=[];
function record(name,map,actions){
 const old=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(map,['A','B'],false,[0,1],[0,1],{mode:'conquest',army:'standard',weather:false,tech:false,logistics:false})}finally{Math.random=old;}
 const revisions=new WeakMap(),counts=[0,0],aliases=new Map(),snapshots=[];
 sim.setRowProbe((slot,row)=>revisions.set(row,++counts[slot]));
 const get=a=>g.units.get(aliases.get(a.alias)??a.id);
 for(const a of actions){let result=null;
  switch(a.op){
   case 'clearUnits':g.units.clear();sim.rebuildGrid(g);break;
   case 'spawn':{const u=sim.internals.spawnUnit(g,a.owner,a.kind);a.id=u.id;aliases.set(a.alias,u.id);Object.assign(u,{x:a.x,z:a.z,...a.fields});if(a.air)u.air.state=a.air;sim.rebuildGrid(g);break;}
   case 'move':{const u=get(a);Object.assign(u,{x:a.x,z:a.z,...a.fields});sim.rebuildGrid(g);break;}
   case 'remove':g.units.delete(aliases.get(a.alias)??a.id);sim.rebuildGrid(g);break;
   case 'set':sim.internals.setCell(g,a.c,a.ch);break;
   case 'mutate':sim.mutateWorldCell(g,a.c,a.change);break;
   case 'log':sim.logCell(g,a.c,[],a.look??false);break;
   case 'vision':g.tick=a.tick;sim.internals.updateVision(g);break;
   case 'tick':g.tick=a.tick;break;
   case 'fire':a.value?g.fires.set(a.c,20):g.fires.delete(a.c);break;
   case 'smoke':g.smokes=a.values;break;
   case 'weather':g.weather.now=a.name;break;
   case 'mine':if(a.owner!==undefined)g.mines.set(a.c,a.owner);if(a.seen!==undefined)g.mineSeen.set(a.c,a.seen);break;
   case 'footprint':{const u=get(a);u.cells=a.cells;for(const c of a.cells)g.buildingCells.set(c,u);break;}
   case 'clearFootprint':for(const c of a.cells)g.buildingCells.delete(c);break;
   case 'visible':g.players[a.slot].visible=new Set(a.aliases.map(k=>aliases.get(k)));break;
   case 'team':g.players[a.slot].team=a.team;break;
   case 'reveal':g.reveal=a.value;break;
   case 'read':result=sim.observedTerrainFor(g,a.slot,a.full);break;
   case 'route':{
    const view=sim.observedPathView(g,a.slot);
    result={chars:view.chars.join(''),flags:runs([...view.flags]),height:runs([...view.height]),hp:runs([...view.cellHp].map(bits)),version:view.navigationKnowledgeVersion,changes:view.navigationChanges.map(v=>[v.from,v.to,v.cells]),fires:[...view.fires.keys()].sort((a,b)=>a-b),bunkers:[...view.units.values()].filter(u=>u.type==='bunker'&&u.hp>0).map(u=>[bits(u.x),bits(u.z)])};
    if(a.path){const u=get(a.path),path=sim.findPath(view,u,a.path.goal);result.path=path.map(p=>[bits(p.x),bits(p.z)]);result.end=[bits(u.routeEnd.x),bits(u.routeEnd.z)];}
    break;
   }
   default:throw Error(a.op);
  }
  snapshots.push({result:structuredClone(result),memories:g.players.map((p,slot)=>({rows:[...(p.terrainMemory?.values()??[])].map(row=>({row,revision:revisions.get(row)})),pending:[...(p.terrainPending??g.cellLog.keys())].sort((a,b)=>a-b)})),visible:g.players.map(p=>[...p.visible]),versions:[g.tick,g.terrainVersion??0,g.visionTick??null]});
 }
 cases.push({name,map,actions,snapshots});
}
record('current-position-discovery-and-cached-fire-fog',blank(),[
 {op:'clearUnits'},{op:'spawn',alias:'eye',owner:0,kind:'rifle',x:11,z:11},{op:'route',slot:0},{op:'read',slot:0,full:false},
 {op:'set',c:206,ch:'B'},{op:'set',c:207,ch:'X'},{op:'fire',c:207,value:true},{op:'route',slot:0},{op:'read',slot:0,full:true},
 {op:'move',alias:'eye',x:61,z:61},{op:'set',c:1230,ch:'T'},{op:'fire',c:1230,value:true},{op:'route',slot:0},{op:'read',slot:0,full:false},
 {op:'vision',tick:1},{op:'route',slot:0},{op:'read',slot:0,full:true},
 {op:'smoke',values:[{x:61,z:66,r:4,t:12}]},{op:'set',c:1470,ch:'M'},{op:'route',slot:0},
 {op:'spawn',alias:'hq-eye',owner:0,kind:'hq',x:61,z:61},{op:'log',c:1470},{op:'route',slot:0},{op:'read',slot:0,full:false},
 {op:'remove',alias:'hq-eye'},{op:'remove',alias:'eye'},{op:'smoke',values:[]},
 {op:'spawn',alias:'tower',owner:0,kind:'tower',x:41,z:41,fields:{built:.5}},{op:'set',c:834,ch:'D'},{op:'route',slot:0},
 {op:'move',alias:'tower',x:41,z:41,fields:{built:1}},{op:'log',c:834},{op:'route',slot:0},
 {op:'weather',name:'fog'},{op:'set',c:838,ch:'M'},{op:'route',slot:0},{op:'weather',name:'clear'},{op:'log',c:838},{op:'route',slot:0},
 {op:'remove',alias:'tower'},{op:'spawn',alias:'plane',owner:0,kind:'fighter',x:41,z:41,air:'base'},
 {op:'set',c:830,ch:'T'},{op:'route',slot:0},{op:'move',alias:'plane',x:41,z:41,fields:{}},{op:'remove',alias:'plane'},
 {op:'spawn',alias:'plane',owner:0,kind:'fighter',x:41,z:41,air:'station'},{op:'log',c:830},{op:'route',slot:0},{op:'read',slot:0,full:false},
]);
record('ordered-memory-mines-footprints-and-owner-binding',blank(),[
 {op:'clearUnits'},{op:'spawn',alias:'eye',owner:0,kind:'rifle',x:5,z:5},
 {op:'set',c:1200,ch:'T'},{op:'set',c:20,ch:'M'},{op:'route',slot:0},
 {op:'move',alias:'eye',x:1,z:61},{op:'vision',tick:1},{op:'route',slot:0},
 {op:'spawn',alias:'other-eye',owner:0,kind:'rifle',x:5,z:5},{op:'set',c:1200,ch:'B'},{op:'set',c:20,ch:'D'},
 {op:'route',slot:0},{op:'read',slot:0,full:false},{op:'read',slot:0,full:true},
 {op:'mutate',c:82,change:{mine:true}},{op:'route',slot:0},{op:'read',slot:0,full:false},{op:'mine',c:82,seen:1},{op:'log',c:82,look:true},{op:'route',slot:0},
 {op:'vision',tick:2},{op:'route',slot:0},{op:'read',slot:0,full:false},
 {op:'move',alias:'other-eye',x:79,z:79},{op:'mutate',c:82,change:{mine:false}},{op:'route',slot:0},
 {op:'spawn',alias:'enemy',owner:1,kind:'hq',x:1,z:57},{op:'set',c:1120,ch:'K'},{op:'set',c:1121,ch:'K'},
 {op:'footprint',alias:'enemy',cells:[1120,1121]},{op:'visible',slot:0,aliases:[]},{op:'route',slot:0},
 {op:'visible',slot:0,aliases:['enemy']},{op:'vision',tick:3},{op:'route',slot:0},{op:'read',slot:0,full:false},
 {op:'remove',alias:'enemy'},{op:'move',alias:'eye',x:79,z:79},{op:'set',c:1120,ch:'.'},{op:'route',slot:0},
 {op:'reveal',value:true},{op:'log',c:1120},{op:'route',slot:0},{op:'reveal',value:false},
 {op:'move',alias:'eye',x:1,z:57},{op:'vision',tick:4},{op:'route',slot:0},{op:'read',slot:0,full:false},
 {op:'set',c:1121,ch:'.'},{op:'clearFootprint',cells:[1120,1121]},{op:'route',slot:0},
 {op:'mutate',c:1122,change:{mine:true}},{op:'mine',c:1122,owner:0},{op:'route',slot:0},
 {op:'route',slot:1},{op:'team',slot:1,team:0},{op:'log',c:1122},{op:'route',slot:1},{op:'read',slot:1,full:true},
 {op:'spawn',alias:'post',owner:1,kind:'bunker',x:61,z:61},{op:'visible',slot:0,aliases:[]},{op:'tick',tick:5},{op:'route',slot:0},
 {op:'move',alias:'post',x:65,z:61,fields:{}},{op:'route',slot:0},{op:'spawn',alias:'new-post',owner:0,kind:'bunker',x:69,z:61},{op:'route',slot:0},{op:'move',alias:'post',x:65,z:61,fields:{hp:0}},{op:'remove',alias:'post'},{op:'route',slot:0},{op:'tick',tick:6},{op:'route',slot:0},
 {op:'read',slot:0,full:true},{op:'read',slot:0,full:false},
]);
const out=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases})+'\n';
const target=new URL('../rust/crates/core/tests/fixtures/match-routing.json',import.meta.url);
if(process.argv.includes('--check')){if(out!==readFileSync(target,'utf8'))throw Error('Match routing fixture differs');console.log('Match routing fixture matches')}else writeFileSync(target,out);
