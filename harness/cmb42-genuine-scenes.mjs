// Separate successor. Ordinary wallets and cooldowns are never assigned.
// Source-only draft. No fixture exists until a separately granted capture.
import assert from 'node:assert/strict';
import {existsSync, readFileSync, statSync, writeFileSync} from 'node:fs';
import {registerHooks} from 'node:module';
import {isAbsolute, resolve} from 'node:path';
import {pathToFileURL} from 'node:url';

const bits=value=>{
 const buffer=new DataView(new ArrayBuffer(8));
 buffer.setFloat64(0,value);
 return buffer.getBigUint64(0).toString(16).padStart(16,'0');
};
// Preserve raw property presence, number bits, container types and order. This
// is a private fixture encoding, not a recipient schema or defaults mapping.
function raw(value){
 if(value===undefined)return {$undefined:true};
 if(typeof value==='number')return {$f64:bits(value)};
 if(value===null||typeof value==='boolean'||typeof value==='string')return value;
 if(value instanceof Map)return {$map:[...value].map(([key,item])=>[raw(key),raw(item)])};
 if(value instanceof Set)return {$set:[...value].map(raw)};
 if(ArrayBuffer.isView(value))return {$typed:value.constructor.name,values:[...value].map(raw)};
 if(Array.isArray(value))return value.map(raw);
 assert.equal(typeof value,'object','receipt must contain only observed data');
 return Object.fromEntries(Object.entries(value).map(([key,item])=>[key,raw(item)]));
}

// One continuous recorded provider per match, including createGame. Its state
// is never replaced during a scene. Native stream alignment remains unproved.
function randomProvider(seed){
 const mask=(1n<<64n)-1n;
 let state=BigInt(seed),context={phase:'constructor',tick:0},consumer;
 const draws=[],environment=[];
 return {
  draws,
  consumer(value){assert.equal(consumer,undefined);consumer=value;},
  environment,
  observeEnvironment(g,before,value,consumer){environment.push({...context,consumer,index:environment.length,tick:g.tick,before,after:g.seed,beforeU32:before>>>0,afterU32:g.seed>>>0,value:bits(value)});},
  context(next){context=next;},
  state:()=>state.toString(16).padStart(16,'0'),
  next(){
   const before=state;
   state=(state+0x9e3779b97f4a7c15n)&mask;
   let value=state;
   value=((value^(value>>30n))*0xbf58476d1ce4e5b9n)&mask;
   value=((value^(value>>27n))*0x94d049bb133111ebn)&mask;
   value^=value>>31n;
   const result=Number(value>>11n)/9007199254740992;
   draws.push({...context,consumer:consumer??['UNMAPPED.global',new Error().stack],index:draws.length,before:before.toString(16).padStart(16,'0'),after:state.toString(16).padStart(16,'0'),value:bits(result)});
   consumer=undefined;
   return result;
  }
 };
}

