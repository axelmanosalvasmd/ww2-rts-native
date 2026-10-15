// Test-only call-boundary instrumentation. The pinned checkout is read-only.
import {readFileSync, writeFileSync, chmodSync, existsSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve, dirname} from 'node:path';
import {SourceTextModule, SyntheticModule} from 'node:vm';
import {isDeepStrictEqual} from 'node:util';

if (process.arch !== 'x64' || process.version !== 'v24.21.0') throw Error('Use x64 Node v24.21.0');
const args = process.argv.slice(2);
const option = name => { const i=args.indexOf(name); if (i<0 || !args[i+1]) throw Error(`Required ${name}`); return args[i+1]; };
const reference = resolve(option('--reference')), output = resolve(option('--output'));
if (!args.includes('--check') && existsSync(output)) throw Error('new capture path must not exist');
const seedText = option('--seed');
const scene = option('--scene');
if (!['ready-replacement','medic-visibility'].includes(scene)) throw Error('unknown combat scene');
if (!/^(0|[1-9][0-9]*)$/.test(seedText) || BigInt(seedText) > (1n<<64n)-1n) throw Error('seed must be a decimal u64');
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=(...a)=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
const clean=()=>{ if(git('rev-parse','HEAD')!==pin || git('status','--porcelain','--untracked-files=no')) throw Error('clean pinned checkout required'); };
clean();
const sourceHead=execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim();
if(execFileSync('git',['status','--porcelain'],{encoding:'utf8'}).trim()) throw Error('freeze the seam head before recording');
const buffer=new DataView(new ArrayBuffer(8));
const bits=value=>{buffer.setFloat64(0,value);return buffer.getBigUint64(0).toString(16).padStart(16,'0');};
const mask=(1n<<64n)-1n;
let state=BigInt(seedText);
// This is only the shared input generator, never an engine or an expected-state oracle.
const values=Array.from({length:16384},()=>{
  state=(state+0x9e3779b97f4a7c15n)&mask;
  let z=state;z=((z^(z>>30n))*0xbf58476d1ce4e5b9n)&mask;z=((z^(z>>27n))*0x94d049bb133111ebn)&mask;
  return Number((z^(z>>31n))>>11n)/9007199254740992;
});
const sharedInput={layout:[0,1],tape:values.map(bits),seed:seedText,
  semantics:'global finite fractions; seed32=floor(draw*2**32); each original environment generator still advances; no discarded tape draws or fallback'};
