// Run the pinned command body and its queued-order/path phases on real game state.
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
registerHooks({load(url,context,next){const out=next(url,context);if(url!==pathToFileURL(resolve(reference,'shared/sim.js')).href)return out;
 const source=String(out.source);
 const preBegin='    if (u.worldGoal && (u.retreating',preEnd='    if (def.building)';
 const amoveBegin='    // attack-move: halt while something is in range',amoveEnd='    // shelling a structure:';
 const replanBegin='    const stalledRoute =',replanEnd='    const route = traffic.goal ?';
 for(const marker of [preBegin,preEnd,amoveBegin,amoveEnd,replanBegin,replanEnd])if(source.split(marker).length!==2)throw Error('Phase anchors changed');
 const pre=source.slice(source.indexOf(preBegin),source.indexOf(preEnd));
 const amove=source.slice(source.indexOf(amoveBegin),source.indexOf(amoveEnd));
 const replan=source.slice(source.indexOf(replanBegin),source.indexOf(replanEnd));
 return {...out,source:source+'\nexport {placeUnit as __placeUnit,beginPathTick as __beginPathTick,startQueuedOrders as __startQueuedOrders,endSpots as __endSpots,observedPathView as __observedPathView,setCell as __setCell};\nexport function __commandPre(g,u){const def=UNITS[u.type];\n'+pre+'}\nexport function __amove(g,u){\n'+amove+'}\nexport function __replan(g,u,shouldReplan){const def=UNITS[u.type],groundVehicle=isGroundVehicle(def),traffic={blocked:false,yielding:false,waiting:false,replan:shouldReplan};\n'+replan+'}\n'};
}});
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const bitsBuffer=new DataView(new ArrayBuffer(8));const bits=n=>{bitsBuffer.setFloat64(0,n);return bitsBuffer.getBigUint64(0).toString(16).padStart(16,'0');};
const pt=p=>p?[bits(p.x),bits(p.z)]:null;
const snapshot=g=>[...g.units.values()].map(u=>({id:u.id,x:bits(u.x),z:bits(u.z),rot:bits(u.rot),aim:bits(u.aim),path:u.path.map(pt),reverse:pt(u.reverse),pace:bits(u.pace??NaN),worldGoal:pt(u.worldGoal),amove:pt(u.amove),attackId:u.attackId,targetId:u.targetId,follow:u.follow??0,holdFire:!!u.holdFire,holdPos:!!u.holdPos,auto:!!u.auto,autoRetreat:!!u.autoRetreat,orders:(u.orders??[]).map(o=>({t:o.t,at:pt(o),face:o.face===undefined?null:bits(o.face)})),face:u.face?{angle:bits(u.face.a),at:pt(u.face)}:null,retreating:!!u.retreating,stuck:bits(u.stuck),repath:bits(u.repath),drift:!!u.drift,garrison:u.garrison,nade:pt(u.nade),enter:u.enter,fireAt:u.fireAt,board:u.board,build:!!u.build,dig:!!u.dig,entrench:!!u.entrench,traffic:!!u.traffic,trafficWait:bits(u.trafficWait??0),trafficEntered:!!u.trafficEntered,trail:(u.trafficTrail??[]).length,outcome:u.moveOutcome??null,outcomeTick:u.moveOutcomeTick??null,routeEnd:pt(u.routeEnd),supplyHold:!!u.supplyHold,supplyEscape:!!u.supplyEscape,forced:u.logistics?{forced:!!u.logistics.forced,stranded:!!u.logistics.stranded}:null}));
const makeMap=(name,rows,height)=>({name,w:rows[0].length,h:rows.length,rows,spawns:[{x:0,y:0},{x:rows[0].length-1,y:rows.length-1}],points:[{id:'p',x:12,y:12}],...(height?{heights:height}:{})});
const flat=()=>Array(24).fill('.'.repeat(32));
const cover=flat();cover[11]='..........TTT...HH...OO.........';cover[12]='..........TTT...HH...OO.........';
const wall=flat().map((r,y)=>r.slice(0,16)+(y<8||y>11?'B':'.')+r.slice(17));
const cliff=flat();const heights=cliff.map((r,y)=>[...r].map((_,x)=>x>15?'2':'0').join('')); 
const maps=[makeMap('flat',flat()),makeMap('cover',cover),makeMap('wall',wall),makeMap('cliff',cliff,heights)];
const cases=[];
function trace(name,map,initial,actions){
 if(sim.validateMap(map)!==null)throw Error(`Invalid fixture map: ${name}`);
 const save=Math.random;Math.random=()=>.5;let g,players;try {g=sim.createGame(map,[],false,[],[],{weather:false});players=sim.createGame(map,['A','B'],false,[0,1],[0,1],{weather:false}).players;}finally{Math.random=save;}
 g.players=players;g.units=new Map();g.nextId=1;g.tick=1;g.pathBudget=4096;g.winner=null;g.supply=false;g.logistics=false;
 for(const row of initial){const u=sim.internals.spawnUnit(g,row.owner??0,row.kind);Object.assign(u,{x:row.x,z:row.z,...row.fields});sim.__placeUnit(g,u,{x:u.x,z:u.z});}
 for(const p of g.players)p.visible=new Set([...g.units.values()].filter(u=>u.owner!==p.slot).map(u=>u.id));
 const steps=[];
 for(const action of actions){g.tick=action.tick??g.tick;g.winner=action.winner?0:null;
  for(const [id,fields]of action.set??[]){const u=g.units.get(id);Object.assign(u,fields);sim.__placeUnit(g,u,{x:u.x,z:u.z});}
  for(const [cell,ch]of action.cells??[])sim.__setCell(g,cell,ch);
  for(const [slot,fields]of action.players??[])Object.assign(g.players[slot],fields);
  let denial=null;
  if(action.command)denial=sim.command(g,action.owner??0,action.command)??null;
  if(action.begin){g.pathBudget=action.budget??4096;g.pathRetryLimit=action.retryLimit??20;sim.__beginPathTick(g);}
  if(action.pre)sim.__commandPre(g,g.units.get(action.pre));
  if(action.amove)sim.__amove(g,g.units.get(action.amove));
  steps.push({action,denial,expected:snapshot(g)});
 }
 cases.push({name,map,initial,steps});
}
const base=[{kind:'rifle',x:9,z:23},{kind:'mg',x:11,z:25},{kind:'tank',x:13,z:23},{kind:'rifle',owner:1,x:53,z:23}];
for(const map of maps)trace(map.name,map,base,[
 {command:{t:'move',orders:[[1,35,23],[2,35,23],[3,35,23]],together:true}},
 {command:{t:'stop',ids:[1,3]}},
 {command:{t:'stance',ids:[1,2],key:'holdPos',on:true}},
 {command:{t:'amove',orders:[[1,25,25],[2,25,25]],face:Math.PI/3}},
 {command:{t:'stance',ids:[1],key:'holdFire',on:true}},
 {command:{t:'stance',ids:[1],key:'autoRetreat',on:false}},
 {command:{t:'move',orders:[[1,40,30],[2,40,30]],queue:true,face:-Math.PI/2}},
 {command:{t:'move',orders:[[1,25,19],[2,37,19]],queue:true,face:0}},
 {set:[[1,{path:[],worldGoal:null,amove:null,drift:'cover'}]],pre:1},
 {command:{t:'move',orders:[[1,-20,70],[4,20,20],[-1,10,10]]}},
 {command:{t:'stop',ids:[4]}},
 {command:{t:'stance',ids:[1],key:'invalid',on:true}},
 {command:{t:'move',orders:[[1,19,19],[2,'bad',23]]}},
 {command:{t:'move',orders:[...Array.from({length:51},()=>[1,20,20]),[1,'bad',20]]}},
 {command:{t:'move',orders:[[1,19,19]],face:null}},
 {winner:true,command:{t:'stop',ids:[1]}},
 {players:[[0,{away:true}]],command:{t:'stop',ids:[1]}},
 {players:[[0,{away:false}]],command:{t:'stop',ids:[1]}},
]);
trace('queues-and-completion',maps[0],base,[
 ...Array.from({length:9},(_,i)=>({command:{t:'move',orders:[[1,20+i,20]],queue:true}})),
 {pre:1},
 {set:[[1,{x:20,z:20,path:[],moveSpeed:0}]],pre:1},
 {set:[[1,{path:[],worldGoal:null,drift:'rally'}]],pre:1},
 {set:[[1,{worldGoal:null,path:[{x:30,z:30}],amove:null}]],command:{t:'stop',ids:[1]}},
 {command:{t:'move',orders:[[1,30,30],[1,30,30]]}},
]);
trace('forced-and-permission',maps[0],base,[
 {set:[[1,{follow:91,logistics:{forced:true,stranded:false}}]],command:{t:'stance',ids:[1],key:'holdPos',on:true}},
 {command:{t:'amove',orders:[[1,39,23]]}},
 {command:{t:'stop',ids:[1]}},
 {command:{t:'move',ids:null,orders:[[1,19,23]]}},
 {command:{t:'move',ids:[1],orders:[[4,19,23]]}},
 {set:[[1,{operative:'test'}]],command:{t:'stop',ids:[1]}},
 {set:[[1,{operative:null,riding:3}]],command:{t:'stop',ids:[1]}},
]);
trace('amove-and-retry',maps[0],base,[
 {command:{t:'amove',orders:[[1,50,23]]}},
 {begin:true,set:[[1,{path:[],targetId:4,x:45,z:23,repath:0}]],amove:1},
 {set:[[1,{targetId:0,repath:0}]],amove:1},
 {set:[[1,{x:49,z:23,path:[]}]],amove:1},
 {command:{t:'move',orders:[[1,35,23]]}},
 {set:[[1,{path:[],repath:0}]],pre:1},
]);
trace('massive-selection',maps[0],[...Array.from({length:64},(_,i)=>({kind:'rifle',x:3+(i%8)*2,z:3+Math.floor(i/8)*2,fields:{holdPos:true}})),{kind:'rifle',owner:1,x:61,z:45}],[
 {command:{t:'move',orders:Array.from({length:64},(_,i)=>[i+1,55,35]),together:true}},
 {command:{t:'stop',ids:Array.from({length:64},(_,i)=>i+1)}},
]);
trace('budgeted-world-retry',maps[0],base,[
 {command:{t:'move',orders:[[1,37,25],[2,39,25]]}},
 {begin:true,budget:0,set:[[1,{path:[],repath:0}],[2,{path:[],repath:0}]],pre:1},
 {pre:2},
 {tick:2,begin:true,budget:1},
 {command:{t:'stop',ids:[2]}},
 {tick:3,begin:true,budget:1},
]);
trace('garrison-and-job-clearing',maps[2],base,[
 {set:[[1,{x:33,z:9,garrison:144,targetId:4,attackId:4,follow:91,nade:{x:20,z:20},dig:{x:20,z:20},enter:33,fireAt:3,board:3,build:9,entrench:{jobs:[{x:22,z:22}],active:0}}]],command:{t:'move',orders:[[1,25,13]]}},
 {set:[[1,{targetId:4,attackId:4,nade:{x:20,z:20},dig:{x:20,z:20},enter:33,fireAt:3,board:3,build:9,entrench:{jobs:[{x:22,z:22}],active:0}}]],command:{t:'stop',ids:[1]}},
]);
trace('reverse-and-hit-memory',maps[0],[{kind:'tank',x:21,z:23},{kind:'rifle',owner:1,x:53,z:23}],[
 {set:[[1,{targetId:2}]],command:{t:'move',orders:[[1,17,23]]}},
 {command:{t:'stop',ids:[1]}},
 {set:[[1,{hitAt:1}]],command:{t:'move',orders:[[1,15,23]]}},
]);
trace('hidden-physical-change',maps[0],[{kind:'rifle',x:5,z:5},{kind:'rifle',owner:1,x:61,z:45}],[
 {cells:[[24+2*32,'B']],command:{t:'move',orders:[[1,55,5]]}},
 {set:[[1,{x:45,z:5,path:[],worldGoal:null}]],tick:2,command:{t:'move',orders:[[1,55,5]]}},
]);
for(const [kind,def]of Object.entries(sim.UNITS).filter(([,d])=>!d.air))trace('type-'+kind,maps[0],[{kind,x:13,z:23},{kind:'rifle',owner:1,x:53,z:23}],[{command:{t:'move',orders:[[1,25,23]]}},{command:{t:'amove',orders:[[1,35,23]]}},{command:{t:'stop',ids:[1]}}]);

