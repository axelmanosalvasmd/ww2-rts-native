// Observe actual cached World unit objects before and after ordered deletion.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
const root=fileURLToPath(new URL('../',import.meta.url));const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`),pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');
const path=resolve(reference,'shared/sim.js');let source=readFileSync(path,'utf8');source=source.replace(/from (['"])(\.\/[^'"]+)\1/g,(_,q,p)=>`from ${q}${new URL(p,pathToFileURL(path)).href}${q}`);source+='\nexport {worldPathView,rebuildGrid};\n';const sim=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
const starts=JSON.parse(readFileSync(`${root}/rust/crates/simtest/tests/fixtures/world-start.json`,'utf8'));const c=starts.cases[4],map=structuredClone(c.map);if(map.world.regionMapRuns){map.world.regionMap=map.world.regionMapRuns.flatMap(([v,n])=>Array(n).fill(v));delete map.world.regionMapRuns;}
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n,mix=x=>{x=((x^(x>>30n))*0xbf58476d1ce4e5b9n)&mask;x=((x^(x>>27n))*0x94d049bb133111ebn)&mask;return x^(x>>31n)};
const environmentSeed=Number(mix((mix(BigInt(c.seed)^((2n*gamma)&mask))+gamma)&mask)&0xffffffffn);
const old=Math.random;Math.random=()=>environmentSeed/4294967296;let g;try{g=sim.createGame(map,c.players.map(p=>p.name),c.settings.shuffle,c.players.map(p=>p.team),c.players.map(p=>p.faction),{mode:'world',army:c.settings.army,weather:c.settings.living?c.settings.weather:false,weatherSeed:c.weatherSeed,tech:c.settings.tech,logistics:false})}finally{Math.random=old;}
g.units.clear();sim.rebuildGrid(g);const units=[];
for(const [owner,x]of[[0,55],[0,63],[1,67]]){const u=sim.internals.spawnUnit(g,owner,'bunker');Object.assign(u,{x,z:51});units.push(u);sim.rebuildGrid(g);}
g.players[0].visible=new Set([units[2].id]);g.players[1].visible=new Set(units.slice(0,2).map(u=>u.id));
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex')};
const view=slot=>[...sim.worldPathView(g,slot).units.values()].filter(u=>u.type==='bunker'&&u.hp>0).map(u=>[bits(u.x),bits(u.z)]);
const initial=units.map(u=>({id:u.id,kind:u.type,owner:u.owner,x:u.x,z:u.z,hp:u.hp}));const before=[view(0),view(1)];
const final=[{...initial[0],hp:42,x:59},{...initial[1],hp:0,x:65}];
for(const row of final){const u=g.units.get(row.id);Object.assign(u,{x:row.x,z:row.z,hp:row.hp});g.units.delete(u.id);}
const after=[view(0),view(1)];const out=JSON.stringify({reference:pin,node:process.version,arch:process.arch,startIndex:4,units:initial,final,before,after})+'\n';const destination=`${root}/rust/crates/simtest/tests/fixtures/world-route-retention.json`;
if(process.argv.includes('--check'))assert.equal(readFileSync(destination,'utf8'),out);else writeFileSync(destination,out);
console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} 2 cached recipients, 2 final removed unit objects`);
