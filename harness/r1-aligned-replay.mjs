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
const map=JSON.parse(readFileSync(resolve(reference,'maps/default.json')));
const orders = [];
for (const [issuer, ids] of [[0, [1, 2, 3]], [1, [4, 5, 6]]]) {
  orders.push({tick: 1, issuer, sequence: 1, command: {t: 'stance', key: 'holdPos', on: true, ids}});
  orders.push({tick: 2, issuer, sequence: 2, command: {t: 'move', orders: [[ids[0], 81, 81]]}});
  orders.push({tick: 120, issuer, sequence: 3, command: {t: 'stop', ids: [ids[0]]}});
  orders.push({tick: 140, issuer, sequence: 4, command: {t: 'amove', orders: [[ids[0], 81, 81]]}});
  orders.push({tick: 400, issuer, sequence: 5, command: {t: 'stop', ids: [ids[0]]}});
}
orders.sort((a, b) => a.tick - b.tick || a.issuer - b.issuer || a.sequence - b.sequence);
const snapshot = g => ({
  tick: g.tick,
  units: [...g.units.values()].map(u => ({id: u.id, owner: u.owner, kind: u.type, x: number(u.x), z: number(u.z), hp: number(u.hp)})),
  players: g.players.map(p => ({slot: p.slot, team: p.team, mp: number(p.mp), mun: resource(p.mun), fuel: resource(p.fuel), vp: number(p.vp), inc: number(p.inc)})),
  mode: {requested: 'conquest', runtime: g.mode ?? null},
  score: {winner: g.winner, vp: g.players.map(p => number(p.vp))},
  points: g.points.map(p => ({owner: p.owner, capper: p.capper, progress: number(p.progress), cut: p.cut})),
});

let completed=0,failure=null,movedTicks=0,startup=null;
const frames=[],denials=[];
try{
  const g=sim.createGame(map,['A','B'],true,[0,1],[0,1],{mode:'conquest',army:'standard',sandbox:false,logistics:false,tech:true,supply:true,weather:'clear',mapKey:'default'});
  if(![1,2,3].every(id=>g.units.get(id)?.owner===0)||![4,5,6].every(id=>g.units.get(id)?.owner===1))throw Error('canonical IDs no longer bind original orders');
  startup={spawns:g.players.map(p=>[bits(p.spawn.x),bits(p.spawn.z)]),environmentSeed32:g.seed>>>0,weatherSeed:1,
    wind:[bits(g.wind.a),bits(g.wind.v)],shower:g.wx?{raining:g.wx.raining,next:bits(g.wx.next)}:null,rain:bits(g.wx.rain),wet:bits(g.wx.wet),weather:g.weather,nextId:g.nextId};
  frames.push(snapshot(g));
  for(let tick=1;tick<=600;tick++){
    const previous=[...g.units.values()].map(u=>[u.id,u.x,u.z]);
    for(const row of orders.filter(r=>r.tick===tick)){const reason=sim.command(g,row.issuer,row.command);if(reason)denials.push({tick,issuer:row.issuer,sequence:row.sequence,reason});}
    sim.step(g);completed++;
    if(previous.some(([id,x,z])=>{const u=g.units.get(id);return u&&(u.x!==x||u.z!==z);}))movedTicks++;
    frames.push(snapshot(g));
  }
}catch(error){failure={nextTick:completed+1,message:String(error),stack:error.stack};}
finally{Math.random=originalRandom;}
clean();
const inputReceipt={capacity:values.length,consumed:consumed.length,unconsumed:values.length-consumed.length,exhausted:consumed.length===values.length,draws:consumed,layout:layoutReceipt,startup};
const trace={format:'r1-whole-match-differential-v1',alignedFormat:'r1-aligned-inputs-v1',reference:pin,sourceHead,seed:seedText,
  sharedInput,inputReceipt,instrumentation:{erasesExactly:true,anchors:edits.map(([before,after])=>({before,after})),originalFunctions:'createGame, spawnSlots, rng, command, step and all called engine functions execute'},
  node:process.version,requestedTicks:600,checkpointEvery:10,map,
  profile:{mode:'conquest',army:'standard',weather:'clear',shuffle:true,tech:true,supply:true,livingWeather:true,logistics:false,sandbox:false,weatherSeed:null},
  orders,frames,completed,movedTicks,draws:consumed.length,denials,failure};
const bytes=JSON.stringify(trace)+'\n';
if(args.includes('--check')){
  const prior=JSON.parse(readFileSync(output));
  // A later fixture-only Git commit does not alter the producing source head.
  trace.sourceHead=prior.sourceHead;
  if(!isDeepStrictEqual(trace,prior))throw Error('fresh aligned Source capture differs from frozen fixture');
}else{writeFileSync(output,bytes,{mode:0o600});chmodSync(output,0o400);}
console.log(JSON.stringify({engine:'Source',sourceHead,completed,movedTicks,checkpoints:frames.filter(f=>f.tick%10===0).length,draws:consumed.length,unconsumed:values.length-consumed.length,failure}));
if(failure)process.exitCode=2;