// Immediate Cover uses the real command body and existing route oracle.
const coverCases=[
 ['open-no-cover',maps[0],[{kind:'rifle',x:25,z:23}],[{command:{t:'cover',ids:[1]}}]],
 ['mixed-retreat-no-cover',maps[0],[{kind:'rifle',x:25,z:23,fields:{retreating:true,follow:77}},{kind:'rifle',x:29,z:23,fields:{follow:77}}],[{command:{t:'cover',ids:[1,2]}}]],
 ['mixed-kind-no-cover',maps[0],[{kind:'tank',x:25,z:23,fields:{follow:77}},{kind:'rifle',x:29,z:23,fields:{follow:77}}],[{command:{t:'cover',ids:[1,2]}}]],
 ['riding-refusal',maps[1],[{kind:'rifle',x:27,z:23,fields:{riding:9,follow:77}}],[{command:{t:'cover',ids:[1]}}]],
 ['operative-refusal',maps[1],[{kind:'rifle',x:27,z:23,fields:{operative:{},follow:77}}],[{command:{t:'cover',ids:[1]}}]],
 ['forced-supply-refusal',maps[1],[{kind:'rifle',x:27,z:23,fields:{logistics:{forced:true,stranded:true},follow:77}}],[{command:{t:'cover',ids:[1]}}]],

 ['wrong-kind',maps[1],[{kind:'tank',x:25,z:23,fields:{follow:77}}],[{command:{t:'cover',ids:[1]}}]],
 ['wrong-owner',maps[1],[{kind:'rifle',owner:1,x:25,z:23}],[{command:{t:'cover',ids:[1]}}]],
 ['retreating',maps[1],[{kind:'rifle',x:25,z:23,fields:{retreating:true,follow:77}}],[{command:{t:'cover',ids:[1]}}]],
 ['trench-stays',maps[1],[{kind:'rifle',x:21,z:23,fields:{path:[{x:45,z:23}],targetId:99}}],[{command:{t:'cover',ids:[1]}}]],
 ['wood-stays',maps[1],[{kind:'rifle',x:41,z:23,fields:{worldGoal:{x:55,z:23},follow:77}}],[{command:{t:'cover',ids:[1]}}]],
 ['run-to-trench',maps[1],[{kind:'rifle',x:27,z:23}],[{command:{t:'cover',ids:[1]}}]],
 ['two-squads',maps[1],[{kind:'rifle',x:27,z:23},{kind:'mg',x:27,z:25}],[{command:{t:'cover',ids:[1,2]}}]],
 ['duplicate-selected',maps[1],[{kind:'rifle',x:27,z:23}],[{command:{t:'cover',ids:[1,1]}}]],
 ['visible-threat',maps[1],[{kind:'rifle',x:27,z:23},{kind:'rifle',owner:1,x:37,z:25}],[{command:{t:'cover',ids:[1]}}]],
];
for(const [name,map,initial,actions] of coverCases) trace('cover-'+name,map,initial,actions);

