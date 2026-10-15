// Unmodified Source creation, command and full steps, with explicit initial drill facts.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
assert.ok(process.argv.slice(2).every(a=>a==='--check'));
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??resolve(root,'reference/ww2-rts'));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};
clean();
const sourceFiles=['shared/sim.js','shared/ai-schedule.js'];
const sourceBytes=sourceFiles.map(f=>readFileSync(resolve(reference,f)));
const sourceHashes=Object.fromEntries(sourceFiles.map((f,i)=>[f,createHash('sha256').update(sourceBytes[i]).digest('hex')]));
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const {playMatch}=await import(pathToFileURL(resolve(reference,'shared/ai-schedule.js')));
const {rebuildGrid}=await import(pathToFileURL(resolve(reference,'shared/grid.js')));
const data=new DataView(new ArrayBuffer(8));
const bits=n=>{data.setFloat64(0,n,false);return data.getBigUint64(0,false).toString(16).padStart(16,'0');};
const real=n=>({value:n,bits:bits(n)});
const clone=n=>structuredClone(n);
const seed=0x516e9200,c=25*64+25;
const cases=[];
for(const scene of [
 {name:'dry-open-subthreshold',ch:'.',wear:0,weather:false,height:0},
 {name:'dry-open-packed-quarter',ch:'.',wear:.2495,weather:false,height:0},
 {name:'dry-open-mud-transition',ch:'.',wear:.9995,weather:false,height:0},
 {name:'dry-mud-depth',ch:'M',wear:.5,weather:false,height:0},
 {name:'wet-open-below-sea',ch:'.',wear:0,weather:'mud',height:-1},
 {name:'wet-mud-depth',ch:'M',wear:.5,weather:'mud',height:0},
]){
 const map={name:'Ground contact drill',w:64,h:64,rows:Array(64).fill('.'.repeat(64)),spawns:[{x:4,y:4},{x:59,y:59}],points:[{x:32,y:32}]};
 map.rows[25]=map.rows[25].slice(0,25)+scene.ch+map.rows[25].slice(26);
 if(scene.height){map.heights=Array(64).fill('0'.repeat(64));map.heights[25]=map.heights[25].slice(0,25)+'a'+map.heights[25].slice(26);}
 assert.equal(sim.validateMap(map),null);
 const options={mode:'conquest',weather:scene.weather,tech:false,supply:false,logistics:false};
 const tape=[],stages=[];let actor,command,commandReturn,creationSeed,initial,postCommand,journalAt=0,newAt=0;
 const observe=g=>{
  const cells=new Set([c,...g.cellJournal]);
  const result={tick:g.tick,actorOrder:[...g.units.keys()],seedSigned:g.seed,seedUnsigned:g.seed>>>0,randomDraws:tape.length,
   unit:{id:actor.id,type:actor.type,owner:actor.owner,x:real(actor.x),z:real(actor.z),rot:real(actor.rot),vx:real(actor.vx??0),vz:real(actor.vz??0),moveSpeed:real(actor.moveSpeed??0),path:actor.path.map(p=>({x:real(p.x),z:real(p.z)})),dust:actor.dust??null,still:real(actor.still),stuck:real(actor.stuck),hp:real(actor.hp)},
   cells:[...cells].sort((a,b)=>a-b).map(i=>({c:i,ch:g.chars[i],ground:g.ground[i],object:g.objects[i],mine:g.mineLayer[i],flags:g.flags[i],height:g.height?.[i]??0,wear:real(g.wear[i]),wearF32Bits:new Uint32Array(g.wear.buffer)[i],surfaceWear:real(g.surfaceWear[i]),surfaceWearF32Bits:new Uint32Array(g.surfaceWear.buffer)[i],state:g.cellState[i],hp:real(g.cellHp[i]),scar:real(g.scar[i]),burnt:g.burnt[i]})),
   journalOrdinal:{from:journalAt,through:g.cellJournal.length},newCellsOrdinal:{from:newAt,through:g.newCells.length},journal:clone(g.cellJournal),journalDelta:g.cellJournal.slice(journalAt),newCells:clone(g.newCells),newCellsDelta:clone(g.newCells.slice(newAt)),cellLog:clone(g.cellLog),
   versions:Object.fromEntries(['terrainVersion','obstructionVersion','infantryRegionVersion','vehicleRegionVersion','navalRegionVersion'].map(k=>[k,g[k]??0])),wx:clone(g.wx),weather:clone(g.weather),shots:clone(g.shots),mines:[...g.mines]};
  journalAt=g.cellJournal.length;newAt=g.newCells.length;return result;
 };
 const record=(phase,fn)=>{const original=Math.random;Math.random=()=>{const value=original();tape.push({phase,value:real(value)});return value;};try{return fn();}finally{Math.random=original;}};
 const source={...sim,createGame:(...args)=>record('creation',()=>sim.createGame(...args)),step:g=>record('step',()=>sim.step(g))};
 const g=playMatch({map,names:['A','B'],teams:[0,1],factions:[0,1],shuffle:false,options,seats:[],seed,maxTicks:2,sim:source,
  setup:g=>{creationSeed=g.seed;actor=sim.internals.spawnUnit(g,0,'tank');Object.assign(actor,{x:51,z:51,rot:0,ghost:0});
   for(const u of g.units.values())Object.assign(u,{holdFire:true,holdPos:true,auto:false,autoRetreat:false});
   g.wear[c]=scene.wear;g.cellState[c]=Math.min(3,Math.floor(g.wear[c]*4));g.cellJournal=[];
   rebuildGrid(g);sim.internals.updateVision(g);initial=observe(g);
   command={t:'move',orders:[[actor.id,65,51]]};commandReturn=record('command',()=>sim.command(g,0,command));assert.equal(commandReturn,undefined);postCommand=observe(g);
  },onTick:g=>{const before=stages.at(-1)?.unit??postCommand.unit;const snapshot=observe(g);const dx=actor.x-before.x.value,dz=actor.z-before.z.value;snapshot.movement={actorOrdinal:[...g.units.keys()].indexOf(actor.id),before:{x:before.x,z:before.z},after:{x:snapshot.unit.x,z:snapshot.unit.z},dx:real(dx),dz:real(dz),distance:real(Math.sqrt(dx*dx+dz*dz)),cell:Math.floor(actor.z/sim.CELL)*g.w+Math.floor(actor.x/sim.CELL)};assert.ok(snapshot.movement.distance.value>0);assert.equal(snapshot.movement.cell,c);assert.deepEqual(snapshot.mines,[]);assert.deepEqual(snapshot.shots,[]);stages.push(snapshot);}});
 assert.equal(g.tick,2);assert.equal(tape.filter(r=>r.phase!=='creation').length,0);
 cases.push({name:scene.name,setup:{map,options,seed,creationSeed,spawn:{id:actor.id,type:'tank',owner:0,x:51,z:51,rot:0,ghost:0},allActors:{holdFire:true,holdPos:true,auto:false,autoRetreat:false},wearPrime:{c,value:scene.wear,stateRule:'floor(actual Float32 wear * 4), capped at 3'},cellJournal:[]},initial,command:{slot:0,body:command,returned:{kind:'undefined'}},postCommand,steps:stages,randomTape:tape});
}
const find=name=>cases.find(s=>s.name===name),cell=s=>s.cells.find(r=>r.c===c);
assert.equal(find('dry-open-subthreshold').steps[0].newCellsDelta.length,0);
assert.equal(cell(find('dry-open-packed-quarter').steps[0]).state,1);
assert.equal(find('dry-open-packed-quarter').steps[0].newCellsDelta.length,1);
assert.equal(cell(find('dry-open-mud-transition').steps[0]).ch,'M');
assert.equal(cell(find('dry-open-mud-transition').steps[0]).wear.value,0);
assert.equal(find('dry-open-mud-transition').steps[0].newCellsDelta.length,2);
for(const s of cases)assert.equal(s.steps[0].unit.dust,s.name.startsWith('dry-open')&&!s.name.includes('transition')?1:null);
clean();sourceFiles.forEach((f,i)=>assert.deepEqual(readFileSync(resolve(reference,f)),sourceBytes[i]));
const content=JSON.stringify({reference:pin,sourceHashes,node:process.version,arch:process.arch,boundary:'Unmodified Source playMatch/createGame/command/full step; declared initial drill position and wear. Source schedule seeded global RNG. No native full-step claim.',steps:cases.length*2,cases})+'\n';
const target=resolve(process.env.GROUND_CONTACT_FIXTURE??resolve(root,'rust/crates/core/tests/fixtures/ground-contact.json'));
if(process.argv.includes('--check'))assert.equal(readFileSync(target,'utf8'),content);else writeFileSync(target,content);
console.log(`${process.argv.includes('--check')?'Verified':'Recorded'} ${cases.length} ground contact cases, ${cases.length*2} full steps`);
