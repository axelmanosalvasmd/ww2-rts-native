// Full source paid-dig admission and unchanged in-reach unit work phases.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdtempSync,rmSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {fileURLToPath,pathToFileURL} from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
assert.ok(process.argv.slice(2).every(a=>a==='--check'));
const root=fileURLToPath(new URL('../',import.meta.url)),reference=resolve(process.env.WW2_REFERENCE??join(root,'reference/ww2-rts'));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d',git=args=>execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};clean();
const temp=mkdtempSync(join(tmpdir(),'ww2-match-engineering-')),originalRandom=Math.random;
try {
 let code=readFileSync(join(reference,'shared/sim.js'),'utf8');
 const start='    // digging: walk to the spot',end='    // grenade order:',a=code.indexOf(start),b=code.indexOf(end,a);assert.ok(a>=0&&b>a);
 const work=code.slice(a,b),marker='  setupStructures(g, map);';assert.equal(code.split(marker).length,2);
 code=code.replace(marker,marker+'\n  return g;').replace(/from '(\.\/[^']+)'/g,(_,p)=>`from '${pathToFileURL(join(reference,'shared',p)).href}'`);
 code+=`\nexport const engineeringHooks={beginPathTick,goal(g,u){const p=currentPathGoal(g,u,'dig');return p?{x:p.x,z:p.z}:null;},phase(g,u){const dt=.05;${work}}};\n`;
 writeFileSync(join(temp,'sim.mjs'),code);const sim=await import(pathToFileURL(join(temp,'sim.mjs'))),grid=await import(pathToFileURL(join(reference,'shared/grid.js')));
 const data=new DataView(new ArrayBuffer(8)),encode=v=>{if(typeof v==='number'){data.setFloat64(0,v);return data.getBigUint64(0).toString(16).padStart(16,'0');}return Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;};
 const blank=()=>({name:'Engineering',w:40,h:40,rows:Array(40).fill('.'.repeat(40)),spawns:[{x:0,y:0},{x:39,y:39}],points:[{x:20,y:20}]}),unit=(id=1,type='rifle',extra={})=>({id,type,x:20,z:23,...extra}),dig=(kind='trench',extra={})=>({op:'command',cmd:{t:'dig',ids:[1],kind,x:20,z:20,...extra}}),step=n=>({op:'step',id:1,n}),traces=[];
 function record(name,units,actions,opts={}) {
  const map=opts.map??blank();assert.equal(sim.validateMap(map),null);Math.random=()=>.5;const g=sim.createGame(map,[],false,[],[],{weather:false});Math.random=originalRandom;
  g.mode={kind:'conquest'};g.players=[0,1].map(slot=>({slot,team:slot,faction:slot,mp:500,mun:0,fuel:0,out:false,away:false,visible:new Set(),armory:{inf:0,guns:0,armor:0}}));g.tick=10;g.tech=false;g.wx={rain:0,wet:0};g.seed=123;
  for(const v of units){const d=sim.UNITS[v.type];g.units.set(v.id,{owner:0,rot:0,hp:d.hpPer*d.models,supp:0,path:[],orders:[],riding:0,cargo:[],xp:0,garrison:-1,follow:77,auto:false,autoRetreat:false,holdFire:false,holdPos:false,attackId:0,targetId:0,cd:0,buff:0,ap:false,sprint:0,nade:null,cooldown:0,retarget:0,repath:0,stuck:0,retreating:false,dig:null,build:0,enter:-1,board:0,amove:null,face:null,entrench:null,fireAt:-1,drift:false,reverse:null,traffic:null,trafficWait:0,worldGoal:null,...structuredClone(v)});}
  grid.rebuildGrid(g);
  const snapshot=()=>encode({units:[...g.units.values()].map(u=>({id:u.id,x:u.x,z:u.z,garrison:u.garrison,dig:u.dig?{kind:u.dig.kind,at:{x:u.dig.x,z:u.dig.z},dir:u.dig.dir,cells:u.dig.cells,t:u.dig.t,n:u.dig.n??null,reach:u.dig.reach??null,from:u.dig.from??null}:null,follow:u.follow,path:u.path,repath:u.repath,drift:u.drift,reverse:u.reverse,stuck:u.stuck,moveOutcome:u.moveOutcome??null,moveOutcomeTick:u.moveOutcomeTick??null})),mp:g.players.map(p=>p.mp),story:g.players.map((_,i)=>({mpSpent:g.story?.[i]?.mpSpent??0,built:g.story?.[i]?.built??0})),chars:g.chars.join(''),pits:[...(g.pits??[])].sort((a,b)=>a-b)});
  const initial=snapshot(),rows=[];
  for(const action of actions){let result=null;
   if(action.op==='command')result=sim.command(g,action.slot??0,action.cmd)??null;
   else if(action.op==='step'){const u=g.units.get(action.id);assert.ok(!u.dig||Math.hypot(u.x-u.dig.x,u.z-u.dig.z)<= (u.dig.reach??3),'fixture work must be in reach');for(let n=0;n<action.n;n++)sim.engineeringHooks.phase(g,u);}
   else if(action.op==='begin'){g.tick=action.tick;g.pathBudget=action.budget;g.pathRetryLimit=action.retry??1;sim.engineeringHooks.beginPathTick(g);}
   else if(action.op==='phase')sim.engineeringHooks.phase(g,g.units.get(action.id));
   else if(action.op==='goal')result=sim.engineeringHooks.goal(g,g.units.get(action.id));
   else if(action.op==='prime'){Object.assign(g.units.get(action.id),action.fields);grid.rebuildGrid(g);}
   else if(action.op==='mutate')for(const cell of action.cells)sim.internals.setCell(g,cell,action.ch);
   else if(action.op==='seen')g.mineSeen.set(action.cell,action.mask);
   else if(action.op==='money')g.players[action.slot??0].mp=action.mp;
   else throw Error(action.op);
   rows.push({action,result:encode(result),snapshot:snapshot()});
  }
  traces.push({name,map,units,initial,rows});
 }
 record('paid-replace-refuse-cancel',[unit()],[dig(),step(20),{op:'money',mp:0},dig('wire'),step(40),{op:'money',mp:500},dig('nest'),step(121),{op:'command',cmd:{t:'stop',ids:[1]}},step(1)]);
 record('twenty-hertz-completion',[unit()],[dig(),step(324)]);
 record('engineer-completion',[unit(1,'engineer')],[dig('aid'),step(241)]);
 record('first-id-only-and-retreat',[unit(1,'mg'),unit(2)],[dig('trench',{ids:[1,2]}),dig('trench',{ids:[999,2]}),{op:'prime',id:2,fields:{retreating:true}},dig('trench',{ids:[2,1]})]);
 record('visible-before-ground',[unit(1,'rifle',{x:1,z:1})],[dig('trench',{x:70,z:70,dir:0}),dig('trench',{x:2,z:2,dir:0})]);
 record('one-hospital-and-refused-replacement',[unit(),unit(2,'engineer',{x:24,z:23})],[dig('aid'),dig('aid',{ids:[2],x:24}),step(241),dig('aid',{ids:[2],x:24})]);
 {const map=blank();map.rows[10]='..........B'+'.'.repeat(29);record('garrison-exit-and-retained-controls',[unit(1,'rifle',{x:21,z:21,garrison:410,stuck:2,drift:true,reverse:{x:10,z:10},path:[{x:30,z:30}],repath:1})],[dig('wire',{x:23,z:21,dir:0})],{map});}
 {const map=blank();map.rows[10]='........NNNN'+'.'.repeat(28);record('demine-cached-canonical-goal',[unit(1,'engineer',{x:1,z:21})],[{op:'seen',cell:408,mask:1},{op:'seen',cell:409,mask:1},dig('demine',{dir:0}),{op:'goal',id:1},{op:'prime',id:1,fields:{x:2}},{op:'goal',id:1}],{map});}
 record('actual-dig-route-request',[unit(1,'rifle',{x:5,z:20})],[dig(),{op:'begin',tick:10,budget:1},{op:'phase',id:1}]);
 record('queued-dig-route-served',[unit(1,'rifle',{x:5,z:20})],[dig(),{op:'begin',tick:10,budget:0},{op:'phase',id:1},{op:'begin',tick:11,budget:1}]);
 record('queued-dig-route-cancelled',[unit(1,'rifle',{x:5,z:20})],[dig(),{op:'begin',tick:10,budget:0},{op:'phase',id:1},{op:'command',cmd:{t:'stop',ids:[1]}},{op:'begin',tick:11,budget:1}]);
 record('failed-dig-route-drops-job',[unit(1,'rifle',{x:5,z:21})],[dig(),{op:'mutate',cells:[361,362,363,401,403,441,442,443],ch:'W'},{op:'begin',tick:10,budget:1,retry:1},{op:'phase',id:1}]);
 clean();const content=JSON.stringify({reference:pin,node:process.version,arch:process.arch,traces})+'\n',path=join(root,'rust/crates/core/tests/fixtures/match-engineering.json');
 if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),content);else writeFileSync(path,content);
 console.log(`${process.argv.includes('--check')?'Verified':'Recorded'} ${traces.length} Match engineering traces, ${traces.reduce((n,t)=>n+t.rows.length,0)} actions`);
} finally {Math.random=originalRandom;rmSync(temp,{recursive:true,force:true});}
