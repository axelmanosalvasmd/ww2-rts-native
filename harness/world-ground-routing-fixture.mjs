// Original World ground queries over explicit restored state. This is not a whole tick replay.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
const root=fileURLToPath(new URL('../',import.meta.url));
const ref=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',args,{cwd:ref,encoding:'utf8'}).trim();
const clean=()=>{if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Pinned tracked-clean reference required');};
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node24.21.0');
if(process.argv.slice(2).some(v=>v!=='--check'))throw Error('Only --check is supported');
clean();
registerHooks({load(url,context,next){const out=next(url,context);if(url!==pathToFileURL(`${ref}/shared/sim.js`).href)return out;return {...out,source:String(out.source)+'\nexport {marchMul as __marchMul,homeOf as __homeOf,coverRank as __coverRank,knownTerrainFor as __knownTerrainFor,beginPathTick as __beginPathTick,requestStepPath as __requestStepPath,performStepPath as __performStepPath,pathFailures as __pathFailures,touch as __touch,walkRoute as __walkRoute,speedMul as __speedMul};\n'};}});
const sim=await import(pathToFileURL(`${ref}/shared/sim.js`));
const {trafficStep}=await import(pathToFileURL(`${ref}/shared/local-traffic.js`));
const {rebuildGrid}=await import(pathToFileURL(`${ref}/shared/grid.js`));
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const definitions=Array.from({length:64},(_,i)=>({id:i*2-100,x:i%8*64+32,y:Math.floor(i/8)*64+32,kind:['rural','city','industrial','resource'][i%4]}));
const at=i=>({x:(definitions[i].x+.5)*sim.CELL,z:(definitions[i].y+.5)*sim.CELL});
const unit=(id,owner,type,region,extra={})=>({id,owner,type,...at(region),hp:sim.UNITS[type].models*sim.UNITS[type].hpPer,built:1,garrison:-1,riding:0,path:[],stuck:0,trafficWait:0,hitAt:-1e9,attackId:0,targetId:0,...extra});
function game({teams=[5,1,5],regionTeams={},units=[],tick=400,neutralSpawn={x:11,z:13},reveal=false}={}){
 const players=teams.map((team,slot)=>({slot,team,spawn:at(slot===1?63:0),visible:new Set()}));
 Object.defineProperty(players,'-1',{value:{slot:-1,team:-1,spawn:neutralSpawn,visible:new Set()},configurable:true});
 return {w:512,h:512,tick,visionTick:0,terrainVersion:0,reveal,mode:{kind:'world'},players,units:new Map(units.map(u=>[u.id,structuredClone(u)])),world:{size:'huge',total:64,regionCells:64,regions:definitions.map((r,i)=>({id:r.id,...at(i),team:regionTeams[i]??(i===0||i===20?teams[0]:i===63?teams[1]:-1)})),memory:new Map()},wrecks:[],fires:new Map()};
}
const march=[],home=[],cover=[],visibility=[],traffic=[];
function marchCase(name,input){const g=game(input),u=structuredClone(input.unit);march.push({name,input,expectedBits:bits(sim.__marchMul(g,u,sim.UNITS[u.type]))});}
const calm=sim.CFG.march.calm/sim.TICK;
const marcher=unit(7,0,'rifle',20);
marchCase('calm infantry on owned region',{unit:marcher});
marchCase('calm vehicle on owned region',{unit:unit(7,0,'tank',20)});
marchCase('allied commander shares region team',{unit:unit(7,2,'rifle',20)});
marchCase('enemy region has no march bonus',{unit:unit(7,0,'rifle',63)});
marchCase('neutral region has no march bonus',{unit:unit(7,0,'rifle',19)});
marchCase('neutral guard on neutral land receives march bonus',{unit:unit(7,-1,'rifle',19)});
marchCase('neutral guard on commander land has no march bonus',{unit:unit(7,-1,'rifle',20)});
for(const fact of ['hitAt','shotAt'])for(const delta of [-1,0,1])marchCase(`${fact} calm boundary ${delta}`,{unit:{...marcher,[fact]:400-calm-delta}});
for(const field of ['attackId','targetId'])marchCase(`${field} excludes march`,{unit:{...marcher,[field]:9}});
marchCase('naval unit excludes march',{unit:unit(7,0,'gunboat',20)});
marchCase('air unit excludes march',{unit:unit(7,0,'fighter',20)});
marchCase('outside world excludes march',{unit:{...marcher,x:-1}});
function homeCase(name,input){const g=game(input),u=structuredClone(input.unit),v=sim.__homeOf(g,u);const kind=v===u?'self':v===u.guardHome?'guard':g.world.regions.includes(v)?'region':g.units.get(v.id)===v?'unit':'spawn';home.push({name,input,expected:{kind,...(v.id===undefined?{}:{id:v.id}),xBits:bits(v.x),zBits:bits(v.z)}});}
const h=unit(7,0,'rifle',20);
const b=(id,owner,region,extra={})=>unit(id,owner,'barracks',region,extra);
homeCase('nearest own finished producing building',{unit:h,units:[b(90,0,0),b(31,0,20,{x:h.x+4})]});
homeCase('equal buildings retain insertion order',{unit:h,units:[b(90,0,20,{x:h.x-4}),b(31,0,20,{x:h.x+4})]});
homeCase('reversed equal buildings retain insertion order',{unit:h,units:[b(31,0,20,{x:h.x+4}),b(90,0,20,{x:h.x-4})]});
homeCase('reject dead unfinished foreign allied and enemy region buildings',{unit:h,units:[b(90,0,20,{hp:0}),b(31,0,20,{built:.999}),b(12,2,20),b(50,0,63),unit(51,0,'depot',20),b(80,0,0)]});
homeCase('fallback uses actual unit pose instead of spawn',{unit:h,units:[]});
homeCase('fallback keeps first equally distant region',{unit:{...h,x:(at(0).x+at(20).x)/2,z:(at(0).z+at(20).z)/2},units:[]});
homeCase('no owned region returns unit pose',{unit:h,regionTeams:{0:-1,20:-1},units:[]});
homeCase('neutral guard returns guardHome',{unit:unit(7,-1,'rifle',19,{guardHome:{x:123,z:127}}),units:[b(90,-1,19)]});
homeCase('neutral without guardHome returns internal actor spawn',{unit:unit(7,-1,'rifle',19),neutralSpawn:{x:29,z:31},units:[]});
// Restore accepted terrain rows and the matching cache key. The original path-view body
// projects remembered cover and its unknown baseline without reading authoritative cells.
function coverCase(name,input){const g=game(),N=g.w*g.h,slot=input.slot??0,team=g.players[slot].team,rows=input.rows??[];
 g.house=new Uint8Array(N);
 g.world.memory.set(team,{key:'0:0:0',version:rows.length,cells:new Map(rows.map(r=>[r[0],r])),regions:new Map(),wrecks:new Map((input.wrecks??[]).map(w=>[w.id,[w.id,w.type,w.owner,w.x,w.z,w.rot??0,...(w.motion?[w.motion]:[])]])),log:rows.map(r=>r[0]),logBase:0});
 g.fog=new Map([[team,{key:0,vis:new Uint8Array(N),visibleCells:[]}]]);
 const view=sim.__knownTerrainFor(g,slot),c=Math.floor(input.at.z/sim.CELL)*g.w+Math.floor(input.at.x/sim.CELL),rank=sim.__coverRank(view,input.at,input.from);
 cover.push({name,input,expected:{rank,bits:bits(rank),knownAt:g.world.memory.get(team).cells.has(c),flagsAt:view.flags[c]}});
}
const spot={x:17,z:17},from={x:27,z:17},c=8*512+8;
coverCase('unknown open baseline',{at:spot,from,rows:[]});
coverCase('unknown spot behind remembered wall',{at:spot,from,rows:[[c+1,'#',0,0]]});
coverCase('unknown spot flank of remembered wall',{at:spot,from:{x:17,z:27},rows:[[c+1,'#',0,0]]});
coverCase('unknown spot behind remembered house corner',{at:spot,from,rows:[[c+513,'B',0,0]]});
coverCase('unknown spot behind remembered stationary wreck',{at:spot,from,rows:[],wrecks:[{id:90,type:'tank',owner:1,x:19,z:17}]});
coverCase('unseen moving wreck memory becomes stationary cover',{at:spot,from,rows:[],wrecks:[{id:90,type:'tank',owner:1,x:19,z:17,motion:{settled:false}}]});
coverCase('unknown no shooter ignores directional cover',{at:spot,from:null,rows:[[c+1,'#',0,0]],wrecks:[{id:90,type:'tank',owner:1,x:19,z:17}]});
coverCase('remembered cover cell',{at:spot,from,rows:[[c,'H',0,0]]});
coverCase('remembered trench faces shooter',{at:spot,from,rows:[[c,'T',0,0,{trenchFront:0}]]});
coverCase('remembered trench opposite shooter',{at:spot,from:{x:7,z:17},rows:[[c,'T',0,0,{trenchFront:0}]]});
coverCase('neutral known view uses same unknown baseline',{slot:-1,at:spot,from,rows:[[c+1,'#',0,0]]});
function visibleCase(name,input){const g=game({teams:input.teams,reveal:input.reveal,units:input.unit?[input.unit]:[]});g.players[input.slot].visible=new Set(input.visible??[]);visibility.push({name,input,expected:sim.seenBy(g,input.slot,input.unit?.id??999)});}
visibleCase('neutral actor sees visible enemy',{slot:-1,visible:[7],unit:unit(7,0,'rifle',20)});
visibleCase('neutral actor hides unseen enemy',{slot:-1,unit:unit(7,0,'rifle',20)});
visibleCase('neutral actor sees neutral ally',{slot:-1,unit:unit(7,-1,'rifle',19)});
visibleCase('commander hides unseen neutral actor',{slot:0,unit:unit(7,-1,'rifle',19)});
visibleCase('commander sees visible neutral actor',{slot:0,visible:[7],unit:unit(7,-1,'rifle',19)});
visibleCase('shared team sees allied actor',{slot:0,unit:unit(7,2,'rifle',20)});
visibleCase('neutral reveal sees enemy',{slot:-1,reveal:true,unit:unit(7,0,'rifle',20)});
for(const state of ['base','out','station','home'])visibleCase(`neutral visible plane ${state}`,{slot:-1,visible:[7],unit:unit(7,0,'fighter',20,{air:{state}})});
visibleCase('reveal shows grounded plane',{slot:-1,reveal:true,unit:unit(7,0,'fighter',20,{air:{state:'base'}})});
visibleCase('neutral nonexistent id follows allied fallback',{slot:-1});
visibleCase('commander nonexistent id is unseen',{slot:0});
// These first-contact calls stop before the terrain and cover search begins. Fail if
// a fixture starts exercising those callbacks instead of silently substituting rules.
function trafficCase(name,input){const g=game({units:[input.unit,input.other],reveal:input.reveal}),u=g.units.get(input.unit.id);g.players[u.owner].visible=new Set(input.visible??[]);
 const unexpected=()=>{throw Error('First-contact scene unexpectedly requested terrain');};
 const out=trafficStep(g,u,u.path[0],{defs:sim.UNITS,grid:rebuildGrid(g),dt:sim.TICK,visible:v=>sim.seenBy(g,u.owner,v.id),friend:v=>sim.allied(g,u.owner,v.owner),clear:unexpected,coverRank:unexpected});
 traffic.push({name,input,expected:{goal:out.goal?[bits(out.goal.x),bits(out.goal.z)]:null,blocked:out.blocked,waiting:!!out.waiting,queuedBehind:u.queuedBehind,waitBits:bits(u.trafficWait)}});
}
const mover=unit(7,-1,'rifle',19,{x:17,z:17,path:[{x:27,z:17}]}),obstacle=unit(90,0,'tank',19,{x:19,z:17});
trafficCase('neutral traffic ignores unseen enemy hull',{unit:mover,other:obstacle});
trafficCase('neutral traffic stops for visible enemy hull',{unit:mover,other:obstacle,visible:[90]});
trafficCase('neutral reveal traffic stops for enemy hull',{unit:mover,other:obstacle,reveal:true});
trafficCase('neutral traffic queues behind neutral moving hull',{unit:mover,other:{...obstacle,owner:-1,path:[{x:29,z:17}]}});
trafficCase('commander traffic queues behind allied moving hull',{unit:{...mover,owner:0},other:{...obstacle,owner:2,path:[{x:29,z:17}]}});
trafficCase('neutral traffic excludes visible airborne actor',{unit:mover,other:{...obstacle,type:'fighter',air:{state:'out'}},visible:[90]});
// Route histories start with explicit physical state and supplied vision masks.
// Discovery, row identity, journal reads and graph versions remain the original bodies.
const routing=[],scheduler=[];
function physicalGame(input){const g=game({units:[input.unit]}),N=g.w*g.h;
 Object.assign(g,{chars:Array(N).fill('.'),ground:Array(N).fill('.'),objects:Array(N).fill('.'),flags:new Uint16Array(N).fill(sim.TERRAIN['.']),height:new Int8Array(N),house:new Uint8Array(N),cellState:new Uint8Array(N),cellHp:new Float32Array(N),wear:new Float32Array(N),burnt:new Uint8Array(N),scar:new Float32Array(N),trenchFront:new Float32Array(N).fill(NaN),mineLayer:new Uint8Array(N),mines:new Map(),mineSeen:new Map(),cellLog:[],newCells:[],cellJournal:[],journalBase:0});
 g.initialTerrain={chars:[...g.chars],ground:[...g.ground],objects:[...g.objects],height:new Int8Array(N)};
 setVision(g,input.slot,input.visibleCells??[]);
 for(const edit of input.initialEdits??[])if(!sim.mutateWorldCell(g,edit.c,edit.change))throw Error('Invalid initial edit');
 return g;
}
function setVision(g,slot,cells){const vis=new Uint8Array(g.w*g.h);for(const c of cells)vis[c]=1;
 (g.fog??=new Map()).set(g.players[slot].team,{key:g.visionTick,vis,visibleCells:[...cells]});
}
function routeCapture(g,u,goal,action){const path=sim.findPath(g,u,goal),view=sim.__knownTerrainFor(g,u.owner),m=g.world.memory.get(g.players[u.owner].team);
 return {action,path:path.map(p=>[bits(p.x),bits(p.z)]),routeEnd:u.routeEnd?[bits(u.routeEnd.x),bits(u.routeEnd.z)]:null,memoryVersion:m.version,navigationVersion:view.navigationKnowledgeVersion,knownRows:[...m.cells.values()]};
}
function routeCase(name,input,actions){const g=physicalGame(input),u=g.units.get(input.unit.id),captures=[];
 for(let i=0;i<actions.length;i++){const a=actions[i];
  if(a.op==='mutate'){if(!sim.mutateWorldCell(g,a.c,a.change))throw Error('Invalid edit');}
  else if(a.op==='visibility'){g.visionTick++;setVision(g,input.slot,a.cells);}
  else if(a.op==='wear'){g.wear[a.c]=a.value;sim.__touch(g,a.c);g.visionTick++;setVision(g,input.slot,a.cells);}
  else if(a.op!=='read')throw Error(a.op);
  captures.push(routeCapture(g,u,input.goal,i));
 }
 routing.push({name,input,actions,captures});
}
const known=[c,c+6],allKnown=[];for(let y=7;y<=9;y++)for(let x=7;x<=15;x++)allKnown.push(y*512+x);
for(const slot of [0,-1])routeCase(slot===0?'commander route memory history':'neutral route memory history',{
 slot,unit:unit(7,slot,'rifle',19,{x:17,z:17}),goal:{x:29,z:17},visibleCells:known},[
 {op:'read'},{op:'read'},{op:'mutate',c:c+3,change:{object:'K'}},
 {op:'visibility',cells:allKnown},{op:'read'},
 {op:'wear',c:c+6,value:.25,cells:allKnown},
 {op:'mutate',c:c+3,change:{object:'.'}},{op:'read'}]);
routeCase('commander vehicle route memory history',{
 slot:0,unit:unit(7,0,'tank',19,{x:17,z:17}),goal:{x:35,z:17},visibleCells:known},[
 {op:'read'},{op:'read'},{op:'mutate',c:c+3,change:{object:'K'}},
 {op:'visibility',cells:allKnown},{op:'read'},
 {op:'wear',c:c+6,value:.25,cells:allKnown},
 {op:'mutate',c:c+3,change:{object:'.'}},{op:'read'}]);
function schedulerCase(){const ring=[];for(let y=7;y<=9;y++)for(let x=7;x<=9;x++)if(x!==8||y!==8)ring.push(y*512+x);
 const input={slot:0,unit:unit(7,0,'rifle',19,{x:17,z:17,worldGoal:{x:29,z:17}}),goal:{x:29,z:17},visibleCells:[c,c+6,...ring],initialEdits:ring.map(c=>({c,change:{object:'K'}})),pathRetryLimit:3},g=physicalGame(input),u=g.units.get(7),captures=[];
 g.pathRetryLimit=input.pathRetryLimit;
 for(let i=0;i<5;i++){
  let mutation=null;
  if(i===2){mutation={c:c+6,wear:.25};g.wear[mutation.c]=mutation.wear;sim.__touch(g,mutation.c);g.visionTick++;setVision(g,input.slot,input.visibleCells);}
  sim.__beginPathTick(g);sim.__requestStepPath(g,u,input.goal,'world');
  const view=sim.__knownTerrainFor(g,0),m=g.world.memory.get(g.players[0].team),failure=sim.__pathFailures.get(u);
  captures.push({attempt:i+1,mutation,memoryVersion:m.version,navigationVersion:view.navigationKnowledgeVersion,failure:failure?{...failure}:null,path:u.path.map(p=>[bits(p.x),bits(p.z)]),worldGoal:u.worldGoal?[bits(u.worldGoal.x),bits(u.worldGoal.z)]:null,moveOutcome:u.moveOutcome??null,stats:{...g.pathStats},knownRows:[...m.cells.values()]});
  g.tick++;
 }
 scheduler.push({name:'accepted wear resets failed route count without graph version change',input,captures});
}
schedulerCase();
// One original infantry movement call per scene, with no other traffic or tick phases.
const movement=[];
function movementCase(name,input){const g=physicalGame(input),u=g.units.get(input.unit.id),def=sim.UNITS[u.type];let readCount=0,cachedView;
 const trafficView=()=>{readCount++;return cachedView=sim.__knownTerrainFor(g,u.owner);};
 const speed=def.speed*sim.__speedMul(g,u)*sim.__marchMul(g,u,def);
 sim.__walkRoute(g,u,def,u.path,speed,sim.TICK,false,{yielding:false},trafficView);
 const m=g.world.memory.get(g.players[u.owner].team),cachedUnit=cachedView?.units.get(u.id);
 movement.push({name,input,expected:{readCount,speedBits:bits(speed),xBits:bits(u.x),zBits:bits(u.z),rotBits:bits(u.rot),path:u.path.map(p=>[bits(p.x),bits(p.z)]),memoryVersion:m?.version??null,navigationVersion:m?.path?.navigationKnowledgeVersion??null,cachedUnitPose:cachedUnit?{xBits:bits(cachedUnit.x),zBits:bits(cachedUnit.z),rotBits:bits(cachedUnit.rot)}:null}});
}
movementCase('commander single waypoint preserves deferred terrain read',{
 slot:0,unit:unit(7,0,'rifle',20,{x:17,z:17,rot:0,path:[{x:27,z:17}]}),visibleCells:[c,c+5]});
movementCase('neutral near waypoint uses remembered clear skip',{
 slot:-1,unit:unit(7,-1,'rifle',19,{x:17,z:17,rot:0,path:[{x:17.1,z:17},{x:21,z:17},{x:27,z:17}]}),visibleCells:[c,c+2,c+5]});
clean();
const output=JSON.stringify({format:1,reference:pin,node:process.version,arch:process.arch,definitions,march,home,cover,visibility,traffic,routing,scheduler,movement})+'\n',path=`${root}/rust/crates/simtest/tests/fixtures/world-ground-routing.json`;
if(process.argv.includes('--check')){if(readFileSync(path,'utf8')!==output)throw Error('World ground routing fixture differs');}else writeFileSync(path,output);
console.log(`Verified original World queries: ${march.length} march, ${home.length} home, ${cover.length} cover, ${visibility.length} visibility, ${traffic.length} traffic, ${routing.length} route histories, ${scheduler.length} scheduler history, ${movement.length} movement`);
