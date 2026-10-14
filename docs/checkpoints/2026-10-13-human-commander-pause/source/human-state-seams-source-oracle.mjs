import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {registerHooks} from 'node:module';
import {pathToFileURL} from 'node:url';
const ref=process.env.WW2_REFERENCE??'/Users/judiazm/Projects/ww2-rts-native/reference/ww2-rts',pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
if(process.version!=='v24.21.0'||process.arch!=='x64')throw Error('Use exact x64 Node24.21.0');
const clean=()=>{if(execFileSync('git',['-C',ref,'rev-parse','HEAD'],{encoding:'utf8'}).trim()!==pin||execFileSync('git',['-C',ref,'status','--porcelain','--untracked-files=no'],{encoding:'utf8'}).trim())throw Error('Source pin/clean');};clean();
const path=ref+'/shared/ai-human.js',original=readFileSync(path,'utf8'),sourceSha256=createHash('sha256').update(original).digest('hex');
registerHooks({load(url,c,next){const out=next(url,c);if(url!==pathToFileURL(path).href)return out;if(String(out.source)!==original)throw Error('Source bytes changed');return {...out,source:String(out.source)+'\nexport {stateOf as __stateOf,startLook as __startLook,planNow as __planNow,runInput as __runInput,inputsFor as __inputsFor};\n'};}});
const sim=await import(pathToFileURL(ref+'/shared/sim.js')),ai=await import(pathToFileURL(ref+'/shared/ai.js')),human=await import(pathToFileURL(path));
const clone=x=>JSON.parse(JSON.stringify(x,(_k,v)=>v instanceof Map?[...v]:v instanceof Set?[...v]:v));
const buffer=new DataView(new ArrayBuffer(8));const bits=n=>{buffer.setFloat64(0,n);return buffer.getBigUint64(0).toString(16).padStart(16,'0')};
const map={name:'Constructed human seams',w:64,h:64,rows:Array(64).fill('.'.repeat(64)),spawns:[{x:4,y:4},{x:59,y:59}],points:[{x:27,y:10},{x:13,y:27},{x:32,y:32}]};
const options={mode:'conquest',tech:false,supply:false,weather:false,logistics:false};
const previous=Math.random;let globalDraws=0;const globalTape=[];Math.random=()=>{globalDraws++;globalTape.push(.5);return .5};const cases=[];
const units=g=>clone([...g.units.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,x:u.x,z:u.z,path:u.path,worldGoal:u.worldGoal,orders:u.orders,dig:u.dig,holdPos:u.holdPos})));
try{
 for(const spec of [
  {name:'peak-cap-inclusive-window',now:200,recent:Array.from({length:20},(_,i)=>181+i),calls:[200,380,381]},
  {name:'minute-cap-inclusive-expiry',now:2000,recent:Array.from({length:68},(_,i)=>801+i*17),calls:[2000,2001]},
  {name:'local-gap-release-at-five',now:104,lastPlace:{x:0,z:0},lastPlaceTick:100,target:{x:90,z:90},calls:[104,105]},
  {name:'local-screen-inclusive-no-gap',now:104,lastPlace:{x:0,z:0},lastPlaceTick:100,target:{x:55,z:40},calls:[104]},
  {name:'urgent-look-resumes-untouched-paid-digger',now:100,resume:true,calls:[100]},
 ]){
  const beforeGlobal=globalDraws,g=sim.createGame(map,['A','B'],false,[0,1],[0,1],options);g.tick=spec.now;
  const setupCommands=[];
  if(spec.resume){const command={t:'dig',ids:[1],x:29,z:27,dir:0},walletBefore=g.players[0].mp,denial=sim.command(g,0,command)??null;setupCommands.push({command,denial,walletBefore,walletAfter:g.players[0].mp,dig:clone(g.units.get(1).dig)});if(denial||!g.units.get(1).dig||g.players[0].mp!==120)throw Error('Real paid Dig setup failed');}
  const view=ai.observe(g,0),viewSnapshot={tick:view.tick,spawn:clone(view.players[0].spawn),units:clone([...view.units.values()]),visible:[...view.players[0].visible],sightings:clone(view.sightings??[])};
  const stateSeed=g.seed,s=human.__stateOf(g,0,'normal'),stateOfResult=clone(s),sourceRng=s.rng,privateTape=[];s.rng=()=>{const n=sourceRng();privateTape.push(bits(n));return n};
  const command=spec.resume?{t:'move',orders:[[1,90,90]]}:spec.target?{t:'move',orders:[[1,spec.target.x,spec.target.z]]}:{t:'stance',ids:[1],key:'holdPos',on:true};
  const cursor={camera:clone(s.camera),selection:'1'},inputs=human.__inputsFor(cursor,command,view).map(i=>({...i,origin:spec.resume?90:spec.now}));
  const overlay={selection:cursor.selection,camera:cursor.camera,inputs:clone(inputs),look:null,nextInput:0,recent:spec.recent??[],lastPlace:spec.lastPlace??null,lastPlaceTick:spec.lastPlaceTick??-1e9};
  Object.assign(s,clone(overlay));let componentAlert=null;
  if(spec.resume){const alert={tick:100,id:1,x:13,z:9,onScreen:true,due:100,answered:null};componentAlert=alert;s.pending=[alert];s.look={at:100,urgent:true,target:null,causes:[]};overlay.pending=clone(s.pending);overlay.look=clone(s.look);}
  const componentInitial=clone(s),callbacks=[],calls=[];
  const opts={view,level:'normal',onInput:i=>callbacks.push(clone(i))};
  for(const now of spec.calls){g.tick=now;opts.view=ai.observe(g,0);const callView=opts.view,callViewSnapshot={tick:callView.tick,spawn:clone(callView.players[0].spawn),units:clone([...callView.units.values()]),visible:[...callView.players[0].visible],sightings:clone(callView.sightings??[])};const canonicalDigBefore=clone(g.units.get(1).dig??null);const beforePrivate=privateTape.length,beforeInputs=callbacks.length,beforeRng=globalDraws;
   if(spec.resume&&calls.length===0)human.__startLook(g,0,s,opts);
   const afterStartLook=spec.resume&&calls.length===0?clone(s):null;
   human.__runInput(g,0,s,opts);
   const plannedInputCommands=spec.resume&&calls.length===0?s.inputs.filter(i=>i.cmd&&i.origin===now).map(i=>clone(i.cmd)):[];
   calls.push({now,plannedInputCommands,componentAlert:componentAlert?clone(componentAlert):null,viewSnapshot:callViewSnapshot,afterStartLook,canonicalDigBefore,canonicalDigAfter:clone(g.units.get(1).dig??null),privateDraws:privateTape.length-beforePrivate,privateTape:privateTape.slice(beforePrivate),globalDraws:globalDraws-beforeRng,globalTape:globalTape.slice(beforeRng).map(bits),callbacks:clone(callbacks.slice(beforeInputs)),state:clone(s),wallet:g.players[0].mp,canonicalUnits:units(g)});
   if(spec.resume&&s.inputs.length&&spec.calls.length<10)spec.calls.push(Math.max(g.tick+1,s.nextInput));
  }
  cases.push({name:spec.name,map,options,slot:0,level:'normal',matchSeed:stateSeed,initialPrivateDraws:2,setupCommands,viewSnapshot,stateOfResult,componentOverlay:overlay,componentInitial,calls,globalDraws:globalDraws-beforeGlobal});
 }
 clean();const result={reference:pin,node:process.version,arch:process.arch,sourceSha256,scope:'Explicit constructed component state/history/queue/urgent context on real source Game. Original stateOf/startLook/planNow/runInput/inputsFor execute unchanged. Paid Dig and executed commands use canonical source admission. Per-call actual source views and command-sensitive units are Source actuator receipts. Private motor replay is detached and does not claim native policy or actuators. Not natural room timing or full-step evidence.',globalRandom:{value:.5},cases};
 const fixturePath=process.env.HUMAN_SEAMS_FIXTURE??'/private/tmp/human-state-seams-source-oracle.json',output=JSON.stringify(result)+'\n';
 if(process.argv.includes('--check')){if(readFileSync(fixturePath,'utf8')!==output)throw Error('Human component seams fixture differs');}else writeFileSync(fixturePath,output);
 for(const c of cases)console.log(JSON.stringify({name:c.name,calls:c.calls.map(r=>({now:r.now,privateDraws:r.privateDraws,globalDraws:r.globalDraws,callbacks:r.callbacks,remaining:r.state.inputs,wallet:r.wallet}))}));
}finally{Math.random=previous;}
