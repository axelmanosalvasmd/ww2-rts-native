// Actual World constructor and unchanged World income/region/finish/story functions.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {registerHooks} from 'node:module';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const ref=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const clean=()=>{if(execFileSync('git',['rev-parse','HEAD'],{cwd:ref,encoding:'utf8'}).trim()!==pin||execFileSync('git',['status','--porcelain','--untracked-files=no'],{cwd:ref,encoding:'utf8'}).trim())throw Error('Pinned tracked-clean reference required');};
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');clean();
registerHooks({load(url,ctx,next){const out=next(url,ctx);if(url!==pathToFileURL(`${ref}/shared/sim.js`).href)return out;const source=String(out.source);
 const periodic='  if (g.winner === null && (g.tick === 1 || g.tick % SAMPLE_EVERY === 0)) sample(g, isStructure(g));';
 if(source.split(periodic).length!==2)throw Error('Sample source anchor moved');
 return {...out,source:source+`\nexport {stepWorld as __stepWorld,worldIncome as __worldIncome,validateWorldMap as __validateWorldMap,registerBuildingStructure as __registerBuildingStructure,setCell as __setCell};\nexport function __periodic(g){${periodic}}\n`};}});
const sim=await import(pathToFileURL(`${ref}/shared/sim.js`));
const {rebuildGrid}=await import(pathToFileURL(`${ref}/shared/grid.js`));
const {STORY_KEYS,storyResult}=await import(pathToFileURL(`${ref}/shared/story.js`));
let definitions=Array.from({length:64},(_,i)=>({id:i*2-100,x:(i%8)*64+32,y:Math.floor(i/8)*64+32,kind:['rural','city','industrial','resource'][i%4]}));
const at=i=>({x:(definitions[i].x+.5)*sim.CELL,z:(definitions[i].y+.5)*sim.CELL});
const unit=(id,owner,type,region,extra={})=>({id,owner,type,...at(region),hp:sim.UNITS[type].hpPer*(sim.UNITS[type].count??1),retreating:false,riding:0,cells:[],built:1,...extra});
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const encode=v=>typeof v==='number'?bits(v):Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)])):v;
let cases=[];
function record(name,{teams=[5,1,5],regions={},players={},units=[],nodes=[],steps,supported=true}){
 const total=definitions.length,height=total/8*64;const homes=[0,1,total-1];const membership=Array.from({length:512*height},(_,c)=>Math.floor(Math.floor(c/512)/64)*8+Math.floor(c%512/64));
 const map={name:'World finish constructor',w:512,h:height,rows:Array(height).fill('.'.repeat(512)),spawns:homes.map(i=>({x:definitions[i].x,y:definitions[i].y})),points:[],world:{size:total===64?'huge':'massive',total,regionCells:64,regionMap:membership,regions:definitions.map((r,i)=>({...r,bounds:[i%8*64,Math.floor(i/8)*64,i%8*64+64,Math.floor(i/8)*64+64],...(homes.includes(i)?{home:homes.indexOf(i)}:{})}))}};
 const invalid=sim.__validateWorldMap(map);if(invalid!==null)throw Error(`Invalid World finish map: ${invalid}`);
 let g;const old=Math.random;Math.random=()=>.5;try{g=sim.createGame(map,teams.map((_,i)=>`Commander ${i}`),false,teams,teams.map(()=>0),{mode:'world',tech:false,weather:false,logistics:false,supply:true});}finally{Math.random=old;}
 g.units=new Map(units.map(u=>[u.id,{garrison:-1,supp:0,rot:0,path:[],cargo:[],...structuredClone(u)}]));g.nodes=structuredClone(nodes);
 g.seed=123456789;g.nextId=1000;g.wind={a:.7,v:.6};g.wx={rain:0,wet:0};g.fallingSections=[];
 // Restore explicit flat scene terrain, removing unrelated bootstrap structures before physics.
 g.structures.clear();g.structuralCells.clear();
 for(let c=0;c<g.chars.length;c++)if(g.chars[c]!=='.')sim.__setCell(g,c,'.');
 for(const u of g.units.values())if(u.cells.length){for(const c of u.cells)sim.mutateWorldCell(g,c,{object:'K'});sim.__registerBuildingStructure(g,u);}
 for(const u of g.units.values())if(u.postRegisterHp!==undefined)u.hp=u.postRegisterHp;
 g.newCells=[];g.cellJournal=[];g.shots=[];g.tick=40;g.winner=null;g.timeline=[];g.reveal=false;
 g.story=g.players.map(()=>Object.fromEntries(STORY_KEYS.map(k=>[k,0])));
 g.players.forEach((p,slot)=>Object.assign(p,{mp:100,mun:4,fuel:2,vp:0,inc:0,away:false,out:false,spawn:at(slot===2?total-1:slot),...players[slot]}));
 g.world.regions.forEach((r,i)=>Object.assign(r,{team:i===0?teams[0]:i===total-1?teams[2]:-1,progress:i===0||i===total-1?1:0,capper:-1,contested:false,...regions[i]}));
 const snapshot=()=>encode({regions:g.world.regions.map(r=>[r.team,r.progress,r.capper<0?null:r.capper,!!r.contested,r.locked??null,r.capTick??null]),players:g.players.map(p=>[p.mp,p.mun,p.fuel,p.inc,p.fuelInc??null,p.upkeep??null,p.out,[p.spawn.x,p.spawn.z]]),units:[...g.units.values()].map(u=>[u.id,u.owner,u.hp,u.garrison??-1]),story:g.story,finish:g.winner===null?null:{winner:g.winner,reason:g.endReason,endAt:g.endAt,endTick:g.endTick},timeline:g.timeline,reveal:!!g.reveal,nodes:g.nodes.map(n=>n.depot??0)});
 const records=[];for(const step of steps){
  g.tick=step.tick??g.tick+1;
  for(const change of step.changeRegions??[])Object.assign(g.world.regions[change.index],change);
  rebuildGrid(g);const order=step.order??[...g.units.keys()],list=order.map(id=>g.units.get(id));
  const before=snapshot();if(step.periodic)sim.__periodic(g);
  const removed=[];const removedUnits=[];const rowStart=g.newCells.length,journalStart=g.cellJournal.length;const sourceDelete=g.units.delete;
  g.units.delete=function(id){removed.push(id);removedUnits.push(structuredClone(g.units.get(id)));return sourceDelete.call(this,id);};
  for(const p of g.players)sim.__worldIncome(g,p,list,step.dt);
  sim.__stepWorld(g,list,step.dt);
  g.units.delete=sourceDelete;
  const postWreckUnits=[...g.units.values()].map(u=>({id:u.id,owner:u.owner,type:u.type,hp:u.hp}));
  records.push({...step,tick:g.tick,order,before,after:snapshot(),removed,removedUnits:removedUnits.map(u=>[u.id,u.owner,u.hp,u.garrison??-1]),postWreckUnits,
    shots:encode(g.shots),newCells:encode(g.newCells.slice(rowStart)),journal:g.cellJournal.slice(journalStart),
    falling:g.fallingSections.length,seed:g.seed>>>0,nextId:g.nextId});
 }
 cases.push({name,input:{teams,army:'standard',regions,players,units,nodes},supported,steps:records});
}

