// Test-only call-boundary instrumentation. The pinned checkout is read-only.
import {readFileSync, openSync, writeSync, closeSync, writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve, dirname} from 'node:path';
import {SourceTextModule, SyntheticModule} from 'node:vm';
import {isDeepStrictEqual} from 'node:util';

if (process.arch !== 'x64' || process.version !== 'v24.21.0') throw Error('Use x64 Node v24.21.0');
const args = process.argv.slice(2);
const option = name => { const i=args.indexOf(name); if (i<0 || !args[i+1]) throw Error(`Required ${name}`); return args[i+1]; };
const reference = resolve(option('--reference')), output = resolve(option('--output'));
const inputPath = resolve(option('--input'));
const originalInput=JSON.parse(readFileSync(inputPath,'utf8'));
const sharedInput=originalInput.sharedInput ?? originalInput;
if(sharedInput.layout?.join(',')!=='0,1' || sharedInput.tape?.length!==16384)throw Error('original R1 finite tape/layout required');
const seedText=sharedInput.seed;
if(typeof seedText!=='string'||!/^\d+$/.test(seedText)||BigInt(seedText)>18446744073709551615n)throw Error('recorded u64 seed string required');
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=(...a)=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
const clean=()=>{if(git('rev-parse','HEAD')!==pin||git('status','--porcelain','--untracked-files=no'))throw Error('clean pinned checkout required');};
clean();
const sourceHead=execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim();
if(execFileSync('git',['status','--porcelain'],{encoding:'utf8'}).trim())throw Error('freeze complete source head before recording');
const buffer=new DataView(new ArrayBuffer(8));
const bits=value=>{buffer.setFloat64(0,value);return buffer.getBigUint64(0).toString(16).padStart(16,'0');};
const number=value=>({bits:bits(value),value:Number.isFinite(value)?value:String(value)});
const resource=value=>value===undefined?{absent:true}:number(value);
const values=sharedInput.tape.map(v=>{if(!/^[0-9a-f]{16}$/.test(v))throw Error('bad tape bits');buffer.setBigUint64(0,BigInt('0x'+v));const n=buffer.getFloat64(0);if(!Number.isFinite(n)||n<0||n>=1)throw Error('bad tape fraction');return n;});
let consumed=[], branches=[], motion=[], retirements=[];
let layoutReceipt=null;
const hooks={
  draw(consumer){const index=consumed.length;if(index>=values.length)throw Error('shared random tape exhausted; no fallback');const value=values[index];consumed.push({index,consumer,bits:bits(value)});return value;},
  branch(g,u,event){branches.push({tick:g.tick,id:u.id,event,targetId:u.targetId,x:number(u.x),z:number(u.z),hp:number(u.hp),goal:point(u.amove),path:u.path.map(point)});},
  motion(g,u,moving){motion.push({id:u.id,moving});},
  retire(g,u){retirements.push({id:u.id,owner:u.owner,kind:u.type,x:number(u.x),z:number(u.z),hp:number(u.hp),lastHit:scalar(u.lastHit)});},
  env(consumer, original){original();return this.draw(consumer);},
  layout(selected){layoutReceipt={input:[...sharedInput.layout],selectedBeforeInjection:selected,boundary:'before player spawns, placement and actor allocation'};return [...sharedInput.layout];},
};
// A constructor/module exception can precede the first normal record. Preserve
// its real consumed prefix separately, without inventing a startup or completion.
let observedStartup=null;
process.once('uncaughtExceptionMonitor',error=>{
  const phase=args.includes('--check')?'check':'generate';
  const row={stage:'uncaughtSourceFailure',sourceHead,reference:pin,
    error:String(error),stack:error.stack,partialInputReceipt:{capacity:values.length,
      consumed:consumed.length,unconsumed:values.length-consumed.length,
      exhausted:consumed.length===values.length,draws:consumed,layout:layoutReceipt,startup:observedStartup}};
  try{writeFileSync(`${output}.${phase}.failure.json`,JSON.stringify(row)+'\n',{flag:'wx',mode:0o600});}
  catch(artifactError){writeSync(2,JSON.stringify({stage:'failureArtifactError',error:String(artifactError)})+'\n');}
});
const originalSource=readFileSync(resolve(reference,'shared/sim.js'),'utf8');
const edits=[];
let transformed=originalSource;
const replace=(needle, replacement)=>{
  if(transformed.split(needle).length!==2)throw Error(`instrumentation anchor not unique: ${needle}`);
  transformed=transformed.replace(needle,replacement);edits.push([needle,replacement]);
};
// Wrap draw expressions, preserving all original engine statements and formulas.
replace('Math.floor(Math.random() * good.length)',`Math.floor(__aligned.draw(['startup.layout.shuffle']) * good.length)`);
replace('g.seed = Math.floor(Math.random() * 2 ** 32)',`g.seed = Math.floor(__aligned.draw(['startup.environment.seed32']) * 2 ** 32)`);
replace('a: rng(g) * Math.PI * 2',`a: __aligned.env(['startup.wind.angle'], () => rng(g)) * Math.PI * 2`);
replace('v: 0.3 + rng(g) * 0.5',`v: 0.3 + __aligned.env(['startup.wind.strength'], () => rng(g)) * 0.5`);
replace('CFG.weather.firstRain + rng(g) * CFG.weather.dry[0]',`CFG.weather.firstRain + __aligned.env(['startup.shower'], () => rng(g)) * CFG.weather.dry[0]`);
replace('const accurate = Math.random() < acc',`const accurate = __aligned.draw(['combat.accuracy',g.tick,u.id,t.id,i]) < acc`);
replace('0.2 + Math.random() * 0.7',`0.2 + __aligned.draw(['combat.offset.magnitude',g.tick,u.id,t.id,i]) * 0.7`);
replace('(Math.random() < 0.5 ? -1 : 1)',`(__aligned.draw(['combat.offset.sign',g.tick,u.id,t.id,i]) < 0.5 ? -1 : 1)`);
replace('(rng(g) - 0.5) * 0.6 * dt',`(__aligned.env(['weather.wind.angle',g.tick], () => rng(g)) - 0.5) * 0.6 * dt`);
replace('(rng(g) - 0.5) * 0.3 * dt',`(__aligned.env(['weather.wind.strength',g.tick], () => rng(g)) - 0.5) * 0.3 * dt`);
replace('wx.next = lo + rng(g) * (hi - lo)',`wx.next = lo + __aligned.env(['weather.shower',g.tick], () => rng(g)) * (hi - lo)`);
replace('rng(g) < CFG.fire.ignite[ch]',`__aligned.env(['environment.ignite',g.tick,c], () => rng(g)) < CFG.fire.ignite[ch]`);
replace('rng(g) < CFG.fire.spread[physicalWorldCell(g, n)]',`__aligned.env(['environment.spread',g.tick,c,n], () => rng(g)) < CFG.fire.spread[physicalWorldCell(g, n)]`);
replace('rng(g) >= CFG.rubbleSpill',`__aligned.env(['environment.rubble',g.tick,c,n], () => rng(g)) >= CFG.rubbleSpill`);
// Observe the executed attack-move branch without changing its predicate or action.
replace('if (dist(u, u.amove) < 2.5) u.amove = null;', `if (dist(u, u.amove) < 2.5) { __aligned.branch(g,u,'arrival'); u.amove = null; }`);
replace('else if (t && canShoot(g, u, t)) u.path = [];', `else if (t && canShoot(g, u, t)) { __aligned.branch(g,u,'halt'); u.path = []; }`);
replace("else if (!u.path.length && u.repath <= 0) requestStepPath(g, u, u.amove, 'amove');", `else if (!u.path.length && u.repath <= 0) { __aligned.branch(g,u,'resumeRequest'); requestStepPath(g, u, u.amove, 'amove'); }`);
replace('const moving = u.path.length > 0 || moved > 0.001;', 'const moving = u.path.length > 0 || moved > 0.001; __aligned.motion(g,u,moving);');
replace('    g.units.delete(u.id); if (UNITS[u.type].building) wreckBuilding(g, u);\n    // A knocked-out vehicle stays', '    __aligned.retire(g,u); g.units.delete(u.id); if (UNITS[u.type].building) wreckBuilding(g, u);\n    // A knocked-out vehicle stays');
let restored=transformed;
for(const [before,after] of [...edits].reverse())restored=restored.replace(after,before);
if(restored!==originalSource)throw Error('instrumentation does not erase to the exact pinned source');
// Any other actual consumer refuses rather than silently using an unseeded draw.
const prefix="import { hooks as __aligned } from 'aligned:test';\n";
const suffix="\nconst __alignedOriginalSpawnSlots = spawnSlots; spawnSlots = (...args) => __aligned.layout(__alignedOriginalSpawnSlots(...args));\nexport const __alignedCanShoot = (g,u,t) => canShoot(g,u,t);\n";
// Cover module initialization too, including any captured default RNG callback.
const originalRandom=Math.random;
Math.random=()=>{throw Error('unmapped Math.random consumer; no fallback');};
const modules=new Map();
const hookModule=new SyntheticModule(['hooks'],function(){this.setExport('hooks',hooks);},{identifier:'aligned:test'});
const load=path=>{
  if(modules.has(path))return modules.get(path);
  const raw=readFileSync(path,'utf8');
  const source=path===resolve(reference,'shared/sim.js')?prefix+transformed+suffix:raw;
  const module=new SourceTextModule(source,{identifier:path});modules.set(path,module);return module;
};
const module=load(resolve(reference,'shared/sim.js'));
await module.link((specifier,parent)=>specifier==='aligned:test'?hookModule:load(resolve(dirname(parent.identifier),specifier)));
await module.evaluate();
const sim=module.namespace;
const scene=JSON.parse(readFileSync(new URL('./scenarios/aligned-stop.json',import.meta.url)));
const caseName=option('--case');
if(!['retreat','scuttle','scuttle-unloaded','retreat-build','retreat-project','retreat-reservations','scuttle-work'].includes(caseName))throw Error('unknown genuine ground case');
const scalar=v=>v===undefined?{absent:true}:typeof v==='number'?number(v):v;
const point=v=>v===undefined?{absent:true}:v===null?null:{x:number(v.x),z:number(v.z)};
const encode=v=>v===undefined?{absent:true}:typeof v==='number'?number(v):Array.isArray(v)?v.map(encode):v!==null&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;
const g=sim.createGame(structuredClone(scene.map),['A','B'],true,[0,1],[0,1],{
  mode:'conquest',army:'endless',weather:'clear',tech:false,supply:true,logistics:false,sandbox:false,mapKey:'aligned-ground-commands'});
