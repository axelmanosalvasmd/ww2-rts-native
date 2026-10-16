// Export the pinned aircraft builders' own silhouettes and dimensions. No art is redrawn.
// node export-aircraft.mjs <reference> <three package> <output> <profile in scratch>
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import {spawn} from 'node:child_process';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
const [ref,three,out,profile]=process.argv.slice(2);
if(!profile)throw Error('reference, three package, output and scratch profile required');
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
if(execFileSync('git',['-C',ref,'rev-parse','HEAD'],{encoding:'utf8'}).trim()!==pin)throw Error('wrong reference pin');
if(execFileSync('git',['-C',ref,'status','--porcelain'],{encoding:'utf8'}).trim())throw Error('dirty reference');
fs.mkdirSync(out,{recursive:true});fs.mkdirSync(profile,{recursive:true});
const html=`<script type="importmap">{"imports":{"three":"/three/build/three.module.js"}}</script><script type="module">
import * as THREE from 'three';import {createAviation} from '/client/aircraft.js';import {UNITS} from '/shared/sim.js';import {buildModel,vehicleBody,__deckSeats} from '/client/unit-models.js';import {gfx} from '/client/gfx.js';import {SLOTS,menDrawn} from '/shared/squad-men.js';
window.capture=()=>{const rows=[],world=new THREE.Group();for(let fac=0;fac<4;fac++)for(const type of ['fighter','attacker','bomber']){
 const aviation=createAviation({hAt:()=>0,UNITS,world:()=>world,colorOf:()=>0x3b73d6,facOf:()=>fac});
 const v={id:1,type,models:[]},root=new THREE.Group();aviation.buildUnit(v,root,type,0);
 const cv=v.shadow.material.map.image;rows.push({faction:fac,type,nose:v.plane.nose,tail:v.plane.tail,ext:v.plane.ext,polys:v.plane.polys,png:cv.toDataURL('image/png')});
 aviation.release(v);
}
const decks=[];for(let faction=0;faction<4;faction++)for(const type of ['tank','medium','tankdestroyer','tiger','churchill']){
 const root=new THREE.Group(),v={id:1,type,models:[],root},look={uniform:0x6b7248,vehicle:0x59623d,color:0x3b73d6};
 buildModel(v,root,look,faction,UNITS[type]);const body=vehicleBody(v),points=[],p=new THREE.Vector3(),inv=new THREE.Matrix4();body.updateWorldMatrix(true,true);inv.copy(body.matrixWorld).invert();
 const visit=node=>{if(node===v.turret||v.mounts?.includes(node))return;if(node.isMesh&&node.geometry?.attributes.position){const a=node.geometry.attributes.position,m=new THREE.Matrix4().multiplyMatrices(inv,node.matrixWorld);for(let i=0;i<a.count;i++){p.fromBufferAttribute(a,i).applyMatrix4(m);points.push([p.x,p.y,p.z]);}}for(const c of node.children)visit(c);};for(const c of body.children)visit(c);
 const unique=[...new Map(points.map(p=>[p.join(','),p])).values()];decks.push({type,faction,points:unique,seats:__deckSeats(v,body)});
}const damage=[];for(const low of [false,true]){gfx.set(low?'low':'high');const events=[],aviation=createAviation({hAt:()=>0,UNITS,world:()=>world,colorOf:()=>0x3b73d6,facOf:()=>0,air:(...a)=>events.push(a)}),root=new THREE.Group(),v={id:9,type:'fighter',models:[],root,x:0,z:0,rot:0,hp:0};root.position.y=20;aviation.buildUnit(v,root,'fighter',0);
 for(const [fraction,visible,dt] of [[1,true,.016],[.6,true,.1],[.599,true,.016],[.3,true,.02],[.29,true,.1],[.29,false,.5],[.29,true,.016],[.9,true,.1],[.29,true,.1]]){v.hp=fraction*UNITS.fighter.models*UNITS.fighter.hpPer;root.visible=visible;events.length=0;aviation.tick(v,dt);damage.push({low,health:v.hp/(UNITS.fighter.models*UNITS.fighter.hpPer),visible,dt,stage:events.some(e=>e[0]==='flame')?2:events.some(e=>e[0]==='smoke')?1:0,wait:v.smokeT});}aviation.release(v);}
 const members=[];for(const [type,slots]of Object.entries(SLOTS)){const max=UNITS[type].models*UNITS[type].hpPer;for(const hp of [...new Set([0,1,Math.floor(max/slots.length),Math.ceil(max/slots.length),Math.floor(max*.25),Math.floor(max*.5),max-1,max])])members.push({type,hp,max,slots:slots.length,alive:menDrawn(type,hp,max)});}
 return {planes:rows,decks,damage,members};};window.ready=true;</script>`;
