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
const tier=trace('tier-pauses-without-access-out-but-away-and-postwinner-run',[hq0,hq1],1208,{
 0:{commands:[{command:{t:'tech',kind:'tier'}}]},1:{units:[{id:1000,fields:{built:.5}}]},3:{units:[{id:1000,fields:{built:1}}]},4:{units:[{id:1000,fields:{owner:1}}]},5:{units:[{id:1000,fields:{owner:0}}],players:[{slot:0,fields:{out:true}}]},6:{players:[{slot:0,fields:{out:false,away:true}}]},7:{winner:0},8:{commands:[{command:{t:'tech',kind:'inf'}}]}
});
assert.equal(tier.frames.at(-1).after.players[0].tier,2);assert.equal(tier.frames[8].commands[0].result,'blocked');
const armory=trace('armory-lab-order-and-restored-access',[hq0,hq1,building('armory',0,29,16),building('armory',0,39,16),building('armory',1,84,71)],1205,{
 0:{commands:[{command:{t:'tech',kind:'inf'}},{command:{t:'tech',kind:'guns'}}]},1:{units:[{id:1003,fields:{built:.5}}]},2:{commands:[{command:{t:'tech',kind:'armor'}}]},3:{units:[{id:1002,fields:{owner:1}}]},5:{units:[{id:1002,fields:{owner:0}}]},605:{units:[{id:1003,fields:{built:1}}]}
});
assert.equal(armory.frames[2].commands[0].result,'queueFull');assert.equal(armory.frames.at(-1).after.players[0].armory.inf,1);assert.equal(armory.frames.at(-1).after.players[0].armory.guns,1);
for(const [name,rally]of [['default-rally',null],['building-rally',{x:90,z:35}]]){
 const buy=trace(`immediate-conquest-buy-${name}`,[building('barracks',0,16,16,...(rally?[{rally}]:[{}])),hq1],8,{0:{commands:[{command:{t:'buy',unit:'rifle',from:1000}}]}});
 assert.equal(buy.frames[0].commands[0].result,null);assert.equal(buy.frames[0].after.nextId,1003);assert.deepEqual(buy.frames[0].unitOrder,[1000,1001,1002]);assert.equal(buy.frames[0].after.units[0].queue.length,0);
}
const restoredQueueFailures=[];
for(const hp of [sim.UNITS.barracks.hpPer,sim.UNITS.barracks.hpPer*.4]){
 const specs=[building('barracks',0,16,16,{hp,queue:['rifle']}),hq1];
 const g=make(specs),before=projection(g);let failure;
 assert.throws(()=>sim.step(g),e=>{failure={name:e.name,message:e.message};return e instanceof TypeError&&e.message.includes('suddenDeath');});
 assert.equal(g.tick,1);assert.deepEqual(g.units.get(1000).queue,['rifle']);assert.equal(g.nextId,1002);
 restoredQueueFailures.push({specs,bootstrap:g.productionFixtureBootstrap,scene:g.productionFixtureScene,before,failure,after:projection(g),sourceWitness:witness(g)});
}
clean();
const fixture={reference:pin,node:process.version,arch:process.arch,scope:{tick:'Every frame executes the complete unchanged Source sim.step; lexical observers are checked against unobserved replay.',scene:'Original constructor on an authored 96-by-96 flat map, then real placeBuilding at explicit top-left cell sites, held controls, clocks, wallets, building fields and seat flags; path retry limit is one. Constructor and placed-scene terrain, sections and retained footprints are witnessed separately; actor cells and structure IDs remain in every projection. Initial actors are discarded without clearing their footprints or sections. No native runtime result is injected.',ordinaryQueue:'Ordinary Source Buy is immediate and leaves queues empty. Restored nonempty queues are excluded because sim5352 dereferences absent g.mode. No fabricated mode object.',random:'Constructor Math.random is fixed at 0.5. Runtime Math.random rejects any unexpected purpose. Native MatchRandom is never replaced.'},restoredQueueFailures,traces,steps:traces.reduce((n,t)=>n+t.frames.length,0),phaseCalls:traces.reduce((n,t)=>n+t.frames.reduce((x,f)=>x+f.phases.length,0),0)};
const compressed=gzipSync(JSON.stringify(fixture)+'\n',{level:9});
const file=`${root}/rust/crates/simtest/tests/fixtures/ordinary-production-tick.json.gz`;
if(process.argv.includes('--check'))assert.deepEqual(readFileSync(file),compressed);else writeFileSync(file,compressed);
console.log(`${traces.length} original full-step traces, ${fixture.steps} steps, ${fixture.phaseCalls} phase observations; unobserved replay agrees`);
