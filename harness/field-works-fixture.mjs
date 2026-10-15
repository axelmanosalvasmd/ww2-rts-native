// Paid dig admission and cell work from original source, with explicit sight and path hooks.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync,writeFileSync,mkdtempSync,rmSync } from 'node:fs';
import { join,resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath,pathToFileURL } from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
assert.ok(process.argv.slice(2).every(v=>v==='--check'));
const root=fileURLToPath(new URL('../',import.meta.url)),reference=resolve(process.env.WW2_REFERENCE??join(root,'reference/ww2-rts'));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};clean();
const temp=mkdtempSync(join(tmpdir(),'ww2-field-works-'));
try {
 const source=readFileSync(join(reference,'shared/sim.js'),'utf8');
 const one=(start,end)=>{const a=source.indexOf(start),b=source.indexOf(end,a);assert.ok(a>=0&&b>a);return source.slice(a,b);};
 const admission=one('    // one squad builds a field fortification','  } else if (cmd.t === \'entrench\')');
 const work=one('    // digging: walk to the spot','    // grenade order:');
 let code=source.replace('  setupStructures(g, map);','  setupStructures(g, map);\n  return g;').replace(/from '(\.\/[^']+)'/g,(_,p)=>`from '${pathToFileURL(join(reference,'shared',p)).href}'`);
 code+=`\nexport function fieldStart(g,slot,cmd) {
 const ids=cmd.ids??[];
 const mine=id=>{const u=g.units.get(id);return u&&u.owner===slot&&!u.operative&&!UNITS[u.type].structure&&!UNITS[u.type].rail&&!u.riding&&!u.logistics?.forced?u:null;};
 const teamSees=(g,team,at)=>!g.hidden.has(cellOf(g,at.x,at.z));
 const tally=(g,owner,k,v=1)=>g.fieldEvents.push({owner,kind:k,value:v});
 ${admission}
}
export function fieldStep(g,id,dt) {
 const u=g.units.get(id); if(!u||u.hp<=0)return;
 const requestStepPath=(g,u,at,kind)=>g.fieldRoutes.push({unit:u.id,at:{x:at.x,z:at.z}});
 const tally=(g,owner,k,v=1)=>g.fieldEvents.push({owner,kind:k,value:v});
 ${work}
}
export const fieldHooks={digProgress,cellOf};\n`;
 writeFileSync(join(temp,'sim.mjs'),code);
 const sim=await import(pathToFileURL(join(temp,'sim.mjs')));
 const {rebuildGrid}=await import(pathToFileURL(join(reference,'shared/grid.js')));
 const number=new DataView(new ArrayBuffer(8));
 const bits=value=>{number.setFloat64(0,value,false);return number.getBigUint64(0,false).toString(16).padStart(16,'0');};
 const encode=value=>typeof value==='number'?bits(value):Array.isArray(value)?value.map(encode):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([k,v])=>[k,encode(v)])):value;
 const runs=values=>{const out=[];for(const value of values){if(out.length&&Object.is(out.at(-1)[0],value))out.at(-1)[1]++;else out.push([value,1]);}return out;};
 const floats=value=>value?runs(Array.from(new Uint32Array(value.buffer,value.byteOffset,value.length))):null;
 const sorted=value=>[...value].sort((a,b)=>a[0]-b[0]);
 const snapshot=g=>encode({mp:g.players.map(p=>p.mp),jobs:[...g.units.values()].filter(u=>u.dig).sort((a,b)=>a.id-b.id).map(u=>[u.id,u.dig]),progress:[...g.units.values()].filter(u=>u.dig).sort((a,b)=>a.id-b.id).map(u=>[u.id,sim.fieldHooks.digProgress(u)]),pits:[...(g.pits??[])].sort((a,b)=>a-b),world:{chars:g.chars.join(''),ground:g.ground.join(''),objects:g.objects.join(''),flags:runs(g.flags),mines:runs(g.mineLayer),hp:floats(g.cellHp),wear:floats(g.wear),front:floats(g.trenchFront),mineOwners:sorted(g.mines),mineSeen:sorted(g.mineSeen),aid:sorted(g.aid),journal:[...g.cellJournal],versions:[g.terrainVersion??0,g.obstructionVersion??0,g.infantryRegionVersion??0,g.vehicleRegionVersion??0,g.navalRegionVersion??0]}});
 const blank=()=>({name:'Field works',w:20,h:20,rows:Array(20).fill('.'.repeat(20)),spawns:[{x:1,y:1},{x:18,y:18}],points:[{x:10,y:10}]});
 const inputUnit=(id=1,type='rifle',owner=0,x=20,z=23)=>({id,type,owner,x,z});
 const start=(kind,extras={})=>({op:'start',unit:1,owner:0,kind,x:20,z:20,...extras});
 const step=(dt,unit=1)=>({op:'step',dt,unit});
 const traces=[];
 function record(name,map,units,actions,mp=500) {
  const g=sim.createGame(map,[],false,[],[],{weather:false});g.players=[0,1].map(slot=>({slot,team:slot,mp,away:false,out:false}));g.tick=10;g.hidden=new Set();g.cellJournal=[];g.fieldEvents=[];g.fieldRoutes=[];
  if(actions.some(action=>action.op==='blast')){g.shots=[];g.seed=123456789;g.nextId=1000;g.wx={rain:0,wet:0};g.wind={a:0,v:0};}
  for(const v of units){const d=sim.UNITS[v.type];g.units.set(v.id,{...v,hp:v.hp??d.hpPer*d.models,supp:0,garrison:-1,riding:v.riding??0,path:[],repath:0,retreating:v.retreating??false,orders:[]});}
  const initial=snapshot(g),rows=[];
  for(const action of actions){g.fieldEvents=[];g.fieldRoutes=[];let result=null;
   if(action.op==='start'){g.hidden=new Set(action.hidden??[]);result=sim.fieldStart(g,action.owner,{t:'dig',ids:[action.unit],kind:action.kind,x:action.x,z:action.z,dir:action.dir})??null;}
   else if(action.op==='step')sim.fieldStep(g,action.unit,action.dt);
   else if(action.op==='cancel')g.units.get(action.unit).dig=null;
   else if(action.op==='move'){Object.assign(g.units.get(action.unit),action.fields);}
   else if(action.op==='money')g.players[action.owner].mp=action.mp;
   else if(action.op==='mutate')sim.internals.setCell(g,action.cell,action.ch);
   else if(action.op==='blast'){rebuildGrid(g);sim.damageCells(g,[...g.units.values()],action.at,action.radius,action.damage);}
   else if(action.op==='roadState'){
    const c=action.cell;
    result={ch:g.chars[c],ground:g.ground[c],object:g.objects[c],mine:!!g.mineLayer[c],wear:g.wear[c],hp:g.cellHp[c],flags:g.flags[c],material:sim.worldMaterial(g,c).name,groundMaterial:sim.worldMaterial(g,c,true).name};
   }
   else if(action.op==='seen')g.mineSeen.set(action.cell,action.mask);
   else if(action.op==='preview'){g.hidden=new Set(action.hidden??[]);const p=sim.placementCheck(g,{kind:action.kind,x:action.x,z:action.z,dir:action.dir??0,team:action.team},at=>!g.hidden.has(sim.fieldHooks.cellOf(g,at.x,at.z)));result={ok:p.ok,reason:p.reason??null,cells:p.cells??[]};}
   else throw Error(action.op);
   rows.push({action,result:encode(result),events:encode(g.fieldEvents),routes:encode(g.fieldRoutes),snapshot:snapshot(g)});
  }
  traces.push({name,map,units,mp,initial,rows});
 }
 for(const kind of ['trench','sandbags','wire','traps','nest','mines','aid']) {
  record(`complete:${kind}`,blank(),[inputUnit()], [start(kind),...Array.from({length:kind==='aid'?1:6},()=>step(kind==='aid'?12:3))]);
 }
 record('engineer-time-boundary',blank(),[inputUnit(1,'engineer')],[start('trench',{dir:0}),step(1.49),step(.01),step(9),step(1.5),step(1.5)]);
 record('twenty-hertz-trench',blank(),[inputUnit()], [start('trench'),...Array.from({length:242},()=>step(.05))]);
 record('route-and-reach',blank(),[inputUnit(1,'rifle',0,5,5)],[start('trench'),step(1),{op:'move',unit:1,fields:{repath:1}},step(1),{op:'move',unit:1,fields:{x:20,z:23,repath:0}},step(3)]);
 record('refused-replacement-and-cancel',blank(),[inputUnit()], [start('trench'),step(1),{op:'money',owner:0,mp:0},start('wire'),step(2),{op:'cancel',unit:1},step(100)]);
 record('visibility-before-ground',blank(),[inputUnit()], [{op:'mutate',cell:210,ch:'H'},start('trench',{dir:0,hidden:[210]}),{op:'preview',kind:'trench',x:20,z:20,dir:0,hidden:[210]},start('trench',{dir:0}),step(3)]);
 record('changed-ground-is-paid-and-skipped',blank(),[inputUnit()], [start('wire',{dir:0}),{op:'mutate',cell:208,ch:'H'},...Array.from({length:5},()=>step(3))]);
 record('traps-never-under-a-vehicle',blank(),[inputUnit(),inputUnit(2,'tank',1,19,21)], [start('traps',{dir:0}),...Array.from({length:4},()=>step(3))]);
 record('hospital-max-standing-and-going-up',blank(),[inputUnit(),inputUnit(2,'engineer',0,24,23)],[start('aid'),start('aid',{unit:2,x:24}),step(12),start('aid',{unit:2,x:24}),{op:'mutate',cell:210,ch:'.'},start('aid',{unit:2,x:24})]);
 record('nonbuilders-retreat-riding-and-empty-plan',blank(),[inputUnit(1,'mg'),inputUnit(2,'rifle',0,20,23),inputUnit(3,'rifle',0,20,23)],[start('trench'),{op:'move',unit:2,fields:{retreating:true}},start('trench',{unit:2}),{op:'move',unit:3,fields:{riding:99}},start('trench',{unit:3}),start('sandbags',{unit:2,x:-100,z:-100})]);
 {const map=blank();map.rows[10]='........DDDD........';record('mine-road-and-known-clearing',map,[inputUnit(),inputUnit(2,'rifle',1,20,23)],[start('mines',{dir:0}),...Array.from({length:4},()=>step(3)),start('demine',{unit:2,owner:1,dir:0}),{op:'seen',cell:208,mask:2},{op:'seen',cell:209,mask:2},start('demine',{unit:2,owner:1,dir:0}),step(3,2),step(3,2),start('demine',{dir:0}),step(3),step(3)]);}
 {const map=blank();map.rows[10]='........NNNN........';record('demine-distant-cached-approach',map,[inputUnit(1,'engineer',0,1,21)],[{op:'seen',cell:208,mask:1},start('demine',{dir:0}),step(0),step(1),{op:'move',unit:1,fields:{x:14,z:21}},step(1.5)]);}
 for(const dir of [0,Math.PI/2,-1.2,7.5])record(`placement-angle:${dir}`,blank(),[inputUnit()], [{op:'preview',kind:'nest',x:1,z:1,dir},start('trench',{dir,x:1,z:1}),step(3)]);
 // ENG-13: complete wire with both named builders, retaining the existing MG refusal.
 for(const type of ['engineer','conscript']){
  record(`complete:wire-${type}`,blank(),[inputUnit(1,type)],[start('wire',{dir:0}),...Array.from({length:5},()=>step(type==='engineer'?1.5:3))]);
  const trace=traces.at(-1),last=trace.rows.at(-1).snapshot;
  assert.equal([...last.world.chars].filter(ch=>ch==='X').length,5);
  assert.equal(last.jobs.length,0);
 }
 // ENG-14: the entire visible plan is woods, with an independent open capture point.
 {
  const map=blank();map.points=[{x:4,y:4}];map.rows[10]='........OOOO........';assert.equal(sim.validateMap(map),null);
  record('woods-trench-blocked',map,[inputUnit()],[start('trench',{dir:0})]);
  const trace=traces.at(-1),row=trace.rows[0];
  assert.equal(row.result,'blocked');assert.deepEqual(row.snapshot,trace.initial);
 }
 // ENG-66: real damage creates wear before paid laying and known clearing.
 // The second real hit must cross the original road threshold beneath rubble.
 {
  const map=blank();map.points=[{x:4,y:4}];map.worldVersion=2;map.rows[10]='........RRRR........';
  const ground=Array(20).fill('.'.repeat(20));ground[10]='........DDDD........';
  map.layers={ground,objects:[...map.rows],groundMaterials:[{c:208,material:'concrete'}],objectMaterials:[{c:208,material:'stone'}]};
  assert.equal(sim.validateMap(map),null);
  const observe=phase=>({op:'roadState',cell:208,phase});
  record('mine-rubble-road-damage-threshold',map,[inputUnit(1,'engineer')],[
   {op:'blast',at:{x:17,z:21},radius:0,damage:120},observe('before'),
   start('mines',{dir:0}),...Array.from({length:4},()=>step(1.5)),observe('mined'),
   start('demine',{dir:0}),...Array.from({length:4},()=>step(1.5)),observe('cleared'),
   {op:'blast',at:{x:17,z:21},radius:0,damage:85},observe('threshold'),
  ]);
  const values=traces.at(-1).rows.filter(row=>row.action.op==='roadState').map(row=>row.result);
  const [before,mined,cleared,threshold]=values;
  assert.equal(mined.mine,true);assert.equal(cleared.mine,false);
  for(const value of [mined,cleared])for(const key of ['ground','object','wear','hp','flags','material','groundMaterial'])assert.equal(value[key],before[key],key);
  assert.equal(threshold.ground,'+');assert.equal(threshold.object,'R');
 }
 clean();const content=JSON.stringify({reference:pin,node:process.version,arch:process.arch,traces})+'\n',path=join(root,'rust/crates/core/tests/fixtures/field-works.json');
 if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),content);else writeFileSync(path,content);
 console.log(`${process.argv.includes('--check')?'Verified':'Recorded'} ${traces.length} field-work traces, ${traces.reduce((n,t)=>n+t.rows.length,0)} actions`);
} finally {rmSync(temp,{recursive:true,force:true});}
