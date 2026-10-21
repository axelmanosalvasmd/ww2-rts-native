// Lead-only rendered control. This module never edits the pinned reference.
// A served diagnostic copy queues unchanged snapshots and calls the original
// atmosphere setters/update once per measured native delta, drawing every step.
// Usage with an already owned Chromium page and the served-copy hook:
// node <this> --native-log <log> --stream <jsonl> --origin http://127.0.0.1:<port>
//   --room <replay-room> --cdp ws://127.0.0.1:<port>/devtools/page/<id> --out <new-dir>
// --prepare-only validates inputs and writes the frame plan without a browser.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, appendFileSync, mkdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
export const world27Kind='rendered-weather';

export function recipientTickSchedule(lines) {
  const messages=lines.map(l=>JSON.parse(l)),first=messages.find(m=>m.t==='s');
  assert.ok(first&&Number.isSafeInteger(first.tick),'recorded original snapshot clock required');
  let tick=first.tick;
  return messages.map(m=>{
    if(m.t==='s') {assert.ok(Number.isSafeInteger(m.tick)&&m.tick>=tick,'ordered recipient ticks required');tick=m.tick;}
    // Non-snapshot rows, including recorded pongs, cost no extra match time.
    return (tick-first.tick)*1000/20;
  });
}

export function instrumentWorld27(path, source) {
  const replace=(a,b)=>{assert.equal(source.split(a).length,2,'pinned served-copy seam: '+a);source=source.replace(a,b);};
  if(path==='/client/connection.js') {
    replace("      emit(message.t, message, event.data.length);", `      if (message.t === 's') {
        (globalThis.__tcWorld27Inbox ??= []).push({ tick: message.tick, deliver: () => emit(message.t, message, event.data.length) });
        return; // Unchanged recipient bytes are delivered at their recorded native frame.
      }
      emit(message.t, message, event.data.length);`);
    return source;
  }
  if(path!=='/client/atmosphere.js') return source;
  assert.ok(source.includes('t = (t + dt) % 10000;'),'natural atmosphere clock required, no fixed clock transform');
  replace('  return {\n    start, update, setWeather,', `  let tcStarts = 0, tcArgs = null, tcFirst = null, tcFrames = 0, tcSeconds = 0, tcDriving = false;
  const tcState = () => ({ weather: wxNow, time: t, drift: drift.toArray(), look: EASED.map(k => cur[k]),
    goal: EASED.map(k => goal[k]), haze: hazeNow.toArray(), hazeGoal: hazeGoal.toArray(),
    fall: fall?.userData.kind ?? '', fallAlpha: fallA, banksVisible: banks.visible, bankOpacity: banks.material.uniforms.uOpacity.value });
  return {
    start(args) { tcArgs = args; tcStarts++; start(args); },
    update() {}, // No unrecorded animation-loop delta before the control starts.
    setWeather(row) { if (tcDriving) setWeather(row); },
    world27Ready: () => tcStarts === 1 && !!ground,
    world27Begin(first) {
      if (tcStarts !== 1 || tcFirst || first.event !== 'start' || first.frame !== 0 || first.start !== 1 || first.fixedClock)
        throw Error('A fresh original constructor and exactly one native start are required');
      if (first.graphics !== (gfx.low ? 'low' : 'high') || tcArgs.weather?.[0] !== first.weather)
        throw Error('Original constructor graphics/weather differ');
      if (Math.abs(t - first.time) > 1e-12 || first.drift.some((v,i) => Math.abs(v-drift.toArray()[i])>1e-12))
        throw Error('Constructor clock/drift differ; never set them to the endpoint');
      tcFirst = first; return tcState();
    },
    world27Step(row) {
      if (!tcFirst || row.event !== 'frame' || row.start !== tcFirst.start || row.frame !== tcFrames+1 || row.fixedClock || row.graphics !== tcFirst.graphics)
        throw Error('Missing/reordered frame, different tier, restart or held clock');
      if (!Number.isFinite(row.dt) || row.dt<0 || row.dt>0.1) throw Error('Delta is outside the original normal-frame cap; never clamp it silently');
      if (!Object.hasOwn(LOOK,row.weather) || ![row.rain,row.wet,...row.wind].every(Number.isFinite)) throw Error('Invalid original weather input');
      tcDriving = true;
      try { this.setWeather([row.weather]); this.setRain(row.rain,row.wet); wind.x=row.wind[0]; wind.z=row.wind[1]; update(row.dt); }
      finally { tcDriving = false; }
      tcFrames++; tcSeconds+=row.dt;
      if (Math.abs(tcSeconds-row.renderSeconds)>1e-8 || Math.abs(t-row.time)>1e-8) throw Error('Consumed history or natural clock does not match');
      return {frame:tcFrames,sourceDt:row.dt,nativeDt:row.dt,sourceSeconds:tcSeconds,nativeSeconds:row.renderSeconds,...tcState()};
    },`);
  replace('    setRain(rain, wet) {','    setRain(rain, wet) {\n      if (!tcDriving) return;');
  return source;
}

