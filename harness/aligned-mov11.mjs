// Test-only call-boundary instrumentation. The pinned checkout is read-only.
import {readFileSync, writeFileSync, chmodSync, mkdirSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve, dirname} from 'node:path';
import {SourceTextModule, SyntheticModule} from 'node:vm';

if (process.arch !== 'x64' || process.version !== 'v24.21.0') throw Error('Use x64 Node v24.21.0');
const args = process.argv.slice(2);
const option = name => { const i=args.indexOf(name); if (i<0 || !args[i+1]) throw Error(`Required ${name}`); return args[i+1]; };
const reference = resolve(option('--reference'));
const checking=args.includes('--check-dir');
const output=resolve(option(checking?'--check-dir':'--output-dir'));
if(!checking)mkdirSync(output,{mode:0o700});
// Reuse only the parent's private authored seed input, never its engine images.
const seedText = args.includes('--seed-from')
  ? JSON.parse(readFileSync(resolve(option('--seed-from')),'utf8')).sharedInput.seed
  : option('--seed');
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
let consumed=[], branches=[], motion=[], retirements=[], walk=[], pathCalls=0;
let layoutReceipt=null;
const hooks={
  walk(g,u,event,route,speed,dt,back,traffic,budget=null){walk.push({tick:g.tick,id:u.id,event,route:route.map(point),x:number(u.x),z:number(u.z),rot:number(u.rot),speed:number(speed),dt:number(dt),back:!!back,yielding:!!traffic.yielding,budget:budget===null?null:number(budget),repath:number(u.repath)});},
  pathQuery(){pathCalls++;},
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
replace('const stats = pathStatsFor(g); stats.calls++;','const stats = pathStatsFor(g); stats.calls++; __aligned.pathQuery();');
let restored=transformed;
for(const [before,after] of [...edits].reverse())restored=restored.replace(after,before);
if(restored!==originalSource)throw Error('instrumentation does not erase to the exact pinned source');
const originalWalk=readFileSync(resolve(reference,'shared/movement.js'),'utf8');
let transformedWalk=originalWalk;
const walkEdits=[];
const walkReplace=(before,after)=>{
  if(transformedWalk.split(before).length!==2)throw Error(`walk observer anchor not unique: ${before}`);
  transformedWalk=transformedWalk.replace(before,after);walkEdits.push([before,after]);
};
const event=(name,budget='null')=>`__aligned.walk(g,u,'${name}',route,speed,dt,back,traffic,${budget});`;
walkReplace('function walkRoute(g, u, def, route, speed, dt, back, traffic, trafficView) {',
  `function walkRoute(g, u, def, route, speed, dt, back, traffic, trafficView) { ${event('enter')}`);
walkReplace('while (route.length > 1 && dist(u, route[0]) < 1 && walkable(trafficView(), u, route[1], blockOf(def) | WIRE)) route.shift();',
  `while (route.length > 1 && dist(u, route[0]) < 1 && walkable(trafficView(), u, route[1], blockOf(def) | WIRE)) { route.shift(); ${event('skip')} }`);
walkReplace('let budget = speed * dt * (back ? CFG.behavior.reverseSpeed : 1);',
  `let budget = speed * dt * (back ? CFG.behavior.reverseSpeed : 1); ${event('budget','budget')}`);
walkReplace('if (!walkable(g, u, next, blockOf(def))) { u.repath = 0.2; if (!traffic.yielding) u.path = []; break; }',
  `if (!walkable(g, u, next, blockOf(def))) { u.repath = 0.2; if (!traffic.yielding) { u.path = []; ${event('blocked-drop','budget')} } else { ${event('blocked-keep','budget')} } break; }`);
walkReplace('if (!back) u.rot = Math.atan2(wp.z - u.z, wp.x - u.x);',
  `if (!back) { u.rot = Math.atan2(wp.z - u.z, wp.x - u.x); ${event('face','budget')} }`);
walkReplace('u.x = next.x; u.z = next.z; budget -= advance;',
  `u.x = next.x; u.z = next.z; budget -= advance; ${event('advance','budget')}`);
walkReplace('if (d <= advance + 1e-9) { if (traffic.yielding) break; route.shift(); } else break;',
  `if (d <= advance + 1e-9) { if (traffic.yielding) { ${event('reached-yield','budget')} break; } route.shift(); ${event('reached-shift','budget')} } else { ${event('partial','budget')} break; }`);
walkReplace(`    }
  }
  return { driveVehicle, walkRoute };`,`    }
    ${event('exit','budget')}
  }
  return { driveVehicle, walkRoute };`);
let erased=transformedWalk;
for(const [before,after] of [...walkEdits].reverse())erased=erased.replace(after,before);
if(erased!==originalWalk)throw Error('walk observers do not erase to exact pinned bytes');

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
  const source=path===resolve(reference,'shared/sim.js')?prefix+transformed+suffix:path===resolve(reference,'shared/movement.js')?prefix+transformedWalk:raw;
  const module=new SourceTextModule(source,{identifier:path});modules.set(path,module);return module;
};
const module=load(resolve(reference,'shared/sim.js'));
await module.link((specifier,parent)=>specifier==='aligned:test'?hookModule:load(resolve(dirname(parent.identifier),specifier)));
await module.evaluate();
const sim=module.namespace;
const scene=JSON.parse(readFileSync(new URL('./scenarios/aligned-mov11.json',import.meta.url)));
const scalar=value=>value===undefined?{absent:true}:typeof value==='number'?number(value):value;
const point=value=>value===undefined?{absent:true}:value===null?null:{x:number(value.x),z:number(value.z)};
const encode=value=>value===undefined?{absent:true}:typeof value==='number'?number(value):Array.isArray(value)?value.map(encode):value!==null&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([k,v])=>[k,encode(v)])):value;
const files=[];
const write=(name,value)=>{
  const path=resolve(output,name),bytes=JSON.stringify(value)+'\n';
  if(checking){if(readFileSync(path,'utf8')!==bytes){const failed=path+`.check-failed-${process.pid}`;writeFileSync(failed,bytes,{flag:'wx',mode:0o600});throw Error(`exact Source check failed: ${name}; actual retained at ${failed}`);}}
  else {writeFileSync(path,bytes,{flag:'wx',mode:0o600});chmodSync(path,0o400);}
};
function snapshot(g,births,deaths,inputs,roles,arrived){
  return {tick:g.tick,nextId:g.nextId,births,deaths,inputs,roles:[...roles],arrived:[...arrived],
    units:[...g.units.values()].map(u=>({id:u.id,owner:u.owner,kind:u.type,x:number(u.x),z:number(u.z),hp:number(u.hp),supp:number(u.supp),
      path:u.path.map(point),rot:number(u.rot),aim:number(u.aim),moveSpeed:number(u.moveSpeed),vx:scalar(u.vx),vz:scalar(u.vz),travelDir:scalar(u.travelDir),
      reverse:point(u.reverse),repath:number(u.repath),stuck:number(u.stuck),still:scalar(u.still),orders:encode(u.orders),
      targetId:u.targetId,attackId:u.attackId,amove:point(u.amove),face:u.face===null?null:encode(u.face),
      holdPos:scalar(u.holdPos),holdFire:scalar(u.holdFire),autoRetreat:scalar(u.autoRetreat),retreating:u.retreating,
      cooldown:number(u.cooldown),retarget:number(u.retarget),hasWorldGoal:!!u.worldGoal,outcome:scalar(u.moveOutcome)})),
    players:g.players.map(p=>({slot:p.slot,team:p.team,mp:number(p.mp),mun:resource(p.mun),fuel:resource(p.fuel),vp:number(p.vp),inc:number(p.inc)})),
    points:g.points.map(p=>({owner:p.owner,capper:p.capper,progress:number(p.progress),cut:p.cut})),winner:g.winner,
    footprints:[...g.units.values()].filter(u=>sim.UNITS[u.type].building).map(u=>({id:u.id,cells:[...u.cells],built:number(u.built)})),
    walk:structuredClone(walk),branchEvents:structuredClone(branches),motion:structuredClone(motion),retirements:structuredClone(retirements),drawCount:consumed.length};
}
function run(authored){
  consumed=[];layoutReceipt=null;branches=[];motion=[];retirements=[];walk=[];pathCalls=0;
  const profile=scene.profile;
  const g=sim.createGame(structuredClone(authored.map),['A','B'],true,[0,1],[0,1],{mode:profile.mode,army:profile.army,sandbox:false,logistics:false,tech:true,supply:true,weather:'clear',mapKey:'aligned-mov11'});
  const startup={spawns:g.players.map(p=>[bits(p.spawn.x),bits(p.spawn.z)]),environmentSeed32:g.seed>>>0,weatherSeed:1,
    wind:[bits(g.wind.a),bits(g.wind.v)],shower:g.wx?{raining:g.wx.raining,next:bits(g.wx.next)}:null,rain:bits(g.wx.rain),wet:bits(g.wx.wet),weather:g.weather,nextId:g.nextId};
  const own=[...g.units.values()].filter(u=>u.owner===0&&u.type==='rifle').map(u=>u.id);
  const ownMg=[...g.units.values()].filter(u=>u.owner===0&&u.type==='mg').map(u=>u.id);
  const mobile=[...g.units.values()].filter(u=>!sim.UNITS[u.type].structure).map(u=>u.id);
  write(authored.name+'.constructor.json',{initialRoster:encode([...g.units.values()]),startup,
    inputReceipt:{capacity:values.length,consumed:consumed.length,unconsumed:values.length-consumed.length,exhausted:consumed.length===values.length,draws:structuredClone(consumed),layout:structuredClone(layoutReceipt),startup}});
  if(own.length!==2||ownMg.length!==1||mobile.length!==6)throw Error('ordinary Endless constructor roster changed; no synthetic actors');
  const roles=authored.kind==='bridge'?Array(18).fill(null):[own[0],own[1]],arrived=new Set(),destinations=new Map();
  if(authored.kind==='bridge'){roles[0]=own[0];roles[4]=own[1];roles[2]=ownMg[0];}
  const row={name:authored.name,startup,frames:[],completed:0,assertions:[],routeCalls:[],inputReceipt:null,failure:null};
  row.frames.push(snapshot(g,[...g.units.keys()],[],[],roles,arrived));
  const assertion=(label,pass)=>{row.assertions.push({tick:g.tick,label,passed:!!pass});if(!pass)throw Error(`original predicate/precondition: ${label}`);};
  // Private observation only. No terrain, path, traffic or recipient provider is called.
  const coverOperands = stage => {
    const fields=['id','owner','type','x','z','hp','supp','path','worldGoal','routeEnd','moveResult','moveOutcome',
      'holdPos','holdFire','autoRetreat','auto','follow','drift','build','dig','entrench','garrison','riding',
      'retreating','attackId','targetId','amove','face','reverse','repath','stuck','vx','vz','traffic',
      'trafficWait','trafficEntered','trafficTrail','queuedBehind','hitAt','hitFrom'];
    const units=[...g.units.values()].map(u=>{
      const x=Math.floor(u.x/sim.CELL),z=Math.floor(u.z/sim.CELL);
      const cell=x<0||z<0||x>=g.w||z>=g.h?-1:z*g.w+x;
      return {fields:Object.fromEntries(fields.map(key=>[key,encode(u[key])])),
        infantry:scalar(sim.UNITS[u.type].infantry),cell,
        char:scalar(cell<0?undefined:g.chars[cell]),flags:scalar(cell<0?undefined:g.flags[cell]),
        inCover:scalar(sim.inCover(g,u))};
    });
    (row.coverOperands??=[]).push({stage,tick:g.tick,roles:[...roles],units});
  };
  const dist=(u,p)=>Math.hypot(u.x-p.x,u.z-p.z);
  const buySlots=authored.kind==='bridge'?roles.map((id,index)=>id===null?index:null).filter(i=>i!==null):[];
  try {
    for(let tick=1;tick<=authored.ticks;tick++){
      branches=[];motion=[];retirements=[];walk=[];
      const inputs=[],callStart=pathCalls;row.pendingInputs=inputs;
      const issue=(owner,command)=>{
        const reason=sim.command(g,owner,command);
        inputs.push({owner,command:structuredClone(command),denial:scalar(reason)});
        if(reason!==undefined)throw Error(`actual command denied ${JSON.stringify({tick,owner,command,reason})}`);
      };
      const move=(ids,targets,extra={})=>issue(0,{t:'move',orders:ids.map((id,i)=>[id,targets[i].x,targets[i].z]),...extra});
      if(tick===1)for(const owner of [0,1])for(const key of ['holdPos','holdFire','autoRetreat'])issue(owner,{t:'stance',ids:mobile.filter(id=>g.units.get(id).owner===owner),key,on:key!=='autoRetreat'});
      if(authored.kind==='movement'){
        if(tick===2)move([own[0]],[{x:5,z:5}],{face:0});
        if(tick===201){const u=g.units.get(own[0]);assertion('Movement original (5,5) start',u.x===5&&u.z===5&&u.rot===0&&u.hp===100);move([own[0]],[{x:30,z:30}]);}
      } else if(authored.kind==='bridge'){
        if(tick===2)issue(0,{t:'tech',kind:'tier'});
        if(tick===1220)issue(0,{t:'build',ids:own,kind:'motorpool',...authored.buildAt});
        if(tick===2400){assertion('real completed motorpool', [...g.units.values()].some(u=>u.owner===0&&u.type==='motorpool'&&u.built>=1));issue(0,{t:'stop',ids:own});}
        const slot=buySlots[tick-2401];
        if(slot!==undefined){const old=new Set(g.units.keys());issue(0,{t:'buy',unit:scene.bridgeTypes[slot]});const born=[...g.units.values()].filter(u=>!old.has(u.id));assertion('one canonical paid recruit',born.length===1&&born[0].type===scene.bridgeTypes[slot]);roles[slot]=born[0].id;}
        if(tick===2420){
          assertion('all18 exact mixed types',roles.every((id,i)=>id!==null&&g.units.get(id).type===scene.bridgeTypes[i]));
          for(const key of ['holdFire','autoRetreat'])issue(0,{t:'stance',ids:roles,key,on:key==='holdFire'});
          issue(0,{t:'stance',ids:buySlots.map(i=>roles[i]),key:'holdPos',on:true});
          move(roles,roles.map((id,i)=>({x:57+i%3*4,z:(authored.variant==='reverse'?47-Math.floor(i/3)*4:77+Math.floor(i/3)*4)+(g.units.get(id).type==='medic'?(authored.variant==='reverse'?-2:2):0)})),{face:authored.variant==='reverse'?Math.PI/2:-Math.PI/2});
        }
        if(tick===2801){
          const medics=roles.map((id,i)=>({id,i})).filter(({id})=>g.units.get(id).type==='medic');
          move(medics.map(({id})=>id),medics.map(({i})=>({x:57+i%3*4,z:authored.variant==='reverse'?47-Math.floor(i/3)*4:77+Math.floor(i/3)*4})),{face:authored.variant==='reverse'?Math.PI/2:-Math.PI/2});
        }
        if(tick===3001){
          assertion('all18 original staged positions',roles.every((id,i)=>{const u=g.units.get(id);return u.x===57+i%3*4&&u.z===(authored.variant==='reverse'?47-Math.floor(i/3)*4:77+Math.floor(i/3)*4);}));
          assertion('all18 original health and facing',roles.every(id=>{const u=g.units.get(id),def=sim.UNITS[u.type];return u.hp===def.models*def.hpPer&&Math.cos(u.rot-(authored.variant==='reverse'?Math.PI/2:-Math.PI/2))>0.999;}));
          issue(0,{t:'stance',ids:buySlots.map(i=>roles[i]),key:'holdPos',on:false});
          issue(0,{t:'stop',ids:roles});
          const goals=roles.map((_,i)=>({x:51+i%6*4,z:authored.variant==='reverse'?89+Math.floor(i/6)*4:31+Math.floor(i/6)*4}));
          issue(0,{t:authored.variant==='attack-move'?'amove':'move',orders:roles.map((id,i)=>[id,goals[i].x,goals[i].z]),together:authored.variant==='together'});
          for(const id of roles){const u=g.units.get(id),goal=u.worldGoal??u.amove;assertion('real resolved destination exists',!!goal);destinations.set(id,{...goal,z:goal.z-(authored.variant==='queued'?12:0)});}
          if(authored.variant==='queued')move(roles,goals.map(p=>({...p,z:p.z-12})),{queue:true});
        }
        if(authored.variant==='deferred'&&(tick===3101||tick===3301)){g.pathBudget=tick===3101?0:4096;inputs.push({op:'pathBudget',value:g.pathBudget});}
      } else if(authored.kind==='cover'){
        if(tick===2)move(roles,[{x:21.81,z:20.1894},{x:23.1,z:20.1994}]);
        if(tick===401)coverOperands('before-command');
        if(tick===401){assertion('protected cover before true move',sim.inCover(g,g.units.get(roles[0])));move(roles,[{x:40,z:2},{x:41.29,z:2.01}]);}
        if(tick===401)coverOperands('after-command');
      } else if(authored.kind==='naval'){
        if(tick===2)for(const owner of [0,1])issue(owner,{t:'tech',kind:'tier'});
        if(tick===1220)for(const owner of [0,1])issue(owner,{t:'build',ids:mobile.filter(id=>g.units.get(id).owner===owner),kind:'shipyard',x:45,z:owner===0?17:111});
        if(tick===2401)for(const owner of [0,1]){const old=new Set(g.units.keys());issue(owner,{t:'buy',unit:'gunboat'});const born=[...g.units.values()].filter(u=>!old.has(u.id));assertion('real coastal gunboat',born.length===1&&born[0].type==='gunboat');roles[owner]=born[0].id;issue(owner,{t:'stance',ids:[roles[owner]],key:'holdFire',on:true});issue(owner,{t:'stance',ids:[roles[owner]],key:'autoRetreat',on:false});issue(owner,{t:'move',orders:[[roles[owner],75,owner===0?57:77]]});}
        if(tick===3001){assertion('both naval staging arrivals',roles.every((id,i)=>dist(g.units.get(id),{x:75,z:i===0?57:77})<0.5));issue(0,{t:'stance',ids:[roles[0]],key:'holdFire',on:false});issue(0,{t:'attack',ids:[roles[0]],target:roles[1]});}
        if(tick===3002){assertion('genuine live target before backing',g.units.get(roles[0]).targetId===roles[1]);move([roles[0]],[{x:75,z:49}]);}
      }
      const before=new Set(g.units.keys());sim.step(g);row.completed++;
      if(authored.kind==='bridge'&&tick>=3001){
        if(authored.variant==='deferred'&&g.pathBudget===0)assertion('zero budget performs no actual findPath calls',pathCalls===callStart);
        for(const id of roles){const u=g.units.get(id),cell=Math.floor(u.z/sim.CELL)*g.w+Math.floor(u.x/sim.CELL);assertion('no river displacement',g.chars[cell]!=='W');assertion('reachable order not abandoned',u.moveOutcome!=='unreachable');const goal=destinations.get(id);if(dist(u,goal)<(authored.variant==='attack-move'?2.5:0.5)&&!u.path.length&&!u.worldGoal&&!u.orders.length&&!arrived.has(id)){arrived.add(id);issue(0,{t:'stance',ids:[id],key:'holdPos',on:true});}}
      }
      if(authored.kind==='cover'&&tick===401)coverOperands('before-post-step-assertion');
      if(authored.kind==='cover'&&tick===401)assertion('friendly bridge traffic retains protected cover',sim.inCover(g,g.units.get(roles[0])));
      row.frames.push(snapshot(g,[...g.units.keys()].filter(id=>!before.has(id)),[...before].filter(id=>!g.units.has(id)),inputs,roles,arrived));
      row.pendingInputs=[];
      row.routeCalls.push({tick,actualFindPathCalls:pathCalls-callStart});
    }
    if(authored.kind==='movement')assertion('Movement arrival after full10seconds',dist(g.units.get(own[0]),{x:30,z:30})<1);
    if(authored.kind==='bridge')assertion('all18 original120second arrivals',arrived.size===18);
    if(authored.kind==='naval')assertion('naval backing actual arrival',dist(g.units.get(roles[0]),{x:75,z:49})<0.5&&!g.units.get(roles[0]).path.length);
  } catch(error){row.failure={tick:g.tick,message:String(error),stack:error.stack};}
  row.inputReceipt={capacity:values.length,consumed:consumed.length,unconsumed:values.length-consumed.length,exhausted:consumed.length===values.length,draws:consumed,layout:layoutReceipt,startup};
  return row;
}
let failure=null;
const covered=new Set();
try {
  for(const authored of scene.cases){
    const row=run(authored),name=authored.name+'.json';
    for(const frame of row.frames){const counts=new Map();for(const e of frame.walk){if(!frame.roles.includes(e.id))continue;covered.add(e.event);if(e.back&&e.event==='advance')covered.add('backing');if(e.event==='reached-shift')counts.set(e.id,(counts.get(e.id)??0)+1);}if([...counts.values()].some(n=>n>1))covered.add('multi-leg');}
    write(name,row);files.push({name:authored.name,file:name,completed:row.completed,frames:row.frames.length,failure:row.failure});
    if(row.failure){failure={case:authored.name,...row.failure};break;}
  }
  if(!failure){const missing=scene.requiredBranches.filter(name=>!covered.has(name));if(missing.length)failure={kind:'missingPositiveBranch',missing};}
} catch(error){failure={kind:'recorder',message:String(error),stack:error.stack};}
finally{Math.random=originalRandom;}
clean();
const manifest={format:'aligned-mov11-trace-v1',reference:pin,sourceHead,node:process.version,scene,sharedInput,cases:files,failure,covered:[...covered].sort(),
  instrumentation:{erasesExactly:true,sim:edits.map(([before,after])=>({before,after})),movement:walkEdits.map(([before,after])=>({before,after}))}};
write('manifest.json',manifest);
console.log(JSON.stringify({sourceHead,cases:files.map(({name,completed,failure})=>({name,completed,failure})),covered:manifest.covered,failure}));
if(failure)process.exitCode=2;
