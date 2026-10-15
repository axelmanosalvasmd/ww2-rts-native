// Original MOV-12 check and a disclosed retained-constructor native companion.
// No execution without a separate Root grant.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {readFileSync, writeFileSync} from 'node:fs';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {gzipSync} from 'node:zlib';
import {encode} from './state-codec.mjs';
const root = fileURLToPath(new URL('../', import.meta.url));
const ref = process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`;
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
assert.equal(process.version, 'v24.21.0'); assert.equal(process.arch, 'x64');
const args = process.argv.slice(2), checking = args.includes('--check');
const paths = args.filter(a => a !== '--check');
assert.equal(paths.length, 1); assert.equal(args.length, 1 + Number(checking));
const target = paths[0]; assert.ok(target.startsWith('/tmp/'));
const git = args => execFileSync('git', ['-C', ref, ...args], {encoding: 'utf8'}).trim();
const clean = () => { assert.equal(git(['rev-parse', 'HEAD']), pin); assert.equal(git(['status', '--porcelain', '--untracked-files=no']), ''); };
clean();
const raw = await import(pathToFileURL(`${ref}/shared/sim.js`));
const fixtures = await import(pathToFileURL(`${ref}/test-fixtures.js`));
const text = readFileSync(`${ref}/test.js`, 'utf8');
const section = (start, end) => { assert.equal(text.split(start).length, 2); const a = text.indexOf(start), b = text.indexOf(end, a + start.length); assert.ok(b > a); return text.slice(a, b); };
const name = 'Jam recovery keeps terrain corners when the next waypoint is obstructed';
const helpers = section('const blank = ', '\nawait check(');
const body = section(`await check(${JSON.stringify(name)},`, '\n});') + '\n});';
const hash = value => createHash('sha256').update(value).digest('hex');
const packet = {reference: pin, node: process.version, arch: process.arch, original: {
  name, helpers, body, sha256: hash(helpers + '\n' + body), constructors: [], commands: [], calls: [], completed: false}, companion: null, completed: false};
const data = new DataView(new ArrayBuffer(8));
const bits = n => { data.setFloat64(0, n); return data.getBigUint64(0).toString(16).padStart(16, '0'); };
const motion = u => ({id:u.id,x:bits(u.x),z:bits(u.z),rot:bits(u.rot),speed:bits(u.moveSpeed),
  path:u.path.map(p=>[bits(p.x),bits(p.z)]),vx:bits(u.vx),vz:bits(u.vz)});
const roster = g => [...g.units.values()].map(u => ({id:u.id,type:u.type,owner:u.owner,cells:u.cells??null}));
const priorRandom = Math.random; let failure;
try {
  Math.random = () => 0.5;
  const sim = {...raw};
  let originalSequence = 0;
  sim.createGame = (...args) => {
    const row = {sequence: originalSequence++, input: encode(args), completed: false};
    packet.original.constructors.push(row);
    try {
      const g = raw.createGame(...args);
      row.initial = encode(g, {runs: true}); row.completed = true;
      return g;
    } catch (error) {
      row.failure = {name:error.name,message:error.message,stack:error.stack};
      throw error;
    }
  };
  const observed = (rows, op, g, input, call) => {
    const row = {sequence: originalSequence++, op, input: encode(input),
      before: encode(g, {runs: true}), completed: false};
    rows.push(row);
    try {
      const result = call();
      row.returnedUndefined = result === undefined;
      if (result !== undefined) row.result = encode(result);
      row.after = encode(g, {runs: true}); row.completed = true;
      return result;
    } catch (error) {
      row.after = encode(g, {runs: true});
      row.failure = {name:error.name,message:error.message,stack:error.stack};
      throw error;
    }
  };
  sim.step = g => observed(packet.original.calls, 'step', g, null, () => raw.step(g));
  const command = (g, owner, cmd) => observed(packet.original.commands,
    'fixtureCommand', g, {owner,cmd}, () => fixtures.fixtureCommand(g, owner, cmd));
  let invoked = 0;
  const check = async (actual, fn) => { assert.equal(actual,name); invoked++; await fn(); };
  const AsyncFunction = Object.getPrototypeOf(async function(){}).constructor;
  await new AsyncFunction('bindings',`with (bindings) {\n${helpers}\n${body}\n}`)
    ({...sim,sim,assert,check,command,clearFixtureUnits:fixtures.clearFixtureUnits});
  assert.equal(invoked,1);
  assert.equal(packet.original.constructors.length,1);
  assert.equal(packet.original.commands.length,1);
  assert.equal(packet.original.calls.length,61);
  assert.equal(originalSequence,63);
  packet.original.completed=true;
  const map = {name:'MOV-12 canonical corner companion',w:80,h:96,
    rows:Array.from({length:96},(_,y)=>y===6?'.'.repeat(6)+'B'+'.'.repeat(73):'.'.repeat(80)),
    spawns:[{x:10,y:82},{x:70,y:82}],points:[{x:40,y:90,mp:0,vp:0}]};
  const options = {mode:'conquest',army:'standard',weather:false,tech:false,supply:false,logistics:false};
  const g = raw.createGame(map,['A','B'],false,[0,1],[0,1],options);
  const c = packet.companion = {map,options,initial:roster(g),commands:[],steps:[]};
  const retained = [...g.units.values()].map(u=>({u,cells:structuredClone(u.cells)}));
  const preserve = () => { for (const before of retained) { assert.equal(g.units.get(before.u.id),before.u); assert.deepEqual(before.u.cells,before.cells); } };
  const issue = cmd => { const old = new Set(g.units.keys()); const result = raw.command(g,0,cmd); const births = [...g.units.keys()].filter(id=>!old.has(id)); c.commands.push({input:structuredClone(cmd),result:result??null,births}); assert.equal(result,undefined); preserve(); return births; };
  g.players[0].mp = 10000;
  const hq = [...g.units.values()].find(u=>u.owner===0&&u.type==='hq'); assert.ok(hq);
  const births = issue({t:'buy',unit:'rifle',from:hq.id}); assert.equal(births.length,1);
  const u = g.units.get(births[0]); c.actor=u.id;
  Object.assign(u,{x:11,z:11,rot:0,aim:0,autoRetreat:false});
  raw.snapshotCache(g); raw.internals.updateVision(g);
  issue({t:'move',orders:[[u.id,17,15]]});
  // These are the original test's authored route inputs, never expected poses.
  const corner={x:11,z:15},end={x:17,z:15}; u.path=[corner,end]; u.was={x:u.x,z:u.z}; u.jam=3.1;
  c.authored={path:structuredClone(u.path),was:structuredClone(u.was),jam:u.jam};
  raw.step(g); preserve(); c.steps.push(motion(u));
  assert.equal(u.path[0],corner,'three seconds of crowding never skips a required corner');
  assert.equal(u.x,11,'the squad keeps moving along the clear leg');
  for(let i=0;i<60;i++){raw.step(g);preserve();c.steps.push(motion(u));}
  assert.ok(Math.hypot(u.x-end.x,u.z-end.z)<1,'the route still reaches its destination around the building');
  c.final=roster(g); c.completed=true; packet.completed=true;
} catch(error) { failure=error; packet.failure={name:error.name,message:error.message,stack:error.stack}; }
finally {
  Math.random=priorRandom;
  try {clean();}catch(error){failure??=error;packet.readbackFailure={message:error.message,stack:error.stack};}
  const bytes=gzipSync(JSON.stringify(packet)+'\n',{level:9});
  if(failure)writeFileSync(checking?`${target}.failed.gz`:target,bytes);
  else if(checking){if(!readFileSync(target).equals(bytes))writeFileSync(`${target}.different.gz`,bytes);assert.deepEqual(readFileSync(target),bytes);}
  else writeFileSync(target,bytes);
}
if(failure)throw failure;
console.log('Complete original MOV-12 callback and canonical companion: 61 ticks each; native proof pending');
