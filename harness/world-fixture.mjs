// Record original terrain mutations and journals without changing the pinned checkout.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync,writeFileSync,mkdtempSync,rmSync } from 'node:fs';
import { join,resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath,pathToFileURL } from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
assert.ok(process.argv.slice(2).every(a=>a==='--check'));
const root=fileURLToPath(new URL('../',import.meta.url)), reference=resolve(process.env.WW2_REFERENCE??join(root,'reference/ww2-rts'));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};clean();
const temp=mkdtempSync(join(tmpdir(),'ww2-world-'));
try {
 let code=readFileSync(join(reference,'shared/sim.js'),'utf8');const marker='  setupStructures(g, map);';assert.equal(code.split(marker).length,2);
 code=code.replace(marker,marker+'\n  return g;').replace(/from '(\.\/[^']+)'/g,(_,p)=>`from '${pathToFileURL(join(reference,'shared',p)).href}'`);
 code+='\nexport const worldHooks={setCell,setLevel,touch,logCell,registerBuildingStructure};\n';writeFileSync(join(temp,'sim.mjs'),code);
 const sim=await import(pathToFileURL(join(temp,'sim.mjs'))),H=sim.worldHooks;
 const runs=a=>{const r=[];for(const v of a){if(r.length&&r.at(-1)[0]===v)r.at(-1)[1]++;else r.push([v,1]);}return r;};
 const floats=a=>a&&runs(new Uint32Array(a.buffer,a.byteOffset,a.length));const sorted=m=>[...m].sort((a,b)=>a[0]-b[0]);
 const section=s=>[s.id,s.structureId,s.c,s.hp,s.maxHp,s.material,s.anchor,s.supports,s.state,s.rubble];
 function snapshot(g){return {
  chars:g.chars.join(''),ground:g.ground.join(''),objects:g.objects.join(''),mines:runs(g.mineLayer),flags:runs(g.flags),height:g.height&&runs(g.height),
  hp:floats(g.cellHp),wear:floats(g.wear),surfaceWear:floats(g.surfaceWear),front:floats(g.trenchFront),burnt:runs(g.burnt),scar:floats(g.scar),state:runs(g.cellState),tough:g.tough&&runs(g.tough),
  materials:sorted(g.materials),baseMaterials:sorted(g.baseMaterials),objectMaterials:sorted(g.objectMaterials),mineMaterials:sorted(g.mineMaterials),
  mineOwners:sorted(g.mines),mineSeen:sorted(g.mineSeen),aid:sorted(g.aid),fires:sorted(g.fires),wrecks:g.wrecks.map(w=>[w.x,w.z,w.c??null,!!w.motion]),
  roads:g.roads,shoal:g.shoal?[...g.shoal]:null,versions:[g.terrainVersion??0,g.obstructionVersion??0,g.infantryRegionVersion??0,g.vehicleRegionVersion??0,g.navalRegionVersion??0],
  soak:[...g.soak],journal:[...g.cellJournal],log:g.cellLog,newCells:g.newCells,
  pending:g.players.map(p=>[p.slot,[...p.terrainPending].sort((a,b)=>a-b)]),watch:g.watchPending?[...g.watchPending].sort((a,b)=>a-b):null,
  structures:[...g.structures.values()].map(s=>[s.id,s.kind,s.entityId??null,s.sections.map(section)]),current:[...g.structuralCells].map(([c,s])=>[c,s.id]).sort((a,b)=>a[0]-b[0]),
  initial:{chars:g.initialTerrain.chars.join(''),ground:g.initialTerrain.ground.join(''),objects:g.initialTerrain.objects.join(''),mines:[...g.initialTerrain.mineLayer],height:g.initialTerrain.height,front:g.initialTerrain.trenchFront&&floats(Float32Array.from(g.initialTerrain.trenchFront))}
 };}
 const cases=[];const random=Math.random;Math.random=()=>0.5;
 function scenario(name,map,actions){assert.equal(sim.validateMap(map),null);const g=sim.createGame(map,[],false,[],[],{weather:false});g.cellJournal=[];
  const snapshots=[structuredClone(snapshot(g))],results=[];
  for(const a of actions){let result=null;
   if(a.op==='mutate') result=sim.mutateWorldCell(g,a.c,a.change);
   else if(a.op==='set')H.setCell(g,a.c,a.ch);
   else if(a.op==='level')H.setLevel(g,a.c,a.height);
   else if(a.op==='touch')H.touch(g,a.c);
   else if(a.op==='log')H.logCell(g,a.c,[],a.look);
   else if(a.op==='register'){if(a.slot==='watch')g.watchPending??=new Set(g.cellLog.keys());else g.players.push({slot:a.slot,terrainPending:new Set(g.cellLog.keys())});}
   else if(a.op==='ack'){const p=a.slot==='watch'?g.watchPending:g.players.find(p=>p.slot===a.slot).terrainPending;for(const i of a.indices)p.delete(i);}
   else if(a.op==='drain')g.newCells=[];
   else if(a.op==='drainSoak')g.soak.clear();
   else if(a.op==='prime'){
    const c=a.c;
    if(a.wear!==undefined)g.wear[c]=a.wear;if(a.scar!==undefined)g.scar[c]=a.scar;if(a.burnt!==undefined)g.burnt[c]=a.burnt;
    if(a.hp!==undefined)g.cellHp[c]=a.hp;if(a.fire!==undefined)g.fires.set(c,a.fire);if(a.aid!==undefined)g.aid.set(c,a.aid);
    if(a.owner!==undefined)g.mines.set(c,a.owner);if(a.seen!==undefined)g.mineSeen.set(c,a.seen);
    if(a.sectionHp!==undefined)g.structuralCells.get(c).hp=a.sectionHp;if(a.sectionState)g.structuralCells.get(c).state=a.sectionState;
    if(a.wreck)g.wrecks.push({x:a.wreck[0],z:a.wreck[1],c:a.wreck[2]??undefined,motion:a.wreck[3]?{}:undefined});
   } else if(a.op==='building'){a.maxHp=sim.UNITS.hq.hpPer;H.registerBuildingStructure(g,{id:a.id,cells:a.cells,hp:a.hp,type:'hq'});}
   else throw Error(a.op);
   results.push(result);const next=structuredClone(snapshot(g));delete next.initial;snapshots.push(next);
  }
  cases.push({name,map,actions,results,snapshots});
 }
 const blank=()=>({name:'World',w:20,h:20,rows:Array(20).fill('.'.repeat(20)),spawns:[{x:0,y:0},{x:19,y:19}],points:[{x:10,y:10}]});
 scenario('mutations-and-pending',blank(),[
  {op:'register',slot:0},{op:'register',slot:'watch'},{op:'drainSoak'},
  {op:'mutate',c:0,change:{ground:'M'}},{op:'prime',c:0,wear:.55,scar:.6,burnt:1},{op:'touch',c:0},
  {op:'mutate',c:0,change:{object:'T',material:'wood'}},{op:'prime',c:0,wear:.75,fire:10},{op:'mutate',c:0,change:{ground:'D'}},
  {op:'mutate',c:0,change:{object:'N',material:'steel'}},{op:'prime',c:0,owner:2,seen:3},{op:'set',c:0,ch:'.'},
  {op:'mutate',c:0,change:{object:'.'}},{op:'drain'},{op:'register',slot:1},{op:'ack',slot:0,indices:[0]},
  {op:'mutate',c:1,change:{height:0}},{op:'level',c:1,height:0},{op:'log',c:1,look:true},
  {op:'mutate',c:0,change:{}},{op:'mutate',c:2,change:{ground:'Z'}},{op:'mutate',c:2,change:{object:'!'}},{op:'mutate',c:2,change:{height:5}},{op:'mutate',c:2,change:{material:'glass'}},
  {op:'mutate',c:-1,change:{}},{op:'mutate',c:400,change:{}},{op:'mutate',c:2,change:{object:'A'}},{op:'prime',c:2,aid:4,fire:5},{op:'set',c:2,ch:'Q'},
  {op:'prime',c:2,wreck:[5,5,2,false]},{op:'prime',c:2,wreck:[7,7,null,true]},{op:'set',c:2,ch:'W'},{op:'ack',slot:'watch',indices:[0,1,2]},
  {op:'mutate',c:3,change:{mine:true,material:'steel'}},{op:'mutate',c:3,change:{object:'.'}},
  {op:'mutate',c:4,change:{ground:'D',material:'concrete'}},{op:'mutate',c:4,change:{object:'H',material:'wood'}},{op:'mutate',c:4,change:{mine:true,material:'steel'}},{op:'mutate',c:4,change:{mine:false}},
  {op:'prime',c:4,fire:10},{op:'mutate',c:4,change:{ground:'F'}},{op:'set',c:4,ch:'.'},
 ]);
 {const m=blank();m.rows[2]='..BBB...............';m.structures=[{id:'custom',kind:'house',sections:[{id:'one',c:42,hp:333.333,material:'stone',anchor:true,supports:[]}]}];scenario('canonical-sections-and-building-replacement',m,[
  {op:'register',slot:0},{op:'prime',c:42,sectionHp:111.111,sectionState:'burning',hp:111.111},{op:'set',c:42,ch:'K'},
  {op:'set',c:43,ch:'K'},{op:'building',id:7,cells:[42,43],hp:333},{op:'prime',c:42,sectionHp:25.555,sectionState:'damaged',hp:25.555},{op:'log',c:42,look:true},
  {op:'set',c:42,ch:'.'},{op:'building',id:8,cells:[42],hp:222},{op:'mutate',c:42,change:{ground:'M'}},
  {op:'prime',c:42,sectionHp:0,sectionState:'failed',hp:0},{op:'set',c:42,ch:'R'},
 ]);}
 for(const filename of ['no-mans-land.json','the-great-bridge.json','three-islands.json','el-alamein.json']){
  const m=JSON.parse(readFileSync(join(reference,'maps',filename),'utf8'));
  const letter=filename==='no-mans-land.json'?'T':filename==='the-great-bridge.json'?'=':filename==='three-islands.json'?'W':'N';
  const c=m.rows.join('').indexOf(letter);
  scenario(filename,m,[{op:'drainSoak'},{op:'register',slot:0},{op:'mutate',c,change:{material:'concrete'}},{op:'set',c,ch:'.'},{op:'set',c,ch:'='},{op:'level',c,height:-2}]);
 }
 Math.random=random;clean();const content=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases})+'\n';const path=join(root,'rust/crates/core/tests/fixtures/world.json');
 if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),content);else writeFileSync(path,content);
 console.error(`${process.argv.includes('--check')?'Verified':'Recorded'} ${cases.length} world traces, ${cases.reduce((n,c)=>n+c.actions.length,0)} actions`);
} finally {rmSync(temp,{recursive:true,force:true});}
