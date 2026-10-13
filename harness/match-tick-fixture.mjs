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
 return {...result,source:source+'\nexport const __placeUnit=placeUnit;\nexport const __setCell=setCell;\nexport function __pointSupply(g,list){if(g.supply && g.tick % CFG.supply.every === 1)supplyLines(g,list);}\nexport function __pointCaptureIncome(g,list,dt){\n'+block+'\n}\n'};
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
for(const spec of [...['standard','large','massive','endless'].map(army=>({army})),{army:'standard',commands:true},{army:'standard',environment:true},{army:'standard',combat:true},{army:'standard',retirement:true},{army:'standard',abilities:true},{army:'standard',recovery:true},{army:'standard',field:true}]) {
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
 if(spec.field){input.paths=[];input.commands=[{tick:1,issuer:0,sequence:1,command:{t:'dig',ids:[1],kind:'trench',x:13,z:17,dir:0}}];}
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
 const ticks=spec.environment?20:spec.combat?5:spec.retirement?800:spec.abilities?35:spec.recovery?80:spec.field?245:200;
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
  if(spec.field&&g.tick===1&&!g.units.get(1).dig)throw Error('Recorded Dig must admit a real paid job before the first work turn');
  if(spec.field&&g.tick===244&&g.units.get(1).dig)throw Error('Four trench cells must complete at the fourth source float threshold');
  if([1,2,3,4,5,40,80,160,161,200].includes(g.tick)||spec.environment&&[9,10,11,20].includes(g.tick)||spec.retirement&&[240,320,400,600,800].includes(g.tick)||spec.abilities&&[10,20,24,25,30,35].includes(g.tick)||spec.recovery&&[10,20,39,41,79].includes(g.tick)||spec.field&&[60,61,121,122,183,243,244,245].includes(g.tick)){
   const state=snapshot(g);
   if(opposing)state.combatState=random.state();
   if(spec.retirement||spec.abilities||spec.recovery||spec.field){state.deaths=structuredClone(deaths);state.story=g.players.map((_,i)=>story.STORY_KEYS.map(k=>bits(g.story?.[i]?.[k]??0)));}
   if(spec.abilities||spec.recovery||spec.field){state.abilityRecovery=[...g.units.values()].map(u=>[u.id,bits(u.cd),bits(u.buff),u.ap,ptOrNull(u.nade),!!u.nade?.auto,!!u.nade?.walked,u.hpWas===undefined?null:bits(u.hpWas),u.woundAt??null,u.healAt??null,bits(u.reinf)]);state.grenades=g.nades.map(n=>[n.flight,bits(n.x),bits(n.z),bits(n.t),n.owner]);}
   if(spec.field)state.field={jobs:[...g.units.values()].filter(u=>u.dig).map(u=>{const d=u.dig;return [u.id,d.kind,bits(d.x),bits(d.z),bits(d.dir),bits(d.t),d.n??null,d.cells.map(cell=>[...cell]),d.front===undefined?null:bits(d.front),d.reach===undefined?null:bits(d.reach),ptOrNull(d.from)];}),chars:g.chars.join(''),front:g.chars.flatMap((ch,c)=>ch==='T'?[[c,bits(g.trenchFront[c])]]:[])};
   if(spec.commands)state.control=[...g.units.values()].map(u=>[u.id,u.moveOutcome??null,u.moveOutcomeTick??null,(u.orders??[]).length,u.worldGoal?[bits(u.worldGoal.x),bits(u.worldGoal.z)]:null,u.amove?[bits(u.amove.x),bits(u.amove.z)]:null]);
   if(spec.environment)state.environment={fires:[...g.fires].map(([c,left])=>[c,bits(left)]),smokes:g.smokes.map(s=>[s.id,bits(s.x),bits(s.z),bits(s.r),bits(s.t)]),chars:g.chars.join(''),burnt:[...g.burnt]};
   checkpoints.push(state);
  }
 }
 if(spec.retirement&&!deaths.length)throw Error('Retirement trace must remove a projectile casualty');
 cases.push({map,army,ticks,input,checkpoints,...(opposing?{enemy:true}:{}),...(spec.retirement||spec.abilities||spec.recovery||spec.field?{retirement:true}:{}),...(spec.abilities||spec.recovery||spec.field?{abilityRecovery:true}:{}),...(spec.field?{field:true}:{})});
}
// These cases run the complete unchanged step. Initial reaction/control facts
// are explicit input; fresh incoming history is produced by real projectile contact.
const reactionSpecs=[
 {name:'reaction-timer',ticks:3,keep:[1],fields:[[1,{holdPos:true,holdFire:true,react:.05}]]},
 {name:'reaction-spacing',ticks:5,positions:[[1,13,17],[2,13.5,17]],fields:[[1,{react:.05}],[2,{react:.05}]]},
 {name:'reaction-contact-cover',ticks:6,enemy:true,changes:[[147,'H']],fields:[[1,{holdPos:true,holdFire:true}],[2,{holdPos:true,holdFire:false}]]},
 {name:'reaction-contact-rally',ticks:6,enemy:true,paths:[[1,19,21]],fields:[[1,{drift:'rally',cooldown:100,holdFire:false}],[2,{holdPos:true,holdFire:false}]]},
 {name:'reaction-autoretreat-before-autocast',ticks:2,enemy:true,startTick:8,hp:34.999,changes:[[173,'H']],fields:[[1,{holdPos:true,holdFire:false,auto:true,cooldown:100}],[2,{holdPos:true,holdFire:true}]]},
 {name:'reaction-health-threshold-autocast',ticks:2,enemy:true,startTick:8,hp:35,changes:[[173,'H']],fields:[[1,{holdPos:true,holdFire:false,auto:true,cooldown:100}],[2,{holdPos:true,holdFire:true}]]},
 {name:'reaction-paid-dig-cancel',ticks:65,keep:[1],hp:34.999,fields:[[1,{holdPos:true,holdFire:true}]],field:true}
];
for(const spec of reactionSpecs){
 const map={name:spec.name,w:20,h:20,rows:Array(20).fill('.'.repeat(20)),spawns:[{x:0,y:0},{x:19,y:19}],points:[{id:'point',x:16,y:14}]};
 const saved=Math.random;Math.random=()=>.5;let g;
 try{g=sim.createGame(map,['A','B'],false,[0,spec.enemy?1:0],[0,1],{army:'standard',weather:false,tech:true,logistics:false});}finally{Math.random=saved;}
 const input={holdPosition:false,holdFire:true,seed:0x12345678,wind:[.75,.4],keep:spec.keep??[1,2],positions:spec.positions??[[1,13,17],[2,27,17]],paths:spec.paths??[],disableAuto:true,fields:spec.fields};
 input.positions=input.positions.filter(([id])=>input.keep.includes(id));
 if(spec.enemy)input.owners=[[2,1]];
 if(spec.startTick)input.startTick=spec.startTick;
 if(spec.hp!==undefined)input.hp=[[1,spec.hp]];
 if(spec.changes)input.changes=spec.changes;
 if(spec.name==='reaction-contact-cover')input.commands=[{tick:5,issuer:0,sequence:1,command:{t:'stance',ids:[1],key:'holdPos',on:false}}];
 if(spec.field)input.commands=[{tick:1,issuer:0,sequence:1,command:{t:'dig',ids:[1],kind:'trench',x:13,z:17,dir:0}}];
 for(const id of [...g.units.keys()])if(!input.keep.includes(id))g.units.delete(id);
 for(const [id,owner]of input.owners??[])g.units.get(id).owner=owner;
 for(const u of g.units.values()){u.auto=false;u.holdPos=input.holdPosition;u.holdFire=input.holdFire;}
 g.tick=input.startTick??0;g.seed=input.seed;g.wind={a:input.wind[0],v:input.wind[1]};
 for(const [id,x,z]of input.positions)if(g.units.has(id))sim.__placeUnit(g,g.units.get(id),{x,z});
 for(const [id,hp]of input.hp??[])g.units.get(id).hp=hp;
 for(const [id,x,z]of input.paths)g.units.get(id).path=[{x,z}];
 for(const [id,fields]of input.fields)Object.assign(g.units.get(id),fields);
 for(const [cell,ch]of input.changes??[])sim.__setCell(g,cell,ch);
 const checkpoints=[],random=spec.enemy?combatRandom(701):null;
 let paid,chargedStory,timer=.05;
 for(let i=0;i<spec.ticks;i++){
  for(const row of (input.commands??[]).filter(c=>c.tick===g.tick+1).sort((a,b)=>a.issuer-b.issuer||a.sequence-b.sequence)){
   const before=g.players[row.issuer].mp;
   const result=sim.command(g,row.issuer,row.command);
   if(result!==undefined)throw Error(`${spec.name} command refused: ${result}`);
   if(spec.field){if(!g.units.get(1).dig)throw Error('Paid Dig must exist before the real step');paid=before-g.players[0].mp;chargedStory=g.story[0].mpSpent;if(!(paid>0))throw Error('Dig must charge actual manpower');}
  }
  const beforeMp=g.players[0].mp;
  const old=Math.random;if(random)Math.random=()=>random.next();try{sim.step(g);}finally{Math.random=old;}
  const u=g.units.get(1),v=g.units.get(2);
  if(spec.name==='reaction-timer'&&(timer-=.05,bits(u.react)!==bits(timer)))throw Error('Reaction timer must decrement once per real ground turn');
  if(spec.name==='reaction-spacing'&&i===0&&(u.path.length||!v.path.length||v.drift!=='space'||v.react!==1))throw Error('Only the newer idle squad must take a spacing route after timer decay');
  if(['reaction-contact-cover','reaction-contact-rally'].includes(spec.name)&&g.tick===2&&(u.hitBy!==2||u.hitAt!==2||!u.hitFrom))throw Error('Automatic enemy firing must produce actual incoming contact on tick2');
  if(spec.name==='reaction-contact-cover'&&g.tick===5&&(!(u.hitAt>=1)||!u.hitFrom||u.drift!=='cover'||!u.path.length||u.react!==2))throw Error('Actual incoming contact must cause the following turn to seek cover');
  if(spec.name==='reaction-contact-rally'&&g.tick===3&&(!(u.hitAt>=1)||!u.amove||u.path.length||u.drift!==false||u.targetId!==2))throw Error('Actual incoming contact must interrupt a rally route to answer its shooter');
  if(spec.name==='reaction-autoretreat-before-autocast'&&i===0&&(!u.retreating||g.nades.length||u.nade||u.cd!==-.05||u.targetId))throw Error('Broken squad must retreat before the eligible autocast beat');
  if(spec.name==='reaction-health-threshold-autocast'&&i===0&&(u.retreating||!g.nades.length||u.cd!==sim.UNITS[u.type].ab.cd))throw Error('Exact health threshold must stay and cast on the same beat');
  if(spec.field&&(g.players[0].mp!==beforeMp+g.players[0].inc*.05||u.dig||g.chars.includes('T')||g.story[0].mpSpent!==chargedStory||g.story[0].built!==0))throw Error('Automatic retreat must cancel the paid job before work, retaining spent story and unbuilt terrain');
  const state=snapshot(g);
  if(spec.enemy)state.combatState=random.state();
  state.reactions=[...g.units.values()].map(u=>({id:u.id,react:bits(u.react),drift:u.drift,reverse:ptOrNull(u.reverse),retreating:u.retreating,hitBy:u.hitBy,hitAt:bits(u.hitAt),hitFrom:ptOrNull(u.hitFrom),shotAt:u.shotAt===undefined?null:bits(u.shotAt),worldGoal:ptOrNull(u.worldGoal),amove:ptOrNull(u.amove),orders:u.orders.length,attack:u.attackId,holdPos:!!u.holdPos,holdFire:!!u.holdFire,autoRetreat:!!u.autoRetreat,auto:!!u.auto,nade:ptOrNull(u.nade),enter:u.enter,fireAt:u.fireAt,board:u.board,build:!!u.build,dig:!!u.dig,entrench:!!u.entrench,moveOutcome:u.moveOutcome??null,moveOutcomeTick:u.moveOutcomeTick??null}));
  state.story=g.players.map((_,i)=>story.STORY_KEYS.map(k=>bits(g.story?.[i]?.[k]??0)));
  state.abilityRecovery=[...g.units.values()].map(u=>[u.id,bits(u.cd),bits(u.buff),u.ap,ptOrNull(u.nade),!!u.nade?.auto,!!u.nade?.walked,u.hpWas===undefined?null:bits(u.hpWas),u.woundAt??null,u.healAt??null,bits(u.reinf)]);
  state.grenades=g.nades.map(n=>[n.flight,bits(n.x),bits(n.z),bits(n.t),n.owner]);
  if(spec.field)state.field={jobs:[...g.units.values()].filter(u=>u.dig).map(u=>{const d=u.dig;return [u.id,d.kind,bits(d.x),bits(d.z),bits(d.dir),bits(d.t),d.n??null,d.cells.map(cell=>[...cell]),d.front===undefined?null:bits(d.front),d.reach===undefined?null:bits(d.reach),ptOrNull(d.from)];}),chars:g.chars.join(''),front:g.chars.flatMap((ch,c)=>ch==='T'?[[c,bits(g.trenchFront[c])]]:[])};
  if(!spec.field||[1,2,3,4,5,20,40,60,61,65].includes(g.tick))checkpoints.push(state);
 }
 cases.push({name:spec.name,map,army:'standard',ticks:spec.ticks,input,checkpoints,reaction:true,abilityRecovery:true,...(spec.enemy?{enemy:true}:{}),...(spec.field?{field:true,paid:bits(paid)}:{})});
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
