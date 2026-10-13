// Observe the actual pinned World controller. Trace additions do not replace phase calls.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {gzipSync,gunzipSync} from 'node:zlib';
const root=fileURLToPath(new URL('../',import.meta.url));
const ref=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',args,{cwd:ref,encoding:'utf8'}).trim();
const clean=()=>{if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Pinned tracked-clean reference required');};
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');
if(process.argv.slice(2).some(v=>v!=='--check'))throw Error('Only --check is supported');
clean();
registerHooks({load(url,context,next){const result=next(url,context);if(url!==pathToFileURL(`${ref}/shared/sim.js`).href)return result;
 let source=String(result.source);
 const once=(anchor,extra)=>{if(source.split(anchor).length!==2)throw Error(`Source anchor moved: ${anchor}`);source=source.replace(anchor,anchor+extra);};
 once('export function step(g) {',"\n  __worldTickObserve(g,'StructureSync');");
 const start=source.indexOf('export function step(g) {'),end=source.indexOf('\nconst isStructure =',start);
 let body=source.slice(start,end);
 const insert=(anchor,extra)=>{if(body.split(anchor).length!==2)throw Error(`Step anchor moved: ${anchor}`);body=body.replace(anchor,anchor+extra);};
 insert('  g.tick++;',"\n  __worldTickObserve(g,'Clock');");
 insert('  if (g.winner === null && (g.tick === 1 || g.tick % SAMPLE_EVERY === 0)) sample(g, isStructure(g));',"\n  __worldTickObserve(g,'AfterPeriodicSample');");
 insert('  const list = [...g.units.values()];',"\n  __worldTickObserve(g,'CapturedList',list);");
 insert('  g.smokes = g.smokes.filter(s => s.t > 0);',"\n  __worldTickObserve(g,'AfterWeatherFireFlood',list);");
 const death='  for (const u of list) if (u.hp <= 0) {';
 if(body.split(death).length!==2)throw Error('Death anchor moved');body=body.replace(death,"  __worldTickObserve(g,'OrdinaryDeath',list);\n"+death);
 const convoys='  stepConvoys(g, dt, supplyHooks(g));';
 if(body.split(convoys).length!==2)throw Error('Convoy anchor moved');body=body.replace(convoys,"  __worldTickObserve(g,'Convoys',list);\n"+convoys+"\n  __worldTickObserve(g,'AfterConvoys',list);");
 const world="  if (g.mode?.kind === 'world') { stepWorld(g, list, dt); return; }";
 if(body.split(world).length!==2)throw Error('World dispatch anchor moved');body=body.replace(world,"  if (g.mode?.kind === 'world') { stepWorld(g, list, dt); __worldTickObserve(g,'AfterWorldRegions',list); return; }");
 source=source.slice(0,start)+body+source.slice(end);
 once('function weather(g, dt) {',"\n  __worldTickObserve(g,'LivingWeather');");
 once('function supplyLines(g, list) {',"\n  __worldTickObserve(g,'PointSupply',list);");
 once('function supplyNetwork(g, list) {',"\n  __worldTickObserve(g,'SupplyNetwork',list);");
 once('function worldIncome(g, p, list, dt) {',"\n  __worldTickObserve(g,'WorldIncome',list,p.slot);");
 once('function stepWorld(g, list, dt) {',"\n  __worldTickObserve(g,'WorldRegions',list);");
 once('export function finish(g, winner, reason, at) {',"\n  __worldTickObserve(g,'Finish',undefined,undefined,reason);");
 source+=`\nfunction __worldTickObserve(g,phase,list,slot,reason){if(!g.__worldTickTrace)return;g.__worldTickPhase=phase;g.__worldTickEvidence?.(g,phase,list,slot,reason);g.__worldTickTrace.push({phase,tick:g.tick,slot:slot??null,reason:reason??null,current:[...g.units.keys()],captured:list?.map(u=>u.id)??null,missing:list?.filter(u=>!g.units.has(u.id)).map(u=>[u.id,u.owner,u.hp])??[],seats:g.players.map(p=>p.slot),local:g.players[-1]?.slot??null,out:g.players.map(p=>!!p.out),money:g.players.map(p=>p.mp),timeline:g.timeline?.length??null,convoys:!!g.convoys,pointSupply:g.supply,regions:g.world.regions.map(r=>[r.team,r.progress,r.capper,r.capTick??null])});}\n`;
 source+='\nexport const __worldTickRegion=worldRegionAt;\n';
 return {...result,source};}});
const simURL=pathToFileURL(`${ref}/shared/sim.js`).href;
const sim=await import(simURL);
// The query loads the pristine source through the next loader, outside the exact hook URL.
const pristine=await import(`${simURL}?world-tick-pristine`);
const regions=Array.from({length:64},(_,i)=>({id:i,x:i%8*64+32,y:Math.floor(i/8)*64+32,kind:'rural',bounds:[i%8*64,Math.floor(i/8)*64,i%8*64+64,Math.floor(i/8)*64+64],...(i===0?{home:0}:i===63?{home:1}:{})}));
const map={name:'World controller ordering',w:512,h:512,rows:Array(512).fill('.'.repeat(512)),spawns:[{x:32,y:32},{x:480,y:480}],points:[],world:{size:'huge',total:64,regionCells:64,regionMap:Array.from({length:512*512},(_,c)=>Math.floor(Math.floor(c/512)/64)*8+Math.floor(c%512/64)),regions}};
const cases=[];
const path=`${root}/rust/crates/simtest/tests/fixtures/world-tick.json.gz`;
const originalCases=JSON.parse(gunzipSync(readFileSync(path))).cases.slice(0,8);
assert.equal(originalCases.length,8,'Preserve all eight predecessor scenes');
const buffer=new DataView(new ArrayBuffer(8));
const bits=n=>{buffer.setFloat64(0,n);return buffer.getBigUint64(0).toString(16).padStart(16,'0');};
const copy=v=>v instanceof Map?[...v].map(([k,v])=>[copy(k),copy(v)]):v instanceof Set?[...v].map(copy):ArrayBuffer.isView(v)?[...v]:Array.isArray(v)?v.map(copy):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).filter(([,v])=>v!==undefined&&typeof v!=='function').map(([k,v])=>[k,copy(v)])):v;
function exact(value){
 const raw=copy(value),numberBits={};
 function visit(v,path=''){
  // Safe integers have exact binary64 bits derivable from raw. Preserve all other Number bits.
  if(typeof v==='number'){if(!Number.isSafeInteger(v)||Object.is(v,-0))numberBits[path]=bits(v);}
  else if(v&&typeof v==='object')for(const[k,x]of Object.entries(v))visit(x,path?`${path}.${k}`:k);
 }
 visit(raw);return {raw,numberBits};
}
function runs(values){
 if(!values)return null;
 const rows=[];
 for(const value of values){const last=rows.at(-1);if(last&&Object.is(last[0],value))last[1]++;else rows.push([value,1]);}
 return rows;
}
const incomeFacts=u=>({id:u.id,owner:u.owner,type:u.type,hp:u.hp});
const wallet=p=>({slot:p.slot,team:p.team,mp:p.mp,inc:p.inc,upkeep:p.upkeep??null,
 mun:p.mun??null,fuel:p.fuel??null,fuelInc:p.fuelInc??null,away:!!p.away,out:!!p.out,spawn:copy(p.spawn)});
