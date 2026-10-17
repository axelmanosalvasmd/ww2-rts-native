// Browser half of the recorded Great Bridge replay. The reference files and packets stay unchanged.
// node capture-browser.mjs --origin URL --out PREFIX --profile SCRATCH --block-maps
//   [--native-log game.log] [--visual-clock]
// --visual-clock holds effects at age zero through the normal terrain rebuild, then pauses the page loop
// and calls the reference's effects.update and light.renderFrame at visual ages 0, 1 and 3 s.
import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';

const arg = (name, fallback) => {
  const i = process.argv.indexOf('--' + name);
  return i < 0 ? fallback : process.argv[i + 1];
};
const origin = arg('origin'), out = arg('out'), profile = arg('profile');
const controlled = process.argv.includes('--visual-clock');
if (!origin || !out || !profile || !process.argv.includes('--block-maps')) {
  throw Error('Required: --origin URL --out PREFIX --profile SCRATCH --block-maps');
}
const nativeLog = arg('native-log');
const framing = nativeLog ? [...fs.readFileSync(nativeLog, 'utf8').matchAll(
  /lookfx framing: focus ([\d.-]+) ([\d.-]+) distance ([\d.-]+)/g,
)].map(m=>({x:+m[1],z:+m[2],dist:+m[3]})) : [];
const eyes = nativeLog ? [...fs.readFileSync(nativeLog, 'utf8').matchAll(
  /bridgefx capture: .*?camera X=([\d.-]+) Y=([\d.-]+) Z=([\d.-]+) rotation P=([\d.-]+) Y=([\d.-]+) R=([\d.-]+)/g,
)].map(m => {
  if (Math.abs(+m[4] + 54.43099) > .0001 || +m[5] !== -90 || Math.abs(+m[6]) > .0001) {
    throw Error('Expected the north-up bridge capture camera');
  }
  return [+m[1] / 100, +m[3] / 100, +m[2] / 100];
}) : null;
if (eyes && eyes.length !== 4) throw Error('--native-log must contain all four bridge capture cameras');
fs.mkdirSync(path.dirname(out), { recursive: true });
fs.mkdirSync(profile, { recursive: true });
fs.rmSync(path.join(profile, 'DevToolsActivePort'), { force: true });
const log = o => console.log(JSON.stringify(o));
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const browser = spawn(arg('chromium', '/usr/bin/chromium'), [
  '--headless=new', `--user-data-dir=${profile}`, '--remote-debugging-port=0', '--no-first-run',
  '--no-default-browser-check', '--disable-extensions', '--disable-background-networking',
  '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist',
  '--window-size=1280,720', '--mute-audio', 'about:blank',
], { stdio: ['ignore', 'ignore', 'pipe'] });
let browserErrors = '';
browser.stderr.on('data', chunk => { browserErrors = (browserErrors + chunk).slice(-4000); });
const timeout = setTimeout(() => {
  log({ error: 'timeout' }); browser.kill('SIGTERM'); process.exit(2);
}, 180000);
let ws, next = 1, blocked = 0;
const pending = new Map();
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const id = next++; pending.set(id, { resolve, reject }); ws.send(JSON.stringify({ id, method, params }));
});
const evaluate = async expression => {
  const r = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
  if (r.exceptionDetails) throw Error(JSON.stringify(r.exceptionDetails));
  return r.result.value;
};
try {
  let port;
  for (let i = 0; i < 100; i++) {
    try { port = fs.readFileSync(path.join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0]; break; } catch {}
    await sleep(100);
  }
  if (!port) throw Error('Chromium has no DevTools port: ' + browserErrors);
  const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  ws = new WebSocket(pages.find(p => p.type === 'page').webSocketDebuggerUrl);
  await new Promise(resolve => ws.addEventListener('open', resolve));
  ws.addEventListener('message', e => {
    const m = JSON.parse(e.data);
    if (pending.has(m.id)) {
      const p = pending.get(m.id); pending.delete(m.id);
      m.error ? p.reject(Error(m.error.message)) : p.resolve(m.result); return;
    }
    if (m.method === 'Fetch.requestPaused') {
      blocked++;
      send('Fetch.failRequest', { requestId: m.params.requestId, errorReason: 'BlockedByClient' }).catch(() => {});
    }
    if (m.method === 'Runtime.exceptionThrown') log({ pageError: m.params.exceptionDetails?.exception?.description ?? m.params.exceptionDetails?.text });
  });
  await send('Page.enable'); await send('Runtime.enable');
  await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 720, deviceScaleFactor: 1, mobile: false });
  await send('Page.addScriptToEvaluateOnNewDocument', { source: `if(location.origin===${JSON.stringify(new URL(origin).origin)}) {
    localStorage.setItem('ww2-gfx','high');localStorage.setItem('ww2-gfx-auto','1');
    localStorage.setItem('ww2-controls-seen','1');localStorage.setItem('ww2-name','BridgeWatch');
  }` });
  await send('Fetch.enable', { patterns: [{ urlPattern: '*/maps/*', requestStage: 'Request' }] });
  await send('Page.navigate', { url: origin + '/play?name=BridgeWatch#bridge' });
  for (let i = 0; i < 600; i++) {
    if (await evaluate(`window.__game?.snapshot?.tick === 8398`)) break;
    await sleep(100);
  }
  if (!await evaluate(`window.__game?.snapshot?.tick === 8398`)) throw Error('Replay did not hold tick 8398');
  // Wait for actual rendered frames before reading rig.intro. It may still be pending at the first snapshot.
  await evaluate(`new Promise(done => { let n=0; const f=()=>{ if (++n>=8 && !window.__game.rig.intro) done(true); else requestAnimationFrame(f); }; requestAnimationFrame(f); })`);
  const initialFraming = framing[0] ?? { x: 119, z: 98, dist: 70 };
  await evaluate(`Object.assign(window.__game.cam,${JSON.stringify({ ...initialFraming, yaw: 0 })});true`);
  await sleep(2000);
  log({ ready: true, tick: 8398, graphics: 'high', controlledVisualClock: controlled, matchedNativeCameras: !!eyes });

  if (arg('cloth-out')) {
    const png = await evaluate(`(() => {
      const {scene}=window.__game; let canvas;
      scene.traverse(o=>{for(const m of Array.isArray(o.material)?o.material:[o.material]) {
        const c=m?.map?.image;
        if(m?.isMeshLambertMaterial && c?.width===128 && c?.height===64 && c.toDataURL) canvas=c;
      }});
      if(!canvas) throw Error('Reference flagMat cloth canvas missing');
      return canvas.toDataURL('image/png').split(',')[1];
    })()`);
    fs.writeFileSync(arg('cloth-out'), Buffer.from(png,'base64'));
    log({cloth:arg('cloth-out'),source:'client/markers.js flagMat, original browser canvas'});
  }

  await evaluate(`(async () => {
    const g=window.__game;
    window.__bridgeRender=(await import('/client/light.js')).renderFrame;
    window.__bridgeSoldiers=(await import('/client/unit-models.js')).drawSoldiers;
    window.__bridgeAdvance=g.effects.update.bind(g.effects);
    window.__bridgeVisualAge=0;
    return true;
  })()`);
  // Capture and metadata come from the same render task, before the WebGL drawing buffer can be discarded.
  const capture = async (name, index, targetAge) => {
    const eye = eyes?.[index] ?? null;
    const frame = framing[index] ?? {x:119,z:98,dist:70};
    const result = await evaluate(`(async () => {
      const g=window.__game,targetAge=${targetAge},controlled=${controlled};
      const frame=${JSON.stringify(frame)};
      Object.assign(g.cam,{...frame,yaw:0});
      if (controlled) {
        while(window.__bridgeVisualAge<targetAge) {
          const dt=Math.min(1/60,targetAge-window.__bridgeVisualAge);
          window.__bridgeAdvance(dt);window.__bridgeVisualAge+=dt;
        }
        window.__bridgeAdvance(0);
      }
      const draw=()=>{
        const eye=${JSON.stringify(eye)};
        const y=eye ? eye[1]-frame.dist*Math.sin(.95) : g.hAt(frame.x,frame.z);
        g.cam.y=y;
        g.camera.position.set(...(eye ?? [frame.x,y+frame.dist*Math.sin(.95),frame.z+frame.dist*Math.cos(.95)]));
        g.camera.quaternion.set(-Math.sin(.95/2),0,0,Math.cos(.95/2));
        g.camera.updateMatrixWorld();
        // main.js rebuilds the crowd for the current camera before renderFrame. Do the same after placing
        // a matched capture eye, or detail views can inherit the previous frame's frustum-culling result.
        window.__bridgeSoldiers(g.units.values(),g.camera);
        window.__bridgeRender(g.cam,g.groundMesh);
        let section;
        const lights=[];
        g.scene.traverse(o=>{
          if(o.isInstancedMesh && o.count>0 && o.geometry?.parameters?.width===1.8 && o.geometry?.parameters?.height===2.8)section=o;
          if(o.isDirectionalLight)lights.push({type:'sun',position:o.position.toArray(),target:o.target.position.toArray(),color:o.color.toArray(),intensity:o.intensity});
          if(o.isHemisphereLight)lights.push({type:'hemisphere',top:o.color.toArray(),bottom:o.groundColor.toArray(),intensity:o.intensity});
        });
        return {png:g.renderer.domElement.toDataURL('image/png'),tick:g.snapshot.tick,visualAge:controlled?window.__bridgeVisualAge:null,
          cam:{...g.cam},position:g.camera.position.toArray(),quaternion:g.camera.quaternion.toArray(),fov:g.camera.fov,
          lights,environment:{intensity:g.scene.environmentIntensity,rotation:g.scene.environmentRotation.toArray()},
          particles:g.effects.count,sections:section?{count:section.count,castShadow:section.castShadow,receiveShadow:section.receiveShadow,
          roughness:section.material.roughness,firstMatrix:Array.from(section.instanceMatrix.array.slice(0,16)),firstColor:Array.from(section.instanceColor.array.slice(0,3))}:null};
      };
      return controlled ? draw() : await new Promise(done=>requestAnimationFrame(()=>done(draw())));
    })()`);
    fs.writeFileSync(out + '-' + name + '.png', Buffer.from(result.png.split(',')[1], 'base64'));
    delete result.png;
    log({ shot: name, secondsSinceReceipt: await evaluate(`window.__bridgeReceived ? (performance.now()-window.__bridgeReceived)/1000 : null`), mapsBlocked: blocked, ...result });
    return result;
  };
  const before = await capture('before', 0, 0);
  if (before.tick !== 8398) throw Error('Before frame is not tick 8398');
  await evaluate(`(() => {
    const g=window.__game;
    g.effects.update=dt=>{
      if(g.snapshot.tick>=8400 && !window.__bridgeReceived)window.__bridgeReceived=performance.now();
      return window.__bridgeAdvance(${controlled} && g.snapshot.tick>=8400 ? 0 : dt);
    };
    return true;
  })()`);
  await fetch(origin + '/replay/release', { method: 'POST' });
  for (let i = 0; i < 600; i++) {
    if (await evaluate(`window.__game.snapshot.tick === 8400`)) break;
    await sleep(50);
  }
  if (!await evaluate(`window.__game.snapshot.tick === 8400`)) throw Error('Replay did not release tick 8400');
  // applyCells queues the reference's structure rebuild for terrainFrame. Keep the real frame loop alive
  // through that work, with only effects frozen, before driving the presentation clock ourselves.
  if (controlled) {
    await evaluate(`new Promise(done=>{
      let n=0;
      const prepared=()=>{
        if(++n<4)requestAnimationFrame(prepared);
        else {window.__game.renderer.setAnimationLoop(null);done(true);}
      };
      requestAnimationFrame(prepared);
    })`);
  }
  for (const [name, index, age] of [['at',1,0],['after1s',2,1],['after3s',3,3]]) {
    if (!controlled) {
      for (;;) { if (await evaluate(`(performance.now()-window.__bridgeReceived)/1000 >= ${age}`)) break; await sleep(20); }
    }
    const shot = await capture(name, index, age);
    if (shot.tick !== 8400 || shot.sections?.count !== 84) throw Error('Missing tick-8400 sections');
  }
} finally {
  clearTimeout(timeout); ws?.close();
  if (browser.exitCode === null && browser.signalCode === null) {
    const exited = new Promise(resolve => browser.once('exit', resolve));
    browser.kill('SIGTERM'); await exited;
  }
}
