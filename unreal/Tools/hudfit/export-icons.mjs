// Reproduce HUD silhouettes directly from the pinned procedural symbols.js. No drawn replacement art.
import fs from 'node:fs';
import path from 'node:path';
import net from 'node:net';
import {spawn,execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
const [origin,profile,reference,out]=process.argv.slice(2);
if(!out)throw Error('usage: export-icons.mjs <origin> <scratch-profile> <reference-dir> <output-inl>');
const pin=execFileSync('git',['-C',reference,'rev-parse','HEAD'],{encoding:'utf8'}).trim();
if(pin!=='e4e579a838aabae3b1d9dadbc17a0a87147bc52d')throw Error('wrong reference pin');
const source=fs.readFileSync(path.join(reference,'client/symbols.js'));
const hud=fs.readFileSync(path.join(reference,'client/hud.js'),'utf8');
const html=fs.readFileSync(path.join(reference,'client/index.html'),'utf8');
const resourcePaths=[...html.matchAll(/<span[^>]*(?:title="Manpower:|id="(?:munRes|fuelRes)")[^\n]*? d="([^"]+)"/g)].map(m=>m[1]);
if(resourcePaths.length!==3)throw Error('missing original resource art');
const toolbarPaths=Object.fromEntries(['menuBtn','fullscreen','captureNow'].map(id=>{const match=html.match(new RegExp('<button id="'+id+'"[^\\n]*? d="([^"]+)"'));if(!match)throw Error('missing source toolbar '+id);return [id,match[1]];}));
const marks=hud.slice(hud.indexOf('const star ='),hud.indexOf('const FACTION_NAME'));
if(!marks.includes('const MARKS ='))throw Error('missing original faction art');
const sha=b=>createHash('sha256').update(b).digest('hex');
if(sha(Buffer.from(await(await fetch(origin+'/client/symbols.js')).arrayBuffer()))!==sha(source))throw Error('served source differs');
const port=await new Promise((resolve,reject)=>{const s=net.createServer();s.on('error',reject);s.listen(0,'127.0.0.1',()=>{const p=s.address().port;s.close(()=>resolve(p));});});
const browser=spawn('/usr/bin/chromium',['--headless=new','--ozone-platform=headless',`--user-data-dir=${profile}`,`--remote-debugging-port=${port}`,'--no-first-run','--disable-extensions','--disable-background-networking','about:blank'],{stdio:'ignore'});
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
let ws,id=0;const pending=new Map();
try{
 for(let i=0;i<80;i++){try{const rows=await(await fetch(`http://127.0.0.1:${port}/json/list`)).json();ws=new WebSocket(rows.find(t=>t.type==='page').webSocketDebuggerUrl);break;}catch{await sleep(100);}}
 if(!ws)throw Error('no own browser');
 await new Promise(r=>ws.addEventListener('open',r,{once:true}));
 ws.addEventListener('message',e=>{const m=JSON.parse(e.data);const p=pending.get(m.id);if(p){pending.delete(m.id);m.error?p.reject(Error(m.error.message)):p.resolve(m.result);}});
 const send=(method,params={})=>new Promise((resolve,reject)=>{const n=++id;pending.set(n,{resolve,reject});ws.send(JSON.stringify({id:n,method,params}));});
 await send('Page.navigate',{url:origin+'/client/symbols.js'});await sleep(200);
 const r=await send('Runtime.evaluate',{awaitPromise:true,returnByValue:true,expression:`(async()=>{
  const {SYMBOLS,GLYPHS}=await import('/client/symbols.js');
  const paths={...Object.fromEntries(Object.entries(SYMBOLS).map(([k,v])=>[k,v.d])),...GLYPHS};
  paths.worldbase=SYMBOLS.bunker.d;
  Object.assign(paths, Object.fromEntries(${JSON.stringify(resourcePaths)}.map((d,i)=>[['resource-mp','resource-mun','resource-fuel'][i],d])));
  ${marks}
  for(let f=0;f<MARKS.length;f++)paths['faction-'+f]='';
  paths['menu-open']='';paths['menu-closed']='';
  Object.assign(paths,${JSON.stringify(toolbarPaths)});
  const glyphs=[0x21e7,0x25b8,0x25be,0x2304];for(const cp of glyphs)paths['glyph-'+cp.toString(16)]='';
  paths['key-shift']=''; // keys.js badge(): U+21E7, with index.html's browser font fallback.
  await document.fonts.ready;
  const keys=Object.keys(paths).sort(), side=96, cols=16;
  const c=document.createElement('canvas');c.width=side*cols;c.height=side*Math.ceil(keys.length/cols);
  const x=c.getContext('2d');x.fillStyle='#fff';const advances={};
  for(let i=0;i<keys.length;i++){x.save();x.translate(i%cols*side,Math.floor(i/cols)*side);x.scale(side/100,side/100);if(keys[i].startsWith('glyph-')){const cp=parseInt(keys[i].slice(6),16);x.font='88px \"Barlow Semi Condensed\", \"Arial Narrow\", sans-serif';x.textAlign='left';x.textBaseline='alphabetic';advances[cp]=x.measureText(String.fromCodePoint(cp)).width/88;x.fillText(String.fromCodePoint(cp),0,88);}else if(keys[i].startsWith('faction-')){const f=+keys[i].slice(8);const img=new Image();img.src='data:image/svg+xml;base64,'+btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">'+MARKS[f]+'</svg>');await img.decode();x.drawImage(img,0,0,100,100);}else if(keys[i]==='menu-open'||keys[i]==='menu-closed'){x.font='88px sans-serif';x.textAlign='center';x.textBaseline='middle';x.fillText(keys[i]==='menu-open'?'▾':'▸',50,50);}else if(keys[i]==='key-shift'){x.font='88px \"Barlow Semi Condensed\", \"Arial Narrow\", sans-serif';x.textAlign='center';x.textBaseline='middle';x.fillText('⇧',50,50);}else x.fill(new Path2D(paths[keys[i]]),'evenodd');x.restore();}
  return {keys,side,cols,advances,width:c.width,height:c.height,png:c.toDataURL('image/png').split(',')[1]};})()`});
 if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));
 const data=r.result.value,png=Buffer.from(data.png,'base64');
 const text=`// Generated by Tools/hudfit/export-icons.mjs from client/symbols.js at ${pin}.\n// Source sha256 ${sha(source)}. Faction source hud.js sha256 ${sha(Buffer.from(hud))}. PNG sha256 ${sha(png)}.\nstatic const TCHAR* const HudIconKeys[] = {\n${data.keys.map(k=>'\tTEXT('+JSON.stringify(k)+'),').join('\n')}\n};\nstatic const struct { uint32 CodePoint; const TCHAR* Key; float Advance; } HudGlyphs[] = {\n${Object.entries(data.advances).map(([cp,a])=>' { '+cp+', TEXT(\"glyph-'+(+cp).toString(16)+'\"), '+a+'f },').join('\n')}\n};\nstatic constexpr int32 HudIconSide=${data.side}, HudIconColumns=${data.cols}, HudIconWidth=${data.width}, HudIconHeight=${data.height};\nstatic const TCHAR* const HudIconPng =\n${data.png.match(/.{1,120}/g).map(s=>'\tTEXT("'+s+'")').join('\n')}\n;\n`;
 fs.writeFileSync(out,text);
 console.log(JSON.stringify({pin,sourceSha256:sha(source),pngSha256:sha(png),icons:data.keys.length,pngBytes:png.length,sourceBytes:text.length}));
}finally{ws?.close();if(browser.exitCode===null){browser.kill('SIGTERM');await Promise.race([new Promise(r=>browser.once('exit',r)),sleep(2000)]);if(browser.exitCode===null)browser.kill('SIGKILL');}}
