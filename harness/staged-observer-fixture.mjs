// Record the original produceUnit and owner findPath over real knowledge adapters.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
assert.equal(process.arch, 'x64'); assert.equal(process.version, 'v24.21.0');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
assert.equal(git(['rev-parse', 'HEAD']), pin);
assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
const path = resolve(reference, 'shared/sim.js'); let source = readFileSync(path, 'utf8');
const probe = 'const view = knownTerrainFor(g, from.owner), next = to;';
assert.equal(source.split(probe).length, 2);
source = source.replace(probe, `${probe} oraclePathView(g, from, to, view);`);
source = source.replace(/from (['"])(\.\/[^'"]+)\1/g, (_, q, p) => `from ${q}${new URL(p, pathToFileURL(path)).href}${q}`);
source += '\nexport { produceUnit, knownTerrainFor, rebuildGrid };\nexport let oraclePathView=()=>{}; export const setPathProbe=f=>{oraclePathView=f};\n';
const sim = await import('data:text/javascript;base64,' + Buffer.from(source).toString('base64'));
const bits = n => { const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex'); };
const runs = values => { const out = []; for (const v of values) { if (out.length && out.at(-1)[0] === v) out.at(-1)[1]++; else out.push([v, 1]); } return out; };
const point = (x, z) => ({ x, z });
const starts = JSON.parse(readFileSync(`${root}/rust/crates/simtest/tests/fixtures/world-start.json`, 'utf8'));
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n;
const mix=x=>{x=((x^(x>>30n))*0xbf58476d1ce4e5b9n)&mask;x=((x^(x>>27n))*0x94d049bb133111ebn)&mask;return x^(x>>31n);};
const environmentSeed=seed=>Number(mix((mix(BigInt(seed)^((2n*gamma)&mask))+gamma)&mask)&0xffffffffn);
const cases = [];
function record(mode, warm, kind='rifle', advance=false, history=false, peerHistory=false) {
 const ordinary = { name:'Birth sight', w:64, h:64, rows:Array(64).fill('.'.repeat(64)), spawns:[{x:4,y:4},{x:59,y:59}], points:[{x:32,y:32}] };
 const startIndex=4, c=starts.cases[startIndex], map=structuredClone(mode==='world'?c.map:ordinary);
 if (map.world?.regionMapRuns) { map.world.regionMap=map.world.regionMapRuns.flatMap(([v,n])=>Array(n).fill(v)); delete map.world.regionMapRuns; }
 const players=mode==='world'?c.players:[{name:'A',team:0,faction:0},{name:'B',team:1,faction:1}];
 const settings=mode==='world'?{mode,army:c.settings.army,weather:c.settings.living?c.settings.weather:false,weatherSeed:c.weatherSeed,tech:c.settings.tech,logistics:false}:{mode,army:'standard',weather:'map',tech:false,logistics:false};
 const old=Math.random; Math.random=()=>mode==='world'?environmentSeed(c.seed)/4294967296:.5;
 let g; try { g=sim.createGame(map,players.map(p=>p.name),mode==='world'?c.settings.shuffle:false,players.map(p=>p.team),players.map(p=>p.faction),settings); } finally { Math.random=old; }
 g.units.clear(); g.players[0].mp=1000; sim.rebuildGrid(g);
 const buildingKind=kind==='tank'?'motorpool':'barracks';
 const building=sim.internals.spawnUnit(g,0,buildingKind);
 Object.assign(building,{x:51,z:51,built:1,queue:[],productionJobs:[],prog:0,rally:point(109,51)});sim.rebuildGrid(g);
 // Changed cells and a settled wreck lie beyond the producer's 24m sight,
 // but inside the newborn's sight after its actual stepOut placement.
 const changes=[{c:25*g.w+42,ch:'T'},{c:26*g.w+43,ch:'M'},{c:27*g.w+44,ch:'B'}];
 for(const change of changes)sim.internals.setCell(g,change.c,change.ch);
 g.wrecks.push({id:900001,type:'tank',owner:1,x:85.14,z:51.26,rot:0});
 let scout=null,peer=null;const historyReads=[],peerReads=[];
 if(history){scout=sim.internals.spawnUnit(g,0,kind);Object.assign(scout,{x:9,z:9});sim.rebuildGrid(g);}
 const fireCell=25*g.w+42;
 if(warm==='fire')g.fires.set(fireCell,20);
 let prior=null;
 const project=v=>({version:v.navigationKnowledgeVersion??0,chars:runs(v.chars),flags:runs(v.flags),height:runs(v.height),fires:[...v.fires.keys()],bunkers:[...v.units.values()].filter(u=>u.type==='bunker'&&u.hp>0).map(u=>[bits(u.x),bits(u.z)]),wrecks:v.wrecks.map(w=>[w.id,bits(w.x),bits(w.z),!!w.motion]),wet:v.wx?.wet??0});
 if(history){
  const goal=point(109,59);
  for(const change of [null,{c:31*g.w+31,ch:'B'},{c:31*g.w+32,ch:'B'},{c:31*g.w+31,ch:'T'}]){
   if(change)sim.internals.setCell(g,change.c,change.ch);
   let selected;sim.setPathProbe((_g,_from,_to,v)=>{selected=project(v)});
   const route=sim.findPath(g,scout,goal);
   historyReads.push({change,goal,view:selected,path:route.map(p=>[bits(p.x),bits(p.z)]),routeEnd:[bits(scout.routeEnd.x),bits(scout.routeEnd.z)]});
  }
  assert.ok(historyReads.at(-1).view.version>historyReads[0].view.version,'History must include accepted incremental changes');
  sim.setPathProbe(()=>{});
 }
 if(warm==='route'||warm==='fire')prior=project(sim.knownTerrainFor(g,0));
 if(warm==='fog')sim.teamFog(g,g.players[0].team);
 if(advance)g.tick++;
 g.fires.set(fireCell,20);
 const fog=()=>{const f=g.fog?.get(g.players[0].team);return f?{key:f.key,version:f.version,vis:runs(f.vis)}:null;};
 if(peerHistory){
  peer=sim.internals.spawnUnit(g,1,'medium');Object.assign(peer,{x:351,z:351});sim.rebuildGrid(g);
  const goal=point(433,367);
  for(const change of [null,{c:175*g.w+191,ch:'B'},{c:175*g.w+192,ch:'B'},{c:175*g.w+191,ch:'T'}]){
   if(change)sim.internals.setCell(g,change.c,change.ch);
   let selected;sim.setPathProbe((_g,_from,_to,v)=>{selected=project(v)});
   const route=sim.findPath(g,peer,goal);
   peerReads.push({change,goal,view:selected,path:route.map(p=>[bits(p.x),bits(p.z)]),routeEnd:[bits(peer.routeEnd.x),bits(peer.routeEnd.z)]});
  }
  assert.ok(peerReads.at(-1).view.version>peerReads[0].view.version);
  sim.setPathProbe(()=>{});
 }
 const beforeFog=fog(), views=[];
 sim.setPathProbe((_g,from,to,v)=>views.push({id:from.id,to,view:project(v)}));
 const scatterIndex=g.units.size;
 const beforeWallet={mp:g.players[0].mp,fuel:g.players[0].fuel??null};
 const nextId=g.nextId;
 const admission=mode==='world'?null:sim.command(g,0,{t:'buy',unit:kind,from:building.id})??null;
 const unit=mode==='world'?sim.produceUnit(g,building,0,kind):g.units.get(nextId);
 assert.ok(unit,`Original buy rejected ${kind}: ${admission}`);
 const afterWallet={mp:g.players[0].mp,fuel:g.players[0].fuel??null};
 assert.equal(views.length,1);
 const memory=mode==='world'?g.world.memory.get(g.players[0].team).cells:g.players[0].terrainMemory;
 cases.push({name:`${mode}:${kind}:${warm}:${advance?'next-tick':'same-tick'}${history?':history':''}${peerHistory?':peer-history':''}`,mode,startIndex:mode==='world'?startIndex:null,map:mode==='world'?null:ordinary,setup:{mp:1000,building:{id:building.id,kind:buildingKind,built:1,at:point(51,51),rally:building.rally},changes,wreck:{id:900001,x:85.14,z:51.26},fireCell,warm,advance,scatterIndex,peer:peer?{id:peer.id,kind:'medium',owner:1,at:point(351,351)}:null,peerReads,scout:scout?{id:scout.id,kind,at:point(9,9)}:null,historyReads},buy:{slot:0,type:kind,b:building.id,command:{t:'buy',unit:kind,from:building.id},admission,beforeWallet,afterWallet},id:unit.id,prior,beforeFog,view:views[0].view,expected:{pose:[unit.x,unit.z,unit.rot,unit.aim,unit.ghost].map(bits),hp:bits(unit.hp),path:unit.path.map(p=>[bits(p.x),bits(p.z)]),routeEnd:unit.routeEnd?[bits(unit.routeEnd.x),bits(unit.routeEnd.z)]:null,worldGoal:unit.worldGoal,drift:unit.drift,rows:[...memory.values()].map(r=>r.slice(0,4)),fog:fog()}});
 sim.setPathProbe(()=>{});
 if(peerHistory){let selected;sim.setPathProbe((_g,_from,_to,v)=>{selected=project(v)});const route=sim.findPath(g,peer,peerReads.at(-1).goal);cases.at(-1).peerAfter={view:selected,path:route.map(p=>[bits(p.x),bits(p.z)]),routeEnd:[bits(peer.routeEnd.x),bits(peer.routeEnd.z)]};sim.setPathProbe(()=>{});}
}
for(const warm of ['cold','route','fire','fog'])record('conquest',warm);
for(const warm of ['route','fire'])record('conquest',warm,'rifle',true);
record('conquest','route','rifle',false,true);record('conquest','route','tank',false,true);
record('conquest','cold','sniper');record('conquest','cold','tank');
for(const warm of ['cold','route','fire'])record('world',warm);
record('world','cold','rifle',false,false,true);
const out=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases})+'\n';
const destination=`${root}/rust/crates/simtest/tests/fixtures/staged-observer.json`;
if(process.argv.includes('--check'))assert.equal(readFileSync(destination,'utf8'),out);else writeFileSync(destination,out);
console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} ${cases.length} actual produceUnit birth sight cases`);
