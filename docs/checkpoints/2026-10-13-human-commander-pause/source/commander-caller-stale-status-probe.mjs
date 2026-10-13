import {execFileSync} from 'node:child_process';
import * as sim from '/Users/judiazm/Projects/ww2-rts-native/reference/ww2-rts/shared/sim.js';
import * as ai from '/Users/judiazm/Projects/ww2-rts-native/reference/ww2-rts/shared/ai.js';
import {planner,playMatch,aiTick} from '/Users/judiazm/Projects/ww2-rts-native/reference/ww2-rts/shared/ai-schedule.js';
const ref='/Users/judiazm/Projects/ww2-rts-native/reference/ww2-rts',pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const clean=()=>{if(execFileSync('git',['-C',ref,'rev-parse','HEAD'],{encoding:'utf8'}).trim()!==pin||execFileSync('git',['-C',ref,'status','--porcelain','--untracked-files=no'],{encoding:'utf8'}).trim())throw Error('Source pin/clean guard failed');};clean();
const map={name:'Stale seat status',w:64,h:64,rows:Array(64).fill('.'.repeat(64)),spawns:[{x:4,y:4},{x:59,y:59}],points:[{x:27,y:10},{x:13,y:27},{x:32,y:32}]};
for(const kind of ['live-out-after-view6','live-winner-after-view6','live-winner-default-sent']){
 let seed=0x516e9200,phase='startup',stored,draws={startup:[],step:[],observe:[],thinker:[],command:[]};const prior=Math.random;
 const deliveries=[],acts=[],attempts=[];
 Math.random=()=>{const value=(seed=(Math.imul(seed,1664525)+1013904223)>>>0)/4294967296;draws[phase].push(value);return value;};
 const wrapped={...ai,observe:(g,s,cache)=>{const old=phase;phase='observe';try{const v=ai.observe(g,s,cache);stored=v;deliveries.push({tick:g.tick,viewTick:v.tick,out:v.players[s].out,winner:v.winner,mp:v.players[s].mp});return v;}finally{phase=old;}},think:(g,s,o)=>{const old=phase;phase='thinker';acts.push({tick:g.tick,viewTick:o.view.tick,viewOut:o.view.players[s].out,viewWinner:o.view.winner,liveOut:!!g.players[s].out,liveWinner:g.winner});try{return ai.think(g,s,{...o,submit:command=>{const save=phase;phase='command';try{const denial=sim.command(g,s,command);attempts.push({tick:g.tick,viewTick:o.view.tick,command,denial:denial??null});return denial;}finally{phase=save;}}});}finally{phase=old;}}};
 const brain=planner(wrapped),seats=[{slot:1,level:'hard',brain}],source={...sim,step:g=>{const old=phase;phase='step';try{return sim.step(g);}finally{phase=old;}}};
 let g,loopReceipt,directReceipt;try{
  g=playMatch({map,names:['A','B'],teams:[0,1],factions:[0,1],shuffle:false,options:{mode:'conquest',tech:false,supply:false,weather:false,logistics:false},seats,maxTicks:7,sim:source,setup:g=>{g.players[1].mp=0;for(const u of g.units.values())if(u.owner!==1)Object.assign(u,{holdFire:true,holdPos:true,autoRetreat:false});},onTick:g=>{if(g.tick===6){if(kind==='live-out-after-view6')g.players[1].out=true;else{g.winner=0;g.endReason='focused-status-boundary';g.endAt={x:0,z:0};}}}});
  loopReceipt={tick:g.tick,attemptCount:attempts.length,draws:Object.fromEntries(Object.entries(draws).map(([k,v])=>[k,v.length]))};
  if(kind.startsWith('live-winner')){
   source.step(g);
   const views=[];views[1]=stored;
   // Force no new delivery to isolate the retained-view boundary. The ordinary default sent flag would deliver after a winner.
   const sent=kind==='live-winner-default-sent'?undefined:false;
   aiTick(g,seats,views,{...(sent===undefined?{}:{sent}),sim:source});
   directReceipt={tick:g.tick,sent:sent??true,viewTick:views[1].tick};
  }
 }finally{Math.random=prior;}
 console.log(JSON.stringify({kind,deliveries,acts,attempts,loopReceipt,directReceipt,draws:Object.fromEntries(Object.entries(draws).map(([k,v])=>[k,{count:v.length,values:v}])),final:{tick:g.tick,out:!!g.players[1].out,winner:g.winner,mp:g.players[1].mp}}));
}clean();console.log('Source pin and tracked-clean checks passed before and after');
