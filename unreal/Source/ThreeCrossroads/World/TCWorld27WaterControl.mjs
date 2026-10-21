// Served diagnostic copies only. Default returns the original bytes unchanged.
// Pair water-off with bed-haze-off to vary haze with the opaque bed exposed.
import assert from 'node:assert/strict';
export const world27Kind='causal-water';
export const waterControls={
  baseline:{water:true,foam:true,shadow:true,haze:true},
  'water-off':{water:false,foam:true,shadow:true,haze:true},
  'foam-off':{water:true,foam:false,shadow:true,haze:true},
  'water-shadow-off':{water:true,foam:true,shadow:false,haze:true},
  'bed-haze-off':{water:false,foam:true,shadow:true,haze:false},
};
export function instrumentWorld27(path, source, name=process.env.TC_WORLD27_WATER_CONTROL??'baseline') {
  const c=waterControls[name];assert.ok(c,'unknown World27 water control');
  const replace=(a,b)=>{assert.equal(source.split(a).length,2,'pinned one-factor source seam: '+a);source=source.replace(a,b);};
  if(path==='/client/water.js') {
    if(!c.foam) {
      replace('crest * (0.14 + 0.3 * ford)','0.0 /* World27 foam-only control */');
      replace('wFoam = max(wFoam, ring * (0.55 + 0.45 * brk));','wFoam = 0.0; // World27: no shoreline, pier or wake foam');
    }
    if(!c.shadow)replace('mesh.receiveShadow = true;','mesh.receiveShadow = false; // World27: water reception only');
  }
  if(path==='/client/light.js'&&!c.haze)
    replace('scene.fog.near = dist * 0.6 / sky.haze; scene.fog.far = dist * 3.8 / sky.haze;',
      'scene.fog.near = 1e8; scene.fog.far = 1e9; // World27 opaque-bed haze control');
  // Visibility changes after the unchanged warmup draw in the copied tour driver.
  // A hidden-from-construction mesh would lack the compiled uTime receipt there.
  return source;
}
export function nativeWaterCommands(name) {
  const c=waterControls[name];assert.ok(c,'unknown World27 water control');
  return `tc.World27.WaterVisible ${+c.water},tc.World27.WaterFoam ${+c.foam},tc.World27.WaterShadow ${+c.shadow},tc.Look.Haze ${+c.haze}`;
}
