// Independent scalar evaluation of the pinned three 0.186.1 shader equations.
// Read-only source. No game state, image adjustment or model changes.
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
// node unreal/Tools/water/light-oracle.mjs --three <node_modules/three> [--check]
// --out and --report may point to scratch files. The pinned reference stays read only.
const here=dirname(fileURLToPath(import.meta.url)), repo=resolve(here,'../../..');
const arg=(name,fallback)=>{const i=process.argv.indexOf(name);return i<0?fallback:process.argv[i+1];};
const three=resolve(arg('--three',process.env.WW2_MODEL_DEPS
    ?join(process.env.WW2_MODEL_DEPS,'node_modules/three')
    :join(repo,'reference/ww2-rts/node_modules/three')));
const root=join(three,'src/renderers/shaders/ShaderChunk/');
const output=resolve(arg('--out',join(repo,'unreal/Source/ThreeCrossroads/World/TCWater.LightingOracle.inl')));
const version=JSON.parse(readFileSync(join(three,'package.json'))).version;
const lock=JSON.parse(readFileSync(join(repo,'reference/ww2-rts/package-lock.json')));
if(version!==lock.packages['node_modules/three'].version || version!=='0.186.1')
    throw Error('three must match the pinned reference package-lock.json');
const common=readFileSync(root+'common.glsl.js','utf8');
const physical=readFileSync(root+'lights_physical_pars_fragment.glsl.js','utf8');
const shadow=readFileSync(root+'shadowmap_pars_fragment.glsl.js','utf8');
function scalar(text,name,args){
 const pattern=new RegExp('float '+name+'\\( ([^]*?)\\) \\{([^]*?)\\n\\}', 'g');
 const match=[...text.matchAll(pattern)].find(m=>!m[1].includes('vec'));
 if(!match)throw Error(name);
 const body=match[2].replaceAll(/\/\/[^\n]*/g,'').replaceAll(/\bfloat\b/g,'let').replaceAll(/\bEPSILON\b/g,'1e-6').replaceAll(/\bRECIPROCAL_PI\b/g,'(1/Math.PI)').replaceAll(/\bexp2\(/g,'Math.pow(2,').replaceAll(/\bsqrt\(/g,'Math.sqrt(').replaceAll(/\bmax\(/g,'Math.max(');
 return Function(...args,'const pow2=x=>x*x;'+body);
}
const f=scalar(common,'F_Schlick',['f0','f90','dotVH']);
const v=scalar(physical,'V_GGX_SmithCorrelated',['alpha','dotNL','dotNV']);
const d=scalar(physical,'D_GGX',['alpha','dotNH']);
for(const token of ['( 1.0 - F )','* material.multiScatteringCompensation'])if(!physical.includes(token))throw Error('changed direct source');
const rows=[
 [.03,.46,.67,.81,.72,.91,.77],
 [.15,.95,.2,.4,.6,.5,.48],
 [.3,.0525,1,1,1,1,1],
 [.08,.7,.01,.05,.12,.1,.62],
 [0,.46,0,0,.5,0,.8]
].map(a=>{const[c,r,nl,nv,nh,vh,e]=a, F=f(.04,1,vh), V=v(r*r,nl,nv), D=d(r*r,nh), brdf=c*(1-F)/Math.PI+F*V*D*(1+.04*(1/e-1));return [...a,F,V,D,brdf];});
const vogel=[];for(const phi of [0,1.7])for(let i=0;i<5;i++){const r=Math.sqrt((i+.5)/5),a=i*2.399963229728653+phi;vogel.push([i,phi,Math.cos(a)*r,Math.sin(a)*r]);}
if(!shadow.includes('2.399963229728653')||!shadow.includes('52.9829189'))throw Error('changed PCF source');
const rotation=[[.5,.5],[640.5,359.5],[1279.5,719.5]].map(([x,y])=>{const fract=x=>x-Math.floor(x);return[x,y,fract(52.9829189*fract(x*.06711056+y*.00583715))*2*Math.PI];});
const report={three:JSON.parse(readFileSync(join(three,'package.json'))).version,sourceHashes:Object.fromEntries(['common.glsl.js','lights_physical_pars_fragment.glsl.js','shadowmap_pars_fragment.glsl.js','lights_fragment_begin.glsl.js'].map(p=>[p,createHash('sha256').update(readFileSync(root+p)).digest('hex')])),rows,vogel,rotation};
const table=(name,rows)=>`static const double ${name}[][${rows[0].length}] = {\n${rows.map(r=>'    { '+r.map(x=>Number(x).toPrecision(17)).join(', ')+' }').join(',\n')}\n};\n`;
const generated='// Source shader arithmetic from three 0.186.1, pinned by reference package-lock.json.\n'+table('WaterLightOracle',rows)+table('WaterVogelOracle',vogel)+table('WaterRotationOracle',rotation);
if(process.argv.includes('--check')) {
    if(readFileSync(output,'utf8')!==generated)throw Error('Water lighting oracle differs: '+output);
} else writeFileSync(output,generated);
const reportPath=arg('--report','');
if(reportPath)writeFileSync(resolve(reportPath),JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify({checked:process.argv.includes('--check'),output,...report},null,2));
