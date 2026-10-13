// Record bounded ability command, timer/autocast and aimed execution phases from the pinned source.
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
const temp=mkdtempSync(join(tmpdir(),'ww2-match-ability-'));
const originalRandom=Math.random;
try {
 let code=readFileSync(join(reference,'shared/sim.js'),'utf8');
 const extract=(start,end)=>{assert.equal(code.split(start).length,2);const at=code.indexOf(start),to=code.indexOf(end,at);assert.ok(to>at);return code.slice(at,to);};
 const timers=extract('    if (def.infantry) u.supp = Math.max(0, u.supp - (inTrench(g, u) ? CFG.trench.rally : 8) * dt);','\n    // auto-retreat:');
 const autocast=extract('    if (u.auto && u.cd <= 0 && !u.nade && !u.retreating && (g.tick + u.id) % AUTO_EVERY === 0) {','\n\n    // Engineers:');
 const aimedBlock=extract('    if (u.nade) {\n      const ab = def.ab, halt = !u.nade.auto || u.nade.walked;','\n\n    // heading into a building:');
 const marker='  setupStructures(g, map);';assert.equal(code.split(marker).length,2);
 code=code.replace(marker,marker+'\n  return g;').replace(/from '(\.\/[^']+)'/g,(_,p)=>`from '${pathToFileURL(join(reference,'shared',p)).href}'`);
 code+=`\nexport const abilityHooks={beginPathTick, phase(g,u,dt){const def=UNITS[u.type],w=def.w;${timers}\n${autocast}\n${aimedBlock}},aimed(g,u){const def=UNITS[u.type],w=def.w;${aimedBlock}}};\n`;
 writeFileSync(join(temp,'sim.mjs'),code);
 const sim=await import(pathToFileURL(join(temp,'sim.mjs'))),H=sim.abilityHooks,grid=await import(pathToFileURL(join(reference,'shared/grid.js')));
 const cases=[],control=['follow','auto','autoRetreat','holdFire','holdPos','attackId','targetId','orders','face','entrench','dig','build','enter','board','amove','fireAt','stuck','drift','reverse','repath','cooldown','retarget','react','traffic','trafficWait','moveOutcome','moveOutcomeTick','moveResult','worldGoal','garrison','shotAt'];
 const blank=()=>({name:'Ability',w:80,h:40,rows:Array(40).fill('.'.repeat(80)),spawns:[{x:0,y:0},{x:79,y:39}],points:[{x:40,y:20}]}),cell=(m,x,y,ch)=>{m.rows[y]=m.rows[y].slice(0,x)+ch+m.rows[y].slice(x+1);};
 const U=(id,type,x=15,z=15,extra={})=>({id,type,x,z,...extra}),command=(ids,x,z,extra={})=>({op:'command',slot:0,cmd:{t:'ability',ids,...(x===undefined?{}:{x,z}),...extra}}),target=id=>({op:'target',id}),prime=(id,changes)=>({op:'prime',id,changes}),phase=(ids,dt=.05,tick=10)=>({op:'phase',ids,dt,tick}),aimed=ids=>({op:'aimed',ids});
 function record(name,units,actions,opts={}) {
  const map=opts.map??blank();assert.equal(sim.validateMap(map),null);
  const initializationRandom=[];Math.random=()=>{initializationRandom.push(.5);return .5;};
  const g=sim.createGame(map,[],false,[],[],{weather:false});
  g.mode={kind:opts.mode??'conquest'};g.players=[0,1].map(slot=>({slot,team:slot,faction:slot,mp:100,mun:0,fuel:0,out:false,away:false,spawn:{x:3+slot*150,z:3+slot*70},visible:new Set(),armory:{inf:0,guns:0,armor:0}}));
  g.tick=10;g.tech=false;g.seed=123;g.nextId=1000;g.nextFlight=2000;g.wx={rain:0,wet:0};g.wind={a:0,v:.5};g.pathRetryLimit=1;
  for(const v of units){const d=sim.UNITS[v.type];assert.ok(d,v.type);g.units.set(v.id,{owner:0,rot:0,aim:0,hp:d.models*d.hpPer,supp:0,path:[],orders:[],riding:0,cargo:[],xp:0,garrison:-1,follow:77,auto:false,autoRetreat:false,holdFire:false,holdPos:false,attackId:0,targetId:0,cd:0,buff:0,ap:false,sprint:0,nade:null,cooldown:0,retarget:0,repath:0,react:0,stuck:0,retreating:false,dig:null,build:0,enter:-1,board:0,amove:null,face:null,entrench:null,fireAt:-1,drift:false,reverse:null,traffic:null,trafficWait:0,worldGoal:null,moveSpeed:0,vx:0,vz:0,...structuredClone(v)});}
  for(const p of g.players)p.visible=new Set([...g.units.keys()]);grid.rebuildGrid(g);
  const randomCalls=[],results=[],snapshots=[];
  const flight=p=>Object.fromEntries(Object.entries(p).filter(([k])=>!['source','weapon','blastSource','suppressed'].includes(k)).concat([['suppressed',[...p.suppressed]],['blastSource',p.blastSource??null]]));
  const snapshot=()=>({tick:g.tick,units:[...g.units.values()].map(u=>({id:u.id,hp:u.hp,x:u.x,z:u.z,cd:u.cd,buff:u.buff,ap:u.ap,sprint:u.sprint,supp:u.supp,nade:u.nade,path:u.path,retreating:u.retreating,logistics:u.logistics??null,control:Object.fromEntries(control.map(k=>[k,u[k]??null]))})),wallets:g.players.map(p=>({mp:p.mp,mun:p.mun,fuel:p.fuel})),smokes:g.smokes,shots:g.shots,flights:[...g.flights.values()].map(flight),salvos:g.salvos,nades:g.nades,nextId:g.nextId,nextFlight:g.nextFlight,randomCalls:[...randomCalls]});
  const save=()=>snapshots.push(structuredClone(snapshot()));save();
  for(const a of actions){g.shots=[];let result=null;
   Math.random=()=>{const value=[.125,.25,.375,.5,.625,.75,.875,.0625][randomCalls.length%8];randomCalls.push(value);return value;};
   if(a.op==='command')result=sim.command(g,a.slot,a.cmd,a.auto??false)??null;
   else if(a.op==='target')result=sim.autocastTarget(g,g.units.get(a.id));
   else if(a.op==='prime'){Object.assign(g.units.get(a.id),structuredClone(a.changes));grid.rebuildGrid(g);}
   else if(a.op==='player')Object.assign(g.players[a.slot],a.changes);
   else if(a.op==='visibility')g.players[a.owner].visible=new Set(a.ids);
   else if(a.op==='phase'||a.op==='aimed'){if(a.tick!==undefined)g.tick=a.tick;H.beginPathTick(g);for(const id of a.ids){const u=g.units.get(id);if(u&&u.hp>0&&!u.riding&&!u.operative){if(a.op==='phase')H.phase(g,u,a.dt);else H.aimed(g,u);}}}
   else throw Error(a.op);
   results.push(result??null);save();
  }
  cases.push({name,map,mode:g.mode.kind,initializationRandom,units,actions,results,snapshots});
 }
 record('instant-families-zero-conquest-cost-shared-smoke-ids',[U(1,'mg'),U(2,'at'),U(3,'tank'),U(4,'conscript',20,15,{supp:75,path:[{x:25,z:15}]}),U(5,'tank',30,15)],[command([1,2,3,4,5]),command([3]),phase([1,2,3,4],.05),phase([1,2,3,4],50)]);
 record('refusal-precedence-and-selection-success',[U(1,'rifle',15,15,{cd:2}),U(2,'rifle',15,20,{retreating:true}),U(3,'rifle',20,20,{logistics:{ammo:0,forced:false,stranded:false}}),U(4,'mg')],[command([1,2]),command([2,1]),command([1,3]),command([3,1]),command([999,2]),command([2,999]),command([1,4])]);
 record('command-ownership-carrier-operative-structure-and-dead',[U(1,'mg',15,15,{owner:1}),U(2,'mg',15,20,{riding:9}),U(3,'mg',20,20,{operative:1}),U(4,'hq'),U(5,'mg',20,25,{hp:0})],[command([1,2,3,4]),command([5]),phase([5],1)]);
 record('forced-logistics-and-stranded-admission',[U(1,'mg',15,15,{logistics:{ammo:2,forced:true,stranded:false}}),U(2,'mg',20,15,{logistics:{ammo:2,forced:true,stranded:true}})],[command([1]),command([2])]);
 record('classic-cost-admission-boundary',[U(1,'mg'),U(2,'rifle',20,15),U(3,'rocket',25,15)],[command([1]),{op:'player',slot:0,changes:{mun:10}},command([1]),command([2],30,15),command([3],45,15)],{mode:'classic'});
 const dirty={path:[{x:60,z:15}],orders:[{t:'move',x:70,z:15}],face:{x:1,z:0},entrench:{jobs:[]},dig:{x:9,z:9},build:9,enter:2,board:9,amove:{x:60,z:15},targetId:9,attackId:9,fireAt:5,drift:'rally',reverse:{x:1,z:1},worldGoal:{x:70,z:15},traffic:{id:8},stuck:3,repath:2};
 record('manual-aimed-clears-and-clamps',[U(1,'rifle',15,15,dirty)],[command([1]),command([1],-5,999),command([1],25,15),aimed([1])]);
 record('grenade-launch-and-ammo-recheck',[U(1,'rifle',15,15,{logistics:{ammo:5,forced:false,stranded:false}})],[command([1],25,15),aimed([1]),prime(1,{cd:0,logistics:{ammo:4,forced:false,stranded:false}}),command([1],25,15),prime(1,{logistics:{ammo:0,forced:false,stranded:false},nade:{x:25,z:15}}),aimed([1])]);
 record('satchel-launch-and-auto-path-retention',[U(1,'ranger',15,15,{path:[{x:40,z:15}],orders:[{t:'move',x:40,z:15}]})],[{...command([1],20,15),auto:true},aimed([1]),prime(1,{cd:0,nade:{x:20,z:15,auto:true,walked:true},path:[{x:40,z:15}]}),aimed([1])]);
 record('barrage-launch-partial-stock-and-delays',[U(1,'mortar',15,15,{logistics:{ammo:2,forced:false,stranded:false},path:[{x:50,z:15}]})],[command([1],40,15),aimed([1]),prime(1,{cd:0,logistics:{ammo:0,forced:false,stranded:false},nade:{x:40,z:15}}),aimed([1])]);
 record('rocket-default-shell-count',[U(1,'rocket')],[command([1],50,15),aimed([1])]);
 record('walk-then-launch-stops-automatic-route',[U(1,'rifle',15,15,{path:[{x:20,z:15}],orders:[{t:'move',x:70,z:15}]})],[{...command([1],45,15),auto:true},aimed([1]),prime(1,{x:30,z:15,repath:0}),aimed([1])]);
 {const map=blank();for(let y=0;y<map.h;y++)cell(map,15,y,'W');record('unreachable-aimed-route-drops-source-order',[U(1,'rifle')],[command([1],45,15),aimed([1])],{map});}
 record('forced-aimed-outside-range-cancels',[U(1,'rifle',15,15,{nade:{x:60,z:15},logistics:{ammo:5,forced:true,stranded:true}})],[aimed([1])]);
 record('exact-cooldown-zero-negative-and-staggered-auto',[U(1,'conscript',15,15,{auto:true,cd:.05,supp:70,path:[{x:30,z:15}],orders:[{t:'move',x:50,z:15}]})],[phase([1],.05,8),prime(1,{supp:70}),phase([1],0,9),phase([1],36,19),prime(1,{supp:70,retreating:true}),phase([1],0,29)]);
 record('auto-manual-nade-and-riding-dead-exclusions',[U(1,'conscript',15,15,{auto:true,supp:70,path:[{x:30,z:15}],nade:{x:90,z:15},repath:5}),U(2,'conscript',20,15,{auto:true,supp:70,path:[{x:30,z:15}],riding:9}),U(3,'conscript',25,15,{auto:true,supp:70,path:[{x:30,z:15}],hp:0})],[phase([1,2,3],.05,9)]);
 {const map=blank();cell(map,15,7,'T');cell(map,7,15,'T');record('grenade-cover-nearest-tie-friend-clear-and-hold',[U(1,'rifle'),U(3,'rifle',31,15,{owner:1}),U(2,'rifle',15,31,{owner:1}),U(4,'rifle',50,50)],[target(1),prime(4,{x:31,z:20.5}),target(1),prime(1,{holdFire:true}),target(1),prime(1,{holdFire:false,garrison:1}),target(1),prime(1,{garrison:-1}),{op:'visibility',owner:0,ids:[1,4]},target(1)],{map});}
 record('suppress-approaching-visible-infantry',[U(1,'mg',15,15,{auto:true}),U(2,'rifle',30,15,{owner:1,rot:Math.PI,path:[{x:15,z:15}]})],[target(1),prime(2,{rot:0}),target(1),prime(2,{rot:Math.PI,retreating:true}),target(1),prime(2,{retreating:false}),prime(1,{path:[{x:16,z:15}]}),target(1),prime(1,{path:[],holdFire:true}),target(1),prime(1,{holdFire:false}),phase([1],0,9)]);
 record('ap-current-target-and-held-fire',[U(1,'at',15,15,{targetId:2,holdFire:true}),U(2,'tank',30,15,{owner:1}),U(3,'rifle',30,15,{owner:1}),U(4,'bunker',30,15,{owner:1})],[target(1),command([1]),target(1),prime(1,{ap:false,targetId:3}),target(1),prime(1,{targetId:4}),target(1),prime(1,{targetId:2}),{op:'visibility',owner:0,ids:[1]},target(1)]);
 record('naval-ap-target-filter',[U(1,'gunboat',15,15,{targetId:2}),U(2,'tank',25,15,{owner:1}),U(3,'gunboat',25,15,{owner:1})],[target(1),prime(1,{targetId:3}),target(1)]);
 record('smoke-half-health-and-recent-antitank-hit',[U(1,'tank',15,15,{hp:179,atHit:-50,holdFire:true,auto:true})],[target(1),prime(1,{atHit:-51}),target(1),prime(1,{hp:180,atHit:10}),target(1),prime(1,{hp:179}),target(1),phase([1],0,9)]);
 record('ura-threshold-and-path',[U(1,'conscript',15,15,{supp:49,path:[{x:30,z:15}],holdFire:true})],[target(1),prime(1,{supp:50}),target(1),prime(1,{path:[]}),target(1)]);
 record('barrage-crowd-tie-clear-holding-fire',[U(1,'mortar',15,15,{auto:true}),U(3,'rifle',45,15,{owner:1}),U(2,'rifle',46,15,{owner:1}),U(4,'rifle',45,16,{owner:1}),U(5,'rifle',100,60)],[target(1),prime(5,{x:45,z:15}),target(1),prime(5,{x:100,z:60}),prime(1,{holdFire:true}),target(1),prime(1,{holdFire:false}),phase([1],0,9)]);
 {const map=blank();cell(map,22,7,'T');record('barrage-dug-building-and-minimum-range',[U(1,'mortar'),U(2,'rifle',45,15,{owner:1}),U(3,'hq',50,15,{owner:1})],[target(1),prime(2,{x:17,z:15,garrison:1}),target(1),prime(2,{x:45,z:15}),target(1)],{map});}
 record('satchel-attack-order-held-fire-and-friend-radius',[U(1,'ranger',15,15,{attackId:2,holdFire:true}),U(2,'rifle',30,15,{owner:1,garrison:2}),U(3,'rifle',80,50)],[target(1),prime(3,{x:35,z:15}),target(1),prime(3,{x:35.01,z:15}),target(1),prime(1,{garrison:1}),target(1),prime(1,{garrison:-1,attackId:0}),target(1)]);
 record('queued-aimed-defers-cooldown-ammo-and-munitions',[U(1,'rifle',15,15,{cd:30,logistics:{ammo:0,forced:false,stranded:false}})],[command([1],25,15,{queue:true}),command([1],25,15),prime(1,{retreating:true}),command([1],25,15,{queue:true})],{mode:'classic'});
 record('autocast-ap-order-preservation',[U(1,'at',15,15,{auto:true,targetId:2,path:[{x:45,z:15}],orders:[{t:'move',x:60,z:15}]}),U(2,'tank',30,15,{owner:1})],[phase([1],0,8),phase([1],0,9)]);
 record('autocast-grenade-preserves-player-orders',[U(1,'rifle',15,15,{auto:true,path:[{x:45,z:15}],orders:[{t:'move',x:60,z:15}]}),U(2,'rifle',30,15,{owner:1,garrison:2})],[phase([1],0,9)]);
 record('autocast-satchel-walks-with-attack-order',[U(1,'ranger',15,15,{auto:true,attackId:2,path:[{x:35,z:15}],orders:[{t:'move',x:60,z:15}]}),U(2,'rifle',30,15,{owner:1,garrison:2})],[phase([1],0,9)]);
 record('throw-exact-range-and-just-outside',[U(1,'rifle')],[command([1],33.001,15),aimed([1]),prime(1,{nade:{x:33,z:15},repath:0}),aimed([1])]);
 record('barrage-exact-range-and-just-outside',[U(1,'mortar')],[command([1],65.001,15),aimed([1]),prime(1,{nade:{x:65,z:15},repath:0}),aimed([1])]);
 const byName=name=>cases.find(c=>c.name===name),last=name=>byName(name).snapshots.at(-1);
 assert.deepEqual(byName('refusal-precedence-and-selection-success').results,['retreating','cooldown','ammo','cooldown','retreating','needs',null]);
 assert.equal(last('command-ownership-carrier-operative-structure-and-dead').units.find(u=>u.id===5).cd,40);
 assert.deepEqual(last('instant-families-zero-conquest-cost-shared-smoke-ids').smokes.map(s=>s.id),[1000,1001]);
 assert.equal(last('unreachable-aimed-route-drops-source-order').units[0].control.moveOutcome,'unreachable');
 assert.equal(last('unreachable-aimed-route-drops-source-order').units[0].nade,null);
 assert.equal(last('barrage-launch-partial-stock-and-delays').flights.length,2);
 assert.equal(last('rocket-default-shell-count').flights.length,8);
 assert.equal(last('autocast-grenade-preserves-player-orders').units[0].control.orders.length,1);
 assert.equal(last('autocast-grenade-preserves-player-orders').flights.length,1);
 assert.equal(last('autocast-satchel-walks-with-attack-order').units[0].nade.walked,true);
 assert.equal(last('throw-exact-range-and-just-outside').flights.length,1);
 assert.equal(last('barrage-exact-range-and-just-outside').flights.length,4);
 clean();const content=JSON.stringify({reference:pin,node:process.version,arch:process.arch,scope:'Original command and autocastTarget; extracted timer, autocast and aimed execution phases. No full tick or public wire claim.',cases})+'\n';
 const path=join(root,'rust/crates/core/tests/fixtures/match-ability.json');if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),content);else writeFileSync(path,content);
 console.error(`${process.argv.includes('--check')?'Verified':'Recorded'} ${cases.length} match ability traces, ${cases.reduce((n,c)=>n+c.actions.length,0)} actions, ${cases.reduce((n,c)=>n+c.snapshots.length,0)} snapshots`);
} finally {Math.random=originalRandom;rmSync(temp,{recursive:true,force:true});clean();}
