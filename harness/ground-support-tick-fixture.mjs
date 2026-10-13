// Record unchanged full Source ticks for the later sole ground-support tick caller.
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {gzipSync} from 'node:zlib';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], {encoding:'utf8'}).trim();
const clean = () => {
  assert.equal(git(['rev-parse','HEAD']), pin);
  assert.equal(git(['status','--porcelain','--untracked-files=no']), '');
};
clean();
const simURL = pathToFileURL(`${reference}/shared/sim.js`).href;
const sim = await import(simURL);
const {rebuildGrid} = await import(pathToFileURL(`${reference}/shared/grid.js`));
const mask=(1n<<64n)-1n, gamma=0x9e3779b97f4a7c15n;
const mix=x=>{x=((x^(x>>30n))*0xbf58476d1ce4e5b9n)&mask;x=((x^(x>>27n))*0x94d049bb133111ebn)&mask;return x^(x>>31n);};
function random(seed) {
  let state=mix(BigInt(seed)^((4n*gamma)&mask)), count=0;
  return {next(){state=(state+gamma)&mask;count++;return Number(mix(state)>>11n)/9007199254740992;},
    snapshot:()=>({count,state:state.toString(16).padStart(16,'0')})};
}
const copy=v=>v instanceof Map?[...v].map(([k,v])=>[copy(k),copy(v)]):v instanceof Set?[...v].map(copy):ArrayBuffer.isView(v)?[...v]:Array.isArray(v)?v.map(copy):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).filter(([,v])=>v!==undefined&&typeof v!=='function').map(([k,v])=>[k,copy(v)])):v;
const buffer=new DataView(new ArrayBuffer(8));
const bits=n=>{buffer.setFloat64(0,n);return buffer.getBigUint64(0).toString(16).padStart(16,'0');};
function exact(v) {
  const raw=copy(v), floatBits={};
  function visit(v,p='') {
    if(typeof v==='number') {assert.ok(Number.isFinite(v),p);floatBits[p]=bits(v);}
    else if(v&&typeof v==='object')for(const[k,x]of Object.entries(v))visit(x,p?`${p}.${k}`:k);
  }
  visit(raw);return {raw,floatBits};
}
const runs=a=>{const out=[];for(const x of a){if(out.length&&Object.is(out.at(-1)[0],x))out.at(-1)[1]++;else out.push([x,1]);}return out;};
function snapshot(g,rng) {
  return exact({tick:g.tick,visionTick:g.visionTick,nextId:g.nextId,seed:g.seed,wind:g.wind,wx:g.wx,winner:g.winner,
    players:g.players.map(p=>({...p,visible:[...p.visible]})),points:g.points,
    units:[...g.units.values()],unitOrder:[...g.units.keys()],strikes:g.strikes,covers:g.covers??[],smokes:g.smokes,
    shots:g.shots,story:g.story??null,timeline:g.timeline??null,fallen:g.fallen??null,
    flights:[...g.flights.values()],nades:g.nades,salvos:g.salvos,
    environment:{fires:[...g.fires],soak:[...g.soak],mines:[...g.mines],aid:[...g.aid],claims:[...(g.claims??[])],wrecks:g.wrecks,newCells:g.newCells,cellLog:g.cellLog},
    terrain:{chars:runs(g.chars),flags:runs(g.flags),height:g.height&&runs(g.height),cellHp:runs(g.cellHp),wear:runs(g.wear),scar:runs(g.scar),burnt:runs(g.burnt),cellState:runs(g.cellState),versions:[g.terrainVersion??0,g.obstructionVersion??0,g.infantryRegionVersion??0,g.vehicleRegionVersion??0,g.navalRegionVersion??0]},
    random:rng.snapshot()});
}
const map={name:'Ground support full tick',w:40,h:40,rows:Array(40).fill('.'.repeat(40)),spawns:[{x:2,y:2},{x:37,y:37}],points:[{id:'radio',x:25,y:25,kind:'radio'}]};
const options={mode:'conquest',weather:false,tech:false,supply:false,logistics:false};
const cases=[
  {name:'artillery-late-effects',kind:'artillery',at:[46.5,40.75],ticks:200},
  {name:'uk-artillery-doctrine',kind:'artillery',at:[46.5,40.75],faction:3,ticks:240},
  {name:'smoke-lifetime-and-weather',kind:'smoke',ticks:465},
  {name:'smoke-rain-decay',kind:'smoke',rain:.5,ticks:395},
  {name:'cover-activation-and-expiry',kind:'cover',ticks:1245},
  {name:'default-cover-alias',kind:'cover',default:true,ticks:45},
  {name:'radio-before-point-capture',kind:'cover',radio:true,ticks:45},
  {name:'cut-radio',kind:'cover',radio:true,cut:true,ticks:45},
  {name:'short-wallet',kind:'artillery',mp:0,ticks:5},
  {name:'cooldown-denial',kind:'smoke',cooldown:1,ticks:5},
  {name:'away-refusal',kind:'cover',away:true,ticks:5},
  {name:'out-refusal',kind:'cover',out:true,ticks:5},
  {name:'unknown-kind-refusal',kind:'missing',ticks:5},
];
function run(spec,sampling) {
  const prior=Math.random, startupDraws=[], setupOptions={...options};let g;
  if(spec.default)delete setupOptions.mode;
  if(spec.rain)setupOptions.weather='rain';
  Math.random=()=>{startupDraws.push(.5);return .5;};
  try {g=sim.createGame(map,['A','B'],false,[0,0],[spec.faction??0,1],setupOptions);}
  finally {Math.random=prior;}
  g.units.clear();g.nextId=1;
  const u=sim.internals.spawnUnit(g,1,'rifle');
  const [x,z]=spec.at??[41,41];
  Object.assign(u,{x,z,holdPos:true,holdFire:true,auto:false,autoRetreat:false});
  rebuildGrid(g);g.nextId=1000;g.seed=123;g.wind={a:0,v:.5};
  for(const p of g.players)p.mp=500;
  Object.assign(g.players[0],{mp:spec.mp??500,away:spec.away??false,out:spec.out??false});
  if(spec.cooldown)g.players[0].sup[spec.kind]=spec.cooldown;
  if(spec.radio){g.points[0].owner=0;g.points[0].cut=spec.cut??false;}
  const request={t:'support',kind:spec.kind,x:41,z:41,dir:.3};
  const rng=random(701), frames=[], initial=snapshot(g,rng);
  const denial=sim.command(g,0,request)??null, admitted=snapshot(g,rng);
  const drawSites=new Map();
  for(let tick=1;tick<=spec.ticks;tick++) {
    const before=JSON.stringify([g.strikes.map(s=>[s.kind,s.live,s.left]),(g.covers??[]).length,g.smokes.length,g.units.size]);
    g.shots=[];
    Math.random=()=>{
      const stack=new Error().stack;
      assert.ok(stack.includes('stepStrikes'),`${spec.name}: unexpected random purpose ${stack}`);
      const site=stack.split('\n').find(s=>s.includes(`${simURL}:`));
      assert.ok(site);drawSites.set(site.trim(),(drawSites.get(site.trim())??0)+1);
      return rng.next();
    };
    try {sim.step(g);}finally {Math.random=prior;}
    const transition=before!==JSON.stringify([g.strikes.map(s=>[s.kind,s.live,s.left]),(g.covers??[]).length,g.smokes.length,g.units.size]);
    if(sampling&&(g.shots.length||transition||[1,2,3,4,5,39,40,41,59,60,61,99,100,101,spec.ticks].includes(tick)))frames.push({tick,expected:snapshot(g,rng)});
  }
  return {name:spec.name,spec,setup:{map,options:setupOptions,nativeMode:spec.default?'Default':'Conquest',teams:[0,0],factions:[spec.faction??0,1],startupDraws,unit:{id:1,owner:1,kind:'rifle',x,z,holdPos:true,holdFire:true,auto:false,autoRetreat:false},nextId:1000,environmentSeed:123,wind:{a:0,v:.5}},request,denial,initial,admitted,frames,final:snapshot(g,rng),drawSites:[...drawSites].map(([site,count])=>({site:site.replace(reference,'reference/ww2-rts'),count}))};
}
const traces=cases.map(spec=>{
  const observed=run(spec,true),unobserved=run(spec,false);
  assert.deepEqual(observed.initial,unobserved.initial);
  assert.deepEqual(observed.admitted,unobserved.admitted);
  assert.deepEqual(observed.final,unobserved.final);
  assert.deepEqual(observed.drawSites,unobserved.drawSites);
  const events=observed.frames.flatMap(frame=>frame.expected.raw.shots);
  if(spec.kind==='artillery'&&observed.denial===null) {
    assert.equal(events.filter(e=>e.k==='shell').length,sim.supShells(spec.faction??0,'artillery'));
    assert.ok(events.some(e=>e.k==='hurt'),'authored artillery actor must receive source damage');
  }
  if(spec.kind==='smoke'&&observed.denial===null) {
    assert.equal(events.filter(e=>e.k==='smokeshells').length,1);
    assert.equal(observed.final.raw.smokes.length,0,'full-step weather must retire smoke');
  }
  if(spec.name==='cover-activation-and-expiry')assert.equal(observed.final.raw.covers.length,0);
  assert.equal(observed.drawSites.reduce((n,row)=>n+row.count,0),observed.final.raw.random.count);
  return observed;
});
clean();
const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,seed:701,
  scope:'Unchanged complete Source step. Ground support only; no native composed-tick acceptance. All raw represented timers/effects retained with numeric bits.',
  randomness:'Source-only established OffMap purpose adapter. Native Match RNG stays naturally seeded, with no runtime or expected-state injection. Every source draw must come from stepStrikes. Sampled and unsampled executions must match.',traces})+'\n';
const path=`${root}/rust/crates/simtest/tests/fixtures/ground-support-tick.json.gz`,compressed=gzipSync(output,{level:9});
if(process.argv.includes('--check'))assert.deepEqual(readFileSync(path),compressed);else writeFileSync(path,compressed);
console.log(`${traces.length} original full-step traces, ${traces.reduce((n,t)=>n+t.spec.ticks,0)} ticks, ${traces.reduce((n,t)=>n+t.frames.length,0)} checkpoints`);
