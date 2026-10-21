// Run the pinned markers.js symbolBadge Canvas2D builder, without redrawing its art.
// node export-badges.mjs <isolated-origin> <output-folder> <scratch-profile> <reference-dir>
import fs from 'node:fs';
import path from 'node:path';
import net from 'node:net';
import {createHash} from 'node:crypto';
import {spawn, execFileSync} from 'node:child_process';
const [origin, out, profile, reference] = process.argv.slice(2);
if (!reference || !profile || !out || !origin) throw Error('usage: export-badges.mjs <origin> <out> <scratch-profile> <reference-dir>');
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = (...args) => execFileSync('git', ['-C', reference, ...args], {encoding:'utf8'}).trim();
if (git('rev-parse', 'HEAD') !== pin || git('status', '--porcelain')) throw Error('reference must be clean at '+pin);
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const sources = {};
for (const file of ['client/markers.js', 'client/symbols.js', 'client/main.js', 'shared/sim.js']) {
  const expected = sha(fs.readFileSync(path.join(reference,file)));
  const response = await fetch(origin+'/'+file, {signal:AbortSignal.timeout(5000)});
  if (!response.ok || sha(Buffer.from(await response.arrayBuffer())) !== expected) throw Error('isolated server source differs: '+file);
  sources[file] = expected;
}
fs.mkdirSync(out,{recursive:true});
const port = 9438;
// Refuse to attach to a browser we did not start.
await new Promise((resolve,reject) => {
  const probe = net.createServer(); probe.once('error',reject);
  probe.listen(port,'127.0.0.1',()=>probe.close(resolve));
});
const browser=spawn('/usr/bin/chromium',['--headless=new',`--user-data-dir=${profile}`,`--remote-debugging-port=${port}`,'--no-first-run','--disable-extensions','--disable-background-networking','--use-angle=swiftshader','--enable-unsafe-swiftshader','--ozone-platform=headless','about:blank'],{stdio:'ignore'});
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
let ws,next=1;const pending=new Map();
try {
  for(let i=0;i<80;i++) {
    if (browser.exitCode !== null) throw Error('own browser exited '+browser.exitCode);
    try {
      const rows=await(await fetch(`http://127.0.0.1:${port}/json/list`,{signal:AbortSignal.timeout(1000)})).json();
      const page=rows.find(t=>t.type==='page');
      if (page) { ws=new WebSocket(page.webSocketDebuggerUrl);break; }
    } catch { await sleep(200); }
  }
  if (!ws) throw Error('own browser CDP did not start');
  await new Promise((resolve,reject)=>{ws.addEventListener('open',resolve,{once:true});ws.addEventListener('error',reject,{once:true});});
  ws.addEventListener('message',e=>{const m=JSON.parse(e.data);if(pending.has(m.id)){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result);}});
  const send=(method,params={})=>new Promise((resolve,reject)=>{const id=next++;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params}));});
  const evalJS=async expression=>{const r=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
  await send('Page.navigate',{url:origin+'/play'});
  for(let i=0;i<80;i++){if(await evalJS("document.readyState==='complete'"))break;await sleep(100);}
  const result=await evalJS(`(async()=>{
    const {symbolBadge}=await import('/client/markers.js');const {UNITS}=await import('/shared/sim.js');
    const source=await(await fetch('/client/main.js')).text();const colors=JSON.parse(source.match(/const COLORS = (\\[[^;]+\\])/)[1].replace(/0x[0-9a-f]+/gi,x=>parseInt(x,16)));
    colors.push(0xdddddd);const types=Object.keys(UNITS).sort(), cols=32;
    const canvas=document.createElement('canvas');canvas.width=cols*128;canvas.height=Math.ceil(types.length*colors.length/cols)*128;
    const c=canvas.getContext('2d');
    for(let t=0;t<types.length;t++)for(let s=0;s<colors.length;s++){const i=t*colors.length+s;c.drawImage(symbolBadge(types[t],colors[s]).material.map.image,(i%cols)*128,Math.floor(i/cols)*128);}
    return {types,colors,cols,width:canvas.width,height:canvas.height,png:canvas.toDataURL('image/png').split(',')[1]};
  })()`);
  const png=Buffer.from(result.png,'base64'); delete result.png;
  fs.writeFileSync(path.join(out,'badges.png'),png);
  fs.writeFileSync(path.join(out,'badges.json'),JSON.stringify({reference:pin,builder:'client/markers.js symbolBadge',sources,png_sha256:sha(png),...result},null,2)+'\n');
  const layout='// Generated from markers.js symbolBadge and shared/sim.js UNITS at e4e579a.\nstatic const TCHAR* const BadgeTypes[] = {\n'+result.types.map(type=>'\tTEXT('+JSON.stringify(type)+'),\n').join('')+'};\n'+`static constexpr int32 BadgeColours = ${result.colors.length}, BadgeColumns = ${result.cols}, BadgeWidth = ${result.width}, BadgeHeight = ${result.height};\n`;
  fs.writeFileSync(path.join(out,'TCMarkers.Badges.inl'),layout);
  console.log(JSON.stringify({reference:pin,png_sha256:sha(png),...result}));
} finally {
  ws?.close();
  if (browser.exitCode===null) {
    browser.kill('SIGTERM');
    await Promise.race([new Promise(resolve=>browser.once('exit',resolve)),sleep(5000)]);
    if (browser.exitCode===null) browser.kill('SIGKILL');
  }
}