export async function recordCmb42Genuine({reference,pin,clean}){
 const flags=process.argv.slice(2);
 assert(flags.every(flag=>['--cmb42-genuine-only','--check'].includes(flag)||flag.startsWith('--cmb42-output=')),'new-only mode rejects other scopes');
 const outputs=flags.filter(flag=>flag.startsWith('--cmb42-output='));
 assert.equal(outputs.length,1,'one explicit private receipt directory required');
 const output=outputs[0].slice('--cmb42-output='.length);
 assert(isAbsolute(output),'receipt directory must be absolute');
 assert(statSync(output).isDirectory(),'private receipt directory must already exist');
 const file=resolve(output,'scenes.json'),failureFile=resolve(output,'first-failure.json');
 assert(!existsSync(failureFile),'preserve earlier failure packet; use a new granted output directory');
 if(!flags.includes('--check'))assert(!existsSync(file),'preserve earlier capture; use a new granted output directory');
 const sourceUrl=pathToFileURL(resolve(reference,'shared/sim.js')).href;
 registerHooks({load(url,context,next){
  const result=next(url,context);
  if(url!==sourceUrl)return result;
  const source=String(result.source);
  assert.equal(source.split('function canShoot(g, u, t) {').length,2,'actual observer anchor');
  const rngLine=source.split('\n').filter(line=>line.startsWith('const rng = (g) => '));
  assert.equal(rngLine.length,1,'actual environment RNG anchor');
  const observed=rngLine[0].replace('const rng =','const __cmb42OriginalRng =')+
   "\nlet __cmb42EnvironmentObserver;\nconst rng=(g,consumer=['UNMAPPED.environment',new Error().stack])=>{const before=g.seed;const value=__cmb42OriginalRng(g);__cmb42EnvironmentObserver?.(g,before,value,consumer);return value;};\n";
  const accuracy='const accurate = Math.random() < acc';
  const dispersion='const off = accurate ? 0 : ((def.radius ?? 1.5) * 0.95 + 0.2 + Math.random() * 0.7) * (Math.random() < 0.5 ? -1 : 1);';
  assert.equal(source.split(accuracy).length,2,'single actual ordinary accuracy consumer');
  assert.equal(source.split(dispersion).length,2,'single actual ordinary dispersion consumers');
  const seedLine='  g.seed = Math.floor(Math.random() * 2 ** 32);';
  const startupWind='  g.wind = { a: rng(g) * Math.PI * 2, v: 0.3 + rng(g) * 0.5 };';
  const tickWind='  g.wind.a += (rng(g) - 0.5) * 0.6 * dt; g.wind.v = Math.min(1, Math.max(0.15, g.wind.v + (rng(g) - 0.5) * 0.3 * dt));';
  for(const anchor of [seedLine,startupWind,tickWind])assert.equal(source.split(anchor).length,2,'single actual seed/wind consumer anchor');
  const inputObserved=source.replace(seedLine,"  g.seed = Math.floor(__cmb42Global(['startup.environment.seed32']) * 2 ** 32);")
   .replace(startupWind,"  g.wind = { a: rng(g,['startup.wind.angle']) * Math.PI * 2, v: 0.3 + rng(g,['startup.wind.strength']) * 0.5 };")
   .replace(tickWind,"  g.wind.a += (rng(g,['weather.wind.angle',g.tick]) - 0.5) * 0.6 * dt; g.wind.v = Math.min(1, Math.max(0.15, g.wind.v + (rng(g,['weather.wind.strength',g.tick]) - 0.5) * 0.3 * dt));");
  const firingObserved=inputObserved.replace(accuracy,"const accurate = __cmb42Global(['combat.accuracy',g.tick,u.id,t.id,i]) < acc")
   .replace(dispersion,"const off = accurate ? 0 : ((def.radius ?? 1.5) * 0.95 + 0.2 + __cmb42Global(['combat.offset.magnitude',g.tick,u.id,t.id,i]) * 0.7) * (__cmb42Global(['combat.offset.sign',g.tick,u.id,t.id,i]) < 0.5 ? -1 : 1);");
  const layoutLine=source.split('\n').filter(line=>line.startsWith('  const usable = spawnsFor(map, opts.mode), spawnIdx = '));
  assert.equal(layoutLine.length,1,'actual constructor layout observer anchor');
  return {...result,source:firingObserved.replace(rngLine[0],observed).replace(layoutLine[0],layoutLine[0]+'\n  __cmb42LayoutObserver?.([...spawnIdx]);')+
   '\nlet __cmb42GlobalObserver;\nfunction __cmb42Global(consumer){__cmb42GlobalObserver?.(consumer);return Math.random();}\nexport const __cmb42SetGlobalObserver=observer=>{__cmb42GlobalObserver=observer;};\nlet __cmb42LayoutObserver;\nexport const __cmb42CanShoot=canShoot;\nexport const __cmb42SetLayoutObserver=observer=>{__cmb42LayoutObserver=observer;};\nexport const __cmb42SetEnvironmentObserver=observer=>{__cmb42EnvironmentObserver=observer;};\n'};
 }});
 const sim=await import(sourceUrl);
 const tech=await import(pathToFileURL(resolve(reference,'shared/tech.js')));
 const arena=JSON.parse(readFileSync(new URL('../rust/crates/simtest/tests/fixtures/base-pending-paratroopers.json',import.meta.url),'utf8')).map;
 const options={mode:'conquest',army:'standard',weather:false,weatherSeed:1,tech:true,supply:true,logistics:false,sandbox:false};
 // Lossless observation storage avoids repeating the unchanged map arrays at
 // every income tick. These references are never constructor/command inputs.
 const observations=[],observationIds=new Map();
 function compact(value){
  if(Array.isArray(value)){
   const rows=value.map(compact);
   if(rows.length<64)return rows;
   const key=JSON.stringify(rows);
   let id=observationIds.get(key);
   if(id===undefined){id=observations.length;observationIds.set(key,id);observations.push(rows);}
   return {$observation:id};
  }
  if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([key,item])=>[key,compact(item)]));
  return value;
 }
 const cases=[];
 for(const name of ['genuine-sniper-12-30-distance-and-recipient','genuine-at-7-28-researched-built-producers']){
  const random=randomProvider(701),saved=Math.random;
  const records=[],bindings={},inputs=[];
  let observedLayout;
  Math.random=()=>random.next();
  sim.__cmb42SetGlobalObserver(consumer=>random.consumer(consumer));
  sim.__cmb42SetEnvironmentObserver((g,before,value,consumer)=>random.observeEnvironment(g,before,value,consumer));
  sim.__cmb42SetLayoutObserver(layout=>{assert.equal(observedLayout,undefined);observedLayout=layout;});
  try{
   const g=sim.createGame(structuredClone(arena),['A','B'],false,[0,1],[0,1],options);
   assert.deepEqual(observedLayout,[0,1],'computed Source layout agrees with declared input; no injection');
   const pending=[],sequences=[0,0];
   let phase='constructor',stable=null,maxActors=10,combatStarted=false;
   const actorFacts=()=>new Map([...g.units].map(([id,u])=>[id,[u.type,u.owner,bits(u.hp)]]));
   const original=actorFacts();
   assert.equal(original.size,10,'retain both complete constructor armies');
   function receipt(event,detail={}){
    random.context({phase:`observe:${phase}`,tick:g.tick});
    const cursor=random.draws.length,environmentCursor=random.environment.length,cache=sim.snapshotCache(g);
    const recipients=g.players.map((_,owner)=>sim.snapshotFor(g,owner,g.shots,[],cache));
    const state={tick:g.tick,visionTick:g.visionTick,units:[...g.units],players:g.players,story:g.story,
     points:g.points,seed:g.seed,wind:g.wind,wx:g.wx,winner:g.winner,shots:g.shots,
     flights:g.flights,nades:g.nades,salvos:g.salvos,strikes:g.strikes,
     terrain:{chars:g.chars,cellHp:g.cellHp,cellState:g.cellState,structuralCells:g.structuralCells}};
    const shooter=g.units.get(bindings.sniper??bindings.at),probes=[];
    if(shooter)for(const label of ['rifle','mg','tank']){
     const target=g.units.get(bindings[label]);
     if(target)probes.push({shooter:shooter.id,target:target.id,visible:g.players[shooter.owner].visible.has(target.id),actualCanShoot:sim.__cmb42CanShoot(g,shooter,target)});
    }
    records.push({index:records.length,event,phase,tick:g.tick,detail:raw(detail),
     random:{count:random.draws.length,environmentCount:random.environment.length,state:random.state()},state:compact(raw(state)),recipients:compact(raw(recipients)),probes:compact(raw(probes))});
    assert.equal(random.draws.length,cursor,'observation consumes no random input');
    assert.equal(random.environment.length,environmentCursor,'observation consumes no environment draw');
    return recipients;
   }
   function retained(facts,exact=false){
    if(exact)assert.equal(g.units.size,facts.size,'no extra or missing actors');
    for(const [id,expected]of facts){
     const u=g.units.get(id);
     assert(u,`actor ${id} remains`);
     assert.deepEqual([u.type,u.owner],expected.slice(0,2),`identity at tick ${g.tick}`);
     if(!combatStarted||original.has(id))assert.equal(bits(u.hp),expected[2],`retained HP at tick ${g.tick}`);
     assert(u.hp>0,'every real actor survives');
    }
   }
   function consume(owner,command,sequence=null,expected=undefined){
    if(sequence===null)inputs.push({index:inputs.length,event:'immediate-command',record:records.length,owner,command:raw(command)});
    random.context({phase:`command:${phase}`,tick:g.tick,owner,sequence,command:command.t});
    const begin=random.draws.length,environmentBegin=random.environment.length,before=[...g.units.keys()],wallet=g.players[owner].mp,spent=g.story?.[owner]?.mpSpent??0;
    const result=sim.command(g,owner,command);
    receipt('command-consumed',{owner,sequence,command,result,beforeIds:before,
     born:[...g.units.keys()].filter(id=>!before.includes(id)),walletBefore:wallet,walletAfter:g.players[owner].mp,
     spentBefore:spent,spentAfter:g.story?.[owner]?.mpSpent??0,drawRange:[begin,random.draws.length],environmentRange:[environmentBegin,random.environment.length]});
    assert.equal(result,expected,`ordinary ${command.t} command matches declared admission`);
   }
   function queue(owner,command){
    const row={owner,sequence:++sequences[owner],tick:g.tick+1,command};
    inputs.push({index:inputs.length,event:'queued-command',record:records.length,...row,command:raw(command)});
    pending.push(row);
    receipt('command-enqueued',row);
   }
   function step(){
    assert(g.tick<8000,'finite complete-scene tick bound');
    const target=g.tick+1,rows=pending.splice(0).sort((a,b)=>a.owner-b.owner||a.sequence-b.sequence);
    inputs.push({index:inputs.length,event:'step',record:records.length,tick:target});
    for(const row of rows){assert.equal(row.tick,target);consume(row.owner,row.command,row.sequence);}
    const before=g.tick,drawBegin=random.draws.length,environmentBegin=random.environment.length,shotBegin=g.shots.length;
    const wallets=g.players.map(p=>p.mp);
    random.context({phase:`step:${phase}`,tick:target});
    sim.step(g);
    assert.equal(g.tick,before+1,'every actual tick is contiguous');
    const recipients=receipt('tick-consumed',{commands:rows.length,drawRange:[drawBegin,random.draws.length],environmentRange:[environmentBegin,random.environment.length],shotRange:[shotBegin,g.shots.length],income:wallets.map((before,owner)=>({owner,before,after:g.players[owner].mp,rate:g.players[owner].inc,dt:0.05}))});
    retained(original);
    if(stable)retained(stable,true);
    assert.equal(g.units.size,maxActors,'only the recorded real births exist');
    if(!combatStarted){
     assert.equal(g.shots.length,shotBegin,'setup remains quiet through real stance commands');
     assert.equal(random.draws.length,drawBegin,'quiet setup consumes no global combat draws');
    }
    for(let owner=0;owner<2;owner++){
     assert(g.players[owner].inc>0,'genuine positive income');
     assert.equal(g.players[owner].mp,wallets[owner]+g.players[owner].inc*0.05,'wallet changes only by actual income in these ticks');
    }
    assert.equal(g.winner,null,'scene must remain an active match');
    return recipients;
   }
   function quiet(){
    for(let owner=0;owner<2;owner++){
     const ids=[...g.units.values()].filter(u=>u.owner===owner).map(u=>u.id);
     for(const [key,on]of [['holdFire',true],['holdPos',true],['autoRetreat',false]])queue(owner,{t:'stance',ids,key,on});
     queue(owner,{t:'autocast',ids,on:false});
    }
   }
   function funds(owner,price){
    for(let count=0;g.players[owner].mp<price&&count<4000;count++)step();
    assert(g.players[owner].mp>=price,'actual constructor wallet and accrued income fund the command');
   }
   function buy(label,owner,kind,from){
    funds(owner,sim.priceOf(g,kind).mp);
    const makers=sim.productionBuildings(g,owner,kind),producer=from===undefined?makers[0]:makers.find(u=>u.id===from);
    assert(producer&&producer.owner===owner&&producer.hp>0&&producer.built===1,'genuine existing completed owned producer');
    const before=actorFacts(),wallet=g.players[owner].mp,spent=g.story?.[owner]?.mpSpent??0,price=sim.priceOf(g,kind).mp;
    consume(owner,{t:'buy',unit:kind,...(from===undefined?{}:{from})});
    const born=[...g.units.values()].filter(u=>!before.has(u.id));
    assert.equal(born.length,1);assert.equal(g.units.size,before.size+1);retained(before);
    const u=born[0];assert.equal(u.owner,owner);assert.equal(u.type,kind);
    assert.equal(g.players[owner].mp,wallet-price);assert.equal(g.story[owner].mpSpent,spent+price);
    assert(g.units.has(producer.id),'producer remains after real purchase');
    bindings[label]=u.id;maxActors++;
    receipt('purchase-provider',{label,id:u.id,producer:producer.id,kind,owner,price});
    for(const [key,on]of [['holdFire',true],['holdPos',true],['autoRetreat',false]])consume(owner,{t:'stance',ids:[u.id],key,on});
    consume(owner,{t:'autocast',ids:[u.id],on:false},null,sim.UNITS[kind].ab.id==='none'?'needs':undefined);
    return u;
   }
   function stage(goals){
    let arrived=false;
    for(let count=0;count<1200;count++){
     step();
     if(goals.every(([id,x,z])=>{const u=g.units.get(id);return u.path.length===0&&Math.hypot(u.x-x,u.z-z)<=1e-6;})){arrived=true;break;}
    }
    assert(arrived,'ordinary moves reach all authored destinations');
    for(let count=0;count<20;count++)step();
   }
   function choices(shooter,candidates,expected,count=20){
    for(let turn=0;turn<count;turn++){
     const views=step();
     geometry(shooter,candidates.map((target,index)=>[target,phase==='at-select-tank'?[7,28][index]:[12,phase==='sniper-select-mg'?30:40][index]]));
     assert.equal(shooter.attackId,0,'no explicit attack order');
     for(const target of candidates){
      assert(g.players[0].visible.has(target.id),'natural candidate visibility');
      assert(sim.__cmb42CanShoot(g,shooter,target),'actual canShoot observer');
     }
     assert.equal(shooter.targetId,expected.id,'actual automatic role choice');
     const row=views[0].units.find(row=>row[0]===shooter.id);
     assert(row);assert.equal(row[9],expected.id,'actual owner recipient target');
     assert(candidates.every(u=>u.hp>0),'original rifle survival');
    }
   }
   function geometry(shooter,pairs){
    for(const [target,distance]of pairs){
     assert(Math.abs(Math.hypot(target.x-shooter.x,target.z-shooter.z)-distance)<=1e-6,'literal named geometry');
     assert(g.players[0].visible.has(target.id));
     assert(sim.__cmb42CanShoot(g,shooter,target));
    }
   }
   receipt('constructor',{map:arena,names:['A','B'],shuffle:false,teams:[0,1],factions:[0,1],options,randomSeed:701});
   assert(g.players.every(p=>p.mp===sim.CFG.mpStart),'untouched constructor wallets');
   phase='quiet-income-prefix';stable=original;quiet();
   for(let count=0;count<20;count++)step();
   stable=null;
   if(name.startsWith('genuine-sniper')){
    phase='sniper-purchases';
    const sniper=buy('sniper',0,'sniper'),rifle=buy('rifle',1,'rifle'),mg=buy('mg',1,'mg');
    stable=actorFacts();quiet();step();
    phase='sniper-stage-12-30';
    queue(0,{t:'move',orders:[[sniper.id,60,80]]});
    queue(1,{t:'move',orders:[[rifle.id,72,80],[mg.id,90,80]]});
    stage([[sniper.id,60,80],[rifle.id,72,80],[mg.id,90,80]]);
    geometry(sniper,[[rifle,12],[mg,30]]);
    assert.equal(sniper.targetId,0);
    queue(0,{t:'stance',ids:[sniper.id],key:'holdFire',on:false});
    combatStarted=true;
    phase='sniper-select-mg';choices(sniper,[rifle,mg],mg);
    phase='sniper-distance-control';
    queue(0,{t:'stance',ids:[sniper.id],key:'holdFire',on:true});
    queue(1,{t:'move',orders:[[mg.id,100,80]]});stage([[mg.id,100,80]]);
    geometry(sniper,[[rifle,12],[mg,40]]);assert.equal(sniper.targetId,0);
    queue(0,{t:'stance',ids:[sniper.id],key:'holdFire',on:false});
    phase='sniper-select-rifle';choices(sniper,[rifle,mg],rifle);
    phase='sniper-hidden-recipient-control';
    queue(0,{t:'stance',ids:[sniper.id],key:'holdFire',on:true});
    queue(1,{t:'move',orders:[[mg.id,120,80]]});stage([[mg.id,120,80]]);
    assert(!g.players[0].visible.has(mg.id));assert(!sim.__cmb42CanShoot(g,sniper,mg));
    const views=receipt('recipient-control');
    assert(!views[0].units.some(row=>row[0]===mg.id),'hidden enemy omitted from actual owner recipient');
    assert.equal(views[0].units.find(row=>row[0]===sniper.id)[9],0);
   }else{
    phase='at-quiet-constructor';stable=original;quiet();step();
    phase='at-tier-research';
    const spent=g.players.map((_,owner)=>g.story?.[owner]?.mpSpent??0),prices=g.players.map(p=>tech.techCost(p,'tier',false).mp);
    for(let owner=0;owner<2;owner++)funds(owner,prices[owner]);
    for(let owner=0;owner<2;owner++){assert.equal(g.players[owner].tier,1);assert.equal(g.players[owner].lab.length,0);queue(owner,{t:'tech',kind:'tier'});}
    let researched=false;
    for(let count=0;count<1600;count++){step();if(g.players.every(p=>p.tier===2)){researched=true;break;}}
    assert(researched,'real tier jobs finish within bound');
    for(let owner=0;owner<2;owner++){assert.equal(g.players[owner].lab.length,0);assert.equal(g.story[owner].mpSpent-spent[owner],prices[owner]);}
    phase='at-motorpool-build';stable=null;
    for(let owner=0;owner<2;owner++)funds(owner,sim.UNITS.motorpool.cost);
    const poolIds=[],buildSpent=g.players.map((_,owner)=>g.story[owner].mpSpent);
    for(let owner=0;owner<2;owner++){
     const builder=[...g.units.values()].find(u=>u.owner===owner&&u.type==='rifle');assert(builder);
     const {x,z}=g.players[owner].spawn,site={x,z:z+16};
     assert(sim.teamSees(g,g.players[owner].team,site),'natural visible build ground');
     queue(owner,{t:'stance',ids:[builder.id],key:'holdPos',on:false});
     queue(owner,{t:'build',ids:[builder.id],kind:'motorpool',...site});
    }
    maxActors+=2;step();
    for(let owner=0;owner<2;owner++){
     const sites=[...g.units.values()].filter(u=>u.owner===owner&&u.type==='motorpool');assert.equal(sites.length,1);
     const site=sites[0];assert(!original.has(site.id));assert(site.built>=0&&site.built<1);poolIds.push(site.id);
     assert.equal(g.story[owner].mpSpent-buildSpent[owner],sim.UNITS.motorpool.cost);
     bindings[`motorpool${owner}`]=site.id;
    }
    let built=false;
    for(let count=0;count<2400;count++){step();if(poolIds.every(id=>g.units.get(id).built===1)){built=true;break;}}
    assert(built,'actual builders complete both paid sites');step();
    phase='at-purchases';
    const at=buy('at',0,'at',poolIds[0]),rifle=buy('rifle',1,'rifle'),tank=buy('tank',1,'tank',poolIds[1]);
    stable=actorFacts();quiet();step();
    phase='at-stage-7-28';
    queue(0,{t:'move',orders:[[at.id,60,80]]});queue(1,{t:'move',orders:[[rifle.id,67,80]]});
    queue(1,{t:'move',orders:[[tank.id,88,80]],face:Math.atan2(0,-1)});
    stage([[at.id,60,80],[rifle.id,67,80],[tank.id,88,80]]);
    for(let count=0;count<20;count++)step();
    assert(at.still>=2,'actual stationary setup');geometry(at,[[rifle,7],[tank,28]]);
    assert(Math.cos(Math.atan2(at.z-tank.z,at.x-tank.x)-tank.rot)>.5,'actual tank front faces shooter');
    combatStarted=true;
    phase='at-select-tank';queue(0,{t:'stance',ids:[at.id],key:'holdFire',on:false});choices(at,[rifle,tank],tank);
   }
   assert.equal(pending.length,0,'consume every authored command');
   assert(random.draws.every(row=>!row.consumer[0].startsWith('UNMAPPED.')),'every actual global consumer must be identified');
   assert(random.environment.every(row=>!row.consumer[0].startsWith('UNMAPPED.')),'every actual environment consumer must be identified');
   retained(stable,true);receipt('scene-complete',{bindings,totalTicks:g.tick});
   cases.push({name,sharedInput:{map:arena,names:['A','B'],teams:[0,1],factions:[0,1],layout:[0,1],shuffle:false,options,seed:701,inputs},observedLayout,bindings,records,random:{algorithm:'splitmix64-global-source-input',seed:701,draws:random.draws,environment:random.environment,finalState:random.state()}});
  }catch(error){
   writeFileSync(failureFile,JSON.stringify({reference:pin,name,inputs,bindings,records,random:{draws:random.draws,environment:random.environment,state:random.state()},completedCases:cases,observations,error:{message:error.message,stack:error.stack}})+'\n',{flag:'wx'});
   throw error;
  }finally{Math.random=saved;sim.__cmb42SetEnvironmentObserver(undefined);sim.__cmb42SetLayoutObserver(undefined);sim.__cmb42SetGlobalObserver(undefined);}
 }
 clean();
 const data=JSON.stringify({format:'cmb42-genuine-tagged-v2',reference:pin,scope:'two genuine-input CMB42 scenes; native/aligned qualification pending',observations,cases})+'\n';
 if(flags.includes('--check'))assert.equal(readFileSync(file,'utf8'),data,'bounded fixture must match exactly');
 else writeFileSync(file,data,{flag:'wx'});
 console.log(`${flags.includes('--check')?'Checked':'Recorded'} exactly ${cases.length} new-only contiguous CMB42 Source scenes`);
}
