// Run the actual pinned Conquest step with explicit terrain, paths and recorded
// combat draws. These traces prove connected phases, not complete gameplay.
import {readFileSync, writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
const reference=resolve(process.env.WW2_REFERENCE??fileURLToPath(new URL('../reference/ww2-rts/',import.meta.url)));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
const clean=()=>{if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Pinned clean reference required');};
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');
clean();
registerHooks({load(url,context,next){const result=next(url,context);if(url!==pathToFileURL(resolve(reference,'shared/sim.js')).href)return result;
 const source=String(result.source),begin='  // teammates standing on it keep it theirs.',end='  stepConvoys(g, dt, supplyHooks(g));';
 if(source.split(begin).length!==2||source.split(end).length!==2)throw Error('Point anchors changed');
 const block=source.slice(source.indexOf(begin),source.indexOf(end));
 return {...result,source:source+'\nexport const __setCell=setCell;\nexport function __pointSupply(g,list){if(g.supply && g.tick % CFG.supply.every === 1)supplyLines(g,list);}\nexport function __pointCaptureIncome(g,list,dt){\n'+block+'\n}\n'};
}});
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const story=await import(pathToFileURL(resolve(reference,'shared/story.js')));
const buffer=new DataView(new ArrayBuffer(8));
const bits=n=>{buffer.setFloat64(0,n);return buffer.getBigUint64(0).toString(16).padStart(16,'0');};
const ptOrNull=p=>p?[bits(p.x),bits(p.z)]:null;
// The recorder supplies the native recorded Combat stream only to the unchanged
// prototype's random draws. Environment randomness retains the source seed.
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n;
const mix=x=>{x=((x^(x>>30n))*0xbf58476d1ce4e5b9n)&mask;x=((x^(x>>27n))*0x94d049bb133111ebn)&mask;return x^(x>>31n);};
function combatRandom(seed){let state=mix(BigInt(seed)^((3n*gamma)&mask));return {next(){state=(state+gamma)&mask;return Number(mix(state)>>11n)/9007199254740992;},state:()=>state.toString(16).padStart(16,'0')};}
const snapshot=g=>({tick:g.tick,visionTick:g.visionTick,seed:g.seed>>>0,wind:[bits(g.wind.a),bits(g.wind.v)],
  units:[...g.units.values()].map(u=>[u.id,bits(u.x),bits(u.z),bits(u.rot),bits(u.hp),bits(u.supp),bits(u.stuck),bits(u.repath),bits(u.sprint),u.path.map(p=>[bits(p.x),bits(p.z)])]),
  weapons:[...g.units.values()].map(u=>[u.id,u.targetId,bits(u.cooldown),bits(u.retarget),bits(u.aim)]),
  players:g.players.map(p=>[bits(p.mp),bits(p.vp),bits(p.inc)]),
  points:g.points.map(p=>[p.owner,p.capper,bits(p.progress),p.cut,p.contested,p.onPoint,p.capTick??null])});
const cases=[];
for(const spec of [...['standard','large','massive','endless'].map(army=>({army})),{army:'standard',commands:true},{army:'standard',environment:true},{army:'standard',combat:true},{army:'standard',retirement:true},{army:'standard',abilities:true},{army:'standard',recovery:true}]) {
 const {army}=spec;
 const opposing=spec.combat||spec.retirement||spec.abilities;
 const map={name:'Friendly tick integration',w:20,h:20,rows:Array(20).fill('.'.repeat(20)),spawns:[{x:0,y:0},{x:19,y:19}],points:[{id:'point',x:8,y:8},{id:'locked',x:11,y:8,needs:0}]};
 const old=Math.random; Math.random=()=>.5;let g;
 try{g=sim.createGame(map,['A','B'],false,[0,opposing?1:0],[0,1],{army,weather:false,tech:true,logistics:false});}finally{Math.random=old;}
 const input={holdPosition:true,seed:0x12345678,wind:[.75,.4],paths:[[1,17,17],[2,23,17]],positions:[[1,13,17],[2,27,17]]};
 if(spec.commands)input.commands=[
  {tick:1,issuer:0,sequence:2,command:{t:'move',orders:[[1,13.05,17]]}},
  {tick:1,issuer:0,sequence:1,command:{t:'stop',ids:[1]}},
  {tick:2,issuer:0,sequence:3,command:{t:'move',orders:[[1,19,21]],queue:true}},
  {tick:5,issuer:0,sequence:4,command:{t:'stop',ids:[1]}},
  {tick:6,issuer:0,sequence:5,command:{t:'amove',orders:[[1,17,19]]}}
 ];
 if(spec.environment){
  input.paths=input.paths.filter(row=>row[0]!==1);
  input.fires=[[166,2]];input.smokes=[{id:999,x:13,z:17,r:5,t:.45}];
  input.changes=Array.from({length:9},(_,i)=>[166+(Math.floor(i/3)-1)*20+(i%3)-1,'H']).filter(row=>row[0]!==166);
 }
 if(opposing){input.paths=[];input.keep=[1,2];input.owners=[[2,1]];input.disableAuto=true;}
 if(spec.retirement)input.disableRetreat=true;
 if(spec.abilities){input.disableRetreat=true;input.commands=[{tick:1,issuer:0,sequence:1,command:{t:'ability',ids:[1],x:27,z:17}}];}
 if(spec.recovery){input.paths=[];input.positions=[[1,3,3],[2,27,17]];input.hp=[[1,60]];}
 if(input.keep)for(const id of [...g.units.keys()])if(!input.keep.includes(id))g.units.delete(id);
 for(const[id,owner]of input.owners??[])g.units.get(id).owner=owner;
 if(input.disableAuto)for(const u of g.units.values())u.auto=false;
 if(input.disableRetreat)for(const u of g.units.values())u.autoRetreat=false;
 for(const u of g.units.values())u.holdPos=true;
 g.seed=input.seed;g.wind={a:input.wind[0],v:input.wind[1]};
 for(const [id,x,z]of input.positions)Object.assign(g.units.get(id),{x,z});
 for(const [id,hp]of input.hp??[])g.units.get(id).hp=hp;
 for(const [id,x,z]of input.paths)g.units.get(id).path=[{x,z}];
 for(const [cell,ch]of input.changes??[])sim.__setCell(g,cell,ch);
 for(const [cell,left]of input.fires??[])g.fires.set(cell,left);
 for(const cloud of input.smokes??[])g.smokes.push({...cloud});
 const checkpoints=[];
 const ticks=spec.environment?20:spec.combat?5:spec.retirement?800:spec.abilities?35:spec.recovery?80:200;
 const random=opposing?combatRandom(701):null;
 const deaths=[];
 for(let i=0;i<ticks;i++){
  for(const row of (input.commands??[]).filter(c=>c.tick===g.tick+1).sort((a,b)=>a.issuer-b.issuer||a.sequence-b.sequence))sim.command(g,row.issuer,row.command);
  const before=[...g.units.values()];
  const saved=Math.random;if(random)Math.random=()=>random.next();try{sim.step(g);}finally{Math.random=saved;}
  for(const u of before)if(!g.units.has(u.id))deaths.push([g.tick,u.id,u.owner,u.type,u.lastHit??null]);
  if(spec.combat&&([...g.units.values()].some(u=>u.hp<=0)||g.units.size!==2))throw Error('Short contact trace must precede retirement');
  if(spec.combat&&g.tick===3&&[...g.units.values()].some(u=>u.hp>=100))throw Error('Both squads must receive projectile contact damage');
  if(spec.commands&&g.tick===2&&(g.units.get(1).orders.length||g.units.get(1).moveOutcomeTick!==2))throw Error('Queued move must resume after completing its predecessor');
  if(spec.environment&&g.tick===10&&(g.fires.size<2||g.smokes.length))throw Error('Expiring smoke must suppress the first spreading fire cloud');
  if(spec.abilities&&g.tick===1&&!g.nades.length)throw Error('The boundary ability must launch its physical grenade on the first unit turn');
  if(spec.recovery&&g.tick===40&&g.units.get(1).hp!==80)throw Error('Home reinforcement must buy the first missing model on tick40');
  if([1,2,3,4,5,40,80,160,161,200].includes(g.tick)||spec.environment&&[9,10,11,20].includes(g.tick)||spec.retirement&&[240,320,400,600,800].includes(g.tick)||spec.abilities&&[10,20,24,25,30,35].includes(g.tick)||spec.recovery&&[10,20,39,41,79].includes(g.tick)){
   const state=snapshot(g);
   if(opposing)state.combatState=random.state();
   if(spec.retirement||spec.abilities||spec.recovery){state.deaths=structuredClone(deaths);state.story=g.players.map((_,i)=>story.STORY_KEYS.map(k=>bits(g.story?.[i]?.[k]??0)));}
   if(spec.abilities||spec.recovery){state.abilityRecovery=[...g.units.values()].map(u=>[u.id,bits(u.cd),bits(u.buff),u.ap,ptOrNull(u.nade),!!u.nade?.auto,!!u.nade?.walked,u.hpWas===undefined?null:bits(u.hpWas),u.woundAt??null,u.healAt??null,bits(u.reinf)]);state.grenades=g.nades.map(n=>[n.flight,bits(n.x),bits(n.z),bits(n.t),n.owner]);}
   if(spec.commands)state.control=[...g.units.values()].map(u=>[u.id,u.moveOutcome??null,u.moveOutcomeTick??null,(u.orders??[]).length,u.worldGoal?[bits(u.worldGoal.x),bits(u.worldGoal.z)]:null,u.amove?[bits(u.amove.x),bits(u.amove.z)]:null]);
   if(spec.environment)state.environment={fires:[...g.fires].map(([c,left])=>[c,bits(left)]),smokes:g.smokes.map(s=>[s.id,bits(s.x),bits(s.z),bits(s.r),bits(s.t)]),chars:g.chars.join(''),burnt:[...g.burnt]};
   checkpoints.push(state);
  }
 }
 if(spec.retirement&&!deaths.length)throw Error('Retirement trace must remove a projectile casualty');
 cases.push({map,army,ticks,input,checkpoints,...(opposing?{enemy:true}:{}),...(spec.retirement||spec.abilities||spec.recovery?{retirement:true}:{}),...(spec.abilities||spec.recovery?{abilityRecovery:true}:{})});
}
// A terrain barrier appears after supply. Income must retain that earlier beat's
// cut flags; the next supply beat observes the barrier.
const orderingMap={name:'Supply before late terrain',w:20,h:20,rows:Array(20).fill('.'.repeat(20)),spawns:[{x:0,y:0},{x:19,y:19}],points:[{id:'supply',x:8,y:14}]};
let orderingGame;const oldRandom=Math.random;Math.random=()=>.5;
try{orderingGame=sim.createGame(orderingMap,['A','B'],false,[0,1],[0,1],{weather:false,tech:true,logistics:false});}finally{Math.random=oldRandom;}
Object.assign(orderingGame.points[0],{owner:0,capper:0,progress:1});
const pointState=g=>({players:g.players.map(p=>[bits(p.mp),bits(p.vp),bits(p.inc)]),points:g.points.map(p=>[p.owner,p.capper,bits(p.progress),p.cut,p.contested,p.onPoint,p.capTick??null])});
const ordering=[];
for(const tick of [1,41]){
 orderingGame.tick=tick;const list=[...orderingGame.units.values()];sim.__pointSupply(orderingGame,list);
 const changes=tick===1?Array.from({length:20},(_,x)=>[10*20+x,'Q']):[];
 for(const [cell,ch]of changes)sim.__setCell(orderingGame,cell,ch);
 sim.__pointCaptureIncome(orderingGame,list,.05);
 ordering.push({tick,changes,expected:pointState(orderingGame)});
}
clean();const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases,ordering:{map:orderingMap,army:'standard',steps:ordering}})+'\n';
const path=fileURLToPath(new URL('../rust/crates/simtest/tests/fixtures/match-tick.json',import.meta.url));
if(process.argv.includes('--check')){if(readFileSync(path,'utf8')!==output)throw Error('Match tick fixture differs');}else writeFileSync(path,output);
console.log(`Verified ${cases.length} actual-step traces, ${cases.reduce((n,c)=>n+c.ticks,0)} ticks, ${cases.reduce((n,c)=>n+c.checkpoints.length,0)} checkpoints`);
