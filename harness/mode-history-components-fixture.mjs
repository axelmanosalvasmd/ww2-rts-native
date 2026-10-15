// Original Source sampling and finish callbacks on declared component inputs.
// Declared component facts stay separate from actual Tutorial constructor output.
import {readFileSync, writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
if (process.arch !== 'x64' || process.version !== 'v24.21.0') throw Error('Use x64 Node v24.21.0');
const clean = () => {
  if (execFileSync('git', ['rev-parse', 'HEAD'], {cwd: reference, encoding:'utf8'}).trim() !== pin
    || execFileSync('git', ['status', '--porcelain', '--untracked-files=no'], {cwd:reference, encoding:'utf8'}).trim()) {
    throw Error('Pinned clean reference required');
  }
};
clean();
registerHooks({load(url, context, next) {
  const result = next(url, context);
  if (url !== pathToFileURL(`${reference}/shared/sim.js`).href) return result;
  const source = String(result.source);
  if (source.split('const isStructure = (g)').length !== 2) throw Error('Source sampler predicate anchor changed');
  return {...result, source:source + '\nexport {isStructure as __historyIsStructure};\n'};
}});
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const story = await import(pathToFileURL(`${reference}/shared/story.js`));
const server=readFileSync(`${reference}/server.js`,'utf8');
function section(begin,end) {
  if (server.split(begin).length !== 2) throw Error(`Source anchor changed: ${begin}`);
  const first=server.indexOf(begin), last=server.indexOf(end,first);
  if (last <= first) throw Error(`Source end anchor changed: ${end}`);
  return server.slice(first,last);
}
// Run the original record owner in memory. No server module, socket, credential,
// timer, shard process or disk record file is opened by this component recorder.
const recordSource=section('const recordKey = ', '// Map editor saves need a password:');
function recordOwner() {
  const file={read:()=> '{}',write:()=>{}};
  return new Function('recordFile','SHARD','TICK',
    `let records=null;\n${recordSource}\nreturn {hordeResult,recordOf};`)(file,false,0.05);
}
// These are the unmodified final composition statements, not a simulated hold.
// The six-second hold and transport have their own existing server tests.
const composition=section('  room.players.forEach((p, i) => (p.lastMatch = ', '\n  return true;');
const compose=new Function('room','g','storyResult','hordeResult','finishMatch',composition);
const projectionSource=section('    result: room.result ? ', '\n    players:');
const projection=projectionSource.trim().slice('result: '.length,-1);
const project=new Function('room','p','i',`return (${projection});`);
const hostEnd=section("if (msg.t === 'end'", "if (msg.t === 'nextwave'");
if (!hostEnd.includes('finishMatch(room, { ended: true })')) throw Error('Source host-end branch changed');
const bits = n => {const b=Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex');};
const encode = value => typeof value === 'number' ? bits(value) : Array.isArray(value) ? value.map(encode)
  : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([k,v]) => [k,encode(v)])) : value;
