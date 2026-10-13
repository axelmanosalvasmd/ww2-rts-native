// Record the original recipient-view helpers without changing the pinned tree.
import { readFileSync, readdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');
const reference=resolve(process.env.WW2_REFERENCE??fileURLToPath(new URL('../reference/ww2-rts/',import.meta.url)));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
if(git(['rev-parse','HEAD'])!==pin)throw Error('Wrong reference pin');
if(git(['status','--porcelain','--untracked-files=no']))throw Error('Reference has tracked changes');
const simPath=resolve(reference,'shared/sim.js');
let source=readFileSync(simPath,'utf8');
source=source.replace(/from (['"])(\.\/[^'"]+)\1/g,(_,q,p)=>`from ${q}${new URL(p,pathToFileURL(simPath)).href}${q}`);
// Only export visibility changes; helper bodies and their calls remain original.
source+='\nexport {worldPathView,observedPathView,coverQ,coverRank,maxHp,startWear};\n';
const sim=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
const vehicle=await import(pathToFileURL(resolve(reference,'shared/vehicle-motion.js')));
const b=x=>{const a=Buffer.alloc(8);a.writeDoubleBE(x);return a.toString('hex');};
const runs=values=>{const out=[];for(const value of values){if(out.length&&JSON.stringify(out.at(-1)[0])===JSON.stringify(value))out.at(-1)[1]++;else out.push([value,1]);}return out;};
const pt=(x,z)=>({x,z});
const cases=[];
function initial(w,h){const n=w*h;return {width:w,height:h,chars:Array(n).fill('.'),ground:Array(n).fill('.'),objects:Array(n).fill('.'),heights:Array(n).fill(0),shoals:Array(n).fill(false),houseKinds:Array(n).fill(0),toughness:Array(n).fill(1),trenchFront:Array(n).fill(null)};}
function row(cell,revision,character,height=0,state=0,metadata={}){return {cell,revision,character,height,state,metadata,coverBasis:{houseKind:0,toughness:1}};}
function scenario(name,mode,input,events){
 const {width:w,height:h}=input,n=w*h,world=mode==='World';
 const initialTerrain={chars:[...input.chars],ground:[...input.ground],objects:[...input.objects],height:Int8Array.from(input.heights),trenchFront:Float32Array.from(input.trenchFront,v=>v??NaN)};
 const memory={cells:new Map(),regions:new Map(),wrecks:new Map(),version:0,log:[],logBase:0};
 const players=[{team:0,visible:new Set(),terrainMemory:new Map(),terrainPending:new Set(),wreckMemory:new Map()}];
 const g={w,h,initialTerrain,house:Uint8Array.from(input.houseKinds),tough:Float64Array.from(input.toughness),shoal:Uint8Array.from(input.shoals),
   chars:Array(n).fill('W'),ground:Array(n).fill('W'),objects:Array(n).fill('B'),flags:Uint16Array.from({length:n},()=>sim.MOVE|sim.SIGHT),height:Int8Array.from({length:n},()=>7),
   players,units:new Map(),wrecks:[],fires:new Map(),fog:new Map(),world:world?{memory:new Map([[0,memory]])}:undefined,mode:world?{kind:'world'}:null,
   cellLog:[],tick:0,visionTick:0,terrainVersion:0,wx:{wet:.2},roads:true,reveal:false};
 const objects=new Map(),revisions=new WeakMap(),snapshots=[];
 for(const event of events){
   g.tick++;g.visionTick++;g.terrainVersion++;
   g.fog.set(0,{key:g.visionTick,vis:new Uint8Array(n),visibleCells:[]});
   const memoryRows=world?memory.cells:players[0].terrainMemory;
   for(const data of event.rows??[]){
     const key=`${data.cell}:${data.revision}`;
     let value=objects.get(key);
     if(!value){value=[];objects.set(key,value);revisions.set(value,data.revision);}
     value.splice(0,value.length,data.cell,data.character,data.height,data.state,structuredClone(data.metadata));
     memoryRows.set(data.cell,value);memory.log.push(data.cell);
     if(data.coverBasis){g.house[data.cell]=data.coverBasis.houseKind??0;g.tough[data.cell]=data.coverBasis.toughness;}
   }
   memory.version=event.version??memory.version+(event.rows??[]).length;
   memory.key=`${g.visionTick}:${g.terrainVersion}:0`;
   if(event.trim){memory.logBase+=memory.log.length;memory.log=[];}
   const explicit=world?(memory.pathAt>=memory.logBase?memory.log.slice(memory.pathAt-memory.logBase).map(c=>memory.cells.get(c)):[...memory.cells.values()]):[...memoryRows.values()];
   const lookup=new Map((event.rows??[]).map(r=>[`${r.cell}:${r.revision}`,r]));
   const suppliedRows=explicit.map(r=>({cell:r[0],revision:revisions.get(r),character:r[1],height:r[2],state:r[3],metadata:r[4],coverBasis:lookup.get(`${r[0]}:${revisions.get(r)}`)?.coverBasis??null}));
   // The original collectors see the supplied memory as already current. Live
   // arrays contain no units, wrecks or fires unless this event supplies them.
   g.wrecks=(event.wrecks??[]).map(w=>({...w,type:'tank',id:1,owner:0,rot:0}));
   players[0].wreckMemory=new Map();memory.wrecks=new Map();
   g.fires=new Map((event.fires??[]).map(c=>[c,1]));
   for(const c of event.fires??[])g.fog.get(0).vis[c]=1;
   if(event.hiddenChange){g.flags.fill(0);g.height.fill(-9);g.chars.fill('#');}
   const view=world?sim.worldPathView(g,0):sim.observedPathView(g,0);
   const known=world?new Set(memoryRows.keys()):new Set(Array.from({length:n},(_,i)=>i));
   const cells=Array.from({length:n},(_,c)=>[known.has(c),view.chars[c],view.ground[c],view.objects[c],view.flags[c],view.height[c],b(view.wear[c]),view.worldNearWalls[c],view.mineLayer[c]?((view.mineSeen.get(c)??0)?false:true):null,b(view.cellHp[c]),b(view.trenchFront[c]),sim.worldMaterial(view,c).name]);
   const paths=(event.paths??(name.startsWith('crafted')?[['rifle',pt(3,9),pt(21,17)],['tank',pt(3,9),pt(21,17)],['gunboat',pt(3,9),pt(21,17)]]:[])).map(([type,from,to])=>{
    const def=sim.UNITS[type],search=vehicle.isGroundVehicle(def)?vehicle.vehicleNavigationView(view,def,sim.blockOf(def)):view;
    search.pathStats={calls:0,deferred:0,failed:0,dropped:0,expansions:0,regionRejected:0};
    const unit={id:1,owner:0,type,...from},path=sim.findPath(search,unit,to),stats=search.pathStats;
    return {type,from,to,expected:{points:path.map(p=>[b(p.x),b(p.z)]),routeEnd:[b(unit.routeEnd.x),b(unit.routeEnd.z)],expansions:stats.expansions,status:stats.deferred?'ExpansionLimit':stats.failed?'Unreachable':'Found'}};
   });
   const coverView={...view,wrecks:(event.wrecks??[]).map(w=>({x:w.x,z:w.z,motion:w.moving?{}:undefined}))};
   const queries=(event.queries??[]).map(q=>({...q,expected:{cover:b(sim.coverQ(coverView,q.cell)),rank:sim.coverRank(coverView,q.at,q.from),behind:b(sim.coverBehind(coverView,q.at,q.from,false)),material:sim.worldMaterial(view,q.cell).name,groundMaterial:sim.worldMaterial(view,q.cell,true).name}}));
   snapshots.push({version:memory.version,rows:suppliedRows,fires:event.fires??[],wrecks:event.wrecks??[],queries,paths,expected:{cells:runs(cells),version:view.navigationKnowledgeVersion??0,changes:(view.navigationChanges??[]).map(c=>[c.from,c.to,[...c.cells]]),direct:view.navigationChangedCells?[view.navigationChangedFromVersion,view.navigationKnowledgeVersion,[...view.navigationChangedCells]]:null}});
 }
 cases.push({name,mode,initial:input,snapshots});
}
const base=initial(13,11);
for(const [c,ch] of [[15,'N'],[16,'B'],[17,'M'],[18,'F'],[19,'+'],[20,'O'],[30,'T']]){base.chars[c]=ch;base.ground[c]=ch==='N'?'D':'.';base.objects[c]=['N','M','F','+'].includes(ch)?'.':ch;if(['M','F','+'].includes(ch))base.ground[c]=ch;}
base.shoals[15]=true;base.toughness[16]=1.8;base.trenchFront[30]=.3;
const queries=[{cell:16,at:pt(7,3),from:pt(9,3)},{cell:19,at:pt(13,3),from:pt(13,1)},{cell:20,at:pt(15,3),from:pt(17,3)},{cell:30,at:pt(9,5),from:pt(11,5)}];
for(const mode of ['Observed','World']){
 const data=structuredClone(base),n=data.width*data.height;
 const sequence=[{rows:[],queries:mode==='Observed'?queries:[]},
  {rows:[row(15,1,'N',0,1,{ground:'D',object:'.',mineOwned:false}),row(16,1,'B',0,8,{ground:'.',object:'B',material:'stone',section:{hp:70,maxHp:150,state:'intact',material:'wood'}}),row(19,1,'+',0,3,{ground:'+',object:'.'}),row(20,1,'O',0,0,{ground:'.',object:'O'}),row(30,1,'T',0,0,{ground:'.',object:'T',trenchFront:.3})],queries},
  {rows:[row(16,2,'B',0,16,{ground:'.',object:'B',section:{hp:17,maxHp:150,state:'failed',material:'steel'}})],queries},
  {rows:[row(16,3,'#',0,8,{ground:'.',object:'#'}),row(15,2,'N',0,1,{mineOwned:true})],queries},
  {rows:[row(16,4,'#',0,16,{ground:'.',object:'#'}),row(15,3,'D',0,0,{ground:'D',object:'.'})],queries},
  {rows:[row(16,4,'W',4,0,{ground:'W'})],queries},
  {rows:[row(n-2,1,'B',1,0,{ground:'.',object:'B'}),row(1,1,'B',0,0,{ground:'.',object:'B'})],hiddenChange:true},
  {rows:[row(1,2,'.',0,0,{ground:'.',object:'.'}),row(n-2,2,'.',0,0,{ground:'.',object:'.'})],trim:true},
 ];
 sequence.splice(2,0,
   {rows:[row(72,1,'.')],fires:[72],wrecks:[{x:17,z:11,moving:false}],queries:[{cell:72,at:pt(15,11),from:pt(21,11)}]},
   {rows:[],wrecks:[{x:17,z:11,moving:true}],queries:[{cell:72,at:pt(15,11),from:pt(21,11)}]});
 for(let i=0;i<35;i++)sequence.push({rows:[row(70,i+1,i%2?'#':'.',0,0,{ground:'.',object:i%2?'#':'.'})]});
 scenario(`crafted-${mode}`,mode,data,sequence);
}
// Three small authored maps exercise real initial layer/height/house inputs.
const maps=readdirSync(resolve(reference,'maps')).filter(f=>f.endsWith('.json')).map(name=>({name,map:JSON.parse(readFileSync(resolve(reference,'maps',name),'utf8'))})).sort((a,b)=>a.map.w*a.map.h-b.map.w*b.map.h).slice(0,3);
for(const {name,map} of maps){
 const saved=Math.random;let seed=17;Math.random=()=>{seed=(Math.imul(seed,1664525)+1013904223)>>>0;return seed/4294967296;};
 let game;try{game=sim.createGame(map,['One','Two'],false,[0,1],[0,1],{mode:'conquest',weather:false});}finally{Math.random=saved;}
 const t=game.initialTerrain,input=initial(game.w,game.h);
 Object.assign(input,{chars:[...t.chars],ground:[...t.ground],objects:[...t.objects],heights:[...(t.height??input.heights)],shoals:[...(game.shoal??input.shoals)].map(Boolean),houseKinds:[...game.house],toughness:[...(game.tough??input.toughness)],trenchFront:[...(t.trenchFront??input.trenchFront)].map(v=>Number.isFinite(v)?v:null)});
 scenario(`map:${name}`,'Observed',input,[{rows:[]}]);
}
const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases})+'\n';
if(process.argv.includes('--check')){if(output!==readFileSync(new URL('../rust/crates/core/tests/fixtures/route-knowledge.json',import.meta.url),'utf8'))throw Error('Route knowledge fixture differs');console.log('Route knowledge fixture matches');}else process.stdout.write(output);