const startup={spawns:g.players.map(p=>[bits(p.spawn.x),bits(p.spawn.z)]),environmentSeed32:g.seed>>>0,weatherSeed:1,wind:[bits(g.wind.a),bits(g.wind.v)],shower:g.wx?{raining:g.wx.raining,next:bits(g.wx.next)}:null,rain:bits(g.wx.rain),wet:bits(g.wx.wet),weather:g.weather,nextId:g.nextId};
observedStartup=startup;
const snapshot=()=>({tick:g.tick,nextId:g.nextId,units:[...g.units.values()].map(u=>({id:u.id,owner:u.owner,kind:u.type,
  x:number(u.x),z:number(u.z),hp:number(u.hp),supp:number(u.supp),speed:number(u.moveSpeed),path:u.path.map(point),goal:point(u.worldGoal),amove:point(u.amove),
  retreating:u.retreating,riding:scalar(u.riding),cargo:encode(u.cargo),attackId:u.attackId,targetId:u.targetId,orders:encode(u.orders),
  build:scalar(u.build),dig:encode(u.dig),entrench:encode(u.entrench),garrison:scalar(u.garrison),lastHit:scalar(u.lastHit),built:scalar(u.built)})),
  wallets:g.players.map(p=>number(p.mp)),wrecks:g.wrecks.map(w=>({id:w.id,x:number(w.x),z:number(w.z),cell:scalar(w.c),moving:encode(w.motion)})),drawCount:consumed.length});
