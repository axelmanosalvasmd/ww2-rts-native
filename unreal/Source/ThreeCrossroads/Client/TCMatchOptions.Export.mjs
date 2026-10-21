// Rebuild the all-map preview cases by running the pinned previewMap function, with a recording canvas.
// node unreal/Source/ThreeCrossroads/Client/TCMatchOptions.Export.mjs [--check]
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {execFileSync} from 'node:child_process';
import {pathToFileURL} from 'node:url';
const root=path.resolve(import.meta.dirname,'../../../..'),ref=path.join(root,'reference/ww2-rts');
const pin=execFileSync('git',['-C',ref,'rev-parse','HEAD'],{encoding:'utf8'}).trim();
if(pin!=='e4e579a838aabae3b1d9dadbc17a0a87147bc52d'||execFileSync('git',['-C',ref,'status','--porcelain'],{encoding:'utf8'}).trim())throw Error('requires the clean pinned reference');
const {CELL,CFG,levelOf}=await import(pathToFileURL(path.join(ref,'shared/sim.js')));
const main=fs.readFileSync(path.join(ref,'client/main.js'),'utf8');
const functionSource=main.slice(main.indexOf('async function previewMap('),main.indexOf('\nfunction renderLobby('));
const palette=main.match(/^const MM_COLORS = .*;$/m)?.[0],colors=main.match(/^const COLORS = \[[^\n]+?;/m)?.[0];
if(!functionSource||!palette||!colors)throw Error('preview source moved');
const info={},canvas={width:280,height:280},cache=new Map();let pixels,arcs=[],current;
const ctx={beginPath(){},arc(x,y,r){current={x,y,r};},fill(){arcs.push({...current,fill:this.fillStyle});},stroke(){arcs.push({...current,stroke:this.strokeStyle});},fillRect(){},drawImage(){},fillText(){}};
canvas.getContext=()=>ctx;
const sandbox={CELL,CFG,levelOf,ImageData:class{constructor(w,h){this.data=new Uint8ClampedArray(w*h*4);}},mapCache:cache,lobbyState:{},wx:{mapDefault(){}},lobbyView:{hide(){},show(){}},esc:s=>String(s),$:id=>id==='mapCanvas'?canvas:info,document:{createElement:()=>({getContext:()=>({putImageData:img=>{pixels=img.data;}})})}};
const context=vm.createContext(sandbox);vm.runInContext(palette+'\n'+colors+'\n'+functionSource+'\nglobalThis.preview=previewMap;',context);
const lit=s=>'TEXT('+JSON.stringify(String(s))+')';
const lines=['\t// Generated from previewMap at '+pin.slice(0,8)+'. Regenerate with TCMatchOptions.Export.mjs.', '\tconst FExpectedPreview DiskPreviews[] = {'];
let cases=0;
for(const file of fs.readdirSync(path.join(ref,'maps')).filter(f=>f.endsWith('.json')).sort()){
 const key=file.slice(0,-5),m=JSON.parse(fs.readFileSync(path.join(ref,'maps',file),'utf8'));cache.set(key,m);
 for(const mode of ['conquest','assault','horde','annihilation','tutorial']){
  arcs=[];context.lobbyState={mapName:key,mode};await context.preview(key,mode);let fnv=0x811c9dc5;for(const b of pixels)fnv=Math.imul(fnv^b,0x01000193)>>>0;
  const scale=280/Math.max(m.w,m.h),ox=(280-m.w*scale)/2,oy=(280-m.h*scale)/2;
  const cell=a=>({x:Math.round((a.x-ox)/scale-.5),y:Math.round((a.y-oy)/scale-.5)});
  const points=arcs.filter(a=>a.stroke==='#f0d98a').map(a=>{const p=cell(a);return p.x+','+p.y;});
  const spawns=arcs.filter(a=>a.fill).map(a=>{const p=cell(a),i=m.spawns.findIndex(sp=>sp.x===p.x&&sp.y===p.y);if(i<0)throw Error('source spawn not found');return i+'@'+p.x+','+p.y+a.fill;});
  const texts=[...info.innerHTML.matchAll(/<div>(.*?)<\/div>/g)].map(m=>m[1].replace(/<[^>]+>/g,''));if(texts.length>9)throw Error('more preview info lines');
  const ring=arcs.find(a=>a.stroke==='#f0d98a')?.r??Math.max(4,CFG.pointRadius/CELL*scale);
  lines.push(`\t\t{ ${lit(key)}, ${lit(mode)}, ${fnv}u, ${scale}, ${ox}, ${oy}, ${ring},`, `\t\t\t{ ${texts.map(lit).join(', ')} },`, `\t\t\t${lit(spawns.join(' '))}, ${lit(points.join(' '))} },`);cases++;
 }
}
lines.push('\t};');
const cpp=path.join(import.meta.dirname,'TCMatchOptions.cpp'),old=fs.readFileSync(cpp,'utf8');
const pattern=/\t(?:\/\/ Generated from previewMap[^\n]+\n)?\t?const FExpectedPreview DiskPreviews\[\] = \{[\s\S]*?\n\t\};/;
if(!pattern.test(old))throw Error('DiskPreviews block missing');
const next=old.replace(pattern,lines.join('\n'));
if(process.argv.includes('--check')){if(next!==old)throw Error('preview fixtures differ');}else fs.writeFileSync(cpp,next);
console.log(JSON.stringify({maps:cases/5,modes:5,cases,check:process.argv.includes('--check')}));