export function nativeWeatherPlan(log) {
  const parse=tag=>log.split('\n').filter(l=>l.includes(tag+' ')).map(l=>JSON.parse(l.slice(l.indexOf(tag+' ')+tag.length+1)));
  const rows=parse('atmos-history'), views=parse('atmos-view');
  assert.ok(rows.length>1&&rows[0].event==='start'&&rows[0].frame===0&&rows[0].start===1&&!rows[0].fixedClock,'complete history from one natural constructor without a held clock is required');
  assert.equal(rows.filter(r=>r.event==='start').length,1,'restarts need their preceding constructor history');
  const first=rows[0], byFrame=new Map();
  for(const v of views) { const key=[v.start,v.frame,v.engineFrame].join('/');assert.ok(!byFrame.has(key),'one camera receipt per frame');byFrame.set(key,v); }
  let seconds=0, previousTick=-1, previousEngine=-1;
  for(let i=1;i<rows.length;i++) {
    const r=rows[i];
    assert.ok(r.event==='frame'&&r.start===first.start&&r.frame===i&&!r.fixedClock,'no missing frames, restarts or fixed clock');
    assert.ok(Number.isFinite(r.dt)&&r.dt>=0&&r.dt<=.1,'native delta exceeds original normal-frame cap; retain as caller diagnostic');
    assert.equal(r.graphics,first.graphics,'one explicit graphics tier throughout');
    assert.ok(Number.isSafeInteger(r.engineFrame)&&r.engineFrame>previousEngine&&r.tick>=previousTick,'ordered engine frames and recipient ticks');
    previousTick=r.tick;previousEngine=r.engineFrame;seconds+=r.dt;
    assert.ok(Math.abs(seconds-r.renderSeconds)<1e-8,'measured accumulated dt covers every native row');
    r.view=byFrame.get([r.start,r.frame,r.engineFrame].join('/'));assert.ok(r.view,'missing camera receipt, do not substitute a final pose');
    for(const k of ['eyeCm','forward','right','up'])assert.ok(r.view[k]?.length===3&&r.view[k].every(Number.isFinite),'finite complete view');
    assert.ok(r.view.hFov>0&&r.view.hFov<180&&r.view.focus?.every(Number.isFinite)&&r.view.distance>0,'public RTS camera receipt required');
    assert.ok(r.view.viewport?.length===2&&r.view.viewport.every(v=>Number.isSafeInteger(v)&&v>0),'actual native viewport required');
  }
  assert.equal(first.weather,'fog','Pegasus natural constructor starts Fog');
  const clear=rows.findIndex((r,i)=>i&&r.weather==='clear');assert.ok(clear>1,'actual Fog to Clear transition required');
  assert.equal(rows[clear].tick,4800,'unaltered Pegasus constructor-derived lift tick');
  const beginning=rows.find(r=>r.event==='frame'&&r.tick>0);assert.ok(beginning);
  const paceError=(rows[clear].renderSeconds-beginning.renderSeconds)-(rows[clear].tick-beginning.tick)/20;
  assert.ok(Math.abs(paceError)<=2,'fast prefix or unpaced native input is not natural rendered history (2 s pacing tolerance)');
  const marks={};
  for(const [tag,tick] of [['before',300],['warning',4600]]) { const r=rows.find(r=>r.event==='frame'&&r.tick>=tick&&r.weather==='fog');assert.ok(r,tag+' control absent');marks[r.frame]=tag; }
  marks[rows[clear-1].frame]='last-fog'; marks[rows[clear].frame]='first-clear';
  for(const age of [1,3,8,14,20]) { const r=rows.find(r=>r.event==='frame'&&r.renderSeconds>=rows[clear].renderSeconds+age);assert.ok(r,'complete measured +'+age+' s history required');marks[r.frame]='after-'+age+'s'; }
  return {rows,marks,graphics:first.graphics,frames:rows.length-1,seconds,pacing:{firstTick:beginning.tick,liftTick:4800,secondsError:paceError,tolerance:2},
    nativeGpuReadbackCorrelated:false,limits:'Native rows and camera receipts observe CPU input. Correlate actual native screenshot callbacks separately.'};
}

