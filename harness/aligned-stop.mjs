// Test-only call-boundary instrumentation. The pinned checkout is read-only.
import {readFileSync, writeFileSync, chmodSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve, dirname} from 'node:path';
import {SourceTextModule, SyntheticModule} from 'node:vm';
import {isDeepStrictEqual} from 'node:util';

if (process.arch !== 'x64' || process.version !== 'v24.21.0') throw Error('Use x64 Node v24.21.0');
const args = process.argv.slice(2);
const option = name => { const i=args.indexOf(name); if (i<0 || !args[i+1]) throw Error(`Required ${name}`); return args[i+1]; };
const reference = resolve(option('--reference')), output = resolve(option('--output'));
const inputPath = resolve(option('--input'));
const originalInput=JSON.parse(readFileSync(inputPath,'utf8'));
const sharedInput=originalInput.sharedInput ?? originalInput;
if(sharedInput.layout?.join(',')!=='0,1' || sharedInput.tape?.length!==16384)throw Error('original R1 finite tape/layout required');
const seedText=sharedInput.seed;
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=(...a)=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
const clean=()=>{if(git('rev-parse','HEAD')!==pin||git('status','--porcelain','--untracked-files=no'))throw Error('clean pinned checkout required');};
clean();
const sourceHead=execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim();
if(execFileSync('git',['status','--porcelain'],{encoding:'utf8'}).trim())throw Error('freeze complete source head before recording');
const buffer=new DataView(new ArrayBuffer(8));
const bits=value=>{buffer.setFloat64(0,value);return buffer.getBigUint64(0).toString(16).padStart(16,'0');};
const number=value=>({bits:bits(value),value:Number.isFinite(value)?value:String(value)});
const resource=value=>value===undefined?{absent:true}:number(value);
const values=sharedInput.tape.map(v=>{if(!/^[0-9a-f]{16}$/.test(v))throw Error('bad tape bits');buffer.setBigUint64(0,BigInt('0x'+v));const n=buffer.getFloat64(0);if(!Number.isFinite(n)||n<0||n>=1)throw Error('bad tape fraction');return n;});
let consumed=[], branches=[], motion=[], retirements=[];
let layoutReceipt=null;
const hooks={
  draw(consumer){const index=consumed.length;if(index>=values.length)throw Error('shared random tape exhausted; no fallback');const value=values[index];consumed.push({index,consumer,bits:bits(value)});return value;},
  branch(g,u,event){branches.push({tick:g.tick,id:u.id,event,targetId:u.targetId,x:number(u.x),z:number(u.z),hp:number(u.hp),goal:point(u.amove),path:u.path.map(point)});},
  motion(g,u,moving){motion.push({id:u.id,moving});},
  retire(g,u){retirements.push({id:u.id,owner:u.owner,kind:u.type,x:number(u.x),z:number(u.z),hp:number(u.hp),lastHit:scalar(u.lastHit)});},
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
// Observe the executed attack-move branch without changing its predicate or action.
replace('if (dist(u, u.amove) < 2.5) u.amove = null;', `if (dist(u, u.amove) < 2.5) { __aligned.branch(g,u,'arrival'); u.amove = null; }`);
replace('else if (t && canShoot(g, u, t)) u.path = [];', `else if (t && canShoot(g, u, t)) { __aligned.branch(g,u,'halt'); u.path = []; }`);
replace("else if (!u.path.length && u.repath <= 0) requestStepPath(g, u, u.amove, 'amove');", `else if (!u.path.length && u.repath <= 0) { __aligned.branch(g,u,'resumeRequest'); requestStepPath(g, u, u.amove, 'amove'); }`);
replace('const moving = u.path.length > 0 || moved > 0.001;', 'const moving = u.path.length > 0 || moved > 0.001; __aligned.motion(g,u,moving);');
replace('    g.units.delete(u.id); if (UNITS[u.type].building) wreckBuilding(g, u);\n    // A knocked-out vehicle stays', '    __aligned.retire(g,u); g.units.delete(u.id); if (UNITS[u.type].building) wreckBuilding(g, u);\n    // A knocked-out vehicle stays');
let restored=transformed;
for(const [before,after] of [...edits].reverse())restored=restored.replace(after,before);
if(restored!==originalSource)throw Error('instrumentation does not erase to the exact pinned source');
// Any other actual consumer refuses rather than silently using an unseeded draw.
const prefix="import { hooks as __aligned } from 'aligned:test';\n";
const suffix="\nconst __alignedOriginalSpawnSlots = spawnSlots; spawnSlots = (...args) => __aligned.layout(__alignedOriginalSpawnSlots(...args));\nexport const __alignedCanShoot = (g,u,t) => canShoot(g,u,t);\n";
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
const scene=JSON.parse(readFileSync(new URL('./scenarios/aligned-stop.json',import.meta.url)));
const scalar=value=>value===undefined?{absent:true}:typeof value==='number'?number(value):value;
const point=value=>value===undefined?{absent:true}:value===null?null:{x:number(value.x),z:number(value.z)};
const encode=value=>value===undefined?{absent:true}:typeof value==='number'?number(value):Array.isArray(value)?value.map(encode):value!==null&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([k,v])=>[k,encode(v)])):value;
const snapshot=(g,births=[],deaths=[])=>({tick:g.tick,nextId:g.nextId,births,deaths,
  units:[...g.units.values()].map(u=>({id:u.id,owner:u.owner,kind:u.type,x:number(u.x),z:number(u.z),hp:number(u.hp),supp:number(u.supp),
    path:u.path.map(point),rot:number(u.rot),aim:number(u.aim),moveSpeed:number(u.moveSpeed),vx:scalar(u.vx),vz:scalar(u.vz),travelDir:scalar(u.travelDir),
    reverse:point(u.reverse),repath:number(u.repath),stuck:number(u.stuck),still:scalar(u.still),
    orders:encode(u.orders),worldGoal:point(u.worldGoal),follow:scalar(u.follow),build:scalar(u.build),entrench:encode(u.entrench),dig:encode(u.dig),nade:point(u.nade),enter:u.enter,board:u.board,fireAt:u.fireAt,traffic:encode(u.traffic),trafficWait:scalar(u.trafficWait),moveOutcome:scalar(u.moveOutcome),moveOutcomeTick:scalar(u.moveOutcomeTick),targetId:u.targetId,attackId:u.attackId,amove:point(u.amove),face:u.face===null?null:encode(u.face),
    holdPos:scalar(u.holdPos),holdFire:scalar(u.holdFire),autoRetreat:scalar(u.autoRetreat),retreating:u.retreating,
    cooldown:number(u.cooldown),retarget:number(u.retarget)})),
  players:g.players.map(p=>({slot:p.slot,team:p.team,mp:number(p.mp),mun:resource(p.mun),fuel:resource(p.fuel),vp:number(p.vp),inc:number(p.inc)})),
  score:{winner:g.winner,vp:g.players.map(p=>number(p.vp))},
  points:g.points.map(p=>({owner:p.owner,capper:p.capper,progress:number(p.progress),cut:p.cut})),
  canShootFoe:g.units.has(scene.actor)&&sim.__alignedCanShoot(g,g.units.get(scene.actor),g.units.get(scene.foe)),
  branchEvents:structuredClone(branches),motion:structuredClone(motion),retirements:structuredClone(retirements),drawCount:consumed.length});
