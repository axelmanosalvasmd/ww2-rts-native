import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {registerHooks} from 'node:module';
import {pathToFileURL} from 'node:url';
const ref=process.env.WW2_REFERENCE??'/Users/judiazm/Projects/ww2-rts-native/reference/ww2-rts',pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
if(process.version!=='v24.21.0'||process.arch!=='x64')throw Error('Use x64 Node24.21.0');
const clean=()=>{if(execFileSync('git',['-C',ref,'rev-parse','HEAD'],{encoding:'utf8'}).trim()!==pin||execFileSync('git',['-C',ref,'status','--porcelain','--untracked-files=no'],{encoding:'utf8'}).trim())throw Error('Source pin/clean');};clean();
const humanPath=ref+'/shared/ai-human.js',humanSource=readFileSync(humanPath,'utf8'),sourceSha256=createHash('sha256').update(humanSource).digest('hex');
registerHooks({load(url,ctx,next){const out=next(url,ctx);if(url!==pathToFileURL(humanPath).href)return out;if(String(out.source)!==humanSource)throw Error('Source bytes changed');return {...out,source:String(out.source)+'\nexport {seeded as __seeded,sample as __sample,skillOf as __skillOf,ticks as __ticks};\n'};}});
const sim=await import(pathToFileURL(ref+'/shared/sim.js')),human=await import(pathToFileURL(humanPath));
const data=new DataView(new ArrayBuffer(8));const bits=x=>{data.setFloat64(0,x);return data.getBigUint64(0).toString(16).padStart(16,'0')};
const map={name:'Human timing leaf',w:64,h:64,rows:Array(64).fill('.'.repeat(64)),spawns:[{x:4,y:4},{x:59,y:59}],points:[{x:27,y:10},{x:13,y:27},{x:32,y:32}]};
let globalDraws=0;const previous=Math.random;Math.random=()=>{globalDraws++;return .5};
const makeGame=()=>sim.createGame(map,['A','B'],false,[0,1],[0,1],{mode:'conquest',tech:false,supply:false,weather:false,logistics:false});
const timing=[],opening=[];
try{
 for(const slot of [0,1])for(const level of ['easy','normal','hard','unknown']){
  const g=makeGame(),skill=human.__skillOf(level),privateSeed=((g.seed??0)^Math.imul(slot+1,0x9E3779B1))>>>0;
  const rng=human.__seeded(privateSeed),tape=[],state={rng:()=>{const n=rng();tape.push(bits(n));return n;}};
  const samples=[];for(const[k,spread]of [['opening',.2],['react',.35],['notice',.35],['input',.35],['scan',.5]]){
   const start=tape.length,n=human.__sample(state,skill[k],spread);samples.push({kind:k,median:bits(skill[k]),spread:bits(spread),sample:bits(n),ticks:human.__ticks(n),random:tape.slice(start)});
  }
  timing.push({slot,level,matchSeed:g.seed,privateSeed,skill,samples});
  const before={needs:human.commanderNeedsView(g,slot),camera:human.commanderCamera(g,slot)};
  human.tickCommander(g,slot,{level});
  const after={needs:human.commanderNeedsView(g,slot),camera:human.commanderCamera(g,slot)};
  let firstNeeds=null;for(let tick=1;tick<400;tick++){g.tick=tick;if(human.commanderNeedsView(g,slot)){firstNeeds=tick;break;}human.tickCommander(g,slot,{level});}
  if(firstNeeds!==samples[0].ticks-1)throw Error('Public opening demand disagrees with original sample');
  opening.push({slot,level,before,after,firstNeeds,openingAt:firstNeeds+1,camera:human.commanderCamera(g,slot)});
 }
 const transitions=[];
 for(const slot of [0,1]){
  const g=makeGame();human.tickCommander(g,slot,{level:'easy'});
  const levels=['hard','normal','unknown','hard','easy'];const edits=[];
  for(let i=0;i<levels.length;i++){g.tick=i+1;human.tickCommander(g,slot,{level:levels[i]});edits.push({tick:g.tick,level:levels[i],needs:human.commanderNeedsView(g,slot),camera:human.commanderCamera(g,slot)});}
  let firstNeeds;for(let t=6;t<400;t++){g.tick=t;if(human.commanderNeedsView(g,slot)){firstNeeds=t;break;}human.tickCommander(g,slot,{level:'hard'});}
  const easyBoundary=opening.find(r=>r.slot===slot&&r.level==='easy').firstNeeds;if(firstNeeds!==easyBoundary)throw Error('Changing level rescheduled opening');
  const resetAt=firstNeeds+10;g.tick=resetAt;human.resetCommander(g,slot);const reset={needs:human.commanderNeedsView(g,slot),camera:human.commanderCamera(g,slot)};
  human.tickCommander(g,slot,{level:'hard'});const reinitialized={needs:human.commanderNeedsView(g,slot),camera:human.commanderCamera(g,slot)};
  let resetNeeds;for(let t=resetAt+1;t<400;t++){g.tick=t;if(human.commanderNeedsView(g,slot)){resetNeeds=t;break;}human.tickCommander(g,slot,{level:'hard'});}
  if(resetNeeds!==resetAt+opening.find(r=>r.slot===slot&&r.level==='hard').firstNeeds)throw Error('Reset private seed replay differs');
  transitions.push({slot,edits,firstNeeds,resetAt,reset,reinitialized,resetNeeds});
 }
 const componentCases=[];
 for(const seed of [0,1,0x80000000,0xffffffff])for(const slot of [0,1,5])for(const level of ['easy','normal','hard','unknown']){
  const skill=human.__skillOf(level),privateSeed=(seed^Math.imul(slot+1,0x9E3779B1))>>>0;
  const sourceRng=human.__seeded(privateSeed);let draws=0;const state={rng:()=>{draws++;return sourceRng();}},steps=[];
  const append=(kind,n,fields={})=>steps.push({kind,...fields,secondsBits:bits(n),ticks:human.__ticks(n)});
  append('opening',human.__sample(state,skill.opening,.2));
  for(const onScreen of [true,false])append('reaction',Math.max(.2,human.__sample(state,onScreen?skill.react:skill.notice)),{onScreen});
  append('input',human.__sample(state,skill.input));
  for(const load of [0,.5,1])append('scan',human.__sample(state,skill.scan,.5)*(1+3*load),{load});
  if(draws!==14)throw Error('Seven timing samples must consume14 private draws');
  componentCases.push({seed,slot,level,privateSeed,draws,steps});
 }
 clean();const result={reference:pin,node:process.version,arch:process.arch,sourceSha256,scope:'Component exports only seeded/sample/skillOf/ticks appended in memory; public opening/view-demand/camera/reset calls unchanged; no delivered view, planner or actuator execution; clock values are leaf inputs, not full match steps.',startupRandom:{value:.5,draws:globalDraws},skills:human.HUMAN_SKILLS,componentCases,opening,transitions};
 const fixturePath=process.env.HUMAN_TIMING_FIXTURE??'/private/tmp/human-timing-source-oracle.json';
 const output=JSON.stringify(result)+'\n';
 if(process.argv.includes('--check')){if(readFileSync(fixturePath,'utf8')!==output)throw Error('Human timing fixture differs');}else writeFileSync(fixturePath,output);
 console.log(`Verified ${componentCases.length} private timing profiles, ${componentCases.reduce((n,r)=>n+r.steps.length,0)} sequential samples, ${opening.length} public openings, ${transitions.length} reset/level-change receipts`);
}finally{Math.random=previous;}