export function browserRenderedControl(plan) {
  // Serialised function contains no Node or workspace access.
  return `(${async function(plan) {
    const g=window.__game,{gfx}=await import('/client/gfx.js'),THREE=await import('three');
    const {renderFrame}=await import('/client/light.js');
    if(gfx.level!==plan.graphics||localStorage.getItem('ww2-gfx')!==plan.graphics)throw Error('Set graphics explicitly before construction');
    if(!g?.atmos?.world27Ready())throw Error('Served original constructor is not ready');
    g.renderer.setAnimationLoop(null);g.rig.skipIntro();
    g.renderer.domElement.setAttribute('data-world27-main','1');
    const style=document.createElement('style');style.textContent='body * { visibility:hidden !important; } canvas[data-world27-main] { visibility:visible !important; }';document.head.appendChild(style);
    const initial=g.atmos.world27Begin(plan.rows[0]);
    const change=(v,scale=1)=>new THREE.Vector3(v[0]*scale,v[2]*scale,v[1]*scale);
    let delivered=0, rendered=0;
    for(const row of plan.rows.slice(1)) {
      await new Promise(requestAnimationFrame);
      const q=window.__tcWorld27Inbox??[];
      while(q[0]?.tick<=row.tick) {q.shift().deliver();delivered++;}
      if(row.tick>0&&g.snapshot?.tick!==row.tick)throw Error('The intended recipient tick has not occurred');
      const view=row.view;
      if(innerWidth!==view.viewport[0]||innerHeight!==view.viewport[1])throw Error('Source/native viewport differs');
      Object.assign(g.cam,{x:view.focus[0],z:view.focus[1],y:g.hAt(...view.focus),dist:view.distance,yaw:-(view.yaw+90)*Math.PI/180});
      g.camera.position.copy(change(view.eyeCm,.01));
      g.camera.quaternion.setFromRotationMatrix(new THREE.Matrix4().makeBasis(change(view.right),change(view.up),change(view.forward).negate()));
      g.camera.fov=2*Math.atan(Math.tan(view.hFov*Math.PI/360)/(innerWidth/innerHeight))*180/Math.PI;
      g.camera.updateProjectionMatrix();g.camera.updateMatrixWorld();
      const state=g.atmos.world27Step(row);g.apron?.update();
      renderFrame(g.cam,g.groundMesh);g.renderer.getContext().finish();rendered++;
      if(!(g.renderer.info.render.calls>0))throw Error('No original scene draw occurred');
      const tag=plan.marks[row.frame]??null;
      const receipt={kind:'world27-rendered-weather',...state,tag,engineFrame:row.engineFrame,tick:row.tick,
        sourceDraw:rendered,deliveredSnapshots:delivered,drawCalls:g.renderer.info.render.calls,
        lookMaxError:Math.max(...state.look.map((v,i)=>Math.abs(v-row.look[i]))),
        hazeMaxError:Math.max(...state.haze.map((v,i)=>Math.abs(v-row.haze[i]))),
        nativeBanks:row.look[8],nativeBanksVisible:row.banksVisible,
        sourceEye:g.camera.position.toArray(),nativeView:view,nativeGpuReadbackCorrelated:false};
      if(tag) {
        receipt.png=g.renderer.domElement.toDataURL('image/png');
        // Keep this exact source frame held until CDP reads its CSS presentation,
        // including the original Low canvas upscale, then acknowledge explicitly.
        await new Promise(resolve=>{window.__tcWorld27Continue=resolve;window.__tcWorld27Receipt(JSON.stringify(receipt));});
      } else window.__tcWorld27Receipt(JSON.stringify(receipt));
    }
    return {initial,rendered,delivered,sourceRenderedThroughout:true,nativeGpuReadbackCorrelated:false,
      limits:'Matched atmosphere dt and per-frame public camera, unchanged ordered snapshots. Other game animation, fog/water clocks and compositor history need separate qualification. No whole-match or native GPU acceptance.'};
  }.toString()})(${JSON.stringify(plan)})`;
}