function canonical(g){
 return exact({tick:g.tick,visionTick:g.visionTick,nextId:g.nextId,nextFlight:g.nextFlight,...(g.nextProductionJob!==undefined?{nextProductionJob:g.nextProductionJob}:{}),seed:g.seed,wind:g.wind,wx:g.wx??null,
  mode:g.mode,army:g.army,pointSupply:g.supply,logisticsEnabled:g.logisticsEnabled,
  units:[...g.units.values()],commanders:g.players.map(copy),localDefender:copy(g.players[-1]),
  regions:g.world.regions,nodes:g.nodes,points:g.points,story:g.story??null,timeline:g.timeline??null,fallen:g.fallen??null,
  finish:{winner:g.winner,reason:g.endReason??null,at:g.endAt??null,tick:g.endTick??null,reveal:!!g.reveal},
  shots:g.shots,flights:[...g.flights.values()],nades:g.nades,salvos:g.salvos,strikes:g.strikes,covers:g.covers??[],
  convoys:g.convoys??null,fires:[...g.fires],soak:[...g.soak],smokes:g.smokes,wrecks:g.wrecks,
  structures:[...g.structures],structuralCells:[...g.structuralCells],
  terrain:{chars:runs(g.chars),flags:runs(g.flags),height:runs(g.height),hp:runs(g.cellHp),wear:runs(g.wear),scar:runs(g.scar),burnt:runs(g.burnt),state:runs(g.cellState),
   versions:[g.terrainVersion??0,g.obstructionVersion??0,g.infantryRegionVersion??0,g.vehicleRegionVersion??0,g.navalRegionVersion??0],
   newCells:g.newCells,cellLog:g.cellLog,journal:g.cellJournal??[],journalBase:g.journalBase??0}});
}
// Encoding reads immutable observations. Decoding never writes to the Source game.
function unpack(encoded){
 let raw=copy(encoded.raw);
 for(const[path,hex]of Object.entries(encoded.numberBits)){
  buffer.setBigUint64(0,BigInt(`0x${hex}`));const number=buffer.getFloat64(0);
  if(path===''){raw=number;continue;}
  const keys=path.split('.');let owner=raw;for(const key of keys.slice(0,-1))owner=owner[key];owner[keys.at(-1)]=number;
 }
 return raw;
}
function delta(before,after){
 const patches=[];
 function visit(a,b,path){
  if(Object.is(a,b))return;
  const objects=a&&b&&typeof a==='object'&&typeof b==='object';
  if(objects&&Array.isArray(a)===Array.isArray(b)&&(!Array.isArray(a)||a.length===b.length)){
   for(const key of Object.keys(a))if(!Object.hasOwn(b,key))patches.push({op:'delete',path:[...path,key]});
   for(const key of Object.keys(b))if(Object.hasOwn(a,key))visit(a[key],b[key],[...path,key]);else patches.push({op:'set',path:[...path,key],value:exact(b[key])});
  }else patches.push({op:'set',path,value:exact(b)});
 }
 visit(before,after,[]);return patches;
}
function applyDelta(image,patches){
 for(const patch of patches){
  if(!patch.path.length){assert.equal(patch.op,'set');image=unpack(patch.value);continue;}
  let owner=image;for(const key of patch.path.slice(0,-1))owner=owner[key];const key=patch.path.at(-1);
  if(patch.op==='delete')delete owner[key];else owner[key]=unpack(patch.value);
 }
 return image;
}
function sourceRandom(){
 let count=0,stage='construction';const sites=new Map();
 return {setStage(value){stage=value;},next(){
  const line=new Error().stack.split('\n').find(s=>s.includes(`${ref}/shared/`));
  assert(line,'Source-only fixed draw must identify its caller');count++;
  const site=`${stage}:${line.trim().replace('?world-tick-pristine','').replace(ref,'reference/ww2-rts')}`;
  sites.set(site,(sites.get(site)??0)+1);return .5;
 },snapshot:()=>({count,sites:[...sites].map(([site,count])=>({site,count}))})};
}
function setup(module,{supply,logistics,lose,death,capture,shortage,decided,guardSnap,wind,commanderWallet},observed){
 const g=module.createGame(map,['A','B'],false,[0,1],[0,0],{mode:'world',tech:true,weather:false,supply,logistics});
 for(const u of g.units.values()){u.holdPos=true;u.holdFire=true;u.auto=false;}
 const authored=[];
 if(wind)g.wind={a:wind.a,v:wind.s};
 if(commanderWallet){Object.assign(g.players[commanderWallet.commander],{mp:commanderWallet.mp,mun:commanderWallet.mun});authored.push(copy(commanderWallet));}
 if(lose)for(const r of g.world.regions)if(r.team===0){r.team=-1;r.progress=0;authored.push({region:r.id,team:-1,progress:0});}
 const corpse=death?[...g.units.values()].find(u=>u.owner===0&&u.type==='rifle'):null;
 if(corpse){corpse.hp=0;corpse.lastHit=1;authored.push({unit:corpse.id,hp:0,lastHit:1});}
 if(shortage){const u=[...g.units.values()].find(u=>u.owner===0&&sim.UNITS[u.type].infantry&&u.logistics);if(!u)throw Error('No canonical reserve unit');const r=g.world.regions.find(r=>r.team===1);u.x=r.x+45;u.z=r.z+45;Object.assign(u.logistics,{ammo:0,provisions:20,cutFor:30,supplyRate:0});authored.push({unit:u.id,x:u.x,z:u.z,logistics:{ammo:0,provisions:20,cutFor:30,supplyRate:0}});}
 let capturedRegion=null;
 if(capture){const r=g.world.regions.find(r=>r.team===0&&!([...g.units.values()].some(u=>u.type==='worldbase'&&sim.__worldTickRegion(g,u)?.id===r.id)));
  if(!r)throw Error('No commander region without a guard-base lock');const u=[...g.units.values()].find(u=>u.owner===0&&sim.UNITS[u.type].infantry);u.x=r.x;u.z=r.z;r.team=-1;r.progress=.999;r.capper=0;capturedRegion=r.id;authored.push({unit:u.id,x:u.x,z:u.z},{region:r.id,team:-1,progress:.999,capper:0});
 }
 let guardId=null;
 if(guardSnap){const u=[...g.units.values()].find(u=>u.owner===-1&&u.type==='rifle'&&u.guardHome);assert(u,'Original constructor must publish a neutral rifle with guardHome');guardId=u.id;
  Object.assign(u,{x:u.guardHome.x+17,z:u.guardHome.z,path:[{x:u.guardHome.x+18,z:u.guardHome.z}],cooldown:1.2,retarget:1.3,repath:9,cd:7,buff:8,sprint:10,react:11});
  authored.push({unit:u.id,x:u.x,z:u.z,path:copy(u.path),cooldown:1.2,retarget:1.3,repath:9,cd:7,buff:8,sprint:10,react:11});
 }
 g.seed=123456789;
 const finishInput=decided?{winner:0,reason:'nations',at:{x:512,z:512}}:null;
 if(finishInput)module.finish(g,finishInput.winner,finishInput.reason,finishInput.at);
 if(observed)g.__worldTickTrace=[];
 return {g,corpse,capturedRegion,guardId,authored,finishInput};
}
function record(name,{until=1,supply=false,logistics=false,lose=false,death=false,capture=false,shortage=false,decided=false,guardSnap=false,wind,commanderWallet,commands}={}){
 const save=Math.random,observedRandom=sourceRandom(),plainRandom=sourceRandom();
 const options={supply,logistics,lose,death,capture,shortage,decided,guardSnap,wind,commanderWallet};
 try{
  Math.random=()=>observedRandom.next();const observed=setup(sim,options,true);
  Math.random=()=>plainRandom.next();const unobserved=setup(pristine,options,false);
  const {g,corpse,capturedRegion,guardId}=observed,baseline=unobserved.g;
  const initial=canonical(g);assert.deepEqual(initial,canonical(baseline),`${name}: initial constructor state`);
  assert.deepEqual(observed.authored,unobserved.authored);assert.deepEqual(observedRandom.snapshot(),plainRandom.snapshot());
  const constructionRandom=observedRandom.snapshot(),deleted=[],originalDelete=g.units.delete;
  g.units.delete=function(id){const u=this.get(id);deleted.push({tick:g.tick,phase:g.__worldTickPhase,id,owner:u?.owner??null,hp:u?.hp??null});return originalDelete.call(this,id);};
  const frames=[];let captured=null,currentFrame=null,previous=initial;
  let decoded=unpack(JSON.parse(JSON.stringify(initial)));assert.deepEqual(decoded,initial.raw,`${name}: initial lossless Number encoding`);
  g.__worldTickEvidence=(game,phase,list,slot)=>{
   if(phase==='CapturedList')captured=list;
   if(!currentFrame||!['CapturedList','OrdinaryDeath','WorldIncome','AfterConvoys','WorldRegions','AfterWorldRegions'].includes(phase))return;
   currentFrame.phaseEvidence.push(exact({phase,tick:game.tick,slot:slot??null,current:[...game.units.keys()],
    captured:list?.map(incomeFacts)??null,currentObjects:phase==='WorldRegions'?[...game.units.values()].map(incomeFacts):null,commanders:game.players.map(wallet),
    regions:game.world.regions.map(r=>({id:r.id,team:r.team,progress:r.progress,capper:r.capper,capTick:r.capTick??null,contested:!!r.contested,locked:!!r.locked})),
    story:game.story??null,timelineLength:game.timeline?.length??0,winner:game.winner}));
  };
  for(let tick=1;tick<=until;tick++){
   const prior=[...g.units.values()].map(u=>[u.id,bits(u.x),bits(u.z),bits(u.rot)]),start=g.shots.length,appended=[];
   currentFrame={tick,prior,seedBefore:g.seed,phaseEvidence:[],emissions:[]};
   Object.defineProperty(g.shots,'push',{configurable:true,value:function(...shots){
    const site=new Error().stack.split('\n').find(s=>s.includes(simURL));assert(site,'Chronological shots must identify their original append site');
    for(const shot of shots){appended.push(shot);currentFrame.emissions.push({index:start+appended.length-1,site:site.trim().replace(ref,'reference/ww2-rts')});}
    return Array.prototype.push.apply(this,shots);
   }});
   observedRandom.setStage(`tick${tick}`);plainRandom.setStage(`tick${tick}`);
   if(commands){
    const receipts=[],commandState=game=>({commanders:game.players.map(p=>({slot:p.slot,mp:p.mp,mun:p.mun,tier:p.tier,lab:copy(p.lab)})),production:[...game.units.values()].filter(u=>u.productionJobs!==undefined).map(u=>({id:u.id,queue:copy(u.queue),prog:u.prog,jobs:copy(u.productionJobs)})),nextProductionJob:game.nextProductionJob??null});
    for(const[index,input]of commands.entries())if(input.tick===tick){
     const before=commandState(g);
     Math.random=()=>observedRandom.next();const result=sim.command(g,input.slot,copy(input.command));
     Math.random=()=>plainRandom.next();const plainResult=pristine.command(baseline,input.slot,copy(input.command));
     assert.equal(result,plainResult,`${name}:${tick}: original command receipt`);assert.equal(result,undefined,`${name}:${tick}: actual command admission`);
     assert.deepEqual(canonical(g),canonical(baseline),`${name}:${tick}: observer changed original command state`);
     receipts.push({index,slot:input.slot,result:result??null,before,after:commandState(g)});
    }
    currentFrame.commandReceipts=exact(receipts);
   }
   Math.random=()=>observedRandom.next();sim.step(g);
   Math.random=()=>plainRandom.next();pristine.step(baseline);
   assert.deepEqual(appended,g.shots.slice(start),`${name}:${tick}: chronological original append order`);
   const after=canonical(g);assert.deepEqual(after,canonical(baseline),`${name}:${tick}: observer changed pristine original step`);
   assert.deepEqual(observedRandom.snapshot(),plainRandom.snapshot(),`${name}:${tick}: observer changed Source fixed draws`);
   assert(captured,`${name}:${tick}: original late list not observed`);
   currentFrame.captured=exact({ids:captured.map(u=>u.id),facts:captured.map(incomeFacts),
    identity:captured.map(u=>({id:u.id,current:g.units.has(u.id),sameObject:g.units.get(u.id)===u})),
    missing:captured.filter(u=>!g.units.has(u.id)).map(copy),
    bornAfterCapture:[...g.units.keys()].filter(id=>!captured.some(u=>u.id===id))});
   currentFrame.shotsAdded=exact(g.shots.slice(start));
   const patches=delta(previous.raw,after.raw);decoded=applyDelta(decoded,JSON.parse(JSON.stringify(patches)));
   assert.deepEqual(decoded,after.raw,`${name}:${tick}: lossless original observation delta`);
   currentFrame.after={baseTick:previous.raw.tick,tick:after.raw.tick,patches};
   currentFrame.summary=exact({tick:after.raw.tick,commanders:g.players.map(wallet),story:after.raw.story,timeline:after.raw.timeline,finish:after.raw.finish,guard:guardId===null?null:copy(g.units.get(guardId))});
   previous=after;
   currentFrame.random=observedRandom.snapshot();frames.push(currentFrame);currentFrame=null;
  }
 const phases=g.__worldTickTrace;
 for(const entry of phases){if(entry.seats.length!==2||entry.seats.includes(-1)||entry.local!==-1||entry.pointSupply!==false)throw Error('Source actor membership changed');}
 for(const tick of new Set(phases.map(e=>e.tick).filter(t=>t>0))){const rows=phases.filter(e=>e.tick===tick);const index=p=>rows.findIndex(e=>e.phase===p);
  if(index('Clock')<0)continue;
  for(const [a,b]of [['Clock','AfterPeriodicSample'],['AfterPeriodicSample','CapturedList'],['CapturedList','LivingWeather'],['LivingWeather','OrdinaryDeath'],['OrdinaryDeath','WorldIncome'],['WorldIncome','Convoys'],['AfterConvoys','WorldRegions'],['WorldRegions','AfterWorldRegions']])if(index(a)<0||index(b)<=index(a))throw Error(`${name} invalid actual phase order ${a}/${b}`);
 }
 if(phases.some(e=>e.phase==='PointSupply'))throw Error('World point supply must stay disabled');
 if(logistics&&until>=20&&!phases.some(e=>e.phase==='SupplyNetwork'&&e.tick===20))throw Error('Missing actual regional network update');
 if(shortage&&!phases.some(e=>e.phase==='AfterConvoys'&&e.current.some(id=>!e.captured.includes(id))))throw Error('Missing actual post-list convoy birth');
 if(capture&&!decided){const r=g.world.regions.find(r=>r.id===capturedRegion);if(r.team!==0||r.progress!==1||r.capTick!==1||g.story[0].captures!==1)throw Error('Actual region capture did not occur');const income=phases.find(e=>e.phase==='WorldIncome'&&e.slot===0);if(income.regions[capturedRegion][0]!==-1)throw Error('Income did not use pre-capture territory');}
 if(lose){if(!g.players[0].out||g.winner!==1||g.endReason!=='nations'||!deleted.length||deleted.some(e=>e.phase!=='WorldRegions'))throw Error('Missing actual regional elimination and nations finish');}
 if(death&&!phases.some(e=>e.phase==='WorldIncome'&&e.missing.some(u=>u[0]===corpse.id)))throw Error('Retired tick facts did not survive to income');

 if(decided){
  const region=g.world.regions.find(r=>r.id===capturedRegion);assert.equal(region.team,0);assert.equal(region.progress,1);
  assert.equal(region.capTick,undefined,'After finish, physical capture must not stamp the story capture clock');
  for(const frame of frames){assert.deepEqual(frame.summary.raw.story,initial.raw.story,'After finish, original story remains frozen');assert.deepEqual(frame.summary.raw.timeline,initial.raw.timeline,'After finish, periodic story samples remain frozen');assert.deepEqual(frame.summary.raw.finish,initial.raw.finish,'Physical capture must not revise the existing result');}
  assert(frames[0].summary.raw.commanders[0].mp>initial.raw.commanders[0].mp,'Already-decided World income still credits the active commander');
  const before=frames[0].phaseEvidence.find(p=>p.raw.phase==='WorldIncome'&&p.raw.slot===0);
  assert.equal(before.raw.regions[capturedRegion].team,-1,'Income observes the physical region before late capture');
  assert.equal(frames[1].phaseEvidence.find(p=>p.raw.phase==='WorldIncome'&&p.raw.slot===0).raw.regions[capturedRegion].team,0,'Next income observes the physically captured region');
  const guard=frames[0].summary.raw.guard,input=initial.raw.units.find(u=>u.id===guardId);
  assert.equal(guard.x,input.guardHome.x);assert.equal(guard.z,input.guardHome.z);assert.deepEqual(guard.path,[],'Original guardHome snap clears its route before movement');
  for(const key of ['cooldown','retarget','repath','cd','buff','sprint','react'])assert.equal(bits(guard[key]),bits(input[key]-sim.TICK),`Neutral guard executes its real ${key} timer`);
 }
 if(commands){
  assert.deepEqual(initial.raw.wind,{a:wind.a,v:wind.s},'Declared wind is applied before the original steps');
  const receipts=frames[0].commandReceipts.raw;assert.equal(receipts.length,2);assert.deepEqual(frames[1].commandReceipts.raw,[]);
  assert.equal(receipts[0].before.commanders[0].mp,400);assert.equal(receipts[0].before.commanders[0].mun,50);
  assert.equal(receipts[0].after.commanders[0].mp,150);assert.equal(receipts[0].after.commanders[0].mun,0);
  assert.deepEqual(receipts[0].after.commanders[0].lab,[{kind:'tier',prog:0,time:60}]);
  assert.equal(receipts[1].after.commanders[0].mp,50);assert.equal(receipts[1].after.nextProductionJob,1);
  assert.deepEqual(receipts[1].after.production.find(b=>b.id===1),{id:1,queue:['rifle'],prog:0,jobs:[{id:1,type:'rifle',status:'active',mp:100,fuel:0}]});
  assert.deepEqual(receipts[1].after.production.filter(b=>b.id!==1),receipts[1].before.production.filter(b=>b.id!==1),'Paid admission changes only the chosen HQ queue');
  assert.deepEqual(g.players[0].lab,[{kind:'tier',prog:2*sim.TICK,time:60}]);
  const hq=g.units.get(commands[1].command.from);assert.deepEqual(hq.queue,['rifle']);assert.equal(bits(hq.prog),bits(2*sim.TICK));
  assert.equal(g.nextProductionJob,1);assert.equal(g.nextId,initial.raw.nextId,'Two full steps advance paid training without inventing a birth');
 }
 const legacy={name,input:{until,supply,logistics,lose,death,capture,capturedRegion,shortage},phases,deleted,final:{tick:g.tick,winner:g.winner,reason:g.endReason??null,out:g.players.map(p=>!!p.out),timeline:g.timeline??null,regions:g.world.regions.map(r=>[r.team,r.progress,r.capper,r.capTick??null]),story:g.story??null,units:[...g.units.values()].map(u=>[u.id,u.owner,u.type,u.hp])}};
 cases.push({...legacy,...(decided?{decided:true,guardSnap:true}:{}),setup:{map:'fixture.map',options:{mode:'world',tech:true,weather:false,supply,logistics},teams:[0,1],factions:[0,0],
  controls:{allUnits:{holdPos:true,holdFire:true,auto:false}},authored:observed.authored,environmentSeed:123456789,finishInput:observed.finishInput,
  sourceRandom:{fixed:.5,construction:constructionRandom,nativeEquivalent:false},...(wind?{wind:copy(wind)}:{}),...(commands?{commands:copy(commands)}:{})},initial,frames,legacyProjectionHash:createHash('sha256').update(JSON.stringify(legacy)).digest('hex')});
 console.log(`${name}: ${until} original/pristine steps agree`);
 }finally{Math.random=save;}
}
record('neutral actors remain outside commander loops',{until:10});
record('World disables point supply requested by options',{until:41,supply:true});
record('regional logistics network precedes World income',{until:20,supply:true,logistics:true});
record('convoy births follow captured upkeep membership',{until:40,logistics:true,shortage:true});
record('region capture occurs after income',{capture:true});
record('last-region elimination follows final income',{lose:true});
record('ordinary retirement leaves captured facts for income',{death:true});
record('already-decided World income and physical capture with frozen story',{until:2,capture:true,decided:true,guardSnap:true});
record('authored wind with real World research and queued ground training',{until:2,wind:{a:0,s:.5},commanderWallet:{commander:0,mp:400,mun:50},commands:[{tick:1,slot:0,command:{t:'tech',kind:'tier'}},{tick:1,slot:0,command:{t:'buy',unit:'rifle',from:1}}]});
assert.deepEqual(cases.slice(0,8),originalCases,'All eight predecessor scenes must remain unchanged');
clean();
const preserved=[
 'e28775de62ca2c8bf84ed1ab3f5b12252ed25c746bfb2f9bf882eb1a51ca3e77',
 '63f48ce56458f7b50944196866d4f8b5cabc8067374c9eae4dab583979bf080c',
 'a56281c417dc98cbd112723463d48223415032a619fdfd11f115e9be4b854bdc',
 '432743d0c01ca8cdb27b3c4e08fc3084328ef0162d68be30f4fa577b4c1de519',
 'b2b03e4ee1ecccae53abd6357ae9f06349d9f9ab954a2a3fd4afc3e9511b8f5a',
 '29088b1f4ed81b341128882979ecb37b31e36a5bf170efa27331986b5e3604a7',
 '8db05bf0cfe01169ae1a7ee556033aa75712241bbeaf2517d1404ec4fb459a78'];
