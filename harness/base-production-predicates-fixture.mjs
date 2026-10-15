// Complete original Source ticks. Observers retain the original function bodies.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync} from 'node:fs';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {gzipSync} from 'node:zlib';
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
assert(process.argv.slice(2).every(a=>a==='--check'));
const git=a=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};
clean();
const url=pathToFileURL(`${reference}/shared/sim.js`).href;
registerHooks({load(u,c,next){const r=next(u,c);if(u!==url)return r;return{...r,source:String(r.source)+`
let productionTickObserver = null;
const originalTickLab=stepLab, originalTickTrain=train, originalTickProduce=produceUnit;
stepLab=function(g,p,dt){productionTickObserver?.('lab',g,p.slot,'before');const r=originalTickLab(g,p,dt);productionTickObserver?.('lab',g,p.slot,'after');return r;};
train=function(g,b,dt){productionTickObserver?.('train',g,b.id,'before');const r=originalTickTrain(g,b,dt);productionTickObserver?.('train',g,b.id,'after');return r;};
produceUnit=function(g,b,owner,type){const r=originalTickProduce(g,b,owner,type);productionTickObserver?.('birth',g,r.id,'after');return r;};
export function observeProductionTick(observer){productionTickObserver=observer;}
export {rebuildGrid,productionJobs};
`};}});
const sim=await import(url);
const bytes=new DataView(new ArrayBuffer(8));
const bits=v=>{bytes.setFloat64(0,v);return bytes.getBigUint64(0).toString(16).padStart(16,'0');};
const encode=v=>v===undefined?null:typeof v==='number'?{f64:bits(v)}:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;
const point=p=>p?{x:bits(p.x),z:bits(p.z)}:null;
const timers=['cooldown','retarget','repath','cd','buff','sprint','react','still','stuck','supp','moveSpeed','vx','vz'];
const map={name:'Ordinary production full-step scene',w:96,h:96,rows:Array(96).fill('.'.repeat(96)),spawns:[{x:3,y:3},{x:92,y:92}],points:[{id:'front',x:48,y:48}]};
// Authored top-left cell sites. Source placeBuilding owns the footprint and centre.
const building=(type,owner,x,y,extra={})=>({type,owner,cell:y*map.w+x,built:1,queue:[],prog:0,...extra});
const hq0=building('hq',0,16,16),hq1=building('hq',1,84,84);
function bootstrap(g){return{chars:g.chars.join(''),ground:g.ground.join(''),objects:g.objects.join(''),flags:[...g.flags],height:g.height?[...g.height]:null,cellHp:[...g.cellHp].map(bits),
 footprints:[...(g.buildingCells??[])].sort(([a],[b])=>a-b).map(([cell,b])=>({cell,id:b.id,owner:b.owner,...point(b)})),
 structures:[...g.structures.values()].map(s=>({id:s.id,kind:s.kind,entityId:s.entityId??null,sections:s.sections.map(v=>({id:v.id,c:v.c,hp:bits(v.hp),maxHp:bits(v.maxHp),material:v.material,anchor:v.anchor,supports:v.supports,rubble:v.rubble,state:v.state}))}))};}
