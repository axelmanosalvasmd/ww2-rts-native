// Execute unchanged source ability and FPS flight launch paths with the native Combat stream.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {gzipSync} from 'node:zlib';
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
 const nadeBegin=source.indexOf('        consumeAmmo(u, w?.perModel ? alive(u) : 1);',source.indexOf('    if (u.nade)'));
 const nadeEnd=source.indexOf('        u.nade = null;',nadeBegin);
 assert(nadeBegin>=0&&nadeEnd>nadeBegin);
 const nade=source.slice(nadeBegin,nadeEnd);
 const ticketBegin=source.indexOf('  for (const n of g.nades) {',source.indexOf('  stepFlights(g, flightPrior);'));
 const ticketEnd=source.indexOf('  for (const s of g.salvos) {',ticketBegin);
 const tickets=source.slice(ticketBegin,ticketEnd);
 const salvo=source.slice(source.indexOf('  for (const s of g.salvos) {',source.indexOf(end)),source.indexOf('  stepStrikes(g, list, dt);',source.indexOf(end)));
 assert(salvo.includes('g.salvos = g.salvos.filter'));
 return {...result,source:source+'\nexport function __abilityLaunch(g,u){const def=UNITS[u.type],w=def.w,ab=def.ab;\n'+nade+'}\nexport function __grenades(g,dt){const list=[...g.units.values()];\n'+tickets+'}\nexport function __firing(g,id,moving,dt){ for(const u of [g.units.get(id)]) { const def=UNITS[u.type],w=def.w,sm=suppMul(u);\n'+block+'}\nexport function __salvos(g,dt){const list=[...g.units.values()];\n'+salvo+'}\nexport {claim,flightContact,stepFlights,setCell,addFlight,launchSalvo,throwOperative};\n'};
}});
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const grid=await import(pathToFileURL(resolve(reference,'shared/grid.js')));
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n;
const mix=x=>{x=((x^(x>>30n))*0xbf58476d1ce4e5b9n)&mask;x=((x^(x>>27n))*0x94d049bb133111ebn)&mask;return x^(x>>31n);};
function combatRandom(seed){let state=mix(BigInt(seed)^((3n*gamma)&mask)),draws=[];return {draws,next(){state=(state+gamma)&mask;const v=Number(mix(state)>>11n)/9007199254740992;draws.push(v);return v;},state:()=>state.toString(16).padStart(16,'0')};}
const buf=new DataView(new ArrayBuffer(8));const bits=n=>{buf.setFloat64(0,n);return buf.getBigUint64(0).toString(16).padStart(16,'0');};
const vector=p=>[bits(p.x),bits(p.y),bits(p.z)];
const sparse=a=>Array.from(a,(v,i)=>v?[i,bits(v)]:null).filter(Boolean);
function snapshot(g){const retained=new Map();for(const p of g.flights.values())if(!g.units.has(p.shooter))retained.set(p.shooter,bits(p.source.xp));return {ammo:[...g.units.values()].filter(u=>u.logistics).map(u=>[u.id,bits(u.logistics.ammo)]),retained:[...retained],environment:[g.seed>>>0,g.nextId],terrain:{chars:g.chars.join(''),hp:sparse(g.cellHp),wear:sparse(g.wear),scar:sparse(g.scar),burnt:Array.from(g.burnt,(v,i)=>v?[i,v]:null).filter(Boolean),versions:[g.terrainVersion??0,g.obstructionVersion??0,g.infantryRegionVersion??0,g.vehicleRegionVersion??0,g.navalRegionVersion??0]},units:[...g.units.values()].map(u=>[u.id,u.targetId,bits(u.cooldown),bits(u.retarget),bits(u.aim),bits(u.rot),u.shotAt??null,bits(u.hp),bits(u.supp),bits(u.xp),u.hitBy??null,bits(u.hitAt),u.lastHit??null,u.hitFrom??null,u.deathImpulse??null]),claims:[...g.claims].sort((a,b)=>a[0]-b[0]).map(([k,v])=>[k,bits(v)]),flights:[...g.flights.values()].map(p=>({id:p.id,shooter:p.shooter,owner:p.owner,kind:p.kind,origin:vector(p),velocity:[bits(p.vx),bits(p.vy),bits(p.vz)],gravity:bits(p.gravity),stamp:bits(p.stamp),launched:bits(p.launched),last:bits(p.last),expires:bits(p.expires),energy:bits(p.energy),sequence:p.sequence,maxContacts:p.maxContacts,near:bits(p.near),startCell:p.startCell,intended:p.intended??null,damageScale:bits(p.damageScale??1),suppression:bits(p.suppression??0),suppressionScale:bits(p.suppressionScale??1),ap:!!p.ap,apMultiplier:bits(p.apMultiplier??1.5),apNaval:!!p.apNaval,explosive:!!p.explosive,terrain:bits(p.terrain??0),scar:bits(p.scar??0),blastRadius:p.blastRadius===undefined?null:bits(p.blastRadius),blastSource:!!p.blastSource,effect:p.effect??null,sent:!!p.sent})),shots:g.shots,salvos:g.salvos.map(s=>({owner:s.owner,at:[bits(s.x),bits(s.z)],left:s.left,next:bits(s.next),every:bits(s.w.every),spread:bits(s.spread)}))};}
function projection(g){const state=snapshot(g);state.blasts=[...g.flights.values()].map(p=>[p.id,...['inf','veh','terrain','supp','scar','antiGarrison'].map(k=>p.blastSource?.[k]===undefined?null:bits(p.blastSource[k]))]);state.grenades=g.nades.map(n=>[n.flight,n.owner,bits(n.x),bits(n.z),bits(n.t)]);state.nextFlight=g.nextFlight;return state;}
const cases=[];
const grenades=Object.entries(sim.UNITS).filter(([,d])=>['grenade','satchel'].includes(d.ab?.id));
const barrages=Object.entries(sim.UNITS).filter(([,d])=>d.ab?.id==='barrage');
for(const [kind,def] of [...grenades,...barrages])for(const variation of ['plain','garrison','ammo-low','edge']){
 if(variation==='garrison'&&def.naval||variation==='edge'&&kind!=='rifle')continue;
 const naval=def.naval,map={name:'Ability flights',w:80,h:24,rows:Array(24).fill('.'.repeat(80)),spawns:[{x:0,y:0},{x:79,y:0}],points:[{x:70,y:3}]};
 if(naval)map.rows=map.rows.map((v,i)=>i<8?v:'W'.repeat(80));
 const old=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(map,['A','B'],false,[0,1],[0,1],{weather:false,logistics:false,tech:false});}finally{Math.random=old;}
 g.units.clear();g.claims=new Map();g.nextId=1;g.seed=123;g.wind={a:0,v:.5};g.wx={rain:0,wet:0};
 const u=sim.internals.spawnUnit(g,0,kind),t=sim.internals.spawnUnit(g,1,naval?'gunboat':'rifle'),other=sim.internals.spawnUnit(g,1,naval?'gunboat':'tank');
 Object.assign(u,{x:25,z:naval?30:15,rot:0,aim:0,still:20,cooldown:0,retarget:0,targetId:0,holdFire:false,attackId:0,auto:false,buff:0,ap:false});
 const targetX=u.x+Math.min(10,def.ab.range??10);
 Object.assign(t,{x:targetX,z:u.z,rot:Math.PI,still:20});Object.assign(other,{x:targetX-1,z:u.z+3,rot:0,still:20});
 if(variation==='edge'){Object.assign(u,{x:145,z:45});Object.assign(t,{x:153,z:45});Object.assign(other,{x:154,z:47});}
 if(variation==='garrison')u.garrison=Math.floor(u.z/2)*g.w+Math.floor(u.x/2);
 if(variation==='ammo-low'&&def.w?.perModel)u.hp=def.hpPer*2;
 if(variation==='ammo-low')u.logistics={ammo:2,ammoMax:20,ammoPrice:1,ammoCredit:0,provisions:120,forced:false,stranded:false,exhausted:false};
 g.players[0].visible=new Set([2,3]);g.players[1].visible=new Set([1]);grid.rebuildGrid(g);g.nextId=1000;
 const input=[...g.units.values()].map(u=>({id:u.id,kind:u.type,owner:u.owner,x:u.x,z:u.z,rot:u.rot,hp:u.hp,still:u.still,supp:u.supp,xp:u.xp,hitBy:null,hitAt:u.hitAt,hitFrom:null,target:0,shotAt:null,logistics:u.logistics?structuredClone(u.logistics):null,hold:false,attack:0,aim:u.aim,cooldown:0,retarget:0}));
 const at=variation==='edge'?{x:160,z:48}:{x:targetX,z:u.z},random=combatRandom(701);g.tick=1;const saved=Math.random;Math.random=()=>random.next();try{
  if(def.ab.id==='barrage')sim.launchSalvo(g,u,at,def.ab.shells);
  else{u.nade=at;sim.__abilityLaunch(g,u);}
 }finally{Math.random=saved;}
 const steps=[{tick:1,expected:projection(g),random:random.state()}];g.shots=[];
 const checkpoints=[2,9,25,41,81,101];
 for(let tick=2;tick<=101;tick++){
  const prior=new Map([...g.units.values()].map(v=>[v.id,{x:v.x,z:v.z,rot:v.rot}]));g.tick=tick;
  const saved=Math.random;Math.random=()=>random.next();try{sim.stepFlights(g,prior);sim.__grenades(g,.05);sim.__salvos(g,.05);}finally{Math.random=saved;}
  if(checkpoints.includes(tick)){steps.push({tick,expected:projection(g),random:random.state()});g.shots=[];}
 }
 cases.push({map,kind,variation,input,visible:[2,3],cells:[],garrison:variation==='garrison'?u.garrison:null,at,barrage:def.ab.id==='barrage',requested:def.ab.shells??null,steps});
}
// Actual FPS throw path supplies its explicit duration and rifle ability profile.
for(const pitch of [-1,0,1]){
 const c=structuredClone(cases.find(c=>c.kind==='rifle'&&c.variation==='plain'));
 const old=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(c.map,['A','B'],false,[0,1],[0,1],{weather:false,logistics:false,tech:false});}finally{Math.random=old;}
 g.units.clear();g.claims=new Map();g.nextId=1;g.seed=123;g.wind={a:0,v:.5};g.wx={rain:0,wet:0};
 for(const v of c.input){const u=sim.internals.spawnUnit(g,v.owner,v.kind);Object.assign(u,{x:v.x,z:v.z,rot:v.rot,aim:v.aim,still:v.still,targetId:0,cooldown:0,retarget:0});}
 g.players[0].visible=new Set([2,3]);g.players[1].visible=new Set([1]);grid.rebuildGrid(g);g.nextId=1000;g.tick=1;
 const u=g.units.get(1);u.nades=3;sim.throwOperative(g,u,{yaw:.25,pitch});
 c.at={x:g.nades[0].x,z:g.nades[0].z};c.duration=g.nades[0].t;c.fps=true;c.variation=`fps-${pitch}`;c.steps=[{tick:1,expected:projection(g),random:combatRandom(701).state()}];g.shots=[];
 for(let tick=2;tick<=101;tick++){const prior=new Map([...g.units.values()].map(v=>[v.id,{x:v.x,z:v.z,rot:v.rot}]));g.tick=tick;sim.stepFlights(g,prior);sim.__grenades(g,.05);if([2,9,25,41,81,101].includes(tick)){c.steps.push({tick,expected:projection(g),random:combatRandom(701).state()});g.shots=[];}}
 cases.push(c);
}
clean();const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,seed:701,cases})+'\n';
const path=`${root}/rust/crates/simtest/tests/fixtures/ability-flights.json.gz`,compressed=gzipSync(output,{level:9});
if(process.argv.includes('--check'))assert.deepEqual(readFileSync(path),compressed);else writeFileSync(path,compressed);
console.log(`Verified ${cases.length} ability-flight traces, ${cases.reduce((n,c)=>n+c.steps.length,0)} checkpoints`);
