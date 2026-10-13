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
const rows=Array(20).fill('.'.repeat(20));
rows[5]=rows[5].slice(0,5)+'T'+rows[5].slice(6);
rows[7]=rows[7].slice(0,7)+'#'+rows[7].slice(8);
rows[9]=rows[9].slice(0,9)+'B'+rows[9].slice(10);
const map={name:'Snapshot fixture',w:20,h:20,rows,spawns:[{x:1,y:1},{x:18,y:18},{x:1,y:18}],points:[{x:10,y:10}]};
assert.ok(process.argv.slice(2).every(arg=>arg==='--check'));
const cases=[];
const fields=['tick','units','mp','mun','fuel','inc','fuelInc','upkeep','out','vp','winner','end','rally','tech','weather','plans','orders','queues','productionJobs'];
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
  cases.push({name:kind+':'+placement,finished:placement==='finished',map,tick:g.tick,chars:g.chars.join(''),inputs,players:g.players.map(p=>({team:p.team,visible:[...p.visible],mp:p.mp,mun:p.mun,fuel:p.fuel,inc:p.inc,vp:p.vp,rally:p.rally,fuelInc:p.fuelInc,upkeep:p.upkeep})),outputs});
 }
}
const prior=Math.random;Math.random=()=>.5;let startup;try{startup=sim.createGame(map,['A','Ally','Enemy'],false,[0,0,1],[0,1,2],{mode:'conquest',weather:'clear',tech:true,logistics:false});}finally{Math.random=prior;}
const inputs=[...startup.units.values()].map(u=>Object.fromEntries(['id','type','owner','x','z','rot','aim','hp','supp','cd','cooldown','targetId','still','shotAt','dust','buff','ap','reinf','healAt','retreating','holdFire','holdPos','autoRetreat','auto','xp','built','moveSpeed','vx','vz','travelDir','cargo','riding','garrison','amove','path','worldGoal','attackId','board','fireAt','nade','enter','orders','queue','productionJobs','prog','rally'].filter(k=>u[k]!==undefined).map(k=>[k,u[k]])));
const cached=sim.snapshotCache(startup);
const outputs=startup.players.map((_,slot)=>{const result=sim.snapshotFor(startup,slot,[],[],cached);return Object.fromEntries(fields.filter(k=>result[k]!==undefined).map(k=>[k,result[k]]));});
cases.push({name:'untouched-startup',map,tick:startup.tick,chars:startup.chars.join(''),inputs,players:startup.players.map(p=>({team:p.team,visible:[...p.visible],mp:p.mp,mun:p.mun,fuel:p.fuel,inc:p.inc,vp:p.vp,rally:p.rally,fuelInc:p.fuelInc,upkeep:p.upkeep})),outputs});

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
 cases.push({name:'owner:'+name,map,tick:g.tick,chars:g.chars.join(''),inputs,players:g.players.map(p=>({team:p.team,visible:[...p.visible],mp:p.mp,mun:p.mun,fuel:p.fuel,inc:p.inc,vp:p.vp,rally:p.rally,fuelInc:p.fuelInc,upkeep:p.upkeep})),outputs});
}
clean();
const content=JSON.stringify({pin,cases})+'\n',target=resolve(root,'rust/crates/core/tests/fixtures/recipient-snapshot.json');
if(process.argv.includes('--check'))assert.equal(readFileSync(target,'utf8'),content);else writeFileSync(target,content);
console.log(`${cases.length} actual cached snapshots, ${cases.length*3} recipient projections`);
