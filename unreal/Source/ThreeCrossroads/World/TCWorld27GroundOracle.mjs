// Original public-ground selection and cover geometry, with no renderer or image
// decoding. Inputs are the unchanged recipient and captured exact-camera receipt.
// Node shims never provide evidence about painted texture pixels.
// WW2_MODEL_DEPS=<deps> node --import unreal/Tools/models/register.mjs <this>
//   --stream <jsonl> --camera-log <browser-driver.log> --graphics high|low
//   --scratch <permitted-directory> --out <json>
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const opt = n => { const i=process.argv.indexOf('--'+n); return i<0 ? null : process.argv[i+1]; };
for (const key of ['stream','camera-log','graphics','scratch','out']) assert.ok(opt(key), '--'+key+' is required');
const repo=resolve(dirname(fileURLToPath(import.meta.url)), '../../../..');
const ref=join(repo,'reference/ww2-rts'), scratch=resolve(opt('scratch'));
assert.ok(['high','low'].includes(opt('graphics')));
mkdirSync(scratch,{recursive:true});
const bytes=p=>readFileSync(p), sha=p=>createHash('sha256').update(bytes(p)).digest('hex');
const imp=p=>import(pathToFileURL(join(ref,p)));
const {installShims,settle}=await import(pathToFileURL(join(repo,'unreal/Tools/models/shims.mjs')));
installShims(); globalThis.localStorage={getItem:()=>null,setItem:()=>{}}; globalThis.location={search:''};
const THREE=await import('three');
const {gfx}=await imp('client/gfx.js'); gfx.set(opt('graphics'));
const {worldLayers,composeWorldCell}=await imp('shared/world-layers.js');
const {startState,levelChar}=await imp('shared/sim.js');
const groundCode=await imp('client/ground.js');
const {createRelief}=await imp('client/relief.js');
const {createApron}=await imp('client/apron.js');
const original=bytes(join(ref,'client/atmosphere.js')).toString();
let observed=original;
function replace(a,b) { assert.equal(observed.split(a).length,2,'pinned observation seam'); observed=observed.replace(a,b); }
replace("import { setMood } from './light.js';", 'const setMood = () => {};');
for(const p of ['gfx.js','moods.js','wind.js']) replace(`from './${p}'`,`from '${pathToFileURL(join(ref,'client',p))}'`);
replace("from '../shared/weather.js'",`from '${pathToFileURL(join(ref,'shared/weather.js'))}'`);
const module=join(scratch,'original-atmosphere-observed.mjs'); writeFileSync(module,observed+'\nexport { groundMask };\n');
const {createAtmosphere,groundMask}=await import(pathToFileURL(module)); await settle();

