// Authoritative environment traces from original helpers, with no reference edits.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync,writeFileSync,readdirSync,mkdtempSync,rmSync } from 'node:fs';
import { join,resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath,pathToFileURL } from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');assert.ok(process.argv.slice(2).every(a=>a==='--check'));
const root=fileURLToPath(new URL('../',import.meta.url)),reference=resolve(process.env.WW2_REFERENCE??join(root,'reference/ww2-rts'));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};clean();
const temp=mkdtempSync(join(tmpdir(),'ww2-environment-'));
try {
 let code=readFileSync(join(reference,'shared/sim.js'),'utf8'),marker='  setupStructures(g, map);';assert.equal(code.split(marker).length,2);
 code=code.replace(marker,marker+'\n  return g;').replace(/from '(\.\/[^']+)'/g,(_,p)=>`from '${pathToFileURL(join(reference,'shared',p)).href}'`);
 code+='\nexport const environmentHooks={ignite,burn,flood,wreckCell,stepWorldDebris,registerBuildingStructure,stepStrikes};\n';writeFileSync(join(temp,'sim.mjs'),code);
 const sim=await import(pathToFileURL(join(temp,'sim.mjs'))),H=sim.environmentHooks;
 const {rebuildGrid}=await import(pathToFileURL(join(reference,'shared/grid.js')));
 const runs=a=>{const out=[];for(const v of a){if(out.length&&out.at(-1)[0]===v)out.at(-1)[1]++;else out.push([v,1]);}return out;};
 const bits=a=>a&&runs(new Uint32Array(a.buffer,a.byteOffset,a.length));const sorted=m=>[...m].sort((a,b)=>a[0]-b[0]);
 const section=s=>({id:s.id,structureId:s.structureId,c:s.c,hp:s.hp,maxHp:s.maxHp,material:s.material,anchor:s.anchor,supports:s.supports,state:s.state,rubble:s.rubble,impulse:s.impulse,failedAt:s.failedAt,direction:s.direction,settledAt:s.settledAt});
 function snapshot(g){return {chars:g.chars.join(''),ground:g.ground.join(''),objects:g.objects.join(''),mines:runs(g.mineLayer),flags:runs(g.flags),height:g.height&&runs(g.height),hp:bits(g.cellHp),wear:bits(g.wear),surfaceWear:bits(g.surfaceWear),burnt:runs(g.burnt),scar:bits(g.scar),state:runs(g.cellState),materials:sorted(g.materials),baseMaterials:sorted(g.baseMaterials),objectMaterials:sorted(g.objectMaterials),mineMaterials:sorted(g.mineMaterials),mineOwners:sorted(g.mines),mineSeen:sorted(g.mineSeen),fires:[...g.fires],versions:[g.terrainVersion??0,g.obstructionVersion??0,g.infantryRegionVersion??0,g.vehicleRegionVersion??0,g.navalRegionVersion??0],soak:[...g.soak],journal:[...g.cellJournal],log:g.cellLog,newCells:g.newCells,structures:[...g.structures.values()].map(s=>[s.id,s.kind,s.entityId??null,s.sections.map(section)]),units:[...g.units.values()].map(u=>[u.id,u.hp,u.supp,u.garrison??-1,u.path,u.lastHit??null]),smokes:g.smokes,falling:g.fallingSections,contacts:[...g.worldContacts],seed:g.seed>>>0,nextId:g.nextId};}
 const cases=[];const random=Math.random;Math.random=()=>0.5;
 function record(name,map,actions,units=[],names=[]){assert.equal(sim.validateMap(map),null);const g=sim.createGame(map,names,false,names.map((_,i)=>i),names.map(()=>0),{weather:false,logistics:false});g.seed=123456789;g.nextId=1000;g.tick=10;g.wind={a:0.7,v:0.6};g.wx={rain:0,wet:0};g.cellJournal=[];g.shots=[];g.smokes=[];
  for(const input of units){const d=sim.UNITS[input.type];g.units.set(input.id,{...input,owner:input.owner??0,hp:input.hp??d.hpPer*d.models,supp:input.supp??0,garrison:input.garrison??-1,path:input.path??[],air:input.air,riding:input.riding,heavy:!d.infantry});}
  rebuildGrid(g);const memory=new Set(),shots=[],lastHit=new Map();let result=null;
  const snapshots=[structuredClone(snapshot(g))],results=[];
  for(const a of actions){g.shots=[];result=null;g.tick=a.tick??g.tick;
   if(a.op==='ignite')H.ignite(g,a.c);
   else if(a.op==='burn')H.burn(g,[...g.units.values()],a.dt);
   else if(a.op==='burn_captured')H.burn(g,a.actors.map(id=>g.units.get(id)),a.dt);
   else if(a.op==='para_birth'){
    g.players[0].mp=1000;
    assert.equal(sim.command(g,0,{t:'support',kind:'para',x:20,z:20,dir:0}),undefined);
    a.timeline=[];const before=new Set(g.units.keys());let arrived=false;
    for(let i=1;i<=121;i++){
     g.tick=(a.tick??9)+i;const list=[...g.units.values()];g.shots=[];H.stepStrikes(g,list,.05);
     a.timeline.push({tick:g.tick,shots:structuredClone(g.shots),live:g.strikes.map(s=>!!s.live),count:g.strikes.length});
     if(i<121){assert.equal(g.shots.length,0);assert.equal(g.strikes.length,1);assert.equal(g.strikes[0].live,false);}
     else {assert.equal(g.shots.length,4);assert.equal(g.shots[0].k,'para');assert(g.shots.slice(1).every(s=>s.k==='chutes'));assert.equal(g.strikes.length,0);a.actors=list.map(u=>u.id);arrived=true;}
    }
    assert(arrived);a.births=[...g.units.values()].filter(u=>!before.has(u.id)).map(u=>({id:u.id,type:u.type,owner:u.owner,x:u.x,z:u.z,hp:u.hp,supp:u.supp,garrison:u.garrison,path:u.path}));
    a.arrivalTick=g.tick;a.nextId=g.nextId;g.shots=[];
   }
   else if(a.op==='ignite_births'){
    for(const u of g.units.values())if(u.id>=1000){const c=Math.floor(u.z/2)*g.w+Math.floor(u.x/2);sim.mutateWorldCell(g,c,{object:'H'});H.ignite(g,c);}
   }
   else if(a.op==='flood')H.flood(g);
   else if(a.op==='debris')H.stepWorldDebris(g,a.dt);
   else if(a.op==='contact_loop'){for(let i=0;i<a.count;i++)sim.damageWorldSection(g,a.c,a.damage,{contactId:`fifo:${i}`});}
   else if(a.op==='section')result=sim.damageWorldSection(g,a.c,a.damage==='Infinity'?Infinity:a.damage,{list:[...g.units.values()],owner:a.owner,contactId:a.contact,direction:a.direction,debitEntity:a.debit});
   else if(a.op==='blast')sim.damageCells(g,[...g.units.values()],a.at,a.radius,a.damage,a.scar??0,{owner:a.owner,contactId:a.contact,direction:a.direction,skipBuildings:new Set(a.skip??[]),damagedCells:a.memory?memory:undefined});
   else if(a.op==='wreck')H.wreckCell(g,[...g.units.values()],a.c,a.into);
   else if(a.op==='set')H.setCell?.(g,a.c,a.ch); // unused; mutations use the exported validator
   else if(a.op==='mutate')sim.mutateWorldCell(g,a.c,a.change);
   else if(a.op==='building'){const u=g.units.get(a.id);u.cells=a.cells;u.hp=a.hp;H.registerBuildingStructure(g,u);}
   else if(a.op==='prime'){
    if(a.rain!==undefined)g.wx.rain=a.rain;if(a.wet!==undefined)g.wx.wet=a.wet;if(a.wind)g.wind=a.wind;
    if(a.fire){if(a.fire[1]===null)g.fires.delete(a.fire[0]);else g.fires.set(...a.fire);}
    if(a.wear)g.wear[a.wear[0]]=a.wear[1];if(a.scar)g.scar[a.scar[0]]=a.scar[1];if(a.hp)g.cellHp[a.hp[0]]=a.hp[1];
   } else throw Error(a.op);
   results.push({value:result,shots:structuredClone(g.shots)});snapshots.push(structuredClone(snapshot(g)));
  }cases.push({name,map,units,actions,results,snapshots});
 }
 const blank=()=>({name:'Environment',w:20,h:20,rows:Array(20).fill('.'.repeat(20)),spawns:[{x:0,y:0},{x:19,y:19}],points:[{x:10,y:10}]});
 const cell=(m,c,ch)=>{const y=Math.floor(c/m.w),x=c%m.w;m.rows[y]=m.rows[y].slice(0,x)+ch+m.rows[y].slice(x+1);};
 for(const filename of readdirSync(join(reference,'maps')).filter(n=>n.endsWith('.json')).sort()){
  const m=JSON.parse(readFileSync(join(reference,'maps',filename),'utf8'));const c=m.rows.join('').search(/[BHOT#XYN]/);const target=c>=0?c:Math.floor(m.w*m.h/2);const at={x:(target%m.w+.5)*2,z:(Math.floor(target/m.w)+.5)*2};
  record(`authored:${filename}`,m,[{op:'blast',at,radius:3,damage:95,scar:.4},{op:'burn',dt:.5},{op:'blast',at,radius:1,damage:1000,scar:1},{op:'debris',dt:.5},{op:'debris',dt:2},{op:'flood'}]);
 }
 {const m=blank();for(const [c,ch]of [[42,'D'],[43,'+'],[44,'T'],[45,'N'],[62,'H'],[63,'O'],[64,'#'],[65,'Y']])cell(m,c,ch);
 record('surfaces-trench-mines-and-wrecks',m,[{op:'blast',at:{x:9,z:5},radius:4,damage:180,scar:1},{op:'blast',at:{x:9,z:5},radius:4,damage:900,scar:1},{op:'burn',dt:30},{op:'flood'}]);}
 {const m=blank();cell(m,42,'H');cell(m,43,'H');cell(m,44,'O');record('fire-order-rain-expiry-and-path',m,[{op:'ignite',c:44},{op:'ignite',c:42},{op:'prime',fire:[44,null]},{op:'ignite',c:44},{op:'burn',dt:0},{op:'burn',dt:.5},{op:'prime',rain:1},{op:'burn',dt:5},{op:'prime',wet:.35},{op:'ignite',c:100},{op:'burn',dt:40}], [{id:7,type:'rifle',x:5,z:5,hp:200},{id:8,type:'rifle',x:9,z:5,hp:200},{id:9,type:'rifle',x:5,z:5,hp:200,path:[{x:5,z:7}]}]);}
 {const m=blank();for(const c of [42,43,44])cell(m,c,'B');m.structures=[{id:'chain',kind:'house',sections:[{id:'a',c:42,hp:100,material:'wood',anchor:true,supports:['b']},{id:'b',c:43,hp:100,material:'stone',anchor:false,supports:['c']},{id:'c',c:44,hp:100,material:'concrete',anchor:false,supports:[]}]}];
 record('support-loss-contact-and-garrison',m,[{op:'section',c:42,damage:20,contact:'one',owner:1},{op:'section',c:42,damage:20,contact:'one',owner:1},{op:'section',c:42,damage:1000,contact:'two',direction:{x:3,z:4}},{op:'debris',dt:2.5}], [{id:7,type:'rifle',x:7,z:5,garrison:43}]);}
 {const m=blank();for(const c of [42,43,44,62,63,64])cell(m,c,'B');record('burning-house-collapse',m,[{op:'ignite',c:42},{op:'burn',dt:20},{op:'burn',dt:20},{op:'debris',dt:2.5}]);}
 {const m=blank();for(const c of [42,43])cell(m,c,'W');cell(m,42,'=');cell(m,43,'=');record('bridge-occupants',m,[{op:'section',c:42,damage:'Infinity'},{op:'section',c:43,damage:'Infinity'},{op:'debris',dt:2.5}],[{id:7,type:'rifle',x:5,z:5},{id:8,type:'tank',x:7,z:5},{id:9,type:'rifle',x:7,z:5,riding:true},{id:10,type:'gunboat',x:5,z:5}]);}
 {const m=blank();record('building-shared-hp-and-beam',m,[{op:'mutate',c:42,change:{object:'K'}},{op:'mutate',c:43,change:{object:'K'}},{op:'building',id:7,cells:[42,43],hp:250},{op:'blast',at:{x:6,z:5},radius:3,damage:100,memory:true,owner:1},{op:'blast',at:{x:6,z:5},radius:3,damage:100,memory:true,owner:1},{op:'blast',at:{x:6,z:5},radius:3,damage:100,skip:[7]},{op:'section',c:42,damage:400,debit:false,owner:0},{op:'debris',dt:2.5}],[{id:7,type:'hq',x:6,z:5}]);}
 {const m=blank();for(const c of [42,43,44,45])cell(m,c,'+');cell(m,41,'W');record('flood-one-wave-and-slope',m,[{op:'prime',wear:[42,.65]},{op:'flood'},{op:'flood'},{op:'flood'},{op:'flood'},{op:'flood'}]);}
 {const m=blank();m.structures=[];for(let c=40;c<160;c++)cell(m,c,'B');record('debris-overflow',m,[{op:'blast',at:{x:20,z:10},radius:20,damage:100000},{op:'debris',dt:2.5}]);}
 {const m=blank();cell(m,42,'T');cell(m,43,'Y');record('float32-assignment-expression-boundaries',m,[{op:'prime',wear:[42,0.99999994]},{op:'blast',at:{x:5,z:5},radius:0,damage:0.000016},{op:'prime',scar:[44,0.99999994]},{op:'blast',at:{x:9,z:5},radius:0,damage:0.000024,scar:1},{op:'prime',hp:[43,1.401298464324817e-45]},{op:'blast',at:{x:7,z:5},radius:0,damage:1e-45}]);}
 {const m=blank();cell(m,42,'H');record('captured-burn-after-real-paratrooper-arrival',m,[{op:'ignite',c:42},{op:'para_birth',tick:9},{op:'ignite_births'},{op:'burn_captured',actors:[7,8],dt:.5},{op:'burn',dt:.5}], [{id:7,type:'rifle',x:5,z:5,hp:100},{id:8,type:'rifle',x:20,z:20,hp:100}],['P0']);}
 {const m=blank();cell(m,42,'B');record('captured-burn-keeps-current-garrison-eviction',m,[{op:'ignite',c:42},{op:'burn_captured',actors:[7],dt:40}],[{id:7,type:'rifle',x:5,z:5,hp:200,garrison:42},{id:8,type:'rifle',x:5,z:5,hp:100,garrison:42}]);}
 {const m=blank();cell(m,42,'H');record('captured-burn-legacy-expiry',m,[{op:'ignite',c:42},{op:'burn_captured',actors:[7],dt:40}],[{id:7,type:'rifle',x:5,z:5,hp:500},{id:8,type:'rifle',x:5,z:5,hp:100,garrison:42}]);}
 // Compact expected states as path replacements. Every Rust comparison reconstructs the full state.
 function changes(a,b,path=[],out=[]){
  if(Object.is(a,b))return out;
  if(Array.isArray(a)&&Array.isArray(b)&&a.length===b.length){for(let i=0;i<a.length;i++)changes(a[i],b[i],[...path,i],out);return out;}
  if(a&&b&&typeof a==='object'&&typeof b==='object'&&!Array.isArray(a)&&!Array.isArray(b)){
   for(const key of new Set([...Object.keys(a),...Object.keys(b)])){if(!Object.hasOwn(b,key))out.push([[...path,key]]);else changes(a[key],b[key],[...path,key],out);}return out;
  }
  out.push([path,b]);return out;
 }
 {const m=blank();for(const c of [42,43])cell(m,c,'B');m.structures=[{id:'old',kind:'house',sections:[{id:'a',c:42,hp:100,material:'wood',anchor:true,supports:['b']},{id:'b',c:43,hp:100,material:'wood',anchor:false,supports:[]}]}];record('detached-section-support-failure',m,[{op:'mutate',c:43,change:{object:'K'}},{op:'building',id:7,cells:[43],hp:200},{op:'section',c:42,damage:500},{op:'debris',dt:2.5}],[{id:7,type:'hq',x:7,z:5}]);}
 {const m=blank();cell(m,42,'B');m.structures=[{id:'durable',kind:'house',sections:[{id:'a',c:42,hp:100000,material:'concrete',anchor:true,supports:[]}]}];record('contact-fifo-limit',m,[{op:'contact_loop',c:42,count:2050,damage:0.000001},{op:'section',c:42,damage:0.000001,contact:'fifo:0'},{op:'section',c:42,damage:0.000001,contact:'fifo:2049'}]);}
 for(const c of cases){c.snapshots=c.snapshots.map(s=>JSON.parse(JSON.stringify(s)));c.initial=c.snapshots[0];c.patches=c.snapshots.slice(1).map((s,i)=>changes(c.snapshots[i],s));delete c.snapshots;}
 Math.random=random;clean();const content=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases})+'\n';const path=join(root,'rust/crates/core/tests/fixtures/environment.json');if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),content);else writeFileSync(path,content);
 console.error(`${process.argv.includes('--check')?'Verified':'Recorded'} ${cases.length} environment traces, ${cases.reduce((n,c)=>n+c.actions.length,0)} actions`);
}finally{rmSync(temp,{recursive:true,force:true});}
