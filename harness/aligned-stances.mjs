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
const seedText = option('--seed');
if (!/^(0|[1-9][0-9]*)$/.test(seedText) || BigInt(seedText) > (1n<<64n)-1n) throw Error('seed must be a decimal u64');
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=(...a)=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
const clean=()=>{ if(git('rev-parse','HEAD')!==pin || git('status','--porcelain','--untracked-files=no')) throw Error('clean pinned checkout required'); };
clean();
const sourceHead=execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim();
if(execFileSync('git',['status','--porcelain'],{encoding:'utf8'}).trim()) throw Error('freeze the seam head before recording');
const buffer=new DataView(new ArrayBuffer(8));
const bits=value=>{buffer.setFloat64(0,value);return buffer.getBigUint64(0).toString(16).padStart(16,'0');};
const number=value=>({bits:bits(value),value:Number.isFinite(value)?value:String(value)});
const resource=value=>value===undefined?{absent:true}:number(value);
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
const scene=JSON.parse(readFileSync(new URL('./scenarios/aligned-stances.json',import.meta.url)));
const scalar=value=>value===undefined?{absent:true}:typeof value==='number'?number(value):value;
const point=value=>value===undefined?{absent:true}:value===null?null:{x:number(value.x),z:number(value.z)};
const encode=value=>value===undefined?{absent:true}:typeof value==='number'?number(value):Array.isArray(value)?value.map(encode):value!==null&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([k,v])=>[k,encode(v)])):value;
const snapshot=(g,births=[],deaths=[])=>({tick:g.tick,nextId:g.nextId,births,deaths,
  units:[...g.units.values()].map(u=>({id:u.id,owner:u.owner,kind:u.type,x:number(u.x),z:number(u.z),hp:number(u.hp),supp:number(u.supp),
    path:u.path.map(point),rot:number(u.rot),aim:number(u.aim),moveSpeed:number(u.moveSpeed),vx:scalar(u.vx),vz:scalar(u.vz),travelDir:scalar(u.travelDir),
    reverse:point(u.reverse),repath:number(u.repath),stuck:number(u.stuck),still:scalar(u.still),
    orders:encode(u.orders),targetId:u.targetId,attackId:u.attackId,amove:point(u.amove),face:u.face===null?null:encode(u.face),
    holdPos:scalar(u.holdPos),holdFire:scalar(u.holdFire),autoRetreat:scalar(u.autoRetreat),retreating:u.retreating,
    cooldown:number(u.cooldown),retarget:number(u.retarget)})),
  players:g.players.map(p=>({slot:p.slot,team:p.team,mp:number(p.mp),mun:resource(p.mun),fuel:resource(p.fuel),vp:number(p.vp),inc:number(p.inc)})),
  score:{winner:g.winner,vp:g.players.map(p=>number(p.vp))},
  points:g.points.map(p=>({owner:p.owner,capper:p.capper,progress:number(p.progress),cut:p.cut})),
  canShootFoe:g.units.has(scene.actor)&&sim.__alignedCanShoot(g,g.units.get(scene.actor),g.units.get(scene.foe)),
  branchEvents:structuredClone(branches),motion:structuredClone(motion),retirements:structuredClone(retirements),drawCount:consumed.length});
