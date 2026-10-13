// Record original spawn helpers from the pinned prototype with explicit random fractions.
// Run with x64 Node 24.21.0. WW2_REFERENCE selects a read-only reference checkout.
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
if (process.version !== 'v24.21.0' || process.arch !== 'x64') throw Error('Use x64 Node 24.21.0');
const root = fileURLToPath(new URL('../',import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim();
if (git(['rev-parse','HEAD']) !== pin || git(['status','--porcelain','--untracked-files=no'])) throw Error('Reference must be clean at the pin');
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const bits = n => { const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex'); };
const runs = values => { const out=[];for(const v of values) {if(out.length && out.at(-1)[0]===v)out.at(-1)[1]++;else out.push([v,1]);}return out; };
const layouts=[];
function layout(n,teams,dist,fraction=null,label='') {
  let draws=0;const previous=Math.random;Math.random=()=>{draws++;return fraction;};
  let slots;
  try {slots=sim.spawnSlots(n,teams,fraction!==null,dist);} finally {Math.random=previous;}
  if(draws !== (fraction===null ? 0:1)) throw Error('Unexpected reference random draw count');
  layouts.push({label,n,teams,dist,fraction,slots});
}
const terrain=[];
function terrainCase(name,map) {
  const groups=[];
  for(const mode of ['conquest','assault','world','tutorial']) {
    const indices=sim.spawnsFor(map,mode);
    const expected=sim.spawnDistances(map,indices);
    groups.push({mode,indices,distances:expected.flat().map(bits)});
    // Real maps exercise unequal terrain distances and both default and random choices.
    if(mode==='conquest') for(const count of [2,4,6]) {
      const teams=Array.from({length:count},(_,i)=>i%2);
      layout(indices.length,teams,expected,null,`${name}/${count}`);
      layout(indices.length,teams,expected,0.731,`${name}/${count}`);
    }
  }
  terrain.push({name,w:map.w,h:map.h,flags:runs(map.rows.flatMap(r=>[...r].map(c=>sim.TERRAIN[c]??0))),heights:map.heights ? runs(map.heights.flatMap(r=>[...r].map(sim.levelOf))):null,spawns:map.spawns,groups});
}
for(const file of readdirSync(`${reference}/maps`).filter(n=>n.endsWith('.json')).sort()) terrainCase(file,JSON.parse(readFileSync(`${reference}/maps/${file}`,'utf8')));
for(const [name,rows,heights,spawns] of [
  ['cliff',['..........','..........','..........','..........','..........'],Array(5).fill('0000022222'),[{x:1,y:2},{x:8,y:2}]],
  ['ramp',['..........','..........','..........','..........','..........'],Array(5).fill('0000011111'),[{x:1,y:2},{x:8,y:2}]],
  ['ford-detour',['....F....','..W.W.W..','....F....'],null,[{x:0,y:1},{x:8,y:1}]],
  ['ford-endpoints',['F.F'],null,[{x:0,y:0},{x:1,y:0},{x:2,y:0}]],
  ['islands',['...WW...','...WW...','...WW...'],null,[{x:0,y:0},{x:7,y:2},{x:7,y:0}]],
  ['infantry-obstacles',['.YQR#.','BHWOMB','......'],null,[{x:0,y:0},{x:5,y:0},{x:5,y:2}]],
  ['negative-levels',['......','......','......'],['ab0124','ab0124','ab0124'],[{x:0,y:0},{x:5,y:2}]],
  ['duplicate-spawns',['.'],null,[{x:0,y:0},{x:0,y:0,assault:true}]],
]) terrainCase(name,{w:rows[0].length,h:rows.length,rows,heights:heights??undefined,spawns});
for(let n=1;n<=6;n++) for(let count=1;count<=6;count++) {
  const patterns=[Array(count).fill(0),Array.from({length:count},(_,i)=>i),Array.from({length:count},(_,i)=>i%2),Array.from({length:count},(_,i)=>Math.floor(i/2))];
  const seen=new Set();
  for(const teams of patterns) {
    const key=teams.join(',');if(seen.has(key))continue;seen.add(key);
    for(const fraction of [null,0,0.49999999999999994,0.9999999999999999]) layout(n,teams,null,fraction,`ring/${n}/${count}`);
  }
}
// Enumerate every six-player team partition against the ring without random selection.
function partitions(prefix) {
  if(prefix.length===6) {layout(6,prefix,null,null,'team-partition');return;}
  for(let t=0;t<=Math.max(-1,...prefix)+1;t++) partitions([...prefix,t]);
}
partitions([]);
// Near-enemy threshold: 97%, with the source's 1e-9 tolerance on both sides.
for(const d of [97-2e-9,97-1e-9,97,97+1e-9]) {
  const dist=[[0,d,103,100],[d,0,100,100],[103,100,0,100],[100,100,100,0]];
  for(const fraction of [null,0.4,0.9999999999999999]) layout(4,[0,1,2],dist,fraction,`nearest/${d}`);
}
// Directed matrices, negative best scores for allies, and score-threshold neighborhoods.
for(const d of [96.999999998,97,97.000000001,99.999999999,100.000000001]) {
  const dist=[[0,d,2,3],[100,0,20,1],[4,5,0,80],[3,2,1,0]];
  for(const teams of [[0,1],[0,0],[0,1,0,1],[0,1,2,2,0,1]]) for(const fraction of [null,0.5]) layout(4,teams,dist,fraction,`directed/${d}`);
}
const goals=[];
for(let count=1;count<=6;count++) for(const teams of [Array(count).fill(-3),Array.from({length:count},(_,i)=>i),Array.from({length:count},(_,i)=>i%2)]) goals.push({teams,expected:bits(sim.winVp(teams))});
const data=JSON.stringify({reference:pin,terrain,layouts,goals})+'\n';
const target=`${root}/rust/crates/core/tests/fixtures/deployment.json`;
if(process.argv.includes('--check')) {if(readFileSync(target,'utf8')!==data)throw Error('Deployment fixture differs');}else writeFileSync(target,data);
console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} ${terrain.length} terrain cases (${terrain.length-8} authored maps), ${layouts.length} layouts and ${goals.length} VP goals`);
