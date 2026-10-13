// Original World income/capture helpers over explicit ordered scene facts.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {registerHooks} from 'node:module';
import {fileURLToPath,pathToFileURL} from 'node:url';
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');
const ref=resolve(process.env.WW2_REFERENCE??fileURLToPath(new URL('../reference/ww2-rts/',import.meta.url)));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',ref,...a],{encoding:'utf8'}).trim();
if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Use clean pinned reference');
registerHooks({load(url,context,next){const result=next(url,context);
 if(url!==pathToFileURL(resolve(ref,'shared/sim.js')).href)return result;
 return {...result,source:String(result.source)+'\nexport {stepWorld as __stepWorld,worldIncome as __worldIncome};\n'};
}});
const sim=await import(pathToFileURL(resolve(ref,'shared/sim.js')));
const {rebuildGrid}=await import(pathToFileURL(resolve(ref,'shared/grid.js')));
const {STORY_KEYS}=await import(pathToFileURL(resolve(ref,'shared/story.js')));
const bits=n=>{if(n===undefined)return null;const b=Buffer.alloc(8);b.writeDoubleLE(n);return b.readBigUInt64LE().toString(16).padStart(16,'0');};
const definitions=Array.from({length:64},(_,i)=>({id:i*2-100,x:(i%8)*64+32,y:Math.floor(i/8)*64+32,kind:['rural','city','industrial','resource'][i%4]}));
const at=i=>({x:(definitions[i].x+.5)*sim.CELL,z:(definitions[i].y+.5)*sim.CELL});
const unit=(id,owner,type='rifle',region=2,extra={})=>({id,owner,type,...at(region),hp:sim.UNITS[type].hpPer*(sim.UNITS[type].count??1),retreating:false,riding:0,cells:[],...extra});
const cases=[];
function record(name,{teams=[0,1],army='standard',regions={},players={},units=[],nodes=[],steps,supported=true}){
 const input={teams,army,regions,players,units,nodes};
 const p=teams.map((team,slot)=>({slot,team,spawn:at(slot===teams.length-1?63:slot),away:false,out:false,mp:100,mun:4,fuel:2,vp:0,inc:0,...players[slot]}));
 p[-1]={slot:-1,team:-1,spawn:at(62)};
 const membership=new Int16Array(512*512);for(let y=0;y<512;y++)for(let x=0;x<512;x++)membership[y*512+x]=Math.floor(y/64)*8+Math.floor(x/64);
 const rows=definitions.map((r,i)=>({...r,...at(i),team:i===0?teams[0]:i===63?teams.at(-1):-1,progress:i===0||i===63?1:0,capper:-1,...regions[i]}));
 const g={w:512,h:512,world:{total:64,regionCells:64,regions:rows,regionMap:membership},players:p,mode:{kind:'world'},
  units:new Map(units.map(u=>[u.id,structuredClone(u)])),nodes:structuredClone(nodes),army:sim.CFG.armies[army],
  tick:40,winner:null,story:p.map(()=>Object.fromEntries(STORY_KEYS.map(k=>[k,0]))),timeline:[],points:[],shots:[],buildingCells:new Map()};
 const projection=()=>({regions:g.world.regions.map(r=>[r.team,bits(r.progress),r.capper<0?null:r.capper,!!r.contested,r.locked??null,r.capTick??null]),
  players:g.players.map(p=>[bits(p.mp),bits(p.mun),bits(p.fuel),bits(p.inc),bits(p.fuelInc),bits(p.upkeep),p.out,[bits(p.spawn.x),bits(p.spawn.z)]]),
  story:g.story.map(s=>bits(s.captures)),units:[...g.units.values()].map(u=>[u.id,u.owner]),winner:g.winner,
  reason:g.endReason??null,endAt:g.endAt?[bits(g.endAt.x),bits(g.endAt.z)]:null});
 const records=[];
 for(const step of steps){
  for(const change of step.changeUnits??[])Object.assign(g.units.get(change.id),change);
  for(const add of step.addUnits??[])g.units.set(add.id,structuredClone(add));
  for(const change of step.changeRegions??[])Object.assign(g.world.regions[change.index],change);
  for(const change of step.changePlayers??[])Object.assign(g.players[change.slot],change);
  for(const id of step.deleteUnits??[])g.units.delete(id);
  g.tick=step.tick??g.tick+1;rebuildGrid(g);
  const order=step.order??[...g.units.keys()],list=step.tickFacts??order.map(id=>g.units.get(id));
  if(list.some(u=>!u))throw Error('This Match oracle requires represented tick facts');
  const before=projection();
  for(const player of g.players)sim.__worldIncome(g,player,list,step.dt);
  sim.__stepWorld(g,list,step.dt);
  records.push({...step,tick:g.tick,order,before,after:projection()});
 }
 cases.push({name,input,supported,steps:records});
}
record('radius, capture and already owned',{units:[unit(1,0,'rifle',2,{x:at(2).x+8})],steps:[{dt:8},{dt:0},{dt:.05}]});
record('outside radius and eligibility gates',{units:[unit(1,0,'rifle',2,{x:at(2).x+8.0001})],steps:[{dt:8},{dt:8,changeUnits:[{id:1,x:at(2).x,retreating:true}]},{dt:8,changeUnits:[{id:1,retreating:false,riding:2}]},{dt:8,changeUnits:[{id:1,riding:0,logistics:{forced:true}}]},{dt:8,changeUnits:[{id:1,logistics:{forced:false}}]}]});
record('enemy drains before capture',{regions:{2:{team:1,progress:1}},units:[unit(1,0)],steps:[{dt:8},{dt:8},{dt:.05}]});
record('allies preserve first grid owner',{teams:[0,0,1],regions:{1:{team:0,progress:1}},units:[unit(30,1),unit(10,0)],steps:[{dt:8,order:[10,30]}]});
record('unsorted commander teams',{teams:[3,1],units:[unit(1,0)],steps:[{dt:8}]});
record('local defender contests first',{units:[unit(30,-1),unit(10,0)],steps:[{dt:8},{dt:8,changeUnits:[{id:30,hp:0}]}]});
record('guard base locks until dead',{units:[unit(20,-1,'worldbase'),unit(1,0)],steps:[{dt:8},{dt:8,changeUnits:[{id:20,hp:0}]}]});
record('construction site locks its team',{regions:{2:{team:1,progress:1}},units:[unit(20,1,'barracks',2,{built:0}),unit(1,0)],steps:[{dt:8},{dt:8,changeUnits:[{id:20,hp:0}]},{dt:8}]});
record('foreign production does not lock neutral',{units:[unit(20,1,'barracks',2,{built:1}),unit(1,0)],steps:[{dt:8}]});
record('neutral HQ is a team lock, not guard lock',{regions:{2:{team:0,progress:1}},units:[unit(20,-1,'hq',2,{built:1}),unit(1,1)],steps:[{dt:8},{dt:8}]});
record('partial capture resets on other team',{regions:{2:{team:-1,progress:.5,capper:1}},units:[unit(1,0)],steps:[{dt:0},{dt:2},{dt:6}]});
record('relocate home to cell center',{regions:{0:{team:1,progress:1},2:{team:0,progress:1}},steps:[{dt:0}]});
record('live depot pays outside tick list',{army:'large',regions:{2:{team:0,progress:1},3:{team:0,progress:1}},units:[unit(20,0,'depot',2,{built:1}),unit(30,0,'depot',3,{built:1}),unit(1,0,'tank',0)],nodes:[{c:0,...at(2),depot:20,rate:3,fuel:false},{c:1,...at(3),depot:30,rate:1.5,fuel:true}],steps:[{dt:.05,order:[1]},{dt:.05,order:[1],changeUnits:[{id:20,owner:1}]},{dt:.05,order:[1],changeUnits:[{id:30,built:.9}]}]});
record('allied shares away and out',{teams:[0,0,1],army:'endless',regions:{1:{team:0,progress:1},2:{team:0,progress:1},3:{team:0,progress:1}},players:{1:{away:true}},units:[unit(1,0,'tank',0)],steps:[{dt:2},{dt:2,changePlayers:[{slot:1,out:true}]},{dt:0}]});
record('explicit retained depot facts',{units:[unit(20,0,'depot',0,{built:1})],nodes:[{c:0,...at(0),depot:20,rate:3,fuel:false}],supported:false,steps:[{dt:.05,order:[20],deleteUnits:[20],tickFacts:[unit(20,0,'depot',0,{built:1})]}]});
record('unsupported fort transfer',{units:[unit(1,0),unit(20,-1,'gate',2,{open:true,gateT:4})],supported:false,steps:[{dt:8}]});
record('unsupported army elimination',{regions:{0:{team:-1,progress:0}},units:[unit(1,0)],supported:false,steps:[{dt:.05}]});
record('unsupported World finish',{regions:Object.fromEntries(Array.from({length:39},(_,i)=>[i,{team:0,progress:1}])),supported:false,steps:[{dt:0}]});
record('unsupported nations finish',{regions:{63:{team:-1,progress:0}},supported:false,steps:[{dt:0}]});
record('unsupported draw finish',{regions:{0:{team:-1,progress:0},63:{team:-1,progress:0}},supported:false,steps:[{dt:0}]});
const output=JSON.stringify({reference:pin,node:process.version,definitions,cases})+'\n';
const path=fileURLToPath(new URL('../rust/crates/simtest/tests/fixtures/world-region.json',import.meta.url));
if(process.argv.includes('--check')){if(readFileSync(path,'utf8')!==output)throw Error('World region fixture differs');}else writeFileSync(path,output);
console.log(`Verified ${cases.length} World region traces, ${cases.reduce((n,c)=>n+c.steps.length,0)} original transitions`);