assert.deepEqual(cases.slice(0,7).map(c=>c.legacyProjectionHash),preserved,'Original seven controller projections from6822056 must remain unchanged');
assert.equal(cases.slice(0,7).reduce((n,c)=>n+c.input.until,0),114);
assert.equal(cases.slice(0,7).reduce((n,c)=>n+c.phases.length,0),1486);
const steps=cases.reduce((n,c)=>n+c.frames.length,0),phaseEntries=cases.reduce((n,c)=>n+c.phases.length,0);
const output=gzipSync(Buffer.from(JSON.stringify({format:2,reference:pin,node:process.version,arch:process.arch,
 scope:'Original complete World steps with passive chronological output and owned-state reads, compared after every step with a separately imported pristine sim.js. Source-only fixed Math.random and explicit environment seed do not claim native RNG equivalence or recipient/privacy behavior.',
 stateEncoding:'Full canonical initial raw image, followed by ordered lossless set/delete patches. Patch Number values retain explicit non-integral, non-finite and signed-zero bits; safe integer bits are exactly derivable from raw. Every serialized patch sequence is decoded and checked against the actual original state before recording.',
 preservation:{source:'6822056',cases:7,steps:114,phaseEntries:1486,projectionHashes:preserved},map,steps,phaseEntries,cases})+'\n'));
if(process.argv.includes('--check')){if(!readFileSync(path).equals(output))throw Error('World tick fixture differs');}else writeFileSync(path,output);
console.log(`Verified ${cases.length} original World controller traces, ${steps} ticks, ${phaseEntries} phase entries; original8 complete scenes and original7/114/1486 projections preserved; pristine replay agrees after every step; fixture SHA256 ${createHash('sha256').update(output).digest('hex')}`);
