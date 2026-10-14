import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {registerHooks} from 'node:module';
import {pathToFileURL,fileURLToPath} from 'node:url';
import {resolve} from 'node:path';
const root=fileURLToPath(new URL('../',import.meta.url));
const ref=process.env.WW2_REFERENCE??resolve(root,'reference/ww2-rts'),pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const protectedPaths=['shared/ai-human.js','shared/ai.js','shared/sim.js','shared/ai-schedule.js'];const protectedBytes=new Map(protectedPaths.map(p=>[p,readFileSync(ref+'/'+p,'utf8')]));
const clean=()=>{if(execFileSync('git',['-C',ref,'rev-parse','HEAD'],{encoding:'utf8'}).trim()!==pin||execFileSync('git',['-C',ref,'status','--porcelain','--untracked-files=no'],{encoding:'utf8'}).trim())throw Error('Source pin or tracked cleanliness changed');for(const[p,source]of protectedBytes)if(readFileSync(ref+'/'+p,'utf8')!==source)throw Error('Read-only Source bytes changed');};clean();
if(process.version!=='v24.21.0'||process.arch!=='x64')throw Error('Exact x64 Node24.21.0');
const path=ref+'/shared/ai-human.js',original=readFileSync(path,'utf8'),sourceSha256=createHash('sha256').update(original).digest('hex');
const aiPath=ref+'/shared/ai.js',aiOriginal=readFileSync(aiPath,'utf8'),aiSourceSha256=createHash('sha256').update(aiOriginal).digest('hex');
registerHooks({load(url,c,next){const out=next(url,c);
 if(url===pathToFileURL(path).href){if(String(out.source)!==original)throw Error('Human Source bytes changed');return {...out,source:String(out.source)+'\nexport const __state=(g,slot)=>STATE.get(g)?.[slot];\n'};}
 if(url===pathToFileURL(aiPath).href){if(String(out.source)!==aiOriginal)throw Error('Planner Source bytes changed');return {...out,source:String(out.source)+`
const __originalThink=think;let __policyObserver=null;
export const __observePolicy=observer=>{__policyObserver=observer;};
think=function(g,slot,opts={}){if(!__policyObserver)return __originalThink(g,slot,opts);const before=__policyObserver.before(g,slot,opts),attempts=[],fractions=[],submit=opts.submit??(cmd=>command(g,slot,cmd)),random=Math.random;Math.random=()=>{const value=random();fractions.push(value);return value;};let result;try{result=__originalThink(g,slot,{...opts,submit:cmd=>{const command=structuredClone(cmd),returned=submit(cmd);attempts.push({command,returned:returned===undefined?{kind:'undefined'}:{kind:'value',value:structuredClone(returned)}});return returned;}});}finally{Math.random=random;}__policyObserver.after(g,slot,opts,before,attempts,fractions,result);return result;};
`};}
 return out;
}});
const sim=await import(pathToFileURL(ref+'/shared/sim.js')),human=await import(pathToFileURL(path)),schedule=await import(pathToFileURL(ref+'/shared/ai-schedule.js')),ai=await import(pathToFileURL(aiPath));
const clone=x=>JSON.parse(JSON.stringify(x,(_k,v)=>v instanceof Map?[...v]:v instanceof Set?[...v]:v));
const runs=values=>{const out=[];for(const value of values){if(out.length&&out.at(-1)[0]===value)out.at(-1)[1]++;else out.push([value,1]);}return out;};
const policyFrame=(view,slot)=>clone({...view,chars:runs(view.chars),flags:runs(view.flags),height:view.height?runs(view.height):null,mapChars:runs(view.mapChars),mapHeight:view.mapHeight?runs(view.mapHeight):null,units:[...view.units.values()],me:view.players[slot]});
const policyMemory=(g,slot)=>{const mind=ai.aiMemories(g)?.[slot]?.mind;return mind?{available:true,aimPresent:Object.hasOwn(mind,'aim'),aim:mind.aim===undefined?null:clone(mind.aim),pointFailures:mind.points?.map(p=>p.failed===undefined?null:p.failed)??null}: {available:false};};
const map={name:'Default human real caller',w:64,h:64,rows:Array(64).fill('.'.repeat(64)),spawns:[{x:4,y:4},{x:59,y:59}],points:[{x:27,y:10},{x:13,y:27},{x:32,y:32}]};
const originalObserve=schedule.commander.observe,originalAct=schedule.commander.act,cases=[];
for(const[slot,level]of [[0,'easy'],[0,'normal'],[1,'hard']]){
 let initializationSeed=null,actualPrivateSeed=null;
 const policyCalls=[];
 const batches=[],generations=new WeakMap();let nextGeneration=1,sourceSpawn;
 const ownerBatch=g=>{if(!generations.has(g.shots))generations.set(g.shots,nextGeneration++);return {tick:g.tick,generation:generations.get(g.shots),through_ordinal:g.shots.length,events:g.shots.flatMap((e,ordinal)=>e.k==='hurt'&&e.to===slot&&e.t&&Number.isFinite(e.x)?[{ordinal,id:e.t,at:{x:e.x,z:e.z}}]:[])};};
 const deliveries=[],inputs=[],alerts=[],timeline=[],draws={startup:[],step:[],humanGlobal:[]};let commandAt=null,sourceSeed;
 const recordRng=(phase,fn)=>{const source=Math.random;Math.random=()=>{const n=source();draws[phase].push(n);return n};try{return fn();}finally{Math.random=source;}};
 const readState=(g)=>{const s=human.__state(g,slot);return s?clone({camera:s.camera,selection:s.selection,nextInput:s.nextInput,viewTick:s.viewTick,look:s.look,inputs:s.inputs,recent:s.recent}):null;};
 schedule.commander.observe=(g,s,cache)=>{const view=originalObserve(g,s,cache);deliveries.push({packetFrame:policyFrame(view,s),tick:g.tick,viewTick:view.tick,mp:view.players[s].mp,rawMP:g.players[s].mp,visible:[...view.players[s].visible],own:[...view.units.values()].filter(u=>u.owner===s).map(u=>({id:u.id,type:u.type,x:u.x,z:u.z,path:u.path,worldGoal:u.worldGoal})),viewSnapshot:{tick:view.tick,spawn:clone(view.players[s].spawn),units:clone([...view.units.values()]),visible:[...view.players[s].visible],sightings:clone(view.sightings??[])}});return view;};
 schedule.commander.act=(g,s,opts)=>{if(initializationSeed===null&&!human.__state(g,s)&&!g.players[s]?.out&&g.winner===null){initializationSeed=g.seed;actualPrivateSeed=(initializationSeed^Math.imul(s+1,0x9E3779B1))>>>0;}batches.push({...ownerBatch(g),control:{now:g.tick,live_owner_out:!!g.players[s].out,winner:g.winner!==null,level:opts.level}});return recordRng('humanGlobal',()=>originalAct(g,s,opts));};
 ai.__observePolicy({before:(g,s,opts)=>({tick:g.tick,slot:s,level:opts.level,frame:policyFrame(opts.view,s),memory:policyMemory(g,s)}),after:(g,s,opts,before,attempts,fractions,result)=>policyCalls.push({tick:g.tick,slot:s,level:opts.level,deliveredViewTick:opts.view.tick,frame:before.frame,memoryBefore:before.memory,memoryAfter:policyMemory(g,s),attempts,fractions,thinkReturn:result===undefined?{kind:'undefined'}:{kind:'value',value:clone(result)}})});
 const source={...sim,createGame:(...a)=>recordRng('startup',()=>{const g=sim.createGame(...a);sourceSeed=g.seed;sourceSpawn=clone(g.players[slot].spawn);return g;}),step:g=>recordRng('step',()=>sim.step(g))};
 let g;try{
 g=schedule.playMatch({map,names:['A','B'],teams:[0,1],factions:[0,1],shuffle:false,seed:0x516e9200,options:{mode:'conquest',tech:false,supply:false,weather:false,logistics:false},seats:[{slot,level,options:{onInput:i=>{inputs.push(clone(i));if(i.cmd&&i.accepted&&commandAt===null)commandAt=i.tick;},onAlert:a=>alerts.push(clone(a))}}],maxTicks:600,sim:source,setup:g=>{g.players[slot].mp=0;for(const u of g.units.values())if(u.owner!==slot)Object.assign(u,{holdFire:true,autoRetreat:false,holdPos:true});},until:g=>commandAt!==null&&g.tick>commandAt,onTick:(g,{sent})=>{const state=readState(g),previous=timeline.at(-1);const receipt={tick:g.tick,sent,mp:g.players[slot].mp,needsView:human.commanderNeedsView(g,slot),state};if(!previous||state?.viewTick!==previous.state?.viewTick||state?.look?.at!==previous.state?.look?.at||state?.inputs[0]?.kind!==previous.state?.inputs[0]?.kind||inputs.at(-1)?.tick===g.tick||commandAt!==null)timeline.push(receipt);}});
 }finally{schedule.commander.observe=originalObserve;schedule.commander.act=originalAct;ai.__observePolicy(null);}
 if(commandAt===null||g.tick!==commandAt+1)throw Error('First command plus next physical tick not reached');
 cases.push({slot,level,sourceSpawn,batches,policyCalls,sourceSeed,initializationSeed,actualPrivateSeed,privateSeed:(sourceSeed^Math.imul(slot+1,0x9E3779B1))>>>0,deliveries,inputs,alerts,timeline,commandAt,draws,final:{tick:g.tick,mp:g.players[slot].mp,state:readState(g),own:[...g.units.values()].filter(u=>u.owner===slot).map(u=>({id:u.id,type:u.type,x:u.x,z:u.z,path:u.path,worldGoal:u.worldGoal}))}});
}
clean();const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,sourceSha256,aiSourceSha256,sourceHashVerification:{pin,method:"Git HEAD and tracked cleanliness before/after, module SHA256 and unchanged guarded bytes"},seed:0x516e9200,initializationSeedProvenance:'sourceSeed/privateSeed retain historical creation projections. initializationSeed/actualPrivateSeed record the seed immediately before first original act initializes stateOf.',scope:'Actual playMatch default commander. Raw thinker wrapper delegates unchanged body and original submit returns; fractions are actual source seeded global RNG, not native RNG. Source human collector returns undefined before motor canonical admission. No private RNG calls; public onInput/onAlert and read-only state export; actual delivered viewSnapshot and original indexed owner hurt batches support component replay, not a native full-policy claim; global seeded RNG is distinct from the private timing stream.',cases})+'\n';
const fixturePath=process.env.HUMAN_POLICY_FIXTURE??resolve(root,'rust/crates/core/tests/fixtures/human-commander-policy.json');
if(process.argv.includes('--check')){if(readFileSync(fixturePath,'utf8')!==output)throw Error('Human commander policy fixture differs');}else writeFileSync(fixturePath,output);
for(const c of cases)console.log(JSON.stringify({slot:c.slot,level:c.level,policyCalls:c.policyCalls.map(p=>({tick:p.tick,viewTick:p.deliveredViewTick,mp:p.frame.me.mp,ids:p.frame.units.map(u=>u.id),attempts:p.attempts,fractions:p.fractions,memoryBefore:p.memoryBefore,memoryAfter:p.memoryAfter})),commandAt:c.commandAt,finalTick:c.final.tick}));
