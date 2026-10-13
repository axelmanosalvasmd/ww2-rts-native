// Original single-commander World readers, without terrain delivery or complete envelopes.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
const root=fileURLToPath(new URL('../',import.meta.url)),ref=resolve(process.env.WW2_REFERENCE??resolve(root,'reference/ww2-rts'));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d',git=a=>execFileSync('git',['-C',ref,...a],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'')};clean();
const path=resolve(ref,'shared/sim.js');let code=readFileSync(path,'utf8');
const found=code.slice(code.indexOf('export function snapshotFor(')).match(/foundMines: (\[\.\.\.g\.mineSeen\].*?),\n/)[1];
code=code.replace(/from (['"])(\.\/[^'"]+)\1/g,(_,q,p)=>`from ${q}${new URL(p,pathToFileURL(path)).href}${q}`);
code+=`\nexport {rebuildGrid,logCell,touch,worldTerrainMemory};\nexport function oracleWorldMetadata(g,slot,map){const p=g.players[slot],h=worldHome(g,p.team,p.spawn);return {...(map?{map:worldMapFor(g,slot)}:{}),world:worldSnapshot(g,slot),home:[rounded(h.x),rounded(h.z)],nodes:worldNodesFor(g,slot),wrecks:worldWrecksFor(g,slot),foundMines:${found}};}\n`;
const sim=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
const starts=JSON.parse(readFileSync(resolve(root,'rust/crates/simtest/tests/fixtures/world-start.json'),'utf8'));
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n,mix=x=>{x=((x^(x>>30n))*0xbf58476d1ce4e5b9n)&mask;x=((x^(x>>27n))*0x94d049bb133111ebn)&mask;return x^(x>>31n);};
const environmentSeed=seed=>Number(mix((mix(BigInt(seed)^((2n*gamma)&mask))+gamma)&mask)&0xffffffffn);
// Canonical value hashing bounds the fixture while covering every map cell and ordered material row.
function mapDigest(value){let h=0xcbf29ce484222325n;const feed=bytes=>{for(const b of bytes)h=BigInt.asUintN(64,(h^BigInt(b))*0x100000001b3n)};const visit=v=>{if(v===null)feed(Buffer.from('n'));else if(typeof v==='boolean')feed(Buffer.from(v?'t':'f'));else if(typeof v==='number'){feed(Buffer.from('d'));const b=Buffer.alloc(8);b.writeDoubleLE(v===0?0:v);feed(b)}else if(typeof v==='string'){feed(Buffer.from('s'));feed(Buffer.from(v));feed([0])}else if(Array.isArray(v)){feed(Buffer.from('a'));v.forEach(visit);feed(Buffer.from(']'))}else{feed(Buffer.from('o'));Object.keys(v).sort().forEach(k=>{visit(k);visit(v[k])});feed(Buffer.from('}'))}};visit(value);return h.toString(16).padStart(16,'0')}
const cases=[];
function record(name,startIndex,actions,teams){
 const c=structuredClone(starts.cases[startIndex]),map=structuredClone(c.map);
 if(teams)c.players.forEach((p,i)=>p.team=teams[i]);
 if(map.world.regionMapRuns){map.world.regionMap=map.world.regionMapRuns.flatMap(([v,n])=>Array(n).fill(v));delete map.world.regionMapRuns;}
 const old=Math.random;Math.random=()=>environmentSeed(c.seed)/4294967296;let g;
 try{g=sim.createGame(map,c.players.map(p=>p.name),c.settings.shuffle,c.players.map(p=>p.team),c.players.map(p=>p.faction),{mode:'world',army:c.settings.army,weather:c.settings.living?c.settings.weather:false,weatherSeed:c.weatherSeed,tech:c.settings.tech,logistics:false})}finally{Math.random=old}
 const aliases=new Map(),rows=[];
 const pos=a=>a.region===undefined?{x:a.x,z:a.z}:g.world.regions[a.region];
 for(const a of actions){let result=null;
  switch(a.op){
   case 'clear':g.units.clear();sim.rebuildGrid(g);break;
   case 'spawn':{const u=sim.internals.spawnUnit(g,a.owner??0,a.kind??'rifle');a.id=u.id;aliases.set(a.alias,u.id);const at=pos(a);Object.assign(u,{x:at.x,z:at.z,...a.fields});a.x=u.x;a.z=u.z;sim.rebuildGrid(g);break}
   case 'move':{const u=g.units.get(aliases.get(a.alias)),at=pos(a);Object.assign(u,{x:at.x,z:at.z,...a.fields});a.x=u.x;a.z=u.z;sim.rebuildGrid(g);break}
   case 'remove':g.units.delete(aliases.get(a.alias));sim.rebuildGrid(g);break;
   case 'vision':g.tick=a.tick;sim.internals.updateVision(g);break;
   case 'reveal':g.reveal=a.value;break;
   case 'region':Object.assign(g.world.regions[a.index],a.fields);break;
   case 'player':Object.assign(g.players[a.slot],a.fields);break;
   case 'homeTie':{const x=g.world.regions[a.a],z=g.world.regions[a.b];a.spawn={x:(x.x+z.x)/2,z:(x.z+z.z)/2};g.players[a.slot].spawn=a.spawn;break;}
   case 'mutate':sim.mutateWorldCell(g,a.cell,a.change);break;
   case 'seen':g.mineSeen.set(a.cell,a.bits);break;
   case 'forget':g.mineSeen.delete(a.cell);break;
   case 'log':sim.logCell(g,a.cell,[],true);break;
   case 'nodes':g.nodes=structuredClone(a.rows);break;
   case 'stations':g.world.stations=structuredClone(a.rows);break;
   case 'wreck':{const w={id:a.id,type:a.kind??'tank',owner:a.owner??1,rot:a.rot??-.26,x:a.x,z:a.z};const prior=g.wrecks.find(w=>w.id===a.id);if(prior)Object.assign(prior,w);else g.wrecks.push(w);break}
   case 'removeWreck':g.wrecks=g.wrecks.filter(w=>w.id!==a.id);break;
   case 'capture':result=sim.oracleWorldMetadata(g,a.slot,!!a.map);if(a.map)result.map=mapDigest(result.map);break;
   default:throw Error(a.op)
  }
  rows.push({action:a,result:structuredClone(result)});
 }
 cases.push({name,startIndex,teams:teams??null,rows});
}
const capture=(slot,map=false)=>({op:'capture',slot,map}),vision=tick=>({op:'vision',tick});
record('massive-world-endless-army',3,[capture(0,true)]);
record('real-startup-shared-team-map',1,[capture(0,true),capture(1,true),capture(2,true),capture(0),vision(1),capture(1)]);
record('known-geography-lists-and-hidden-replacements',1,[
 {op:'clear'},{op:'spawn',alias:'eye',x:41,z:41},vision(1),
 {op:'mutate',cell:10260,change:{ground:'M',height:-2,object:'K',material:'stone',groundMaterial:'soil'}},
 {op:'mutate',cell:10261,change:{height:4,mine:true}}, {op:'seen',cell:10261,bits:1},{op:'log',cell:10261},
 {op:'nodes',rows:[{c:10260,x:41.14,z:41.26,rate:7.125,fuel:false},{c:200000,x:641,z:781,rate:3.5,fuel:true}]},
 {op:'stations',rows:[{c:10260,region:0,x:41.14,z:41.26},{c:200000,region:2,x:641,z:781}]},
 {op:'wreck',id:9002,x:43.14,z:41.26},capture(0,true),capture(1),capture(2),
 {op:'move',alias:'eye',x:901,z:901},vision(2),
 {op:'mutate',cell:10260,change:{ground:'W',height:1,object:'.',material:'wood'}},
 {op:'removeWreck',id:9002},capture(0,true),capture(1),
 {op:'reveal',value:true},capture(0,true),{op:'reveal',value:false},capture(0),
 {op:'move',alias:'eye',x:41,z:41},vision(3),capture(0,true),
 {op:'seen',cell:10262,bits:1},{op:'mutate',cell:10262,change:{mine:true}},
 {op:'forget',cell:10261},{op:'seen',cell:10261,bits:1},capture(0),
]);
record('region-locks-remembered-progress-and-recovery',4,[
 {op:'clear'},{op:'spawn',alias:'eye',region:0},{op:'spawn',alias:'hq',kind:'hq',region:0,fields:{built:.2,hp:0}},
 {op:'spawn',alias:'engineer',kind:'engineer',region:0,fields:{hp:0}},vision(1),capture(0,true),
 {op:'region',index:0,fields:{team:1,progress:.351,capper:1,contested:true}},capture(0),
 {op:'move',alias:'hq',region:0,fields:{hp:5}},{op:'move',alias:'engineer',region:0,fields:{hp:5}},{op:'player',slot:0,fields:{away:true}},capture(0),
 {op:'spawn',alias:'enemy',owner:1,region:0},vision(2),capture(0),
 {op:'move',alias:'eye',region:63},{op:'move',alias:'hq',region:63},{op:'move',alias:'engineer',region:63},vision(3),capture(0),
 {op:'region',index:0,fields:{team:3,progress:.921,capper:0,contested:false}},capture(0),
 {op:'remove',alias:'hq'},{op:'remove',alias:'engineer'},capture(0),
 {op:'player',slot:0,fields:{out:true}},capture(0),{op:'reveal',value:true},capture(0,true),
]);
record('allied-seat-cap-home-ties-and-fallback',1,[
 {op:'clear'},{op:'player',slot:1,fields:{out:true}},
 ...Array.from({length:8},(_,index)=>({op:'region',index,fields:{team:0}})),
 {op:'homeTie',slot:0,a:0,b:1},capture(0),capture(1),{op:'player',slot:0,fields:{out:true}},capture(0),
 ...Array.from({length:64},(_,index)=>({op:'region',index,fields:{team:-1}})),capture(0,true),
], [0,0,0]);
clean();const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases})+'\n';
const out=resolve(root,'rust/crates/core/tests/fixtures/world-recipient-metadata.json');
if(process.argv.includes('--check'))assert.equal(readFileSync(out,'utf8'),output);else writeFileSync(out,output);
console.log(`${process.argv.includes('--check')?'Verified':'Recorded'} ${cases.length} World metadata traces, ${cases.reduce((n,c)=>n+c.rows.length,0)} actions, ${cases.reduce((n,c)=>n+c.rows.filter(r=>r.result!==null).length,0)} captures`);