const recipientFlags=g=>{
  const rows=cache=>[0,1].map(slot=>({slot,units:sim.snapshotFor(g,slot,[],[],cache).units.map(row=>({id:row[0],owner:row[2],flags:row[12]}))}));
  const direct=rows(undefined),cached=rows(sim.snapshotCache(g));
  return {direct,cached};
};
const commandFields=g=>recipientFlags(g);
const originalSnapshot=snapshot;
const stanceSnapshot=(g,births=[],deaths=[],commands=[])=>({...originalSnapshot(g,births,deaths),recipientFlags:recipientFlags(g),commands});
const stanceBranches=frames=>{
  const unit=(tick,id)=>frames[tick]?.units.find(u=>u.id===id);
  const n=(tick,id,key)=>unit(tick,id)?.[key]?.value;
  const sameSpot=(a,b)=>a&&b&&a.x.bits===b.x.bits&&a.z.bits===b.z.bits;
  const receipts=frames.flatMap(f=>f.commands);
  const malformed=receipts.filter(c=>c.label.startsWith('malformed-')||c.label==='missing-on');
  const foreign=receipts.filter(c=>c.label.startsWith('foreign-'));
  const allDispositions=receipts.length===scene.cases[0].orders.length&&receipts.every((c,i)=>c.denial===scene.cases[0].orders[i].expectedDenial);
  const inRange=frames.slice(500,601).some(f=>f.canShootFoe);
  const holdFirePreventsDamage=inRange&&n(500,4,'hp')===100&&n(600,4,'hp')===100;
  const naturalIncoming=n(600,1,'hp')<n(520,1,'hp');
  const holdPositionUnderFire=naturalIncoming&&frames.slice(520,601).every(f=>sameSpot(f.units.find(u=>u.id===1),unit(520,1)));
  const explicitAttackOverridesHoldFire=n(700,4,'hp')<n(619,4,'hp');
  const holdFireOffResumesDamage=n(779,4,'hp')<n(701,4,'hp');
  const releasedMoveTick=frames.findIndex((f,t)=>t>820&&unit(t,1)&&!sameSpot(unit(t,1),unit(820,1)));
  const actualBooleanToggles=unit(16,1)?.holdPos===false&&unit(17,1)?.holdPos===true&&unit(20,1)?.autoRetreat===true&&unit(21,1)?.autoRetreat===false&&unit(702,1)?.holdFire===false&&unit(780,1)?.holdFire===true;
  let ownerBits=0,visibleForeignPrivateChecks=0,leaked=false;
  for(const f of frames)for(const mode of ['direct','cached'])for(const view of f.recipientFlags[mode])for(const row of view.units){
    if(row.owner===view.slot)ownerBits|=row.flags&30720;
    else {if(row.flags&30720)leaked=true;const own=f.recipientFlags[mode][row.owner].units.find(u=>u.id===row.id);if(own?.flags&30720)visibleForeignPrivateChecks++;}
  }
  const ownerOnlyRecipientBits=ownerBits===30720&&visibleForeignPrivateChecks>0&&!leaked;
  const result={allDispositions,malformedBlocked:malformed.length===5&&malformed.every(c=>c.denial==='blocked'),foreignNeeds:foreign.length===2&&foreign.every(c=>c.denial==='needs'),inRange,holdFirePreventsDamage,naturalIncoming,holdPositionUnderFire,explicitAttackOverridesHoldFire,holdFireOffResumesDamage,releasedMoveTick:releasedMoveTick<0?null:releasedMoveTick,actualBooleanToggles,ownerBits,visibleForeignPrivateChecks,ownerOnlyRecipientBits};
  result.complete=allDispositions&&result.malformedBlocked&&result.foreignNeeds&&holdFirePreventsDamage&&holdPositionUnderFire&&explicitAttackOverridesHoldFire&&holdFireOffResumesDamage&&releasedMoveTick>820&&actualBooleanToggles&&ownerOnlyRecipientBits;
  return result;
};
const cases=[];
let failure=null;
try {
  for(const authored of scene.cases) {
    consumed=[];layoutReceipt=null;branches=[];motion=[];retirements=[];
    const g=sim.createGame(structuredClone(scene.map),['A','B'],true,[0,1],[0,1],{mode:'conquest',army:'standard',sandbox:false,logistics:false,tech:true,supply:true,weather:'clear',mapKey:'aligned-stances'});
    const startup={spawns:g.players.map(p=>[bits(p.spawn.x),bits(p.spawn.z)]),environmentSeed32:g.seed>>>0,weatherSeed:1,
      wind:[bits(g.wind.a),bits(g.wind.v)],shower:g.wx?{raining:g.wx.raining,next:bits(g.wx.next)}:null,rain:bits(g.wx.rain),wet:bits(g.wx.wet),weather:g.weather,nextId:g.nextId};
    const frames=[stanceSnapshot(g,[...g.units.keys()])],denials=[];
    const row={name:authored.name,frames,denials,startup,completed:0,inputReceipt:null};cases.push(row);
    for(let tick=1;tick<=scene.ticks;tick++) {
      branches=[];motion=[];retirements=[];
      const commands=[];
      for(const order of authored.orders.filter(r=>r.tick===tick)) {
        const before=commandFields(g);
        const reason=sim.command(g,order.issuer,structuredClone(order.command));
        const result={tick,issuer:order.issuer,sequence:order.sequence,label:order.label,command:order.command,denial:reason===undefined?null:reason,before,after:commandFields(g)};
        commands.push(result);
        if(result.denial!==order.expectedDenial) {
          row.commandFailure=result;
          throw Error(`actual command disposition differs at ${tick}: ${reason}`);
        }
      }
      const beforeIds=new Set(g.units.keys());
      sim.step(g);row.completed++;
      frames.push(stanceSnapshot(g,[...g.units.keys()].filter(id=>!beforeIds.has(id)),[...beforeIds].filter(id=>!g.units.has(id)),commands));
    }
    row.branches=stanceBranches(frames);
    if(!row.branches.complete)throw Error(`Source stance branch incomplete: ${JSON.stringify(row.branches)}`);
    row.inputReceipt={capacity:values.length,consumed:consumed.length,unconsumed:values.length-consumed.length,exhausted:consumed.length===values.length,draws:consumed,layout:layoutReceipt,startup};
  }
}catch(error){failure={case:cases.at(-1)?.name,tick:cases.at(-1)?.completed,message:String(error),stack:error.stack};
  if(cases.length)cases.at(-1).inputReceipt={capacity:values.length,consumed:consumed.length,unconsumed:values.length-consumed.length,exhausted:consumed.length===values.length,draws:consumed,layout:layoutReceipt};
}finally{Math.random=originalRandom;}
clean();
const trace={format:'aligned-stances-trace-v1',reference:pin,sourceHead,node:process.version,scene,sharedInput,cases,failure,
  instrumentation:{erasesExactly:true,anchors:edits.map(([before,after])=>({before,after})),originalFunctions:'createGame, spawnSlots, rng, command, step, movement, attack-move, firing and retirement execute; only observers and accepted input hooks added'}};
if(args.includes('--check')) {
  const prior=JSON.parse(readFileSync(output));
  if(!isDeepStrictEqual(prior,trace)) {writeFileSync(output+'.check-failure.json',JSON.stringify(trace)+'\n',{mode:0o400,flag:'wx'});throw Error('authentic stance check differs from frozen capture');}
} else {writeFileSync(output,JSON.stringify(trace)+'\n',{mode:0o600,flag:'wx'});chmodSync(output,0o400);}
console.log(JSON.stringify({sourceHead,cases:cases.map(c=>({name:c.name,completed:c.completed,frames:c.frames.length,branches:c.branches,consumed:c.inputReceipt?.consumed,unconsumed:c.inputReceipt?.unconsumed})),failure}));
if(failure)process.exitCode=2;
