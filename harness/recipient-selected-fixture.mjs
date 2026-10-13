// Record actual room selection and original selected terrain/fog/list collectors.
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
assert.ok(process.argv.slice(2).every(a=>a==='--check'));
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const {rebuildGrid}=await import(pathToFileURL(resolve(reference,'shared/grid.js')));
const {createRoomTick}=await import(pathToFileURL(resolve(reference,'server/room-tick.js')));
const map={name:'Selected delivery',w:100,h:60,rows:Array(60).fill('.'.repeat(100)),spawns:[{x:2,y:2},{x:2,y:57},{x:97,y:57}],points:[{x:50,y:30}]};
const random=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(map,['A','Ally','Enemy'],false,[0,0,1],[0,1,2],{mode:'conquest',weather:'clear',tech:true,logistics:false});}finally{Math.random=random;}
g.units.clear();g.nextId=1;
for(const [owner,x,z]of [[0,41,41],[1,181,101],[2,181,101]])Object.assign(sim.internals.spawnUnit(g,owner,'rifle'),{x,z});
const keys=['id','type','owner','x','z','rot','aim','hp','supp','cd','cooldown','targetId','still','shotAt','dust','buff','ap','reinf','healAt','retreating','holdFire','holdPos','autoRetreat','auto','xp','built','moveSpeed','vx','vz','travelDir','cargo','riding','garrison','amove','path','worldGoal','attackId','board','fireAt','nade','enter','orders','queue','productionJobs','prog','rally'];
const initial={map,tick:g.tick,chars:g.chars.join(''),inputs:[...g.units.values()].map(u=>Object.fromEntries(keys.filter(k=>u[k]!==undefined).map(k=>[k,u[k]]))),publicState:{army:'standard',tech:g.tech,wx:g.wx,wind:g.wind,support:g.players.map(p=>p.sup),strikes:g.strikes,covers:g.covers??[],points:g.points.map(q=>({owner:q.owner,capper:q.capper,progress:q.progress,cut:q.cut,needs:q.needs,contested:q.contested,onPoint:q.onPoint})),smokes:g.smokes,fires:[...g.fires]},players:g.players.map(p=>({team:p.team,visible:[...p.visible],mp:p.mp,inc:p.inc,vp:p.vp,rally:p.rally}))};
const actions=[
 {name:'all-ready',tick:2,vision:true,ready:[true,true,true]},
 {name:'stalled-ally',tick:4,ready:[true,true,true],stalled:[false,true,false],cell:2020,ch:'T',wrecks:[{id:9001,type:'tank',owner:2,x:43.14,z:41.26,rot:-.26}]},
 {name:'disconnected-enemy',tick:6,ready:[true,false,false],cell:5090,ch:'T',wrecks:[{id:9002,type:'halftrack',owner:0,x:183,z:101,rot:.35}]},
 {name:'no-eligible-room-bypass',tick:8,ready:[false,true,false],stalled:[false,true,false],cell:2021,ch:'T'},
 {name:'late-ally-catches-up',tick:10,vision:true,ready:[false,true,false]},
 {name:'all-resume',tick:12,ready:[true,true,true]},
 {name:'full-reconnect-retains-outbox',tick:13,cell:5091,ch:'T',manual:[{slot:1,full:true,omit:false}]},
 {name:'delta-after-full',tick:14,ready:[false,true,false]},
 {name:'omit-mask-retains-fog-base',tick:17,vision:true,move:[{id:1,x:61,z:61}],manual:[{slot:0,full:false,omit:true}]},
 {name:'mask-resumes-from-own-base',tick:18,ready:[true,false,false]},
 {name:'explicit-caller-order',tick:19,manual:[{slot:2,full:false,omit:false},{slot:0,full:false,omit:false}]},
];
const copy=v=>JSON.parse(JSON.stringify(v));
const memory=p=>JSON.stringify({terrain:p.terrainMemory?[...p.terrainMemory]:null,pending:p.terrainPending?[...p.terrainPending]:null,key:p.terrainPending?.key,fog:p.fog?{version:p.fog.version,base:[...p.fog.base]}:null,wrecks:p.wreckMemory?[...p.wreckMemory]:null});
let current,outputs,cacheCalls,meterSent;
const room={game:g,snapEvery:2,players:Array.from({length:3},()=>({ws:{readyState:1}})),spectators:[],operatives:[]};
const tick=createRoomTick({step:()=>{},aiTick:()=>undefined,snapshotCache:game=>{cacheCalls++;return sim.snapshotCache(game);},snapshotFor:sim.snapshotFor,
 trimmed:(_room,_net,{online,ping,...msg})=>msg,sendSnapshot:(_room,p,json)=>outputs.push({slot:room.players.indexOf(p),fields:JSON.parse(json)}),sendWatchers:()=>assert.fail('unexpected watcher'),
 connected:p=>p.ws?.readyState===1,backedUp:p=>p.stalled,createTickMeter:()=>({}),recordTick:(_m,fields)=>{meterSent=fields.sent;return 2;},tickStats:()=>({})});
for(current of actions){
 g.tick=current.tick;
 if(current.cell!==undefined)sim.internals.setCell(g,current.cell,current.ch);
 if(current.move)for(const u of current.move)Object.assign(g.units.get(u.id),{x:u.x,z:u.z});
 if(current.wrecks)g.wrecks.push(...copy(current.wrecks));
 if(current.vision){rebuildGrid(g);sim.internals.updateVision(g);}
 outputs=[];cacheCalls=0;meterSent=false;
 let selected;
 if(current.manual){
  selected=current.manual;
  const before=g.players.map(memory);
  for(const {slot,full,omit}of selected){const fields={cells:sim.terrainFor(g,slot,full)};if(!omit)fields.fog=sim.fogFor(g,slot,full);outputs.push({slot,fields:copy(fields)});}
  g.players.forEach((p,i)=>{if(!selected.some(s=>s.slot===i))assert.equal(memory(p),before[i]);});
 }else{
  room.players.forEach((p,i)=>{p.ws.readyState=current.ready[i]?1:3;p.stalled=current.stalled?.[i]??false;});
  selected=room.players.flatMap((p,slot)=>p.ws.readyState===1&&!p.stalled?[{slot,full:false,omit:false}]:[]);
  const before=g.players.map(memory);
  tick(room);
  assert.equal(cacheCalls,selected.length?1:0);assert.equal(meterSent,true);
  g.players.forEach((p,i)=>{if(!selected.some(s=>s.slot===i))assert.equal(memory(p),before[i]);});
 }
 current.selected=selected;current.outputs=outputs;
}
clean();const content=JSON.stringify({pin,initial,actions})+'\n',target=resolve(root,'rust/crates/core/tests/fixtures/recipient-selected.json');
if(process.argv.includes('--check'))assert.equal(readFileSync(target,'utf8'),content);else writeFileSync(target,content);
console.log(`${actions.length} selected-delivery actions, ${actions.reduce((n,a)=>n+a.outputs.length,0)} recipient results; actual room callback and full/omit collectors`);