const consumed=[];
let layoutReceipt=null;
const hooks={
  draw(consumer){const index=consumed.length;if(index>=values.length)throw Error('shared random tape exhausted; no fallback');const value=values[index];consumed.push({index,consumer,bits:bits(value)});return value;},
  env(consumer, original){original();return this.draw(consumer);},
  layout(selected){layoutReceipt={input:[...sharedInput.layout],selectedBeforeInjection:selected,boundary:'before player spawns, placement and actor allocation'};return [...sharedInput.layout];},
};
const originalSource=readFileSync(resolve(reference,'shared/sim.js'),'utf8');
const edits=[];
let transformed=originalSource;
const replace=(needle, replacement)=>{
  if(transformed.split(needle).length!==2)throw Error(`instrumentation anchor not unique: ${needle}`);
  transformed=transformed.replace(needle,replacement);edits.push([needle,replacement]);
};
// Wrap draw expressions, preserving all original engine statements and formulas.
replace('Math.floor(Math.random() * good.length)',`Math.floor(__aligned.draw(['startup.layout.shuffle']) * good.length)`);
replace('g.seed = Math.floor(Math.random() * 2 ** 32)',`g.seed = Math.floor(__aligned.draw(['startup.environment.seed32']) * 2 ** 32)`);
replace('a: rng(g) * Math.PI * 2',`a: __aligned.env(['startup.wind.angle'], () => rng(g)) * Math.PI * 2`);
replace('v: 0.3 + rng(g) * 0.5',`v: 0.3 + __aligned.env(['startup.wind.strength'], () => rng(g)) * 0.5`);
replace('CFG.weather.firstRain + rng(g) * CFG.weather.dry[0]',`CFG.weather.firstRain + __aligned.env(['startup.shower'], () => rng(g)) * CFG.weather.dry[0]`);
replace('const accurate = Math.random() < acc',`const accurate = __aligned.draw(['combat.accuracy',g.tick,u.id,t.id,i]) < acc`);
replace('0.2 + Math.random() * 0.7',`0.2 + __aligned.draw(['combat.offset.magnitude',g.tick,u.id,t.id,i]) * 0.7`);
replace('(Math.random() < 0.5 ? -1 : 1)',`(__aligned.draw(['combat.offset.sign',g.tick,u.id,t.id,i]) < 0.5 ? -1 : 1)`);
replace('(rng(g) - 0.5) * 0.6 * dt',`(__aligned.env(['weather.wind.angle',g.tick], () => rng(g)) - 0.5) * 0.6 * dt`);
replace('(rng(g) - 0.5) * 0.3 * dt',`(__aligned.env(['weather.wind.strength',g.tick], () => rng(g)) - 0.5) * 0.3 * dt`);
replace('wx.next = lo + rng(g) * (hi - lo)',`wx.next = lo + __aligned.env(['weather.shower',g.tick], () => rng(g)) * (hi - lo)`);
replace('rng(g) < CFG.fire.ignite[ch]',`__aligned.env(['environment.ignite',g.tick,c], () => rng(g)) < CFG.fire.ignite[ch]`);
replace('rng(g) < CFG.fire.spread[physicalWorldCell(g, n)]',`__aligned.env(['environment.spread',g.tick,c,n], () => rng(g)) < CFG.fire.spread[physicalWorldCell(g, n)]`);
replace('rng(g) >= CFG.rubbleSpill',`__aligned.env(['environment.rubble',g.tick,c,n], () => rng(g)) >= CFG.rubbleSpill`);
let restored=transformed;
for(const [before,after] of [...edits].reverse())restored=restored.replace(after,before);
if(restored!==originalSource)throw Error('instrumentation does not erase to the exact pinned source');
// Any other actual consumer refuses rather than silently using an unseeded draw.
const prefix="import { hooks as __aligned } from 'aligned:test';\n";
const suffix="\nconst __alignedOriginalSpawnSlots = spawnSlots; spawnSlots = (...args) => __aligned.layout(__alignedOriginalSpawnSlots(...args));\n";
// Cover module initialization too, including any captured default RNG callback.
const originalRandom=Math.random;
Math.random=()=>{throw Error('unmapped Math.random consumer; no fallback');};
const modules=new Map();
const hookModule=new SyntheticModule(['hooks'],function(){this.setExport('hooks',hooks);},{identifier:'aligned:test'});
const load=path=>{
  if(modules.has(path))return modules.get(path);
  const raw=readFileSync(path,'utf8');
  const source=path===resolve(reference,'shared/sim.js')?prefix+transformed+suffix:raw;
  const module=new SourceTextModule(source,{identifier:path});modules.set(path,module);return module;
};
const module=load(resolve(reference,'shared/sim.js'));
await module.link((specifier,parent)=>specifier==='aligned:test'?hookModule:load(resolve(dirname(parent.identifier),specifier)));
await module.evaluate();
const sim=module.namespace;

// Separate command-only scenes. No existing firing fixture is read or rewritten.
const map={name:'Combat genuine command controls',w:64,h:48,rows:Array(48).fill('.'.repeat(64)),spawns:[{x:2,y:24},{x:61,y:24}],points:[{x:32,y:42}]};
const orders=[],frames=[],denials=[];
let completed=0,failure=null,startup=null,sequence=0,firstShot=0,invalidated=0,medic=null,oldTarget=null,replacement=null,replacementHealth=null;
let purchase=null,arrivalTick=0;
const bitsOf=v=>bits(v);
const environment=g=>({seed32:g.seed>>>0,wind:[bits(g.wind.a),bits(g.wind.v)],
 shower:g.wx?{raining:g.wx.raining,next:bits(g.wx.next)}:null,rain:bits(g.wx.rain),wet:bits(g.wx.wet)});