async function main() {
  const opt=n=>{const i=process.argv.indexOf('--'+n);return i<0?null:process.argv[i+1];};
  for(const key of ['native-log','stream','out'])assert.ok(opt(key),'--'+key+' is required');
  const out=resolve(opt('out'));mkdirSync(out,{recursive:false});
  const sha=p=>createHash('sha256').update(readFileSync(p)).digest('hex');
  const plan=nativeWeatherPlan(readFileSync(opt('native-log'),'utf8'));
  const messages=readFileSync(opt('stream'),'utf8').split('\n').filter(Boolean).map(JSON.parse),start=messages.find(m=>m.t==='start');
  assert.ok(start?.map?.name==='Pegasus Bridge'&&!start.map.world,'the unchanged public Pegasus recipient is required');
  assert.equal(start.weather[0],plan.rows[0].weather);
  const snapshots=messages.filter(m=>m.t==='s'),ticks=new Set([0,...snapshots.map(s=>s.tick)]);
  assert.ok(plan.rows.slice(1).every(r=>ticks.has(r.tick)),'every native held tick must exist in the unchanged recipient');
  const width=Number(opt('width')??1280),height=Number(opt('height')??720),lang=opt('lang')??'en',ui=Number(opt('ui')??100);
  assert.ok([[1280,720],[1920,1080]].some(p=>p[0]===width&&p[1]===height)&&['en','es'].includes(lang)&&[100,150].includes(ui),'supported viewport/language/UI control required');
  const meta={status:'prepared only',reference:'e4e579a838aabae3b1d9dadbc17a0a87147bc52d',graphics:plan.graphics,viewport:[width,height],lang,ui,
    streamSHA256:sha(opt('stream')),nativeLogSHA256:sha(opt('native-log')),toolSHA256:sha(fileURLToPath(import.meta.url)),
    frames:plan.frames,seconds:plan.seconds,marks:plan.marks,nativeGpuReadbackCorrelated:false};
  writeFileSync(join(out,'plan.json'),JSON.stringify(plan)+'\n');writeFileSync(join(out,'manifest.json'),JSON.stringify(meta,null,2)+'\n');
  writeFileSync(join(out,'browser-control.js'),browserRenderedControl(plan));
  if(process.argv.includes('--prepare-only')) {console.log(JSON.stringify(meta));return;}
  for(const key of ['origin','room','cdp'])assert.ok(opt(key),'--'+key+' is required for rendering');
  const origin=new URL(opt('origin')),cdp=new URL(opt('cdp'));
  assert.ok(origin.protocol==='http:'&&['127.0.0.1','localhost'].includes(origin.hostname),'local replay origin only');
  assert.ok(cdp.protocol==='ws:'&&['127.0.0.1','localhost'].includes(cdp.hostname),'owned local page only');
  const ws=new WebSocket(cdp),pending=new Map();let id=0,blocked=0,receipts=0;
  await new Promise((ok,no)=>{ws.addEventListener('open',ok,{once:true});ws.addEventListener('error',no,{once:true});});
  const send=(method,params={})=>new Promise((ok,no)=>{pending.set(++id,{ok,no});ws.send(JSON.stringify({id,method,params}));});
  const evaluate=async expression=>{const r=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});if(r.exceptionDetails)throw Error(r.exceptionDetails.exception?.description??r.exceptionDetails.text);return r.result.value;};
  ws.addEventListener('message',async event=>{
    try {
      const m=JSON.parse(event.data);
      if(m.id) {const p=pending.get(m.id);if(p){pending.delete(m.id);m.error?p.no(Error(m.error.message)):p.ok(m.result);}return;}
      if(m.method==='Fetch.requestPaused') {blocked++;send('Fetch.failRequest',{requestId:m.params.requestId,errorReason:'BlockedByClient'}).catch(()=>{});}
      if(m.method==='Runtime.bindingCalled'&&m.params.name==='__tcWorld27Receipt') {
        const r=JSON.parse(m.params.payload);receipts++;
        if(r.png) {
          const file=join(out,r.tag+'-source-canvas.png');writeFileSync(file,Buffer.from(r.png.split(',')[1],'base64'));delete r.png;r.canvasSHA256=sha(file);
          const display=await send('Page.captureScreenshot',{format:'png'}),shown=join(out,r.tag+'-source.png');
          writeFileSync(shown,Buffer.from(display.data,'base64'));r.imageSHA256=sha(shown);r.cssPresentationReadback=true;
        }
        appendFileSync(join(out,'source-frames.jsonl'),JSON.stringify(r)+'\n');
        if(r.tag)await evaluate('window.__tcWorld27Continue(); true');
      }
    } catch(error) {
      for(const p of pending.values())p.no(error);pending.clear();ws.close();
    }
  });
  try {
    await send('Page.enable');await send('Runtime.enable');await send('Runtime.addBinding',{name:'__tcWorld27Receipt'});
    await send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
    await send('Page.navigate',{url:origin.href});
    const wait=async fn=>{for(let n=0;n<1200;n++){try{if(await evaluate(fn))return;}catch{}await new Promise(r=>setTimeout(r,100));}throw Error('source readiness timeout');};
    await wait('document.readyState === "complete"');
    await evaluate(`localStorage.setItem('ww2-gfx',${JSON.stringify(plan.graphics)});localStorage.setItem('ww2-gfx-auto','1');localStorage.setItem('ww2-controls-seen','1');localStorage.setItem('ww2rts-lang',${JSON.stringify(lang)});localStorage.setItem('ww2-ui-scale',${JSON.stringify(String(ui))});true`);
    await send('Fetch.enable',{patterns:[{urlPattern:'*/maps/*',requestStage:'Request'}]});
    await send('Page.navigate',{url:new URL('/play?name=World27Observer#'+encodeURIComponent(opt('room')),origin).href});
    await wait(`!!window.__game?.atmos?.world27Ready?.() && (window.__tcWorld27Inbox?.at(-1)?.tick??-1)>=${plan.rows.at(-1).tick}`);
    const result=await evaluate(browserRenderedControl(plan));
    assert.equal(receipts,plan.frames,'every consumed native row produced a source draw receipt');
    writeFileSync(join(out,'result.json'),JSON.stringify({...result,blockedMapRequests:blocked,receipts},null,2)+'\n');
    writeFileSync(join(out,'manifest.json'),JSON.stringify({...meta,status:'source rendered history completed; native GPU callback correlation remains open',receipts,sourceRenderedThroughout:true},null,2)+'\n');
    console.log(JSON.stringify({frames:receipts,sourceRenderedThroughout:true,nativeGpuReadbackCorrelated:false}));
  } catch(error) {
    writeFileSync(join(out,'failure.json'),JSON.stringify({status:'failed control, retain this try',message:String(error),receipts,nativeGpuReadbackCorrelated:false},null,2)+'\n');
    throw error;
  } finally {ws.close();}
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href) await main();