const messages=bytes(opt('stream')).toString().split('\n').filter(Boolean).map(JSON.parse);
const start=messages.find(m=>m.t==='start'); assert.ok(start?.map?.rows && start.weather);
const map=structuredClone(start.map); assert.ok(!map.world,'this oracle accepts complete public maps only');
const cameras=bytes(opt('camera-log')).toString().split('\n').flatMap(l=>{try{return [JSON.parse(l)];}catch{return [];}}).filter(r=>r.exactCamera);
assert.ok(cameras.length,'exact-camera receipts required, not the earlier tour state');
const tick=cameras[0].tick; assert.ok(cameras.every(r=>r.tick===tick),'one held recipient endpoint required');
const grid=map.rows.map(r=>[...r]), layers=worldLayers(map), n=map.w*map.h;
const groundGrid=Array.from({length:map.h},(_,y)=>[...layers.ground.slice(y*map.w,(y+1)*map.w)]);
const objectGrid=Array.from({length:map.h},(_,y)=>[...layers.objects.slice(y*map.w,(y+1)*map.w)]);
const physical=groundGrid.map((r,y)=>r.map((c,x)=>composeWorldCell(c,objectGrid[y][x])));
const state=Uint8Array.from(map.rows.join(''),startState);
function applyCells(cells) {
  for(const [c,ch,lv,st,data] of cells??[]) {
    const x=c%map.w,y=Math.floor(c/map.w);
    if(data) { groundGrid[y][x]=data.ground; objectGrid[y][x]=data.object; }
    physical[y][x]=ch==='N'?composeWorldCell(groundGrid[y][x],objectGrid[y][x]):ch;
    grid[y][x]=ch; state[c]=st??0;
    if(lv!==undefined) { map.heights??=map.rows.map(r=>'0'.repeat(r.length)); const r=map.heights[y]; map.heights[y]=r.slice(0,x)+levelChar(lv)+r.slice(x+1); }
  }
}
applyCells(start.cells);
for(const s of messages.filter(m=>m.t==='s'&&m.tick<=tick)) applyCells(s.cells);
groundCode.internals.buildNoise();
// fieldCells calls the original private field builder and only renames ids.
const fields=groundCode.fieldCells(map).map(v=>v===1?3:v===2?9:0);
const attrs=groundCode.internals.cellAttrs({w:map.w,h:map.h,map,fields,state,groundGrid,objectGrid},grid);
const ground={cells:()=>attrs,version:()=>1,isRoad:(x,y)=>x>=0&&y>=0&&x<map.w&&y<map.h&&attrs.prim[y*map.w+x]===4};
const relief=createRelief(map,physical,{isRoad:ground.isRoad,low:gfx.low});
const apron=createApron({ground,relief,grid:physical,map});
const scene=new THREE.Scene(), camera=new THREE.PerspectiveCamera(42,16/9,1,2200);
const atmosphere=createAtmosphere({scene,camera,renderer:{getDrawingBufferSize:v=>v.set(1280,720),getPixelRatio:()=>1},cam:{x:0,z:0,y:0}});
atmosphere.start({map,key:'ardennes-crossing',ground:relief.mesh,hAt:relief.hAt,weather:start.weather,apron});
atmosphere.update(0);
const root=scene.children.find(o=>o.children?.some(c=>c.renderOrder===.58)); assert.ok(root);
const covers=root.children.filter(o=>o.renderOrder===.58), cloud=root.children.find(o=>o.renderOrder===.6);
assert.equal(covers.length,2); assert.equal(apron.fogMesh.renderOrder,1);
const uniforms=covers[0].material.uniforms;
const recipientMask=groundMask({...map,rows:grid.map(r=>r.join(''))});
const maskInputDifferences=uniforms.tMask.value.image.data.reduce((n,v,i)=>n+(v!==recipientMask.image.data[i]),0);
const maskDifferences=[];
uniforms.tMask.value.image.data.forEach((v,i)=>{if(v!==recipientMask.image.data[i])maskDifferences.push({
  cell:[Math.floor(i/4)%map.w,Math.floor(i/4/map.w)],channel:'rgba'[i%4],constructor:v,currentRecipient:recipientMask.image.data[i]});});
