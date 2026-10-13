// Record actual convoy path/handoff helpers, with observed terrain exported as input for native replay.
import assert from 'node:assert/strict';import {readFileSync,writeFileSync} from 'node:fs';import {execFileSync} from 'node:child_process';import {registerHooks} from 'node:module';import {fileURLToPath,pathToFileURL} from 'node:url';import {resolve} from 'node:path';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');const root=fileURLToPath(new URL('../',import.meta.url)),ref=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`),pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';assert.equal(execFileSync('git',['rev-parse','HEAD'],{cwd:ref,encoding:'utf8'}).trim(),pin);execFileSync('git',['diff','--exit-code',pin,'--','shared'],{cwd:ref});const simUrl=pathToFileURL(`${ref}/shared/sim.js`).href;registerHooks({load(url,c,next){const r=next(url,c);return url===simUrl?{...r,source:String(r.source)+'\nexport {fullConvoyRoute,supplyContinueRoute,supplyHandoff,knownTerrainFor,nearWalls,dangerViews};\n'}:r;}});const S=await import(simUrl);
const base=JSON.parse(readFileSync(`${root}/rust/crates/simtest/tests/fixtures/match-start.json`,'utf8'));const rle=v=>{const out=[];for(const n of v){if(out.length&&out.at(-1)[0]===n)out.at(-1)[1]++;else out.push([n,1]);}return out;};const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};const point=p=>[bits(p.x),bits(p.z)];
function terrain(g){const mask=1<<g.players[0].team;return{w:g.w,h:g.h,worldKnown:!!g.worldKnown,flags:rle(g.flags),height:g.height?rle(g.height):null,near:rle(g.worldNearWalls??S.nearWalls(g)),mines:[...g.mineSeen].filter(([c,n])=>(n&mask)&&g.mines?.get(c)!==g.players[0].team).map(([c])=>c),fires:[...g.fires.keys()],bunkers:[...g.units.values()].filter(u=>u.type==='bunker'&&u.hp>0).map(point),wear:g.wear?rle(new Uint32Array(g.wear.buffer,g.wear.byteOffset,g.wear.length)):null,wet:g.wx?.wet??0,roads:!!g.roads,version:g.navigationKnowledgeVersion??0,infantry:g.infantryRegionVersion??0,vehicle:g.vehicleRegionVersion??0,naval:g.navalRegionVersion??0,obstruction:g.obstructionVersion??0};}
const cases=[];
for(let m=0;m<base.maps.length;m++){
 const old=Math.random;Math.random=()=>.5;let g;try{g=S.createGame(base.maps[m].map,['A','B'],false,[0,1],[0,1],{mode:'conquest',weather:false,logistics:true});}finally{Math.random=old;}
 const first=[...g.units.values()].find(u=>u.owner===0&&!S.UNITS[u.type].structure);const u={...first,id:900,type:'truck',path:[],stuck:0};g.units.set(u.id,u);
 const targets=[g.players[1].spawn,{x:g.w,z:g.h},{x:u.x+8,z:u.z},g.points[0]?{x:g.points[0].x,z:g.points[0].z}:g.players[1].spawn];
 const queries=[];
 for(let i=0;i<targets.length;i++)for(const safe of [false,true]){
  g.tick=i*40;g.players[0].visible=new Set();u.supplyRouteReason=null;
  if(safe&&i===1){const t={id:999,type:'rifle',owner:1,hp:100,x:targets[i].x,z:targets[i].z,air:null};g.units.set(t.id,t);g.players[0].visible.add(t.id);}
  const known=S.knownTerrainFor(g,0),input=terrain(known);const threats=[...g.players[0].visible].map(id=>g.units.get(id)).filter(t=>t.hp>0&&t.owner!==0&&!t.air&&S.UNITS[t.type].w?.range>0).map(t=>({at:{x:t.x,z:t.z},range:S.UNITS[t.type].w.range}));
  const before=u.routeEnd;const route=S.fullConvoyRoute(g,u,targets[i],safe);u.path=structuredClone(route);const continuation=S.supplyContinueRoute(g,u);u.stuck=1;const stuck=S.supplyContinueRoute(g,u);u.stuck=0;
  queries.push({tick:g.tick,from:{x:u.x,z:u.z},to:targets[i],safe,input,threats,routeEnd:u.routeEnd!==before?point(u.routeEnd):null,path:route.map(point),failure:u.supplyRouteReason??null,continuation,stuck});
 }
 // Reuse one danger view for multiple trucks, then record a visible terrain edit.
 const threat={id:1001,type:'rifle',owner:1,hp:100,x:(u.x+g.players[1].spawn.x)/2,z:(u.z+g.players[1].spawn.z)/2,air:null};g.units.set(threat.id,threat);g.players[0].visible=new Set([threat.id]);
 const original={x:u.x,z:u.z};
 for(let k=0;k<4;k++) {
  g.tick=400+k;u.x=original.x+k*2;u.z=original.z;
  if(k===2){const cell=Math.floor(original.z/2)*g.w+Math.floor(original.x/2)+3;S.internals.setCell(g,cell,g.chars[cell]==='.'?'F':'.');}
  u.supplyRouteReason=null;const known=S.knownTerrainFor(g,0),input=terrain(known),threats=[{at:{x:threat.x,z:threat.z},range:S.UNITS.rifle.w.range}];
  const before=u.routeEnd;const to=g.players[1].spawn,route=S.fullConvoyRoute(g,u,to,true);u.path=structuredClone(route);const continuation=S.supplyContinueRoute(g,u);u.stuck=1;const stuck=S.supplyContinueRoute(g,u);u.stuck=0;
  const d=S.dangerViews.get(g)?.get(0);queries.push({tick:g.tick,from:{x:u.x,z:u.z},to,safe:true,input,threats,routeEnd:u.routeEnd!==before?point(u.routeEnd):null,path:route.map(point),failure:u.supplyRouteReason??null,continuation,stuck,danger:d?{version:d.version,history:d.changes.length,changed:d.changes.reduce((n,v)=>n+v.cells.length,0)}:null});
  // An ordinary search must retain the previous danger graph for the next truck.
  if(k===0){u.supplyRouteReason=null;const before=u.routeEnd,route=S.fullConvoyRoute(g,u,to,false);u.path=structuredClone(route);const continuation=S.supplyContinueRoute(g,u);u.stuck=1;const stuck=S.supplyContinueRoute(g,u);u.stuck=0;queries.push({tick:g.tick,from:{x:u.x,z:u.z},to,safe:false,input,threats,routeEnd:u.routeEnd!==before?point(u.routeEnd):null,path:route.map(point),failure:u.supplyRouteReason??null,continuation,stuck});}

 }
 const handoffs=[];const store=[...g.units.values()].find(u=>u.type==='hq'&&u.owner===0)??{...g.players[0].spawn,cells:[]};
 const physical={...g,navigationObserved:true,worldKnown:true,worldNearWalls:S.nearWalls(g)};
 for(const radius of [0,15,30])for(const at of [first,{...first,x:store.x+10,z:store.z},{...first,x:store.x+10.01,z:store.z}]){g.tick=200;const target={x:store.x,z:store.z,entity:store.id};handoffs.push({tick:g.tick,start:{x:at.x,z:at.z},store:{x:store.x,z:store.z},cells:store.cells??[],radius,input:terrain(physical),answer:S.supplyHandoff(g,at,target,radius)});}
 cases.push({map:m,queries,handoffs});
}
const terrains=[],indices=new Map();for(const c of cases)for(const q of [...c.queries,...c.handoffs]){const key=JSON.stringify(q.input);let index=indices.get(key);if(index===undefined){index=terrains.length;terrains.push(q.input);indices.set(key,index);}q.input=index;}
const data=JSON.stringify({reference:pin,terrains,cases})+'\n',path=`${root}/rust/crates/core/tests/fixtures/supply-routes.json`;if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),data);else writeFileSync(path,data);console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} ${cases.reduce((n,c)=>n+c.queries.length,0)} real convoy routes and ${cases.reduce((n,c)=>n+c.handoffs.length,0)} physical handoffs`);
