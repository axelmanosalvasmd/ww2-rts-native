// Record actual cached snapshotFor fields, without rewriting the projection source.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');clean();
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const {rebuildGrid}=await import(pathToFileURL(resolve(reference,'shared/grid.js')));
const rows=Array(20).fill('.'.repeat(20));
rows[5]=rows[5].slice(0,5)+'T'+rows[5].slice(6);
rows[7]=rows[7].slice(0,7)+'#'+rows[7].slice(8);
rows[9]=rows[9].slice(0,9)+'B'+rows[9].slice(10);
const map={name:'Snapshot fixture',w:20,h:20,rows,spawns:[{x:1,y:1},{x:18,y:18},{x:1,y:18}],points:[{x:10,y:10}]};
assert.ok(process.argv.slice(2).every(arg=>arg==='--check'));
const cases=[];
const publicState=(g,army='standard')=>({army,tech:g.tech,wx:g.wx,wind:g.wind,support:g.players.map(p=>p.sup),strikes:g.strikes,covers:g.covers??[],points:g.points.map(q=>({owner:q.owner,capper:q.capper,progress:q.progress,cut:q.cut,needs:q.needs,contested:q.contested,onPoint:q.onPoint})),smokes:g.smokes,fires:[...g.fires]});
const fields=['tick','units','mp','mun','fuel','inc','fuelInc','upkeep','out','vp','winner','end','rally','tech','weather','plans','orders','queues','productionJobs','army','wx','sup','points','smokes','fires','movement','strikes','covers','works'];
const fullFor=g=>{const cache=sim.snapshotCache(g);return g.players.map((_,slot)=>{const result=sim.snapshotFor(g,slot,[],[],cache);result.cells=sim.terrainFor(g,slot,true);result.fog=sim.fogFor(g,slot,true);return JSON.parse(JSON.stringify(result));});};
for(const kind of ['rifle','sniper','tank','halftrack','engineer','bunker']) {
 for(const placement of ['open','trench','cover','near-corner','near-vehicle','near-wrap','deleted-target','finished']) {
  if(placement==='finished' && kind!=='rifle')continue;
  const prior=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(map,['A','Ally','Enemy'],false,[0,0,1],[0,1,2],{mode:'conquest',weather:'clear',tech:true,logistics:false});}finally{Math.random=prior;}
  g.units.clear();g.nextId=1;g.tick=125;
  const unit=sim.internals.spawnUnit(g,0,kind),friend=sim.internals.spawnUnit(g,1,'rifle'),enemy=sim.internals.spawnUnit(g,2,'rifle');
  const positions={open:[6.05,6.15],trench:[11,11],cover:[15,15],'near-corner':[17,17],'near-vehicle':[25,25],'near-wrap':[39,8],'deleted-target':[6.05,6.15],'finished':[6.05,6.15]};
  const [x,z]=positions[placement];
  Object.assign(unit,{x,z,rot:-.05,aim:2.05,hp:unit.hp-0.3,supp:12.5,cd:2.1,cooldown:9.5,targetId:3,still:4,shotAt:40,dust:120,buff:2,ap:true,reinf:.2,healAt:124,retreating:true,holdFire:true,holdPos:true,autoRetreat:true,auto:true,xp:sim.UNITS[kind].cost*2.8,built:.8,moveSpeed:-.05,vx:1.05,vz:-1.05,travelDir:-2.05});
  Object.assign(friend,{x:30,z:30,rot:0,aim:0,targetId:1,cd:-.2,holdFire:true,auto:true});
  Object.assign(enemy,{x:35,z:35,rot:0,aim:0,targetId:1,cd:4.1,buff:1,auto:true});
  if(kind==='halftrack')unit.cargo=[friend.id];
  if(placement==='near-vehicle'){const heavy=sim.internals.spawnUnit(g,2,'tank');Object.assign(heavy,{x:x+2,z:z+1,cd:0,aim:0});}
  if(placement==='near-wrap')g.chars[Math.floor(z/2)*g.w+20]='#';
  g.players[0].visible=new Set([3]);g.players[1].visible=new Set([]);g.players[2].visible=new Set([1]);
  g.players.forEach((p,i)=>{Object.assign(p,{mp:500.95+i,inc:8.15+i,vp:12.9+i,rally:i===0?{x:-.05,z:6.15}:null});if(i===0){p.fuelInc=3.15;p.upkeep=2.15;}});
  if(placement==='deleted-target')g.units.delete(3);
  if(placement==='finished'){g.players[0].vp=g.winVp;sim.finish(g,0,'vp');}
  const inputs=[...g.units.values()].map(u=>Object.fromEntries(['id','type','owner','x','z','rot','aim','hp','supp','cd','cooldown','targetId','still','shotAt','dust','buff','ap','reinf','healAt','retreating','holdFire','holdPos','autoRetreat','auto','xp','built','moveSpeed','vx','vz','travelDir','cargo','riding','garrison','amove','path','worldGoal','attackId','board','fireAt','nade','enter','orders','queue','productionJobs','prog','rally'].filter(k=>u[k]!==undefined).map(k=>[k,u[k]])));
  const cache=sim.snapshotCache(g);
  const outputs=g.players.map((_,slot)=>{const result=sim.snapshotFor(g,slot,[],[],cache);return Object.fromEntries(fields.filter(k=>result[k]!==undefined).map(k=>[k,result[k]]));});
  cases.push({fullOutputs:fullFor(g),name:kind+':'+placement,finished:placement==='finished',map,tick:g.tick,chars:g.chars.join(''),inputs,publicState:publicState(g),players:g.players.map(p=>({team:p.team,visible:[...p.visible],mp:p.mp,mun:p.mun,fuel:p.fuel,inc:p.inc,vp:p.vp,rally:p.rally,fuelInc:p.fuelInc,upkeep:p.upkeep})),outputs});
 }
}
const prior=Math.random;Math.random=()=>.5;let startup;try{startup=sim.createGame(map,['A','Ally','Enemy'],false,[0,0,1],[0,1,2],{mode:'conquest',weather:'clear',tech:true,logistics:false});}finally{Math.random=prior;}
const inputs=[...startup.units.values()].map(u=>Object.fromEntries(['id','type','owner','x','z','rot','aim','hp','supp','cd','cooldown','targetId','still','shotAt','dust','buff','ap','reinf','healAt','retreating','holdFire','holdPos','autoRetreat','auto','xp','built','moveSpeed','vx','vz','travelDir','cargo','riding','garrison','amove','path','worldGoal','attackId','board','fireAt','nade','enter','orders','queue','productionJobs','prog','rally'].filter(k=>u[k]!==undefined).map(k=>[k,u[k]])));
const cached=sim.snapshotCache(startup);
const outputs=startup.players.map((_,slot)=>{const result=sim.snapshotFor(startup,slot,[],[],cached);return Object.fromEntries(fields.filter(k=>result[k]!==undefined).map(k=>[k,result[k]]));});
cases.push({fullOutputs:fullFor(startup),name:'untouched-startup',map,tick:startup.tick,chars:startup.chars.join(''),inputs,publicState:publicState(startup),players:startup.players.map(p=>({team:p.team,visible:[...p.visible],mp:p.mp,mun:p.mun,fuel:p.fuel,inc:p.inc,vp:p.vp,rally:p.rally,fuelInc:p.fuelInc,upkeep:p.upkeep})),outputs});

