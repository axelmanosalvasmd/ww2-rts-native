import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import assert from 'node:assert/strict';
import { instrumentAtmosphereHistory, atmosphereHistoryRows } from './history-source-transform.mjs';
import { historyThroughRequest } from './history-prefix.mjs';
const repo = process.cwd(), ref = join(repo, 'reference/ww2-rts');
const evidence = resolve(process.argv[2]), scratch = resolve(process.argv[3]);
assert.ok(evidence && scratch); mkdirSync(scratch, { recursive: true });
const { installShims, settle } = await import(pathToFileURL(join(repo, 'unreal/Tools/models/shims.mjs')));
installShims(); globalThis.localStorage = { getItem: () => null, setItem: () => {} }; globalThis.location = { search: '' };
const THREE = await import('three');
const { gfx } = await import(pathToFileURL(join(ref, 'client/gfx.js')));
const { wind } = await import(pathToFileURL(join(ref, 'client/wind.js')));
const raw = readFileSync(join(ref, 'client/atmosphere.js'), 'utf8');
let source = instrumentAtmosphereHistory(raw).replace("import { setMood } from './light.js';", 'const setMood = () => {};');
for (const p of ['gfx.js', 'moods.js', 'wind.js']) source = source.replace(`from './${p}'`, `from '${pathToFileURL(join(ref, 'client', p))}'`);
source = source.replace("from '../shared/weather.js'", `from '${pathToFileURL(join(ref, 'shared/weather.js'))}'`);
const file = join(scratch, 'history-controlled-source.mjs'); writeFileSync(file, source);
const { createAtmosphere } = await import(pathToFileURL(file)); await settle();
const map = JSON.parse(readFileSync(join(ref, 'maps/pegasus-bridge.json'), 'utf8'));
function create(height = () => 0) {
  gfx.set('high'); wind.x = 0.55; wind.z = 0.25;
  const at = createAtmosphere({ scene: new THREE.Scene(), renderer: { getDrawingBufferSize: v => v.set(1280,720), getPixelRatio: () => 1 },
    camera: new THREE.PerspectiveCamera(42,16/9,1,900), cam: { x: map.w, y: 0, z: map.h } });
  at.start({ map, key: 'pegasus-bridge', weather: ['fog'], hAt: height,
    ground: new THREE.Mesh(new THREE.PlaneGeometry(map.w*2,map.h*2)),
    apron: { overlayGeometry: new THREE.BufferGeometry(), mesh: { geometry: new THREE.BufferGeometry() } } });
  return at;
}
const dt = Math.fround(.05), rows = [{ event:'start', start:1, frame:0, time:0, drift:[0,0], weather:'fog', graphics:'high' }];
let seconds=0;
for(let i=1;i<=161;i++) { seconds+=dt; rows.push({event:'frame',start:1,frame:i,dt,weather:'clear',graphics:'high',rain:0,wet:0,wind:[.55,.25],fixedClock:false,renderSeconds:seconds}); }
const passed=[];
function check(name, fn) { fn(); passed.push(name); }
const at=create();
at.setWeather(['clear']); for(let i=0;i<50;i++)at.update(.1); // Automatic updates are held in this comparison copy.
check('unrecorded automatic frames are held',()=>assert.equal(at.historyState().look[8],1));
const result=at.replayHistory(rows);
const oracle=JSON.parse(readFileSync(join(evidence,'weather-history-source.json'),'utf8')).rows.find(r=>r.caseIndex===0&&r.tick===4960);
check('original look matches the controlled source oracle',()=>assert.ok(Math.abs(result.look[8]-oracle.look[8])<1e-12));
check('particle time follows consumed dt',()=>assert.ok(Math.abs(result.time-result.seconds)<1e-12));
check('repeat replay rejected',()=>assert.throws(()=>at.replayHistory(rows),/Fresh source/));
check('missing start rejected',()=>assert.throws(()=>create().replayHistory(rows.slice(1)),/Complete start/));
check('missing frame rejected',()=>assert.throws(()=>create().replayHistory([rows[0],...rows.slice(2)]),/Missing\/reordered/));
check('held clock rejected',()=>assert.throws(()=>create().replayHistory([rows[0],{...rows[1],fixedClock:true}]),/frozen/));
check('elapsed time mismatch rejected',()=>assert.throws(()=>create().replayHistory([rows[0],{...rows[1],renderSeconds:99}]),/Consumed dt/));
check('guessed constructor age rejected',()=>assert.throws(()=>create().replayHistory([{...rows[0],time:37},rows[1]]),/Constructor clock/));
check('frozen source transform rejected',()=>assert.throws(()=>instrumentAtmosphereHistory(raw.replace('t = (t + dt) % 10000;','t = 37;')),/natural constructor/));
check('raw native log parser',()=>assert.deepEqual(atmosphereHistoryRows('log prefix atmos-history '+JSON.stringify(rows[0])+'\n'),[rows[0]]));
check('missing elapsed time rejected',()=>assert.throws(()=>create().replayHistory([rows[0],{...rows[1],renderSeconds:undefined}]),/Invalid frame input/));
check('non-finite elapsed time rejected',()=>assert.throws(()=>create().replayHistory([rows[0],{...rows[1],renderSeconds:NaN}]),/Invalid frame input/));
check('missing shower input rejected',()=>assert.throws(()=>create().replayHistory([rows[0],{...rows[1],rain:undefined}]),/Invalid frame input/));
check('unknown weather rejected',()=>assert.throws(()=>create().replayHistory([rows[0],{...rows[1],weather:'invented'}]),/Invalid frame input/));
check('wrong graphics lane rejected',()=>assert.throws(()=>create().replayHistory([rows[0],{...rows[1],graphics:'low'}]),/graphics lane/));
check('replay preserves source sites constructed before terrain updates',()=>{
  let height=0;const view=create(()=>height), initial=view.historyState().bankSiteHeights;
  height=10;view.replayHistory(rows);assert.deepEqual(view.historyState().bankSiteHeights,initial);
});
check('a second presentation start requires a separate history epoch',()=>{
  const view=create();view.start({map,key:'pegasus-bridge',weather:['fog'],hAt:()=>0,
    ground:new THREE.Mesh(new THREE.PlaneGeometry(map.w*2,map.h*2)),
    apron:{overlayGeometry:new THREE.BufferGeometry(),mesh:{geometry:new THREE.BufferGeometry()}}});
  assert.throws(()=>view.replayHistory(rows),/Fresh source/);
});
check('constructor weather mismatch rejected',()=>assert.throws(()=>create().replayHistory([{...rows[0],weather:'clear'},rows[1]]),/constructor weather/));
const requestLog=rows.map((r,i)=>'LogTC: atmos-history '+JSON.stringify({...r,engineFrame:100+i,tick:4800+i})).join('\n')+
  '\nLogTC: tour: /evidence/pegasus-after.png: target (90,100) at match time 248.0 s\n';
