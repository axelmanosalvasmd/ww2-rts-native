// Apply actual original combat/death helpers to canonical world state, without reference edits.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdtempSync,rmSync} from 'node:fs';
import {join,resolve} from 'node:path';import {tmpdir} from 'node:os';import {fileURLToPath,pathToFileURL} from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');assert.ok(process.argv.slice(2).every(a=>a==='--check'));
const root=fileURLToPath(new URL('../',import.meta.url)),reference=resolve(process.env.WW2_REFERENCE??join(root,'reference/ww2-rts'));
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d',git=args=>execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};clean();
const temp=mkdtempSync(join(tmpdir(),'ww2-match-combat-'));
try{
 let code=readFileSync(join(reference,'shared/sim.js'),'utf8'),marker='  setupStructures(g, map);';assert.equal(code.split(marker).length,2);
 const deathStart='  for (const u of list) if (u.hp <= 0) {';const start=code.indexOf(deathStart),end=code.indexOf('\n  // Annihilation: a player is out',start);assert.ok(start>0&&end>start);assert.equal(code.split(deathStart).length,2);
 const death=code.slice(start,end);
 code=code.replace(marker,marker+'\n  return g;').replace(/from '(\.\/[^']+)'/g,(_,p)=>`from '${pathToFileURL(join(reference,'shared',p)).href}'`);
 code+=`\nexport const combatHooks={flightDamage,flightSuppress,localFlightDamage,blast,syncBuildingSections,registerBuildingStructure,stepWorldDebris,retire(g,list){${death}}};\n`;writeFileSync(join(temp,'sim.mjs'),code);
 const sim=await import(pathToFileURL(join(temp,'sim.mjs'))),H=sim.combatHooks;const grid=await import(pathToFileURL(join(reference,'shared/grid.js')));
 const body=b=>b&&structuredClone(b),section=s=>({id:s.id,hp:s.hp,state:s.state,rubble:s.rubble,impulse:s.impulse,failedAt:s.failedAt,direction:s.direction,settledAt:s.settledAt});
 const runs=a=>{const out=[];for(const v of a){if(out.length&&out.at(-1)[0]===v)out.at(-1)[1]++;else out.push([v,1]);}return out;},bits=a=>runs(new Uint32Array(a.buffer,a.byteOffset,a.length));
 const cases=[];const blank=()=>({name:'Combat',w:20,h:20,rows:Array(20).fill('.'.repeat(20)),spawns:[{x:0,y:0},{x:19,y:19}],points:[{x:10,y:10}]});
 const cell=(m,c,ch)=>{const y=Math.floor(c/20),x=c%20;m.rows[y]=m.rows[y].slice(0,x)+ch+m.rows[y].slice(x+1);};
 function record(name,map,units,actions){assert.equal(sim.validateMap(map),null);const g=sim.createGame(map,[],false,[],[],{weather:false});g.players=[0,1].map(slot=>({slot,team:slot,mp:100,out:false,tier:1,armory:{inf:0,armor:0,support:0}}));g.tech=false;g.tick=10;g.seed=123;g.wind={a:0,v:.5};g.wx={rain:0,wet:0};g.nextId=1000;g.winner=null;g.story=g.players.map(()=>({kills:0,losses:0,built:0,captures:0,mpSpent:0,supportCalls:0,planesDowned:0}));g.cellJournal=[];g.smokes=[];g.shots=[];
  const retained=new Map();for(const v of units){const d=sim.UNITS[v.type];const u={id:v.id,type:v.type,owner:v.owner??0,x:v.x,z:v.z,rot:v.rot??0,hp:v.hp??d.models*d.hpPer,supp:v.supp??0,path:v.path??[],riding:v.carrier??0,cargo:v.cargo??[],xp:v.xp??0,hitBy:0,hitAt:-1e9,hitFrom:null,retarget:v.retarget??7,retreating:v.retreating??false,sprint:v.sprint??0,garrison:v.garrison??-1,moveSpeed:0,vx:0,vz:0,...v};g.units.set(u.id,u);retained.set(u.id,u);}
  grid.setHeavy(u=>!sim.UNITS[u.type].infantry);grid.rebuildGrid(g);const groups=new Map(),results=[],snapshots=[];
  function snapshot(){return {units:[...g.units.values()].map(u=>[u.id,u.hp,u.supp,u.x,u.z,u.riding,u.cargo,u.xp,u.lastHit??null,u.deathImpulse??null,u.hitBy,u.hitAt,u.hitFrom,u.atHit??null,u.retarget,u.sprint]),retained:[...retained].map(([id,u])=>[id,u.xp]),suppressed:[...groups].map(([id,s])=>[id,[...s]]),wallets:g.players.map(p=>p.mp),chars:g.chars.join(''),hp:bits(g.cellHp),flags:runs(g.flags),height:g.height&&runs(g.height),wear:bits(g.wear),burnt:runs(g.burnt),structures:[...g.structures.values()].map(s=>[s.id,s.kind,s.sections.map(section)]),wrecks:g.wrecks.map(w=>[w.id,w.type,w.owner,w.x,w.z,w.rot,w.c,body(w.motion)]),falling:g.fallingSections,contacts:[...g.worldContacts],fires:[...g.fires],smokes:g.smokes,log:g.cellLog,journal:g.cellJournal,versions:[g.terrainVersion??0,g.obstructionVersion??0,g.infantryRegionVersion??0,g.vehicleRegionVersion??0,g.navalRegionVersion??0],seed:g.seed>>>0,nextId:g.nextId};}
  snapshots.push(structuredClone(snapshot(g)));
  for(const a of actions){g.shots=[];let result=null;
   const source=retained.get(a.shooter);const p=source&&{source,owner:source.owner,shooter:source.id,kind:source.type,weapon:sim.UNITS[source.type].w,energy:a.energy??1,damageScale:a.scale,launchOrigin:a.from??{x:source.x,z:source.z},ap:false,apMultiplier:1.5,apNaval:false,suppression:a.amount??10,suppressionScale:a.suppScale};
   if(a.op==='damage')result=H.flightDamage(g,p,g.units.get(a.target),a.at??{x:g.units.get(a.target).x,z:g.units.get(a.target).z},a.direction??{x:1,z:0});
   else if(a.op==='suppress'){let set=groups.get(a.group??0);if(!set)groups.set(a.group??0,set=new Set());p.suppressed=set;H.flightSuppress(g,p,g.units.get(a.target));}
   else if(a.op==='local'){result=H.localFlightDamage(g,p,{cell:a.c},{x:a.c%20*2+1,z:Math.floor(a.c/20)*2+1},a.direction??{x:1,z:0},a.contact);}
   else if(a.op==='blast')H.blast(g,[...g.units.values()],a.at,a.radius,a.support?sim.SUPPORT[a.support]:sim.UNITS[a.kind].w,a.owner??1,{contactId:a.contact,direction:a.direction});
   else if(a.op==='mutate')sim.mutateWorldCell(g,a.c,a.change);
   else if(a.op==='building'){const u=g.units.get(a.id);u.cells=a.cells;u.hp=a.hp;H.registerBuildingStructure(g,u);}
   else if(a.op==='sync')H.syncBuildingSections(g,g.units.get(a.id));
   else if(a.op==='prime'){const u=g.units.get(a.id);if(a.out!==undefined)g.players[a.id].out=a.out;if(a.hp!==undefined)u.hp=a.hp;if(a.xp!==undefined)u.xp=a.xp;if(a.hitFrom)u.hitFrom=a.hitFrom;if(a.lastHit!==undefined)u.lastHit=a.lastHit;}
   else if(a.op==='retire')H.retire(g,a.order.map(id=>g.units.get(id)).filter(Boolean));
   else if(a.op==='debris')H.stepWorldDebris(g,a.dt);
   else throw Error(a.op);
   results.push({value:result,shots:structuredClone(g.shots)});snapshots.push(structuredClone(snapshot(g)));
  }cases.push({name,map,units,actions,results,snapshots});
 }
 const random=Math.random;Math.random=()=>.5;
 record('damage-suppression-retained-shooter',blank(),[{id:1,type:'rifle',owner:0,x:5,z:5},{id:2,type:'rifle',owner:1,x:7,z:5,retarget:9},{id:3,type:'tank',owner:1,x:9,z:5}], [{op:'suppress',shooter:1,target:2,group:0,amount:30},{op:'suppress',shooter:1,target:2,group:0,amount:30},{op:'damage',shooter:1,target:2},{op:'damage',shooter:1,target:3},{op:'prime',id:1,hp:0},{op:'retire',order:[1,2,3]},{op:'damage',shooter:1,target:2}]);
 record('zero-damage-riders-and-missing-cargo',blank(),[{id:1,type:'rifle',owner:0,x:5,z:5},{id:2,type:'tank',owner:1,x:7,z:5,cargo:[3,999,3]},{id:3,type:'rifle',owner:1,x:7,z:5,carrier:2,path:[{x:11,z:5}]}],[{op:'damage',shooter:1,target:2,scale:0},{op:'damage',shooter:1,target:3,scale:0}]);
 {const m=blank();cell(m,42,'T');record('trench-retreat-and-friendly-suppression',m,[{id:1,type:'flamer',owner:0,x:7,z:5},{id:2,type:'rifle',owner:1,x:5,z:5},{id:3,type:'rifle',owner:0,x:9,z:5,retreating:true}],[{op:'damage',shooter:1,target:2},{op:'suppress',shooter:1,target:3},{op:'blast',kind:'tank',owner:0,at:{x:7,z:5},radius:5}]);}
 {const m=blank();record('building-local-sync-and-retire',m,[{id:1,type:'tank',owner:0,x:11,z:5},{id:2,type:'hq',owner:1,x:6,z:5}],[{op:'mutate',c:42,change:{object:'K'}},{op:'mutate',c:43,change:{object:'K'}},{op:'building',id:2,cells:[42,43],hp:300},{op:'local',shooter:1,c:42,contact:'round:1'},{op:'local',shooter:1,c:42,contact:'round:1'},{op:'prime',id:2,hp:40,hitFrom:{x:3,z:5},lastHit:0},{op:'sync',id:2},{op:'prime',id:2,hp:0},{op:'retire',order:[1,2]},{op:'debris',dt:2.5}]);}
 {const m=blank();cell(m,42,'H');cell(m,43,'B');record('terminal-demolition-and-bridge-independent-health',m,[{id:1,type:'tank',owner:0,x:5,z:5},{id:2,type:'rifle',owner:1,x:7,z:5},{id:3,type:'tank',owner:1,x:9,z:5}],[{op:'blast',kind:'tank',owner:0,at:{x:7,z:5},radius:5,contact:'blast:1',direction:{x:0,z:1}},{op:'retire',order:[1,2,3]},{op:'debris',dt:.5},{op:'debris',dt:2}]);}
 record('wreck-lifecycle-bounty-and-friendly-death',blank(),[{id:1,type:'tank',owner:0,x:5,z:5,hp:0,lastHit:1},{id:2,type:'tank',owner:1,x:5,z:5,hp:0,lastHit:1},{id:3,type:'rifle',owner:0,x:9,z:5,hp:0,lastHit:1},{id:4,type:'tank',owner:1,x:6,z:5}],[{op:'retire',order:[2,1,3,4]},{op:'debris',dt:.5},{op:'debris',dt:2}]);
 record('wreck-cap-and-source-order',blank(),Array.from({length:42},(_,i)=>({id:i+1,type:'tank',owner:i%2,x:3+(i%8)*4,z:3+Math.floor(i/8)*4,hp:0,lastHit:1-i%2})),[{op:'retire',order:Array.from({length:42},(_,i)=>i+1)},{op:'debris',dt:2.5}]);
 record('out-killer-no-bounty',blank(),[{id:1,type:'rifle',owner:0,x:5,z:5,hp:0,lastHit:1}], [{op:'prime',id:1,out:true},{op:'retire',order:[1]}]);
 record('expired-negative-sprint-remains-canonical',blank(),[{id:1,type:'rifle',owner:0,x:5,z:5,sprint:-.05},{id:2,type:'rifle',owner:1,x:7,z:5,sprint:-.05}], [{op:'suppress',shooter:1,target:2,group:0,amount:30},{op:'damage',shooter:1,target:2}]);
 Math.random=random;clean();const content=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases})+'\n';const path=join(root,'rust/crates/core/tests/fixtures/match-combat.json');if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),content);else writeFileSync(path,content);
 console.error(`${process.argv.includes('--check')?'Verified':'Recorded'} ${cases.length} match combat traces, ${cases.reduce((n,c)=>n+c.actions.length,0)} actions`);
}finally{rmSync(temp,{recursive:true,force:true});}