const expression=covers[0].material.fragmentShader.match(/float snow = ([^;]+);/)[1];
const smooth=(a,b,x)=>{const t=Math.max(0,Math.min(1,(x-a)/(b-a)));return t*t*(3-2*t);};
const snow=Function('uCover','dry','ny','small','road','smoothstep',`return ${expression.replaceAll('vN.y','ny').replaceAll('m.b','road')};`);
function sample(tex,u,v,channel) {
  const {data,width:w,height:h}=tex.image;
  let x=u*w-.5,y=v*h-.5,ix=Math.floor(x),iy=Math.floor(y),tx=x-ix,ty=y-iy;
  const index=(a,size,wrap)=>wrap===THREE.RepeatWrapping?((a%size)+size)%size:Math.max(0,Math.min(size-1,a));
  const at=(a,b)=>data[(index(b,h,tex.wrapT)*w+index(a,w,tex.wrapS))*4+channel]/255;
  return (at(ix,iy)*(1-tx)+at(ix+1,iy)*tx)*(1-ty)+(at(ix,iy+1)*(1-tx)+at(ix+1,iy+1)*tx)*ty;
}
const carriers=covers.map((o,i)=>{const m=new THREE.Mesh(o.geometry,new THREE.MeshBasicMaterial({side:o.material.side}));m.position.copy(o.position);m.quaternion.copy(o.quaternion);m.updateMatrixWorld();m.name=i?'apron':'playable';return m;});
const v=new THREE.Vector3(), triangle=new THREE.Triangle(), bary=new THREE.Vector3();
const ray=new THREE.Raycaster(), samples=[];
for(const receipt of cameras) {
  camera.position.fromArray(receipt.eye); camera.quaternion.fromArray(receipt.quaternion);camera.fov=receipt.fov;camera.updateProjectionMatrix();camera.updateMatrixWorld();
  for(let py=40;py<720;py+=80) for(let px=40;px<1280;px+=80) {
    ray.setFromCamera(new THREE.Vector2(px/1280*2-1,1-py/720*2),camera);
    const hit=ray.intersectObjects(carriers,false)[0]; if(!hit)continue;
    const pos=hit.object.geometry.attributes.position, normal=hit.object.geometry.attributes.normal, face=hit.face;
    triangle.setFromAttributeAndIndices(pos,face.a,face.b,face.c);
    triangle.getBarycoord(hit.object.worldToLocal(hit.point.clone()),bary);
    const ny=normal.getY(face.a)*bary.x+normal.getY(face.b)*bary.y+normal.getY(face.c)*bary.z;
    const [x,y,z]=hit.point.toArray(), u=x/(map.w*2),t=z/(map.h*2);
    const mask=[0,1,2].map(ch=>sample(uniforms.tMask.value,u,t,ch));
    const small=sample(uniforms.tNoise.value,x/9+.37,z/9+.37,0);
    const rawAlpha=snow(uniforms.uCover.value,1-mask[1],ny,small,mask[2],smooth)*.82;
    const depth=-v.copy(hit.point).applyMatrix4(camera.matrixWorldInverse).z;
    const fogF=smooth(receipt.fog[0],receipt.fog[1],depth);
    const cx=Math.max(0,Math.min(map.w-1,Math.floor(x/2))),cy=Math.max(0,Math.min(map.h-1,Math.floor(z/2))),c=cy*map.w+cx;
    samples.push({pose:receipt.exactCamera,pixel:[px,py],carrier:hit.object.name,position:[x,y,z],normalUp:ny,
      cell:[cx,cy],material:{primary:attrs.prim[c],secondary:attrs.sec[c],amount:attrs.amt[c]},
      mask,small,snowAlphaBeforeHaze:rawAlpha,depth,sceneHaze:fogF,snowAlphaAfterHaze:rawAlpha*(1-fogF),
      exclusion:mask[1]>.99?'water-mask':ny<.55?'steep-slope':mask[2]>.99?'road-thinning':null});
  }
}
assert.ok(samples.some(s=>s.carrier==='apron'&&s.snowAlphaAfterHaze>.1),'original outer snow exists on sampled dry apron');
const hashArray=a=>createHash('sha256').update(new Uint8Array(a.buffer,a.byteOffset,a.byteLength)).digest('hex');
const textureInfo=t=>({size:[t.image.width,t.image.height],minFilter:t.minFilter,magFilter:t.magFilter,wrap:[t.wrapS,t.wrapT],generateMipmaps:t.generateMipmaps,flipY:t.flipY,bytesSHA256:hashArray(t.image.data)});
const result={format:'tc-world27-original-ground-v1',reference:'e4e579a838aabae3b1d9dadbc17a0a87147bc52d',
  graphics:gfx.level,provenance:{streamSHA256:sha(opt('stream')),cameraLogSHA256:sha(opt('camera-log')),tick,
    mapName:map.name,mapSize:[map.w,map.h],sourceHashes:Object.fromEntries(['client/ground.js','client/apron.js','client/relief.js','client/atmosphere.js','client/surfaces.js','client/gfx.js','client/light.js'].map(p=>[p,sha(join(ref,p))]))},
  source:{groundPixelsPerCell:Math.max(8,Math.min(24,Math.floor((gfx.low?1280:2048)/Math.max(map.w,map.h)))),
    groundAttributeHashes:Object.fromEntries(['prim','sec','amt','lev','ov','scar','tr','tg','tb'].map(k=>[k,hashArray(attrs[k])])),
    coverOrder:covers[0].renderOrder,cloudOrder:cloud.renderOrder,fogOrder:apron.fogMesh.renderOrder,
    coverLift:covers.map(c=>c.position.y),cloudLift:cloud.position.y,coverPolygonOffset:[covers[0].material.polygonOffsetFactor,covers[0].material.polygonOffsetUnits],
    snowExpression:expression,cover:uniforms.uCover.value,cloudVisible:cloud.visible,
    mask:textureInfo(uniforms.tMask.value),currentRecipientMask:textureInfo(recipientMask),maskInputDifferences,maskDifferences,noise:textureInfo(uniforms.tNoise.value),
    geometry:{playableVertices:relief.geometry.attributes.position.count,apronVertices:apron.mesh.geometry.attributes.position.count}},
  counts:{samples:samples.length,outerSnow:samples.filter(s=>s.carrier==='apron'&&s.snowAlphaAfterHaze>.1).length,
    waterExcluded:samples.filter(s=>s.exclusion==='water-mask').length,steepExcluded:samples.filter(s=>s.exclusion==='steep-slope').length},samples,
  limits:'Source/oracle only. Complete public map and exact held camera; level-zero numerical texture samples, not derivative-selected GPU mips. No painted image pixels, final color, native rendering or whole-match parity. Known sanitized World query gap remains open.'};
writeFileSync(opt('out'),JSON.stringify(result,null,2)+'\n');
console.log(JSON.stringify({graphics:result.graphics,...result.counts,coverOrder:result.source.coverOrder,fogOrder:result.source.fogOrder,groundPixelsPerCell:result.source.groundPixelsPerCell}));
apron.dispose();relief.dispose();atmosphere.dispose?.();
