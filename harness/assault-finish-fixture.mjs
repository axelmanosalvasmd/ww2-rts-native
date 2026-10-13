// Execute unchanged source Assault late phase, periodic sample, finish and story callbacks.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const clean=()=>{if(execFileSync('git',['rev-parse','HEAD'],{cwd:reference,encoding:'utf8'}).trim()!==pin||execFileSync('git',['status','--porcelain','--untracked-files=no'],{cwd:reference,encoding:'utf8'}).trim())throw Error('Pinned tracked-clean reference required');};
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');clean();
registerHooks({load(url,context,next){const result=next(url,context);if(url!==pathToFileURL(`${reference}/shared/sim.js`).href)return result;let source=String(result.source);
const start='  if (g.mode) {\n    // assault: the attackers win when the last bunker falls, the defenders when the clock runs out';
const end='  for (const pl of g.players) if (teamVp(pl.team) >= g.winVp';
const periodic='  if (g.winner === null && (g.tick === 1 || g.tick % SAMPLE_EVERY === 0)) sample(g, isStructure(g));';
for(const anchor of [start,end,periodic])if(source.split(anchor).length!==2)throw Error('Assault finish source anchor moved');
source+=`\nexport function __assaultFinish(g,dt){${source.slice(source.indexOf(start),source.indexOf(end))}}\nexport function __periodic(g){${periodic}}\n`;
return{...result,source};}});
const sim=await import(pathToFileURL(`${reference}/shared/sim.js`));
const story=await import(pathToFileURL(`${reference}/shared/story.js`));
const {UNITS}=sim;
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const encode=v=>typeof v==='number'?bits(v):Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)])):v;
const map={name:'Assault finish',w:30,h:30,rows:Array(30).fill('.'.repeat(30)),spawns:[{x:2,y:2},{x:27,y:27},{x:2,y:27}],defend:[1,2],points:[{id:'A',x:10,y:10,mp:2,vp:0},{id:'B',x:20,y:10,mp:3,vp:0}]};
const invalid=sim.validateMap(map);if(invalid!==null)throw Error(`Invalid controlled map: ${invalid}`);
const cases=[];
function trace(name,teams,defender,actions){const old=Math.random;Math.random=()=>0.5;let g;try{g=sim.createGame(map,teams.map((_,i)=>`P${i}`),false,teams,teams.map((_,i)=>i%4),{mode:'assault',defenderTeam:defender,weather:false,tech:true,supply:false,logistics:false});}finally{Math.random=old;}
const steps=[];
for(const action of actions){g.tick=action.tick;
if('timeLeft'in action)g.mode.timeLeft=action.timeLeft;
for(const [i,owner]of action.points??[])g.points[i].owner=owner;
for(const [i,out]of action.out??[])g.players[i].out=out;
for(const [owner,hp]of action.bunkers??[])for(const u of g.units.values())if(u.type==='bunker'&&(owner===null||u.owner===owner))u.hp=hp;
if(action.fallen)g.fallen={...(g.fallen??{}),bunker:action.fallen};
if(action.neutral) { const b=[...g.units.values()].find(u=>u.type==='bunker'); if(b)b.owner=-1; }
if(action.death){const u={owner:action.death.owner,type:'bunker',...action.death.at};story.died(g,u,UNITS.bunker,action.death.by);}
if(action.spend)story.tally(g,action.spend[0],'mpSpent',action.spend[1]);
if(action.periodic)sim.__periodic(g);
sim.__assaultFinish(g,action.dt??sim.TICK);
steps.push({action,expected:encode({timeLeft:g.mode.timeLeft,winner:g.winner,reason:g.endReason??null,endAt:g.endAt??null,endTick:g.endTick??null,reveal:!!g.reveal,timeline:g.timeline??[],story:g.players.map((_,i)=>g.story?.[i]??Object.fromEntries(story.STORY_KEYS.map(k=>[k,0]))),fallen:g.fallen??{},report:g.winner===null?null:story.storyResult(g)})});}
cases.push({name,map,teams,defender,represented:!actions.some(a=>a.neutral),steps});}
trace('timer-and-decided-freeze',[0,1],1,[{tick:1,periodic:true,spend:[0,2.5],points:[[0,0],[1,1]]},{tick:200,periodic:true,timeLeft:.025},{tick:400,periodic:true,dt:100,bunkers:[[null,0]],spend:[0,99],death:{owner:1,by:0,at:{x:1,z:2}}}]);
trace('last-bunker-before-expired-clock',[0,1],1,[{tick:200,periodic:true,timeLeft:0,bunkers:[[null,0]],death:{owner:1,by:0,at:{x:-.05,z:17.25}}}]);
trace('no-fallen-falls-back-to-center',[0,1],1,[{tick:4,bunkers:[[null,-1]]}]);
trace('living-allied-bunker-delays-victory',[0,1,1],1,[{tick:1,periodic:true,bunkers:[[1,0]],points:[[0,1],[1,2]]},{tick:200,periodic:true,bunkers:[[2,0]],death:{owner:2,by:0,at:{x:13.35,z:-.15}}}]);
trace('normalization-and-gaps',[4,2,5],2,[{tick:1,periodic:true,points:[[0,2],[1,1]],out:[[1,true]]},{tick:21,timeLeft:.01}]);
trace('only-defenders-draw',[3,3],3,[{tick:1,periodic:true},{tick:10,bunkers:[[null,0]],fallen:{x:-.15,z:.05}}]);
trace('no-defenders-immediate-attacker-win',[0,2],5,[{tick:1,periodic:true},{tick:200,periodic:true}]);
trace('tiny-positive-hp-is-live',[0,1],1,[{tick:1,bunkers:[[null,Number.MIN_VALUE]],dt:0},{tick:2,bunkers:[[null,0]]}]);
trace('neutral-live-bunker-counts-for-victory-not-team-sample',[0,1],1,[{tick:1,neutral:true,periodic:true},{tick:200,timeLeft:0,periodic:true}]);
trace('zero-and-negative-elapsed-preserve-arithmetic',[0,1],1,[{tick:1,dt:0,timeLeft:0.05},{tick:20,dt:-0.05},{tick:200,dt:.1,periodic:true}]);
trace('unowned-points-and-flak-excluded',[0,1],1,[{tick:1,periodic:true,points:[[0,-1],[1,0]]},{tick:40,timeLeft:.05,dt:.05}]);
clean();const output=JSON.stringify({format:1,reference:pin,node:process.version,arch:process.arch,cases})+'\n';const path=`${root}/rust/crates/simtest/tests/fixtures/assault-finish.json`;if(process.argv.includes('--check')){if(readFileSync(path,'utf8')!==output)throw Error('Assault finish fixture differs');}else writeFileSync(path,output);console.log(`Verified ${cases.length} Assault finish traces, ${cases.reduce((n,c)=>n+c.steps.length,0)} phases`);
