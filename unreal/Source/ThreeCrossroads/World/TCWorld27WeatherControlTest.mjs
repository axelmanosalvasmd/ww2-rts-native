// Standalone source test for the rendered-control scheduler. Synthetic native
// log rows are labelled as such. No engine, browser or GPU result is produced.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join, resolve, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { instrumentWorld27, nativeWeatherPlan, browserRenderedControl, recipientTickSchedule } from './TCWorld27WeatherControl.mjs';
const opt=n=>{const i=process.argv.indexOf('--'+n);return i<0?null:process.argv[i+1];};
for(const n of ['scratch','stream','out'])assert.ok(opt(n),'--'+n+' required');
const repo=resolve(dirname(fileURLToPath(import.meta.url)),'../../../..'),ref=join(repo,'reference/ww2-rts'),scratch=resolve(opt('scratch'));
mkdirSync(scratch,{recursive:true});
const {installShims,settle}=await import(pathToFileURL(join(repo,'unreal/Tools/models/shims.mjs')));
installShims();globalThis.localStorage={getItem:()=>null,setItem:()=>{}};globalThis.location={search:''};
const THREE=await import('three'),{gfx}=await import(pathToFileURL(join(ref,'client/gfx.js')));
const {wind,setWind}=await import(pathToFileURL(join(ref,'client/wind.js')));
const {planWeather}=await import(pathToFileURL(join(ref,'shared/weather.js')));
const raw=readFileSync(join(ref,'client/atmosphere.js'),'utf8');
function nodeCopy(source,name) {
  source=source.replace("import { setMood } from './light.js';",'const setMood = () => {};');
  for(const p of ['gfx.js','moods.js','wind.js'])source=source.replace(`from './${p}'`,`from '${pathToFileURL(join(ref,'client',p))}'`);
  source=source.replace("from '../shared/weather.js'",`from '${pathToFileURL(join(ref,'shared/weather.js'))}'`);
  const file=join(scratch,name+'.mjs');writeFileSync(file,source);return import(pathToFileURL(file));
}
const state='() => ({time:t,drift:drift.toArray(),look:EASED.map(k=>cur[k]),goal:EASED.map(k=>goal[k]),haze:hazeNow.toArray(),hazeGoal:hazeGoal.toArray(),banksVisible:banks.visible})';
const original=await nodeCopy(raw.replace('    start, update, setWeather,',`    start, update, setWeather, observe: ${state},`),'original-observed');
const controlled=await nodeCopy(instrumentWorld27('/client/atmosphere.js',raw),'rendered-control');await settle();
const streamBytes=readFileSync(opt('stream')),messages=streamBytes.toString().split('\n').filter(Boolean).map(JSON.parse);
const start=messages.find(m=>m.t==='start'),snapshots=messages.filter(m=>m.t==='s');
const schedule=planWeather('map',JSON.parse(readFileSync(join(ref,'maps/pegasus-bridge.json'))),'pegasus-bridge');
assert.equal(schedule.now,'fog');assert.equal(schedule.next,'clear');assert.equal(schedule.at,4800);
assert.equal(start.map.name,'Pegasus Bridge');assert.equal(start.weather[0],'fog');
function create(module,graphics) {
  gfx.set(graphics);wind.x=.55;wind.z=.25;
  const camera=new THREE.PerspectiveCamera(42,16/9,1,2200),scene=new THREE.Scene();
  const view=module.createAtmosphere({scene,camera,renderer:{getDrawingBufferSize:v=>v.set(1280,720),getPixelRatio:()=>1},cam:{x:100,z:100,y:0}});
  view.start({map:start.map,key:'pegasus-bridge',weather:start.weather,hAt:()=>0,
    ground:new THREE.Mesh(new THREE.PlaneGeometry(start.map.w*2,start.map.h*2)),
    apron:{mesh:{geometry:new THREE.BufferGeometry()},overlayGeometry:new THREE.BufferGeometry()}});
  return view;
}
const summaries=[];
for(const graphics of ['high','low']) {
  const control=create(original,graphics),firstTick=snapshots[0].tick,rows=[];
  let elapsed=0,current=snapshots[0],next=0;
  rows.push({event:'start',start:1,frame:0,engineFrame:1,tick:0,dt:0,renderSeconds:0,graphics,weather:start.weather[0],rain:0,wet:0,
    wind:[.55,.25],fixedClock:false,...control.observe()});
  for(let tick=firstTick;tick<=5200;tick++) {
    while(next<snapshots.length&&snapshots[next].tick<=tick)current=snapshots[next++];
    control.setWeather(current.weather);control.setRain(current.wx[0],current.wx[1]);setWind(current.wx[2],current.wx[3]);
    const dt=Math.fround(.05);control.update(dt);elapsed+=dt;
    rows.push({event:'frame',start:1,frame:rows.length,engineFrame:rows.length+1,tick:current.tick,dt,renderSeconds:elapsed,graphics,
      weather:current.weather[0],rain:current.wx[0],wet:current.wx[1],wind:[wind.x,wind.z],fixedClock:false,...control.observe(),
      view:{start:1,frame:rows.length,engineFrame:rows.length+1,eyeCm:[10000,14000,6000],
        forward:[0,-.8,-.6],right:[1,0,0],up:[0,-.6,.8],hFov:68.621216,focus:[100,100],distance:85,yaw:-90,viewport:[1280,720]}});
  }
  const log=rows.map(r=>'atmos-history '+JSON.stringify({...r,view:undefined})+(r.view?'\natmos-view '+JSON.stringify(r.view):'')).join('\n');
  const plan=nativeWeatherPlan(log),rendered=create(controlled,graphics);
  const before=rendered.world27Begin(rows[0]);assert.equal(before.time,0);let draws=0,maxError=0;
  for(const row of rows.slice(1)) {
    const r=rendered.world27Step(row);draws++; // Mock callback only, never a rendered/GPU claim.
    maxError=Math.max(maxError,...r.look.map((x,i)=>Math.abs(x-row.look[i])));
    assert.equal(r.sourceDt,row.dt);assert.equal(r.sourceSeconds,row.renderSeconds);
  }
  assert.equal(draws,rows.length-1);assert.equal(maxError,0);
  assert.throws(()=>rendered.world27Step(rows.at(-1)),/Missing\/reordered/);
  const clone=create(controlled,graphics);assert.throws(()=>clone.world27Begin({...rows[0],time:1}),/clock\/drift/);
  clone.world27Begin(rows[0]);assert.throws(()=>clone.world27Step({...rows[1],dt:.2}),/cap/);
  assert.throws(()=>clone.world27Step(rows[2]),/Missing\/reordered/);
  assert.throws(()=>nativeWeatherPlan(log.replace('"fixedClock":false','"fixedClock":true')),/clock|constructor/);
  const noView=log.split('\n').filter(l=>!l.startsWith('atmos-view ')).join('\n');assert.throws(()=>nativeWeatherPlan(noView),/camera/);
  const fast=structuredClone(rows);let fastSeconds=0;
  for(const r of fast.slice(1)){r.dt=r.weather==='fog'?.00001:r.dt;fastSeconds+=r.dt;r.renderSeconds=fastSeconds;}
  const fastLog=fast.map(r=>'atmos-history '+JSON.stringify({...r,view:undefined})+(r.view?'\natmos-view '+JSON.stringify(r.view):'')).join('\n');
  assert.throws(()=>nativeWeatherPlan(fastLog),/fast prefix/);
  const script=browserRenderedControl(plan);assert.ok(script.includes('await new Promise(requestAnimationFrame)')&&script.includes('getContext().finish()'));
  summaries.push({graphics,frames:draws,seconds:elapsed,maxLookError:maxError,checkpoints:Object.values(plan.marks),pacing:plan.pacing,
    rejected:['changed constructor clock','delta above cap','missing/reordered frame','held clock','missing camera','fast prefix'],
    verification:'original source plus mock draw callback, synthetic native rows; no browser/engine/GPU'});
}
const connection=readFileSync(join(ref,'client/connection.js'),'utf8');
const connectionPath=join(scratch,'connection-controlled.mjs');writeFileSync(connectionPath,instrumentWorld27('/client/connection.js',connection));
const {createConnection}=await import(pathToFileURL(connectionPath));let socket;
class Socket {
  constructor(){socket=this;this.readyState=1;}
  send(){} close(){this.readyState=3;}
}
const connectionView=createConnection({url:'ws://source-test.invalid',hello:{t:'hello'},WebSocket:Socket});
const received=[];for(const t of ['start','s'])connectionView.on(t,(message,size)=>received.push({message,size}));
connectionView.start();socket.onopen();
const sourceLines=streamBytes.toString().split('\n').filter(Boolean);
const paced=recipientTickSchedule(sourceLines);
assert.equal(paced.at(-1),(snapshots.at(-1).tick-snapshots[0].tick)*1000/20,'recorded pongs never stretch the natural match clock');
const deliveredLines=[sourceLines.find(l=>JSON.parse(l).t==='start'),...sourceLines.filter(l=>JSON.parse(l).t==='s').slice(0,2)];
for(const data of deliveredLines)socket.onmessage({data});
assert.equal(received.length,1,'snapshot delivery waits for its actual controlled frame');
assert.equal(globalThis.__tcWorld27Inbox.length,2);
while(globalThis.__tcWorld27Inbox.length)globalThis.__tcWorld27Inbox.shift().deliver();
assert.deepEqual(received,deliveredLines.map(data=>({message:JSON.parse(data),size:data.length})),'queued source messages preserve order, content and measured wire size');
connectionView.stop();
const result={format:'tc-world27-weather-control-source-test-v1',streamSHA256:createHash('sha256').update(streamBytes).digest('hex'),schedule,summaries};
writeFileSync(opt('out'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify({status:'PASS source scheduler only',lanes:summaries.map(s=>({graphics:s.graphics,frames:s.frames,maxLookError:s.maxLookError}))}));
