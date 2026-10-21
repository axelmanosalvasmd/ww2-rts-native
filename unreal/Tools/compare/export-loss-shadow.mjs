// Convert the pinned original CSS loss shadow. No art is redrawn and no game server is started.
// node export-loss-shadow.mjs <reference> <scratch> <TCEndReport.cpp> [--check]
// Set TMPDIR to a short permitted scratch directory when the output path is deeply nested.
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import {spawn,execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
const [referenceArg,scratchArg,cppArg,...flags]=process.argv.slice(2);
if(!referenceArg||!scratchArg||!cppArg||flags.some(f=>f!=='--check'))throw Error('reference, scratch and C++ target required, optional --check');
const reference=path.resolve(referenceArg),scratch=path.resolve(scratchArg),cpp=path.resolve(cppArg);
const pin=execFileSync('git',['-C',reference,'rev-parse','HEAD'],{encoding:'utf8'}).trim();
assert.equal(pin,'e4e579a838aabae3b1d9dadbc17a0a87147bc52d','wrong source pin');
fs.mkdirSync(scratch,{recursive:true});
const profile=fs.mkdtempSync(path.join(scratch,'shadow-profile-'));
const index=fs.readFileSync(path.join(reference,'client/index.html'),'utf8');
const alerts=fs.readFileSync(path.join(reference,'client/alerts.css'),'utf8');
const originalStyle=index.match(/<style>([\s\S]*?)<\/style>/)?.[1];
assert.ok(originalStyle,'source index style required');
const html='<!doctype html><html><head><style>'+originalStyle+'\n'+alerts+'</style></head><body><div id="hud"><div id="alerts"><div id="lossAlerts"><div class="alert"></div></div></div></div></body></html>';
const server=http.createServer((req,res)=>{res.setHeader('Content-Type','text/html');res.end(html);});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const log=fs.createWriteStream(path.join(scratch,'chromium.log'));
const browser=spawn(process.env.CHROMIUM??'/usr/bin/chromium',['--headless=new','--ozone-platform=headless',`--user-data-dir=${profile}`,'--remote-debugging-port=0','--no-first-run','--no-default-browser-check','--disable-extensions','--disable-background-networking','about:blank'],{stdio:['ignore','ignore','pipe'],env:{...process.env,TMPDIR:process.env.TMPDIR??scratch}});
browser.stderr.pipe(log);
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
let ws,id=0;const pending=new Map();
const send=(method,params={})=>new Promise((resolve,reject)=>{const n=++id;pending.set(n,{resolve,reject});ws.send(JSON.stringify({id:n,method,params}));});
const evaluate=async expression=>{const r=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
try{
 let port;
 for(let i=0;i<100;i++){
  try{port=fs.readFileSync(path.join(profile,'DevToolsActivePort'),'utf8').split('\n')[0];break;}catch{}
  if(browser.exitCode!==null)throw Error('own Chromium exited '+browser.exitCode);
  await sleep(50);
 }
 if(!port)throw Error('own Chromium did not start');
 const pages=await(await fetch(`http://127.0.0.1:${port}/json/list`)).json();
 ws=new WebSocket(pages.find(p=>p.type==='page').webSocketDebuggerUrl);await new Promise(r=>ws.addEventListener('open',r,{once:true}));
 ws.addEventListener('message',e=>{const m=JSON.parse(e.data),p=pending.get(m.id);if(p){pending.delete(m.id);m.error?p.reject(Error(m.error.message)):p.resolve(m.result);}});
 const version=await send('Browser.getVersion');
 await send('Emulation.setDeviceMetricsOverride',{width:192,height:192,deviceScaleFactor:1,mobile:false});
 await send('Page.navigate',{url:`http://127.0.0.1:${server.address().port}/`});await sleep(100);
 await send('Emulation.setDefaultBackgroundColorOverride',{color:{r:0,g:0,b:0,a:0}});
 const rule=await evaluate(`(()=>{
  document.documentElement.style.background='transparent';document.body.style.background='transparent';
  document.getElementById('hud').style.cssText='position:absolute;inset:0;zoom:1;pointer-events:none';
  document.getElementById('alerts').style.cssText='position:absolute;left:0;top:0;display:block;width:192px;max-width:none';
  document.getElementById('lossAlerts').style.cssText='position:static;display:block;width:192px;margin:0;max-width:none';
  const card=document.querySelector('.alert'),style=getComputedStyle(card),rule={shadow:style.boxShadow,radius:style.borderRadius};
  card.style.cssText='position:absolute;left:32px;top:32px;width:128px;height:128px;max-width:none;box-sizing:border-box;padding:0;margin:0;background:transparent;border-color:transparent;animation:none';
  const b=card.getBoundingClientRect();return {...rule,box:[b.x,b.y,b.width,b.height]};
 })()`);
 assert.equal(rule.shadow,'rgba(0, 0, 0, 0.4) 0px 3px 14px 0px');assert.equal(rule.radius,'2px');assert.deepEqual(rule.box,[32,32,128,128]);
 const capture=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});
 const png=Buffer.from(capture.data,'base64'),sha=createHash('sha256').update(png).digest('hex');
 fs.writeFileSync(path.join(scratch,'loss-shadow.png'),png);
 const text=fs.readFileSync(cpp,'utf8');
 const region=/ \/\/ BEGIN GENERATED SOURCE LOSS SHADOW e4e579a[\s\S]*? \/\/ END GENERATED SOURCE LOSS SHADOW/;
 assert.ok(region.test(text),'generated source region is missing');
 const generated=` // BEGIN GENERATED SOURCE LOSS SHADOW e4e579a\n // alerts.css: 0 3px 14px rgba(0,0,0,.4), radius 2px. Original Chromium raster, transparent fill/border.\n // PNG sha256: ${sha}\n const TCHAR* SourceLossShadowPng = TEXT("${png.toString('base64')}");\n // END GENERATED SOURCE LOSS SHADOW`;
 const updated=text.replace(region,generated),check=flags.includes('--check');
 if(check)assert.equal(updated,text,'source shadow differs, regenerate with the pinned Chromium version');else fs.writeFileSync(cpp,updated);
 const hashes=Object.fromEntries(['client/index.html','client/alerts.css'].map(f=>[f,createHash('sha256').update(fs.readFileSync(path.join(reference,f))).digest('hex')]));
 const result={pin,sourceHashes:hashes,chromium:version.product,rule,description:'Original outer shadow only, transparent source fill and border. Eight exterior tiles, center omitted.',cuts:[0,40,152,192],sha256:sha,check,passed:true};
 fs.writeFileSync(path.join(scratch,'result.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));
}finally{
 ws?.close();if(browser.exitCode===null){browser.kill('SIGTERM');await Promise.race([new Promise(r=>browser.once('exit',r)),sleep(2000)]);if(browser.exitCode===null)browser.kill('SIGKILL');}
 await new Promise(r=>server.close(r));log.end();fs.rmSync(profile,{recursive:true,force:true});
}