const server=http.createServer((q,r)=>{try{if(q.url==='/'){r.setHeader('Content-Type','text/html');r.end(html);return;}
 const url=new URL(q.url,'http://localhost'),prefix=url.pathname.startsWith('/three/')?'/three/':'/',base=prefix==='/three/'?three:ref;
 const file=path.resolve(base,'.'+(prefix==='/three/'?url.pathname.slice(6):url.pathname));
 if(!file.startsWith(path.resolve(base)+path.sep))throw Error('path');
 if(url.pathname==='/client/unit-models.js'){r.setHeader('Content-Type','text/javascript');r.end(fs.readFileSync(file,'utf8')+'\nexport {deckSeats as __deckSeats};\n');return;}
 r.setHeader('Content-Type',file.endsWith('.js')?'text/javascript':file.endsWith('.png')?'image/png':file.endsWith('.webp')?'image/webp':'application/octet-stream');
 fs.createReadStream(file).on('error',()=>{console.log('missing',file);r.statusCode=404;r.end();}).pipe(r);
}catch{r.statusCode=404;r.end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));const origin='http://127.0.0.1:'+server.address().port;
const browser=spawn('/usr/bin/chromium',['--headless=new','--no-first-run','--disable-background-networking','--use-angle=swiftshader','--enable-unsafe-swiftshader','--remote-debugging-port=0','--user-data-dir='+profile,'about:blank'],{stdio:['ignore','ignore','pipe']});
let ws;let output='';browser.stderr.on('data',d=>{output+=d;});const sleep=ms=>new Promise(r=>setTimeout(r,ms));
try{
 for(let i=0;i<100&&!output.includes('DevTools listening on ');i++)await sleep(100);
 const debuggerUrl=output.match(/DevTools listening on (ws:\/\/[^\s]+)/)?.[1];if(!debuggerUrl)throw Error(output);
 const target=await (await fetch('http://'+new URL(debuggerUrl).host+'/json/list')).json();
 ws=new WebSocket(target.find(t=>t.type==='page').webSocketDebuggerUrl);await new Promise(r=>ws.onopen=r);
 const pending=new Map();let id=0;ws.onmessage=e=>{const m=JSON.parse(e.data);if(m.method==='Runtime.exceptionThrown'||m.method==='Runtime.consoleAPICalled')console.log(JSON.stringify(m));if(m.id&&pending.has(m.id)){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result);}};
 const send=(method,params={})=>new Promise((resolve,reject)=>{pending.set(++id,{resolve,reject});ws.send(JSON.stringify({id,method,params}));});
 const evaluate=async expression=>{const v=await send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(v.exceptionDetails)throw Error(v.exceptionDetails.exception?.description);return v.result.value;};
 await send('Page.enable');await send('Runtime.enable');await send('Page.navigate',{url:origin});
 for(let i=0;i<100&&!await evaluate('!!window.ready');i++)await sleep(100);
 const captured=await evaluate('capture()'),rows=captured.planes;if(rows?.length!==12)throw Error('expected 12 source models');
 for(const r of rows){const file=`shadow-${r.type}-${r.faction}.png`;const bytes=Buffer.from(r.png.split(',')[1],'base64');fs.writeFileSync(path.join(out,file),bytes);delete r.png;r.texture=file;r.sha256=createHash('sha256').update(bytes).digest('hex');}
 const sources=Object.fromEntries(['client/aircraft.js','client/models/planes.js','client/unit-models.js','shared/sim.js','shared/squad-men.js'].map(f=>[f,createHash('sha256').update(fs.readFileSync(path.join(ref,f))).digest('hex')]));
 fs.writeFileSync(path.join(out,'aircraft.json'),JSON.stringify({reference:pin,sources,planes:rows,decks:captured.decks,damage:captured.damage,members:captured.members},null,0)+'\n');console.log('exported',rows.length,'source shadow canvases and dimensions');
}finally{ws?.close();browser.kill('SIGTERM');await new Promise(r=>browser.once('exit',r));await new Promise(r=>server.close(r));}
