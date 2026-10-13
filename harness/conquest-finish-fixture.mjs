// Execute original Conquest point, sample, victory and finish code on createGame state.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const clean=()=>{if(execFileSync('git',['rev-parse','HEAD'],{cwd:reference,encoding:'utf8'}).trim()!==pin||execFileSync('git',['status','--porcelain','--untracked-files=no'],{cwd:reference,encoding:'utf8'}).trim())throw Error('Pinned clean reference required');};
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');clean();
registerHooks({load(url,context,next){const result=next(url,context);if(url!==pathToFileURL(`${reference}/shared/sim.js`).href)return result;let source=String(result.source);
const pointStart='  // teammates standing on it keep it theirs.',pointEnd='  stepConvoys(g, dt, supplyHooks(g));';
const victory="  for (const pl of g.players) if (teamVp(pl.team) >= g.winVp && g.winner === null) finish(g, pl.team, 'vp', lastCapture(g, pl.team)); // winner = team id";
const periodic='  if (g.winner === null && (g.tick === 1 || g.tick % SAMPLE_EVERY === 0)) sample(g, isStructure(g));';
for(const anchor of [pointStart,pointEnd,victory,periodic])if(source.split(anchor).length!==2)throw Error('Conquest source anchor moved');
source+=`\nexport function __points(g,list,dt){${source.slice(source.indexOf(pointStart),source.indexOf(pointEnd))}}\nexport function __victory(g){const teamVp = (t) => g.players.reduce((a, q) => a + (q.team === t ? q.vp : 0), 0);\n${victory}\n}\nexport function __periodic(g){${periodic}}\n`;
return{...result,source};}});
const sim=await import(pathToFileURL(`${reference}/shared/sim.js`));
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const map={name:'Conquest finish',w:20,h:20,rows:Array(20).fill('.'.repeat(20)),spawns:[{x:1,y:1},{x:18,y:18},{x:1,y:18},{x:18,y:1}],points:[{id:'first',x:8,y:8},{id:'second',x:11,y:8},{id:'third',x:8,y:11}]};
const capture=g=>({winner:g.winner,reason:g.endReason??null,endAt:g.endAt?{x:bits(g.endAt.x),z:bits(g.endAt.z)}:null,endTick:g.endTick??null,reveal:!!g.reveal,timeline:(g.timeline??[]).map(s=>({t:bits(s.t),vp:s.vp.map(bits)})),captures:g.players.map((_,i)=>bits(g.story?.[i]?.captures??0)),players:g.players.map(p=>[bits(p.mp),bits(p.vp),bits(p.inc)]),captureTicks:g.points.map(p=>p.capTick??null)});
const cases=[];
function trace(name,teams,actions){const old=Math.random;Math.random=()=>0.5;let g;try{g=sim.createGame(map,teams.map((_,i)=>`P${i}`),false,teams,teams.map((_,i)=>i%4),{weather:false,tech:true,logistics:false,supply:false});}finally{Math.random=old;}const steps=[];
for(const action of actions){g.tick=action.tick;
for(const [i,value]of action.vp??[])g.players[i].vp=value;
for(const [i,value]of action.out??[])g.players[i].out=value;
for(const [i,value]of action.away??[])g.players[i].away=value;
for(const p of action.points??[]){Object.assign(g.points[p.index],{owner:p.owner,progress:p.progress??(p.owner>=0?1:0),capper:p.capper??p.owner});if('capTick'in p)g.points[p.index].capTick=p.capTick;if(p.at)Object.assign(g.points[p.index],p.at);}
if(action.kill)for(const u of g.units.values())u.hp=0;
for(const [id,x,z]of action.moves??[])Object.assign(g.units.get(id),{x,z});
if(action.periodic)sim.__periodic(g);
if(action.pointsPhase)sim.__points(g,[...g.units.values()],sim.TICK);
sim.__victory(g);steps.push({action,expected:capture(g)});}
cases.push({name,map,teams,steps});}
trace('capture-to-victory',[0,1],[{tick:1,periodic:true,moves:[[1,17,17],[2,17,17],[3,17,17]],points:[{index:0,owner:-1,progress:0.995,capper:0}],pointsPhase:true},{tick:160,moves:[[1,17,17],[2,17,17],[3,17,17]],points:[{index:0,owner:0,capTick:159}],vp:[[0,1199.99]],pointsPhase:true},{tick:200,periodic:true,vp:[[0,2400],[1,4800]],pointsPhase:true},{tick:400,periodic:true,kill:true,pointsPhase:true}]);
trace('allied-team-sum',[2,5,2],[{tick:1,periodic:true},{tick:40,vp:[[0,1200],[2,1200]],points:[{index:0,owner:2,capTick:10},{index:1,owner:0,capTick:20}]},{tick:200,periodic:true,vp:[[1,9999]]}]);
trace('simultaneous-slot-order',[5,2],[{tick:1,vp:[[0,1200],[1,1200]],points:[{index:0,owner:1,capTick:50},{index:1,owner:0,capTick:40}]}]);
trace('latest-still-held-point',[0,1],[{tick:200,periodic:true,vp:[[0,1200.7]],points:[{index:0,owner:0,capTick:20},{index:1,owner:1,capTick:100},{index:2,owner:0,capTick:40,at:{x:17.25,z:-0.05}}]}]);
trace('stable-capture-tie',[0,1],[{tick:33,vp:[[0,1200]],points:[{index:0,owner:0,capTick:20,at:{x:-0.15,z:17.35}},{index:1,owner:0,capTick:20}]}]);
trace('missing-capture-times',[0,1],[{tick:45,vp:[[0,1200]],points:[{index:0,owner:0},{index:1,owner:0}]}]);
trace('map-center-fallback',[0,1],[{tick:8,vp:[[0,1200]],points:[]}]);
trace('losses-not-elimination',[0,1],[{tick:1,periodic:true,kill:true},{tick:200,periodic:true,vp:[[0,1199.999],[1,0]]},{tick:201,vp:[[0,1200]]}]);
trace('out-away-scores-still-count',[0,1],[{tick:15,out:[[0,true]],away:[[0,true]],vp:[[0,1200]]}]);
trace('same-tick-periodic-and-finish',[0,1],[{tick:200,periodic:true,vp:[[1,1200.5]]},{tick:400,periodic:true,vp:[[0,9999]]}]);
trace('single-team-goal',[3],[{tick:1,periodic:true,vp:[[0,1199.99]]},{tick:10,vp:[[0,1200]]}]);
trace('new-capture-story-freezes',[0,1],[{tick:1,periodic:true,points:[{index:0,owner:-1,progress:0.995,capper:0}],moves:[[1,17,17],[2,17,17],[3,17,17]],pointsPhase:true},{tick:2,points:[{index:0,owner:-1,progress:0.995,capper:0}],moves:[[1,17,17],[2,17,17],[3,17,17]],pointsPhase:true},{tick:3,vp:[[0,1200]]},{tick:4,pointsPhase:true,points:[{index:0,owner:-1,progress:0.995,capper:0}],moves:[[1,17,17],[2,17,17],[3,17,17]]}]);
clean();const output=JSON.stringify({format:1,reference:pin,node:process.version,arch:process.arch,cases})+'\n';const path=`${root}/rust/crates/simtest/tests/fixtures/conquest-finish.json`;if(process.argv.includes('--check')){if(readFileSync(path,'utf8')!==output)throw Error('Conquest finish fixture differs');}else writeFileSync(path,output);console.log(`Verified ${cases.length} native-Conquest comparison traces, ${cases.reduce((n,c)=>n+c.steps.length,0)} phases`);
