// Observe unchanged createGame Assault startup under the native startup random streams.
import assert from 'node:assert/strict';
import {readFileSync,readdirSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');
const clean=()=>{if(execFileSync('git',['rev-parse','HEAD'],{cwd:reference,encoding:'utf8'}).trim()!==pin||execFileSync('git',['status','--porcelain','--untracked-files=no'],{cwd:reference,encoding:'utf8'}).trim())throw Error('Pinned clean reference required');};clean();
registerHooks({load(url,ctx,next){const out=next(url,ctx);if(url!==pathToFileURL(`${reference}/shared/sim.js`).href)return out;let source=String(out.source);const anchor="  if (opts.mode === 'classic') setupClassic(g);";if(source.split(anchor).length!==2)throw Error('Startup observation anchor changed');source=source.replace(anchor,'  g.cellJournal = []; for (const p of g.players) p.terrainPending = new Set();\n'+anchor);return{...out,source};}});
const sim=await import(pathToFileURL(`${reference}/shared/sim.js`));
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n;
function mix(x){x=((x^(x>>30n))*0xbf58476d1ce4e5b9n)&mask;x=((x^(x>>27n))*0x94d049bb133111ebn)&mask;return x^(x>>31n);}
function stream(seed,purpose){let state=mix((seed^((BigInt(purpose)*gamma)&mask))&mask);return()=>mix(state=(state+gamma)&mask);}
const fraction=n=>Number(n>>11n)/9007199254740992;
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
function runs(a){const out=[];for(const v of a){if(out.length&&out.at(-1)[0]===v)out.at(-1)[1]++;else out.push([v,1]);}return out;}
const floatRuns=a=>runs(new Uint32Array(a.buffer,a.byteOffset,a.length));
const maps=[];for(const file of readdirSync(`${reference}/maps`).filter(v=>v.endsWith('.json')).sort()){const map=JSON.parse(readFileSync(`${reference}/maps/${file}`,'utf8'));if(map.world||map.scenario)continue;maps.push({name:file,map});}
const cases=[];
function record(index,teams,defender,army='standard',shuffle=false,weather='map',living=false,tech=false,roster=null,supply=false){
 const invalid=sim.validateMap(maps[index].map);if(invalid!==null)throw Error(`Invalid ${maps[index].name}: ${invalid}`);
 const seed=BigInt(0x571800+cases.length),terrain=stream(seed,1),wx=stream(seed,2);
 const draws=[...(shuffle?[fraction(terrain())]:[]),Number(wx()&0xffffffffn)/4294967296];
 const players=teams.map((team,i)=>({name:roster?.[i]?.name??`P${i}`,team,faction:roster?.[i]?.faction??i%4}));let used=0,g;const old=Math.random;
 Math.random=()=>{if(used===draws.length)throw Error('Unexpected startup randomness');return draws[used++];};
 try{g=sim.createGame(maps[index].map,players.map(p=>p.name),shuffle,teams,players.map(p=>p.faction),{mode:'assault',defenderTeam:defender,army,weather:living?weather:false,weatherSeed:31,tech,logistics:false,supply});}finally{Math.random=old;}
 if(used!==draws.length)throw Error('Startup random draw count differs');
 cases.push({map:index,seed:String(seed),players,defender,army,shuffle,weather,living,tech,expected:{
  mode:g.mode,players:g.players.map(p=>({team:p.team,spawn:[bits(p.spawn.x),bits(p.spawn.z)],mp:bits(p.mp),inc:bits(p.inc),vp:bits(p.vp)})),
  points:g.points.map(p=>({id:p.scenarioId,at:[bits(p.x),bits(p.z)],vp:bits(p.vp),mp:bits(p.mp),kind:p.kind??null,needs:p.needs})),
  winVp:bits(g.winVp),units:[...g.units.values()],nextId:g.nextId,scenario:g.scenario??null,
  controls:[...g.units.values()].map(u=>[u.id,bits(u.aim),u.auto,u.autoRetreat,u.attackId,u.orders]),
  production:[...g.units.values()].filter(u=>u.queue).map(u=>[u.id,u.queue,u.productionJobs,bits(u.prog),u.rally]),
  tech:g.players.map(p=>p.tier===undefined?null:[p.tier,p.armory,p.lab]),
  terrain:{chars:runs(g.chars),ground:runs(g.ground),objects:runs(g.objects),flags:runs(g.flags),height:g.height&&runs(g.height),hp:floatRuns(g.cellHp),wear:floatRuns(g.wear),surfaceWear:floatRuns(g.surfaceWear),state:runs(g.cellState),
   versions:[g.terrainVersion??0,g.obstructionVersion??0,g.infantryRegionVersion??0,g.vehicleRegionVersion??0,g.navalRegionVersion??0],log:g.cellLog,newCells:g.newCells,journal:g.cellJournal,
   pending:g.players.map(p=>[...p.terrainPending].sort((a,b)=>a-b)),buildingCells:[...g.buildingCells??[]].sort((a,b)=>a[0]-b[0]).map(([c,b])=>[c,b.id,b.owner,bits(b.x),bits(b.z)]),
   buildings:[...g.structures.values()].filter(s=>s.kind==='building').map(s=>[s.id,s.entityId,s.sections.map(t=>t.c),s.sections.map(t=>[t.id,t.structureId,t.c,bits(t.hp),bits(t.maxHp),t.material,t.anchor,t.supports,t.state,t.rubble])])},
  environmentSeed:g.seed>>>0,wind:[bits(g.wind.a),bits(g.wind.v)],rain:bits(g.wx?.rain??0),wet:g.wx?bits(g.wx.wet):null,weather:g.weather,
 }});
 return {game:g,recorded:cases.at(-1)};
}
for(let i=0;i<maps.length;i++)record(i,[0,1],1);
for(const [teams,defender,army,shuffle,weather]of [[[2,5,3,2],5,'large',true,'rain'],[[0,0,0],0,'massive',false,'mud'],[[2,3,5],1,'endless',true,'snow'],[[0,1,2,3,4,5],1,'standard',false,'fog']])record(0,teams,defender,army,shuffle,weather,true,true);
for(const kind of ['open','edge','river','steep','tight','wrap']){
 const w=50,h=40,map={name:kind,w,h,rows:Array(h).fill('.'.repeat(w)),spawns:[{x:2,y:2},{x:47,y:37},{x:2,y:37,assault:true}],points:[{id:'vp',x:25,y:10,mp:0},{id:'resource',x:25,y:20,mp:2,needs:0},{id:'linked',x:25,y:30,needs:1}],defend:[2],assaultTime:300};
 if(kind==='edge')map.spawns[2]={x:0,y:39,assault:true};
 if(kind==='river'){for(let y=0;y<h;y++)map.rows[y]='.'.repeat(23)+(y===20?'=':'W').repeat(4)+'.'.repeat(23);for(const p of map.points)p.x=20;}
 if(kind==='steep')map.heights=Array(h).fill('01234012340123401234012340123401234012340123401234');
 if(kind==='tight'){map.rows=Array(h).fill('W'.repeat(w));map.rows[37]='.'.repeat(w);map.spawns[0].y=37;map.points.forEach((p,i)=>{p.x=20+i*5;p.y=37;});}
 if(kind==='wrap')map.defend=[1,2];
 maps.push({name:kind,map});record(maps.length-1,kind==='wrap'?[1,1,1,0,0,0]:[0,1],1);
}
// Append named map predicates after the original 43 cases so their seeds and observations stay exact.
const originalCases=cases.length;
assert.equal(originalCases,43);
const authoredIndex=name=>{const index=maps.findIndex(m=>m.name===`${name}.json`);assert.ok(index>=0,`Missing authored map ${name}`);return index;};
const xlNames=['pegasus-bridge-xl','hill-112-xl','seawall-xl','monte-cassino-xl'];
for(const name of xlNames){
 const index=authoredIndex(name),map=maps[index].map;
 assert.equal(sim.validateMap(map),null,name+' is valid');
 assert.equal(map.spawns.length,6,name+' has 6 spawns');
 assert.deepEqual(map.defend,[0,1,2]);
 const roster=['a','b','c','d','e','f'].map((name,i)=>({name,faction:i%3}));
 const {game:g,recorded}=record(index,[0,0,0,1,1,1],0,'standard',false,'map',true,false,roster,true);
 recorded.supply=true;
 assert.equal(g.supply,true,'the named Source check keeps default point supply');
 assert.equal(g.mode.timeLeft,map.assaultTime??sim.CFG.assault.time,name+' uses its own clock');
 const bunkers=[...g.units.values()].filter(u=>u.type==='bunker').map(u=>u.id);
 assert.equal(bunkers.length,3,name+': a bunker per defender');
 recorded.selectedCheck={kind:'xl',map:name,clock:bits(g.mode.timeLeft),bunkers};
}
const hillIndex=authoredIndex('hill-112'),hill=maps[hillIndex].map;
assert.equal(sim.validateMap(hill),null);
for(let shuffleCase=0;shuffleCase<10;shuffleCase++){
 const {game:g,recorded}=record(hillIndex,[0,0,1],1,'standard',true,'map',true,false,[{name:'a',faction:0},{name:'b',faction:1},{name:'d',faction:2}],true);
 recorded.supply=true;
 assert.equal(g.supply,true,'the named Source check keeps default point supply');
 // The route probe may annotate its actual actor. Keep the constructor observation from before the query.
 recorded.expected=structuredClone(recorded.expected);
 const levels=g.players.map(p=>g.height[Math.floor(p.spawn.z/sim.CELL)*g.w+Math.floor(p.spawn.x/sim.CELL)]);
 assert.deepEqual(levels,[0,0,4],'attackers below, defender on top');
 const actor=[...g.units.values()].find(u=>u.owner===0&&u.type==='rifle');
 assert.ok(actor,'the actual owner-zero starting rifle exists');
 const from=[bits(actor.x),bits(actor.z)],target=g.players[2].spawn;
 const path=sim.findPath(g,actor,target);
 assert.ok(path.length,'a way up from the HQ doorstep');
 recorded.selectedCheck={kind:'hill',map:'hill-112',shuffleCase,levels,route:{actor:actor.id,from,target:[bits(target.x),bits(target.z)],path:path.map(p=>[bits(p.x),bits(p.z)])}};
}
// Retain the selected Source errors without claiming native text equality or changing the validator.
const invalidDefend={...hill,defend:[0,1,2,3]};
const invalidTime={...maps[authoredIndex('default')].map,assaultTime:5};
const selectedValidation=[
 {name:'hill-defend-all',map:invalidDefend,error:sim.validateMap(invalidDefend)},
 {name:'assault-time-below-minimum',map:invalidTime,error:sim.validateMap(invalidTime)},
];
assert.equal(selectedValidation[0].error,'defend must list some (not all) spawn numbers');
assert.equal(selectedValidation[1].error,'assaultTime must be 300-3600 seconds');
assert.equal(cases.length,57);
// Repeated terrain rows and complete arrays are interned. No observed value is dropped.
const raw={reference:pin,node:process.version,arch:process.arch,maps,cases,selectedValidation},counts=new Map();
function scan(v){if(v&&typeof v==='object'){const key=JSON.stringify(v);if(key.length>120)counts.set(key,(counts.get(key)??0)+1);for(const child of Object.values(v))scan(child);}else if(typeof v==='string'&&v.length>120){const key=JSON.stringify(v);counts.set(key,(counts.get(key)??0)+1);}}scan(raw);
const table=[],ids=new Map();function pack(v,skip=false){const object=v&&typeof v==='object';if(!object&&!(typeof v==='string'&&v.length>120))return v;const key=JSON.stringify(v);if(!skip&&(counts.get(key)??0)>1){if(!ids.has(key)){const id=table.length;ids.set(key,id);table.push(null);table[id]=pack(v,true);}return{$fixtureRef:ids.get(key)};}if(!object)return v;return Array.isArray(v)?v.map(child=>pack(child)):Object.fromEntries(Object.entries(v).map(([k,child])=>[k,pack(child)]));}
const data=pack(raw);function unpack(v){if(v&&typeof v==='object'){if(!Array.isArray(v)&&Object.keys(v).length===1&&'$fixtureRef'in v)return unpack(table[v.$fixtureRef]);return Array.isArray(v)?v.map(unpack):Object.fromEntries(Object.entries(v).map(([k,child])=>[k,unpack(child)]));}return v;}
if(JSON.stringify(unpack(data))!==JSON.stringify(raw))throw Error('Compacted fixture does not reconstruct original observations');
clean();const output=JSON.stringify({format:2,table,data})+'\n';const file=`${root}/rust/crates/simtest/tests/fixtures/assault-start.json`;
if(process.argv.includes('--check')){if(readFileSync(file,'utf8')!==output)throw Error('Assault startup fixture differs');}else writeFileSync(file,output);
console.log(`Verified ${cases.length} Assault startups, ${cases.reduce((n,c)=>n+c.expected.units.length,0)} units`);
