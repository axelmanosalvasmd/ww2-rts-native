// Record original recovery helpers and captured late recovery phases without running a full tick.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdtempSync,rmSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {fileURLToPath,pathToFileURL} from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');assert.ok(process.argv.slice(2).every(a=>a==='--check'));
const root=fileURLToPath(new URL('../',import.meta.url)),reference=resolve(process.env.WW2_REFERENCE??join(root,'reference/ww2-rts'));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d',git=args=>execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};clean();
const temp=mkdtempSync(join(tmpdir(),'ww2-match-recovery-')),originalRandom=Math.random;
try {
 let code=readFileSync(join(reference,'shared/sim.js'),'utf8');
 const extract=(start,end)=>{assert.equal(code.split(start).length,2);const at=code.indexOf(start),to=code.indexOf(end,at);assert.ok(to>at);return code.slice(at,to);};
 const prepare=extract('  const bases = isConstructionMode(g) ? list.filter','  if (g.tick % 10 === 0) { healWounded');
 const cadence=extract('  if (g.tick % 10 === 0) { healWounded','\n  if (g.supply &&');
 const reinforcement=extract('  for (const u of list) {\n    if (u.operative) continue;','\n\n  for (const u of list) if (u.hp <= 0) {');
 const marker='  setupStructures(g, map);';assert.equal(code.split(marker).length,2);
 code=code.replace(marker,marker+'\n  return g;').replace(/from '(\.\/[^']+)'/g,(_,p)=>`from '${pathToFileURL(join(reference,'shared',p)).href}'`);
 code+=`\nexport const recoveryHooks={repairBuildingSections,registerBuildingStructure,healWounded,sweepMines,setupConvoys,prepare(g){const list=[...g.units.values()];${prepare}\nreturn {list,bases,depots,tents,tracks,atBase,atAid};},cadence(g,frame,dt){const {list}=frame;${cadence}},reinforce(g,frame,dt){const {list,atBase,atAid}=frame;${reinforcement}}};\n`;
 writeFileSync(join(temp,'sim.mjs'),code);
 const sim=await import(pathToFileURL(join(temp,'sim.mjs'))),H=sim.recoveryHooks,grid=await import(pathToFileURL(join(reference,'shared/grid.js')));
 const cases=[],blank=()=>({name:'Recovery',w:30,h:20,rows:Array(20).fill('.'.repeat(30)),spawns:[{x:0,y:0},{x:29,y:19}],points:[{x:15,y:10}]}),U=(id,type,x=15,z=15,extra={})=>({id,type,x,z,...extra});
 const prime=(id,changes)=>({op:'prime',id,changes}),prepareAction={op:'prepare'},reinforce=(dt,tick)=>({op:'reinforce',dt,...(tick===undefined?{}:{tick})}),medic=(dt,tick)=>({op:'medic',dt,...(tick===undefined?{}:{tick})}),player=(slot,changes)=>({op:'player',slot,changes});
 const runs=a=>{const out=[];for(const v of a){if(out.length&&out.at(-1)[0]===v)out.at(-1)[1]++;else out.push([v,1]);}return out;},bits=a=>runs(new Uint32Array(a.buffer,a.byteOffset,a.length));
 function record(name,units,actions,opts={}){
  const map=opts.map??blank();assert.equal(sim.validateMap(map),null);const initializationRandom=[];Math.random=()=>{initializationRandom.push(.5);return .5;};
  const g=sim.createGame(map,[],false,[],[],{weather:false});g.mode={kind:'conquest'};g.players=[0,1,2].map(slot=>({slot,team:slot===2?0:slot,faction:slot%2,mp:100,mun:0,fuel:0,out:false,spawn:{x:3+slot*20,z:3},visible:new Set(),terrainPending:new Set(),armory:{inf:0,guns:0,armor:0}}));
  g.tick=10;g.seed=123;g.tech=false;g.cellJournal=[];g.watchPending=new Set();g.story=g.players.map(()=>({mpSpent:0}));g.wx={rain:0,wet:0};g.wind={a:0,v:.5};g.logisticsEnabled=opts.logistics===true;if(g.logisticsEnabled)H.setupConvoys(g);
  function insert(v){const def=sim.UNITS[v.type];assert.ok(def,v.type);const u={owner:0,rot:0,hp:def.models*def.hpPer,hpWas:undefined,woundAt:undefined,healAt:undefined,reinf:0,path:[],amove:null,garrison:-1,enter:-1,board:0,riding:0,cargo:[],retreating:false,still:0,xp:0,hitBy:0,hitAt:-1e9,retarget:0,supp:0,built:def.building?1:undefined,moveSpeed:0,...structuredClone(v)};g.units.set(u.id,u);return u;}
  units.forEach(insert);for(const p of g.players)p.visible=new Set(g.units.keys());grid.rebuildGrid(g);let frame=null;
  const snapshot=()=>({tick:g.tick,units:[...g.units.values()].map(u=>({id:u.id,hp:u.hp,reinf:u.reinf,hpWas:u.hpWas??null,woundAt:u.woundAt??null,healAt:u.healAt??null,path:u.path,logistics:u.logistics??null,structureId:u.structureId??null})),wallets:g.players.map(p=>({mp:p.mp})),story:g.story,structures:[...g.structures.values()],cellHp:bits(g.cellHp),flags:runs(g.flags),objects:g.objects.join(''),chars:g.chars.join(''),wear:bits(g.wear),burnt:runs(g.burnt),cellLog:g.cellLog,newCells:g.newCells,journal:g.cellJournal,pending:g.players.map(p=>[...p.terrainPending]),watchPending:[...g.watchPending],versions:[g.terrainVersion??0,g.obstructionVersion??0,g.infantryRegionVersion??0,g.vehicleRegionVersion??0,g.navalRegionVersion??0],mineSeen:[...g.mineSeen],mines:[...g.mines],aid:[...g.aid],points:g.points,shots:g.shots,fallingSections:g.fallingSections,seed:g.seed>>>0,stores:g.convoys?[...g.convoys.stores.values()].map(s=>({...s,buckets:[...s.buckets]})):null,frame:frame?{list:frame.list.map(u=>u.id),bases:frame.bases?.map(u=>u.id)??null,depots:frame.depots.map(q=>g.points.indexOf(q)),tents:frame.tents,tracks:frame.tracks.map(u=>u.id)}:null});
  const results=[],snapshots=[structuredClone(snapshot())];
  Math.random=()=>{throw Error('Recovery helper unexpectedly drew unseeded randomness');};
  for(const a of actions){g.shots=[];if(a.tick!==undefined)g.tick=a.tick;let result=null;
   if(a.op==='prepare')frame=H.prepare(g);
   else if(a.op==='cadence'){assert.ok(frame);H.cadence(g,frame,a.dt);}
   else if(a.op==='reinforce'){assert.ok(frame);H.reinforce(g,frame,a.dt);}
   else if(a.op==='medic')H.healWounded(g,a.ids?a.ids.map(id=>g.units.get(id)):[...g.units.values()],a.dt);
   else if(a.op==='sweep')H.sweepMines(g,a.ids?a.ids.map(id=>g.units.get(id)):[...g.units.values()]);
   else if(a.op==='repair')result=H.repairBuildingSections(g,g.units.get(a.id),a.amount);
   else if(a.op==='building'){const u=g.units.get(a.id);u.cells=a.cells;u.hp=a.hp;for(const c of a.cells)sim.mutateWorldCell(g,c,{object:'K'});H.registerBuildingStructure(g,u);}
   else if(a.op==='damage')result=sim.damageWorldSection(g,a.c,a.amount,{owner:a.owner??1,contactId:a.contact??'recovery:'+results.length});
   else if(a.op==='mutate')sim.mutateWorldCell(g,a.c,a.change);
   else if(a.op==='section')Object.assign(g.structuralCells.get(a.c),a.changes);
   else if(a.op==='prime')Object.assign(g.units.get(a.id),structuredClone(a.changes));
   else if(a.op==='player')Object.assign(g.players[a.slot],a.changes);
   else if(a.op==='point'){if(a.index===undefined)g.points.push({...a.value});else Object.assign(g.points[a.index],a.changes);}
   else if(a.op==='add')insert(a.unit);
   else if(a.op==='aid')g.aid.set(a.c,a.owner);
   else if(a.op==='fire'){if(a.value===null)g.fires.delete(a.c);else g.fires.set(a.c,a.value);}
   else if(a.op==='mineOwner')g.mines.set(a.c,a.owner);
   else if(a.op==='convoys'){g.logisticsEnabled=true;H.setupConvoys(g);}
   else if(a.op==='store')g.convoys.stores.set(a.id,{...a.value,id:a.id,buckets:new Map(a.buckets??[])});
   else throw Error(a.op);
   grid.rebuildGrid(g);results.push(result);snapshots.push(structuredClone(snapshot()));
  }
  cases.push({name,map,units,logistics:opts.logistics===true,initializationRandom,actions,results,snapshots});
 }
 const reserve=provisions=>({ammo:10,ammoMax:10,provisions,fuel:null,emergency:null,exhausted:false,forced:false,stranded:false});
 record('repair-fractional-f32-full-and-history',[U(1,'hq')],[{op:'building',id:1,cells:[62,63,64],hp:2000},{op:'repair',id:1,amount:.1},{op:'damage',c:62,amount:666.8},{op:'repair',id:1,amount:200},{op:'repair',id:1,amount:5000},{op:'repair',id:1,amount:5}]);
 record('failed-anchor-occupancy-object-and-fire',[U(1,'hq'),U(2,'tank',5,5)],[{op:'building',id:1,cells:[62,63,64],hp:1500},{op:'damage',c:62,amount:600},{op:'repair',id:1,amount:100},{op:'mutate',c:62,change:{object:'Y'}},prime(2,{x:50,z:30}),{op:'repair',id:1,amount:100},{op:'mutate',c:62,change:{object:'.'}},{op:'fire',c:62,value:5},{op:'repair',id:1,amount:100},{op:'fire',c:62,value:5},{op:'repair',id:1,amount:100},{op:'fire',c:62,value:null},{op:'repair',id:1,amount:5000}]);
 record('failed-support-captured-once-per-repair',[U(1,'hq')],[{op:'building',id:1,cells:[62,63,64],hp:1500},{op:'damage',c:63,amount:600},{op:'damage',c:64,amount:600},{op:'section',c:63,changes:{anchor:false}},{op:'section',c:64,changes:{anchor:false}},{op:'repair',id:1,amount:1500},{op:'repair',id:1,amount:1500}]);
 record('repair-no-structure-and-dead',[U(1,'hq',15,15,{hp:2500}),U(2,'hq',25,15,{hp:0})],[{op:'repair',id:1,amount:125.25},{op:'repair',id:1,amount:1000},{op:'repair',id:2,amount:1000}]);
 record('medic-grid-bucket-tie-and-wound-history',[U(1,'medic',15,15),U(3,'rifle',15,21,{hp:40}),U(2,'rifle',21,15,{hp:40})],[medic(.5),prime(2,{hp:30}),medic(.5),medic(.5,110),medic(20,111)]);
 record('medic-hot-calm-exact-boundary',[U(1,'medic'),U(2,'rifle',20,15,{hp:40,hpWas:50})],[medic(1,10),medic(1,109),medic(1,110)]);
 record('medic-provision-and-shortage-limits',[U(1,'medic',15,15,{logistics:reserve(.1)}),U(2,'rifle',20,15,{hp:40,logistics:reserve(29)})],[medic(1),medic(1),prime(1,{logistics:reserve(120)}),prime(2,{logistics:{...reserve(120),exhausted:true}}),medic(1)]);
 record('medic-idle-pathseek-and-exclusions',[U(1,'medic',15,15),U(2,'rifle',39,15,{hp:40}),U(3,'rifle',20,15,{hp:1,riding:9}),U(4,'rifle',20,16,{hp:1,operative:1}),U(5,'rifle',20,17,{hp:1,owner:1}),U(6,'rifle',20,18,{hp:1,retreating:true})],[medic(.5),prime(1,{path:[],amove:{x:40,z:15}}),medic(.5),prime(1,{amove:null,retreating:true}),medic(.5)]);
 record('conquest-own-hq-only-not-allied-or-spawn',[U(1,'rifle',3,3,{hp:40,reinf:1}),U(2,'hq',20,20,{owner:2}),U(3,'rifle',20,20,{hp:40}),U(4,'hq',45,25,{owner:0}),U(5,'rifle',45,25,{hp:40})],[prepareAction,reinforce(2)]);
 record('infantry-timer-modelheal-wallet-and-reset',[U(1,'hq'),U(2,'rifle',20,15,{hp:41})],[prepareAction,reinforce(1),reinforce(1),prime(2,{hp:61,reinf:1.9}),player(0,{mp:0}),reinforce(.1),reinforce(1),player(0,{mp:100}),reinforce(0),prime(2,{x:55,z:35,reinf:7}),reinforce(1)]);
 record('vehicle-rate-wallet-provisions-and-insufficient-wallet',[U(1,'hq'),U(2,'tank',20,15,{hp:300,reinf:.2,logistics:reserve(120)})],[prepareAction,reinforce(.5),player(0,{mp:0}),reinforce(.5),player(0,{mp:100}),prime(2,{logistics:reserve(.5)}),reinforce(1),prime(2,{logistics:reserve(120)}),reinforce(5)],{logistics:true});
 record('infantry-provision-failure-accumulates-timer',[U(1,'hq'),U(2,'rifle',20,15,{hp:40,reinf:1.9,logistics:reserve(4)})],[prepareAction,reinforce(.2),reinforce(1),prime(2,{logistics:reserve(120)}),reinforce(0)],{logistics:true});
 record('captured-depot-membership-survives-cut-and-birth-excluded',[U(1,'rifle',35,25,{hp:40})],[{op:'point',value:{kind:'depot',x:35,z:25,owner:2,cut:false}},prepareAction,{op:'point',index:1,changes:{cut:true}},{op:'add',unit:U(2,'rifle',35,25,{hp:40})},reinforce(2),prepareAction,reinforce(2)]);
 record('captured-hq-reference-live-and-new-base-excluded',[U(1,'rifle',35,25,{hp:40})],[prepareAction,{op:'add',unit:U(2,'hq',35,25)},reinforce(2),prepareAction,prime(2,{hp:0}),reinforce(2)]);
 record('field-hospital-and-halted-halftrack-aid',[U(1,'rifle',15,15,{hp:40}),U(2,'rifle',40,25,{hp:40,riding:3}),U(3,'halftrack',40,25,{owner:2,still:2}),U(4,'tank',17,15,{hp:300})],[{op:'aid',c:217,owner:2},prepareAction,reinforce(2),reinforce(2),prime(3,{still:0}),reinforce(4),prepareAction,reinforce(4)]);
 record('reinforcement-shortage-dead-operative-and-full-reset',[U(1,'hq'),U(2,'rifle',20,15,{hp:40,logistics:reserve(29)}),U(3,'rifle',20,16,{hp:0,reinf:4}),U(4,'rifle',20,17,{hp:40,reinf:4,operative:1}),U(5,'rifle',20,18,{hp:100,reinf:4})],[prepareAction,reinforce(2),reinforce(2),prime(2,{logistics:{...reserve(120),exhausted:true}}),reinforce(5)]);
 {const map=blank();map.rows[7]=map.rows[7].slice(0,10)+'N'+map.rows[7].slice(11);map.rows[8]=map.rows[8].slice(0,10)+'N'+map.rows[8].slice(11);record('mine-team-mask-stationary-and-source-cadence',[U(1,'rifle',15,15,{still:1.999}),U(2,'engineer',21,17,{owner:1,still:0}),U(3,'rifle',21,17,{owner:2,still:2})],[{op:'mineOwner',c:250,owner:2},prepareAction,{op:'cadence',dt:.05,tick:9},{op:'cadence',dt:.05,tick:10},prime(1,{still:2}),{op:'sweep',ids:[1]},prime(2,{riding:9}),{op:'sweep',ids:[2]}],{map});}
 record('source-store-provision-before-local-stock',[U(1,'hq'),U(2,'rifle',20,15,{hp:40,logistics:reserve(120)})],[{op:'store',id:'cache',value:{owner:0,x:21,z:15,active:true,source:false},buckets:[[0,{ammo:0,provisions:5,fuel:0}]]},prepareAction,reinforce(2),prime(2,{hp:40}),reinforce(2)],{logistics:true});
 record('home-exact-radius-and-outside',[U(1,'hq'),U(2,'rifle',30,15,{hp:40}),U(3,'rifle',30.001,15,{hp:40,reinf:7})],[prepareAction,reinforce(2)]);
 record('medic-near-seek-and-hurt-boundaries',[U(1,'medic'),U(2,'rifle',25,15,{hp:40}),U(3,'rifle',45,15,{hp:89})],[medic(1),prime(2,{hp:100}),medic(1),prime(1,{path:[]}),prime(3,{x:45.001,z:15}),medic(1),prime(3,{x:45,z:15,hp:90}),medic(1)]);
 record('vehicle-half-recovery-rate',[U(1,'hq'),U(2,'tank',20,15,{hp:300,logistics:reserve(29)})],[prepareAction,reinforce(1)],{logistics:true});
 {const map=blank();map.rows[7]=map.rows[7].slice(0,10)+'NN'+map.rows[7].slice(12);record('mine-exact-distance-and-builder-exclusions',[U(1,'engineer',15,15),U(2,'rifle',21,15,{owner:1,still:1.999}),U(3,'mg',21,15,{owner:2,still:5}),U(4,'engineer',21,15,{owner:1,hp:0})],[{op:'sweep',ids:[1,2,3,4]},prime(2,{still:2}),{op:'sweep',ids:[2]}],{map});}
 const find=name=>cases.find(c=>c.name===name),last=name=>find(name).snapshots.at(-1),unit=(snapshot,id)=>snapshot.units.find(u=>u.id===id);
 assert.equal(last('repair-fractional-f32-full-and-history').units[0].hp,3000);
 assert.equal(last('repair-fractional-f32-full-and-history').structures[0].sections[0].failedAt,.5);
 assert.equal(find('failed-support-captured-once-per-repair').snapshots.at(-2).structures[0].sections[2].state,'failed');
 assert.equal(last('failed-support-captured-once-per-repair').structures[0].sections[2].state,'intact');
 assert.equal(unit(find('medic-grid-bucket-tie-and-wound-history').snapshots[1],2).hp,42.5);
 assert.equal(unit(find('medic-grid-bucket-tie-and-wound-history').snapshots[1],3).hp,40);
 assert.equal(unit(last('medic-hot-calm-exact-boundary'),2).hp,50);
 assert.equal(unit(last('medic-grid-bucket-tie-and-wound-history'),2).hp,100);
 assert.equal(unit(last('captured-depot-membership-survives-cut-and-birth-excluded'),1).hp,60);
 assert.equal(unit(last('captured-depot-membership-survives-cut-and-birth-excluded'),2).hp,40);
 assert.equal(unit(last('home-exact-radius-and-outside'),2).hp,60);
 assert.equal(unit(last('home-exact-radius-and-outside'),3).hp,40);
 assert.equal(unit(last('vehicle-half-recovery-rate'),2).hp,309);
 assert.equal(last('vehicle-half-recovery-rate').wallets[0].mp,95.5);
 clean();const content=JSON.stringify({reference:pin,node:process.version,arch:process.arch,scope:'Original repair, medic and mine helpers; captured late preparation, cadence and reinforcement. No construction scheduler, World recovery or full tick.',cases})+'\n';
 const path=join(root,'rust/crates/core/tests/fixtures/match-recovery.json');if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),content);else writeFileSync(path,content);
 console.error(`${process.argv.includes('--check')?'Verified':'Recorded'} ${cases.length} match recovery traces, ${cases.reduce((n,c)=>n+c.actions.length,0)} actions, ${cases.reduce((n,c)=>n+c.snapshots.length,0)} snapshots`);
}finally{Math.random=originalRandom;rmSync(temp,{recursive:true,force:true});clean();}