function make(spec,sceneMap=map){const old=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(sceneMap,['A','B'],false,[0,1],[0,1],{weather:false,tech:true,supply:false,logistics:false});}finally{Math.random=old;}
 assert.equal(g.mode,undefined);assert.equal(g.wx,null);assert.equal(g.logisticsEnabled,false);
 g.productionFixtureBootstrap=bootstrap(g);
 g.units.clear();g.nextId=1000;g.pathRetryLimit=1;g.seed=123;g.wind={a:0,v:.5};g.shots=[];
 for(const p of g.players)Object.assign(p,{mp:5000.125,mun:500.5,fuel:500.25});
 for(const row of spec){const {cell,...fields}=row;const u=sim.internals.placeBuilding(g,row.owner,row.type,cell,true);Object.assign(u,{holdFire:true,holdPos:true,auto:false,autoRetreat:false},structuredClone(fields));}
 g.productionFixtureScene=bootstrap(g);
 sim.rebuildGrid(g);sim.internals.updateVision(g);return g;
}
function projection(g){return{tick:g.tick,nextId:g.nextId,nextJob:g.nextProductionJob??0,winner:g.winner,pathDropped:g.pathStats?.dropped??0,
 units:[...g.units.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,hp:bits(u.hp),x:bits(u.x),z:bits(u.z),rot:bits(u.rot),aim:bits(u.aim),built:u.built===undefined?null:bits(u.built),cells:u.cells??null,structureId:u.structureId??null,queue:u.queue??null,jobs:(u.productionJobs??[]).map(j=>({id:j.id,type:j.type,status:j.status,mp:bits(j.mp),fuel:bits(j.fuel)})),prog:u.prog===undefined?null:bits(u.prog),rally:point(u.rally),path:u.path.map(point),goal:point(u.worldGoal),routeEnd:point(u.routeEnd),ghost:bits(u.ghost??0),auto:u.auto,autoRetreat:u.autoRetreat,holdFire:!!u.holdFire,holdPos:!!u.holdPos,drift:u.drift==='rally',timers:Object.fromEntries(timers.map(k=>[k,bits(u[k])]))})),
 players:g.players.map(p=>({slot:p.slot,mp:bits(p.mp),mun:bits(p.mun),fuel:bits(p.fuel),inc:bits(p.inc),vp:bits(p.vp),away:!!p.away,out:!!p.out,tier:p.tier,armory:{...p.armory},lab:p.lab.map(j=>({kind:j.kind,prog:bits(j.prog),time:bits(j.time)})),visible:[...p.visible].sort((a,b)=>a-b)})),
 story:g.players.map((_,i)=>Object.fromEntries(['kills','losses','built','captures','mpSpent','supportCalls','planesDowned'].map(k=>[k,bits(g.story?.[i]?.[k]??0)]))),
 points:g.points.map(p=>({owner:p.owner,capper:p.capper,prog:bits(p.progress),on:p.onPoint??[],contested:!!p.contested,cut:!!p.cut})),
 environment:{seed:g.seed|0,wind:[bits(g.wind.a),bits(g.wind.v)]},shots:encode(g.shots)};}