for (const name of ['idle','path','goal','amove','attack-hidden','attack-seen','fire','nade','enter','board','orders','paid-queues']) {
 const prior=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(map,['A','Ally','Enemy'],false,[0,0,1],[0,1,2],{mode:'conquest',weather:'clear',tech:true,logistics:false});}finally{Math.random=prior;}
 if(name!=='paid-queues'){g.units.clear();g.nextId=1;}
 const unit= name==='paid-queues'?[...g.units.values()].find(u=>u.owner===0&&!sim.UNITS[u.type].structure):sim.internals.spawnUnit(g,0,'rifle');
 const enemy= name==='paid-queues'?[...g.units.values()].find(u=>u.owner===2&&!sim.UNITS[u.type].structure):sim.internals.spawnUnit(g,2,'rifle');
 Object.assign(unit,{x:8.05,z:8.15,rot:0,aim:0,retreating:false});Object.assign(enemy,{x:28.05,z:28.15});
 g.players.forEach(p=>p.visible=new Set());
 if(name==='path')unit.path=[{x:9.15,z:10.05},{x:12.15,z:13.05}];
 if(name==='goal'){unit.worldGoal={x:19.15,z:20.05};unit.path=[{x:11,z:12}];}
 if(name==='amove')unit.amove={x:24.15,z:25.05};
 if(name.startsWith('attack')){unit.attackId=enemy.id;unit.path=[{x:11,z:12}];if(name==='attack-seen')g.players[0].visible.add(enemy.id);}
 if(name==='fire')unit.fireAt=205;
 if(name==='nade')unit.nade={x:22.15,z:23.05};
 if(name==='enter')unit.enter=190;
 if(name==='board')unit.board=enemy.id;
 if(name==='orders')unit.orders=[{t:'move',x:14.15,z:15.05},{t:'amove',x:17.15,z:18.05}];
 if(name==='paid-queues'){const b=[...g.units.values()].find(u=>u.owner===0&&u.type==='barracks');assert.ok(b);b.queue=['rifle','rifle'];b.productionJobs=[{id:1,type:'rifle',mp:70.15,fuel:0},{id:2,type:'rifle',mp:65.05,fuel:0}];b.prog=3.15;b.rally={x:14.15,z:15.05};}
 const inputs=[...g.units.values()].map(u=>Object.fromEntries(['id','type','owner','x','z','rot','aim','hp','supp','cd','cooldown','targetId','still','shotAt','dust','buff','ap','reinf','healAt','retreating','holdFire','holdPos','autoRetreat','auto','xp','built','moveSpeed','vx','vz','travelDir','cargo','riding','garrison','amove','path','worldGoal','attackId','board','fireAt','nade','enter','orders','queue','productionJobs','prog','rally'].filter(k=>u[k]!==undefined).map(k=>[k,u[k]])));
 const cache=sim.snapshotCache(g);
 const outputs=g.players.map((_,slot)=>{const result=sim.snapshotFor(g,slot,[],[],cache);return Object.fromEntries(fields.filter(k=>result[k]!==undefined).map(k=>[k,result[k]]));});
 cases.push({fullOutputs:fullFor(g),name:'owner:'+name,map,tick:g.tick,chars:g.chars.join(''),inputs,publicState:publicState(g),players:g.players.map(p=>({team:p.team,visible:[...p.visible],mp:p.mp,mun:p.mun,fuel:p.fuel,inc:p.inc,vp:p.vp,rally:p.rally,fuelInc:p.fuelInc,upkeep:p.upkeep})),outputs});
}

