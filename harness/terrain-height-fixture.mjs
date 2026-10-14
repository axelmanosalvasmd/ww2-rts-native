// Original pre-match terrain queries. These are source fixtures, not native or rendered mesh output.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,readdirSync,writeFileSync} from 'node:fs';
import {registerHooks} from 'node:module';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
assert.ok(process.argv.slice(2).every(v=>v==='--check'));
const root=fileURLToPath(new URL('../',import.meta.url)),reference=resolve(process.env.WW2_REFERENCE??resolve(root,'reference/ww2-rts'));
const referenceSha='e4e579a838aabae3b1d9dadbc17a0a87147bc52d',authoringBaseline='efb677b6b0b2ac043415c47631dcacb0005b925d';
const git=args=>execFileSync('git',['-C',reference,...args],{encoding:'utf8'}).trim(),clean=()=>{assert.equal(git(['rev-parse','HEAD']),referenceSha);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};clean();
registerHooks({load(url,context,next){const result=next(url,context);if(url!==pathToFileURL(resolve(reference,'shared/sim.js')).href)return result;const source=String(result.source),marker='  // The map clients download stays separate from later building footprints and terrain edits.';assert.equal(source.split(marker).length,2);assert.equal(source.split('function heightAt(g, x, z) {').length,2);return {...result,source:source.replace(marker,'  return g;\n'+marker)+'\nexport {heightAt};\n'};}});
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js'))),names=readdirSync(resolve(reference,'maps')).filter(v=>v.endsWith('.json')).sort();assert.equal(names.length,33);
const originalRandom=Math.random;let cells=0,bytes=0;
try{
 const directory=resolve(root,'docs/fixtures/terrain/source-authored');if(process.argv.includes('--check'))assert.deepEqual(readdirSync(directory).filter(v=>v.endsWith('.json')).sort(),names);
 for(const filename of names){const map=JSON.parse(readFileSync(resolve(reference,'maps',filename),'utf8'));assert.equal(sim.validateMap(map),null);let initializationDraws=0;Math.random=()=>{initializationDraws++;return .5;};const g=sim.createGame(map,[],false,[],[],{weather:false});Math.random=()=>{throw Error('Terrain height query unexpectedly drew randomness');};
  const centreColumns=['cell','x','z','rawLevel','levelMetres','heightAtMetres'],pointColumns=['x','z','rawLevel','levelMetres','heightAtMetres'];
  const query=(x,z)=>{const raw=sim.levelAt(g,x,z),metres=raw*sim.CFG.levelHeight,height=g.height?sim.heightAt(g,x,z):0;assert.ok([x,z,raw,metres,height].every(Number.isFinite));return [x,z,raw,metres,height];};
  const centres=Array.from({length:g.w*g.h},(_,cell)=>[cell,...query((cell%g.w+.5)*sim.CELL,(Math.floor(cell/g.w)+.5)*sim.CELL)]);
  const xMax=g.w*sim.CELL,zMax=g.h*sim.CELL,e=.000001;
  const inputs=[[0,0],[xMax,0],[0,zMax],[xMax,zMax],[-sim.CELL,-sim.CELL],[xMax+sim.CELL,zMax+sim.CELL],[1,1],[xMax-1,zMax-1],[sim.CELL,sim.CELL],[xMax/2,zMax/2],[-e,zMax/2],[e,zMax/2],[xMax-e,zMax/2],[xMax+e,zMax/2],[xMax/2,zMax-e],[xMax/2,zMax+e]];
  const seed=0x54455252;let state=seed,draws=0;const next=()=>{state=(Math.imul(state,1664525)+1013904223)>>>0;draws++;return state/2**32;};while(inputs.length<1000)inputs.push([next()*(xMax+2*sim.CELL)-sim.CELL,next()*(zMax+2*sim.CELL)-sim.CELL]);assert.equal(inputs.length,1000);
  const points=inputs.map(([x,z])=>query(x,z));
  const record={schemaVersion:1,producer:{kind:'original-javascript',referenceSha,node:process.version,arch:process.arch,capture:'original createGame terrain initialization before structural graphs, starting actors and mode setup',authoringBaseline,initializationRandom:{constant:.5,draws:initializationDraws}},mapKey:filename.slice(0,-5),mapEncoding:'Original authoritative authored map input, fixture only, not client-safe start.map',map,cellMetres:sim.CELL,metresPerLevel:sim.CFG.levelHeight,heightSemantics:{rawLevel:'original levelAt returns an integer level; outside the half-open map returns 0',levelMetres:'rawLevel multiplied by original CFG.levelHeight',heightAtMetres:'original heightAt bilinear gameplay query; flat maps use the original guarded caller branch 0',renderedMesh:false},sampler:{name:'terrain-height-boundary-lcg-v1',seed,pointsPerMap:1000,boundaryPoints:16,lcg:{multiplier:1664525,increment:1013904223,modulus:'2^32',fraction:'state/2^32',drawOrder:['x','z']},extent:'[-cellMetres,widthMetres+cellMetres) x [-cellMetres,heightMetres+cellMetres)',draws,finalState:state},numericEncoding:'Finite JSON numbers. Parse with nearest-double roundtrip; compare every numeric field by IEEE754 binary64 bits (raw cell/level integers are exact).',centreColumns,pointColumns,centres,points};
  const output=JSON.stringify(record)+'\n',path=resolve(directory,filename);if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),output,filename);else writeFileSync(path,output);cells+=centres.length;bytes+=Buffer.byteLength(output);
 }
 assert.deepEqual(readdirSync(resolve(root,'docs/fixtures/terrain/source-authored')).filter(v=>v.endsWith('.json')).sort(),names);
 console.error(`${process.argv.includes('--check')?'Verified':'Recorded'} ${names.length} original authored terrain maps, ${cells} cell centres, ${names.length*1000} sample points, ${bytes} bytes`);
}finally{Math.random=originalRandom;clean();}
