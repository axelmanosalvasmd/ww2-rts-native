// Full ordered World cell rows from original scouting and delivery collectors.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync} from 'node:fs';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
function clean(){assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');}
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');clean();
assert.ok(process.argv.slice(2).every(a=>a==='--check'));
const url=pathToFileURL(resolve(reference,'shared/sim.js')).href;
registerHooks({load(u,c,next){const r=next(u,c);return u===url?{...r,source:String(r.source)+'\nexport {worldTerrainFor,worldTerrainMemory,logCell};\n'}:r;}});
const sim=await import(url);
const {rebuildGrid}=await import(pathToFileURL(resolve(reference,'shared/grid.js')));
const {generateWorldMap}=await import(pathToFileURL(resolve(reference,'shared/world-conquest.js')));
const copy=v=>JSON.parse(JSON.stringify(v));
const teams=[0,0,1],players=teams.map((team,slot)=>({name:`P${slot}`,team,faction:slot}));
const generation={size:'huge',seed:71,players:3,teams,scenery:true};
const map=generateWorldMap(generation);
// Constructor-only pairing with Match seed701's first Weather-stream low32.
const mask=(1n<<64n)-1n,gamma=0x9e3779b97f4a7c15n;
const mix=n=>{n=((n^(n>>30n))*0xbf58476d1ce4e5b9n)&mask;n=((n^(n>>27n))*0x94d049bb133111ebn)&mask;return n^(n>>31n);};
const bootstrap=Number(mix((mix(701n^(2n*gamma&mask))+gamma)&mask)&0xffffffffn);
const settings={seed:701,weatherSeed:1,players,shuffle:false,army:'standard',living:false,tech:false,supply:false,logistics:false};
let draws=0,g;const saved=Math.random;Math.random=()=>{draws++;return bootstrap/2**32;};
try{g=sim.createGame(map,players.map(p=>p.name),false,teams,players.map(p=>p.faction),{mode:'world',army:'standard',weather:false,weatherSeed:1,tech:false,supply:false,logistics:false});}finally{Math.random=saved;}
assert.equal(draws,1);
const actors=[...g.units.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,x:u.x,z:u.z,hp:u.hp}));
const eye=actors.find(u=>u.owner===0&&u.type==='rifle').id;
// Choose declared open scene coordinates using only the generated authoring map.
let c=-1;for(let y=20;y<100&&c<0;y++)for(let x=20;x<100;x++)if(map.rows[y].slice(x,x+3)==='...'&&map.rows[y+1].slice(x,x+3)==='...'){c=y*map.w+x+1;break;}
assert(c>=0);const at={x:(c%g.w+.5)*2,z:(Math.floor(c/g.w)+.5)*2};
const pool=[],indexes=new Map();
function pack(rows){return rows.map(r=>{const k=JSON.stringify(r[4]);let i=indexes.get(k);if(i===undefined){i=pool.length;indexes.set(k,i);pool.push(copy(r[4]));}return [...r.slice(0,4),i];});}
function expand(rows){return rows.map(r=>[...r.slice(0,4),pool[r[4]]]);}
const actions=[
 {op:'vision',tick:1},
 {op:'read',name:'initial-own-history',slot:0,full:false},
 {op:'read',name:'allied-independent-initial-cursor',slot:1,full:false},
 {op:'read',name:'same-cursor-no-duplicates',slot:0,full:false},
 {op:'move',id:eye,x:at.x,z:at.z},
 {op:'vision',tick:2},
 {op:'read',name:'scout-new-ground',slot:0,full:false},
 {op:'read',name:'ally-receives-shared-scout-history',slot:1,full:false},
 {op:'mutate',c,change:{ground:'+',object:'T',material:'soil',height:2}},
 {op:'vision',tick:3},
 {op:'read',name:'visible-layered-change',slot:0,full:false},
 {op:'read',name:'ally-keeps-independent-change-cursor',slot:1,full:false},
 {op:'move',id:eye,x:actors.find(u=>u.id===eye).x,z:actors.find(u=>u.id===eye).z},
 {op:'vision',tick:4},
 {op:'mutate',c,change:{object:'R',material:'stone',height:1,mine:true}},
 {op:'read',name:'hidden-change-not-delivered',slot:0,full:false},
 {op:'read',name:'full-retains-last-seen-layer',slot:1,full:true},
 {op:'move',id:eye,x:at.x,z:at.z},
 {op:'vision',tick:5},
 {op:'read',name:'rescout-accepts-live-layer-hides-mine',slot:0,full:false},
 {op:'mine',c,owner:0,seen:1},
 {op:'log',c,look:true},
 {op:'read',name:'same-key-retains-prior-mine-row',slot:0,full:false},
 {op:'vision',tick:6},
 {op:'read',name:'new-vision-discovers-owned-mine',slot:0,full:false},
 {op:'read',name:'ally-cursor-discovers-same-mine',slot:1,full:false},
 {op:'mutate',c,change:{mine:true,material:'steel'}},
 {op:'read',name:'known-mine-material-change',slot:0,full:false},
 {op:'read',name:'enemy-independent-history',slot:2,full:false},
 {op:'reveal',value:true},
 {op:'read',name:'end-reveal-all-world-rows',slot:0,full:true},
 {op:'read',name:'revealed-no-new-rows-after-full',slot:0,full:false},
];
const receipts=[];
for(const action of actions){let rows=null;const old=Math.random;Math.random=()=>assert.fail('runtime Math.random draw');try{
 switch(action.op){
  case 'vision':g.tick=action.tick;rebuildGrid(g);sim.internals.updateVision(g);break;
  case 'move':Object.assign(g.units.get(action.id),{x:action.x,z:action.z});rebuildGrid(g);break;
  case 'mutate':sim.mutateWorldCell(g,action.c,action.change);break;
  case 'mine':g.mines.set(action.c,action.owner);g.mineSeen.set(action.c,action.seen);break;
  case 'log':sim.logCell(g,action.c,[],action.look);break;
  case 'reveal':g.reveal=action.value;break;
  case 'read':rows=copy(sim.worldTerrainFor(g,action.slot,action.full));break;
  default:assert.fail(action.op);
 }
}finally{Math.random=old;}
 if(rows){const packed=pack(rows);assert.deepEqual(expand(packed),rows);receipts.push({name:action.name,slot:action.slot,full:action.full,rows:packed,rowCount:rows.length,tick:g.tick,memoryVersion:g.world.memory.get(g.players[action.slot].team).version});}
}
const receipt=name=>receipts.find(r=>r.name===name),row=(name)=>expand(receipt(name).rows).find(r=>r[0]===c);
assert.equal(receipt('same-cursor-no-duplicates').rows.length,0);
assert.deepEqual(receipt('initial-own-history').rows,receipt('allied-independent-initial-cursor').rows);
assert.deepEqual(receipt('scout-new-ground').rows,receipt('ally-receives-shared-scout-history').rows);
assert.equal(row('visible-layered-change')[1],'T');
assert.equal(row('hidden-change-not-delivered'),undefined);
assert.equal(row('full-retains-last-seen-layer')[1],'T');
assert.equal(row('rescout-accepts-live-layer-hides-mine')[1],'R');
assert.equal(row('rescout-accepts-live-layer-hides-mine')[4].mine,undefined);
assert.equal(receipt('same-key-retains-prior-mine-row').rows.length,0);
assert.equal(row('new-vision-discovers-owned-mine')[4].mine,true);
assert.equal(row('new-vision-discovers-owned-mine')[4].mineOwned,true);
assert.equal(row('known-mine-material-change')[4].mineMaterial,'steel');
assert.equal(receipt('end-reveal-all-world-rows').rows.length,g.w*g.h);
assert.equal(receipt('revealed-no-new-rows-after-full').rows.length,0);
assert.equal(g.units.size,actors.length);
const stored=copy(map),labels=stored.world.regionMap,runs=[];for(const v of labels){if(runs.length&&runs.at(-1)[0]===v)runs.at(-1)[1]++;else runs.push([v,1]);}stored.world.regionMapRuns=runs;delete stored.world.regionMap;
assert.deepEqual(runs.flatMap(([v,n])=>Array(n).fill(v)),labels);
clean();const output={schema:1,source:{nativeBaseline:'efb677b6b0b2ac043415c47631dcacb0005b925d',pin,node:process.version,arch:process.arch,generation,constructorRandomDraws:draws,bootstrap},scope:'Original World scouting and terrain delivery component, not a World match snapshot. All initial actors remain. Input map region labels and expected row data are losslessly pooled only in this fixture; exported cell rows remain full JSON.',scene:{testOnlyPrivateInputs:true,map:stored,settings,actors,scout:eye,cell:c},actions,dataPool:pool,receipts};
const path=resolve(root,'docs/fixtures/terrain/source-world-scout.json'),content=JSON.stringify(output)+'\n';
if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),content);else writeFileSync(path,content);
console.log(`${actors.length} original actors, ${actions.length} actions, ${receipts.length} full row receipts, ${receipts.reduce((n,r)=>n+r.rows.length,0)} rows, ${pool.length} data objects`);