for (const name of ['standard','large','massive','endless','visible-contest','hidden-contest','deleted-contest','cut','needs-enemy','needs-ally','wx-rounding','support-rounding','current-effects','reveal-effects','support-calls','support-actors','tech-off']) {
 const prior=Math.random;Math.random=()=>.5;let g;const army=['standard','large','massive','endless'].includes(name)?name:'standard';try{g=sim.createGame(map,['A','Ally','Enemy'],false,[0,0,1],[0,1,2],{mode:'conquest',weather:'clear',logistics:false,army,tech:name!=='tech-off'});}finally{Math.random=prior;}
 const own=[...g.units.values()].find(u=>u.owner===0&&!sim.UNITS[u.type].structure),enemy=[...g.units.values()].find(u=>u.owner===2&&!sim.UNITS[u.type].structure);
 g.players.forEach(p=>p.visible=new Set());
 if(name.includes('contest')){Object.assign(g.points[0],{owner:0,capper:2,progress:.15,contested:true,onPoint:[own.id,enemy.id]});if(name!=='hidden-contest'){g.players[0].visible.add(enemy.id);g.players[2].visible.add(own.id);}if(name==='deleted-contest')g.units.delete(enemy.id);}
 if(name==='cut')Object.assign(g.points[0],{cut:true,owner:2,capper:1,progress:.25});
 if(name.startsWith('needs')){g.points.push({...g.points[0],owner:name==='needs-ally'?1:2});Object.assign(g.points[0],{needs:1,owner:-1});}
 if(name==='wx-rounding'){g.wx={rain:.15,wet:.25};g.wind={a:-.05,v:.15};}
 if(name==='support-rounding'){g.players.forEach((p,i)=>Object.keys(p.sup).forEach((k,j)=>p.sup[k]=i?-.15:j+.15));}
 if(name==='support-calls'){g.players.forEach(p=>p.mp=1000);for(const [owner,kind] of [[0,'artillery'],[1,'smoke'],[2,'cover']])assert.equal(sim.command(g,owner,{t:'support',kind,x:14.15+owner,z:16.25,dir:-.05}),undefined);}
 if(name==='support-actors'){g.strikes=[{kind:'artillery',owner:2,x:14.15,z:16.25,dir:-.05,t:-.15,left:3,next:.5,live:true},{kind:'smoke',owner:0,x:24.25,z:26.15,dir:1.05,t:2.15,left:8,next:0,live:false}];g.covers=[{team:0,owner:1,x:12.15,z:14.25,r:27.15,t:2.01},{team:1,owner:2,x:22.25,z:24.15,r:28.25,t:.01},{team:0,owner:0,x:32.15,z:34.25,r:29.15,t:-.01}];}
 if(name.endsWith('effects')){g.smokes=[{id:1,x:own.x,z:own.z,r:1.15,t:8},{id:2,x:enemy.x,z:enemy.z,r:2.25,t:5}];g.fires.set(5*20+5,4);g.fires.set(17*20+17,7);g.fires.set(5*20+5,6);if(name==='reveal-effects')g.reveal=true;}
 const inputs=[...g.units.values()].map(u=>Object.fromEntries(['id','type','owner','x','z','rot','aim','hp','supp','cd','cooldown','targetId','still','shotAt','dust','buff','ap','reinf','healAt','retreating','holdFire','holdPos','autoRetreat','auto','xp','built','moveSpeed','vx','vz','travelDir','cargo','riding','garrison','amove','path','worldGoal','attackId','board','fireAt','nade','enter','orders','queue','productionJobs','prog','rally'].filter(k=>u[k]!==undefined).map(k=>[k,u[k]])));
 const cache=sim.snapshotCache(g);
 const outputs=g.players.map((_,slot)=>{const result=sim.snapshotFor(g,slot,[],[],cache);return Object.fromEntries(fields.filter(k=>result[k]!==undefined).map(k=>[k,result[k]]));});
 cases.push({fullOutputs:fullFor(g),name:'public:'+name,map,tick:g.tick,chars:g.chars.join(''),inputs,publicState:publicState(g,army),reveal:g.reveal,players:g.players.map(p=>({team:p.team,visible:[...p.visible],mp:p.mp,inc:p.inc,vp:p.vp,rally:p.rally})),outputs});
}
// Retain one canonical game across list observations. Full snapshots come from
// the original reader, including its hidden/removal memory and mine order.
const listMap={name:'Active snapshot lists',w:100,h:60,rows:Array(60).fill('.'.repeat(100)),spawns:[{x:2,y:2},{x:2,y:57},{x:97,y:57}],points:[{x:50,y:30}]};
const originalRandom=Math.random;Math.random=()=>.5;let listsGame;try{listsGame=sim.createGame(listMap,['A','Ally','Enemy'],false,[0,0,1],[0,1,2],{mode:'conquest',weather:'clear',tech:true,logistics:false});}finally{Math.random=originalRandom;}
listsGame.units.clear();listsGame.nextId=1;
for(const [owner,x,z]of [[0,41,41],[1,181,101],[2,181,101]])Object.assign(sim.internals.spawnUnit(listsGame,owner,'rifle'),{x,z});
const inputKeys=['id','type','owner','x','z','rot','aim','hp','supp','cd','cooldown','targetId','still','shotAt','dust','buff','ap','reinf','healAt','retreating','holdFire','holdPos','autoRetreat','auto','xp','built','moveSpeed','vx','vz','travelDir','cargo','riding','garrison','amove','path','worldGoal','attackId','board','fireAt','nade','enter','orders','queue','productionJobs','prog','rally'];
const initial={map:listMap,tick:listsGame.tick,chars:listsGame.chars.join(''),inputs:[...listsGame.units.values()].map(u=>Object.fromEntries(inputKeys.filter(k=>u[k]!==undefined).map(k=>[k,u[k]]))),publicState:publicState(listsGame),players:listsGame.players.map(p=>({team:p.team,visible:[...p.visible],mp:p.mp,inc:p.inc,vp:p.vp,rally:p.rally}))};
const listActions=[
 {name:'visible-wreck-insertion-order',tick:1,vision:true,wrecks:[{id:9002,type:'tank',owner:2,x:43.14,z:41.26,rot:-.26,moving:true},{id:9001,type:'halftrack',owner:1,x:45.15,z:41.34,rot:.35}]},
 {name:'visible-motion-and-owner-change',wrecks:[{id:9002,type:'halftrack',owner:0,x:43.44,z:41.56,rot:1.36,moving:true}]},
 {name:'hidden-motion-retains-observed-metadata',tick:5,vision:true,move:[{id:1,x:181,z:101}],wrecks:[{id:9002,type:'tank',owner:2,x:43.74,z:41.86,rot:2.35,moving:true}]},
 {name:'hidden-removal-keeps-memory',remove:[9001]},
 {name:'team-mine-discovery-order',mines:[{cell:422,value:true},{cell:420,value:true},{cell:421,value:true}],seen:[{cell:422,bits:1},{cell:420,bits:2},{cell:421,bits:1}]},
 {name:'update-mine-bits-retains-order',seen:[{cell:422,bits:3}]},
 {name:'mine-reinsert-moves-to-end',forget:[422],seen:[{cell:422,bits:1}]},
 {name:'removed-mine-is-omitted',mines:[{cell:421,value:false}]},
 {name:'reveal-updates-hidden-lists',reveal:true},
 {name:'visible-removal-prunes-memory',tick:9,vision:true,reveal:false,move:[{id:1,x:41,z:41}],remove:[9002]},
 {name:'reused-wreck-id-appends-after-removal',wrecks:[{id:9001,type:'tank',owner:2,x:44.14,z:42.24,rot:-.26}]},
];
for(const action of listActions){
 if(action.tick!==undefined)listsGame.tick=action.tick;
 if(action.move)for(const u of action.move)Object.assign(listsGame.units.get(u.id),{x:u.x,z:u.z});
 if(action.wrecks)for(const input of action.wrecks){const {moving,...w}=input;const motion=moving?{id:`wreck:${w.id}`,kind:'wreck',x:w.x,y:.08,z:w.z,dir:.35,material:'steel',mass:8500,vx:.13,vy:-.02,vz:.27,gravity:9.81,drag:3.2,stamp:1.25,born:.5,tilt:.007,settled:false}:undefined;const old=listsGame.wrecks.find(q=>q.id===w.id);if(old)Object.assign(old,w,{motion});else listsGame.wrecks.push({...w,motion});}
 if(action.remove)listsGame.wrecks=listsGame.wrecks.filter(w=>!action.remove.includes(w.id));
 if(action.mines)for(const mine of action.mines)sim.mutateWorldCell(listsGame,mine.cell,{mine:mine.value});
 if(action.forget)for(const cell of action.forget)listsGame.mineSeen.delete(cell);
 if(action.seen)for(const mine of action.seen)listsGame.mineSeen.set(mine.cell,mine.bits);
 if(action.reveal!==undefined)listsGame.reveal=action.reveal;
 if(action.vision){rebuildGrid(listsGame);sim.internals.updateVision(listsGame);}
 action.outputs=fullFor(listsGame);
}
const listTraces=[{name:'ordinary-active-lists',initial,actions:listActions}];
clean();
const content=JSON.stringify({pin,cases,listTraces})+'\n',target=resolve(root,'rust/crates/core/tests/fixtures/recipient-snapshot.json');
if(process.argv.includes('--check'))assert.equal(readFileSync(target,'utf8'),content);else writeFileSync(target,content);
console.log(`${cases.length} actual cached snapshots, ${cases.length*3} partial and ${cases.length*3} complete quiet seat projections; ${listActions.length} active-list frames, ${listActions.length*3} full recipient results`);
