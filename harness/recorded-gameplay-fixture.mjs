// One recorded host transcript, executed by the pinned game handlers.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
const reference=resolve(process.env.WW2_REFERENCE??fileURLToPath(new URL('../reference/ww2-rts',import.meta.url)));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};clean();
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n;
const mix=x=>{x=((x^(x>>30n))*0xbf58476d1ce4e5b9n)&mask;x=((x^(x>>27n))*0x94d049bb133111ebn)&mask;return x^(x>>31n);};
const rawStream=(seed,purpose)=>{let state=mix(BigInt(seed)^((BigInt(purpose)*gamma)&mask));return ()=>mix(state=(state+gamma)&mask);};
const stream=(seed,purpose)=>{const raw=rawStream(seed,purpose);return ()=>Number(raw()>>11n)/9007199254740992;};
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const point=p=>p?[bits(p.x),bits(p.z)]:null;
const scalarFields=['rot','aim','hp','supp','cd','buff','sprint','cooldown','retarget','react','stuck','repath','pace','ghost','moveSpeed','xp','hitAt','jy','vy','reloadT','nadeT'];
function snapshot(g){return {
 tick:g.tick,nextId:g.nextId,nextFlight:g.nextFlight,
 players:g.players.map(p=>({slot:p.slot,team:p.team,faction:p.faction,away:!!p.away,out:!!p.out,mp:bits(p.mp),mun:bits(p.mun??0),fuel:bits(p.fuel??0),tier:p.tier,armory:p.armory,lab:p.lab,support:Object.fromEntries(Object.entries(p.sup).map(([k,v])=>[k,bits(v)]))})),
 units:[...g.units.values()].map(u=>({id:u.id,kind:u.type,owner:u.owner,at:point(u),scalars:Object.fromEntries(scalarFields.filter(k=>u[k]!==undefined).map(k=>[k,bits(u[k])])),path:u.path.map(point),worldGoal:point(u.worldGoal),amove:point(u.amove),reverse:point(u.reverse),nade:u.nade??null,auto:!!u.auto,autoRetreat:!!u.autoRetreat,holdFire:!!u.holdFire,holdPos:!!u.holdPos,ap:!!u.ap,drift:u.drift??false,operative:u.operative??null,follow:u.follow??0,attackId:u.attackId,targetId:u.targetId,garrison:u.garrison,orders:u.orders??[],face:u.face??null,weapon:u.weapon??null,mags:u.mags??null,nades:u.nades??null,traffic:u.traffic??null,trafficWait:bits(u.trafficWait??0),outcome:u.moveOutcome??null,outcomeTick:u.moveOutcomeTick??null,routeEnd:point(u.routeEnd)})),
 companions:[...(g.operatives?.values()??[])].map(op=>({key:op.key,owner:op.owner,unitId:op.unitId,squadId:op.squadId,online:op.online,lastSeq:op.lastSeq,lastInput:op.lastInput,input:op.input,seen:op.seen,want:op.want,respawnAt:op.respawnAt,shots:op.shots,hits:op.hits,kills:op.kills})),
 strikes:g.strikes,smokes:g.smokes,covers:g.covers??[],shots:g.shots,
 story:(g.story??[]).map(s=>({mpSpent:bits(s.mpSpent??0),supportCalls:bits(s.supportCalls??0)})),
};}
const seed=0x7107,map={name:'Recorded gameplay mix',w:40,h:30,rows:Array(30).fill('.'.repeat(40)),spawns:[{x:3,y:3},{x:35,y:25}],points:[{id:'front',x:20,y:15}]};
const options={mode:'conquest',weather:false,tech:true,logistics:false};
const originalRandom=Math.random,initializationRandom=[];
const startupWeather=rawStream(seed,2),startupDraw=Number(startupWeather()&0xffffffffn)/4294967296;
Math.random=()=>{assert.equal(initializationRandom.length,0,'Unexpected startup draw');initializationRandom.push(startupDraw);return startupDraw;};
let g;try{g=sim.createGame(map,['Commander','Enemy'],false,[0,1],[0,1],options);}finally{Math.random=originalRandom;}
for(const p of g.players)p.mp=5000.125;
const initial=structuredClone(snapshot(g)),newMg=g.nextId,records=[];
const add=(tick,issuer,sequence,operation)=>records.push({key:{tick,issuer,sequence},operation});
const command=(tick,issuer,sequence,value)=>add(tick,issuer,sequence,{kind:'command',value});
command(0,0,0,{t:'buy',unit:'mg'});
command(0,0,1,{t:'ability',ids:[newMg]});
command(0,0,2,{t:'autocast',ids:[newMg],on:true});
command(0,0,3,{t:'tech',kind:'tier'});
command(0,0,4,{t:'support',kind:'smoke',x:37,z:25,dir:.3});
command(0,0,5,{t:'move',orders:[[newMg,35,25]]});
command(0,0,6,{t:'stop',ids:[newMg]});
add(0,0,7,{kind:'trusted_join',journalId:7,owner:0});
const input=seq=>({seq,forward:0,strafe:0,yaw:0,pitch:0,fire:false,ads:false,crouch:false,sprint:false,weapon:0,jump:0,reload:0,nade:0,squad:0});
add(0,0,8,{kind:'trusted_input',journalId:7,owner:0,value:input(0)});
add(0,0,9,{kind:'trusted_input',journalId:7,owner:0,value:input(0)});
command(0,0,10,{t:'buy',unit:'rifle',orders:[[newMg,23,'bad']]});
command(0,0,11,{t:'support',kind:'cover',x:37,z:25,orders:[[newMg,23,'bad']]});
command(0,1,0,{t:'buy',unit:'rifle'});
add(0,1,1,{kind:'trusted_input',journalId:7,owner:0,value:input(1),hostGate:'issuer_owner_mismatch'});
add(1,0,12,{kind:'trusted_join',journalId:3,owner:0});
add(1,0,13,{kind:'trusted_input',journalId:3,owner:0,value:input(0)});
add(1,0,14,{kind:'trusted_input',journalId:7,owner:0,value:input(1)});
command(1,0,15,{t:'move',orders:[[newMg,39,25]]});
command(1,1,2,{t:'tech',kind:'inf'});
command(2,0,16,{t:'buy',unit:'rifle',from:999999});
command(2,0,17,{t:'support',kind:'cover',x:37,z:25});
command(3,0,18,{t:'buy',unit:'rifle',from:999999});
command(3,0,19,{t:'buy',unit:'rifle'});
add(3,0,20,{kind:'trusted_input',journalId:7,owner:0,value:input(2)});
const boundaries=[{tick:0,players:[{owner:0,fields:{mp:5000.125}},{owner:1,fields:{mp:5000.125}}]},{tick:1,players:[{owner:0,fields:{away:true}}]},{tick:2,players:[{owner:0,fields:{away:false,mp:0}}]},{tick:3,players:[{owner:0,fields:{mp:1000.125}}]}];
// This sorting is host OrderKey policy, not a replacement for a source handler.
const sorted=[...records].sort((a,b)=>a.key.tick-b.key.tick||a.key.issuer-b.key.issuer||a.key.sequence-b.key.sequence);
const arrivals=[...records].reverse().map(r=>r.key),operationRandom=[],random=stream(seed,3),checkpoints=[];
let currentTick=-1;
for(const record of sorted){
 const {key,operation}=record;
 if(key.tick!==currentTick){currentTick=key.tick;g.tick=key.tick;for(const patch of boundaries.find(b=>b.tick===key.tick)?.players??[])Object.assign(g.players[patch.owner],patch.fields);}
 const previous=Math.random;Math.random=()=>{const v=random();operationRandom.push(v);return v;};
 let result=null,executed=true;
 try{
  if(operation.hostGate){executed=false;}
  else if(operation.kind==='command')result=sim.command(g,key.issuer,operation.value)??null;
  else if(operation.kind==='trusted_join'){const op=sim.joinOperative(g,`journal-companion:${operation.journalId}`,operation.owner);result=op?{owner:op.owner,unitId:op.unitId}:null;}
  else if(operation.kind==='trusted_input')result=sim.operativeInput(g,`journal-companion:${operation.journalId}`,operation.value);
  else throw Error(operation.kind);
 }finally{Math.random=previous;}
 checkpoints.push({key,operation,executed,...(executed?{sourceResult:result}:{nativeHostExpected:'issuer_owner_mismatch',sourceHandler:null}),expected:structuredClone(snapshot(g))});
}
const at=(tick,issuer,sequence)=>checkpoints.find(c=>c.key.tick===tick&&c.key.issuer===issuer&&c.key.sequence===sequence);
assert.equal(at(0,0,0).sourceResult,null);assert.equal(at(0,0,1).sourceResult,null);assert.equal(at(0,0,3).sourceResult,null);assert.equal(at(0,0,4).sourceResult,null);
assert.equal(at(0,0,0).expected.units.find(u=>u.id===newMg).kind,'mg');
assert.ok(at(0,0,1).expected.units.find(u=>u.id===newMg).scalars.buff!==bits(0));
assert.equal(at(0,0,2).expected.units.find(u=>u.id===newMg).auto,true);
assert.equal(at(0,0,3).expected.players[0].lab[0].kind,'tier');
assert.equal(at(0,0,4).expected.strikes[0].kind,'smoke');
assert.ok(at(0,0,5).expected.units.find(u=>u.id===newMg).path.length>0);
assert.equal(at(0,0,6).expected.units.find(u=>u.id===newMg).path.length,0);
assert.equal(at(0,0,9).sourceResult,false);assert.equal(at(0,0,10).sourceResult,'blocked');assert.equal(at(0,0,11).sourceResult,'blocked');
assert.equal(at(1,0,12).sourceResult.owner,0);assert.equal(at(1,0,13).sourceResult,true);assert.equal(at(1,0,14).sourceResult,true);assert.equal(at(1,0,15).sourceResult,'blocked');
assert.equal(at(2,0,16).sourceResult,'mp');assert.equal(at(3,0,18).sourceResult,'needs');
assert.deepEqual(checkpoints.at(-1).expected.companions.map(op=>op.key),['journal-companion:7','journal-companion:3']);
clean();
const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,seed,scope:'Original command, joinOperative and operativeInput handler effects. Host OrderKey sorting and native issuer-owner rejection are explicitly separate. No full tick, armed support progression, socket authentication or FPS firing.',map,options,initializationRandom,operationRandom,initial,boundaries,arrivals,checkpoints})+'\n';
const path=fileURLToPath(new URL('../rust/crates/core/tests/fixtures/recorded-gameplay.json',import.meta.url));
if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),output);else writeFileSync(path,output);
console.log(`${checkpoints.length} recorded operations, ${checkpoints.filter(c=>c.executed).length} original handler calls, ${operationRandom.length} runtime random draws`);
