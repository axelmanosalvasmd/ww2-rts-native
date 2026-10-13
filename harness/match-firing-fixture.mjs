// Execute the unchanged reference target/fire block with the recorded native Combat stream.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');clean();
registerHooks({load(url,context,next){const result=next(url,context);if(url!==pathToFileURL(resolve(reference,'shared/sim.js')).href)return result;
 const source=String(result.source),begin='    // A vehicle standing still keeps its front armor',end='  // more engineers build faster';
 assert.equal(source.split(begin).length,2);assert.equal(source.split(end).length,2);
 const block=source.slice(source.indexOf(begin),source.indexOf(end));
 return {...result,source:source+'\nexport function __firing(g,id,moving,dt){ for(const u of [g.units.get(id)]) { const def=UNITS[u.type],w=def.w,sm=suppMul(u);\n'+block+'}\nexport {claim,flightContact,stepFlights,setCell};\n'};
}});
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const grid=await import(pathToFileURL(resolve(reference,'shared/grid.js')));
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n;
const mix=x=>{x=((x^(x>>30n))*0xbf58476d1ce4e5b9n)&mask;x=((x^(x>>27n))*0x94d049bb133111ebn)&mask;return x^(x>>31n);};
function combatRandom(seed){let state=mix(BigInt(seed)^((3n*gamma)&mask)),draws=[];return {draws,next(){state=(state+gamma)&mask;const v=Number(mix(state)>>11n)/9007199254740992;draws.push(v);return v;},state:()=>state.toString(16).padStart(16,'0')};}
const buf=new DataView(new ArrayBuffer(8));const bits=n=>{buf.setFloat64(0,n);return buf.getBigUint64(0).toString(16).padStart(16,'0');};
const vector=p=>[bits(p.x),bits(p.y),bits(p.z)];
function snapshot(g){return {units:[...g.units.values()].map(u=>[u.id,u.targetId,bits(u.cooldown),bits(u.retarget),bits(u.aim),bits(u.rot),u.shotAt??null]),claims:[...g.claims].sort((a,b)=>a[0]-b[0]).map(([k,v])=>[k,bits(v)]),flights:[...g.flights.values()].map(p=>({id:p.id,shooter:p.shooter,owner:p.owner,kind:p.kind,origin:vector(p),velocity:[bits(p.vx),bits(p.vy),bits(p.vz)],gravity:bits(p.gravity),stamp:bits(p.stamp),launched:bits(p.launched),last:bits(p.last),expires:bits(p.expires),energy:bits(p.energy),sequence:p.sequence,maxContacts:p.maxContacts,near:bits(p.near),startCell:p.startCell,intended:p.intended??null,damageScale:bits(p.damageScale??1),suppression:bits(p.suppression??0),suppressionScale:bits(p.suppressionScale??1),ap:!!p.ap,apMultiplier:bits(p.apMultiplier??1.5),apNaval:!!p.apNaval,explosive:!!p.explosive,terrain:bits(p.terrain??0),scar:bits(p.scar??0),blastRadius:p.blastRadius===undefined?null:bits(p.blastRadius),blastSource:!!p.blastSource,effect:p.effect??null,sent:!!p.sent})),shots:g.shots,salvos:g.salvos.map(s=>({owner:s.owner,at:[bits(s.x),bits(s.z)],left:s.left,next:bits(s.next),every:bits(s.w.every),spread:bits(s.spread)}))};}
const cases=[];
for(const shooter of ['rifle','mg','sniper','at','tank','flamer','mortar','howitzer','rocket','gunboat','destroyer'])for(const variation of ['plain','cover','moving','hold','unseen','hurt']){
 if(!sim.UNITS[shooter])continue;
 const naval=sim.UNITS[shooter].naval;
 const map={name:'Firing integration',w:80,h:24,rows:Array(24).fill((naval?'W':'.').repeat(80)),spawns:[{x:0,y:0},{x:79,y:23}],points:[{x:70,y:3}]};
 // Naval ships use an ordinary ground-start arena with a water band so the original constructor remains valid.
 if(naval){map.rows=map.rows.map((_,i)=>i<8?'.'.repeat(80):'W'.repeat(80));map.spawns=[{x:0,y:0},{x:79,y:0}];}
 const original=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(map,['A','B'],false,[0,1],[0,1],{weather:false,logistics:false,tech:false});}finally{Math.random=original;}
 g.units.clear();g.nextId=1;
 const u=sim.internals.spawnUnit(g,0,shooter),t=sim.internals.spawnUnit(g,1,naval?'gunboat':'rifle'),other=sim.internals.spawnUnit(g,1,naval?'gunboat':'tank');
 const range=sim.UNITS[shooter].w.range,minimum=sim.UNITS[shooter].w.minRange??0,distance=Math.max(minimum+2,Math.min(range-2,30));
 Object.assign(u,{x:25,z:naval?30:15,rot:0,aim:0,still:20,cooldown:0,retarget:0,targetId:0,auto:false,buff:0,ap:false,holdFire:variation==='hold'});
 Object.assign(t,{x:25+distance,z:u.z,still:20,rot:Math.PI});Object.assign(other,{x:t.x+2,z:t.z+3,still:20,rot:0});
 if(variation==='cover'){const c=Math.floor(t.z/2)*g.w+Math.floor(t.x/2);sim.setCell(g,c,'#');}
 if(variation==='hurt'){Object.assign(u,{supp:55,xp:sim.UNITS[shooter].cost*5,hitBy:3,hitAt:1,hitFrom:{x:other.x,z:other.z}});Object.assign(other,{targetId:1,shotAt:1});}
 g.players[0].visible=new Set(variation==='unseen'?[]:[2,3]);g.players[1].visible=new Set([1]);grid.rebuildGrid(g);
 const input=[...g.units.values()].map(u=>({id:u.id,kind:u.type,owner:u.owner,x:u.x,z:u.z,rot:u.rot,hp:u.hp,still:u.still,supp:u.supp,xp:u.xp,hitBy:u.hitBy??null,hitAt:u.hitAt,hitFrom:u.hitFrom??null,target:u.targetId,shotAt:u.shotAt??null,hold:u.holdFire,attack:u.attackId,aim:u.aim,cooldown:u.cooldown,retarget:u.retarget}));
 const random=combatRandom(701);const steps=[];
 for(const tick of [1,2,11,21]){g.tick=tick;g.shots=[];g.claims=new Map();for(const v of g.units.values())if(v.targetId&&v.hp>0)sim.claim(g,v,v.targetId,1);u.cooldown-=.05;u.retarget-=.05;
  const old=Math.random;Math.random=()=>random.next();try{sim.__firing(g,1,variation==='moving',.05);}finally{Math.random=old;}
  steps.push({tick,expected:snapshot(g),random:random.state()});
 }
 cases.push({map,shooter,variation,cells:variation==='cover'?[[Math.floor(t.z/2)*g.w+Math.floor(t.x/2),'#']]:[],input,visible:[...g.players[0].visible],steps});
}
clean();const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,seed:701,cases})+'\n';
const path=`${root}/rust/crates/simtest/tests/fixtures/match-firing.json`;
if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),output);else writeFileSync(path,output);
console.log(`Verified ${cases.length} Match firing traces, ${cases.reduce((n,c)=>n+c.steps.length,0)} phases`);
