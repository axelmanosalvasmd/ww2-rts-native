// Six source lanes, original public masks and source diagnostic shader controls.
// No renderer. The existing original relief is used for water's ground queries.
// --inputs <json with maps:[{map,stream}]> --scratch <dir> --out <json>
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { waterControls,instrumentWorld27,nativeWaterCommands } from './TCWorld27WaterControl.mjs';
const opt=n=>{const i=process.argv.indexOf('--'+n);return i<0?null:process.argv[i+1];};
for(const key of ['inputs','scratch','out'])assert.ok(opt(key),'--'+key+' required');
const repo=resolve(dirname(fileURLToPath(import.meta.url)),'../../../..'),ref=join(repo,'reference/ww2-rts'),scratch=resolve(opt('scratch'));
mkdirSync(scratch,{recursive:true});
const {installShims,settle}=await import(pathToFileURL(join(repo,'unreal/Tools/models/shims.mjs')));
installShims();globalThis.localStorage={getItem:()=>null,setItem:()=>{}};globalThis.location={search:''};
const THREE=await import('three'),{gfx}=await import(pathToFileURL(join(ref,'client/gfx.js')));
const {createRelief}=await import(pathToFileURL(join(ref,'client/relief.js')));
const {internals,fieldCells}=await import(pathToFileURL(join(ref,'client/ground.js')));
const raw=readFileSync(join(ref,'client/water.js'),'utf8'),light=readFileSync(join(ref,'client/light.js'),'utf8');
const sha=x=>createHash('sha256').update(x).digest('hex'),modules={};
assert.equal(instrumentWorld27('/client/water.js',raw,'baseline'),raw,'default diagnostic copy is byte-identical');
assert.equal(instrumentWorld27('/client/light.js',light,'baseline'),light,'default light is byte-identical');
for(const name of Object.keys(waterControls)) {
  let source=instrumentWorld27('/client/water.js',raw,name);
  for(const [a,b] of [["from '../shared/sim.js'",`from '${pathToFileURL(join(ref,'shared/sim.js'))}'`],
    ["from './water-levels.js'",`from '${pathToFileURL(join(ref,'client/water-levels.js'))}'`],
    ["from './gfx.js'",`from '${pathToFileURL(join(ref,'client/gfx.js'))}'`]])source=source.replace(a,b);
  const path=join(scratch,'water-'+name+'.mjs');writeFileSync(path,source);modules[name]=await import(pathToFileURL(path));
}
await settle();const lanes=[];
for(const input of JSON.parse(readFileSync(opt('inputs'),'utf8')).maps) {
  const stream=readFileSync(input.stream);if(input.sha256)assert.equal(sha(stream),input.sha256,'unchanged public recipient');
  const start=stream.toString().split('\n').filter(Boolean).map(JSON.parse).find(m=>m.t==='start'),map=start.map;
  assert.ok(!map.world&&map.rows,'complete public authored map required');
  for(const graphics of ['high','low']) {
    gfx.set(graphics);internals.buildNoise();
    const fields=fieldCells(map).map(v=>v===1?3:v===2?9:0),grid=map.rows.map(r=>[...r]);
    const attrs=internals.cellAttrs({w:map.w,h:map.h,map,fields},grid);
    const relief=createRelief(map,grid,{low:gfx.low,isRoad:(x,y)=>x>=0&&y>=0&&x<map.w&&y<map.h&&attrs.prim[y*map.w+x]===4});
    let baseline=null;const controls=[];
    for(const name of Object.keys(waterControls)) {
      const water=modules[name].createWater(grid,map,relief.hAt);assert.ok(water,'intended source water actually exists');
      water.tick(37000);
      const shader={uniforms:{},vertexShader:'#include <common>\n#include <begin_vertex>',fragmentShader:'#include <common>\n#include <map_fragment>\n#include <roughnessmap_fragment>\n#include <normal_fragment_maps>\n#include <emissivemap_fragment>'};
      water.mesh.material.onBeforeCompile(shader);
      const {uMask,uNoise,uTime}=shader.uniforms;
      const bytes=t=>new Uint8Array(t.image.data.buffer,t.image.data.byteOffset,t.image.data.byteLength);
      const hashAttribute=a=>sha(new Uint8Array(a.array.buffer,a.array.byteOffset,a.array.byteLength));
      const signature={mask:sha(bytes(uMask.value)),noise:sha(bytes(uNoise.value)),position:hashAttribute(water.mesh.geometry.attributes.position),index:hashAttribute(water.mesh.geometry.index)};
      if(!baseline)baseline=signature;else assert.deepEqual(signature,baseline,'one-factor control preserves geometry/mask/noise');
      const foam=waterControls[name].foam;
      assert.equal(shader.fragmentShader.includes('World27 foam-only control'),!foam);
      assert.equal(water.mesh.receiveShadow,waterControls[name].shadow);
      assert.equal(water.mesh.renderOrder,-1);assert.equal(uMask.value.minFilter,THREE.LinearFilter);assert.equal(uMask.value.generateMipmaps,false);
      assert.equal(uNoise.value.minFilter,THREE.LinearMipmapLinearFilter);assert.equal(uNoise.value.wrapS,THREE.RepeatWrapping);
      assert.equal(uTime.value,graphics==='low'?0:37,'original High running and Low constructor-held clocks');
      const haze=instrumentWorld27('/client/light.js',light,name);
      assert.equal(haze.includes('World27 opaque-bed haze control'),!waterControls[name].haze);
      const data=uMask.value.image.data,counts={covered:0,shoreBand:0,ford:0,pier:0};
      for(let i=0;i<data.length;i+=4){if(data[i]>0)counts.covered++;if(data[i]>79&&data[i]<133)counts.shoreBand++;if(data[i+2]>0)counts.ford++;if(data[i+3]>0)counts.pier++;}
      assert.ok(counts.covered>0&&counts.shoreBand>0,'real source coverage and shoreline are required');
      controls.push({name,nativeCommands:nativeWaterCommands(name),...signature,maskSize:[uMask.value.image.width,uMask.value.image.height],
        noiseSize:[uNoise.value.image.width,uNoise.value.image.height],counts,time:uTime.value,receiveShadow:water.mesh.receiveShadow,
        shaderSHA256:sha(shader.fragmentShader),lightSHA256:sha(haze)});
      water.dispose();
    }
    lanes.push({map:input.map,graphics,streamSHA256:sha(stream),controls});relief.dispose();
  }
}
const result={format:'tc-world27-causal-water-source-v1',reference:'e4e579a838aabae3b1d9dadbc17a0a87147bc52d',sourceWaterSHA256:sha(raw),lanes,
  limits:'Original source geometry/material controls only. No shader compilation, native visibility, foam filtering, opaque-bed pixel haze, atlas/offshore GPU samples, driver/performance or whole-match acceptance.'};
writeFileSync(opt('out'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify({status:'PASS source/oracle only',maps:lanes.length/2,lanes:lanes.length,controls:lanes.reduce((n,l)=>n+l.controls.length,0)}));
