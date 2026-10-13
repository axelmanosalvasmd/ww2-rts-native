// Original pattern functions and bounded shared-project command/work phases.
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
const temp=mkdtempSync(join(tmpdir(),'ww2-mass-entrenchment-')),originalRandom=Math.random;
try {
 let source=readFileSync(join(reference,'shared/sim.js'),'utf8');
 const one=(start,end)=>{assert.equal(source.split(start).length,2);const a=source.indexOf(start),b=source.indexOf(end,a);assert.ok(a>=0&&b>a);return source.slice(a,b);};
 const recountBlock=one('  if (g.projects?.size) {\n    for (const p of g.projects.values()) p.active = 0;','\n\n  beginPathTick(g);');
 const mass=one('    if (u.entrench && !u.dig) {','\n\n    // digging: walk');
 const workBlock=one('    if (u.dig) {\n      if (dist(u, u.dig) >','\n\n    // grenade order:');
 const marker='  setupStructures(g, map);';assert.equal(source.split(marker).length,2);
 let code=source.replace(marker,marker+'\n  return g;').replace(/from '(\.\/[^']+)'/g,(_,p)=>`from '${pathToFileURL(join(reference,'shared',p)).href}'`);
 code+=`\nexport const massHooks={joinProject,takeDigJob,startQueuedOrders,beginPathTick,requestStepPath,performStepPath,findPath,cellOf,fortCells,recount(g){${recountBlock}},mass(g,u){${mass}},work(g,u,dt){${workBlock}},setCell};\n`;
 writeFileSync(join(temp,'sim.mjs'),code);const sim=await import(pathToFileURL(join(temp,'sim.mjs'))),H=sim.massHooks,grid=await import(pathToFileURL(join(reference,'shared/grid.js')));
 const buf=new DataView(new ArrayBuffer(8)),bits=v=>{buf.setFloat64(0,v,false);return buf.getBigUint64(0,false).toString(16).padStart(16,'0');};
 const at=v=>({x:bits(v.x),z:bits(v.z)}),job=j=>({kind:j.kind,...at(j),dir:bits(j.dir),front:bits(j.front)});
 const encodeCommand=cmd=>Object.fromEntries(Object.entries(cmd).map(([k,v])=>[k,['x','z','x2','z2','dir'].includes(k)?bits(v):structuredClone(v)]));
 const plans=[],costs=[],traces=[],forts=Object.keys(sim.FORTS).filter(sim.lineFort);
 const geometries=[{a:{x:40,z:40},b:{x:40,z:40},back:{x:20,z:40}},{a:{x:40,z:40},b:{x:41.999,z:40},back:null},{a:{x:40,z:40},b:{x:42,z:40},back:{x:40,z:40.5}},{a:{x:40,z:40},b:{x:64,z:40},back:{x:40,z:20}},{a:{x:40,z:40},b:{x:64,z:40},back:{x:40,z:60}},{a:{x:40,z:40},b:{x:40,z:76},back:{x:39.499,z:40}},{a:{x:-20,z:100},b:{x:200,z:-100},back:{x:-19.5,z:100}},{a:{x:40,z:40},b:{x:640,z:40},back:{x:39.5,z:40}}];
 assert.deepEqual(sim.ENTRENCH_TYPES,['line','zigzag','double','arc','ring','strongpoint']);
 for(const pattern of sim.ENTRENCH_TYPES)for(const fort of ['line','zigzag','double'].includes(pattern)?forts:['trench'])for(const geometry of geometries){
  const rows=sim.entrenchPlan(pattern,geometry.a,geometry.b,geometry.back,fort);
  plans.push({input:{pattern,fort,a:at(geometry.a),b:at(geometry.b),back:geometry.back&&at(geometry.back)},jobs:rows.map(job)});
 }
 for(const fort of forts)for(const cells of [0,1,sim.FORTS[fort].n-1,sim.FORTS[fort].n,sim.FORTS[fort].n+3])costs.push({fort,cells,cost:bits(sim.segmentCost(fort,cells))});
 const blank=()=>({name:'Mass entrenchment',w:40,h:40,rows:Array(40).fill('.'.repeat(40)),spawns:[{x:0,y:0},{x:39,y:39}],points:[{x:20,y:20}]}),U=(id,type='rifle',owner=0,x=25,z=15,extra={})=>({id,type,owner,x,z,...extra});
 const cmd=(ids,extra={})=>({op:'command',slot:0,cmd:{t:'entrench',ids,pattern:'line',x:25,z:25,x2:57,z2:25,...extra}}),joinAction=(slot,ids,id=1,extra={})=>({op:'command',slot,cmd:{t:'entrench',ids,join:id,...extra}}),massAction=ids=>({op:'mass',ids}),work=(id,dt=3)=>({op:'work',id,dt}),money=(slot,mp)=>({op:'player',slot,changes:{mp}}),prime=(id,changes)=>({op:'prime',id,changes}),recount=tick=>({op:'recount',tick});
 function record(name,units,actions,options={}){
  const map=options.map??blank();assert.equal(sim.validateMap(map),null);const initializationRandom=[];Math.random=()=>{initializationRandom.push(.5);return .5;};
  const g=sim.createGame(map,[],false,[],[],{weather:false});g.mode={kind:'conquest'};g.tech=options.tech??false;g.players=[0,1,2].map(slot=>({slot,team:slot===2?0:slot,faction:slot%2,mp:options.mp?.[slot]??100,mun:0,fuel:0,out:false,away:false,tier:1,spawn:{x:3+slot*20,z:3},visible:new Set(),armory:{inf:0,guns:0,armor:0}}));
  g.tick=10;g.seed=123;g.wx={rain:0,wet:0};g.wind={a:0,v:.5};g.cellJournal=[];g.story=g.players.map(()=>({mpSpent:0,built:0}));g.nextId=1000;
  for(const v of units){const d=sim.UNITS[v.type];g.units.set(v.id,{hp:d.models*d.hpPer,supp:0,rot:0,aim:0,owner:0,path:[],orders:[],riding:0,cargo:[],garrison:-1,retreating:false,entrench:null,dig:null,follow:77,face:null,attackId:0,targetId:0,nade:null,enter:-1,board:0,amove:null,fireAt:-1,build:0,repath:0,worldGoal:null,traffic:null,trafficWait:0,drift:false,reverse:null,stuck:0,still:0,cd:0,buff:0,ap:false,sprint:0,cooldown:0,retarget:0,react:0,moveSpeed:0,vx:0,vz:0,auto:false,autoRetreat:false,...structuredClone(v)});}
  for(const p of g.players)p.visible=new Set(g.units.keys());grid.rebuildGrid(g);const retained=new Map(),results=[],snapshots=[];
  const project=p=>({id:p.id,owner:p.owner,fort:p.fort,jobs:p.jobs.map(job),crew:p.crew,active:p.active});
  const descriptor=d=>{if(!d)return null;const v={...at(d),cells:structuredClone(d.cells),t:bits(d.t),kind:d.kind,dir:bits(d.dir),project:d.project?.id??null};if(d.front!==undefined)v.front=bits(d.front);if(d.reach!==undefined)v.reach=bits(d.reach);if(d.n!==undefined)v.n=d.n;return v;};
  const snapshot=()=>({tick:g.tick,projectSeq:g.projectSeq??0,projects:[...(g.projects??new Map()).values()].map(project),retainedProjects:[...retained.values()].map(project),units:[...g.units.values()].map(u=>({id:u.id,hp:bits(u.hp),...at(u),entrench:u.entrench?.id??null,dig:descriptor(u.dig),orders:u.orders.map(encodeCommand),path:u.path.map(at),follow:u.follow??0,garrison:u.garrison,retreating:u.retreating,control:{attackId:u.attackId,targetId:u.targetId,nade:u.nade&&at(u.nade),enter:u.enter,board:u.board,amove:u.amove&&at(u.amove),fireAt:u.fireAt,build:u.build,repath:bits(u.repath),worldGoal:u.worldGoal&&at(u.worldGoal),face:u.face,traffic:u.traffic,trafficWait:bits(u.trafficWait),moveOutcome:u.moveOutcome??null,moveOutcomeTick:u.moveOutcomeTick??null}})),wallets:g.players.map(p=>bits(p.mp)),story:g.story.map(s=>({mpSpent:bits(s.mpSpent),built:bits(s.built)})),chars:g.chars.join(''),journal:[...g.cellJournal],versions:[g.terrainVersion??0,g.obstructionVersion??0,g.infantryRegionVersion??0,g.vehicleRegionVersion??0,g.navalRegionVersion??0],frontBits:g.trenchFront?Array.from(new Uint32Array(g.trenchFront.buffer)).flatMap((v,c)=>Number.isNaN(g.trenchFront[c])?[]:[[c,v]]):[],mines:[...g.mines],mineSeen:[...g.mineSeen],aid:[...g.aid]});
  const save=()=>{for(const p of (g.projects??new Map()).values())retained.set(p.id,p);snapshots.push(snapshot());};save();Math.random=()=>{throw Error('Mass project phase unexpectedly drew unseeded randomness');};
  for(const a of actions){if(a.tick!==undefined)g.tick=a.tick;let result=null;
   if(a.op==='command')result=sim.command(g,a.slot,structuredClone(a.cmd))??null;
   else if(a.op==='join'){const p=g.projects?.get(a.project);assert.ok(p);result=H.joinProject(g,p,a.ids.map(id=>g.units.get(id)),a.queue??false)??null;}
   else if(a.op==='take'){const u=g.units.get(a.id);assert.ok(u.entrench?.jobs.length);H.takeDigJob(g,u);}
   else if(a.op==='mass')for(const id of a.ids){const u=g.units.get(id);if(u.hp>0&&!u.operative&&!u.riding)H.mass(g,u);}
   else if(a.op==='work'){const u=g.units.get(a.id);H.beginPathTick(g);if(u.hp>0&&!u.operative&&!u.riding)H.work(g,u,a.dt);}
   else if(a.op==='pathBegin'){g.pathBudget=a.budget;g.pathRetryLimit=a.retryLimit;H.beginPathTick(g);}
   else if(a.op==='queued')H.startQueuedOrders(g,g.units.get(a.id));
   else if(a.op==='recount')H.recount(g);
   else if(a.op==='prime')Object.assign(g.units.get(a.id),structuredClone(a.changes));
   else if(a.op==='player')Object.assign(g.players[a.slot],a.changes);
   else if(a.op==='atDig'){const u=g.units.get(a.id);assert.ok(u.dig);Object.assign(u,{x:u.dig.x,z:u.dig.z,path:[],repath:0});}
   else if(a.op==='invalidate'){const p=g.projects.get(a.project),j=p.jobs[a.index??0],place=sim.placementCheck(g,{...j,team:g.players[p.owner].team},at=>sim.teamSees(g,g.players[p.owner].team,at));assert.ok(place.ok);for(const [c]of place.cells)H.setCell(g,c,a.ch);}
   else if(a.op==='mutate')H.setCell(g,a.c,a.ch);
   else if(a.op==='remove')g.units.delete(a.id);
   else throw Error(a.op);
   grid.rebuildGrid(g);results.push(result);save();
  }
  if(options.assertPathDrop){assert.equal(g.pathStats.failed,1);assert.equal(g.pathStats.dropped,1);assert.equal(g.pathStats.calls,1);}
  const inputUnits=units.map(u=>Object.fromEntries(Object.entries(u).map(([k,v])=>[k,['x','z','hp','supp','repath','cd'].includes(k)?bits(v):k==='orders'?v.map(encodeCommand):k==='path'?v.map(at):structuredClone(v)])));
  const inputActions=actions.map(a=>{const v=structuredClone(a);if(v.cmd)v.cmd=encodeCommand(v.cmd);if(v.dt!==undefined)v.dt=bits(v.dt);if(v.changes)for(const k of ['x','z','hp','mp','repath'])if(v.changes[k]!==undefined)v.changes[k]=bits(v.changes[k]);return v;});
  traces.push({name,map,mode:'conquest',units:inputUnits,mp:g.players.map((_,slot)=>bits(options.mp?.[slot]??100)),tech:options.tech??false,initializationRandom,actions:inputActions,results,snapshots});
 }
 record('new-project-nearest-window-and-allied-payment',[U(1),U(2,'rifle',2,49,15),U(3,'rifle',1,33,15)],[cmd([1]),joinAction(2,[2]),joinAction(1,[3]),money(2,0),massAction([1,2]),recount(19),money(2,30),massAction([2]),joinAction(0,[1]),recount(20)]);
 record('queued-project-live-reference-and-count-repair',[U(1),U(2,'rifle',0,49,15)],[cmd([1,2],{queue:true}),recount(20),{op:'queued',id:1},recount(21),{op:'queued',id:2},recount(40),massAction([1,2])]);
 record('no-money-wait-keeps-job-and-full-price-admission',[U(1)],[money(0,29),cmd([1]),money(0,30),cmd([1]),money(0,0),massAction([1]),money(0,29),massAction([1]),money(0,30),massAction([1])]);
 record('invalidated-segment-is-dropped-without-payment',[U(1)],[cmd([1]),{op:'invalidate',project:1,ch:'H'},massAction([1]),massAction([1])]);
 record('cancelled-dead-recount-and-reference-retirement',[U(1),U(2,'rifle',0,49,15)],[cmd([1,2]),massAction([1,2]),{op:'command',slot:0,cmd:{t:'stop',ids:[1]}},prime(2,{hp:0}),recount(19),recount(20),{op:'remove',id:2},recount(40),massAction([1])]);
 {const actions=[cmd([1],{x:25,z:25,x2:33,z2:25}),massAction([1]),{op:'atDig',id:1},work(1,2.999),work(1,.001),work(1),work(1),work(1),massAction([1]),recount(20)];record('completion-active-decrement-cleanup-and-built',[U(1)],actions);}
 record('paid-work-cancel-keeps-committed-wallet',[U(1)],[cmd([1]),massAction([1]),{op:'command',slot:0,cmd:{t:'stop',ids:[1]}},recount(19),recount(20)]);
 record('waiting-order-protects-project-then-drops',[U(1),U(2,'rifle',0,49,15)],[cmd([1],{queue:true}),recount(20),{op:'command',slot:0,cmd:{t:'stop',ids:[2]}},recount(40),{op:'command',slot:0,cmd:{t:'stop',ids:[1]}},recount(59),recount(60)]);
 const fullOrders=Array.from({length:8},()=>({t:'move',ids:[1],x:35,z:25}));
 record('full-queue-admission-does-not-create-project',[U(1,'rifle',0,25,15,{orders:fullOrders})],[cmd([1],{queue:true})]);
 record('partial-queue-join-retains-only-accepted-waiter',[U(1,'rifle',0,25,15,{orders:fullOrders}),U(2,'rifle',0,49,15)],[cmd([1,2],{queue:true}),recount(20)]);
 record('command-refusal-boundaries-and-owned-follow-writes',[U(1,'mg'),U(2,'rifle',0,25,15,{retreating:true}),U(3,'rifle',0,49,15,{riding:99}),U(4,'rifle',0,33,15,{operative:1}),U(5,'rifle',1,40,15)],[cmd([1]),cmd([2]),cmd([3,4,5]),cmd([]),cmd([2],{pattern:'wrong'}),cmd([2],{pattern:'arc',fort:'wire'}),joinAction(0,[2],99)]);
 record('unseen-plan-and-clamped-command-coordinates',[U(1,'rifle',0,3,3)],[cmd([1],{x:65,z:65,x2:73,z2:65}),cmd([1],{x:999,z:999,x2:999,z2:999}),cmd([1],{x:-5,z:-5,x2:10,z2:0})]);
 record('mine-lines-replacement-and-sandbag-completion',[U(1,'rifle',0,25,15)],[cmd([1],{fort:'mines'}),cmd([1],{fort:'sandbags'}),massAction([1]),{op:'atDig',id:1},work(1),work(1),work(1),work(1)],{tech:true});
 for(const pattern of ['zigzag','double','arc','ring','strongpoint'])record(`admission:${pattern}`,[U(1,'rifle',0,35,35),U(2,'rifle',0,45,35)],[cmd([1,2],{pattern,x:40,z:40,x2:50,z2:40}),massAction([1,2]),recount(20)]);
 record('actual-join-take-helpers-with-source-preconditions',[U(1),U(2,'rifle',2,49,15)],[cmd([1]),{op:'join',project:1,ids:[2]},recount(20),{op:'take',id:2},{op:'take',id:1}]);
 record('source-wall-tier-admission-boundary',[U(1)],[cmd([1],{fort:'wall'})],{tech:true});
 record('project-identity-sequence-and-unpaid-replacement',[U(1)],[cmd([1]),cmd([1],{pattern:'zigzag'}),recount(20)]);
 record('busy-digger-waits-and-duplicate-join-is-idempotent',[U(1)],[cmd([1,1]),prime(1,{attackId:99}),massAction([1]),prime(1,{attackId:0}),massAction([1,1])]);
 {const actions=[cmd([1,2],{x:25,z:25,x2:41,z2:25}),massAction([1,2]),{op:'atDig',id:1},work(1),work(1),work(1),work(1),massAction([1]),recount(20),{op:'atDig',id:2},work(2),work(2),work(2),work(2),massAction([1,2]),recount(40)];record('completed-digger-waits-for-other-paid-segment',[U(1),U(2,'rifle',0,41,15)],actions);}
 record('edge-segment-admits-and-pays-prorated-cells',[U(1,'rifle',0,3,3)],[cmd([1],{x:0,z:6,x2:0,z2:6}),massAction([1])],{mp:[15,100,100]});
 {const map={name:'Paid segment across water',w:30,h:30,rows:Array(30).fill('.'.repeat(15)+'W'+'.'.repeat(14)),spawns:[{x:0,y:0},{x:29,y:29}],points:[{x:14,y:10}]};record('unreachable-paid-segment-drops-job-keeps-project',[U(1),U(2,'rifle',2,49,15)],[cmd([1],{x:45,z:25,x2:45,z2:25}),massAction([1]),{op:'pathBegin',budget:4096,retryLimit:1},work(1,.05),recount(19),massAction([1]),recount(20)],{map,assertPathDrop:true});}
 const byName=name=>traces.find(t=>t.name===name);
 assert.equal(byName('full-queue-admission-does-not-create-project').results[0],'queueFull');
 assert.equal(byName('no-money-wait-keeps-job-and-full-price-admission').results[1],'mp');
 assert.equal(byName('completion-active-decrement-cleanup-and-built').snapshots.at(-1).story[0].built,bits(1));
 assert.equal(byName('unseen-plan-and-clamped-command-coordinates').results[0],'notVisible');
 assert.equal(byName('source-wall-tier-admission-boundary').results[0],'tier');
 assert.equal(byName('busy-digger-waits-and-duplicate-join-is-idempotent').snapshots[1].projects[0].crew,1);
 assert.equal(byName('new-project-nearest-window-and-allied-payment').snapshots[5].units[0].dig.x,bits(37));
 assert.equal(byName('completed-digger-waits-for-other-paid-segment').snapshots.at(-1).story[0].built,bits(2));
 assert.equal(byName('project-identity-sequence-and-unpaid-replacement').snapshots.at(-1).retainedProjects.length,2);
 assert.equal(byName('edge-segment-admits-and-pays-prorated-cells').results[0],null);
 assert.equal(byName('edge-segment-admits-and-pays-prorated-cells').snapshots.at(-1).wallets[0],bits(0));
 const dropped=byName('unreachable-paid-segment-drops-job-keeps-project');assert.equal(dropped.results[0],null);assert.equal(dropped.snapshots[4].units[0].dig,null);assert.equal(dropped.snapshots[4].units[0].entrench,1);assert.equal(dropped.snapshots[4].projects[0].active,1);assert.equal(dropped.snapshots[4].wallets[0],bits(77));assert.equal(dropped.snapshots[4].story[0].mpSpent,bits(23));assert.equal(dropped.snapshots[7].wallets[0],bits(77));assert.equal(dropped.snapshots[4].units[0].control.moveOutcome,'unreachable');assert.equal(dropped.snapshots[4].units[0].control.repath,bits(1));assert.deepEqual(dropped.snapshots[4].units[0].path,[]);assert.equal(dropped.snapshots[5].projects[0].active,0);assert.equal(dropped.snapshots[6].units[0].entrench,null);assert.equal(dropped.snapshots[7].projects.length,0);
 clean();const content=JSON.stringify({reference:pin,node:process.version,arch:process.arch,scope:'Original pattern/cost functions and shared-project commands, joins, takes, queued starts, recount, mass assignment, dig execution and actual paid-job route failure. No full tick, movement or retirement claim.',plans,costs,traces})+'\n';
 const path=join(root,'rust/crates/core/tests/fixtures/mass-entrenchment.json');if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),content);else writeFileSync(path,content);
 console.error(`${process.argv.includes('--check')?'Verified':'Recorded'} ${plans.length} mass plans, ${costs.length} segment costs, ${traces.length} traces, ${traces.reduce((n,t)=>n+t.actions.length,0)} actions, ${traces.reduce((n,t)=>n+t.snapshots.length,0)} snapshots`);
}finally{Math.random=originalRandom;rmSync(temp,{recursive:true,force:true});clean();}
