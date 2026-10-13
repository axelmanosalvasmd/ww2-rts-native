// Execute the original late point/income block on an actual createGame instance.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const reference=resolve(process.env.WW2_REFERENCE??fileURLToPath(new URL('../reference/ww2-rts/',import.meta.url)));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
const clean=()=>{if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Pinned clean reference required');};
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');
clean();
registerHooks({load(url,context,next){const result=next(url,context);if(url!==pathToFileURL(resolve(reference,'shared/sim.js')).href)return result;
 let source=String(result.source),begin='  // teammates standing on it keep it theirs.',end='  stepConvoys(g, dt, supplyHooks(g));';
 if(source.split(begin).length!==2||source.split(end).length!==2)throw Error('Phase anchors changed');
 const block=source.slice(source.indexOf(begin),source.indexOf(end));
 source+='\nexport function __pointEconomy(g,list,dt){if(g.supply && g.tick % CFG.supply.every === 1)supplyLines(g,list);\n'+block+'\n}\nexport const __setCell=setCell;\n';
 return {...result,source};
}});
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const b=new DataView(new ArrayBuffer(8));const bits=n=>{b.setFloat64(0,n);return b.getBigUint64(0).toString(16).padStart(16,'0');};
const snapshot=g=>({players:g.players.map(p=>[bits(p.mp),bits(p.vp),bits(p.inc),!!p.away,!!p.out]),points:g.points.map(p=>[p.owner,p.capper,bits(p.progress),p.cut,p.contested,p.onPoint,p.capTick??null])});
const cases=[];
for(const army of ['standard','large','massive','endless'])for(const allied of [false,true]){
 const map={name:'Point integration',w:20,h:20,rows:Array(20).fill('.'.repeat(20)),spawns:[{x:0,y:0},{x:19,y:19}],points:[{id:'first',x:8,y:8},{id:'locked',x:11,y:8,needs:0},{id:'depot',x:8,y:11,mp:2,vp:.5,kind:'depot'}]};
 const old=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(map,['A','B'],false,allied?[0,0]:[0,1],[0,1],{army,weather:false,tech:true,logistics:false});}finally{Math.random=old;}
 const script=[
 {tick:1,moves:[[1,17,17],[2,23,17],[3,17,23]],points:[[0,-1,0,.995],[1,-1,0,.995],[2,1,1,.001]]},
 {tick:2,moves:[[4,17,17]]},
 {tick:3,away:[[0,true]],out:[[1,true]]},
 {tick:4,away:[[0,false]],out:[[1,false]],moves:[[4,37,37]]},
 {tick:41,cells:Array.from({length:20},(_,x)=>[10*20+x,'O'])},
 {tick:42,spawn:{owner:0,kind:'rifle',x:17,z:23},beforeSpawn:true},
 {tick:81,cells:Array.from({length:20},(_,x)=>[10*20+x,'.'])},
 {tick:82,points:[[0,-1,0,.995]],winner:true},
 {tick:83,winner:false},
 ];
 const steps=[];
 for(const action of script){g.tick=action.tick;
  for(const [id,x,z]of action.moves??[])Object.assign(g.units.get(id),{x,z});
  for(const [i,owner,capper,progress]of action.points??[])Object.assign(g.points[i],{owner,capper,progress});
  for(const [i,value]of action.away??[])g.players[i].away=value;
  for(const [i,value]of action.out??[])g.players[i].out=value;
  for(const [cell,ch]of action.cells??[])sim.__setCell(g,cell,ch);
  let list=[...g.units.values()];
  if(action.spawn){const s=action.spawn,u=sim.internals.spawnUnit(g,s.owner,s.kind);Object.assign(u,{x:s.x,z:s.z});if(!action.beforeSpawn)list=[...g.units.values()];}
  g.winner=action.winner?0:null;
  sim.__pointEconomy(g,list,.05);steps.push({action,expected:snapshot(g)});
 }
 cases.push({map,army,allied,steps});
}
clean();const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases})+'\n';
const path=fileURLToPath(new URL('../rust/crates/simtest/tests/fixtures/match-economy.json',import.meta.url));
if(process.argv.includes('--check')){if(readFileSync(path,'utf8')!==output)throw Error('Match economy fixture differs');}else writeFileSync(path,output);
console.log(`Verified ${cases.length} assembled-match traces, ${cases.reduce((n,c)=>n+c.steps.length,0)} phases`);
