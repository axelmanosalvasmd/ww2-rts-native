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
const codes=['mp','mun','fuel','ammo','pop','cooldown','unseen','notVisible','blocked','needs','max','suddenDeath','queueFull','ordersFull','retreating','noBuilders','noCover','notWaiting','scenario','territory','coast','tier','rail','save','station','railCut','airfield','shore','unknown'];
const denials=codes.flatMap(code=>[null,'','buy','move'].map(command=>({code,command,expected:denySentence(code,command)})));
const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,cases,denials})+'\n';
const path=new URL('../rust/crates/core/tests/fixtures/interface-availability.json',import.meta.url);
if(process.argv.includes('--check')) {if(await readFile(path,'utf8')!==output)throw Error('fixture differs');}
else await writeFile(path,output);
console.log(`${cases.length} pure availability queries, ${denials.length} denial queries`);