const checking=args.includes('--check');
const expected=checking?readFileSync(output,'utf8').trimEnd().split('\n'):null;
let lineIndex=0;
const fd=checking?null:openSync(output,'wx',0o600);
const record=evidence=>{
  let row;
  try{row={evidence,state:snapshot()};}catch(error){row={evidence,stateProviderError:String(error)};}
  if(checking){if(!isDeepStrictEqual(row,JSON.parse(expected[lineIndex]??'null')))throw Error(`authentic Source recheck differs at row ${lineIndex}`);}
  else writeSync(fd,JSON.stringify(row)+'\n');
  lineIndex++;
};
const require=(ok,label)=>{if(!ok){record({stage:'predicateFailure',label});throw Error(label);}};
const command=(owner,raw,expected=null)=>{
  const before=snapshot();let result;
  try{result=sim.command(g,owner,raw)??null;}catch(error){record({stage:'commandException',owner,command:raw,commandBits:encode(raw),before,error:String(error),stack:error.stack});throw error;}
  record({stage:'command',owner,command:raw,commandBits:encode(raw),before,result});require(result===expected,'command disposition '+JSON.stringify(raw));
};
const unchangedCommand=(owner,raw,denial)=>{const before=snapshot();command(owner,raw,denial);require(isDeepStrictEqual(before,snapshot()),'denied command changed observed state');};
const step=()=>{const before=new Set(g.units.keys()),beforeTick=g.tick,beforeDrawCount=consumed.length;try{sim.step(g);}catch(error){record({stage:'tickException',beforeTick,beforeDrawCount,error:String(error),stack:error.stack});throw error;}record({stage:'tick',removed:[...before].filter(id=>!g.units.has(id))});};
const until=(limit,label,reached)=>{for(let i=0;i<limit;i++){if(reached()){record({stage:'milestone',label});return;}step();}require(false,'missing milestone '+label);};
const moveTo=(owner,id,at)=>{command(owner,{t:'move',orders:[[id,at.x,at.z]]});until(1000,'ordinary-move-arrival',()=>{const u=g.units.get(id);return !u.path.length&&Math.hypot(u.x-at.x,u.z-at.z)<1;});};
let productionWitness=null;
const tank=owner=>{
  const builders=owner===0?[1,2]:[4,5],at=owner===0?{x:29,z:9}:{x:93,z:53},site=g.nextId;
  const buildTick=g.tick,buildMP=g.players[owner].mp,beforeIDs=new Set(g.units.keys());
  command(owner,{t:'build',ids:builders,kind:'motorpool',...at});
  const siteRow=g.units.get(site);
  require(g.tick===buildTick&&g.players[owner].mp===buildMP-sim.UNITS.motorpool.cost,'immediate paid building charge');
  require(isDeepStrictEqual([...g.units.keys()].filter(id=>!beforeIDs.has(id)),[site])&&g.nextId===site+1,'exactly one allocated building');
  require(siteRow?.type==='motorpool'&&siteRow.built===0&&siteRow.hp===sim.UNITS.motorpool.hpPer*0.25,'genuine quarter-health paid site');
  require(builders.every(id=>g.units.get(id).build===site),'actual construction assigned to both builders');
  until(2400,'paid-motorpool-completed',()=>g.units.get(site).built===1);
  const completedTick=g.tick,id=g.nextId,buyMP=g.players[owner].mp,preBuyIDs=new Set(g.units.keys());
  const queues=encode([...g.units.values()].map(u=>({id:u.id,queue:u.queue,productionJobs:u.productionJobs}))),nextJob=encode(g.nextProductionJob);
  command(owner,{t:'buy',unit:'tank',from:site});
  require(g.tick===completedTick&&g.players[owner].mp===buyMP-sim.UNITS.tank.cost,'immediate paid tank charge');
  require(isDeepStrictEqual([...g.units.keys()].filter(id=>!preBuyIDs.has(id)),[id])&&g.nextId===id+1&&g.units.get(id)?.type==='tank','exactly one immediate tank allocation');
  require(isDeepStrictEqual(queues,encode([...g.units.values()].filter(u=>preBuyIDs.has(u.id)).map(u=>({id:u.id,queue:u.queue,productionJobs:u.productionJobs}))))&&isDeepStrictEqual(nextJob,encode(g.nextProductionJob)),'Conquest purchase adds no queued job');
  productionWitness={owner,site,tank:id,buildTick,completedTick,purchaseTick:g.tick,buildCharge:number(sim.UNITS.motorpool.cost),tankCharge:number(sim.UNITS.tank.cost),initialSiteHP:number(sim.UNITS.motorpool.hpPer*0.25),immediatePurchase:true};
  record({stage:'productionWitness',facts:productionWitness});
  command(owner,{t:'stance',ids:[id],key:'holdFire',on:true});command(owner,{t:'autocast',ids:[id],on:false});command(owner,{t:'stop',ids:[id]});
  return [id,site];
};
let failure=null;
try{
  record({stage:'startup',sourceHead,reference:pin,case:caseName,map:scene.map,profile:{mode:'conquest',army:'endless',tech:false,supply:true,logistics:false},instrumentation:{erasesExactly:true,anchors:edits},sharedInput});
  for(let owner=0;owner<2;owner++){
    const ids=[...g.units.values()].filter(u=>u.owner===owner&&!sim.UNITS[u.type].structure).map(u=>u.id);
    for(const [key,on] of [['holdFire',true],['holdPos',true],['autoRetreat',false]])command(owner,{t:'stance',ids,key,on});
    command(owner,{t:'autocast',ids,on:false});
  }
  if(caseName==='scuttle-work'){
    const site=g.nextId,siteCharge=sim.UNITS.motorpool.cost,initialMP=g.players[0].mp;
    command(0,{t:'build',ids:[1,2],kind:'motorpool',x:29,z:9});
    require(g.players[0].mp===initialMP-siteCharge,'actual scuttle site charge');
    require(typeof g.story?.[0]?.mpSpent==='number'&&g.story[0].mpSpent===siteCharge,'actual first paid scuttle site story');
    require(g.units.get(site)?.type==='motorpool'&&g.units.get(site).built===0,'genuine newly allocated scuttle site');
    until(2400,'real-partial-scuttle-site',()=>g.units.get(site).built>0);
    require(g.units.get(site).built<1,'real unfinished scuttle site');
    const beforeProjectSpent=g.story[0].mpSpent;
    command(0,{t:'entrench',ids:[2],pattern:'line',fort:'trench',x:41,z:9,x2:53,z2:9});
    const project=g.units.get(2).entrench;
    require(!!project,'real project before paid segment');
    until(2400,'real-paid-scuttle-segment',()=>g.units.get(2).dig?.project===project);
    const projectCharge=g.story[0].mpSpent-beforeProjectSpent;
    require(Number.isFinite(projectCharge)&&projectCharge>0,'actual paid segment records real expenditure');
    require(g.units.get(site).built<1&&g.units.get(1).build===site,'unfinished site retains actual first builder');
    require(sim.UNITS[g.units.get(site).type].produces===true,'actual selected site is a producing building');
    const producerType=g.units.get(site).type,producerBuilt=g.units.get(site).built;
    const ids=[...g.units.keys()],siteAt={x:g.units.get(site).x,z:g.units.get(site).z};
    const work=encode([1,2].map(id=>{const u=g.units.get(id);return {id,build:u.build,dig:u.dig,entrench:u.entrench,orders:u.orders};}));
    const wallets=g.players.map(p=>p.mp),story=encode(g.story),foreign=encode(g.units.get(4));
    command(0,{t:'destroy',ids:[1,1,2,4],id:site});
    require(isDeepStrictEqual(encode([1,2].map(id=>{const u=g.units.get(id);return {id,build:u.build,dig:u.dig,entrench:u.entrench,orders:u.orders};})),work),'Destroy defers work teardown');
    require(isDeepStrictEqual(wallets,g.players.map(p=>p.mp))&&isDeepStrictEqual(story,encode(g.story)),'dispatch changes no wallet/story');
    require(g.units.get(site).type===producerType&&g.units.get(site).built===producerBuilt,'Destroy retains the actual producer until retirement');
    require([1,2,site].every(id=>g.units.get(id).hp===0&&g.units.get(id).lastHit===0),'actual scuttle HP and friendly attribution');
    require(isDeepStrictEqual(encode(g.units.get(4)),foreign),'foreign actor preserved');
    unchangedCommand(0,{t:'destroy',ids:[1,2],id:site},'unseen');
    step();
    require(isDeepStrictEqual([...g.units.keys()],ids.filter(id=>![1,2,site].includes(id))),'exact three selected retirements');
    for(let owner=0;owner<g.players.length;owner++){
      for(const key of ['kills','losses','built','captures','mpSpent','supportCalls','planesDowned']){
        const prior=story[owner][key];require(typeof prior?.value==='number'&&Number.isFinite(prior.value),'actual prior story field '+key);
        require(g.story[owner][key]===prior.value+(key==='losses'&&owner===0?3:0),'actual paid-work scuttle story');
      }
      require(g.players[owner].mp===wallets[owner]+g.players[owner].inc*sim.TICK,'ordinary income only');
    }
    require(isDeepStrictEqual(g.fallen.structure,siteAt)&&isDeepStrictEqual(g.fallen.hq,siteAt),'actual fallen paid producer');
    const ticks=20-g.tick%20;
    for(let i=0;i<ticks;i++)step();
    require(!g.projects.has(project.id),'ordinary no-crew project sweep');
    record({stage:'complete',checks:{site,paidProject:project.id,deferredTeardown:true,paidSiteCharge:number(siteCharge),paidProjectCharge:number(projectCharge),storyFieldsChecked:7,retiredActors:[1,2,site],
      ownLossDelta:3,enemyKillDelta:0,siteProducerRemoved:true,projectSwept:true,sweepTicks:ticks,noRefundOrBounty:true}});
  }else if(caseName==='retreat-reservations'){
    moveTo(0,3,{x:43,z:29});
    for(const owner of [0,1]){
      const ids=owner===0?[1,2]:[4,5],x=owner===0?29:93,z=owner===0?9:53;
      command(owner,{t:'entrench',ids,pattern:'line',fort:'trench',x,z,x2:x+12,z2:z});
      until(2400,'guard-case-real-paid-segments',()=>ids.every(id=>!!g.units.get(id).dig?.project));
    }
    for(let control=0;control<2;control++){
      unchangedCommand(0,{t:'retreat',ids:[4]},'needs');
      const foreign=encode(g.units.get(4));
      command(0,{t:'retreat',ids:[3,4],queue:true});
      require(g.units.get(3).type==='mg'&&g.units.get(3).retreating&&g.units.get(3).path.length>0,'actual own constructor MG takes mixed command');
      require(isDeepStrictEqual(encode(g.units.get(4)),foreign),'foreign real paid worker unchanged');
    }
    // Native-only typed reservation faults are separate controls, not Source states.
    record({stage:'complete',checks:{foreignOnlyControls:2,mixedAcceptedControls:2,selectedActor:3,
      foreignActor:4,realPaidActors:[1,2,4,5],nativeTypedFaultsComparedToSource:false}});
  }else if(caseName==='retreat-build'||caseName==='retreat-project'){
    const cancel=ids=>{
      const wallets=encode(g.players.map(p=>({mp:p.mp,mun:p.mun,fuel:p.fuel,vp:p.vp}))),story=encode(g.story),tick=g.tick;
      command(0,{t:'retreat',ids,queue:true});
      require(isDeepStrictEqual(encode(g.players.map(p=>({mp:p.mp,mun:p.mun,fuel:p.fuel,vp:p.vp}))),wallets),'Retreat does not refund paid work');
      require(isDeepStrictEqual(encode(g.story),story)&&g.tick===tick,'cancellation is not completion or expenditure');
      for(const id of ids){const u=g.units.get(id);if(u.owner!==0)continue;require(u.retreating&&u.path.length>0&&!u.build&&!u.dig&&!u.entrench&&!u.orders.length&&u.attackId===0,'real paid activity cancelled through Retreat');}
    };
    if(caseName==='retreat-build'){
      const site=g.nextId,charge=sim.UNITS.motorpool.cost,mp=g.players[0].mp;
      command(0,{t:'build',ids:[1,2],kind:'motorpool',x:29,z:9});
      require(g.players[0].mp===mp-charge,'actual paid site charge');
      require(g.story[0].mpSpent===charge,'actual first paid site story expenditure');
      require([1,2].every(id=>g.units.get(id).build===site),'both actual builders assigned');
      until(2400,'genuine-partial-site',()=>g.units.get(site).built>0);
      require(g.units.get(site).built<1,'unfinished actual site');
      unchangedCommand(0,{t:'retreat',ids:[4]},'needs');
      const other=encode(g.units.get(2)),siteBefore=encode(g.units.get(site));
      cancel([1,1,4]);
      require(isDeepStrictEqual(encode(g.units.get(2)),other),'unselected builder unchanged');
      require(isDeepStrictEqual(encode(g.units.get(site)),siteBefore),'paid site retained unchanged');
      const progress=g.units.get(site).built;
      until(2400,'uncancelled-worker-progress',()=>g.units.get(site).built>progress);
      require(g.units.get(site).built<1,'positive unselected builder progress');
      const otherProgress=g.units.get(site).built;
      cancel([2]);
      for(let i=0;i<20;i++){const ids=[...g.units.keys()];step();require(isDeepStrictEqual([...g.units.keys()],ids),'no invented retirement');require(g.units.get(site).built===otherProgress,'cancelled builders absent from ordinary crew');}
      record({stage:'complete',checks:{site,paidCharge:number(charge),unselectedProgress:true,siteRetained:true,noRefundOrStoryEvent:true,noWorkerProgressTicks:20}});
    }else{
      command(0,{t:'entrench',ids:[1,2],pattern:'line',fort:'trench',x:29,z:9,x2:41,z2:9});
      const project=g.units.get(1).entrench;
      require(!!project&&g.units.get(2).entrench===project,'shared project from real order');
      until(2400,'both-real-paid-segments',()=>[1,2].every(id=>g.units.get(id).dig?.project===project));
      require(project.active===2,'two actual paid segments');
      unchangedCommand(0,{t:'retreat',ids:[4]},'needs');
      const other=encode(g.units.get(2).dig);
      cancel([1,1,4]);
      require(isDeepStrictEqual(encode(g.units.get(2).dig),other)&&g.units.get(2).entrench===project,'unselected paid segment preserved');
      require(project.active===2&&project.crew===2,'legitimate cached active/crew remain until ordinary recount');
      step();require(project.active===1,'next early recount releases cancelled paid reservation');
      cancel([2]);
      const ticks=20-g.tick%20;
      for(let i=0;i<ticks;i++)step();
      require(!g.projects.has(project.id)&&[1,2].every(id=>!g.units.get(id).dig&&!g.units.get(id).entrench),'real no-crew sweep removes listed project');
      record({stage:'complete',checks:{project:project.id,paidActors:[1,2],unselectedPaidJobPreserved:true,cachedActiveAfterCancel:2,cachedCrewAfterCancel:2,nextTickActive:1,sweepTicks:ticks,projectSwept:true,noRefundOrStoryEvent:true}});
    }
  }else if(caseName==='retreat'){
    const initialRoster=[...g.units.values()].map(u=>({id:u.id,owner:u.owner,kind:u.type,hp:number(u.hp)}));
    require(initialRoster.length===10,'fixed constructor has two rifles, one MG, HQ and Barracks per side');
    for(const owner of [0,1])for(const [kind,count] of [['rifle',2],['mg',1],['hq',1],['barracks',1]])require(initialRoster.filter(u=>u.owner===owner&&u.kind===kind).length===count,'complete real initial roster '+owner+'/'+kind);
    record({stage:'retreatPredicate',initialRoster});
    const [id]=tank(1);moveTo(1,id,{x:57,z:29});moveTo(0,1,{x:45,z:29});moveTo(0,2,{x:43,z:33});
    command(0,{t:'stance',ids:[1,2],key:'holdFire',on:false});command(0,{t:'amove',orders:[[1,53,29]]});
    command(0,{t:'move',orders:[[1,49,33]],queue:true});command(0,{t:'attack',ids:[2],target:id});
    require(g.units.get(1).amove!==null&&g.units.get(1).orders.length>0&&g.units.get(2).attackId===id,'positive intents before retreat');
    const liveRoster=[...g.units.keys()];require(liveRoster.length===12,'ten constructor actors plus the paid Motor Pool and tank');
    const beforeBatch=new Map([...g.units.values()].map(u=>[u.id,encode(u)]));
    const foreign=encode(g.units.get(id));unchangedCommand(0,{t:'retreat',ids:[id]},'needs');unchangedCommand(0,{t:'retreat',ids:[]},'needs');
    const hp=g.units.get(1).hp;require(hp===100,'actual full-health actor');
    command(0,{t:'retreat',ids:[1,2,id],queue:true});require(isDeepStrictEqual(encode(g.units.get(id)),foreign),'foreign actor preserved');
    for(const [actor,before] of beforeBatch)if(actor!==1&&actor!==2)require(isDeepStrictEqual(before,encode(g.units.get(actor))),'every unselected real actor remains unchanged');
    const homes=new Map([1,2].map(actor=>{const u=g.units.get(actor),home=u.worldGoal;require(home!==null&&[...g.units.values()].some(b=>b.owner===u.owner&&b.type==='hq'&&b.hp>0&&b.built>=1&&b.x===home.x&&b.z===home.z),'published home is an actual finished own HQ');return [actor,{x:home.x,z:home.z}];}));
    const startDistance=new Map([1,2].map(actor=>{const u=g.units.get(actor),home=homes.get(actor);return [actor,Math.hypot(u.x-home.x,u.z-home.z)];}));
    const previousDistance=new Map(startDistance),progress=new Map([[1,0],[2,0]]);
    const storyRows=()=>{require(Array.isArray(g.story)&&g.story.length===2,'actual story producer initialized by real spending');return g.story.map(row=>Object.fromEntries(['kills','losses','built','captures','mpSpent','supportCalls','planesDowned'].map(key=>{require(typeof row[key]==='number'&&Number.isFinite(row[key]),'real finite story field '+key);return [key,number(row[key])];})));};
    for(const actor of [1,2]){
      const u=g.units.get(actor),home=[...g.units.values()].find(u=>u.owner===0&&u.type==='hq');
      require(u.retreating&&u.path.length>0&&u.amove===null&&!u.orders.length&&u.attackId===0&&u.targetId===0,'immediate group retreat despite queue');
      require(!u.build&&!u.dig&&!u.entrench&&u.garrison<0&&u.worldGoal.x===home.x&&u.worldGoal.z===home.z,'reset and actual home');
    }
    command(1,{t:'attack',ids:[id],target:1});
    let hit=false,fast=new Set(),retreatTicks=0,originalSixTicks=null;
    for(let i=0;i<600;i++){
      const active=[1,2].filter(id=>g.units.get(id).retreating);if(!active.length)break;
      const prior=new Map(active.map(id=>[id,{x:g.units.get(id).x,z:g.units.get(id).z,cooldown:g.units.get(id).cooldown}]));
      const priorStory=storyRows(),priorMP=g.players.map(p=>p.mp);
      step();retreatTicks++;
      require(isDeepStrictEqual(liveRoster,[...g.units.keys()]),'every actual actor survives the retreat course');
      const currentStory=storyRows();
      const balances=g.players.map((p,owner)=>{
        for(const key of ['kills','losses','built','captures','supportCalls','planesDowned'])require(isDeepStrictEqual(priorStory[owner][key],currentStory[owner][key]),'no invented retreat story event '+key);
        const spent=g.story[owner].mpSpent-priorStory[owner].mpSpent.value;require(spent>=0,'no hidden refund');
        require(p.mp===priorMP[owner]-spent+p.inc*sim.TICK,'ordinary income and actual story expenditure explain wallet');
        return {owner,spent:number(spent),mp:number(p.mp),income:number(p.inc)};
      });
      if(retreatTicks===6){require(g.units.get(1).hp===100-30*0.25&&g.units.get(1).targetId===0,'full original Retreat check at six ticks');originalSixTicks={ticks:6,hp:number(g.units.get(1).hp),targetId:0};}
      for(const actor of [1,2]){const u=g.units.get(actor),home=homes.get(actor),distance=Math.hypot(u.x-home.x,u.z-home.z);if(distance<previousDistance.get(actor))progress.set(actor,progress.get(actor)+1);previousDistance.set(actor,distance);}
      record({stage:'retreatPredicate',relativeTick:retreatTicks,sixTickCheckpoint:originalSixTicks,tail:{tick:g.tick,rosterCount:liveRoster.length,retired:0,story:currentStory,balances}});
      for(const actor of active){const u=g.units.get(actor),p=prior.get(actor);if(u.retreating){require(u.targetId===0&&u.supp===0,'retreat cannot fire or gain suppression');if(Math.hypot(u.x-p.x,u.z-p.z)/sim.TICK>=sim.UNITS[u.type].speed*1.49)fast.add(actor);}}
      if(!hit&&g.units.get(1).hp<hp){require(g.units.get(1).hp===100-30*0.25,'first genuine tank contact gives 7.5 damage');require(g.shots.some(e=>e.k==='contact'&&e.f===id&&e.t===1&&e.fo===1&&e.to===0&&e.hit&&!e.kill),'actual tank contact identifies the damaged retreating rifle');hit=true;command(1,{t:'stop',ids:[id]});}
    }
    require(hit&&fast.has(1)&&fast.has(2)&&retreatTicks>0&&originalSixTicks!==null,'actual retreat hit, original six-tick check and sprint for both own actors');
    for(const id of [1,2]){const u=g.units.get(id);require(!u.retreating&&!u.path.length&&!u.worldGoal,'retreat finishes home');}
    const completion=[1,2].map(actor=>{const u=g.units.get(actor),home=homes.get(actor);require(progress.get(actor)>0&&Math.hypot(u.x-home.x,u.z-home.z)<startDistance.get(actor),'actual progress toward home');require(u.moveOutcome==='completed'&&u.moveResult?.[0]==='completed','actual ordinary completion publication');require(!!u.routeEnd&&Math.hypot(u.x-u.routeEnd.x,u.z-u.routeEnd.z)<=0.25&&Math.abs(u.moveSpeed)<0.1,'actual route-provider endpoint completion');return {id:actor,home:point(home),routeEnd:point(u.routeEnd),x:number(u.x),z:number(u.z),progressTicks:progress.get(actor),outcome:u.moveOutcome,outcomeTick:u.moveOutcomeTick,result:encode(u.moveResult)};});
    record({stage:'retreatPredicate',completion});
    record({stage:'complete',checks:{originalSixTicks,initialRoster,quarterTankHit:hit,sprintActors:[...fast].sort((a,b)=>a-b),retreatTicks,groupImmediateDespiteQueue:true,foreignPreserved:true,production:productionWitness}});
  }else{
    const [id,site]=tank(0);moveTo(0,id,{x:43,z:29});command(0,{t:'board',ids:[1],target:id});
    until(1000,'ordinary-rifle-boarded',()=>!!g.units.get(1).riding);
    require(g.units.get(1).riding===id&&g.units.get(id).cargo.includes(1),'actual passenger and carrier');
    for(const ids of [[1],[4],[999999],[1,4,999999]])unchangedCommand(0,{t:'destroy',ids},'unseen');
    const unload=caseName==='scuttle-unloaded';
    if(unload){command(0,{t:'unload',ids:[id]});require(!g.units.get(1).riding&&!g.units.get(id).cargo.includes(1),'actual ordinary unload before independent scuttle case');}
    const preserved=[1,4].map(id=>encode(g.units.get(id)));
    const unloadedStory=unload?encode(g.story):null,unloadedIDs=unload?[...g.units.keys()]:null;
    const unloadedSite=unload?{x:g.units.get(site).x,z:g.units.get(site).z}:null;
    const wallets=g.players.map(p=>p.mp);
    command(0,{t:'destroy',ids:unload?[id,4]:[id,1,4],id:site});
    for(const actor of [id,site])require(g.units.get(actor).hp===0&&g.units.get(actor).lastHit===0,'own live tank and scalar building marked dead');
    require(g.units.get(1).hp>0&&g.units.get(4).hp>0&&isDeepStrictEqual(preserved,[1,4].map(id=>encode(g.units.get(id))))&&isDeepStrictEqual(wallets,g.players.map(p=>p.mp)),'passenger and foreign preserved without refund');
    for(const ids of [[id],[site],[id,site]])unchangedCommand(0,{t:'destroy',ids},'unseen');step();
    require(!g.units.has(id)&&!g.units.has(site)&&g.wrecks.some(w=>w.id===id),'actual next-tick retirement and wreck');
    for(let owner=0;owner<2;owner++)require(g.players[owner].mp===wallets[owner]+g.players[owner].inc*sim.TICK,'income only, no refund or bounty');
    require(g.units.get(1).hp>0&&!g.units.get(1).riding,'passenger survives carrier scuttle');
    if(unload){
      require(isDeepStrictEqual([...g.units.keys()],unloadedIDs.filter(actor=>actor!==id&&actor!==site)),'only the two selected actors retire');
      for(let owner=0;owner<g.players.length;owner++)for(const key of ['kills','losses','built','captures','mpSpent','supportCalls','planesDowned']){
        const before=unloadedStory[owner][key];require(typeof before?.value==='number'&&Number.isFinite(before.value),'actual prior story field '+key);
        require(g.story[owner][key]===before.value+(owner===0&&key==='losses'?2:0),'actual friendly-scuttle death story '+owner+'/'+key);
      }
      require(isDeepStrictEqual(g.fallen.structure,unloadedSite)&&isDeepStrictEqual(g.fallen.hq,unloadedSite),'actual fallen producing structure records the paid site');
      record({stage:'milestone',label:'unloaded-scuttle-story-tail',ownLossDelta:2,enemyKillDelta:0,onlySelectedRetired:true});
    }
    record({stage:'complete',checks:{carriedForeignDeadFiltered:true,nextTickTankAndBuildingRetired:true,wreck:id,noRefundOrBounty:true,ordinaryUnload:unload,production:productionWitness}});
  }
  record({stage:'inputReceipt',capacity:values.length,consumed:consumed.length,unconsumed:values.length-consumed.length,exhausted:consumed.length===values.length,draws:consumed,layout:layoutReceipt,startup,ownerApproval:'PENDING',rowStatusChanged:false});
}catch(error){failure=String(error);record({stage:'failure',failure,stack:error.stack,partialInputReceipt:{capacity:values.length,consumed:consumed.length,unconsumed:values.length-consumed.length,exhausted:consumed.length===values.length,draws:consumed,layout:layoutReceipt,startup}});}
finally{Math.random=originalRandom;if(fd!==null)closeSync(fd);clean();}
if(checking&&lineIndex!==expected.length)throw Error('authentic Source recheck row count differs');
console.log(JSON.stringify({case:caseName,sourceHead,rows:lineIndex,ticks:g.tick,draws:consumed.length,failure}));
if(failure)process.exitCode=2;
