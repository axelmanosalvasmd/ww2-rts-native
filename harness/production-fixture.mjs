// Original-source production/research traces; no full simulation or spawn placement.
// Extracted function/command bodies stay unchanged. The dispatcher selects the same
// command branch; produceUnit records a completion and tally has no side effects.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node 24.21.0');
const root=fileURLToPath(new URL('../',import.meta.url));
const ref=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`),pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',ref,...a],{encoding:'utf8'}).trim();
if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Reference must be clean at pin');
const sim=await import(pathToFileURL(`${ref}/shared/sim.js`)),tech=await import(pathToFileURL(`${ref}/shared/tech.js`));
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
  if(a.op==='buy')result=fn.command(g,a.slot,{t:'buy',unit:a.kind,...(a.from!==undefined?{from:a.from}:{})})??null;
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
run('stored-charges-and-head-progress',[
 {op:'paid',id:1,kind:'rifle',charge:{mp:123.5,fuel:7.25}},{op:'paid',id:1,kind:'conscript',charge:{mp:41.75,fuel:2.5}},
 {op:'step',dt:5},{op:'cancel',slot:0,id:1,job:1},{op:'cancel',slot:0,id:1,job:2},{op:'step',dt:11},{op:'step',dt:100}
]);
run('twenty-hertz-progress',[{op:'paid',id:1,kind:'rifle',charge:{mp:100,fuel:0}},{op:'paid',id:1,kind:'rifle',charge:{mp:100,fuel:0}},{op:'tech',slot:0,kind:'inf'},{op:'step',dt:.05,repeat:299},{op:'step',dt:.05},{op:'player',slot:0,fields:{out:true}},{op:'step',dt:.05,repeat:300},{op:'player',slot:0,fields:{out:false}},{op:'step',dt:.05,repeat:301}]);
run('queue-reconciliation',[
 {op:'queue',id:1,types:['rifle','mg']},{op:'paid',id:1,kind:'conscript',charge:{mp:90,fuel:4}},
 {op:'queue',id:1,types:['rifle','sniper','conscript']},{op:'cancel',slot:0,id:1,job:4},{op:'queue',id:1,types:['conscript']},{op:'step',dt:20}
]);
run('production-admission',[
 {op:'buy',slot:0,kind:'tank'}, {op:'config',fields:{tech:false}},{op:'player',slot:0,fields:{fuel:0}}, {op:'buy',slot:0,kind:'tank'},
 {op:'player',slot:0,fields:{fuel:400,mp:0}}, {op:'buy',slot:0,kind:'rifle'},{op:'player',slot:0,fields:{mp:4000}},
 {op:'buy',slot:0,kind:'tiger'},{op:'buy',slot:0,kind:'worldbase'}, {op:'buy',slot:0,kind:'rifle',from:2},
 ...Array.from({length:6},()=>({op:'buy',slot:0,kind:'rifle',from:1})),{op:'step',dt:100},{op:'step',dt:100},
 {op:'unit',id:3,fields:{hp:0}},{op:'buy',slot:0,kind:'tank',from:3},{op:'step',dt:100},{op:'unit',id:3,fields:{hp:100}},{op:'step',dt:100},
 {op:'buy',slot:0,kind:'lcvp'},{op:'config',fields:{mode:{kind:'classic',suddenDeath:true}}},{op:'buy',slot:0,kind:'rifle'}, {op:'cancel',slot:0,id:1,job:2},{op:'clear'},{op:'step',dt:100}
]);
run('population-and-unit-max', [{op:'buy',slot:0,kind:'rifle'}],i=>{i.units.push(...Array.from({length:24},(_,n)=>unit(n+20,0,'rifle')));});
run('queued-unit-limit', [{op:'config',fields:{tech:false}},{op:'unit',id:8,fields:{owner:1}},{op:'buy',slot:1,kind:'tiger'},{op:'buy',slot:1,kind:'tiger'}]);
run('pending-drop-population', [{op:'buy',slot:0,kind:'rifle'}],i=>{i.strikes=Array.from({length:8},()=>({owner:0,kind:'para',live:false}));});
run('research-pause-parallel-completion',[
 {op:'config',fields:{tech:false}},{op:'tech',slot:0,kind:'inf'},{op:'config',fields:{tech:true}},
 {op:'tech',slot:0,kind:'guns'},{op:'tech',slot:0,kind:'inf'},{op:'tech',slot:0,kind:'tier'},{op:'tech',slot:0,kind:'tier'},
 {op:'unit',id:4,fields:{hp:0}},{op:'unit',id:1,fields:{built:.5}},{op:'step',dt:40},
 {op:'unit',id:1,fields:{built:1}},{op:'unit',id:4,fields:{hp:100}},{op:'unit',id:5,fields:{built:1}},
 {op:'tech',slot:0,kind:'inf'},{op:'tech',slot:0,kind:'armor'},{op:'step',dt:30},{op:'step',dt:30},
 {op:'tech',slot:0,kind:'inf'},{op:'tech',slot:0,kind:'armor'},{op:'tech',slot:0,kind:'guns'},
 {op:'unit',id:4,fields:{hp:0}},{op:'player',slot:0,fields:{away:true}},{op:'step',dt:45},
 {op:'player',slot:0,fields:{out:true}},{op:'step',dt:100},{op:'player',slot:0,fields:{out:false}},
 {op:'unit',id:4,fields:{hp:100}},{op:'step',dt:45}, {op:'tech',slot:0,kind:'inf'},
 {op:'tech',slot:0,kind:'tier'},{op:'config',fields:{mode:{kind:'classic',suddenDeath:true}}},{op:'tech',slot:0,kind:'tier'},{op:'step',dt:45},{op:'step',dt:50}
]);
run('no-finished-labs', [{op:'step',dt:0},{op:'unit',id:1,fields:{hp:0}},{op:'unit',id:4,fields:{hp:0}},{op:'tech',slot:0,kind:'tier'},{op:'tech',slot:0,kind:'armor'},{op:'unit',id:1,fields:{hp:100,built:.5}},{op:'tech',slot:0,kind:'tier'}]);
run('horde-shared-labs', [{op:'unit',id:1,fields:{hp:0}},{op:'unit',id:4,fields:{hp:0}}, {op:'tech',slot:0,kind:'tier'},{op:'tech',slot:0,kind:'inf'},{op:'step',dt:60},{op:'tech',slot:2,kind:'tier'}],i=>{i.mode={kind:'horde',slot:2};});
run('world-research-currency', [{op:'player',slot:0,fields:{mun:0}},{op:'tech',slot:0,kind:'tier'},{op:'player',slot:0,fields:{mun:400}},{op:'tech',slot:0,kind:'tier'},{op:'buy',slot:0,kind:'rifle'},{op:'step',dt:60}],i=>{i.mode={kind:'world'};});
run('ordinary-mode-lab-currency', [{op:'player',slot:0,fields:{mun:0}},{op:'tech',slot:0,kind:'tier'},{op:'tech',slot:0,kind:'armor'},{op:'step',dt:60}],i=>{i.mode=null;});
const data=JSON.stringify({reference:pin,scenarios})+'\n',file=`${root}/rust/crates/core/tests/fixtures/production.json`;
if(process.argv.includes('--check')){if(readFileSync(file,'utf8')!==data)throw Error('Production fixture differs');}else writeFileSync(file,data);
const actions=scenarios.reduce((n,s)=>n+s.trace.length,0),advances=scenarios.reduce((n,s)=>n+s.trace.filter(r=>r.action.op==='step').reduce((m,r)=>m+(r.action.repeat??1),0),0);
console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} ${scenarios.length} traces, ${actions} actions and ${advances} advance calls`);