check('request endpoint keeps the complete history and declines GPU correlation',()=>{
  const selected=historyThroughRequest(requestLog,'pegasus-after');assert.equal(selected.rows.length,162);
  assert.equal(selected.endpoint.engineFrame,261);assert.equal(selected.endpoint.gpuReadbackCorrelated,false);
});
check('missing intended screenshot request rejected',()=>assert.throws(()=>historyThroughRequest(requestLog,'absent'),/one exact native screenshot/));
check('multiple starts are not silently sliced away',()=>assert.throws(()=>historyThroughRequest('LogTC: atmos-history '+JSON.stringify(rows[0])+'\n'+requestLog,'pegasus-after'),/multiple starts/));
check('native deltas beyond the source loop cap remain diagnostic',()=>assert.throws(()=>create().replayHistory([rows[0],{...rows[1],dt:.2,renderSeconds:.2}]),/source normal-frame cap/));
const report={sourceOnly:true,checks:passed.length,passed,frames:result.frames,consumedSeconds:result.seconds,banks:result.look[8],
  sourceOracleBanks:oracle.look[8],incompleteHistoryRejected:true,constructorClockNeverAssigned:true,
  limits:'Executed unchanged original atmosphere update bodies in an instrumented comparison copy under Node. No browser GPU or native engine run.'};
writeFileSync(join(evidence,'history-source-transform-check.json'),JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify(report));
