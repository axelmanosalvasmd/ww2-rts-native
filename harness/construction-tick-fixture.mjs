// Original full creation, commands and steps. Construction phases are never extracted.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
assert.ok(process.argv.slice(2).every(a=>a==='--check'));
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??join(root,'reference/ww2-rts'));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};
clean();
const sim=await import(pathToFileURL(join(reference,'shared/sim.js')));
const {recoveryRate}=await import(pathToFileURL(join(reference,'shared/logistics.js')));
const {rebuildGrid}=await import(pathToFileURL(join(reference,'shared/grid.js')));
const view=new DataView(new ArrayBuffer(8)),bits=v=>{view.setFloat64(0,v,false);return view.getBigUint64(0,false).toString(16).padStart(16,'0');};
const number=v=>{view.setBigUint64(0,BigInt('0x'+v),false);return view.getFloat64(0,false);};
const encode=v=>v===undefined?null:typeof v==='number'?{f64:bits(v)}:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;
const point=p=>p?{x:bits(p.x),z:bits(p.z)}:null;
const paid=d=>d?{...point(d),kind:d.kind,cells:structuredClone(d.cells),t:bits(d.t),dir:bits(d.dir),front:d.front===undefined?null:bits(d.front),n:d.n??null,reach:d.reach===undefined?null:bits(d.reach),project:d.project?.id??null}:null;
const changes=(a,b,path=[],out=[])=>{if(Object.is(a,b))return out;if(Array.isArray(a)&&Array.isArray(b)&&a.length===b.length){for(let i=0;i<a.length;i++)changes(a[i],b[i],[...path,i],out);return out;}if(a&&b&&typeof a==='object'&&typeof b==='object'&&!Array.isArray(a)&&!Array.isArray(b)){for(const k of new Set([...Object.keys(a),...Object.keys(b)]))if(!Object.hasOwn(b,k))out.push([[...path,k]]);else changes(a[k],b[k],[...path,k],out);return out;}out.push([path,b]);return out;};
const staticKinds=['hq','barracks','motorpool','airfield','armory','flakpos','pillbox','tower'];
const mapFor=({wall=false,slope=0,hiddenBlock=false,house=false}={})=>{
 const map={name:'Full-step paid ground construction',w:60,h:60,rows:Array(60).fill(wall?'.'.repeat(20)+'W'+'.'.repeat(39):'.'.repeat(60)),spawns:[{x:4,y:4},{x:55,y:55}],points:[{x:30,y:30}]};
 if(house)map.rows[12]=map.rows[12].slice(0,14)+'B'+map.rows[12].slice(15);
 if(slope){map.heights=Array(60).fill('0'.repeat(60));map.heights[11]=map.heights[11].slice(0,15)+slope+map.heights[11].slice(16);}
 if(hiddenBlock)map.rows[11]=map.rows[11].slice(0,51)+'###'+map.rows[11].slice(54);
 return map;
};
// Match seed701's first Weather-stream draw supplies the existing Source seed draw.
// This pairs constructor input only; no runtime random stream or reset is added.
const u64=v=>BigInt.asUintN(64,v),gamma=0x9e3779b97f4a7c15n;
const mix=v=>{v=u64((v^(v>>30n))*0xbf58476d1ce4e5b9n);v=u64((v^(v>>27n))*0x94d049bb133111ebn);return v^(v>>31n);};
const bootstrap=Number(mix(u64(mix(701n^u64(2n*gamma))+gamma))&0xffffffffn);
const constructorDraw=bootstrap/2**32;
const originalRandom=Math.random,cases=[];let totalSteps=0,totalSnapshots=0;
try{
 function record(name,actions,options={}){
  const draws=[];Math.random=()=>{draws.push(constructorDraw);return constructorDraw;};
  const map=mapFor(options);assert.equal(sim.validateMap(map),null);
  const opts={weather:false,tech:options.tech??false,supply:false,logistics:false};
  const g=sim.createGame(map,['Crew','Opposition'],false,[0,1],[0,1],opts);
  assert.equal(g.mode,undefined);assert.equal(sim.isConstructionMode(g),false);assert.equal(sim.isSkirmishBaseMode(g),true);assert.equal(g.wx,null);
  assert.equal(draws.length,1,'full Source construction must consume one bootstrap draw');
  Math.random=()=>{draws.push(null);throw Error(`${name}: quiet runtime drew Math.random`);};
  const creationRandomDraws=draws.length,initialActorOrder=[...g.units.keys()],aliases={};
  const initialSeed=g.seed>>>0,spawn=[];const size=sim.UNITS[options.sceneKind??'flakpos'].size;
  for(let i=0;i<(options.workers??1);i++){
   const u=sim.internals.spawnUnit(g,0,'engineer');aliases[`worker${i+1}`]=u.id;
   const pose={x:28,z:22+size+(i?.5:-.5)};Object.assign(u,pose);spawn.push({id:u.id,type:'engineer',owner:0,...pose});
  }
  if(options.vehicleFactory){const u=sim.internals.spawnUnit(g,0,'medium');aliases.vehicle=u.id;Object.assign(u,{x:25,z:26});spawn.push({id:u.id,type:'medium',owner:0,x:25,z:26});}
  aliases.nonBuilder=[...g.units.values()].find(u=>u.owner===0&&u.type==='mg').id;
  for(const u of g.units.values())Object.assign(u,{holdFire:true,holdPos:true,auto:false,autoRetreat:false});
  g.players[0].mp=5000;g.pathRetryLimit=options.retryLimit??1;g.cellJournal=[];
  if(options.wall){const u=[...g.units.values()].find(u=>u.owner===0&&u.type==='rifle');aliases.observer=u.id;Object.assign(u,{x:69,z:20});}
  if(options.tier!==undefined)g.players[0].tier=options.tier;
  rebuildGrid(g);sim.internals.updateVision(g);
  assert.equal(g.seed>>>0,initialSeed);assert.equal(draws.length,creationRandomDraws);
  const setup={map,names:['Crew','Opposition'],shuffle:false,factions:[0,1],teams:[0,1],opts,initialActorOrder,spawn,initialAliases:structuredClone(aliases),prime:{allUnits:{holdFire:true,holdPos:true,auto:false,autoRetreat:false},player0:{mp:5000,...(options.tier===undefined?{}:{tier:options.tier})},pathRetryLimit:options.retryLimit??1,cellJournal:[],...(options.wall?{observer:{id:aliases.observer,x:69,z:20}}:{})},creationRandomDraws,setupRandomDraws:draws.length-creationRandomDraws};
  let previous=null,newAt=0,journalAt=0,index=0;const patches=[],records=[],checkpoints=new Map();
  const sites=new Map();
  const snapshot=()=>{
   const units=[...g.units.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,hp:bits(u.hp),...point(u),rot:bits(u.rot),aim:bits(u.aim),built:u.built===undefined?null:bits(u.built),cells:u.cells??null,structureId:u.structureId??null,build:u.build??null,dig:paid(u.dig),entrench:u.entrench?.id??null,path:u.path.map(point),orders:encode(u.orders),follow:u.follow??null,garrison:u.garrison,riding:u.riding??null,holdFire:u.holdFire??null,holdPos:u.holdPos??null,auto:u.auto??null,autoRetreat:u.autoRetreat??null,retreating:u.retreating,repath:bits(u.repath),still:u.still===undefined?null:bits(u.still),moveSpeed:bits(u.moveSpeed),moveOutcome:u.moveOutcome??null,moveOutcomeTick:u.moveOutcomeTick??null,moveResult:encode(u.moveResult),queue:encode(u.queue),productionJobs:encode(u.productionJobs),prog:u.prog===undefined?null:bits(u.prog),...(options.reserveCase?{logistics:encode(u.logistics),recoveryRate:bits(recoveryRate(u))}:{})}));
   for(const u of g.units.values())if(u.cells)sites.set(u.id,u);
   const cells=new Set(g.cellLog.map(row=>row[0]));for(const u of sites.values())for(const c of u.cells)cells.add(c);
   const cellRows=[...cells].sort((a,b)=>a-b).map(c=>({c,ch:g.chars[c],ground:g.ground[c],object:g.objects[c],mine:g.mineLayer[c],flags:g.flags[c],height:g.height?.[c]??0,hpBits:new Uint32Array(g.cellHp.buffer)[c],wearBits:new Uint32Array(g.wear.buffer)[c],state:g.cellState[c],burnt:g.burnt[c],scarBits:new Uint32Array(g.scar.buffer)[c],frontBits:g.trenchFront?new Uint32Array(g.trenchFront.buffer)[c]:null}));
   const batch=g.newCells.slice(newAt),journal=g.cellJournal.slice(journalAt);newAt=g.newCells.length;journalAt=g.cellJournal.length;
   return {tick:g.tick,mode:encode(g.mode),constructionMode:sim.isConstructionMode(g),skirmishBaseMode:sim.isSkirmishBaseMode(g),actorOrder:[...g.units.keys()],aliases:structuredClone(aliases),units,sites:[...sites.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,live:g.units.get(u.id)===u,hp:bits(u.hp),built:bits(u.built),cells:u.cells,structureId:u.structureId,at:point(u),rot:bits(u.rot),cost:bits(sim.UNITS[u.type].cost),buildTime:bits(sim.UNITS[u.type].buildTime),maxHp:bits(sim.UNITS[u.type].hpPer)})),structures:[...g.structures.values()].map(s=>({id:s.id,kind:s.kind,entityId:s.entityId??null,sections:s.sections.map(t=>encode(t))})),structuralCellOrder:[...g.structuralCells.keys()],buildingCells:encode([...g.buildingCells]),players:g.players.map(p=>({slot:p.slot,team:p.team,mp:bits(p.mp),inc:bits(p.inc),vp:bits(p.vp),tier:p.tier??null,mun:p.mun===undefined?null:bits(p.mun),fuel:p.fuel===undefined?null:bits(p.fuel),out:p.out??false,away:p.away??false})),story:encode(g.story),timeline:encode(g.timeline),terrain:{cells:cellRows,newCells:encode(batch),newCellsTotal:g.newCells.length,journal,journalTotal:g.cellJournal.length,cellLog:encode(g.cellLog),versions:[g.terrainVersion??0,g.obstructionVersion??0,g.infantryRegionVersion??0,g.vehicleRegionVersion??0,g.navalRegionVersion??0],mines:[...g.mines],mineSeen:[...g.mineSeen]},pathStats:encode(g.pathStats),seed:g.seed>>>0,wind:{a:bits(g.wind.a),v:bits(g.wind.v)},weather:encode(g.weather),wx:encode(g.wx),nextId:g.nextId,winner:g.winner,shots:encode(g.shots),falling:encode(g.fallingSections),randomDraws:draws.length};
  };
  const save=()=>{const state=snapshot();if(previous===null)checkpoints.set('initial',state);else patches.push(changes(previous,state));previous=state;index++;totalSnapshots++;return state;};
  save();
  const resultUnit=(state,name)=>state.units.find(u=>u.id===aliases[name]);
  for(const input of actions){const a=structuredClone(input),beforeDraws=draws.length,beforeSeed=g.seed>>>0,beforeIds=new Set(g.units.keys());let result=null,count=1;
   if(a.op==='command'){
    if(a.cmd.orders)a.cmd.orders=a.cmd.orders.map(([id,x,z])=>[typeof id==='string'?aliases[id]:id,x,z]);
    if(a.cmd.ids)a.cmd.ids=a.cmd.ids.map(id=>typeof id==='string'?aliases[id]:id);
    if(typeof a.cmd.id==='string')a.cmd.id=aliases[a.cmd.id];
    result=sim.command(g,a.slot??0,a.cmd)??null;
    if(a.as){const fresh=[...g.units.values()].filter(u=>!beforeIds.has(u.id));assert.equal(fresh.length,1);aliases[a.as]=fresh[0].id;}
    assert.equal(g.seed>>>0,beforeSeed,`${name}: construction command changed environment seed`);save();
   }else if(a.op==='prime'){
    for(const [who,values]of Object.entries(a.units??{}))Object.assign(g.units.get(aliases[who]),structuredClone(values));
    for(const [slot,values]of Object.entries(a.players??{}))Object.assign(g.players[Number(slot)],structuredClone(values));
    rebuildGrid(g);sim.internals.updateVision(g);assert.equal(g.seed>>>0,beforeSeed);save();
   }else if(a.op==='steps'||a.op==='until'){
    count=0;const done=()=>{const u=g.units.get(aliases[a.target]);return a.condition==='finished'?u?.built>=1:a.condition==='progress'?(u?.built??0)>0:a.condition==='digEmpty'?!u?.dig:a.condition==='buildEmpty'?!u?.build:false;};
    const limit=a.op==='steps'?a.count:a.max;while(count<limit&&(a.op==='steps'||!done())){
     const drawsBefore=draws.length;sim.step(g);assert.equal(draws.length,drawsBefore,`${name}: source step drew Math.random`);count++;totalSteps++;save();
    }
    if(a.op==='until')assert.ok(done(),`${name}: ${a.target} failed to ${a.condition}`);
   }else throw Error(a.op);
   assert.equal(draws.length,beforeDraws,`${name}: quiet action drew Math.random`);
   if(a.expect!==undefined)assert.equal(result,a.expect,`${name}: command result`);
   if(a.label)checkpoints.set(a.label,previous);
   records.push({action:a,result,steps:a.op==='steps'||a.op==='until'?count:0,lastSnapshot:index-1,randomDraws:draws.length-beforeDraws,seedBefore:beforeSeed,seedAfter:g.seed>>>0});
  }
  const at=label=>checkpoints.get(label),spent=state=>state.story?.[0]?.mpSpent?.f64??bits(0),site=(state,key='site')=>state.sites.find(u=>u.id===aliases[key]);
  for(const r of records)if(r.action.as){const state=checkpoints.get(r.action.label);if(state){const s=site(state,r.action.as);assert.equal(s.live,true);assert.equal(s.built,bits(0));assert.equal(s.hp,bits(number(s.maxHp)*.25));}}
  if(name.startsWith('stamp:')){assert.equal(site(at('stamp')).type,name.slice(6));assert.ok(number(site(at('turn')).built)>0);}
  if(name==='capture-before-later-movement'){assert.equal(site(at('first')).built,bits(0));const u=resultUnit(at('first'),'worker1'),s=site(at('first'));assert.ok(Math.hypot(number(u.x)-number(s.at.x),number(u.z)-number(s.at.z))<=sim.UNITS.hq.radius+sim.CFG.classic.buildReach,JSON.stringify({pose:[number(u.x),number(u.z)],site:[number(s.at.x),number(s.at.z)],path:u.path}));assert.ok(number(site(at('second')).built)>0);}
  if(name==='two-workers-finish-and-repair'){assert.equal(number(site(at('first')).built),2**sim.CFG.classic.crew/sim.UNITS.tower.buildTime*sim.TICK);assert.equal(resultUnit(at('complete'),'worker1').build,aliases.site);assert.equal(resultUnit(at('cleared'),'worker1').build,0);assert.equal(spent(at('assist')),spent(at('repaired')));assert.equal(site(at('repaired')).hp,bits(sim.UNITS.tower.hpPer));}
  if(name==='one-worker-completion'){assert.equal(number(site(at('first')).built),1/sim.UNITS.tower.buildTime*sim.TICK);assert.equal(resultUnit(at('complete'),'worker1').build,aliases.site);assert.ok(number(site(at('complete')).hp)<sim.UNITS.tower.hpPer);assert.equal(resultUnit(at('repair-turn'),'worker1').build,aliases.site);assert.equal(site(at('repair-turn')).hp,bits(sim.UNITS.tower.hpPer));assert.equal(resultUnit(at('cleared'),'worker1').build,0);}
  if(name==='paused-site-after-stop'){assert.equal(site(at('stopped')).built,site(at('waited')).built);assert.equal(spent(at('stopped')),spent(at('waited')));assert.ok(number(at('waited').players[0].mp)>number(at('stopped').players[0].mp));}
  if(name==='dead-site-and-missing-assist'){assert.equal(site(at('retired')).live,false);assert.equal(resultUnit(at('retired'),'worker1').build,0);assert.equal(spent(at('stamp')),spent(at('missing')));assert.equal(resultUnit(at('missing'),'worker1').follow,77);}
  if(name==='accepted-build-cancels-paid-dig'){assert.ok(resultUnit(at('dig'),'worker1').dig);assert.equal(resultUnit(at('stamp'),'worker1').dig,null);assert.equal(number(spent(at('stamp'))),sim.FORTS.trench.cost+sim.UNITS.flakpos.cost);}
  if(name==='refused-and-queued-build-retains-paid-dig'){assert.deepEqual(resultUnit(at('dig'),'worker1').dig,resultUnit(at('refused'),'worker1').dig);assert.equal(resultUnit(at('refused'),'worker1').follow,0);assert.deepEqual(resultUnit(at('dig'),'worker1').dig,resultUnit(at('queued'),'worker1').dig);const orders=resultUnit(at('queued'),'worker1').orders;assert.equal(orders.length,1);assert.deepEqual(orders[0],encode({t:'assist',ids:[aliases.worker1],id:aliases.site,x:32,z:24}));assert.equal(site(at('queued')).built,bits(0));assert.equal(resultUnit(at('assist'),'worker1').build,aliases.site);assert.ok(number(site(at('assist')).built)>0);assert.equal(spent(at('queued')),spent(at('assist')));}
  if(name==='dead-worker-excluded-before-late-retirement'){assert.equal(resultUnit(at('turn'),'worker2'),undefined);assert.equal(number(site(at('turn')).built),1/sim.UNITS.hq.buildTime*sim.TICK);assert.equal(at('turn').story[0].losses.f64,bits(1));}
  if(name==='actual-build-retry-drop'){assert.equal(resultUnit(at('drop'),'worker1').build,0);assert.equal(resultUnit(at('drop'),'worker1').moveOutcome,'unreachable');assert.equal(site(at('drop')).built,bits(0));assert.equal(site(at('drop')).live,true);assert.ok(g.pathStats.dropped>0);assert.equal(spent(at('stamp')),spent(at('drop')));}
  if(name==='slope-leveling-stamp'){assert.ok(site(at('stamp')).cells.every(c=>at('stamp').terrain.cells.find(row=>row.c===c).height===1));}
  if(name==='refused-build-follow-precedence'){assert.equal(resultUnit(at('noMoney'),'worker1').follow,0);assert.equal(resultUnit(at('noBuilders'),'nonBuilder').follow,0);}
  if(name==='finished-repair-budget-and-missing-first-reserve'){assert.equal(g.logisticsEnabled,false);const before=at('limited-ready'),after=at('limited-turn');const budget=.001*5+.002*5;assert.equal(site(after).hp,bits(number(site(before).hp)+budget));assert.equal(resultUnit(after,'worker1').logistics.provisions.f64,bits(0));assert.equal(resultUnit(after,'worker2').logistics.provisions.f64,bits(.002-(budget-.001*5)/5));assert.equal(resultUnit(before,'worker1').recoveryRate,bits(.5));assert.equal(resultUnit(before,'worker2').recoveryRate,bits(.5));assert.equal(resultUnit(at('missing-turn'),'worker1').logistics,null);assert.equal(resultUnit(at('missing-turn'),'worker2').logistics.provisions.f64,bits(1));const rate=(1+.5)/2;assert.equal(site(at('missing-turn')).hp,bits(number(site(at('missing-ready')).hp)+sim.UNITS.tower.hpPer*(sim.CFG.classic.repair*2**sim.CFG.classic.crew*rate)*sim.TICK));assert.equal(spent(before),spent(at('missing-turn')));}
  if(name==='garrison-build-assist-and-queued-retention'){assert.equal(resultUnit(at('held'),'worker1').garrison,734);assert.equal(resultUnit(at('stamp'),'worker1').garrison,-1);assert.equal(resultUnit(at('queued'),'worker1').garrison,734);assert.equal(resultUnit(at('queued'),'worker1').orders.length,1);assert.equal(resultUnit(at('waiting'),'worker1').garrison,734);assert.equal(resultUnit(at('assist'),'worker1').garrison,-1);assert.equal(resultUnit(at('assist'),'worker1').orders.length,0);assert.equal(spent(at('stamp')),spent(at('assist')));}
  if(name==='warm-infantry-and-vehicle-routes-around-paid-stamp'){assert.ok(resultUnit(at('warm'),'worker1').path.length);assert.ok(resultUnit(at('warm'),'vehicle').path.length);assert.notDeepEqual(resultUnit(at('warm'),'vehicle').path,resultUnit(at('reroute'),'vehicle').path);assert.equal(site(at('stamp')).built,bits(0));assert.equal(spent(at('stamp')),spent(at('reroute')));}
  if(name.startsWith('retry-two-')){assert.equal(number(at('first-failure').pathStats.dropped.f64),0);assert.equal(resultUnit(at('first-failure'),'worker1').build,aliases.site);assert.equal(number(at('retained').pathStats.dropped.f64),0);assert.equal(resultUnit(at('retained'),'worker1').build,aliases.site);assert.equal(number(at('drop').pathStats.dropped.f64),1);assert.equal(resultUnit(at('drop'),'worker1').build,0);assert.equal(at('drop').tick,name==='retry-two-immediate-assist-resets'?22:21);assert.equal(spent(at('stamp')),spent(at('drop')));if(name==='retry-two-queued-assist-retains'){assert.equal(resultUnit(at('drop'),'worker1').orders.length,1);}}
  if(name==='pillbox-raw-direction-and-target-refusal-follow'){assert.equal(resultUnit(at('unlisted'),'worker1').follow,77);assert.equal(resultUnit(at('unseen'),'worker1').follow,77);assert.equal(site(at('raw-dir')).rot,bits(7));assert.equal(site(at('turn')).rot,bits(7));assert.equal(resultUnit(at('raw-dir'),'worker1').follow,0);}
  const initial=checkpoints.get('initial');cases.push({name,setup,initial,records,patches,snapshots:index,finalAliases:structuredClone(aliases),witnesses:Object.fromEntries([...checkpoints].filter(([k])=>k!=='initial').map(([k,v])=>[k,{tick:v.tick,actorOrder:v.actorOrder,site:site(v)??null,mpSpent:spent(v)}]))});
 }
 const build=(kind,extra={},label='stamp',as='site')=>({op:'command',cmd:{t:'build',kind,ids:['worker1'],x:30+sim.UNITS[kind].size,z:22+sim.UNITS[kind].size,...extra},as,label,expect:null});
 const steps=(count,label)=>({op:'steps',count,label});
 const until=(target,condition,max,label)=>({op:'until',target,condition,max,label});
 const command=(cmd,label,expect=null)=>({op:'command',cmd,label,expect});
 for(const kind of staticKinds){
  const actions=[];
  if(kind==='airfield'){actions.push(build('motorpool',{ids:['worker1','worker2']},'motor-stamp','motor'),until('motor','finished',700,'motor-complete'),steps(1,'motor-cleared'),{op:'prime',units:{worker1:{x:42,z:24.5},worker2:{x:42,z:25.5}}});actions.push(build(kind,{x:47,z:25}));}
  else actions.push(build(kind,{...(kind==='pillbox'?{dir:.7}:{})}));
  actions.push(steps(2,'turn'));record(`stamp:${kind}`,actions,{sceneKind:kind,workers:kind==='airfield'?2:1,tech:kind==='armory',...(kind==='armory'?{tier:2}:{})});
 }
 record('capture-before-later-movement',[{op:'prime',units:{worker1:{x:38.51,z:25}}},build('hq'),steps(1,'first'),steps(1,'second')],{sceneKind:'hq'});
 record('one-worker-completion',[build('tower'),steps(1,'first'),until('site','finished',600,'complete'),steps(1,'repair-turn'),steps(1,'cleared')],{sceneKind:'tower'});
 record('two-workers-finish-and-repair',[build('tower',{ids:['worker1','worker2']}),steps(1,'first'),until('site','finished',400,'complete'),steps(2,'cleared'),{op:'prime',units:{site:{hp:299.5}}},command({t:'assist',ids:['worker1','worker2'],id:'site'},'assist'),steps(6,'repaired')],{sceneKind:'tower',workers:2});
 record('paused-site-after-stop',[build('flakpos'),steps(2,'work'),command({t:'stop',ids:['worker1']},'stopped'),steps(5,'waited')]);
 record('dead-site-and-missing-assist',[build('flakpos'),steps(1,'work'),{op:'prime',units:{site:{hp:0}}},steps(1,'retired'),{op:'prime',units:{worker1:{follow:77}}},command({t:'assist',ids:['worker1'],id:'site'},'missing','unseen')]);
 const dig=command({t:'dig',ids:['worker1'],kind:'trench',x:25,z:24,dir:Math.PI/2},'dig');
 record('accepted-build-cancels-paid-dig',[dig,build('flakpos'),steps(1,'turn')]);
 record('refused-and-queued-build-retains-paid-dig',[dig,{op:'prime',units:{worker1:{follow:77}}},command({t:'build',kind:'armory',ids:['worker1'],x:33,z:25},'refused','blocked'),build('flakpos',{queue:true,x:31.1,z:24.1},'queued'),steps(2,'waiting'),until('worker1','digEmpty',160,'dig-complete'),steps(1,'assist-assigned'),until('site','progress',80,'assist')]);
 record('dead-worker-excluded-before-late-retirement',[build('hq',{ids:['worker1','worker2']}),{op:'prime',units:{worker2:{hp:0}}},steps(1,'turn')],{sceneKind:'hq',workers:2});
 record('actual-build-retry-drop',[build('flakpos',{x:62,z:24}),steps(1,'drop')],{wall:true});
 record('slope-leveling-stamp',[build('hq'),steps(1,'turn')],{sceneKind:'hq',slope:1});
 record('refused-build-follow-precedence',[{op:'prime',units:{worker1:{follow:77},nonBuilder:{follow:77}},players:{0:{mp:0}}},command({t:'build',kind:'tower',ids:['worker1'],x:31,z:23},'noMoney','mp'),{op:'prime',players:{0:{mp:5000}}},command({t:'build',kind:'tower',ids:['nonBuilder'],x:31,z:23},'noBuilders','noBuilders')]);
 record('actual-needs-tier-and-armory-gates',[command({t:'build',kind:'airfield',ids:['worker1'],x:33,z:25},'needs','needs'),command({t:'build',kind:'motorpool',ids:['worker1'],x:33,z:25},'tier','tier'),{op:'prime',players:{0:{tier:2}}},build('armory')],{tech:true,sceneKind:'armory'});
 record('unseen-masks-blocked-footprint',[command({t:'build',kind:'hq',ids:['worker1'],x:105,z:25},'hidden','notVisible')],{hiddenBlock:true});
 record('occupied-footprint-blocks-payment',[{op:'prime',units:{worker1:{x:32,z:24}}},command({t:'build',kind:'flakpos',ids:['worker1'],x:32,z:24},'occupied','blocked')]);
 record('steep-footprint-blocks-payment',[command({t:'build',kind:'hq',ids:['worker1'],x:33,z:25},'steep','blocked')],{sceneKind:'hq',slope:2});
 const reserve=provisions=>({ammo:0,ammoMax:0,ammoPrice:.35,ammoCredit:0,provisions,fuel:null,emergency:null,warning:null,forced:false,stranded:false,exhausted:false});
 record('finished-repair-budget-and-missing-first-reserve',[build('tower',{ids:['worker1','worker2']}),until('site','finished',400,'complete'),steps(2,'cleared'),{op:'prime',units:{site:{hp:290},worker1:{logistics:reserve(.001)},worker2:{logistics:reserve(.002)}},label:'limited-ready'},command({t:'assist',ids:['worker1','worker2'],id:'site'},'assist'),steps(1,'limited-turn'),{op:'prime',units:{worker1:{logistics:null},worker2:{logistics:reserve(1)}},label:'missing-ready'},steps(1,'missing-turn')],{sceneKind:'tower',workers:2,reserveCase:true});
 record('garrison-build-assist-and-queued-retention',[{op:'prime',units:{worker1:{x:29,z:25,garrison:734}},label:'held'},build('flakpos'),steps(1,'work'),{op:'prime',units:{worker1:{x:29,z:25,garrison:734}}},command({t:'assist',ids:['worker1'],id:'site',queue:true},'queued'),steps(1,'waiting'),command({t:'assist',ids:['worker1'],id:'site'},'assist'),steps(1,'turn')],{house:true});
 record('warm-infantry-and-vehicle-routes-around-paid-stamp',[command({t:'move',orders:[['worker1',40,24],['vehicle',40,24]]},'warm'),steps(1,'walking'),build('flakpos'),command({t:'move',orders:[['vehicle',40,24]]},'reroute'),steps(3,'around'),command({t:'stop',ids:['worker1','vehicle']},'stopped')],{vehicleFactory:true});
 record('retry-two-immediate-assist-resets',[build('flakpos',{x:62,z:24}),steps(1,'first-failure'),command({t:'assist',ids:['worker1'],id:'site'},'reset'),steps(1,'retained'),until('worker1','buildEmpty',25,'drop')],{wall:true,retryLimit:2});
 record('retry-two-queued-assist-retains',[build('flakpos',{x:62,z:24}),steps(1,'first-failure'),command({t:'assist',ids:['worker1'],id:'site',queue:true},'queued'),steps(1,'retained'),until('worker1','buildEmpty',25,'drop')],{wall:true,retryLimit:2});
 record('pillbox-raw-direction-and-target-refusal-follow',[{op:'prime',units:{worker1:{follow:77}}},command({t:'build',kind:'unlisted',ids:['worker1'],x:32,z:24},'unlisted','blocked'),command({t:'assist',ids:['worker1'],id:9999},'unseen','unseen'),build('pillbox',{dir:7},'raw-dir'),steps(2,'turn')],{sceneKind:'pillbox'});
 clean();
 const content=JSON.stringify({reference:pin,node:process.version,arch:process.arch,boundary:'unmodified createGame + command + full step; native construction replay pending',scope:staticKinds,sourceBuildKinds:sim.buildKinds(false,true,false),sourceBuilderTypes:sim.builderTypes(false),future:['depot','shipyard','supplycache','classic','world','site-cancel/refund'],random:{constructor:{nativeMatchSeed:701,stream:'Weather',key:2,bootstrap,value:bits(constructorDraw)},quietMathRandomDraws:0,environment:'source-owned weather and lifecycle seed progression, never reset'},rules:{crewExponent:bits(sim.CFG.classic.crew),repairFraction:bits(sim.CFG.classic.repair),buildReach:bits(sim.CFG.classic.buildReach)},steps:totalSteps,cases})+'\n';
 const target=join(root,'rust/crates/simtest/tests/fixtures/construction-tick.json');if(process.argv.includes('--check'))assert.equal(readFileSync(target,'utf8'),content);else writeFileSync(target,content);
 console.error(`${process.argv.includes('--check')?'Verified':'Recorded'} ${cases.length} construction cases, ${cases.reduce((n,c)=>n+c.records.length,0)} actions, ${totalSteps} full steps, ${totalSnapshots} snapshots`);
}finally{Math.random=originalRandom;clean();}
