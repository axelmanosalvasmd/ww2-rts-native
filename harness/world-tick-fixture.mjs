// Observe the actual pinned World controller. Trace additions do not replace phase calls.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {gzipSync} from 'node:zlib';
const root=fileURLToPath(new URL('../',import.meta.url));
const ref=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=args=>execFileSync('git',args,{cwd:ref,encoding:'utf8'}).trim();
const clean=()=>{if(git(['rev-parse','HEAD'])!==pin||git(['status','--porcelain','--untracked-files=no']))throw Error('Pinned tracked-clean reference required');};
if(process.arch!=='x64'||process.version!=='v24.21.0')throw Error('Use x64 Node v24.21.0');
if(process.argv.slice(2).some(v=>v!=='--check'))throw Error('Only --check is supported');
clean();
registerHooks({load(url,context,next){const result=next(url,context);if(url!==pathToFileURL(`${ref}/shared/sim.js`).href)return result;
 let source=String(result.source);
 const once=(anchor,extra)=>{if(source.split(anchor).length!==2)throw Error(`Source anchor moved: ${anchor}`);source=source.replace(anchor,anchor+extra);};
 once('export function step(g) {',"\n  __worldTickObserve(g,'StructureSync');");
 const start=source.indexOf('export function step(g) {'),end=source.indexOf('\nconst isStructure =',start);
 let body=source.slice(start,end);
 const insert=(anchor,extra)=>{if(body.split(anchor).length!==2)throw Error(`Step anchor moved: ${anchor}`);body=body.replace(anchor,anchor+extra);};
 insert('  g.tick++;',"\n  __worldTickObserve(g,'Clock');");
 insert('  if (g.winner === null && (g.tick === 1 || g.tick % SAMPLE_EVERY === 0)) sample(g, isStructure(g));',"\n  __worldTickObserve(g,'AfterPeriodicSample');");
 insert('  const list = [...g.units.values()];',"\n  __worldTickObserve(g,'CapturedList',list);");
 insert('  g.smokes = g.smokes.filter(s => s.t > 0);',"\n  __worldTickObserve(g,'AfterWeatherFireFlood',list);");
 const death='  for (const u of list) if (u.hp <= 0) {';
 if(body.split(death).length!==2)throw Error('Death anchor moved');body=body.replace(death,"  __worldTickObserve(g,'OrdinaryDeath',list);\n"+death);
 const convoys='  stepConvoys(g, dt, supplyHooks(g));';
 if(body.split(convoys).length!==2)throw Error('Convoy anchor moved');body=body.replace(convoys,"  __worldTickObserve(g,'Convoys',list);\n"+convoys+"\n  __worldTickObserve(g,'AfterConvoys',list);");
 const world="  if (g.mode?.kind === 'world') { stepWorld(g, list, dt); return; }";
 if(body.split(world).length!==2)throw Error('World dispatch anchor moved');body=body.replace(world,"  if (g.mode?.kind === 'world') { stepWorld(g, list, dt); __worldTickObserve(g,'AfterWorldRegions',list); return; }");
 source=source.slice(0,start)+body+source.slice(end);
 once('function weather(g, dt) {',"\n  __worldTickObserve(g,'LivingWeather');");
 once('function supplyLines(g, list) {',"\n  __worldTickObserve(g,'PointSupply',list);");
 once('function supplyNetwork(g, list) {',"\n  __worldTickObserve(g,'SupplyNetwork',list);");
 once('function worldIncome(g, p, list, dt) {',"\n  __worldTickObserve(g,'WorldIncome',list,p.slot);");
 once('function stepWorld(g, list, dt) {',"\n  __worldTickObserve(g,'WorldRegions',list);");
 once('export function finish(g, winner, reason, at) {',"\n  __worldTickObserve(g,'Finish',undefined,undefined,reason);");
 source+=`\nfunction __worldTickObserve(g,phase,list,slot,reason){if(!g.__worldTickTrace)return;g.__worldTickPhase=phase;g.__worldTickTrace.push({phase,tick:g.tick,slot:slot??null,reason:reason??null,current:[...g.units.keys()],captured:list?.map(u=>u.id)??null,missing:list?.filter(u=>!g.units.has(u.id)).map(u=>[u.id,u.owner,u.hp])??[],seats:g.players.map(p=>p.slot),local:g.players[-1]?.slot??null,out:g.players.map(p=>!!p.out),money:g.players.map(p=>p.mp),timeline:g.timeline?.length??null,convoys:!!g.convoys,pointSupply:g.supply,regions:g.world.regions.map(r=>[r.team,r.progress,r.capper,r.capTick??null])});}\n`;
 source+='\nexport const __worldTickRegion=worldRegionAt;\n';
 return {...result,source};}});
