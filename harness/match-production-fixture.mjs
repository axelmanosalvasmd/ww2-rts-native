// Original command/research/training helpers on real constructors. Full ticks are
// labeled separately; ordinary Conquest never receives a nonempty training queue.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node 24.21.0');
const root=fileURLToPath(new URL('../',import.meta.url)),ref=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`),pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',ref,...a],{encoding:'utf8'}).trim();
if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Use clean pinned reference');
const url=pathToFileURL(`${ref}/shared/sim.js`).href;
registerHooks({load(u,c,next){const r=next(u,c);return u===url?{...r,source:String(r.source)+'\nexport {train,stepLab,placeBuilding,setCell,gridFor};\n'}:r;}});
const sim=await import(url),bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');},point=p=>p?[bits(p.x),bits(p.z)]:null;
const ordinary={name:'Production research',w:40,h:30,fill:'.',spawns:[{x:3,y:3},{x:35,y:25}],points:[{id:'front',x:20,y:15}]};
const world={name:'Compact legal World training',w:256,h:256,fill:'.',spawns:[{x:16,y:16},{x:240,y:240}],points:[],world:{size:'huge',total:64,regionCells:64,seed:701,regions:Array.from({length:64},(_,id)=>{const x=(id%8)*32+16,y=Math.floor(id/8)*32+16;return{id,name:`Region ${id}`,x,y,kind:'rural',bounds:[x-16,y-16,x+16,y+16],...(id===0?{home:0}:id===63?{home:1}:{})};})}};
const cases=[];let commands=0,labs=0,trains=0,ticks=0;
function run(name,map,actions){
 const input={...structuredClone(map),rows:Array(map.h).fill(map.fill.repeat(map.w))};delete input.fill;
 const settings={mode:map.world?'world':'conquest',tech:true,weather:false,logistics:false};
 const old=Math.random;Math.random=()=>.5;
 try{
  const g=sim.createGame(input,['A','B'],false,[0,1],[0,1],settings);
  const initial=[...g.units.keys()],initialNext=g.nextId;
  for(const p of g.players)Object.assign(p,{mp:5000.125,mun:500.5,fuel:500.25});
  const makers=[...g.units.values()].filter(u=>u.owner>=0&&u.queue).map(u=>u.id);
  const stages=[];
  const id=a=>a.id??[...g.units.values()].find(u=>u.owner===(a.slot??0)&&u.type===(a.maker??'hq'))?.id;
  function projection(){return{tick:g.tick,next:g.nextId,nextJob:g.nextProductionJob??0,mode:g.mode?.kind??null,winner:g.winner,
   players:g.players.map(p=>({slot:p.slot,away:!!p.away,out:!!p.out,mp:bits(p.mp),mun:bits(p.mun),fuel:bits(p.fuel),tier:p.tier,armory:{...p.armory},lab:p.lab.map(j=>({kind:j.kind,progress:bits(j.prog),time:bits(j.time)})),spent:bits(g.story?.[p.slot]?.mpSpent??0)})),
   buildings:makers.filter(n=>g.units.has(n)).map(n=>{const b=g.units.get(n);return{id:n,owner:b.owner,kind:b.type,hp:bits(b.hp),built:bits(b.built),queue:[...b.queue],jobs:(b.productionJobs??[]).map(j=>({...j})),progress:bits(b.prog),rally:point(b.rally)};}),
   created:[...g.units.values()].filter(u=>u.id>=initialNext).map(u=>({id:u.id,owner:u.owner,kind:u.type,hp:bits(u.hp),pose:[u.x,u.z,u.rot,u.aim,u.ghost??0].map(bits),goal:point(u.worldGoal),routeEnd:point(u.routeEnd),path:u.path.map(point),rally:u.drift==='rally',auto:u.auto,autoRetreat:u.autoRetreat,grid:sim.gridFor(g).candidates(u,12).map(v=>v.id)}))};}
  const start=projection();
  for(const raw of actions){const a=structuredClone(raw);let result=null,executed=true;
   switch(a.op){
    case 'command':{commands++;const c={...a.command};if(a.fromMaker)c.from=id({maker:a.fromMaker,slot:a.slot});result=sim.command(g,a.slot??0,c)??null;break;}
    case 'lab-helper':labs++;sim.stepLab(g,g.players[a.slot??0],a.dt);break;
    case 'tick':ticks++;sim.step(g);break;
    case 'player':Object.assign(g.players[a.slot??0],a.fields);break;
    case 'winner':g.winner=a.value?0:null;break;
    case 'maker':a.id=id(a);Object.assign(g.units.get(a.id),a.fields);break;
    case 'finished-building':{const b=sim.placeBuilding(g,a.slot??0,a.kind,a.cell,true);a.id=b.id;makers.push(b.id);break;}
    case 'rally':a.id=id(a);g.units.get(a.id).rally=a.to;break;
    case 'cell':sim.setCell(g,a.cell,a.character);break;
    case 'train-helper':{
     a.id=id(a);const b=g.units.get(a.id);
     // The literal per-building guard from step. It is not a complete tick.
     executed=b.hp>0&&b.queue.length>0&&b.built>=1&&!g.mode.suddenDeath;
     if(executed){trains++;sim.train(g,b,a.dt);}break;
    }
    default:throw Error(`Unknown action ${a.op}`);
   }
   stages.push({action:a,result,executed,expected:projection()});
  }
  cases.push({name,map,settings,initialIds:initial,start,stages});
 }finally{Math.random=old;}
}
run('conquest-research-pause-and-tick-gates',ordinary,[
 {op:'command',command:{t:'tech',kind:'tier'}},{op:'lab-helper',dt:20},
 {op:'maker',fields:{hp:0}},{op:'lab-helper',dt:100},{op:'maker',fields:{hp:2200}},
 {op:'player',fields:{away:true}},{op:'lab-helper',dt:40},{op:'command',command:{t:'tech',kind:'tier'}},
 {op:'player',fields:{away:false}},{op:'command',command:{t:'tech',kind:'tier'}},
 {op:'player',fields:{out:true}},{op:'tick'},{op:'player',fields:{out:false,away:true}},
 {op:'winner',value:true},{op:'tick'},{op:'player',fields:{away:false}},{op:'command',command:{t:'tech',kind:'inf'}},
]);
run('conquest-armory-order-and-no-lab',ordinary,[
 {op:'command',command:{t:'tech',kind:'inf'}},
 {op:'finished-building',kind:'armory',cell:8*40+8},{op:'finished-building',kind:'armory',cell:8*40+18},
 {op:'command',command:{t:'tech',kind:'inf'}},{op:'command',command:{t:'tech',kind:'guns'}},{op:'command',command:{t:'tech',kind:'armor'}},
 {op:'maker',maker:'armory',fields:{hp:0}},{op:'lab-helper',dt:31},{op:'maker',maker:'armory',fields:{hp:1500}},{op:'lab-helper',dt:31},{op:'lab-helper',dt:10},
]);
run('world-legal-queue-single-completion',world,[
 {op:'command',command:{t:'buy',unit:'engineer'},fromMaker:'hq'},{op:'command',command:{t:'buy',unit:'engineer'},fromMaker:'hq'},
 {op:'tick'},{op:'train-helper',dt:3},{op:'train-helper',dt:1000},{op:'train-helper',dt:12},{op:'train-helper',dt:12},
]);
run('world-training-rally-and-guards',world,[
 {op:'rally',to:{x:57,z:33}},{op:'command',command:{t:'buy',unit:'engineer'},fromMaker:'hq'},
 {op:'maker',fields:{built:.5}},{op:'train-helper',dt:12},{op:'maker',fields:{built:1}},
 {op:'player',fields:{away:true,out:true}},{op:'winner',value:true},{op:'train-helper',dt:12},
]);
run('world-disconnected-rally-still-produces',world,[
 ...[-1,0,1].flatMap(dy=>[-1,0,1].filter(dx=>dx||dy).map(dx=>({op:'cell',cell:(16+dy)*256+28+dx,character:'B'}))),
 {op:'rally',to:{x:57,z:33}},{op:'command',command:{t:'buy',unit:'engineer'},fromMaker:'hq'},{op:'train-helper',dt:12},
]);
const data=JSON.stringify({reference:pin,scope:{ordinary:'Original tech commands and unmodified stepLab; full ticks only with empty production queues. Ordinary Conquest has no mode object and cannot legally queue training.',world:'Original World constructor, buy commands and unmodified train called under the source building guard. Helper dt is not a full tick; tick actions execute original step.',fixtureSetup:'Finished extra armories use original placeBuilding as explicit setup, not a paid construction command.',random:'Math.random fixed at 0.5 during constructors and full ticks; weather and logistics disabled.'},counts:{cases:cases.length,commands,labHelpers:labs,trainHelpers:trains,fullTicks:ticks},cases})+'\n';
const file=`${root}/rust/crates/simtest/tests/fixtures/match-production.json`;
if(process.argv.includes('--check')){if(readFileSync(file,'utf8')!==data)throw Error('Match production fixture differs');}else writeFileSync(file,data);
console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} ${cases.length} cases, ${commands} actual commands, ${labs} research helpers, ${trains} training helpers and ${ticks} full ticks`);
