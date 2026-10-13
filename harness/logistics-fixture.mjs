// Original-JavaScript oracle for reserve state only, without the game loop or convoy network.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url)),ref=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
assert.equal(execFileSync('git',['rev-parse','HEAD'],{cwd:ref,encoding:'utf8'}).trim(),'e4e579a838aabae3b1d9dadbc17a0a87147bc52d');
execFileSync('git',['diff','--exit-code','HEAD','--','shared'],{cwd:ref});
const L=await import(pathToFileURL(`${ref}/shared/logistics.js`)),{UNITS}=await import(pathToFileURL(`${ref}/shared/sim.js`));
const encode=v=>typeof v==='number'?(Number.isNaN(v)?{$number:'NaN'}:v===Infinity?{$number:'Infinity'}:v===-Infinity?{$number:'-Infinity'}:Object.is(v,-0)?{$number:'-0'}:v):Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;
const ctx=(mode,override,scenario=false)=>({...(mode===null?{}:{mode:{kind:mode}}),...(override===null?{}:{logisticsEnabled:override}),...(scenario?{scenario:{}}:{}),players:[{},{}]});
const contexts=[];
for(const mode of [null,'conquest','classic','annihilation','world','assault','horde','tutorial','','unknown'])for(const override of [null,true,false])for(const scenario of [false,true]){const g=ctx(mode,override,scenario);contexts.push({mode,override,scenario,expected:L.enabledFor(g)});}
const definitions=Object.entries(UNITS).map(([kind,def])=>({kind,price:L.ammoPrice(def)}));
definitions.push({kind:null,def:null,price:L.ammoPrice(null)});
for(const name of ['Ordinary','ROCKET launcher','howitzer','rocket','ſhell'])for(const w of [null,{},{salvo:true,rockets:1,range:79},{salvo:true,rockets:1,range:80},{salvo:true,rockets:2,range:20},{salvo:true,rockets:0,range:20},{shellTerrain:1},{veh:19.99},{veh:20},{range:1,interval:7,inf:1,perModel:true},{salvo:true,range:20,interval:0.7,inf:1,rockets:2.9},{salvo:true,range:20,interval:1,inf:1,rockets:NaN},{salvo:true,range:20,interval:1,inf:1,rockets:Infinity},{range:1,interval:Infinity,inf:1}]){const def={name,speed:2,models:7,w};definitions.push({kind:null,def,price:L.ammoPrice(def)});}
const initial=[];
for(let d=0;d<definitions.length;d++)for(const facts of [{owner:0,exists:true,guard:false},{owner:-1,exists:true,guard:false},{owner:0.5,exists:true,guard:false},{owner:2,exists:false,guard:false},{owner:0,exists:true,guard:true},{owner:0,exists:true,guard:false,type:'supplytruck'}]) {
 const entry=definitions[d],def=entry.kind?UNITS[entry.kind]:entry.def,u={id:1,type:facts.type??entry.kind??'synthetic',owner:facts.owner,guardHome:facts.guard};const g=ctx('conquest',true);if(!facts.exists)g.players=[];
 initial.push({definition:d,facts,expected:L.initializeUnit(g,u,def)});
}
const query=u=>({canFire:[0,0.99,1,1.5,10,1e6,-1,NaN,Infinity].map(n=>L.canFire(u,n)),recovered:L.recovered(u),shortage:L.shortageState(u),shortageRate:L.shortageRate(u),recoveryRate:L.recoveryRate(u)});
const traces=[];
function trace(kind,initial,steps,store={ammo:20,provisions:600,fuel:1000}) {
 const u={id:17,type:kind,owner:0,riding:false},g=ctx('conquest',true),def=UNITS[kind];if(initial!==undefined)u.logistics=initial===null?undefined:structuredClone(initial);else L.initializeUnit(g,u,def);
 let cargo=store===null?null:{...store};const record={kind,initial:structuredClone(u.logistics??null),store:cargo?{...cargo}:null,steps:[]};
 for(const step of steps){let result=null;if(step.op==='consume')result=L.consumeAmmo(u,step.count);else if(step.op==='tick'){g.logisticsEnabled=step.enabled??true;u.riding=step.riding??false;L.tickReserves(g,u,step.dt,step.travel??0);}else if(step.op==='drive'){u.riding=step.riding??false;L.consumeDrivingFuel(u,step.seconds);}else if(step.op==='transfer')result=L.transferStock(step.absent?null:cargo,u,step.seconds);else if(step.op==='initialize')result=!!L.initializeUnit(g,u,def);else throw Error(step.op);
 record.steps.push({...step,result,reserves:u.logistics??null,stock:cargo,query:query(u)});
 // Preserve this step's state, since the next call mutates the same original objects.
 record.steps[record.steps.length-1]=structuredClone(record.steps.at(-1));
 }
 traces.push(record);
}
const sequence=[...[0,0.2,1.8,-1,NaN,Infinity,-Infinity,1e6].map(count=>({op:'consume',count})),...Array.from({length:12},()=>({op:'transfer',seconds:0.0001})),{op:'tick',dt:119.5},{op:'tick',dt:0.5},{op:'tick',dt:19.5},{op:'tick',dt:0.5},{op:'initialize'},{op:'transfer',seconds:4},{op:'drive',seconds:179.5},{op:'drive',seconds:0.5},{op:'drive',seconds:59.5},{op:'drive',seconds:0.5},{op:'drive',seconds:50,riding:true},{op:'tick',dt:10,travel:10,riding:true},{op:'tick',dt:5,travel:9,enabled:false},{op:'initialize'},{op:'tick',dt:NaN,travel:Infinity},{op:'transfer',seconds:0},{op:'transfer',seconds:Infinity},{op:'transfer',seconds:-1},{op:'transfer',seconds:8},{op:'tick',dt:2,travel:1},{op:'initialize'},{op:'transfer',seconds:1,absent:true}];
for(const kind of ['rifle','conscript','mg','mortar','rocket','tank','lorry','howitzer','medic','engineer','supplytruck'])trace(kind,undefined,sequence);
trace('rifle',null,sequence);
for(const ammo of [24.999,25,49.999,50])for(const provisions of [0,29.999,30,59.999,60]) {
 const base={};L.initializeUnit(ctx('conquest',true),Object.assign(base,{type:'tank',owner:0}),UNITS.tank);
 Object.assign(base.logistics,{ammo,ammoMax:100,provisions,fuel:89.999,emergency:0,forced:true,exhausted:provisions===0,warning:0,ammoCredit:null});
 trace('tank',base.logistics,[{op:'tick',dt:0},{op:'transfer',seconds:0.01},{op:'transfer',seconds:8},{op:'drive',seconds:241},{op:'transfer',seconds:8}],{ammo:0.123,provisions:1000,fuel:1000});
}
for(const credit of [0.05-5e-10,0.05-2e-9]) { const u={owner:0,type:'tank'};L.initializeUnit(ctx('conquest',true),u,UNITS.tank);Object.assign(u.logistics,{ammo:0,ammoMax:1,ammoPrice:0.05,ammoCredit:credit,fuel:90,emergency:60});trace('tank',u.logistics,[{op:'transfer',seconds:1},{op:'tick',dt:0}],{ammo:0,provisions:0,fuel:0}); }
for(const seconds of [NaN,Infinity,-Infinity,-1,-0,0,0.001,1,8,1000]){const u={owner:0,type:'tank'};L.initializeUnit(ctx('conquest',true),u,UNITS.tank);Object.assign(u.logistics,{ammo:0,provisions:0,fuel:0,emergency:0,exhausted:true,forced:true,stranded:true,warning:0});trace('tank',u.logistics,[{op:'transfer',seconds}],{ammo:NaN,provisions:-5,fuel:Infinity});}
const result=JSON.stringify(encode({reference:'e4e579a',constants:L.LOGISTICS,contexts,definitions,initial,traces}))+'\n';
const path=`${root}/rust/crates/core/tests/fixtures/logistics.json`;if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),result);else writeFileSync(path,result);
console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} ${contexts.length} contexts, ${definitions.length} definitions, ${initial.length} initializations, ${traces.length} traces, ${traces.reduce((n,t)=>n+t.steps.length,0)} steps`);
