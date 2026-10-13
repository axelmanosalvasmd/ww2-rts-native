// Record actual ground carrier commands and unchanged early/boarding unit blocks.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdtempSync,rmSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {fileURLToPath,pathToFileURL} from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
const root=fileURLToPath(new URL('../',import.meta.url)),ref=resolve(process.env.WW2_REFERENCE??join(root,'reference/ww2-rts'));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d',git=a=>execFileSync('git',['-C',ref,...a],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};clean();
const temp=mkdtempSync(join(tmpdir(),'ww2-carriers-')),oldRandom=Math.random;
try{
 let code=readFileSync(join(ref,'shared/sim.js'),'utf8');
 const extract=(start,end)=>{const a=code.indexOf(start),b=code.indexOf(end,a);assert(a>=0&&b>a);return code.slice(a,b);};
 const earlyBlock=extract('    // riding in a carrier:', '    if (def.infantry) u.supp'),boarding=extract('    // boarding: walk up to the carrier,', '    // attack-move: halt');
 const marker='  setupStructures(g, map);';assert.equal(code.split(marker).length,2);
 // Isolate the same authored map/structure checkpoint used by native fixture setup.
 code=code.replace(marker,marker+'\n  return g;').replace(/from '(\.\/[^']+)'/g,(_,p)=>`from '${pathToFileURL(join(ref,'shared',p)).href}'`);
 code+=`\nexport const carrierHooks={early(g,id,dt){for(const u of [g.units.get(id)]){${earlyBlock}}},boarding(g,id){for(const u of [g.units.get(id)]){${boarding}}},beginPathTick,work(g){return pathWork.get(g)},flags:unitFlags};\n`;
 writeFileSync(join(temp,'sim.mjs'),code);const sim=await import(pathToFileURL(join(temp,'sim.mjs'))),grid=await import(pathToFileURL(join(ref,'shared/grid.js')));
 const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex')},pt=p=>p?[bits(p.x),bits(p.z)]:null;
 const map=(w=40,h=30)=>({name:'Carrier checkpoint',w,h,rows:Array(h).fill('.'.repeat(w)),spawns:[{x:0,y:0},{x:w-1,y:h-1}],points:[{x:w>>1,y:h>>1}]}),unit=(id,type,x,z,extra={})=>({id,type,x,z,...extra}),traces=[];
 function record(name,units,actions,terrain=map()){
  Math.random=()=>.5;const g=sim.createGame(terrain,[],false,[],[],{weather:false});Math.random=oldRandom;
  g.players=[0,1].map(slot=>({slot,team:slot,faction:slot,mp:500,mun:0,fuel:0,out:false,away:false,visible:new Set(),spawn:{x:1+slot*(terrain.w*2-2),z:1+slot*(terrain.h*2-2)}}));g.tick=10;g.tech=false;g.seed=123;g.wx={rain:0,wet:0};
  for(const row of units){const d=sim.UNITS[row.type];g.units.set(row.id,{owner:0,rot:0,hp:d.models*d.hpPer,supp:0,path:[],orders:[],riding:0,cargo:[],xp:0,garrison:-1,follow:77,auto:false,autoRetreat:false,holdFire:false,holdPos:false,attackId:0,targetId:0,cd:0,buff:0,ap:false,sprint:0,nade:null,cooldown:0,retarget:0,repath:0,stuck:0,retreating:false,dig:null,build:0,enter:-1,board:0,amove:null,face:null,entrench:null,fireAt:-1,drift:false,reverse:null,traffic:null,trafficWait:0,worldGoal:null,still:0,...structuredClone(row)});}
  grid.rebuildGrid(g);
  const snapshot=()=>({units:[...g.units.values()].sort((a,b)=>a.id-b.id).map(u=>({id:u.id,kind:u.type,owner:u.owner,pose:[u.x,u.z,u.rot].map(bits),hp:bits(u.hp),supp:bits(u.supp),still:bits(u.still),cd:bits(u.cd),cooldown:bits(u.cooldown),retarget:bits(u.retarget),repath:bits(u.repath),board:u.board,riding:u.riding,cargo:[...u.cargo],path:u.path.map(pt),follow:u.follow,attack:u.attackId,target:u.targetId,work:!!u.dig,orders:u.orders.length,face:!!u.face,nade:pt(u.nade),enter:u.enter,fireAt:u.fireAt,build:!!u.build,entrench:!!u.entrench,worldGoal:pt(u.worldGoal),amove:pt(u.amove),traffic:!!u.traffic,trafficWait:bits(u.trafficWait),outcome:u.moveOutcome??null,outcomeTick:u.moveOutcomeTick??null,flags:sim.carrierHooks.flags(g,u)&(262144|524288|(3<<23)),grid:grid.gridFor(g).candidates(u,6).map(v=>v.id)})),money:g.players.map(p=>bits(p.mp)),shots:g.shots});
  const initial=snapshot(),rows=[];
  for(const action of actions){let result=null;
   if(action.op==='command')result=sim.command(g,action.owner??0,action.cmd)??null;
   else if(action.op==='prime'){Object.assign(g.units.get(action.id),structuredClone(action.fields));grid.rebuildGrid(g);}
   else if(action.op==='mutate')sim.internals.setCell(g,action.cell,action.ch);
   else if(action.op==='early')sim.carrierHooks.early(g,action.id,action.dt??.05);
   else if(action.op==='phase'){
    g.pathBudget=0;sim.carrierHooks.beginPathTick(g);const u=g.units.get(action.id),before=u.board;
    sim.carrierHooks.boarding(g,action.id);const pending=sim.carrierHooks.work(g).queued.get(action.id);
    result=u.riding?{boarded:u.riding}:before&&!u.board?'cleared':pending?{needsPath:u.board,at:pt(g.units.get(u.board))}:'idle';
   }else throw Error(action.op);
   rows.push({action,result,snapshot:snapshot()});
  }
  traces.push({name,map:terrain,units,initial,rows});
 }
 const board=(ids,target)=>({op:'command',cmd:{t:'board',ids,...(target===undefined?{}:{target})}}),unload=ids=>({op:'command',cmd:{t:'unload',ids}}),phase=id=>({op:'phase',id}),early=id=>({op:'early',id});
 record('targeted-board-ride-unload',[unit(1,'rifle',20,20),unit(2,'halftrack',24,20)],[board([1],2),phase(1),{op:'prime',id:2,fields:{x:35,z:23,rot:.75}},early(1),unload([2])]);
 record('nearest-pairs-recorded-ties',[unit(1,'rifle',20,20),unit(2,'rifle',20,20),unit(3,'halftrack',18,20),unit(4,'halftrack',22,20)],[board([2,1,4,3]),phase(2),phase(1),early(2),early(1),unload([4,3])]);
 record('own-seat-reservations',[unit(1,'rifle',20,20),unit(2,'rifle',18,20,{board:4}),unit(3,'rifle',20,18,{board:4,owner:1}),unit(4,'tank',22,20)],[board([1],4),{op:'prime',id:3,fields:{board:0}},board([1],4),phase(1)]);
 record('target-and-selection-denials',[unit(1,'rifle',20,20),unit(2,'halftrack',24,20),unit(3,'halftrack',25,20,{owner:1}),unit(4,'halftrack',25,23,{hp:0})],[board([1],-1),board([1],0),board([1],3),board([1],4),board([1]),board([2],2),unload([2])]);
 record('retreating-squad-and-full-carrier',[unit(1,'rifle',20,20,{retreating:true}),unit(2,'tank',24,20),unit(3,'rifle',24,20,{riding:2}),unit(4,'rifle',24,20,{riding:2})],[{op:'prime',id:2,fields:{cargo:[3,4]}},board([1],2),{op:'prime',id:1,fields:{retreating:false}},board([1],2),unload([2]),board([1],2),phase(1)]);
 record('reachable-and-moving-target-request',[unit(1,'rifle',20,20),unit(2,'halftrack',40,20)],[board([1],2),{op:'prime',id:1,fields:{path:[],repath:0}},phase(1),{op:'prime',id:1,fields:{x:38,z:20,path:[]}},phase(1)]);
 record('blocked-boarding',[unit(1,'rifle',20,20),unit(2,'halftrack',24,20)],[board([1],2),{op:'mutate',cell:10*40+11,ch:'B'},{op:'prime',id:1,fields:{path:[]}},phase(1)]);
 record('carrier-missing-or-full-clears',[unit(1,'rifle',20,20,{board:999}),unit(2,'tank',24,20),unit(3,'rifle',24,20,{riding:2}),unit(4,'rifle',24,20,{riding:2})],[phase(1),{op:'prime',id:1,fields:{board:2}},{op:'prime',id:2,fields:{cargo:[3,4]}},phase(1)]);
 record('stale-cargo-cleanup-and-unload',[unit(1,'rifle',24,20,{riding:2}),unit(2,'halftrack',24,20,{cargo:[1,999]}),unit(3,'rifle',20,20)],[early(2),unload([2]),early(2)]);
 record('missing-cargo-unload-defers-pruning',[unit(1,'rifle',20,20),unit(2,'halftrack',24,20,{cargo:[999]})],[board([1],2),unload([2]),board([1],2),early(2),board([1],2),phase(1)]);
 record('boarding-cancels-paid-real-dig',[unit(1,'rifle',20,23),unit(2,'halftrack',24,23)],[{op:'command',cmd:{t:'dig',ids:[1],kind:'trench',x:20,z:20,dir:0}},board([1],2),phase(1),unload([2])]);
 record('full-carrier-keeps-paid-dig',[unit(1,'rifle',20,23),unit(2,'halftrack',24,23,{cargo:[3]}),unit(3,'rifle',24,23,{riding:2})],[{op:'command',cmd:{t:'dig',ids:[1],kind:'trench',x:20,z:20,dir:0}},board([1],2),unload([2]),board([1],2),phase(1)]);
 record('hidden-obstacle-owner-route',[unit(1,'rifle',20,20),unit(2,'halftrack',170,20)],[{op:'mutate',cell:10*100+45,ch:'B'},board([1],2)],map(100,40));
 record('duplicate-inputs-do-not-reserve-twice',[unit(1,'rifle',20,20),unit(2,'halftrack',24,20)],[board([1,1,2,2]),phase(1),early(1),unload([2,2])]);
 const output=JSON.stringify({reference:pin,traces},null,2)+'\n',path=join(root,'rust/crates/core/tests/fixtures/carriers.json');clean();
 if(process.argv.includes('--check')){assert.equal(readFileSync(path,'utf8'),output);console.log(`Verified ${traces.length} carrier traces, ${traces.reduce((n,c)=>n+c.rows.length,0)} actions`)}else{writeFileSync(path,output);console.log(`Recorded ${traces.length} carrier traces, ${traces.reduce((n,c)=>n+c.rows.length,0)} actions`)}
}finally{Math.random=oldRandom;rmSync(temp,{recursive:true,force:true});}