const movement=[];
for(const kind of ['rifle','tank']) {
 const map=makeMap('disconnected',flat().map(r=>r.slice(0,16)+'B'+r.slice(17)));
 const saved=Math.random;Math.random=()=>.5;let g,players;try{g=sim.createGame(map,[],false,[],[],{weather:false});players=sim.createGame(map,['A','B'],false,[0,1],[0,1],{weather:false}).players;}finally{Math.random=saved;}
 g.players=players;g.units=new Map();g.tick=1;g.nextId=1;
 const u=sim.internals.spawnUnit(g,0,kind);Object.assign(u,{x:9,z:23,path:[{x:55,z:23}],stuck:1,repath:0,holdPos:true,auto:false,autoRetreat:false});sim.__placeUnit(g,u,{x:9,z:23});
 sim.__beginPathTick(g);const old=u.path;sim.__replan(g,u,false);
 movement.push({map,kind,expected:{path:u.path.map(pt),repath:bits(u.repath),routeEnd:pt(u.routeEnd),replaced:u.path!==old}});
}
clean();const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases,movement})+'\n';
const path=fileURLToPath(new URL('../rust/crates/core/tests/fixtures/match-commands.json',import.meta.url));
if(process.argv.includes('--check')){if(readFileSync(path,'utf8')!==output)throw Error('Match commands fixture differs');}else writeFileSync(path,output);
console.log(`Verified ${cases.length} command traces, ${cases.reduce((n,c)=>n+c.steps.length,0)} steps, ${movement.length} recovery/replan branches`);