const snapshot=g=>({tick:g.tick,randomConsumed:consumed.length,environment:environment(g),units:[...g.units.values()].map(u=>({id:u.id,owner:u.owner,kind:u.type,
 x:bitsOf(u.x),z:bitsOf(u.z),hp:bitsOf(u.hp),target:u.targetId,cooldown:bitsOf(u.cooldown),retarget:bitsOf(u.retarget),
 attack:u.attackId,shotAt:u.shotAt===undefined?null:bitsOf(u.shotAt),visible:[0,1].map(owner=>g.players[owner].visible.has(u.id))}))});
const issue=(g,owner,command)=>{
 const row={tick:g.tick+1,issuer:owner,sequence:++sequence,command};
 const previous=orders.at(-1);
 if(previous&&(previous.tick>row.tick||(previous.tick===row.tick&&previous.issuer>row.issuer)))throw Error('commands must execute in canonical tick/issuer/sequence order');
 orders.push(row);
 const reason=sim.command(g,owner,command)??null;denials.push({...row,reason});return reason;
};
const require=(ok,message)=>{if(!ok)throw Error(message);};
try{
 const g=sim.createGame(map,['A','B'],true,[0,1],[0,1],{mode:'conquest',army:'standard',sandbox:false,logistics:false,tech:true,supply:true,weather:'clear'});
 startup={spawns:g.players.map(p=>[bits(p.spawn.x),bits(p.spawn.z)]),environmentSeed32:g.seed>>>0,weatherSeed:1,
 wind:[bits(g.wind.a),bits(g.wind.v)],shower:g.wx?{raining:g.wx.raining,next:bits(g.wx.next)}:null,rain:bits(g.wx.rain),wet:bits(g.wx.wet),weather:g.weather,nextId:g.nextId};
 require([1,2,4,5].every(id=>g.units.get(id)?.type==='rifle'),'natural rifle IDs');
 frames.push(snapshot(g));
 for(let tick=1;tick<=1000;tick++){
  if(tick===1){
   for(const [owner,ids] of [[0,[1,2,3]],[1,[4,5,6]]]){
    for(const key of ['holdFire','holdPos','autoRetreat'])require(issue(g,owner,{t:'stance',ids,key,on:key!=='autoRetreat'})===null,'stance denied');
    require(issue(g,owner,{t:'autocast',ids,on:false})===null,'autocast denied');
    if(scene==='ready-replacement')require(issue(g,owner,{t:'move',orders:owner===0?[[1,30,48]]:[[4,50,48],[5,54,52]]})===null,'ordinary stage denied');
   }
  }
  if(scene==='medic-visibility'){
   if(tick===2){
    const makers=[...g.units.values()].filter(u=>u.owner===0&&u.built>=1&&sim.UNITS[u.type].makes?.includes('medic'));
    require(makers.length>0,'actual completed medic producer required');
    const before=g.players[0].mp;require(issue(g,0,{t:'buy',unit:'medic',from:makers[0].id})===null,'real medic purchase denied');
    require(before-g.players[0].mp===sim.UNITS.medic.cost,'medic must spend real manpower');
    purchase={maker:makers[0].id,cost:sim.UNITS.medic.cost,before:bits(before),after:bits(g.players[0].mp)};
   }
   if(tick===3){
    const purchased=[...g.units.values()].filter(u=>u.owner===0&&u.type==='medic');require(purchased.length===1,'one purchased medic');medic=purchased[0].id;
    require(!g.players[0].visible.has(4),'enemy must be naturally unseen');
    require(issue(g,0,{t:'stop',ids:[medic]})===null,'medic stop denied');
    require(issue(g,0,{t:'attack',ids:[medic],target:4})==='unseen','unseen medic denial');
   }
   if(tick===4)require(issue(g,1,{t:'move',orders:[[4,20,48]]})===null,'visibility control move denied');
   if(tick===700){require(g.players[0].visible.has(4),'ordinary movement must produce visibility');require(issue(g,0,{t:'attack',ids:[medic],target:4})==='queueFull','visible unarmed control');}
  }else{
   if(tick===701){
    for(const [id,x,z] of [[1,30,48],[4,50,48],[5,54,52]])require(Math.hypot(g.units.get(id).x-x,g.units.get(id).z-z)<.5,'ordinary stage incomplete');
    require(issue(g,0,{t:'stance',ids:[1],key:'holdFire',on:false})===null,'search enable denied');
   }
   const shooter=g.units.get(1);
   if(tick>701&&!firstShot&&shooter.shotAt>=701){firstShot=shooter.shotAt;oldTarget=shooter.targetId;replacement=oldTarget===4?5:4;require([4,5].includes(oldTarget),'actual first target');}
   if(firstShot&&!invalidated&&shooter.cooldown>0&&shooter.cooldown<=.05&&shooter.retarget>.05){
    require(shooter.targetId===oldTarget&&g.units.get(oldTarget)?.hp>0,'live current target retained through real reload');
    require(g.units.get(oldTarget).hp<sim.UNITS.rifle.models*sim.UNITS.rifle.hpPer,'first physical volley damage');
    replacementHealth=g.units.get(replacement).hp;
    require(replacementHealth===sim.UNITS.rifle.models*sim.UNITS.rifle.hpPer,'replacement starts undamaged');
    require(issue(g,1,{t:'destroy',ids:[oldTarget]})===null,'actual target-owner invalidation denied');invalidated=tick;
   }
  }
  sim.step(g);completed++;frames.push(snapshot(g));
  if(scene==='ready-replacement'&&tick===invalidated){
   const shooter=g.units.get(1);require(shooter.targetId===replacement,'same-tick replacement');require(shooter.shotAt===tick,'ready replacement must launch acquisition tick');
   require(g.shots.some(shot=>shot.k==='flight'&&shot.f===1&&shot.time===g.tick*sim.TICK),'physical flight on acquisition tick');
  }
  if(scene==='ready-replacement'&&invalidated&&tick===invalidated+4){
   require((g.units.get(replacement)?.hp??0)<replacementHealth,'acquisition volley damage within 0.2 seconds');
   require(g.units.get(1).shotAt===invalidated,'no later volley substitutes for acquisition volley');arrivalTick=tick;
  }
 }
 if(scene==='ready-replacement'){
  require(arrivalTick===invalidated+4,'acquisition volley arrival observed');
  require(invalidated>0,'naturally ready replacement opportunity missing');
  require((g.units.get(replacement)?.hp??0)<sim.UNITS.rifle.models*sim.UNITS.rifle.hpPer,'replacement projectile damage');
 }else require(medic!==null,'purchased medic required');
}catch(error){failure={nextTick:completed+1,message:String(error),stack:error.stack};}
finally{Math.random=originalRandom;}
clean();
const inputReceipt={capacity:values.length,consumed:consumed.length,unconsumed:values.length-consumed.length,exhausted:consumed.length===values.length,draws:consumed,layout:layoutReceipt,startup};
const trace={format:'combat-command-predicates-v1',reference:pin,sourceHead,node:process.version,arch:process.arch,seed:seedText,scene,sharedInput,inputReceipt,map,orders,frames,denials,completed,
 witness:{firstShot,invalidated,oldTarget,replacement,medic,purchase,arrivalTick},instrumentation:{erasesExactly:true,anchors:edits.map(([before,after])=>({before,after}))},failure};
if(args.includes('--check')){const prior=JSON.parse(readFileSync(output));if(prior.sourceHead!==sourceHead)throw Error('regeneration must use the capture producing head');if(!isDeepStrictEqual(trace,prior))throw Error('fresh command scene differs');}
else{writeFileSync(output,JSON.stringify(trace)+'\n',{mode:0o600,flag:'wx'});chmodSync(output,0o400);}
console.log(JSON.stringify({engine:'Source',sourceHead,scene,completed,commands:orders.length,draws:consumed.length,failure}));
if(failure)process.exitCode=2;