const witness=g=>encode({units:[...g.units.values()],players:g.players.map(({visible,...p})=>({...p,visible:[...visible]})),story:g.story,timeline:g.timeline,points:g.points,shots:g.shots,pathStats:g.pathStats});
function patch(g,p){for(const row of p.units??[])Object.assign(g.units.get(row.id),structuredClone(row.fields));for(const row of p.players??[])Object.assign(g.players[row.slot],structuredClone(row.fields));if(p.winner!==undefined)g.winner=p.winner;}
const traces=[];
function trace(name,specs,count,actions={},sceneMap=map){
 const g=make(specs,sceneMap),baseline=make(specs,sceneMap),initial=projection(g),frames=[];
 for(let i=0;i<count;i++){
  const input=structuredClone(actions[i]??{});patch(g,input);patch(baseline,input);
  const commands=(input.commands??[]).map(c=>({slot:c.slot??0,command:c.command,result:sim.command(g,c.slot??0,structuredClone(c.command))??null}));
  for(const row of commands)assert.equal(sim.command(baseline,row.slot,structuredClone(row.command))??null,row.result);
  const prior=[...g.units.values()].map(u=>({id:u.id,...point(u),rot:bits(u.rot)}));
  const phases=[];sim.observeProductionTick((op,game,actor,edge)=>phases.push({op,actor,edge,state:projection(game)}));
  const old=Math.random;Math.random=()=>{throw Error(`${name}: unexpected runtime random purpose`);};
  try{sim.step(g);sim.observeProductionTick(null);sim.step(baseline);}finally{Math.random=old;sim.observeProductionTick(null);}
  assert.deepEqual(projection(g),projection(baseline),`${name}:${i} observation changed tick`);
  assert.deepEqual(witness(g),witness(baseline),`${name}:${i} observation changed witness`);
  assert.equal(g.shots.length,0);assert.equal(g.flights.size,0);assert.equal(g.nades.length,0);assert.equal(g.salvos.length,0);
  frames.push({input,commands,prior,unitOrder:[...g.units.keys()],phases,after:projection(g),sourceWitness:witness(g)});
 }
 traces.push({name,map:sceneMap,specs,bootstrap:g.productionFixtureBootstrap,scene:g.productionFixtureScene,initial,frames});return traces.at(-1);
}
// These actions combine actual recruitment with the pending HQ job.
const concurrent=trace('pending-hq-tier-research-keeps-real-recruitment', [hq0,hq1],1310,{
 0:{commands:[{command:{t:'tech',kind:'tier'}},{command:{t:'tech',kind:'tier'}},{command:{t:'buy',unit:'rifle',from:1000}}]},
 1:{units:[{id:1000,fields:{built:.5}}]},
 101:{units:[{id:1000,fields:{built:1}}]}
});
assert.deepEqual(concurrent.frames[0].commands.map(c=>c.result),[null,'max',null]);
assert.equal(concurrent.frames[0].after.players[0].tier,1);
assert.equal(concurrent.frames[0].after.players[0].lab.length,1);
assert.equal(concurrent.frames[0].after.units.filter(u=>u.type==='rifle').length,1);
assert.equal(concurrent.frames[0].after.units[0].queue.length,0);
assert.equal(concurrent.frames[1].after.players[0].lab[0].prog,concurrent.frames[100].after.players[0].lab[0].prog);
assert.equal(concurrent.frames.at(-1).after.players[0].tier,2);
const ordinary={reference:pin,traces,steps:1310,births:1,scope:'Actual unchanged Source command/full step and actual newborn while HQ research remains pending; unfinished-HQ pause/resume. No Classic queue or recipient claim.'};
clean();
const ordinaryFile=`${root}/rust/crates/simtest/tests/fixtures/base-production-hq-concurrency.json.gz`;
const compressed=gzipSync(JSON.stringify(ordinary)+'\n',{level:9});
if(process.argv.includes('--check'))assert.deepEqual(readFileSync(ordinaryFile),compressed);else writeFileSync(ordinaryFile,compressed);

