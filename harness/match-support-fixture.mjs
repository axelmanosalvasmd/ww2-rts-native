// Calls original command, strike, blast and timer code with the recorded native OffMap stream.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {gzipSync} from 'node:zlib';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`),pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
registerHooks({load(url,context,next){const r=next(url,context);if(url!==pathToFileURL(resolve(reference,'shared/sim.js')).href)return r;
 const src=String(r.source),a='  if (g.covers) { for (const c of g.covers)',b='  if (g.winner === null) stepScenario';
 assert.equal(src.split(a).length,2);assert.equal(src.split(b).length,2);
 return {...r,source:src+'\nexport {stepStrikes,setCell};\nexport function __supportTimers(g,dt){'+src.slice(src.indexOf(a),src.indexOf(b))+'}\n'};
}});
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const logistics=await import(pathToFileURL(resolve(reference,'shared/logistics.js')));
const grid=await import(pathToFileURL(resolve(reference,'shared/grid.js')));
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n,mix=x=>{x=((x^(x>>30n))*0xbf58476d1ce4e5b9n)&mask;x=((x^(x>>27n))*0x94d049bb133111ebn)&mask;return x^(x>>31n);};
const rng=seed=>{let s=mix(BigInt(seed)^((4n*gamma)&mask));return {next(){s=(s+gamma)&mask;return Number(mix(s)>>11n)/9007199254740992;},state:()=>s.toString(16).padStart(16,'0')};};
const buf=new DataView(new ArrayBuffer(8)),bits=n=>{buf.setFloat64(0,n);return buf.getBigUint64(0).toString(16).padStart(16,'0');};
const sparse=a=>Array.from(a,(v,i)=>v?[i,bits(v)]:null).filter(Boolean);
const snapshot=(g,random)=>{sim.internals.updateVision(g);return ({out:g.players.map(p=>p.out??false),fog:g.players.map(p=>Array.from(sim.teamFog(g,p.team).vis)),visible:g.players.map(p=>[...p.visible]),mp:g.players.map(p=>bits(p.mp)),cooldowns:g.players.map(p=>Object.fromEntries(Object.entries(p.sup).map(([k,v])=>[k,bits(v)]))),strikes:g.strikes.map(s=>({kind:s.kind,owner:s.owner,x:bits(s.x),z:bits(s.z),dir:bits(s.dir),t:bits(s.t),left:bits(s.left),next:bits(s.next),live:s.live})),covers:(g.covers??[]).map(c=>({team:c.team,owner:c.owner,x:bits(c.x),z:bits(c.z),r:bits(c.r),t:bits(c.t)})),smokes:g.smokes.map(s=>({id:s.id,x:bits(s.x),z:bits(s.z),r:bits(s.r),t:bits(s.t)})),units:[...g.units.values()].map(u=>({id:u.id,kind:u.type,owner:u.owner,x:bits(u.x),z:bits(u.z),riding:u.riding,cargo:u.cargo,ammo:u.logistics?bits(u.logistics.ammo):null,hp:bits(u.hp),supp:bits(u.supp),lastHit:u.lastHit??null,hitBy:u.hitBy??null,hitAt:bits(u.hitAt),retarget:bits(u.retarget)})),terrain:{chars:g.chars.join(''),height:Array.from(g.height??Array(g.w*g.h).fill(0)),hp:sparse(g.cellHp),wear:sparse(g.wear),scar:sparse(g.scar),versions:[g.terrainVersion??0,g.obstructionVersion??0,g.infantryRegionVersion??0,g.vehicleRegionVersion??0,g.navalRegionVersion??0]},environment:[g.seed>>>0,g.nextId],story:(g.story??g.players.map(()=>({supportCalls:0,mpSpent:0}))).map(s=>[bits(s.supportCalls),bits(s.mpSpent),bits(s.planesDowned??0)]),shots:g.shots,random:random.state()});};
const map={name:'Support arena',w:40,h:40,rows:Array(40).fill('.'.repeat(40)),spawns:[{x:2,y:2},{x:37,y:37}],points:[{x:20,y:20,kind:'radio'}]};
const traces=[];
for(const kind of ['artillery','smoke','cover','recon','strafe','bombing','dive','para'])for(const faction of [0,3])for(const variation of [...['normal','radio','cut','poor','cooldown','clamp','fallback','late-unit','late-bridge','late-house'],...(kind==='para'?['unseen','full-pop','out-before-delivery']:[]),...(kind==='strafe'?['riders']:[]),...(sim.SUPPORT[kind].plane?['flak','flak-held','flak-unready','flak-allied','late-flak','fighters','multi-flak','flak-empty','flak-forced','flak-stranded']:[])]){
 const witness=rng(701),along=(witness.next()-.5)*24,side=(witness.next()-.5)*10,landing={x:41+Math.cos(.3)*along-Math.sin(.3)*side,z:41+Math.sin(.3)*along+Math.cos(.3)*side};
 const cx=Math.floor(landing.x/2),cy=Math.floor(landing.z/2),cell=cy*40+cx;
 const arena=structuredClone(map);if(variation==='late-bridge'||variation==='late-house'){arena.rows[cy]='.'.repeat(cx)+(variation==='late-bridge'?'=':'B')+'.'.repeat(39-cx);arena.points[0].x=25;arena.points[0].y=25;}
 const original=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(arena,['A','B'],false,[0,1],[faction,1],{weather:false,logistics:false,tech:false});}finally{Math.random=original;}
 g.units.clear();const capturedEmpty=[...g.units.values()];g.nextId=1;const u=sim.internals.spawnUnit(g,variation==='flak-allied'||kind==='para'&&!variation.includes('flak')&&variation!=='unseen'?0:1,variation.includes('flak')?'flak':variation==='late-house'||variation==='riders'?'tank':'rifle');Object.assign(u,{x:variation==='late-house'?(cx+1.5)*2:variation==='late-bridge'?(cx+.5)*2:landing.x,z:variation==='late-house'||variation==='late-bridge'?(cy+.5)*2:landing.z,still:variation==='flak-unready'?1:20,holdFire:variation==='flak-held',retarget:.5});if(variation==='late-house'){g.structuralCells.clear();g.structures.clear();}if(variation==='late-house'||variation==='late-bridge'){g.cellHp[cell]=40;if(variation==='late-bridge')g.structuralCells.get(cell).hp=40;}grid.rebuildGrid(g);g.nextId=2;g.seed=123;g.wind={a:0,v:.5};g.wx={rain:0,wet:0};
 for(const p of g.players)p.mp=variation==='poor'?0:500;
 if(variation==='cooldown')g.players[0].sup[kind]=1;
 if(variation==='radio'||variation==='cut'){g.points[0].owner=0;g.points[0].cut=variation==='cut';}
 if(kind==='para'&&variation.includes('flak')){const observer=sim.internals.spawnUnit(g,0,'rifle');Object.assign(observer,{x:41,z:41,still:20});}
 if(variation==='multi-flak'){for(let i=0;i<2;i++){const f=sim.internals.spawnUnit(g,1,'flak');Object.assign(f,{x:41,z:41,still:20});}}
 if(['flak-empty','flak-forced','flak-stranded','multi-flak'].includes(variation))for(const f of g.units.values())if(f.type==='flak'){
  logistics.initializeUnit({...g,logisticsEnabled:true},f,sim.UNITS.flak);f.logistics.ammo=variation==='flak-empty'?0:2;
  f.logistics.forced=variation==='flak-forced'||variation==='flak-stranded';f.logistics.stranded=variation==='flak-stranded';
 }
 if(variation==='riders'){Object.assign(u,{x:41,z:41});for(let i=0;i<2;i++){const rider=sim.internals.spawnUnit(g,1,'rifle');Object.assign(rider,{x:41,z:41,riding:u.id});u.cargo.push(rider.id);}}
 if(variation==='full-pop')while(sim.popOf(g,0)+sim.dropPop('para')<=sim.popCap(g,0)){const extra=sim.internals.spawnUnit(g,0,'rifle');Object.assign(extra,{x:41,z:41});}
 grid.rebuildGrid(g);g.nextId=1000;
 const initialUnits=structuredClone([...g.units.values()].map(u=>({id:u.id,kind:u.type,owner:u.owner,x:u.x,z:u.z,still:u.still,holdFire:u.holdFire??false,retarget:u.retarget,riding:u.riding,cargo:u.cargo,logistics:u.logistics??null})));
 if(variation==='fighters')g.covers=[{team:1,owner:1,x:41,z:41,r:40,t:60}];
 const random=rng(701),frames=[];
 const request={t:'support',kind,x:variation==='clamp'?-20:41,z:variation==='clamp'?95:41,dir:variation==='fallback'?'bad':0.3};
 let denial=sim.command(g,0,request)??null;frames.push({action:{op:'admit',request},denial,expected:snapshot(g,random)});
 denial=sim.command(g,0,request)??null;frames.push({action:{op:'admit',request},denial,expected:snapshot(g,random)});
 for(let tick=1;tick<=1280;tick++){
  const prior=JSON.stringify([g.strikes.map(s=>s.live),(g.covers??[]).length]);
  if(variation==='out-before-delivery'&&tick===1)g.players[0].out=true;
  g.tick=tick;g.shots=[];const old=Math.random;Math.random=()=>random.next();try{sim.stepStrikes(g,variation.startsWith('late-')?capturedEmpty:[...g.units.values()],.05);sim.__supportTimers(g,.05);}finally{Math.random=old;}
  if(g.shots.length||prior!==JSON.stringify([g.strikes.map(s=>s.live),(g.covers??[]).length])||[1,39,40,59,60,99,100,109,119,159,199,239,240,360,400,480,1240,1280].includes(tick))frames.push({action:{op:'step',tick},denial:null,expected:snapshot(g,random)});
 }
 traces.push({kind,faction,variation,map:arena,units:initialUnits,unit:{kind:u.type,owner:u.owner,x:u.x,z:u.z,still:u.still,holdFire:u.holdFire},weakened:variation==='late-house'||variation==='late-bridge'?cell:null,frames});
}
const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,seed:701,traces})+'\n';
const path=`${root}/rust/crates/simtest/tests/fixtures/match-support.json.gz`,compressed=gzipSync(output,{level:9});
if(process.argv.includes('--check'))assert.deepEqual(readFileSync(path),compressed);else writeFileSync(path,compressed);
console.log(`${traces.length} support traces, ${traces.reduce((n,t)=>n+t.frames.length,0)} checkpoints`);
