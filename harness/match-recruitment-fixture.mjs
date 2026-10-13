// Actual command -> produceUnit -> placement -> observed findPath and grid updates.
// The loader adds observations/exports; original command/helper bodies stay intact.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node 24.21.0');
const root=fileURLToPath(new URL('../',import.meta.url)),ref=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`),pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',ref,...a],{encoding:'utf8'}).trim();
if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Use clean pinned reference');
const simUrl=pathToFileURL(`${ref}/shared/sim.js`).href;
registerHooks({load(url,c,next){const r=next(url,c);if(url!==simUrl)return r;const anchor='function produceUnit(g, b, owner, type) {';if(String(r.source).split(anchor).length!==2)throw Error('Production probe anchor');return{...r,source:String(r.source).replace(anchor,anchor+'\n  g.__producer = b.id;')+'\nexport {knownTerrainFor,gridFor};\n'};}});
const sim=await import(simUrl);
const bits=x=>{const b=Buffer.alloc(8);b.writeDoubleBE(x);return b.toString('hex');},point=p=>p?[bits(p.x),bits(p.z)]:null;
const base={name:'Recruit integration',w:40,h:30,rows:Array(30).fill('.'.repeat(40)),spawns:[{x:3,y:3},{x:35,y:25}],points:[{id:"front",x:10,y:15}]};
const cases=[];
function run(name,actions,map=base){
 const old=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(map,['A','B'],false,[0,1],[0,1],{mode:'conquest',weather:false,tech:false,logistics:false});}finally{Math.random=old;}
 g.players[0].mp=5000.125;g.skipFog=true;
 const revisions=new WeakMap(),rowPool=[];let nextRevision=0;
 const stages=[];
 const resolveId=a=>a.id??[...g.units.values()].find(u=>u.owner===(a.owner??0)&&u.type===(a.maker??'hq'))?.id;
 for(const original of actions){const a=structuredClone(original);g.tick++;
  if(a.op==='player')Object.assign(g.players[a.slot??0],a.fields);
  if(a.op==='maker'){a.id=resolveId(a);Object.assign(g.units.get(a.id),a.fields);}
  if(a.op==='rally'){a.id=resolveId(a);g.units.get(a.id).rally=a.to;}
  if(a.op==='winner')g.winner=a.value?0:null;
  if(a.op!=='buy'){stages.push({action:a});continue;}
  const slot=a.slot??0;if(a.maker)a.from=resolveId(a);
  const view=sim.knownTerrainFor(g,slot),memory=[...g.players[slot].terrainMemory.values()];
  const routing={rows:memory.map(row=>{if(!revisions.has(row)){revisions.set(row,++nextRevision);rowPool.push({revision:nextRevision,row:structuredClone(row)});}return revisions.get(row);}),fires:[...view.fires.keys()],bunkers:[...view.units.values()].filter(u=>u.type==='bunker'&&u.hp>0).map(u=>({x:u.x,z:u.z})),wet:view.wx?.wet??0,roads:!!view.roads};
  const before=g.nextId,failed=g.pathStats?.failed??0;g.__producer=null;
  const result=sim.command(g,slot,{t:'buy',unit:a.kind,...(a.from===undefined?{}:{from:a.from})})??null;
  const u=g.units.get(before);
  const unit=u?{id:u.id,owner:u.owner,kind:u.type,pose:[u.x,u.z,u.rot,u.aim,u.ghost].map(bits),hp:bits(u.hp),path:u.path.map(point),goal:point(u.worldGoal),routeEnd:point(u.routeEnd),drift:u.drift==='rally',auto:u.auto,autoRetreat:u.autoRetreat,grid:sim.gridFor(g).candidates(u,20).map(v=>v.id),producer:g.__producer,routeFailed:(g.pathStats?.failed??0)>failed}:null;
  stages.push({action:a,routing,expected:{result,next:g.nextId,money:g.players.map(p=>bits(p.mp)),unit}});
 }
 cases.push({name,map,rows:rowPool,stages});
}
run('instant-maker-and-order',[
 {op:'buy',kind:'rifle'},{op:'buy',kind:'rifle',maker:'barracks'},{op:'buy',kind:'mg'},
 {op:'buy',kind:'rifle',from:999},{op:'buy',kind:'rifle',maker:'hq',owner:1},{op:'buy',kind:'engineer'},{op:'buy',kind:'tiger'},
 {op:'maker',maker:'hq',fields:{hp:0}},{op:'buy',kind:'rifle'},{op:'maker',maker:'barracks',fields:{built:.5}},{op:'buy',kind:'rifle'},
]);
run('personal-and-building-rally',[
 {op:'player',fields:{rally:{x:45,z:35}}},{op:'buy',kind:'rifle'},
 {op:'rally',maker:'barracks',to:{x:17,z:43}},{op:'buy',kind:'rifle',maker:'barracks'},
 {op:'maker',maker:'hq',fields:{type:'motorpool'}},{op:'buy',kind:'tank'},{op:'buy',kind:'halftrack'},
]);
run('source-command-gates',[
 {op:'player',fields:{away:true}},{op:'buy',kind:'rifle'},{op:'player',fields:{away:false,out:true}},{op:'buy',kind:'rifle'},
 {op:'player',fields:{out:false}},{op:'winner',value:true},{op:'buy',kind:'rifle'},{op:'winner',value:false},
 {op:'player',fields:{mp:0}},{op:'buy',kind:'rifle'},{op:'player',fields:{mp:5000.125}},
 ...Array.from({length:10},()=>({op:'buy',kind:'rifle'})),
]);
run('unreachable-rally-still-buys',[{op:'player',fields:{rally:{x:65,z:35}}},{op:'buy',kind:'rifle'},{op:'rally',maker:'barracks',to:{x:63,z:33}},{op:'buy',kind:'rifle',maker:'barracks'}],{...base,rows:Array(30).fill('.'.repeat(20)+'B'+'.'.repeat(19))});
run('naval-producer',[{op:'maker',maker:'hq',fields:{type:'shipyard'}},{op:'buy',kind:'lcvp'},{op:'buy',kind:'gunboat'},{op:'player',fields:{rally:{x:55,z:35}}},{op:'buy',kind:'destroyer'}],{...base,naval:true,rows:Array(30).fill('.'.repeat(18)+'W'.repeat(22)),spawns:[{x:3,y:3},{x:3,y:25}]});
const data=JSON.stringify({reference:pin,cases})+'\n',file=`${root}/rust/crates/simtest/tests/fixtures/match-recruitment.json`;
if(process.argv.includes('--check')){if(readFileSync(file,'utf8')!==data)throw Error('Match recruitment fixture differs');}else writeFileSync(file,data);
console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} ${cases.length} Match traces, ${cases.reduce((n,c)=>n+c.stages.filter(s=>s.action.op==='buy').length,0)} original buy commands`);