const gone={0:{team:-1,progress:0},63:{team:1,progress:1}};
record('already out positive squad still disappears',{regions:gone,players:{0:{out:true}},units:[unit(7,0,'rifle',2),unit(8,1,'rifle',3),unit(9,-1,'rifle',4)],steps:[{dt:.05}]});
record('outer player order and signed health',{regions:gone,units:[unit(90,2,'tank',2,{hp:-3}),unit(8,1,'rifle',3),unit(31,0,'rifle',2),unit(3,2,'fighter',2,{hp:0}),unit(6,0,'rifle',2,{hp:-2}),unit(9,-1,'rifle',4)],steps:[{dt:0}]});
record('allied surviving territory protects both commanders',{regions:{0:{team:-1,progress:0},2:{team:5,progress:1},63:{team:1,progress:1}},units:[unit(31,0,'rifle',3),unit(90,2,'tank',2),unit(8,1,'rifle',3)],steps:[{dt:.05}]});
const c=210*512+210;
record('registered building evicts later deleted and surviving garrisons',{regions:gone,units:[unit(40,0,'hq',27,{x:421,z:421,cells:[c,c+1]}),unit(12,2,'rifle',27,{x:421,z:421,garrison:c}),unit(14,1,'rifle',27,{x:421,z:421,garrison:c+1}),unit(13,-1,'rifle',27,{x:421,z:421,garrison:c+1})],nodes:[{c,x:421,z:421,depot:40,rate:2,fuel:false}],steps:[{dt:.05}]});
record('multiple registered buildings follow player then Map order',{regions:gone,units:[unit(22,2,'hq',27,{x:429,z:421,cells:[c+4]}),unit(40,0,'hq',27,{x:421,z:421,cells:[c,c+1]}),unit(41,0,'barracks',27,{x:425,z:421,cells:[c+2],postRegisterHp:-10})],steps:[{dt:0}]});
record('actual section overflow settles after ninety six flights',{regions:gone,units:[unit(40,0,'hq',27,{x:517,z:421,cells:Array.from({length:97},(_,i)=>c+i)})],steps:[{dt:0}]});
clean();const output=JSON.stringify({format:1,reference:pin,node:process.version,arch:process.arch,definitions,cases})+'\n';
const path=`${root}/rust/crates/simtest/tests/fixtures/world-elimination.json`;
if(process.argv.includes('--check')){if(readFileSync(path,'utf8')!==output)throw Error('World elimination fixture differs');}else writeFileSync(path,output);
console.log(`Verified ${cases.length} actual World elimination cases, ${cases.reduce((n,c)=>n+c.steps.length,0)} transitions`);