const base = {
  width:40, height:40, teams:[0,2,0], scores:[1.75,4.5,2.5],
  points:[0,2,-1,1],
  units:[
    {owner:0, type:'bunker', hp:300}, {owner:1, type:'flakpos', hp:300},
    {owner:2, type:'hq', hp:600}, {owner:1, type:'barracks', hp:600},
    {owner:2, type:'wall', hp:300}, {owner:0, type:'rifle', hp:100},
    {owner:1, type:'hq', hp:0},
  ],
};
function game(input) {
  return {
    w:input.width, h:input.height, tick:input.tick, winner:null, winVp:750,
    mode:input.mode === null ? undefined : {kind:input.mode},
    players:input.teams.map((team,slot) => ({team,slot,name:`P${slot}`,vp:input.scores[slot]})),
    points:input.points.map(owner => ({owner})),
    units:new Map(input.units.map((unit,index) => [index+1,{id:index+1,...unit}])),
  };
}
const samples=[];
for (const mode of [null,'classic','world','annihilation','assault','horde','tutorial']) {
  for (const tick of [1,199,200]) {
    const input=structuredClone({...base,mode,tick});
    if (mode === 'world') input.units.push({owner:-1,type:'worldbase',hp:600});
    const g=game(input);
    story.sample(g,sim.__historyIsStructure(g));
    samples.push({name:`${mode ?? 'conquest'}-${tick}`,input,expected:encode(g.timeline)});
  }
}
const deaths=[
  {owner:1,kind:'hq',at:{x:7.25,z:-0.05},by:0},
  {owner:1,kind:'bunker',at:{x:19.75,z:21.25},by:0},
  {owner:1,kind:'wall',at:{x:31.25,z:41.75},by:0},
  {owner:0,kind:'rifle',at:{x:99,z:99},by:1},
];
const finishes=[];
for (const [selector,mode,reason] of [['hq','classic','hq'],['bunker','annihilation','bunkers'],
  ['bunker','assault','structures'],['bunker','horde','structures']]) {
  for (const variant of ['win','draw','fallback']) {
    const input=structuredClone({...base,mode,tick:219,deaths:variant === 'fallback' ? [] : deaths,
      selector,reason,winner:variant === 'draw' ? -1 : 0});
    const g=game(input);
    for (const death of input.deaths) story.died(g,{owner:death.owner,type:death.kind,...death.at},sim.UNITS[death.kind],death.by);
    sim.finish(g,input.winner,reason,g.fallen?.[selector]);
    finishes.push({name:`${mode}-${variant}`,input,expected:encode({
      winner:g.winner,reason:g.endReason,at:g.endAt,endTick:g.endTick,
      fallen:g.fallen ?? {},timeline:g.timeline,
    })});
  }
}
const tutorialStarts=[];
const tutorialMap=JSON.parse(readFileSync(`${reference}/maps/tutorial.json`,'utf8'));
for (const factions of [[1],[1,0],[1,2,0],[3,1,2,0,1]]) {
  const input={map:tutorialMap,factions};
  const previous=Math.random; let g;
  Math.random=()=>0.5;
  try {
    g=sim.createGame(input.map,factions.map((_,i)=>`Human${i}`),false,factions.map(()=>0),factions,
      {mode:'tutorial',weather:false,tech:false,supply:false,logistics:false});
  } finally { Math.random=previous; }
  tutorialStarts.push({input,expected:encode({
    players:g.players.map(p=>({name:p.name,team:p.team,faction:p.faction,spawn:p.spawn,mp:p.mp,support:p.sup})),
    units:[...g.units.values()].map(u=>({id:u.id,type:u.type,owner:u.owner,x:u.x,z:u.z,hp:u.hp,rot:u.rot,
      autoRetreat:u.autoRetreat,retreating:u.retreating,holdPos:u.holdPos,path:u.path,amove:u.amove ?? null})),
    mode:{enemy:g.mode.slot,step:g.mode.step,steps:g.mode.steps,goal:g.mode.goal,spawned:g.mode.spawned},
    points:g.points.map(p=>({x:p.x,z:p.z,vp:p.vp,mp:p.mp,owner:p.owner})),
  })});
}
const reports=[];
for (const [mode,sandbox] of [null,'classic','world','annihilation','assault','horde','tutorial']
  .flatMap(mode=>mode === 'horde' ? [[mode,false],[mode,true]] : [[mode,false]])) {
  for (const winner of [0,1,-1]) {
    const shared=mode === 'horde' || mode === 'tutorial';
    const input={mode,winner,teams:shared ? [0,0,1] : [0,1,0],
      canonicalNames:['Sim A','Sim B',shared ? 'Enemy' : 'Sim C'],
      roomNames:shared ? ['Alice','Bob'] : ['Alice','Bob','Cara'],
      story:[{kills:2.25,mpSpent:-1.5},{kills:3.5,supportCalls:2},{kills:999,planesDowned:1}],
      tick:339,endTick:219,reason:winner === -1 ? 'draw' : mode === 'tutorial' ? 'tutorial' : mode === null ? 'vp' : 'structures',
      at:{x:7.3,z:21.3},winVp:750,wave:3,sandbox,
      timeline:mode === null ? [{t:11,vp:[750,2]}] : [{t:11,points:[2,1],structures:[1,0]}]};
    const g={tick:input.tick,winner,players:input.teams.map((team,i)=>({team,name:input.canonicalNames[i]})),
      mode:mode === null ? undefined : {kind:mode,slot:input.roomNames.length,wave:input.wave},
      story:input.story.map(row=>Object.fromEntries(story.STORY_KEYS.map(key=>[key,row[key] ?? 0]))),
      endReason:input.reason,endAt:input.at,endTick:input.endTick,winVp:input.winVp,
      sandbox:input.sandbox,timeline:input.timeline};
    const room={mapName:'Crossroads',army:'standard',players:input.roomNames.map(name=>({name}))};
    const records=recordOwner();
    compose(room,g,story.storyResult,records.hordeResult,(room,result)=>{room.result=result;});
    const people=room.players.map(p=>({...p}));
    const expected={report:room.result,lastMatch:people.map(p=>p.lastMatch),
      seated:people.map((p,i)=>project(room,p,i)),
      moved:project(room,people[1],0),observer:project(room,people[1],-1),
      newcomer:project(room,{name:'New'},0)};
    room.result={ended:true};
    expected.hostEnded=project(room,people[1],0);
    reports.push({input,expected:encode(expected)});
  }
}
const hordeRuns=[];
const records=recordOwner();
for (const [wave,tick,mapName,army] of [[3,339,'Crossroads','standard'],[3,339,'Crossroads','standard'],
  [3,360,'Crossroads','standard'],[2,2000,'Crossroads','standard'],[4,260,'Crossroads','standard'],
  [1,400,'Crossroads','large'],[1,400,'Village','standard']]) {
  const input={wave,tick,map:mapName,army,names:['Alice','Bob'],kills:[2.25,3.5,999]};
  const room={mapName,army,players:input.names.map(name=>({name}))};
  const g={tick,mode:{slot:2,wave},story:input.kills.map(kills=>({kills}))};
  const result=records.hordeResult(room,g);
  hordeRuns.push({input,expected:encode({result,stored:records.recordOf(room)})});
}
clean();
const output=JSON.stringify({format:1,reference:pin,node:process.version,arch:process.arch,samples,finishes,tutorialStarts,reports,hordeRuns})+'\n';
const path=`${root}/rust/crates/simtest/tests/fixtures/mode-history-components.json`;
if (process.argv.includes('--check')) {
  if (readFileSync(path,'utf8') !== output) throw Error('Mode history component fixture differs');
} else writeFileSync(path,output);
console.log(`Verified ${samples.length} sampler cases, ${finishes.length} finish cases, ${tutorialStarts.length} Tutorial starts, ${reports.length} report cases and ${hordeRuns.length} Horde record updates`);
