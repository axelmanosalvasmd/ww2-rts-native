// Full creation, command and step oracle. The pinned simulation is imported unchanged.
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
const {rebuildGrid}=await import(pathToFileURL(join(reference,'shared/grid.js')));
const view=new DataView(new ArrayBuffer(8));
const bits=v=>{view.setFloat64(0,v,false);return view.getBigUint64(0,false).toString(16).padStart(16,'0');};
const encode=v=>v===undefined?null:typeof v==='number'?{f64:bits(v)}:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;
const number=hex=>{view.setBigUint64(0,BigInt('0x'+hex),false);return view.getFloat64(0,false);};
const point=p=>p?{x:bits(p.x),z:bits(p.z)}:null;
const job=j=>({kind:j.kind,...point(j),dir:bits(j.dir),front:j.front===undefined?null:bits(j.front)});
const project=p=>({id:p.id,owner:p.owner,fort:p.fort,jobs:p.jobs.map(job),crew:p.crew,active:p.active});
const paid=d=>d?{...point(d),cells:structuredClone(d.cells),t:bits(d.t),dir:bits(d.dir),front:d.front===undefined?null:bits(d.front),kind:d.kind,reach:d.reach===undefined?null:bits(d.reach),n:d.n??null,waits:d.waits??null,project:d.project?.id??null}:null;
const blank=wall=>({name:'Full-step shared field projects',w:30,h:30,rows:Array(30).fill(wall?'.'.repeat(15)+'W'+'.'.repeat(14):'.'.repeat(30)),spawns:[{x:1,y:1},{x:28,y:28}],points:[{x:12,y:27}]});
const originalRandom=Math.random;
const cases=[];
let totalSteps=0;
try {
 function record(name,actions,{wall=false,crew=2}={}){
  const draws=[];Math.random=()=>{draws.push(.5);return .5;};
  const map=blank(wall);assert.equal(sim.validateMap(map),null);
  const opts={weather:false,tech:false,supply:false,logistics:false};
  const g=sim.createGame(map,['Crew','Opposition'],false,[0,1],[0,1],opts);
  assert.equal(g.mode,undefined);assert.equal(g.wx,null);assert.equal(g.supply,false);assert.equal(g.logisticsEnabled,false);
  const creationDraws=draws.length;
  const initialActors=[...g.units.keys()];
  const aliases={};
  for(let i=0;i<crew;i++){const u=sim.internals.spawnUnit(g,0,'engineer');aliases[`worker${i+1}`]=u.id;Object.assign(u,{x:25+i*2,z:25});}
  const observer=[...g.units.values()].find(u=>u.owner===0&&u.type==='rifle');assert.ok(observer);aliases.observer=observer.id;Object.assign(observer,{x:49,z:15});
  for(const u of g.units.values())Object.assign(u,{holdFire:true,holdPos:true,auto:false,autoRetreat:false});
  g.players[0].mp=1000;g.pathRetryLimit=1;g.cellJournal=[];
  rebuildGrid(g);sim.internals.updateVision(g);
  const setup={map,names:['Crew','Opposition'],shuffle:false,factions:[0,1],teams:[0,1],opts,initialActors,aliases,spawnCrew:{type:'engineer',owner:0,count:crew,positions:Array.from({length:crew},(_,i)=>({x:25+i*2,z:25}))},prime:{observer:{x:49,z:15},allUnits:{holdFire:true,holdPos:true,auto:false,autoRetreat:false},mp:{slot:0,value:1000},pathRetryLimit:1,cellJournal:[]},creationRandomDraws:creationDraws,setupRandomDraws:draws.length-creationDraws};
  const retained=new Map(),states=[],records=[];
  const observe=()=>{
   for(const p of g.projects?.values()??[])retained.set(p.id,p);
   for(const u of g.units.values()){if(u.entrench)retained.set(u.entrench.id,u.entrench);if(u.dig?.project)retained.set(u.dig.project.id,u.dig.project);}
   const touched=new Set(g.cellJournal??[]);for(const [c]of g.mines)touched.add(c);if(g.trenchFront)for(let c=0;c<g.trenchFront.length;c++)if(Number.isFinite(g.trenchFront[c]))touched.add(c);
   const snapshot={tick:g.tick,actorOrder:[...g.units.keys()],projectSeq:g.projectSeq??0,listed:[...(g.projects?.values()??[])].map(project),retained:[...retained.values()].map(p=>({...project(p),listed:g.projects?.get(p.id)===p,memberIds:[...g.units.values()].filter(u=>u.entrench===p).map(u=>u.id),paidIds:[...g.units.values()].filter(u=>u.dig?.project===p).map(u=>u.id),queuedJoinIds:[...g.units.values()].filter(u=>u.orders?.some(o=>o.join===p.id)).map(u=>u.id)})),units:[...g.units.values()].map(u=>({id:u.id,owner:u.owner,type:u.type,hp:bits(u.hp),...point(u),rot:bits(u.rot),aim:bits(u.aim),dig:paid(u.dig),entrench:u.entrench?.id??null,orders:encode(u.orders),path:u.path.map(point),controls:encode(Object.fromEntries(['follow','face','garrison','riding','retreating','attackId','targetId','enter','board','amove','fireAt','build','worldGoal','moveOutcome','moveOutcomeTick','moveResult','holdFire','holdPos','auto','autoRetreat'].map(k=>[k,u[k]]))),timers:encode(Object.fromEntries(['repath','cooldown','retarget','cd','buff','sprint','react','still','stuck','supp','moveSpeed','vx','vz','trafficWait'].map(k=>[k,u[k]])))})),players:g.players.map(p=>({slot:p.slot,team:p.team,mp:bits(p.mp),inc:bits(p.inc),mun:p.mun===undefined?null:bits(p.mun),fuel:p.fuel===undefined?null:bits(p.fuel),vp:bits(p.vp),out:p.out??false,away:p.away??false})),story:encode(g.story),timeline:encode(g.timeline),terrain:{cells:[...touched].sort((a,b)=>a-b).map(c=>({c,ch:g.chars[c],ground:g.ground[c],object:g.objects[c],mine:g.mineLayer[c],flags:g.flags[c],height:g.height?.[c]??0,hp:bits(g.cellHp[c]),wear:bits(g.wear[c]),surfaceWear:bits(g.surfaceWear[c]),scar:bits(g.scar[c]),burnt:g.burnt[c],state:g.cellState[c],frontBits:g.trenchFront?new Uint32Array(g.trenchFront.buffer)[c]:null})),journal:[...(g.cellJournal??[])],newCells:encode(g.newCells),cellLog:encode(g.cellLog),versions:[g.terrainVersion??0,g.obstructionVersion??0,g.infantryRegionVersion??0,g.vehicleRegionVersion??0,g.navalRegionVersion??0],mines:[...g.mines],mineSeen:[...g.mineSeen],aid:[...g.aid]},seed:g.seed>>>0,nextId:g.nextId,winner:g.winner,shots:encode(g.shots),pathStats:encode(g.pathStats),randomDraws:draws.length};
   states.push(snapshot);return snapshot;
  };
  observe();
  for(const input of actions){const a=structuredClone(input),before=draws.length;let result=null;
   if(a.op==='command'){a.cmd.ids=a.cmd.ids.map(id=>typeof id==='string'?aliases[id]:id);result=sim.command(g,a.slot??0,a.cmd)??null;observe();}
   else if(a.op==='prime'){if(a.tick!==undefined)g.tick=a.tick;for(const [id,changes]of Object.entries(a.units??{}))Object.assign(g.units.get(aliases[id]??Number(id)),changes);rebuildGrid(g);sim.internals.updateVision(g);observe();}
   else if(a.op==='steps'){for(let i=0;i<a.count;i++){const old=draws.length;sim.step(g);assert.equal(draws.length,old,`${name}: quiet step drew Math.random`);totalSteps++;observe();}}
   else throw Error(a.op);
   records.push({action:a,result,randomDraws:draws.length-before,lastSnapshot:states.length-1});
  }
  const find=(s,id)=>s.units.find(u=>u.id===aliases[id]);
  const spend=s=>s.story?.[0]?.mpSpent?.f64??null;
  const payment=s=>number(spend(s));
  const assertIncome=(a,b)=>{assert.equal(payment(a),payment(b));assert.equal(number(b.players[0].mp),number(a.players[0].mp)+number(b.players[0].inc)*sim.TICK);};
  if(name==='allocation-payment-once'){assert.equal(states[1].listed[0].active,0);assert.equal(states[1].listed[0].crew,2);assert.equal(find(states[2],'worker1').dig.project,1);assert.equal(find(states[2],'worker2').dig.project,1);assert.equal(states[2].listed[0].active,2);assert.equal(spend(states[2]),spend(states.at(-1)));for(let i=3;i<states.length;i++)assertIncome(states[i-1],states[i]);assert.ok(payment(states[2])>0);assert.notDeepEqual(find(states[2],'worker1').dig.cells,find(states[2],'worker2').dig.cells);}
  if(name==='immediate-join-before-sweep'){assert.equal(states[2].listed[0].crew,1);assert.equal(states[3].listed[0].crew,2);assert.equal(states[3].listed[0].active,0);assert.equal(states[3].retained[0].memberIds.length,2);const sweep=states.find(s=>s.tick===20);assert.equal(sweep.listed[0].crew,2);assert.equal(sweep.listed[0].active,2);}
  if(name==='dead-paid-worker-late-retirement'){const pre=states.at(-3),after=states.at(-2),next=states.at(-1);assert.ok(find(pre,'worker1').dig);assert.equal(find(pre,'worker1').hp,bits(0));assert.equal(find(after,'worker1'),undefined);assert.equal(after.retained[0].crew,2);assert.equal(after.retained[0].active,1);assert.equal(next.retained[0].active,1);assert.equal(after.actorOrder.length,pre.actorOrder.length-1);assert.equal(after.story[0].losses.f64,bits(1));assertIncome(after,next);}
  if(name==='accepted-stop-no-refund'){assert.equal(records[2].result,null);assert.equal(find(states[3],'worker1').dig,null);assert.equal(find(states[3],'worker1').entrench,null);assert.equal(spend(states[2]),spend(states.at(-1)));assert.equal(states[3].players[0].mp,states[2].players[0].mp);assert.equal(states.at(-1).listed.length,0);}
  if(name==='completion-clears-on-next-turn'){const done=states.find(s=>s.story?.[0]?.built?.f64===bits(1));assert.ok(done);assert.equal(find(done,'worker1').dig,null);assert.equal(find(done,'worker1').entrench,1);const next=states[states.indexOf(done)+1];assert.equal(find(next,'worker1').entrench,null);assert.ok(states.at(-1).listed.length===0);assert.equal(states.at(-1).terrain.journal.length,4);assert.equal(states.at(-1).terrain.cells.length,4);assert.ok(states.at(-1).terrain.cells.every(c=>c.ch==='T'&&c.frontBits!==null));}
  if(name==='water-wall-paid-route-drop'){const last=states.at(-1);assert.equal(find(last,'worker1').dig,null);assert.equal(find(last,'worker1').entrench,1);assert.equal(find(last,'worker1').controls.moveOutcome,'unreachable');assert.ok(g.pathStats.dropped>0);assert.equal(g.story[0].mpSpent>0,true);}
  if(name==='joined-and-queued-sweep'){const sweep=states.find(s=>s.tick===20);assert.ok(sweep);assert.equal(sweep.retained[0].queuedJoinIds.length,1);assert.equal(sweep.listed[0].crew,1);assert.equal(sweep.retained[0].memberIds.length,1);}
  assert.ok(records.every(r=>r.result===null),`${name}: command refusal`);
  assert.ok(records.every(r=>r.randomDraws===0),`${name}: quiet action drew Math.random`);
  cases.push({name,setup,records,states});
 }
 const entrench=(ids,extra={})=>({op:'command',cmd:{t:'entrench',ids,pattern:'line',x:25,z:25,x2:41,z2:25,...extra}});
 record('allocation-payment-once',[entrench(['worker1','worker2']),{op:'steps',count:4}]);
 record('immediate-join-before-sweep',[{op:'prime',tick:18},entrench(['worker1']),{op:'command',cmd:{t:'entrench',ids:['worker2'],join:1}},{op:'steps',count:2}]);
 record('dead-paid-worker-late-retirement',[{op:'prime',tick:18},entrench(['worker1','worker2']),{op:'steps',count:1},{op:'prime',units:{worker1:{hp:0}}},{op:'steps',count:1},{op:'steps',count:1}]);
 record('accepted-stop-no-refund',[entrench(['worker1'],{x2:25}),{op:'steps',count:1},{op:'command',cmd:{t:'stop',ids:['worker1']}},{op:'steps',count:20}],{crew:1});
 record('completion-clears-on-next-turn',[entrench(['worker1'],{x2:25}),{op:'steps',count:160}],{crew:1});
 record('water-wall-paid-route-drop',[entrench(['worker1'],{x:45,z:25,x2:45,z2:25}),{op:'steps',count:1}],{wall:true,crew:1});
 record('joined-and-queued-sweep',[{op:'prime',tick:18},entrench(['worker1']),{op:'command',cmd:{t:'dig',ids:['worker2'],kind:'trench',x:27,z:35,dir:0}},{op:'command',cmd:{t:'entrench',ids:['worker2'],join:1,queue:true}},{op:'steps',count:2}]);
 // Store complete initial states and ordered path patches, retaining every actual step snapshot.
 const changes=(a,b,path=[],out=[])=>{if(Object.is(a,b))return out;if(Array.isArray(a)&&Array.isArray(b)&&a.length===b.length){for(let i=0;i<a.length;i++)changes(a[i],b[i],[...path,i],out);return out;}if(a&&b&&typeof a==='object'&&typeof b==='object'&&!Array.isArray(a)&&!Array.isArray(b)){for(const k of new Set([...Object.keys(a),...Object.keys(b)]))if(!Object.hasOwn(b,k))out.push([[...path,k]]);else changes(a[k],b[k],[...path,k],out);return out;}out.push([path,b]);return out;};
 for(const c of cases){c.initial=c.states[0];c.patches=c.states.slice(1).map((s,i)=>changes(c.states[i],s));c.snapshots=c.states.length;delete c.states;}
 clean();const content=JSON.stringify({reference:pin,node:process.version,arch:process.arch,boundary:'unmodified createGame + command + full step; native full-tick consumer pending',random:{value:bits(.5),quietActionDraws:0},steps:totalSteps,cases})+'\n';
 const target=join(root,'rust/crates/core/tests/fixtures/mass-project-tick.json');if(process.argv.includes('--check'))assert.equal(readFileSync(target,'utf8'),content);else writeFileSync(target,content);
 console.error(`${process.argv.includes('--check')?'Verified':'Recorded'} ${cases.length} full-step mass project cases, ${totalSteps} steps, ${cases.reduce((n,c)=>n+c.snapshots,0)} snapshots`);
}finally{Math.random=originalRandom;clean();}
