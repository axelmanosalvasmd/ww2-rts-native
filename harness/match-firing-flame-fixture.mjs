// CMB-61 extension. Keep the ordinary firing recorder and its fixture untouched.
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
 const salvo=source.slice(source.indexOf('  for (const s of g.salvos) {',source.indexOf(end)),source.indexOf('  stepStrikes(g, list, dt);',source.indexOf(end)));
 assert(salvo.includes('g.salvos = g.salvos.filter'));
 return {...result,source:source+'\nexport function __firing(g,id,moving,dt){ for(const u of [g.units.get(id)]) { const def=UNITS[u.type],w=def.w,sm=suppMul(u);\n'+block+'}\nexport function __salvos(g,dt){const list=[...g.units.values()];\n'+salvo+'}\nexport {claim,flightContact,stepFlights,setCell};\n'};
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
const spec=JSON.parse(readFileSync(`${root}/harness/match-firing-flame-inputs.json`,'utf8'));
assert.equal(spec.reference,pin);assert.equal(spec.seed,701);assert.equal(spec.cases.length,16);
const cases=[];
for(const authored of spec.cases){
 const old=Math.random;Math.random=()=>.5;let g;
 try{g=sim.createGame(spec.map,['A','B'],false,[0,1],[0,1],{weather:false,logistics:false,tech:false});}finally{Math.random=old;}
 g.units.clear();g.nextId=1;g.seed=123;g.wind={a:0,v:.5};g.wx={rain:0,wet:0};
 for(const v of authored.input){
  const u=sim.internals.spawnUnit(g,v.owner,v.kind);assert.equal(u.id,v.id);
  Object.assign(u,{x:v.x,z:v.z,rot:v.rot,hp:v.hp,still:v.still,supp:v.supp,xp:v.xp,hitBy:v.hitBy,hitAt:v.hitAt,hitFrom:v.hitFrom,
   targetId:v.target,shotAt:v.shotAt,holdFire:v.hold,attackId:v.attack,aim:v.aim,cooldown:v.cooldown,retarget:v.retarget,
   garrison:v.garrison??-1,auto:false,buff:0,ap:false});
 }
 for(const [cell,ch] of authored.cells)sim.setCell(g,cell,ch);
 for(const v of authored.input)if(v.garrison!==null){
  assert.equal(g.chars[v.garrison],'B');assert.equal(v.x,(v.garrison%g.w+.5)*2);assert.equal(v.z,(Math.floor(v.garrison/g.w)+.5)*2);
 }
 g.nextId=1000;g.players[0].visible=new Set(authored.visible);g.players[1].visible=new Set([1]);grid.rebuildGrid(g);
 const random=combatRandom(spec.seed),steps=[],u=g.units.get(1);
 for(let tick=1;tick<=authored.volleys;tick++){
  g.tick=tick;g.shots=[];
  // Authored readiness pulses exercise the firing component, with no arrivals or full-step claim.
  if(authored.group==='launch')u.cooldown=0;
  g.claims=new Map();for(const v of g.units.values())if(v.targetId&&v.hp>0)sim.claim(g,v,v.targetId,1);
  u.cooldown-=.05;u.retarget-=.05;
  const previous=Math.random;Math.random=()=>random.next();
  try{sim.__firing(g,1,false,.05);}finally{Math.random=previous;}
  steps.push({tick,expected:snapshot(g),random:random.draws.length?random.state():null,draws:random.draws.length});
 }
 if(authored.group==='launch'){
  const rounds=authored.shooter==='flamer'?1:5;
  assert.equal(g.flights.size,authored.volleys*rounds,authored.name);
  assert(random.draws.length>=g.flights.size&&random.draws.length<=3*g.flights.size,authored.name);
 }else{
  const preferred=authored.shooter==='flamer'&&authored.cover!=='open'&&authored.name.endsWith('-below');
  assert.equal(u.targetId,preferred?3:2,authored.name);
  assert.equal(g.flights.size,0,authored.name);assert.equal(random.draws.length,0,authored.name);
 }
 cases.push({name:authored.name,input:authored,steps});
}
const signature=c=>c.steps.at(-1).expected.flights.map(p=>p.velocity);
const named=name=>cases.find(c=>c.name===name);
for(const cover of ['trench','garrison']){
 assert.deepEqual(signature(named('launch-flamer-open')),signature(named(`launch-flamer-${cover}`)));
 assert.equal(named('launch-flamer-open').steps.at(-1).random,named(`launch-flamer-${cover}`).steps.at(-1).random);
 assert.notDeepEqual(signature(named('launch-rifle-open')),signature(named(`launch-rifle-${cover}`)));
}
clean();const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,seed:spec.seed,map:spec.map,cases})+'\n';
const path=`${root}/rust/crates/simtest/tests/fixtures/match-firing-flame.json.gz`;
const compressed=gzipSync(output,{level:9});
if(process.argv.includes('--check'))assert.deepEqual(readFileSync(path),compressed);else writeFileSync(path,compressed);
console.log(`Verified ${cases.length} flame firing extension scenes, ${cases.reduce((n,c)=>n+c.steps.length,0)} phases`);
