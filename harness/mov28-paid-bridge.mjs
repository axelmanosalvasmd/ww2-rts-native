// Original bridge predicates in a disclosed ordinary paid-command context.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { observation, install, raw, bits, point } from './mov28-bridge-observer.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const args = process.argv.slice(2);
assert([4,5].includes(args.length));
assert.deepEqual([args[0],args[2]], ['--reference','--output']);
const checking = args.length === 5;
if (checking) assert.equal(args[4], '--check');
const reference = resolve(args[1]), output = resolve(args[3]);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = (cwd,...args) => execFileSync('git',['-C',cwd,...args],{encoding:'utf8'}).trim();
assert.equal(process.arch,'x64'); assert.equal(process.version,'v24.21.0');
assert.equal(git(reference,'rev-parse','HEAD'),pin);
assert.equal(git(reference,'status','--porcelain','--untracked-files=no'),'');
assert.equal(git(root,'status','--porcelain'),'','freeze the child first');
const map = JSON.parse(readFileSync(resolve(root,'harness/scenes/mov28-paid-bridge.json'),'utf8'));
const trace = { format:'mov28-paid-bridge-v1', reference:pin, node:process.version, arch:process.arch,
  sourceHead:git(root,'rev-parse','HEAD'), map, constructors:observation.constructors,
  initial:null, frames:[], events:[], queries:[], assertions:[], failure:null };
let g;
process.on('uncaughtExceptionMonitor',error=>{trace.failure={message:String(error),stack:error.stack};});
process.on('exit',code=>{
  trace.exit=process.exitCode ?? code;
  trace.instrumentation=observation.instrumentation;
  trace.finalActualPrefix=g ? raw({tick:g.tick,nextId:g.nextId,units:[...g.units.values()],players:g.players,
    flags:g.flags,chars:g.chars,pathStats:g.pathStats,projects:g.projects}) : null;
  const text=JSON.stringify(trace)+'\n';
  writeFileSync(checking ? output+'.check-actual.json' : output,text,{flag:'wx',mode:0o400});
  if (checking) {
    const previous=JSON.parse(readFileSync(output,'utf8'));
    // Compare this component's declared inputs and outputs. Keep both raw startup
    // and per-tick inventories; they are not an aligned whole-match claim.
    const component=t=>Object.fromEntries(['format','reference','node','arch','sourceHead','map',
      'crew','events','queries','assertions','failure','exit'].map(key=>[key,t[key]]));
    assert.deepEqual(component(trace),component(previous));
  }
});
install(reference);
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
g=sim.createGame(map,['A','B'],false,[0,1],[0,1],
  {mode:'conquest',army:'standard',weather:false,supply:false,tech:false,logistics:false});
const roster=()=>[...g.units.values()].map(u=>({id:u.id,owner:u.owner,kind:u.type,x:bits(u.x),z:bits(u.z),hp:bits(u.hp),rot:bits(u.rot)}));
trace.initial=roster(); // Complete actual constructor inventory precedes every scene guard.
const initialIds=[...g.units.keys()];
const own=[...g.units.values()].filter(u=>u.owner===0&&u.type==='rifle');
assert.equal(own.length,2,'actual constructor rifles, never three invented rifles');
const crew=own[0]; trace.crew=crew.id;
const frame=()=>({tick:g.tick,units:roster(),crew:{id:crew.id,path:crew.path.map(point),paid:!!crew.dig},
  mp:g.players.map(p=>bits(p.mp))});
trace.frames.push(frame());
const require=(label,passed)=>{trace.assertions.push({tick:g.tick,label,passed});assert(passed,label);};
const issue=(owner,command,expected)=>{
  const row={tick:g.tick,owner,command:structuredClone(command),beforeMp:bits(g.players[owner].mp),result:null,afterMp:null};
  trace.events.push(row);
  const result=sim.command(g,owner,command);
  row.result=result===undefined ? null : result;row.afterMp=bits(g.players[owner].mp);
  assert.equal(row.result,expected,'actual ordinary command result');
};
const steps=count=>{for(let n=0;n<count;n++){
  sim.step(g);trace.frames.push(frame());
  require('all constructor actors retained',JSON.stringify([...g.units.keys()])===JSON.stringify(initialIds));
}};
for(const owner of [0,1]) {
  const ids=[...g.units.values()].filter(u=>u.owner===owner&&!sim.UNITS[u.type].structure).map(u=>u.id);
  for(const key of ['holdPos','holdFire','autoRetreat']) issue(owner,{t:'stance',ids,key,on:key!=='autoRetreat'},null);
}
steps(1);
issue(0,{t:'move',orders:[[crew.id,13,21]]},null);
steps(600);
require('original builder start at (13,21)',crew.x===13&&crew.z===21);
require('ordinary income can pay all three strips',g.players[0].mp>=3*sim.FORTS.bridge.cost);
const probe=label=>{
  assert(g.pathStats,'ordinary setup must have actual search statistics');
  const before={...g.pathStats};
  observation.near=null;observation.calls=[];observation.active=true;
  let path;
  try{path=sim.findPath(g,{x:5,z:21,type:'tank'},{x:35,z:21});}
  finally{observation.active=false;}
  const row={label,tick:g.tick,flags:[...g.flags],chars:g.chars.join(''),near:observation.near,providerCalls:observation.calls,
    expected:{points:path.map(point),expansions:g.pathStats.expansions-before.expansions,
      status:g.pathStats.deferred>before.deferred?'ExpansionLimit':g.pathStats.failed>before.failed?'Unreachable':'Found'},
    rawVersions:raw({infantry:g.infantryRegionVersion,vehicle:g.vehicleRegionVersion,naval:g.navalRegionVersion,
      obstruction:g.obstructionVersion,knowledge:g.navigationKnowledgeVersion,changedFrom:g.navigationChangedFromVersion,changedCells:g.navigationChangedCells,changes:g.navigationChanges})};
  trace.queries.push(row);return path;
};
require('the river stops vehicles',probe('river').length===0);
const beforeInvalid=g.players[0].mp;
issue(0,{t:'dig',ids:[crew.id],kind:'bridge',x:5,z:21,dir:0},'blocked');
require('invalid bridge does not debit',g.players[0].mp===beforeInvalid);
const buildTicks=(sim.CFG.digTime*3+1)/sim.TICK;
assert.equal(buildTicks,200,'unchanged original three-cell work window');
for(const [index,z] of [21,19,23].entries()) {
  const before=g.players[0].mp;
  issue(0,{t:'dig',ids:[crew.id],kind:'bridge',x:21,z,dir:0},null);
  require('each original bridge strip is paid',g.players[0].mp===before-sim.FORTS.bridge.cost);
  steps(buildTicks);
  require('builders remain on their bank',crew.x<18);
  if(index===0){
    require('the original three-cell span is built',[9,10,11].map(x=>g.chars[10*g.w+x]).join('')==='===');
    require('a 2 m span is narrower than the tank hull',probe('two-metres').length===0);
  }
}
require('the complete 6 m bridge is built',[9,10,11].every(y=>[9,10,11].every(x=>g.chars[y*g.w+x]==='=')));
require('vehicles have a path once physical width fits',probe('six-metres').length>0);
require('all original probes retained',trace.queries.length===3);
require('complete original windows retained',g.tick===1201);
assert.equal(git(reference,'rev-parse','HEAD'),pin);
assert.equal(git(reference,'status','--porcelain','--untracked-files=no'),'');
console.log('MOV28 complete paid bridge component: 3 path queries, 3 paid strips, 1 denied dig, 1,201 actual ticks');
