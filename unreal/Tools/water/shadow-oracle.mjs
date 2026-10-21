// Source-only oracle. Evaluate the actual pinned three frustum expressions and
// independent ClampToEdge texel addresses in a subrectangle of a native atlas.
import {readFileSync, writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {dirname, join, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

// node unreal/Tools/water/shadow-oracle.mjs --three <node_modules/three> [--check]
// Use --repo when running a copy outside Tools/water. --out and --report accept
// scratch/evidence paths. The pinned reference stays read only.
const here=dirname(fileURLToPath(import.meta.url));
const arg=(name,fallback)=>{
  const i=process.argv.indexOf(name);
  if(i<0) return fallback;
  if(!process.argv[i+1] || process.argv[i+1].startsWith('--')) throw Error('missing value for '+name);
  return process.argv[i+1];
};
const repo=resolve(arg('--repo',resolve(here,'../../..')));
const three=resolve(arg('--three',process.env.WW2_MODEL_DEPS
  ?join(process.env.WW2_MODEL_DEPS,'node_modules/three')
  :join(repo,'reference/ww2-rts/node_modules/three')));
const file=join(three,'src/renderers/shaders/ShaderChunk/shadowmap_pars_fragment.glsl.js');
const js=readFileSync(file,'utf8');
const version=JSON.parse(readFileSync(join(three,'package.json'))).version;
const lock=JSON.parse(readFileSync(join(repo,'reference/ww2-rts/package-lock.json')));
if(version!==lock.packages['node_modules/three'].version || version!=='0.186.1')
  throw Error('three must match the pinned reference package-lock.json');
const gates=[...js.matchAll(/bool inFrustum = ([^;]+);\s*bool frustumTest = ([^;]+);/g)];
if(gates.length!==3) throw Error('pinned getShadow gates changed');
const evalGate=gates.map(m=>Function('shadowCoord',`const inFrustum=${m[1]};return ${m[2]};`));
const frustum=[];
for(const u of [-.001,0,.5,1,1.001]) for(const v of [0,.5,1]) for(const z of [-.5,0,.5,1,1.000001,2]) {
  const results=evalGate.map(f=>f({x:u,y:v,z}));
  if(!results.every(v=>v===results[0])) throw Error('PCF and Basic gates disagree');
  frustum.push([u,v,z,Number(results[0])]);
}
const texels=[];
for(const [size,first,count] of [[16,4,4],[32,9,7],[16,0,16]]) {
  for(const offset of [-6,-1,0,1,count-1,count,count+5]) {
    // A source texture's integer coordinate clamps in [0,count-1]. Move that
    // selected source texel into the native allocation only after clamping.
    texels.push([first+offset,size,first/size,(first+count)/size,first+Math.min(count-1,Math.max(0,offset))]);
  }
}
const table=(name,rows)=>`static constexpr double ${name}[][${rows[0].length}] = {\n${rows.map(row=>'    { '+row.map(n=>n.toPrecision(17)).join(', ')+' }').join(',\n')}\n};\n`;
const generated='// Pinned three 0.186.1 getShadow frustum and ClampToEdge oracle.\n'+table('WaterShadowFrustumOracle',frustum)+table('WaterShadowTexelOracle',texels);
const out=resolve(arg('--out',join(repo,'unreal/Source/ThreeCrossroads/World/TCWater.ShadowOracle.inl')));
if(process.argv.includes('--check')) {if(readFileSync(out,'utf8')!==generated) throw Error('stale oracle');}
else writeFileSync(out,generated);
const report={source:file,sha256:createHash('sha256').update(js).digest('hex'),three:'0.186.1',checked:process.argv.includes('--check'),frustumCases:frustum.length,texelCases:texels.length,
  note:'Numeric source/adapter oracle, not a GPU atlas capture or visual acceptance.',
  missingDepthGateExample:{u:.5,v:.5,depth:1.000001,source:1,oldNative:0},
  atlasExample:{size:16,tile:[4,8],sample:8,sourceTexel:7,oldNativeTexel:8}};
const reportPath=arg('--report','');
if(reportPath) writeFileSync(resolve(reportPath),JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify({output:out,...report},null,2));