const sim=await import(pathToFileURL(`${ref}/shared/sim.js`));
const regions=Array.from({length:64},(_,i)=>({id:i,x:i%8*64+32,y:Math.floor(i/8)*64+32,kind:'rural',bounds:[i%8*64,Math.floor(i/8)*64,i%8*64+64,Math.floor(i/8)*64+64],...(i===0?{home:0}:i===63?{home:1}:{})}));
const map={name:'World controller ordering',w:512,h:512,rows:Array(512).fill('.'.repeat(512)),spawns:[{x:32,y:32},{x:480,y:480}],points:[],world:{size:'huge',total:64,regionCells:64,regionMap:Array.from({length:512*512},(_,c)=>Math.floor(Math.floor(c/512)/64)*8+Math.floor(c%512/64)),regions}};
const cases=[];
function record(name,{until=1,supply=false,logistics=false,lose=false,death=false,capture=false,shortage=false}={}){
 const save=Math.random;Math.random=()=>.5;let g;
 try{g=sim.createGame(map,['A','B'],false,[0,1],[0,0],{mode:'world',tech:true,weather:false,supply,logistics});
 for(const u of g.units.values()){u.holdPos=true;u.holdFire=true;u.auto=false;}
 if(lose)for(const r of g.world.regions)if(r.team===0){r.team=-1;r.progress=0;}
 const corpse=death?[...g.units.values()].find(u=>u.owner===0&&u.type==='rifle'):null;if(corpse){corpse.hp=0;corpse.lastHit=1;}
 if(shortage){const u=[...g.units.values()].find(u=>u.owner===0&&sim.UNITS[u.type].infantry&&u.logistics);if(!u)throw Error('No canonical reserve unit');const r=g.world.regions.find(r=>r.team===1);u.x=r.x+45;u.z=r.z+45;Object.assign(u.logistics,{ammo:0,provisions:20,cutFor:30,supplyRate:0});}
 let capturedRegion=null;
 if(capture){const r=g.world.regions.find(r=>r.team===0&&!([...g.units.values()].some(u=>u.type==='worldbase'&&sim.__worldTickRegion(g,u)?.id===r.id)));
  if(!r)throw Error('No commander region without a guard-base lock');const u=[...g.units.values()].find(u=>u.owner===0&&sim.UNITS[u.type].infantry);u.x=r.x;u.z=r.z;r.team=-1;r.progress=.999;r.capper=0;capturedRegion=r.id;
 }
 g.seed=123456789;g.__worldTickTrace=[];
 const deleted=[],originalDelete=g.units.delete;
 g.units.delete=function(id){const u=this.get(id);deleted.push({tick:g.tick,phase:g.__worldTickPhase,id,owner:u?.owner??null,hp:u?.hp??null});return originalDelete.call(this,id);};
 for(let tick=1;tick<=until;tick++)sim.step(g);
 const phases=g.__worldTickTrace;
 for(const entry of phases){if(entry.seats.length!==2||entry.seats.includes(-1)||entry.local!==-1||entry.pointSupply!==false)throw Error('Source actor membership changed');}
 for(const tick of new Set(phases.map(e=>e.tick).filter(t=>t>0))){const rows=phases.filter(e=>e.tick===tick);const index=p=>rows.findIndex(e=>e.phase===p);
  if(index('Clock')<0)continue;
  for(const [a,b]of [['Clock','AfterPeriodicSample'],['AfterPeriodicSample','CapturedList'],['CapturedList','LivingWeather'],['LivingWeather','OrdinaryDeath'],['OrdinaryDeath','WorldIncome'],['WorldIncome','Convoys'],['AfterConvoys','WorldRegions'],['WorldRegions','AfterWorldRegions']])if(index(a)<0||index(b)<=index(a))throw Error(`${name} invalid actual phase order ${a}/${b}`);
 }
 if(phases.some(e=>e.phase==='PointSupply'))throw Error('World point supply must stay disabled');
 if(logistics&&until>=20&&!phases.some(e=>e.phase==='SupplyNetwork'&&e.tick===20))throw Error('Missing actual regional network update');
 if(shortage&&!phases.some(e=>e.phase==='AfterConvoys'&&e.current.some(id=>!e.captured.includes(id))))throw Error('Missing actual post-list convoy birth');
 if(capture){const r=g.world.regions.find(r=>r.id===capturedRegion);if(r.team!==0||r.progress!==1||r.capTick!==1||g.story[0].captures!==1)throw Error('Actual region capture did not occur');const income=phases.find(e=>e.phase==='WorldIncome'&&e.slot===0);if(income.regions[capturedRegion][0]!==-1)throw Error('Income did not use pre-capture territory');}
 if(lose){if(!g.players[0].out||g.winner!==1||g.endReason!=='nations'||!deleted.length||deleted.some(e=>e.phase!=='WorldRegions'))throw Error('Missing actual regional elimination and nations finish');}
 if(death&&!phases.some(e=>e.phase==='WorldIncome'&&e.missing.some(u=>u[0]===corpse.id)))throw Error('Retired tick facts did not survive to income');
 cases.push({name,input:{until,supply,logistics,lose,death,capture,capturedRegion,shortage},phases,deleted,final:{tick:g.tick,winner:g.winner,reason:g.endReason??null,out:g.players.map(p=>!!p.out),timeline:g.timeline??null,regions:g.world.regions.map(r=>[r.team,r.progress,r.capper,r.capTick??null]),story:g.story??null,units:[...g.units.values()].map(u=>[u.id,u.owner,u.type,u.hp])}});
 }finally{Math.random=save;}
}
record('neutral actors remain outside commander loops',{until:10});
record('World disables point supply requested by options',{until:41,supply:true});
record('regional logistics network precedes World income',{until:20,supply:true,logistics:true});
record('convoy births follow captured upkeep membership',{until:40,logistics:true,shortage:true});
record('region capture occurs after income',{capture:true});
record('last-region elimination follows final income',{lose:true});
record('ordinary retirement leaves captured facts for income',{death:true});
clean();
const output=gzipSync(Buffer.from(JSON.stringify({format:1,reference:pin,node:process.version,arch:process.arch,cases})+'\n'));
const path=`${root}/rust/crates/simtest/tests/fixtures/world-tick.json.gz`;
if(process.argv.includes('--check')){if(!readFileSync(path).equals(output))throw Error('World tick fixture differs');}else writeFileSync(path,output);
console.log(`Verified ${cases.length} actual World controller traces, ${cases.reduce((n,c)=>n+c.input.until,0)} ticks`);
