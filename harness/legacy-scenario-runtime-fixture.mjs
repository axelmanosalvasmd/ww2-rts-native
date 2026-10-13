// Record the five shipped legacy-trigger maps through original complete sim.step calls.
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {gzipSync} from 'node:zlib';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {registerHooks} from 'node:module';

const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};
assert.equal(process.version,'v24.21.0');
clean();
const simURL=pathToFileURL(resolve(reference,'shared/sim.js')).href;
registerHooks({load(url,context,next){
  const result=next(url,context);
  if(url!==simURL)return result;
  return {...result,source:String(result.source)+'\nexport const __scenarioTools=SCENARIO_TOOLS;\n'};
}});
const sim=await import(simURL);
const specs=[
  ['el-alamein',[59,60,61]],
  ['hurtgen-forest',[59,60,61]],
  ['prokhorovka',[59,60,61]],
  ['seelow-heights',[59,60,61]],
  ['the-great-bridge',[5999,6000,6001,7999,8000,8001,8399,8400,8401]],
];
function load(name){return JSON.parse(readFileSync(resolve(reference,'maps',`${name}.json`),'utf8'));}
function recipientProjection(g){return [0,1].map(slot=>{const snapshot=sim.snapshotFor(g,slot,g.shots);return{slot,scenario:structuredClone(snapshot.scenario??null),shots:structuredClone(snapshot.shots)};});}
function recipientInputs(g){const falling=(g.fallingSections??[]).filter(section=>!section.settled);return{reveal:!!g.reveal,mode:g.mode?.kind??null,weather:g.weather?.now??null,players:g.players.map((p,slot)=>({slot,team:p.team,out:!!p.out,visible:[...p.visible].sort((a,b)=>a-b)})),units:[...g.units.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,x:u.x,z:u.z,air:u.air?{state:u.air.state}:null,garrison:u.garrison,riding:!!u.riding,built:u.built??null,hp:u.hp})),fallingCount:falling.length,fallingSections:falling.map(s=>({id:s.id,kind:s.kind,section:s.section,c:s.c,x:s.x,z:s.z,y:s.y,dir:s.dir,time:s.time,until:s.until,impulse:s.impulse,mass:s.mass,material:s.material,radius:s.radius})),terrain:{w:g.w,h:g.h,flags:Array.from(g.flags),height:g.height?Array.from(g.height):null},smokes:g.smokes.map(s=>({x:s.x,z:s.z,r:s.r,id:s.id})),strikes:g.strikes.map(s=>({kind:s.kind,x:s.x,z:s.z,dir:s.dir,t:s.t,owner:s.owner}))};}
function structureSections(g){return[...new Set(g.structuralCells?.values()??[])].map(s=>({id:s.id,c:s.c,structureId:s.structureId,hp:s.hp,maxHp:s.maxHp,state:s.state,material:s.material,anchor:s.anchor,supports:structuredClone(s.supports??[]),rubble:structuredClone(s.rubble??[]),failedAt:s.failedAt,direction:structuredClone(s.direction),impulse:s.impulse,entityId:s.entityId})).sort((a,b)=>a.c-b.c);}
function run(name,ticks,failAfterExecution=null){
  const map=load(name), callbacks=[];
  const saved=Math.random;Math.random=()=>0.5;
  let g;try{g=sim.createGame(map,['A','B'],false,[0,1],[0,1],{mode:'conquest',weather:false,tech:false,supply:false,logistics:false,mapKey:name});}
  finally{Math.random=saved;}
  const damage=sim.__scenarioTools.damage;
  sim.__scenarioTools.damage=(state,action,execution)=>{
    callbacks.push({kind:'damage.begin',execution,action:structuredClone(action)});
    const result=damage(state,action,execution);
    callbacks.push({kind:'damage.commit',execution,cellLogLength:state.cellLog.length});
    if(execution===failAfterExecution)throw new Error(`injected after original damage callback ${execution}`);
    return result;
  };
  const selected=[];let previousCellLog= g.cellLog.length;
  for(let target=1;target<=ticks.at(-1);target++){
    const beforeBridgeSections=name==='the-great-bridge'&&target===8400?structureSections(g):null;
    g.shots=[];
    const appended=[];
    Object.defineProperty(g.shots,'push',{value:function(...rows){
      for(const row of rows)appended.push(structuredClone(row));
      return Array.prototype.push.apply(this,rows);
    }});
    callbacks.length=0;
    const savedRandom=Math.random;Math.random=()=>0.5;
    let failure=null;
    try{sim.step(g);}catch(error){failure=String(error?.message??error);}finally{Math.random=savedRandom;}
    assert.equal(g.tick,target,`${name}: source step clock`);
    if(ticks.includes(g.tick)){
      const state=g.scenario?structuredClone(g.scenario):null;
      const frame={tick:g.tick,shots:appended,recipientInputs:recipientInputs(g),recipients:recipientProjection(g),callbacks:structuredClone(callbacks),scenario:state,
        cellChanges:structuredClone(g.cellLog.slice(previousCellLog)),failure};
      if(beforeBridgeSections){frame.structureSectionsBefore=beforeBridgeSections;frame.structureSectionsAfter=structureSections(g);}
      selected.push(frame);
    }
    previousCellLog=g.cellLog.length;
    if(failure){assert.equal(target,ticks.at(-1),`${name}: unexpected callback failure tick`);break;}
  }
  sim.__scenarioTools.damage=damage;
  return {name,map,steps:selected};
}
const traces=specs.map(([name,ticks])=>run(name,ticks));
const bridge8400=traces.find(trace=>trace.name==='the-great-bridge').steps.find(step=>step.tick===8400);
assert.equal(bridge8400.recipientInputs.fallingCount,84,'Great Bridge tick 8400 unsettled Source falling sections');
assert.equal(bridge8400.structureSectionsBefore.length,516,'Great Bridge structural section count before collapse');
assert.equal(bridge8400.structureSectionsAfter.length,516,'Great Bridge structural section count after collapse');
const bridgeStructureBefore=bridge8400.structureSectionsBefore.filter(section=>section.structureId==='map:bridge:5573');
const bridgeStructureAfter=bridge8400.structureSectionsAfter.filter(section=>section.structureId==='map:bridge:5573');
assert.equal(bridgeStructureBefore.length,84,'Great Bridge structure section count before collapse');
assert(bridgeStructureBefore.every(section=>section.state==='intact'&&section.hp===2400),'Great Bridge sections begin intact at full HP');
assert.equal(bridgeStructureAfter.length,84,'Great Bridge structure section count after collapse');
assert(bridgeStructureAfter.every(section=>section.state==='failed'&&section.hp===0),'Great Bridge sections fail at zero HP');
assert.equal(bridge8400.structureSectionsAfter.filter(section=>section.state==='failed').length,84,'Great Bridge failed bridge section count');
assert.equal(bridge8400.structureSectionsAfter.filter(section=>section.state==='failed').reduce((sum,section)=>sum+section.hp,0),0,'failed bridge sections have zero HP');
const failureTrace=run('the-great-bridge',[8400],'scenario:3:1');
const failedFrame=failureTrace.steps[0];
assert.match(failedFrame.failure,/injected after original damage callback scenario:3:1/);
assert.equal(failedFrame.shots[0].k,'say');
assert.equal(failedFrame.scenario.triggers['legacy:2@match'].count,1);
assert(failedFrame.cellChanges.length>0,'original damage callback must commit terrain before injected failure');
clean();
const out={reference:pin,node:process.version,arch:process.arch,
  scope:'Pinned complete createGame and sim.step callbacks, actual SCENARIO_TOOLS.damage execution, chronological public shots, and selected per-seat snapshotFor scenario/shot projections; no replacement rules or custom trigger evaluator. The failure frame injects an error only after the original damage callback commits, to record its real output/state prefix.',
  traces,failureTrace};
const path=resolve(root,'rust/crates/simtest/tests/fixtures/legacy-scenario-runtime.json.gz');
const bytes=gzipSync(JSON.stringify(out)+'\n',{level:9});
if(process.argv.includes('--check'))assert.deepEqual(readFileSync(path),bytes);
else writeFileSync(path,bytes);
console.log(`${traces.length} shipped maps, ${traces.reduce((n,t)=>n+t.steps.length,0)} selected complete-step frames plus one post-commit failure-prefix frame`);
