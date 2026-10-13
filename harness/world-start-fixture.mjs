// Original World createGame. Explicit maps and deterministic environment draws only.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');
const ref=resolve(process.env.WW2_REFERENCE??fileURLToPath(new URL('../reference/ww2-rts/',import.meta.url)));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',ref,...a],{encoding:'utf8'}).trim();
if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Use clean pinned reference');
const {generateWorldMap}=await import(pathToFileURL(resolve(ref,'shared/world-conquest.js')));
const sim=await import(pathToFileURL(resolve(ref,'shared/sim.js')));
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n;
const mix=x=>{x=((x^(x>>30n))*0xbf58476d1ce4e5b9n)&mask;x=((x^(x>>27n))*0x94d049bb133111ebn)&mask;return x^(x>>31n);};
const environmentSeed=seed=>Number(mix((mix(BigInt(seed)^((2n*gamma)&mask))+gamma)&mask)&0xffffffffn);
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleLE(n);return b.readBigUInt64LE().toString(16).padStart(16,'0');};
const fnv=bytes=>{let h=0xcbf29ce484222325n;for(const b of bytes)h=BigInt.asUintN(64,(h^BigInt(b))*0x100000001b3n);return h.toString(16).padStart(16,'0');};
const hash=typed=>fnv(typed.buffer?Buffer.from(typed.buffer,typed.byteOffset,typed.byteLength):Buffer.from(typed.map(c=>c.charCodeAt(0))));
function runs(values){const out=[];for(const v of values){if(out.length&&out.at(-1)[0]===v)out.at(-1)[1]++;else out.push([v,1]);}return out;}
const cases=[];
function record(map,settings){
 const seed=0x570000+cases.length,players=settings.teams.map((team,slot)=>({name:`Commander ${slot}`,team,faction:slot%4}));
 const draw=environmentSeed(seed)/4294967296,old=Math.random;let count=0,g;
 Math.random=()=>{if(count++)throw Error('Unexpected World startup random draw');return draw;};
 try{g=sim.createGame(map,players.map(p=>p.name),settings.shuffle,players.map(p=>p.team),players.map(p=>p.faction),{mode:'world',army:settings.army,weather:settings.living?settings.weather:false,weatherSeed:300+cases.length,tech:settings.tech,logistics:false});}finally{Math.random=old;}
 if(count!==1)throw Error('Missing environment draw');
 const actor=g.players[-1];const expected={
  players:g.players.map(p=>({spawn:[bits(p.spawn.x),bits(p.spawn.z)],mp:bits(p.mp),inc:bits(p.inc),mun:bits(p.mun),fuel:bits(p.fuel),visible:[...p.visible]})),
  world:g.world.regions.map(r=>[r.team,bits(r.progress),r.capper]),
  nodes:g.nodes.map(n=>[n.c,bits(n.x),bits(n.z),n.depot,bits(n.rate),n.fuel]),
  actor:{faction:actor.faction,spawn:[bits(actor.spawn.x),bits(actor.spawn.z)],visible:[...actor.visible]},
  units:[...g.units.values()],
  ghostTeams:[...g.ghosts].map(([team,rows])=>[team,[...rows.values()]]),
  rails:g.world.stations?{stations:g.world.stations,lines:g.world.lines,railAt:[...g.world.railAt],stationAt:[...g.world.stationAt],yard:[...g.world.railYard]}:null,
  environmentSeed:g.seed>>>0,wind:[bits(g.wind.a),bits(g.wind.v)],weather:g.weather,
  wet:g.wx?bits(g.wx.wet):null,rain:bits(g.wx?.rain??0),
  living:g.wx?{raining:g.wx.raining,next:bits(g.wx.next)}:null,
  terrain:{chars:hash(g.chars),ground:hash(g.ground),objects:hash(g.objects),flags:hash(g.flags),height:g.height&&hash(g.height),hp:hash(g.cellHp),wear:hash(g.wear),state:hash(g.cellState),
   versions:[g.terrainVersion??0,g.obstructionVersion??0,g.infantryRegionVersion??0,g.vehicleRegionVersion??0,g.navalRegionVersion??0],log:g.cellLog,newCells:g.newCells,journal:g.cellJournal,
   buildingCells:[...g.buildingCells].sort((a,b)=>a[0]-b[0]).map(([c,b])=>[c,b.id,b.owner,bits(b.x),bits(b.z)])},
 };
 const input=structuredClone(map);if(input.world.regionMap){input.world.regionMapRuns=runs(input.world.regionMap);delete input.world.regionMap;}
 cases.push({seed,players,settings,weatherSeed:300+cases.length,map:input,expected});
}
for(const [size,scenery,teams,army,weather,living,tech,shuffle] of [
 ['huge',false,[0], 'standard','map',false,false,true],
 ['huge',true,[0,0,2], 'large','rain',true,true,true],
 ['massive',false,[0,1,2,3,4,5], 'massive','mud',true,false,false],
 ['massive',true,[0,0,1,1,2,2], 'endless','fog',true,true,false],
]){const map=generateWorldMap({size,seed:42,players:teams.length,teams,scenery});record(map,{size,scenery,teams,army,weather,living,tech,shuffle});}
// A source-valid open province fixture puts enemy buildings and armed guards in vision,
// repeats rail keys, keeps a nonadjacent hop, and gives actual teams different home hints.
const map=generateWorldMap({size:'huge',seed:1,players:2,teams:[0,1],scenery:false});
map.rows=Array(map.h).fill('.'.repeat(map.w));map.heights=Array(map.h).fill('0'.repeat(map.w));
map.world.regions.forEach((r,i)=>{r.id=i*2-50;r.x=20+(i%8)*50;r.y=20+Math.floor(i/8)*50;r.kind=['rural','city','industrial','resource','custom'][i%5];r.team=5;});
const homes=map.world.regions.filter(r=>r.home!==undefined);homes.forEach(r=>{map.spawns[r.home]={x:r.x,y:r.y};});
const near=map.world.regions.find(r=>r.home===undefined);near.x=homes[0].x+12;near.y=homes[0].y+2;
map.world.rails={stations:[{region:0,x:0,y:0},{region:1,x:0,y:0},{region:2,x:2,y:0}],lines:[{a:0,b:1,cells:[0,1,2,1,513,1027],regions:[0,1,1]},{a:1,b:2,cells:[2,1,0],regions:[1,2]}]};
record(map,{size:'huge',scenery:false,teams:[3,1],army:'standard',weather:'clear',living:true,tech:true,shuffle:true,synthetic:true});
const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases})+'\n';
const path=fileURLToPath(new URL('../rust/crates/simtest/tests/fixtures/world-start.json',import.meta.url));
if(process.argv.includes('--check')){if(readFileSync(path,'utf8')!==output)throw Error('World startup fixture differs');}else writeFileSync(path,output);
console.log(`Verified ${cases.length} World startups, ${cases.reduce((n,c)=>n+c.expected.units.length,0)} units`);