async function recordComponents(){
const ref=reference;
const tech=await import(pathToFileURL(`${ref}/shared/tech.js`));
const src=readFileSync(`${ref}/shared/sim.js`,'utf8');
const section=(a,b)=>{let i=src.indexOf(a),j=src.indexOf(b,i);if(i<0||j<0)throw Error(a);return src.slice(i,j);};
const helpers=section('function productionJobs(g, b)', 'function produceUnit(g, b, owner, type)');
const research=section("  } else if (cmd.t === 'tech') {", "  } else if (cmd.t === 'cancel') {").replace(/^  } else /,'')+'\n}';
const buy=section("  } else if (cmd.t === 'buy' &&",'\n}\n\n// ---------- simulation').replace(/^  } else /,'');
const names=['UNITS','TECH','techCost','tierOf','canBuild','isConstructionMode','isSkirmishBaseMode','productionAccess','productionBuildings','popOf','popCap','popUse','priceOf'];
const values=names.map(n=>sim[n]??tech[n]);
const factory=new Function(...names,`${helpers}\nfunction tally(){}\nfunction produceUnit(g,b,owner,type){g.completed.push({building:b.id,owner,kind:type});}\nfunction command(g,slot,cmd){if(cmd.t==='tech'||cmd.t==='cancelProduction'){${research}\nreturn;}\n${buy}}\nreturn {productionJobs,queueProduction,productionRow,stepLab,train,command,techLabs};`);
const fn=factory(...values),bits=x=>{const b=Buffer.alloc(8);b.writeDoubleBE(x);return b.toString('hex');};
const player=(slot,team,faction,tier=1)=>({slot,team,faction,tier,armory:{inf:0,guns:0,armor:0},lab:[],mp:4000,mun:400,fuel:400});
const unit=(id,owner,type,built=1)=>({id,owner,type,built,hp:100,queue:[],productionJobs:[],prog:0});
const base={mode:{kind:'classic'},tech:true,naval:false,army:'standard',players:[player(0,0,0),player(1,0,1,2),player(2,1,2)],units:[unit(1,0,'hq'),unit(2,0,'barracks'),unit(3,0,'motorpool'),unit(4,0,'armory'),unit(5,0,'armory',.5),unit(6,1,'hq'),unit(7,1,'armory'),unit(8,2,'motorpool')],strikes:[],regions:Array.from({length:4},()=>({team:0,kind:'other'}))};
const scenarios=[];
function run(name,actions,change=()=>{}){
 const input=structuredClone(base);change(input);
 const g={...structuredClone(input),army:sim.CFG.armies[input.army],units:new Map(input.units.map(u=>[u.id,structuredClone(u)])),world:{regions:input.regions},completed:[]};
 const trace=[];
 for(const a of actions){let result=null;g.completed=[];
  if(a.op==='unit')Object.assign(g.units.get(a.id),a.fields);
  if(a.op==='player')Object.assign(g.players[a.slot],a.fields);
  if(a.op==='config')Object.assign(g,a.fields);
  if(a.op==='queue'){g.units.get(a.id).queue=[...a.types];fn.productionJobs(g,g.units.get(a.id));}
  if(a.op==='paid')fn.queueProduction(g,g.units.get(a.id),a.kind,a.charge);
  if(a.op==='buy')result=fn.command(g,a.slot,{t:'buy',unit:a.kind,...(a.from!==undefined?{from:sourceFrom(a.from)}:{})})??null;
  if(a.op==='tech')result=fn.command(g,a.slot,{t:'tech',kind:a.kind})??null;
  if(a.op==='cancel')result=fn.command(g,a.slot,{t:'cancelProduction',id:a.id,job:a.job})??null;
  if(a.op==='clear')for(const u of g.units.values()){u.queue=[];u.productionJobs=[];u.prog=0;}
  if(a.op==='step')for(let repeat=0;repeat<(a.repeat??1);repeat++){
   if(g.tech)for(const p of g.players)if(p.lab.length&&!p.out)fn.stepLab(g,p,a.dt);
   for(const u of g.units.values())if(u.hp>0&&sim.UNITS[u.type].building&&u.queue.length&&u.built>=1&&!g.mode?.suddenDeath){const before=u.productionJobs[0]?.id;const old=g.completed.length;fn.train(g,u,a.dt);if(g.completed.length>old)g.completed.at(-1).job_id=before;}
  }
  trace.push({action:a,result,expected:{next:g.nextProductionJob??0,players:g.players.map(p=>({money:[p.mp,p.mun,p.fuel].map(bits),tier:p.tier,armory:structuredClone(p.armory),lab:p.lab.map(j=>[j.kind,bits(j.prog),bits(j.time)])})),buildings:[...g.units.values()].map(u=>({id:u.id,progress:bits(u.prog),jobs:fn.productionRow(u)[1].map(j=>[j.id,j.type,bits(j.mp),bits(j.fuel),j.status])})),completed:g.completed,access:g.players.map(p=>[sim.productionBuildings(g,p.slot).map(u=>u.id),sim.productionBuildings(g,p.slot,'rifle').map(u=>u.id),fn.techLabs(g,p.slot,'tier'),fn.techLabs(g,p.slot,'inf')])}});
 }
 scenarios.push({name,input,trace});
}

