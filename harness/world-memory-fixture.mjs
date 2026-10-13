// Original World collectors over real createGame, fog, visibility and terrain mutation.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');
const ref=resolve(process.env.WW2_REFERENCE??fileURLToPath(new URL('../reference/ww2-rts/',import.meta.url))),pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',ref,...a],{encoding:'utf8'}).trim();
if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Use clean pinned reference');
const path=resolve(ref,'shared/sim.js');let source=readFileSync(path,'utf8');
const probe='m.cells.set(c, row); m.version++; m.log.push(c);';
if(source.split(probe).length!==2)throw Error('World row identity probe changed');
source=source.replace(probe,'oracleWorldRow(team,row,m.version+1); '+probe);
source=source.replace(/from (['"])(\.\/[^'"]+)\1/g,(_,q,p)=>`from ${q}${new URL(p,pathToFileURL(path)).href}${q}`);
source+='\nexport {worldTerrainMemory,worldTerrainFor,worldPathView,worldSnapshot,worldWrecksFor,rebuildGrid,logCell,touch};\nexport let oracleWorldRow=()=>{}; export const setWorldRowProbe=f=>{oracleWorldRow=f};\n';
const sim=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
const {generateWorldMap}=await import(pathToFileURL(resolve(ref,'shared/world-conquest.js')));
const starts=JSON.parse(readFileSync(new URL('../rust/crates/simtest/tests/fixtures/world-start.json',import.meta.url),'utf8'));
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n;
const mix=x=>{x=((x^(x>>30n))*0xbf58476d1ce4e5b9n)&mask;x=((x^(x>>27n))*0x94d049bb133111ebn)&mask;return x^(x>>31n);};
const environmentSeed=seed=>Number(mix((mix(BigInt(seed)^((2n*gamma)&mask))+gamma)&mask)&0xffffffffn);
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex')};
const hash=v=>{let h=0xcbf29ce484222325n;for(const b of Buffer.from(JSON.stringify(v)))h=BigInt.asUintN(64,(h^BigInt(b))*0x100000001b3n);return h.toString(16).padStart(16,'0');};
const runs=vs=>{const out=[];for(const v of vs){if(out.length&&out.at(-1)[0]===v)out.at(-1)[1]++;else out.push([v,1]);}return out;};
const cases=[];
function record(startIndex,actions,rawLabels=false){
 const c=starts.cases[startIndex],map=structuredClone(c.map);if(map.world.regionMapRuns){map.world.regionMap=map.world.regionMapRuns.flatMap(([v,n])=>Array(n).fill(v));delete map.world.regionMapRuns;}
 let mapShape=null;
 if(rawLabels){const labels=generateWorldMap({size:'huge',seed:1,players:2,teams:[0,1],scenery:false}).world.regionMap;map.w=512;map.h=512;map.rows=Array(512).fill('.'.repeat(512));map.heights=Array(512).fill('0'.repeat(512));map.world.regionMap=labels;mapShape={w:512,h:512,regionMapRuns:runs(labels)};}
 const old=Math.random;Math.random=()=>environmentSeed(c.seed)/4294967296;let g;
 try{g=sim.createGame(map,c.players.map(p=>p.name),c.settings.shuffle,c.players.map(p=>p.team),c.players.map(p=>p.faction),{mode:'world',army:c.settings.army,weather:c.settings.living?c.settings.weather:false,weatherSeed:c.weatherSeed,tech:c.settings.tech,logistics:false});}finally{Math.random=old;}
 const revisions=new WeakMap(),aliases=new Map(),snapshots=[];sim.setWorldRowProbe((team,row,rev)=>revisions.set(row,rev));
 const project=row=>{const d=row[4],s=d.section;return [row[0],row[1],row[2],row[3],d.ground,d.object,d.material,d.groundMaterial,bits(d.groundWear),d.trenchFront===undefined?null:bits(d.trenchFront),!!d.mine,d.mineMaterial??null,s?[s.id,s.structureId,s.state,bits(s.hp),bits(s.maxHp),s.material,s.anchor]:null,d.mineOwned??null,d.initial,d.biome??null,d.landmark??null,d.rail??null,d.station??null,revisions.get(row)];};
 const rows=vs=>{const a=[...vs].map(project);return {count:a.length,hash:hash(a)};};
 const wrecks=rs=>rs.map(r=>[r[0],bits(r[3]),bits(r[4]),r.length>6]);
 const get=a=>g.units.get(aliases.get(a.alias));
 for(const a of actions){let result=null;
  switch(a.op){
   case 'clearUnits':g.units.clear();sim.rebuildGrid(g);break;
   case 'spawn':{const u=sim.internals.spawnUnit(g,a.owner,a.kind);a.id=u.id;aliases.set(a.alias,u.id);Object.assign(u,{x:a.x,z:a.z,...a.fields});sim.rebuildGrid(g);break;}
   case 'move':Object.assign(get(a),{x:a.x,z:a.z,...a.fields});sim.rebuildGrid(g);break;
   case 'remove':g.units.delete(aliases.get(a.alias));sim.rebuildGrid(g);break;
   case 'vision':g.tick=a.tick;sim.internals.updateVision(g);break;
   case 'tick':g.tick=a.tick;break;
   case 'set':sim.internals.setCell(g,a.c,a.ch);break;
   case 'mutate':sim.mutateWorldCell(g,a.c,a.change);break;
   case 'mine':if(a.owner!==undefined)g.mines.set(a.c,a.owner);if(a.seen!==undefined)g.mineSeen.set(a.c,a.seen);break;
   case 'log':sim.logCell(g,a.c,[],a.look??false);break;
   case 'cycleChanges':for(let i=0;i<a.count;i++){sim.internals.setCell(g,a.c,i%2?'M':'T');sim.worldTerrainMemory(g,g.players[a.slot].team);}break;
   case 'touchMany':for(let i=0;i<a.count;i++)sim.touch(g,a.c);break;
   case 'reveal':g.reveal=a.value;break;
   case 'wet':g.wx.wet=a.value;break;
   case 'fire':a.value?g.fires.set(a.c,20):g.fires.delete(a.c);break;
   case 'region':Object.assign(g.world.regions[a.index],a.fields);break;
   case 'visible':g.players[a.slot].visible=new Set(a.aliases.map(k=>aliases.get(k)));break;
   case 'wreck':{const w={id:a.id,type:'tank',owner:1,rot:0,x:a.x,z:a.z};if(a.moving)w.motion={id:a.id,kind:'wreck',x:a.x,y:0,z:a.z,dir:0,material:'steel',mass:1,vx:1,vy:0,vz:1,gravity:9.8,drag:.9,stamp:0,born:0,tilt:0,settled:false};const prior=g.wrecks.find(w=>w.id===a.id);if(prior)Object.assign(prior,w,{motion:w.motion});else g.wrecks.push(w);break;}
   case 'removeWreck':g.wrecks=g.wrecks.filter(w=>w.id!==a.id);break;
   case 'read':result=rows(sim.worldTerrainFor(g,a.slot,a.full));break;
   case 'regions':result=sim.worldSnapshot(g,a.slot).regions.map(r=>[r.id,r.name??null,r.kind??null,r.team,bits(r.x),bits(r.z),r.bounds??null,r.runs??[],bits(r.progress),r.capper,r.locked,r.contested]);break;
   case 'wreckRead':result=wrecks(sim.worldWrecksFor(g,a.slot));break;
   case 'route':{const v=sim.worldPathView(g,a.slot);result={version:v.navigationKnowledgeVersion??0,changes:(v.navigationChanges??[]).map(c=>[c.from,c.to,c.cells]),known:[...g.world.memory.get(g.players[a.slot].team).cells.keys()].map(c=>[c,v.chars[c],v.flags[c],v.height[c],bits(v.cellHp[c]),bits(v.trenchFront[c])]),fires:[...v.fires.keys()].sort((a,b)=>a-b),bunkers:[...v.units.values()].filter(u=>u.type==='bunker'&&u.hp>0).map(u=>[bits(u.x),bits(u.z)]),wrecks:v.wrecks.map(w=>[w.id,bits(w.x),bits(w.z),!!w.motion]),wet:v.wx?.wet??0};result.knownHash=hash(result.known);result.knownCount=result.known.length;delete result.known;break;}
   default:throw Error(a.op);
  }
  snapshots.push({result:structuredClone(result),teams:[...g.world.memory].map(([t,m])=>[t,m.version,rows(m.cells.values()),m.regions.size,wrecks(m.wrecks.values())]),journal:[g.journalBase??0,g.cellJournal.length],versions:[g.tick,g.terrainVersion??0,g.visionTick??null],visible:g.players.map(p=>[...p.visible]),localVisible:[...g.players[-1].visible]});
 }
 cases.push({startIndex,mapShape,actions,snapshots});
}
// Shared team memory and independent player cursors, then unseen mutation and same-key mine discovery.
record(1,[
 {op:'clearUnits'},{op:'spawn',alias:'eye',owner:0,kind:'rifle',x:41,z:41},{op:'vision',tick:1},
 {op:'route',slot:0},{op:'read',slot:0,full:false},{op:'read',slot:1,full:false},{op:'read',slot:0,full:false},
 {op:'cycleChanges',c:10260,count:6200,slot:0},{op:'read',slot:0,full:false},{op:'route',slot:1},{op:'read',slot:1,full:false},
 {op:'set',c:10260,ch:'T'},{op:'set',c:10261,ch:'M'},{op:'route',slot:1},{op:'read',slot:0,full:false},
 {op:'mutate',c:10262,change:{mine:true}},{op:'route',slot:0},{op:'mine',c:10262,seen:1},{op:'log',c:10262,look:true},{op:'route',slot:0},
 {op:'vision',tick:2},{op:'route',slot:1},{op:'read',slot:1,full:false},
 {op:'mine',c:10262,owner:0},{op:'log',c:10262},{op:'route',slot:1},
 {op:'move',alias:'eye',x:201,z:201},{op:'set',c:10260,ch:'.'},{op:'route',slot:0},{op:'vision',tick:3},{op:'route',slot:0},
 {op:'set',c:10260,ch:'B'},{op:'read',slot:0,full:true},{op:'route',slot:1},{op:'read',slot:1,full:false},
 {op:'route',slot:2},{op:'touchMany',c:51400,count:65536},{op:'vision',tick:4},{op:'read',slot:0,full:false},{op:'route',slot:2},
 {op:'touchMany',c:51400,count:1048577},{op:'vision',tick:5},{op:'route',slot:0},{op:'route',slot:2},
 {op:'move',alias:'eye',x:41,z:41},{op:'vision',tick:6},{op:'route',slot:1},{op:'read',slot:0,full:false},
 {op:'read',slot:1,full:true},{op:'read',slot:1,full:false},
]);
// Signed IDs deliberately differ from raw map labels; own and formerly own regions persist out of sight.
record(4,[
 {op:'clearUnits'},{op:'spawn',alias:'eye',owner:0,kind:'rifle',x:41,z:41},{op:'vision',tick:1},
 {op:'regions',slot:0},{op:'region',index:0,fields:{team:3,progress:.351,capper:0,contested:true}},{op:'regions',slot:0},
 {op:'spawn',alias:'enemy',owner:1,kind:'rifle',x:43,z:41},{op:'vision',tick:2},{op:'regions',slot:0},
 {op:'spawn',alias:'base',owner:1,kind:'hq',x:41,z:41},{op:'vision',tick:3},{op:'regions',slot:0},
 {op:'move',alias:'eye',x:901,z:901},{op:'vision',tick:4},{op:'regions',slot:0},
 {op:'region',index:0,fields:{team:1,progress:.921,capper:1,contested:false}},{op:'regions',slot:0},
 {op:'remove',alias:'base'},{op:'regions',slot:0},{op:'reveal',value:true},{op:'regions',slot:0},{op:'read',slot:0,full:false},{op:'route',slot:0},
 {op:'reveal',value:false},{op:'read',slot:0,full:false},{op:'route',slot:-1},
],true);
record(4,[
 {op:'clearUnits'},{op:'spawn',alias:'eye',owner:0,kind:'rifle',x:41,z:41},{op:'vision',tick:1},
 {op:'wreck',id:9001,x:43.14,z:41.26,moving:true},{op:'route',slot:0},{op:'wreckRead',slot:0},
 {op:'move',alias:'eye',x:901,z:901},{op:'wreck',id:9001,x:905.1,z:905.2,moving:true},{op:'wreckRead',slot:0},{op:'route',slot:0},{op:'tick',tick:2},{op:'route',slot:0},
 {op:'removeWreck',id:9001},{op:'wreckRead',slot:0},{op:'reveal',value:true},{op:'wreckRead',slot:0},
 {op:'reveal',value:false},{op:'move',alias:'eye',x:41,z:41},{op:'wreckRead',slot:0},
 {op:'spawn',alias:'post',owner:0,kind:'bunker',x:45,z:41},{op:'fire',c:10261,value:true},{op:'tick',tick:3},{op:'route',slot:0},
 {op:'move',alias:'post',x:49,z:41},{op:'route',slot:0},{op:'move',alias:'post',x:49,z:41,fields:{hp:0}},{op:'remove',alias:'post'},{op:'route',slot:0},
 {op:'wet',value:1},{op:'route',slot:0},{op:'wet',value:0},{op:'route',slot:0},
 {op:'vision',tick:4},{op:'route',slot:0},{op:'wreckRead',slot:1},
],true);
record(4,[
 {op:'clearUnits'},{op:'spawn',alias:'eye',owner:0,kind:'rifle',x:43,z:143},
 {op:'spawn',alias:'base',owner:0,kind:'hq',x:43,z:143},
 {op:'region',index:8,fields:{team:3,progress:.41,capper:0}},
 {op:'vision',tick:1},{op:'regions',slot:0},{op:'route',slot:0},{op:'read',slot:0,full:true},
]);
const out=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases})+'\n';
const target=new URL('../rust/crates/simtest/tests/fixtures/world-memory.json',import.meta.url);
if(process.argv.includes('--check')){if(readFileSync(target,'utf8')!==out)throw Error('World memory fixture differs');}else writeFileSync(target,out);
console.log(`Verified ${cases.length} World memory traces, ${cases.reduce((n,c)=>n+c.actions.length,0)} actions`);