const unit=(frame,id)=>frame.units.find(u=>u.id===id);
const cleared=u=>u.amove===null&&u.attackId===0&&u.orders.length===0&&u.path.length===0&&u.face===null&&u.nade===null&&u.dig===null&&u.entrench===null&&u.build?.bits==='0000000000000000'&&u.enter===-1&&u.board===0&&u.fireAt===-1&&!u.retreating&&u.worldGoal===null&&u.traffic===null;
const qualify=observations=>{
  const stop=observations.find(o=>o.label==='batch-stop');
  if(!stop)throw Error('actual batch Stop observation missing');
  const a=unit(stop.before,1),b=unit(stop.before,2),c=unit(stop.before,3);
  const preconditions={actorAAmove:a.amove!==null,actorAPath:a.path.length>0,actorCPath:c.path.length>0,actorBAttack:b.attackId===4,actorCQueue:c.orders.length===2&&c.orders[0].t==='move'&&c.orders[1].t==='attack',foreignMoving:unit(stop.before,4).path.length>0};
  const postconditions=scene.selected.map(id=>({id,cleared:cleared(unit(stop.after,id))}));
  const controls=observations.filter(o=>['foreign-only-denied','empty-denied','no-own-denied'].includes(o.label)).map(o=>({label:o.label,needs:o.denial==='needs',unchanged:isDeepStrictEqual(o.before,o.after)}));
  const mixed=observations.find(o=>o.label==='mixed-stop');
  const foreignPreserved=isDeepStrictEqual(unit(stop.before,4),unit(stop.after,4))&&isDeepStrictEqual(unit(mixed.before,4),unit(mixed.after,4));
  const complete=Object.values(preconditions).every(Boolean)&&postconditions.every(r=>r.cleared)&&controls.length===3&&controls.every(r=>r.needs&&r.unchanged)&&foreignPreserved;
  return {preconditions,postconditions,controls,foreignPreserved,complete,simultaneousSingleActorClaim:false};
};
const cases=[];let failure=null;
try{
  consumed=[];layoutReceipt=null;branches=[];motion=[];retirements=[];
  const g=sim.createGame(structuredClone(scene.map),['A','B'],true,[0,1],[0,1],{mode:'conquest',army:'standard',sandbox:false,logistics:false,tech:true,supply:true,weather:'clear',mapKey:'aligned-amove'});
  const startup={spawns:g.players.map(p=>[bits(p.spawn.x),bits(p.spawn.z)]),environmentSeed32:g.seed>>>0,weatherSeed:1,wind:[bits(g.wind.a),bits(g.wind.v)],shower:g.wx?{raining:g.wx.raining,next:bits(g.wx.next)}:null,rain:bits(g.wx.rain),wet:bits(g.wx.wet),weather:g.weather,nextId:g.nextId};
  const row={name:'three-actor-stop',frames:[snapshot(g,[...g.units.keys()])],observations:[],startup,completed:0,inputReceipt:null,qualification:null};cases.push(row);
  for(let tick=1;tick<=scene.ticks;tick++){
    branches=[];motion=[];retirements=[];
    for(const order of scene.orders.filter(r=>r.tick===tick)){
      const before=snapshot(g);const denial=sim.command(g,order.issuer,order.command)??null;const after=snapshot(g);
      row.observations.push({tick,label:order.label,issuer:order.issuer,denial,before,after});
      if(denial!==order.expectedDenial)throw Error(`admission ${order.label} at ${tick}: ${denial}`);
      if(order.label==='batch-stop'){
        const a=unit(before,1),b=unit(before,2),c=unit(before,3);
        if(a.amove===null||b.attackId!==4||c.orders.length!==2||!scene.selected.every(id=>cleared(unit(after,id))))throw Error('actual Stop positive preconditions or postconditions missing');
      }
    }
    const beforeIds=new Set(g.units.keys());sim.step(g);row.completed++;
    row.frames.push(snapshot(g,[...g.units.keys()].filter(id=>!beforeIds.has(id)),[...beforeIds].filter(id=>!g.units.has(id))));
  }
  row.qualification=qualify(row.observations);
  if(!row.qualification.complete)throw Error('Stop branch incomplete: '+JSON.stringify(row.qualification));
  row.inputReceipt={capacity:values.length,consumed:consumed.length,unconsumed:values.length-consumed.length,exhausted:consumed.length===values.length,draws:consumed,layout:layoutReceipt,startup};
}catch(error){failure={tick:cases[0]?.completed??0,message:String(error),stack:error.stack};if(cases.length)cases[0].inputReceipt={capacity:values.length,consumed:consumed.length,unconsumed:values.length-consumed.length,exhausted:consumed.length===values.length,draws:consumed,layout:layoutReceipt};}
finally{Math.random=originalRandom;}
clean();
const registry=JSON.parse(readFileSync(new URL('../docs/testing/aligned-stop-proposed-defaults.json',import.meta.url)));
for(const field of registry.fields){const lines=readFileSync(resolve(reference,field.sourceFile),'utf8').split('\n');if(lines[field.consumerLine-1]!==field.uniqueSourceLine||!field.uniqueSourceLine.includes(field.consumingExpression))throw Error('registry Source binding changed');}
const trace={format:'aligned-stop-trace-v1',reference:pin,sourceHead,node:process.version,scene,sharedInput,cases,failure,instrumentation:{erasesExactly:true,anchors:edits.map(([before,after])=>({before,after})),zeroEngineEffects:true},proposedRegistry:registry};
if(args.includes('--check')){if(!isDeepStrictEqual(trace,JSON.parse(readFileSync(output,'utf8'))))throw Error('new Stop serial Source check differs');}
else{writeFileSync(output,JSON.stringify(trace)+'\n',{mode:0o600,flag:'wx'});chmodSync(output,0o400);}
console.log(JSON.stringify({sourceHead,check:args.includes('--check'),completed:cases[0]?.completed,frames:cases[0]?.frames.length,qualification:cases[0]?.qualification,consumed:consumed.length,failure}));
if(failure)process.exitCode=2;
