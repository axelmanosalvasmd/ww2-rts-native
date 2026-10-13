// Original private World phases over explicit restored scenes, not a full tick replay.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
const root=fileURLToPath(new URL('../',import.meta.url));
const ref=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',args,{cwd:ref,encoding:'utf8'}).trim();
const clean=()=>{if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Pinned tracked-clean reference required');};
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node24.21.0');
if(process.argv.slice(2).some(v=>v!=='--check'))throw Error('Only --check is supported');
clean();
registerHooks({load(url,context,next){const out=next(url,context);if(url!==pathToFileURL(`${ref}/shared/sim.js`).href)return out;return {...out,source:String(out.source)+'\nexport {worldIncome as __worldIncome,stepWorld as __stepWorld};\n'};}});
const sim=await import(pathToFileURL(`${ref}/shared/sim.js`));
const {rebuildGrid}=await import(pathToFileURL(`${ref}/shared/grid.js`));
const {STORY_KEYS}=await import(pathToFileURL(`${ref}/shared/story.js`));
const definitions=Array.from({length:64},(_,i)=>({id:i*2-100,x:i%8*64+32,y:Math.floor(i/8)*64+32,kind:['rural','city','industrial','resource'][i%4]}));
const at=i=>({x:(definitions[i].x+.5)*sim.CELL,z:(definitions[i].y+.5)*sim.CELL});
const unit=(id,owner,type,region,extra={})=>({id,owner,type,...at(region),hp:sim.UNITS[type].models*sim.UNITS[type].hpPer,rot:0,garrison:-1,supp:0,path:[],cargo:[],retreating:false,riding:0,cells:[],built:1,...extra});
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const facts=units=>units.map(u=>({id:u.id,owner:u.owner,type:u.type,hp:bits(u.hp)}));
const row=u=>({id:u.id,owner:u.owner,type:u.type,x:u.x,z:u.z,hp:u.hp,...(u.built===undefined?{}:{built:u.built}),retreating:!!u.retreating,riding:u.riding??0});
const cases=[];
function record(name,{teams=[0,1],regions={},players={},units=[],nodes=[],beforeIncome=[],between=[],dt=.05}={}){
 const homes=teams.map((_,i)=>i+1===teams.length?63:i),map={name:'Explicit World phases',w:512,h:512,rows:Array(512).fill('.'.repeat(512)),spawns:homes.map(i=>({x:definitions[i].x,y:definitions[i].y})),points:[],world:{size:'huge',total:64,regionCells:64,regionMap:Array.from({length:512*512},(_,c)=>Math.floor(Math.floor(c/512)/64)*8+Math.floor(c%512/64)),regions:definitions.map((r,i)=>({...r,bounds:[i%8*64,Math.floor(i/8)*64,i%8*64+64,Math.floor(i/8)*64+64],...(homes.includes(i)?{home:homes.indexOf(i)}:{})}))}};
 const old=Math.random;Math.random=()=>.5;let g;
 try{
 g=sim.createGame(map,teams.map((_,i)=>`P${i}`),false,teams,teams.map(()=>0),{mode:'world',army:'standard',tech:false,weather:false,logistics:false,supply:true});
 g.units=new Map(units.map(u=>[u.id,structuredClone(u)]));g.nodes=structuredClone(nodes);g.nextId=1000;g.tick=40;g.winner=null;g.timeline=[];g.reveal=false;g.story=g.players.map(()=>Object.fromEntries(STORY_KEYS.map(k=>[k,0])));
 g.players.forEach((p,slot)=>Object.assign(p,{mp:100,mun:4,fuel:2,vp:0,inc:0,fuelInc:undefined,upkeep:undefined,away:false,out:false,spawn:at(homes[slot]),...players[slot]}));
 g.world.regions.forEach((r,i)=>Object.assign(r,{team:i===0?teams[0]:i===63?teams.at(-1):-1,progress:i===0||i===63?1:0,capper:-1,contested:false,locked:undefined,capTick:undefined,...regions[i]}));
 const snapshot=()=>({regions:g.world.regions.map(r=>[r.team,bits(r.progress),r.capper<0?null:r.capper,!!r.contested,r.locked??null,r.capTick??null]),players:g.players.map(p=>[bits(p.mp),bits(p.mun),bits(p.fuel),bits(p.inc),p.fuelInc===undefined?null:bits(p.fuelInc),p.upkeep===undefined?null:bits(p.upkeep),!!p.out,[bits(p.spawn.x),bits(p.spawn.z)]]),story:g.story.map(s=>bits(s.captures)),units:[...g.units.values()].map(u=>[u.id,u.owner]),winner:g.winner,reason:g.endReason??null,endAt:g.endAt?[bits(g.endAt.x),bits(g.endAt.z)]:null});
 const captured=[...g.units.values()],capturedOrder=captured.map(u=>u.id),actions=[];
 const perform=(input,phase)=>{for(const a of input){let born=null;
  if(a.op==='health')g.units.get(a.id).hp=a.hp;
  else if(a.op==='remove')g.units.delete(a.id);
  else if(a.op==='birth'){const u=sim.internals.spawnUnit(g,a.owner,a.type);Object.assign(u,{...a.at,...(a.built===undefined?{}:{built:a.built})});born=row(u);}
  else throw Error(a.op);
  actions.push({...a,phase,born});
 }};
 perform(beforeIncome,'beforeIncome');
 const capturedFacts=facts(captured),incomeOrder=[...g.units.keys()],before=snapshot();
 for(const p of g.players)sim.__worldIncome(g,p,captured,dt);
 const afterIncome=snapshot();
 perform(between,'between');rebuildGrid(g);
 const regionOrder=[...g.units.keys()],beforeRegions=snapshot();
 const removed=[],originalDelete=g.units.delete;g.units.delete=function(id){removed.push(id);return originalDelete.call(this,id);};
 sim.__stepWorld(g,captured,dt);g.units.delete=originalDelete;
 cases.push({name,input:{teams,army:'standard',regions,players,units,nodes},dt,capturedOrder,capturedFacts,actions,incomeOrder,regionOrder,before,afterIncome,beforeRegions,afterRegions:snapshot(),removed});
 }finally{Math.random=old;}
}
record('late final health survives object removal',{units:[unit(90,0,'tank',20),unit(31,0,'rifle',20),unit(7,1,'rifle',63)],beforeIncome:[{op:'health',id:90,hp:-3},{op:'remove',id:90},{op:'health',id:31,hp:0},{op:'remove',id:31}]});
record('explicit positive retired upkeep and current depot payments',{units:[unit(50,0,'depot',0),unit(90,0,'tank',20),unit(7,1,'rifle',63)],nodes:[{c:16416,...at(0),depot:50,rate:7,fuel:false},{c:16417,...at(0),depot:1000,rate:4,fuel:true}],beforeIncome:[{op:'remove',id:50},{op:'remove',id:90},{op:'birth',owner:0,type:'depot',at:at(0),built:1}]});
record('income precedes current birth and region capture',{regions:{1:{team:-1,progress:.999,capper:0}},units:[unit(90,0,'tank',20),unit(7,1,'rifle',63)],between:[{op:'birth',owner:0,type:'rifle',at:at(1)}]});
record('final income precedes current army elimination',{regions:{0:{team:-1,progress:0}},units:[unit(90,0,'tank',20),unit(7,1,'rifle',63),unit(31,0,'rifle',20)]});
record('captured order governs fractional upkeep',{units:[unit(90,0,'medium',20,{x:at(20).x+45}),unit(7,0,'lorry',20,{x:at(20).x+45}),unit(31,0,'armoredcar',20,{x:at(20).x+45}),unit(12,0,'mg',20,{x:at(20).x+45}),unit(80,1,'rifle',63)]});
record('allied commanders and neutral captured actors',{teams:[5,1,5],players:{2:{away:true}},units:[unit(90,2,'tank',20),unit(7,0,'rifle',20,{x:at(20).x+45}),unit(12,-1,'rifle',20,{x:at(20).x+45}),unit(80,1,'rifle',63)]});
clean();const output=JSON.stringify({format:1,reference:pin,node:process.version,arch:process.arch,definitions,cases})+'\n',path=`${root}/rust/crates/simtest/tests/fixtures/world-phases.json`;
if(process.argv.includes('--check')){if(readFileSync(path,'utf8')!==output)throw Error('World phase fixture differs');}else writeFileSync(path,output);
console.log(`Verified ${cases.length} original split World phase scenes, ${cases.reduce((n,c)=>n+c.capturedFacts.length,0)} captured objects`);
