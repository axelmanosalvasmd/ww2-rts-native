// Run the actual pinned Conquest step on friendly matches with explicit terrain and paths.
// These traces prove the connected phase foundation, not combat or complete gameplay.
import {readFileSync, writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
const reference=resolve(process.env.WW2_REFERENCE??fileURLToPath(new URL('../reference/ww2-rts/',import.meta.url)));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
const clean=()=>{if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Pinned clean reference required');};
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');
clean();
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const buffer=new DataView(new ArrayBuffer(8));
const bits=n=>{buffer.setFloat64(0,n);return buffer.getBigUint64(0).toString(16).padStart(16,'0');};
const snapshot=g=>({tick:g.tick,visionTick:g.visionTick,seed:g.seed,wind:[bits(g.wind.a),bits(g.wind.v)],
  units:[...g.units.values()].map(u=>[u.id,bits(u.x),bits(u.z),bits(u.rot),bits(u.hp),bits(u.supp),bits(u.stuck),bits(u.repath),bits(u.sprint),u.path.map(p=>[bits(p.x),bits(p.z)])]),
  players:g.players.map(p=>[bits(p.mp),bits(p.vp),bits(p.inc)]),
  points:g.points.map(p=>[p.owner,p.capper,bits(p.progress),p.cut,p.contested,p.onPoint,p.capTick??null])});
const cases=[];
for(const army of ['standard','large','massive','endless']) {
 const map={name:'Friendly tick integration',w:20,h:20,rows:Array(20).fill('.'.repeat(20)),spawns:[{x:0,y:0},{x:19,y:19}],points:[{id:'point',x:8,y:8},{id:'locked',x:11,y:8,needs:0}]};
 const old=Math.random; Math.random=()=>.5;let g;
 try{g=sim.createGame(map,['A','B'],false,[0,0],[0,1],{army,weather:false,tech:true,logistics:false});}finally{Math.random=old;}
 const input={seed:0x12345678,wind:[.75,.4],paths:[[1,17,17],[2,23,17]],positions:[[1,13,17],[2,27,17]]};
 g.seed=input.seed;g.wind={a:input.wind[0],v:input.wind[1]};
 for(const [id,x,z]of input.positions)Object.assign(g.units.get(id),{x,z});
 for(const [id,x,z]of input.paths)g.units.get(id).path=[{x,z}];
 const checkpoints=[];
 for(let i=0;i<200;i++){sim.step(g);if([1,2,3,4,5,40,80,160,161,200].includes(g.tick))checkpoints.push(snapshot(g));}
 cases.push({map,army,input,checkpoints});
}
clean();const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases})+'\n';
const path=fileURLToPath(new URL('../rust/crates/simtest/tests/fixtures/match-tick.json',import.meta.url));
if(process.argv.includes('--check')){if(readFileSync(path,'utf8')!==output)throw Error('Match tick fixture differs');}else writeFileSync(path,output);
console.log(`Verified ${cases.length} actual-step traces, 800 ticks, ${cases.reduce((n,c)=>n+c.checkpoints.length,0)} checkpoints`);
