// Actual pinned command branches, isolated from the unimplemented complete aircraft tick.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {gzipSync} from 'node:zlib';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`),pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const buf=new DataView(new ArrayBuffer(8)),bits=n=>{buf.setFloat64(0,n);return buf.getBigUint64(0).toString(16).padStart(16,'0');};
const map={name:'Aircraft command arena',w:40,h:40,rows:Array(40).fill('.'.repeat(40)),spawns:[{x:3,y:3},{x:36,y:36}],points:[{id:'front',x:20,y:20}]};
const point=p=>p?{x:bits(p.x),z:bits(p.z)}:null;
const waiting=o=>({t:o.t,orders:o.orders??null,ids:o.ids??null,target:o.target??null,x:o.x??null,z:o.z??null});
const snapshot=g=>[...g.units.values()].map(u=>({id:u.id,owner:u.owner,kind:u.type,at:point(u),rot:bits(u.rot),hp:bits(u.hp),path:u.path,follow:u.follow??0,orders:u.orders.map(waiting),face:u.face?{a:bits(u.face.a),...point(u.face)}:null,traffic:!!u.traffic,trafficWait:bits(u.trafficWait??0),worldGoal:point(u.worldGoal),entrench:!!u.entrench,outcome:u.moveOutcome??null,outcomeTick:u.moveOutcomeTick??null,attackId:u.attackId,targetId:u.targetId,retreating:u.retreating,holdFire:!!u.holdFire,holdPos:!!u.holdPos,autoRetreat:u.autoRetreat,air:u.air?{state:u.air.state,fuel:bits(u.air.fuel),ammo:bits(u.air.ammo),timer:bits(u.air.timer),ang:bits(u.air.ang),mission:u.air.mission?{kind:u.air.mission.kind,id:u.air.mission.id??0,...point(u.air.mission)}:null}:null}));
const stale=id=>({follow:777,path:[{x:9,z:10}],orders:[{t:'move',orders:[[id,8,8]]}],face:{a:.75,x:22,z:23},traffic:{goal:{x:6,z:7},legs:[],passing:false,by:888,until:20},trafficWait:5,worldGoal:{x:19,z:20},entrench:{jobs:[]},moveOutcome:'moving',moveOutcomeTick:3,attackId:777,targetId:555,retreating:true});
const aircraft=(kind='fighter',state='out',owner=0,extra={})=>({kind,owner,x:40,z:40,holdFire:true,air:{state,fuel:50,ammo:sim.UNITS[kind].ammo,timer:5,ang:1,mission:{kind:'patrol',id:0,x:38,z:39}},...extra});
const friend={kind:'rifle',owner:0,x:46,z:47},enemy={kind:'fighter',owner:1,x:48,z:49,holdFire:true,air:{state:'station'}};
const traces=[];
function trace(name,specs,commands,visible=true){
 const old=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(map,['A','B'],false,[0,1],[0,1],{weather:false,logistics:false,tech:false});}finally{Math.random=old;}
 g.units.clear();g.nextId=1000;
 const inputs=structuredClone(specs);
 for(const spec of inputs){const u=sim.internals.spawnUnit(g,spec.owner,spec.kind);Object.assign(u,stale(u.id));const {kind,owner,air,...fields}=spec;Object.assign(u,structuredClone(fields));if(air)Object.assign(u.air,structuredClone(air));}
 for(const p of g.players)p.visible=new Set(visible?[...g.units.keys()]:[]);
 const initial=snapshot(g),frames=[];
 for(const row of commands){const command=structuredClone(row.command),denial=sim.command(g,row.owner??0,structuredClone(command))??null;frames.push({owner:row.owner??0,command,denial,expected:snapshot(g)});}
 traces.push({name,map,specs:structuredClone(specs),visible,initial,frames});
}
for(const kind of ['fighter','attacker','bomber'])for(const state of ['base','out','station','home','rearm']){
 trace(`${kind}-${state}-move`,[aircraft(kind,state)],[{command:{t:'move',orders:[[1000,100,-5]],face:1,together:true}}]);
 trace(`${kind}-${state}-stop`,[aircraft(kind,state)],[{command:{t:'stop',ids:[1000]}}]);
 trace(`${kind}-${state}-retreat`,[aircraft(kind,state)],[{command:{t:'retreat',ids:[1000]}}]);
}
for(const kind of ['fighter','attacker','bomber']){
 trace(`${kind}-escort`,[aircraft(kind),friend],[{command:{t:'escort',ids:[1000,1001],target:1001}}]);
 trace(`${kind}-escort-unseen`,[aircraft(kind),{...friend,owner:1}],[{command:{t:'escort',ids:[1000],target:1001}}]);
 trace(`${kind}-escort-air-denial`,[aircraft(kind),aircraft('fighter','out')],[{command:{t:'escort',ids:[1000],target:1001}}]);
}
for(const state of ['base','out','station','home','rearm'])trace(`fighter-${state}-attack-air`,[aircraft('fighter',state),enemy,friend],[{command:{t:'attack',ids:[1002,1000],target:1001}}]);
trace('fighter-attack-hidden',[aircraft(),enemy],[{command:{t:'attack',ids:[1000],target:1001}}],false);
trace('fighter-attack-allied',[aircraft(),aircraft('fighter','out',0)],[{command:{t:'attack',ids:[1000],target:1001}}]);
trace('move-duplicate-orders',[aircraft()],[{command:{t:'amove',orders:[[1000,11,12],[1000,15,16]]}}]);
trace('move-ignore-foreign-aircraft',[aircraft(),aircraft('bomber','out',1,{holdFire:false})],[{command:{t:'move',orders:[[1001,10,11],[1000,21,22],[9999,1,2]]}}]);
trace('move-ignore-operative-and-rider',[aircraft(),aircraft('attacker','out',0,{operative:'alpha'}),aircraft('bomber','out',0,{riding:1})],[{command:{t:'move',orders:[[1001,10,11],[1002,15,16],[1000,21,22]]}}]);
trace('stop-empty-selection',[aircraft()],[{command:{t:'stop',ids:[9999,-1]}}]);
trace('move-missing-selection',[aircraft()],[{command:{t:'move',orders:[[9999,11,12]]}}]);
trace('move-malformed-tail-before-cap',[aircraft()],[{command:{t:'move',orders:[...Array(50).fill([1000,11,12]),[1000,'bad',12]]}}]);
for(const [name,air] of [['fuel-empty',{fuel:0}],['ammo-empty',{ammo:0}]]){
 trace(`move-${name}-cleanup`,[aircraft('fighter','out',0,{air:{state:'out',mission:{kind:'patrol',id:0,x:38,z:39},...air}})],[{command:{t:'move',orders:[[1000,21,22]]}},{command:{t:'stop',ids:[1000]}}]);
}
trace('recorded-plane-command-sequence',[aircraft('fighter','base',0,{holdFire:false}),enemy],[{command:{t:'stance',ids:[1000],key:'holdFire',on:true}},{command:{t:'move',orders:[[1000,21,22]]}},{command:{t:'attack',ids:[1000],target:1001}},{command:{t:'stop',ids:[1000]}}]);
trace('forced-ground-stance-remains-ineligible',[{...friend,logistics:{ammo:100,ammoMax:100,ammoPrice:.01,ammoCredit:0,provisions:120,fuel:180,emergency:60,warning:null,forced:true,stranded:false,exhausted:false}}],[{command:{t:'stance',ids:[1000],key:'holdPos',on:true}}]);
const checkpoints=traces.reduce((n,t)=>n+t.frames.length+1,0),fixture={reference:pin,node:process.version,arch:process.arch,traces,checkpoints};
const path=`${root}/rust/crates/simtest/tests/fixtures/match-aircraft-commands.json.gz`,compressed=gzipSync(JSON.stringify(fixture)+'\n',{level:9});
if(process.argv.includes('--check'))assert.deepEqual(readFileSync(path),compressed);else writeFileSync(path,compressed);
console.log(`${traces.length} actual aircraft command traces, ${checkpoints} checkpoints`);