function sourceFrom(value){if(value&&typeof value==='object'&&value.$number)return value.$number==='NaN'?NaN:Infinity;return value;}
const invalidMakers=[['foreign',6],['nonmaker',9],['numeric-string','1'],['NaN',{$number:'NaN'}],['Infinity',{$number:'Infinity'}],['null',null],['zero',0],['object',{}]];
run('all-explicit-maker-shapes-never-fall-back',[
 ...invalidMakers.map(([variant,from])=>({op:'buy',slot:0,kind:'rifle',from,variant})),
 {op:'buy',slot:0,kind:'rifle',from:1},{op:'buy',slot:0,kind:'rifle'}
],i=>{i.units.push(unit(9,0,'engineer'));});
run('five-slots-and-explicit-barracks-mg',[
 {op:'buy',slot:0,kind:'mg',from:2},
 ...Array.from({length:5},()=>({op:'buy',slot:0,kind:'rifle',from:1})),
 {op:'buy',slot:0,kind:'rifle',from:1}
]);
run('waiting-cancel-refunds-original-charge-keeps-head-progress',[
 {op:'paid',id:1,kind:'rifle',charge:{mp:123.5,fuel:7.25}},
 {op:'paid',id:1,kind:'conscript',charge:{mp:41.75,fuel:2.5}},
 {op:'step',dt:5},
 {op:'cancel',slot:1,id:1,job:2},{op:'cancel',slot:0,id:1,job:1},
 {op:'cancel',slot:0,id:1,job:2},{op:'cancel',slot:0,id:1,job:2}
]);
run('armory-needs-finished-accessible-lab',[
 {op:'tech',slot:0,kind:'inf'},
 {op:'unit',id:5,fields:{built:1}},{op:'tech',slot:0,kind:'inf'}
],i=>{i.players[0].tier=2;i.units.find(u=>u.id===4).built=.5;});
run('classic-tier-research-munitions-and-duplicate',[
 {op:'player',slot:0,fields:{mun:0}},{op:'tech',slot:0,kind:'tier'},
 {op:'player',slot:0,fields:{mun:100}},{op:'tech',slot:0,kind:'tier'},
 {op:'tech',slot:0,kind:'tier'}
]);
const first=scenarios[0];
for(const row of first.trace.slice(0,8)){
 assert.notEqual(row.result,null,`${row.action.variant}: maker must be refused`);
 assert.equal(row.expected.players[0].money[0],bits(first.input.players[0].mp));
 assert.equal(row.expected.buildings.find(b=>b.id===1).jobs.length,0);
}
assert.equal(first.trace[8].expected.buildings.find(b=>b.id===1).jobs.length,1);
assert.equal(first.trace[9].expected.buildings.find(b=>b.id===1).jobs.length,2);
const slots=scenarios[1];
assert.equal(slots.trace[0].result,null);
assert.equal(slots.trace[0].expected.buildings.find(b=>b.id===2).jobs[0][1],'mg');
assert.equal(slots.trace.at(-1).result,'queueFull');
const cancel=scenarios[2];
assert.equal(cancel.trace[3].result,'unseen');assert.equal(cancel.trace[4].result,'notWaiting');
assert.equal(cancel.trace[5].result,null);assert.equal(cancel.trace[6].result,'notWaiting');
assert.equal(cancel.trace[2].expected.buildings[0].progress,cancel.trace[5].expected.buildings[0].progress);
assert.equal(scenarios[3].trace[0].result,'needs');assert.equal(scenarios[3].trace[2].result,null);
assert.equal(scenarios[4].trace[1].result,'mun');assert.equal(scenarios[4].trace[4].result,'max');
const data=JSON.stringify({reference:pin,scope:'Unchanged extracted Source production/research/cancellation operations. Dynamic nonfinite makers stay explicitly tagged; native typed maker adapter does not claim a JSON nonfinite interface. Recipient ownership/reconnect remains outside these component traces.',scenarios})+'\n';
const file=`${root}/rust/crates/core/tests/fixtures/base-production-predicates.json`;
if(process.argv.includes('--check'))assert.equal(readFileSync(file,'utf8'),data);else writeFileSync(file,data);
console.log(`Recorded ${scenarios.length} component scenarios, ${scenarios.reduce((n,s)=>n+s.trace.length,0)} actions; ${ordinary.steps} actual full ticks and ${ordinary.births} birth`);
}
await recordComponents();
