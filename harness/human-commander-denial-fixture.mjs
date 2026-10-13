import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {registerHooks} from 'node:module';
import {pathToFileURL,fileURLToPath} from 'node:url';
import {resolve} from 'node:path';
const root=fileURLToPath(new URL('../',import.meta.url));
const ref=process.env.WW2_REFERENCE??resolve(root,'reference/ww2-rts'),pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const clean=()=>{if(execFileSync('git',['-C',ref,'rev-parse','HEAD'],{encoding:'utf8'}).trim()!==pin||execFileSync('git',['-C',ref,'status','--porcelain','--untracked-files=no'],{encoding:'utf8'}).trim())throw Error('Source pin or clean guard');};clean();
if(process.version!=='v24.21.0'||process.arch!=='x64')throw Error('Exact x64 Node24.21.0');
const path=ref+'/shared/ai-human.js',original=readFileSync(path,'utf8'),sourceSha256=createHash('sha256').update(original).digest('hex');
registerHooks({load(url,c,next){const out=next(url,c);if(url!==pathToFileURL(path).href)return out;if(String(out.source)!==original)throw Error('Source bytes changed');return {...out,source:String(out.source)+'\nexport const __state=(g,slot)=>STATE.get(g)?.[slot];\n'};}});
const sim=await import(pathToFileURL(ref+'/shared/sim.js')),human=await import(pathToFileURL(path)),schedule=await import(pathToFileURL(ref+'/shared/ai-schedule.js'));
const clone=x=>JSON.parse(JSON.stringify(x,(_k,v)=>v instanceof Map?[...v]:v instanceof Set?[...v]:v));
const map={name:'Default human real caller',w:64,h:64,rows:Array(64).fill('.'.repeat(64)),spawns:[{x:4,y:4},{x:59,y:59}],points:[{x:27,y:10},{x:13,y:27},{x:32,y:32}]};
const originalObserve=schedule.commander.observe,originalAct=schedule.commander.act,cases=[];
for(const[slot,level]of [[0,'normal']]){
 const batches=[],generations=new WeakMap();let nextGeneration=1,sourceSpawn;
 const ownerBatch=g=>{if(!generations.has(g.shots))generations.set(g.shots,nextGeneration++);return {tick:g.tick,generation:generations.get(g.shots),through_ordinal:g.shots.length,events:g.shots.flatMap((e,ordinal)=>e.k==='hurt'&&e.to===slot&&e.t&&Number.isFinite(e.x)?[{ordinal,id:e.t,at:{x:e.x,z:e.z}}]:[])};};
 const deliveries=[],inputs=[],alerts=[],timeline=[],draws={startup:[],step:[],humanGlobal:[]};let commandAt=null,sourceSeed,liveGame;const lifecycleOverlays=[];
 const recordRng=(phase,fn)=>{const source=Math.random;Math.random=()=>{const n=source();draws[phase].push(n);return n};try{return fn();}finally{Math.random=source;}};
 const readState=(g)=>{const s=human.__state(g,slot);return s?clone({camera:s.camera,selection:s.selection,nextInput:s.nextInput,viewTick:s.viewTick,look:s.look,inputs:s.inputs,recent:s.recent,lastPlace:s.lastPlace,lastPlaceTick:s.lastPlaceTick,pending:s.pending}):null;};
 schedule.commander.observe=(g,s,cache)=>{const view=originalObserve(g,s,cache);deliveries.push({tick:g.tick,viewTick:view.tick,mp:view.players[s].mp,rawMP:g.players[s].mp,visible:[...view.players[s].visible],own:[...view.units.values()].filter(u=>u.owner===s).map(u=>({id:u.id,type:u.type,x:u.x,z:u.z,path:u.path,worldGoal:u.worldGoal})),viewSnapshot:{tick:view.tick,spawn:clone(view.players[s].spawn),units:clone([...view.units.values()]),visible:[...view.players[s].visible],sightings:clone(view.sightings??[])}});return view;};
 schedule.commander.act=(g,s,opts)=>{batches.push({...ownerBatch(g),control:{now:g.tick,live_owner_out:!!g.players[s].out,winner:g.winner!==null,level:opts.level}});return recordRng('humanGlobal',()=>originalAct(g,s,opts));};
 const source={...sim,createGame:(...a)=>recordRng('startup',()=>{const g=sim.createGame(...a);sourceSeed=g.seed;sourceSpawn=clone(g.players[slot].spawn);return g;}),step:g=>recordRng('step',()=>sim.step(g))};
 let g;try{
 g=schedule.playMatch({map,names:['A','B'],teams:[0,1],factions:[0,1],shuffle:false,seed:0x516e9200,options:{mode:'conquest',tech:false,supply:false,weather:false,logistics:false},seats:[{slot,level,options:{onInput:i=>{inputs.push(clone(i));if(i.kind==='select'&&!lifecycleOverlays.length){liveGame.players[slot].away=true;lifecycleOverlays.push({tick:i.tick,appliedAfter:'original onInput select',kind:'direct public room-lifecycle state overlay',away:true,out:!!liveGame.players[slot].out});}if(i.cmd&&commandAt===null)commandAt=i.tick;},onAlert:a=>alerts.push(clone(a))}}],maxTicks:600,sim:source,setup:g=>{liveGame=g;g.players[slot].mp=0;for(const u of g.units.values())if(u.owner!==slot)Object.assign(u,{holdFire:true,autoRetreat:false,holdPos:true});},until:g=>commandAt!==null&&g.tick>commandAt,onTick:(g,{sent})=>{const state=readState(g),previous=timeline.at(-1);const receipt={tick:g.tick,sent,mp:g.players[slot].mp,needsView:human.commanderNeedsView(g,slot),state};if(!previous||state?.viewTick!==previous.state?.viewTick||state?.look?.at!==previous.state?.look?.at||state?.inputs[0]?.kind!==previous.state?.inputs[0]?.kind||inputs.at(-1)?.tick===g.tick||commandAt!==null)timeline.push(receipt);}});
 }finally{schedule.commander.observe=originalObserve;schedule.commander.act=originalAct;}
 if(commandAt===null||g.tick!==commandAt+1)throw Error('First command plus next physical tick not reached');
 cases.push({slot,level,sourceSpawn,batches,lifecycleOverlays,sourceSeed,privateSeed:(sourceSeed^Math.imul(slot+1,0x9E3779B1))>>>0,deliveries,inputs,alerts,timeline,commandAt,draws,final:{tick:g.tick,mp:g.players[slot].mp,away:!!g.players[slot].away,out:!!g.players[slot].out,state:readState(g),own:[...g.units.values()].filter(u=>u.owner===slot).map(u=>({id:u.id,type:u.type,x:u.x,z:u.z,path:u.path,worldGoal:u.worldGoal}))}});
}
clean();const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,sourceSha256,seed:0x516e9200,scope:'Actual playMatch default commander with an explicitly recorded direct public away overlay after original selection; source simulation/brain/command bodies unchanged, no fabricated admission return or private RNG calls; public onInput/onAlert and read-only state export; actual delivered viewSnapshot and original indexed owner hurt batches support component replay, not a native full-policy claim; global seeded RNG is distinct from the private timing stream.',cases})+'\n';
const fixturePath=process.env.HUMAN_AWAY_DENIAL_FIXTURE??resolve(root,'rust/crates/core/tests/fixtures/human-commander-denial.json');
if(process.argv.includes('--check')){if(readFileSync(fixturePath,'utf8')!==output)throw Error('Default human away denial receipt differs');}else writeFileSync(fixturePath,output);
for(const c of cases)console.log(JSON.stringify({slot:c.slot,level:c.level,sourceSeed:c.sourceSeed,privateSeed:c.privateSeed,deliveries:c.deliveries.map(r=>({tick:r.tick,mp:r.mp,rawMP:r.rawMP})),inputs:c.inputs,alerts:c.alerts,opening:c.timeline[0],commandAt:c.commandAt,finalTick:c.final.tick,mp:c.final.mp,globalDraws:Object.fromEntries(Object.entries(c.draws).map(([k,v])=>[k,v.length])),physical:c.final.own.map(u=>({id:u.id,x:u.x,z:u.z,worldGoal:u.worldGoal}))}));
