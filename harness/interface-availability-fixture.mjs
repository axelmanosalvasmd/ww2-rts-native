// Pure recipient-query inputs, not captured match behavior or authoritative state.
import { readFile, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { availability, denySentence } from '../reference/ww2-rts/client/availability.js';
import { UNITS, SUPPORT, FORTS, CFG } from '../reference/ww2-rts/shared/sim.js';

const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const root = fileURLToPath(new URL('../', import.meta.url));
if (execFileSync('git', ['-C', root + 'reference/ww2-rts', 'rev-parse', 'HEAD'], {encoding:'utf8'}).trim() !== pin) throw Error('wrong prototype pin');
const row = (id, kind, owner = 0) => [id,kind,owner,10,10,0,0,100,0,0,0,0,0,0,1];
const cases = [];
const kinds = Object.keys(UNITS).filter(k => !UNITS[k].structure);
const supports = Object.keys(SUPPORT), forts = Object.keys(FORTS);
for (let i = 0; i < 1000; i++) {
  const j = Math.floor(i / 10), branch = i % 10;
  const mode = ['conquest','classic','world','horde','tutorial'][j % 5];
  const s = {t:'s',mode:{kind:mode,slot:2,suddenDeath:j % 23 === 0},mp:[0,99,149,299,1000][Math.floor(j/5)%5],fuel:[0,59,1000][Math.floor(j/7)%3],mun:[0,14,89,1000][Math.floor(j/3)%4],
    units:[row(1,'rifle'),row(2,'engineer'),row(3,'hq'),row(4,'barracks'),row(5,'motorpool'),row(6,'armory'),row(7,'airfield')],
    army:{pop:[0.1,1,2][Math.floor(j/11)%3]},tech:{tier:1+j%3,armory:{inf:j%4,guns:j%4,armor:j%4},lab:[]},out:[j%29===0,false,false],sup:{},queues:[],productionJobs:[]};
  if (j%4===0) s.tech.lab=[['guns',10,30]];
  if (j%6===0) s.units[5][14]=0.5;
  if (j%8===0) s.units[4][7]=0;
  if (j%7===0) s.units[0][12]=1;
  if (j%9===0) s.units[1][12]=1;
  if (j%3===0) s.units[0][11]=0.01+j%5;
  if (j%13===0) s.units[5][2]=1;
  if (j%5===0) s.queues=[[4,'conscript',1,12,...Array(j%7).fill('conscript')]];
  if (mode==='world') s.world={cap:[1,2,60][Math.floor(j/5)%3]};
  const a={slot:0,ids:j%17===0?[]:[1,2],teams:[0,j%2,1],naval:j%2===0};
  if (branch===0) Object.assign(a,{t:'buy',unit:kinds[j%kinds.length],...(j%2?{from:4}:{})});
  if (branch===1) {Object.assign(a,{t:'support',kind:supports[j%supports.length]});s.sup[a.kind]=j%3?0:1.01;}
  if (branch===2) Object.assign(a,{t:'build',kind:['hq','armory','airfield','supplycache','flakpos','motorpool'][j%6]});
  if (branch===3) Object.assign(a,{t:'tech',kind:['tier','inf','guns','armor'][j%4]});
  if (branch===4) Object.assign(a,{t:'dig',kind:forts[j%forts.length],queue:j%2===0});
  if (branch===5) Object.assign(a,{t:'entrench'});
  if (branch===6) Object.assign(a,{t:'stance'});
  if (branch===7) Object.assign(a,{t:'cover'});
  if (branch===8) Object.assign(a,{t:'ability',unit:'rifle',queue:j%2===0});
  if (branch===9) {Object.assign(a,{t:'cancelProduction',id:4,job:7});s.productionJobs=[[4,[{id:7,status:j%2?'active':'waiting'}]]];}
  cases.push({snapshot:s,action:a,expected:availability(s,CFG,a)});
}
// Isolate denial precedence and threshold branches that the broad matrix can miss.
const focusedCases = [];
const focused = (name, action, change = () => {}) => {
  const snapshot = {t:'s',mode:{kind:'classic'},mp:10000,fuel:10000,mun:10000,
    units:['rifle','engineer','hq','barracks','motorpool','armory','airfield','shipyard'].map((kind,i)=>row(i+1,kind)),
    tech:{tier:3,armory:{inf:0,guns:0,armor:0},lab:[]},sup:{},queues:[],productionJobs:[]};
  const query = {slot:0,ids:[1,2],teams:[0,0,1],naval:true,...action};
  change(snapshot,query);
  focusedCases.push({name,snapshot,action:query,expected:availability(snapshot,CFG,query)});
};
for (const [kind, prerequisite] of [['motorpool','barracks'],['airfield','motorpool']]) {
  focused(`build-${kind}-finished-prerequisite`,{t:'build',kind});
  for (const state of ['absent','unfinished','dead','enemy']) focused(`build-${kind}-${state}-prerequisite`,{t:'build',kind},s=>{
    const unit=s.units.find(v=>v[1]===prerequisite);
    if(state==='absent') s.units=s.units.filter(v=>v!==unit);
    if(state==='unfinished') unit[14]=0.99;
    if(state==='dead') unit[7]=0;
    if(state==='enemy') unit[2]=1;
  });
}
focused('build-armory-without-tech',{t:'build',kind:'armory'},s=>{delete s.tech;});
for (const kind of ['wall','gate']) {
  for (const tier of [1,2]) focused(`dig-${kind}-tier-${tier}`,{t:'dig',kind},s=>{s.tech.tier=tier;});
  focused(`dig-${kind}-requires-engineer`,{t:'dig',kind,ids:[1]});
  focused(`dig-${kind}-retreating-engineer`,{t:'dig',kind,ids:[2]},s=>{s.units[1][12]=1;});
  focused(`dig-${kind}-queued-without-money`,{t:'dig',kind,queue:true},s=>{s.mp=0;});
  for(const delta of [-1,0]) focused(`dig-${kind}-money-${delta}`,{t:'dig',kind},s=>{s.mp=FORTS[kind].cost+delta;});
}
focused('tech-absent',{t:'tech',kind:'tier'},s=>{delete s.tech;});
focused('tech-sudden-death',{t:'tech',kind:'inf'},s=>{s.mode.suddenDeath=true;});
focused('tech-tier-maxed',{t:'tech',kind:'tier'});
focused('tech-line-maxed',{t:'tech',kind:'inf'},s=>{s.tech.armory.inf=3;});
focused('tech-already-researching',{t:'tech',kind:'inf'},s=>{s.tech.lab=[['inf',1,30]];});
focused('tech-line-needs-next-tier',{t:'tech',kind:'inf'},s=>{s.tech.tier=1;s.tech.armory.inf=1;});
for(const [kind,lab] of [['tier','hq'],['inf','armory']]) {
  for(const state of ['absent','unfinished','dead','enemy']) focused(`tech-${kind}-${state}-lab`,{t:'tech',kind},s=>{
    s.tech.tier=2;
    const unit=s.units.find(v=>v[1]===lab);
    if(state==='absent') s.units=s.units.filter(v=>v!==unit);
    if(state==='unfinished') unit[14]=0.99;
    if(state==='dead') unit[7]=0;
    if(state==='enemy') unit[2]=1;
  });
}
focused('tech-armory-all-lines-busy',{t:'tech',kind:'inf'},s=>{s.tech.lab=[['guns',1,30]];});
focused('tech-second-armory-has-room',{t:'tech',kind:'inf'},s=>{s.tech.lab=[['guns',1,30]];s.units.push(row(9,'armory'));});
focused('tech-tier-job-does-not-occupy-armory',{t:'tech',kind:'inf'},s=>{s.tech.lab=[['tier',1,30]];});
for(const kind of ['tier','inf','guns','armor']) {
  for(const [resource,cost] of [['mp',kind==='tier'?400:100],['mun',kind==='tier'?100:25]]) {
    for(const delta of [-1,0]) focused(`tech-${kind}-${resource}-${delta}`,{t:'tech',kind},s=>{s.tech.tier=2;s[resource]=cost+delta;});
  }
}
for(const waiting of [4,5]) focused(`buy-classic-waiting-${waiting}`,{t:'buy',unit:'rifle',from:3},s=>{
  s.queues=[[3,'rifle',1,15,...Array(waiting).fill('rifle')]];
});
focused('buy-selected-full-producer-does-not-use-other',{t:'buy',unit:'rifle',from:3},s=>{
  s.queues=[[3,'rifle',1,15,...Array(5).fill('rifle')]];s.units.push(row(9,'hq'));
});
focused('buy-unspecified-producer-uses-room',{t:'buy',unit:'rifle'},s=>{
  s.queues=[[3,'rifle',1,15,...Array(5).fill('rifle')]];s.units.push(row(9,'hq'));
});
focused('buy-skirmish-ignores-classic-queue-limit',{t:'buy',unit:'rifle',from:4},s=>{
  s.mode.kind='conquest';s.queues=[[4,'rifle',1,15,...Array(5).fill('rifle')]];
});
focused('buy-maximum-includes-waiting-recruit',{t:'buy',unit:'tiger'},s=>{s.queues=[[5,'tank',1,35,'tiger']];});
for(const cap of [2,3]) focused(`buy-population-cap-${cap}`,{t:'buy',unit:'rifle'},s=>{s.world={cap};});
for(const kind of Object.keys(UNITS).filter(kind=>UNITS[kind].ab?.id!=='none'&&!UNITS[kind].structure)) {
  for(const cd of [0,0.01,1,1.01]) focused(`ability-${kind}-cooldown-${cd}`,{t:'ability',unit:kind,ids:[1]},s=>{
    s.units[0][1]=kind;s.units[0][11]=cd;
  });
  focused(`ability-${kind}-queued-without-money`,{t:'ability',unit:kind,ids:[1],queue:true},s=>{
    s.units[0][1]=kind;s.units[0][11]=1;s.mun=0;
  });
  focused(`ability-${kind}-without-munitions`,{t:'ability',unit:kind,ids:[1]},s=>{s.units[0][1]=kind;s.mun=0;});
}
focused('ability-mixed-cooldowns-uses-ready-squad',{t:'ability',unit:'rifle',ids:[1,9]},s=>{s.units[0][11]=3;s.units.push(row(9,'rifle'));});
focused('ability-all-cooldowns-displays-minimum',{t:'ability',unit:'rifle',ids:[1,9]},s=>{s.units[0][11]=3;s.units.push(row(9,'rifle'));s.units.at(-1)[11]=1.01;});
focused('ability-retreating-squad-does-not-mask-ready-squad',{t:'ability',unit:'rifle',ids:[1,9]},s=>{s.units[0][12]=1;s.units.push(row(9,'rifle'));});
for(const state of ['absent','unfinished','finished']) focused(`support-world-para-airfield-${state}`,{t:'support',kind:'para'},s=>{
  s.mode.kind='world';s.world={cap:24};
  if(state==='absent') s.units=s.units.filter(v=>v[1]!=='airfield');
  if(state==='unfinished') s.units.find(v=>v[1]==='airfield')[14]=0.99;
});
const codes=['mp','mun','fuel','ammo','pop','cooldown','unseen','notVisible','blocked','needs','max','suddenDeath','queueFull','ordersFull','retreating','noBuilders','noCover','notWaiting','scenario','territory','coast','tier','rail','save','station','railCut','airfield','shore','unknown'];
const denials=codes.flatMap(code=>[null,'','buy','move'].map(command=>({code,command,expected:denySentence(code,command)})));
const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases,denials,focusedCases})+'\n';
const path=new URL('../rust/crates/core/tests/fixtures/interface-availability.json',import.meta.url);
if(process.argv.includes('--check')) {if(await readFile(path,'utf8')!==output)throw Error('fixture differs');}
else await writeFile(path,output);
console.log(`${cases.length} pure availability queries, ${denials.length} denial queries, ${focusedCases.length} focused queries`);
