// Record the original weather helpers. Requires x64 Node 24.21.0; --check compares without writing.
// Reuse committed client-safe authored maps for road-mask inputs, avoiding a second map corpus.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const ref = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
assert.equal(process.version, 'v24.21.0'); assert.equal(process.arch, 'x64');
assert.equal(execFileSync('git', ['rev-parse', 'HEAD'], { cwd: ref, encoding: 'utf8' }).trim(), 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d');
execFileSync('git', ['diff', '--exit-code', 'HEAD', '--', 'shared', 'maps'], { cwd: ref });
const w = await import(pathToFileURL(`${ref}/shared/weather.js`));
const { mapForClient } = await import(pathToFileURL(`${ref}/shared/world-layers.js`));
const hash = bytes => { let h=0xcbf29ce484222325n; for(const b of bytes) h=BigInt.asUintN(64,(h^BigInt(b))*0x100000001b3n); return h.toString(16).padStart(16,'0'); };
const bits = n => { const b=Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex'); };
const normalized = p => ({ now:p.now, next:p.next??null, at:p.next?p.at:0 });
const metadata=[], authored=[];
function meta(map, key) {
  const m=Object.fromEntries(['name','mood','weather'].filter(k=>typeof map?.[k]==='string').map(k=>[k,map[k]]));
  const plan=w.mapWeather(map,key);
  metadata.push({ map:m, key:key??null, mood:w.mapMood(map,key), expected:normalized({...plan,at:plan.at*20}) });
}
for (const file of readdirSync(`${ref}/maps`).filter(f=>f.endsWith('.json')).sort()) {
  const raw=JSON.parse(readFileSync(`${ref}/maps/${file}`,'utf8')), safe=mapForClient(raw);
  assert.deepEqual(JSON.parse(readFileSync(`${root}/rust/crates/server/src/maps/${file}`,'utf8')),safe);
  meta(raw,file.slice(0,-5));
  const mask=w.roadMask(safe); authored.push({name:file.slice(0,-5),cells:mask.length,hash:hash(mask)});
}
for(const mood of [undefined,'warm','dawn','overcast','snow','dust','unknown','Dawn','__proto__']) for(const weather of [undefined,'clear','fog','rain','mud','snow','unknown','constructor']) meta({name:'River in WINTER',mood,weather},'desert');
for(const word of ['ardennes','bastogne','bulge','winter','snow','kasserine','desert','tobruk','alamein','africa','tunis','pegasus','polder','river','canal','marsh','bocage','cassino','stalingrad','seawall','rain','storm','plain','ſnow','Kasserine']) { meta({name:word.toUpperCase()},null); meta({},word); }
meta({},undefined); meta({name:'desert river storm'},null); meta({name:'river storm'},null);
const choices=[null,'map','clear','fog','rain','mud','snow','random','unknown','__proto__'];
const plans=[];
for(let seed=0;seed<4096;seed++) plans.push(['random',0,seed,w.planWeather('random',metadata[0].map,metadata[0].key,seed)]);
for(let index=0;index<metadata.length;index++) for(const choice of choices) {
  const m=metadata[index]; plans.push([choice,index,0xffffffff,w.planWeather(choice,m.map,m.key,0xffffffff)]);
}
for(const seed of [0x80000000,0x7fffffff,0xfffffffe,0xffffffff,0x6d2b79f5,0x3c1055]) plans.push(['random',0,seed,w.planWeather('random',{},null,seed)]);
const states=[null,...w.WEATHER_KINDS.map(now=>({now,next:null,at:0})),...plans.slice(0,64).map(r=>r[3]),{now:'rain',next:'mud',at:0},{now:'fog',next:'clear',at:1},{now:'fog',next:'clear',at:200},{now:'fog',next:'clear',at:201}];
const transitions=[];
for(const plan of states) for(const tick of [...new Set([0,1,19,20,21,...[-201,-200,-199,-21,-20,-19,-1,0,1,20].map(d=>Math.max(0,(plan?.at??0)+d)),0xffffffff])]) {
  const game={weather:plan?{...plan}:undefined,tick};
  const row=w.weatherRow(game); w.stepWeather(game);
  transitions.push({plan,tick,row,after:game.weather??null});
}
const effects=[null,...w.WEATHER_KINDS].map(kind=>({kind,effects:w.weatherEffects(kind),caution:bits(w.aiCaution({weather:kind?{now:kind}:undefined}))}));
const roads=[];
function road(rows) { const map={w:rows[0].length,h:rows.length,rows};roads.push({...map,expected:[...w.roadMask(map)]}); }
road(['.']);road(['B']);road(['D']);road(['=']);road(['.B...','.....','W=.D+']);road(['B..']);road(['B','.','.']);
const neighbors=[0,1,2,3,5,6,7,8];
for(const i of neighbors) for(const ch of ['B','W','F','=','D','+','N']) { const cells=Array(9).fill('.');cells[i]=ch;road([cells.slice(0,3).join(''),cells.slice(3,6).join(''),cells.slice(6).join('')]); }
for(const b of neighbors) for(const wet of neighbors.filter(i=>i!==b)) for(const ch of ['W','F','=']) { const cells=Array(9).fill('.');cells[b]='B';cells[wet]=ch;road([cells.slice(0,3).join(''),cells.slice(3,6).join(''),cells.slice(6).join('')]); }
const result={reference:'e4e579a',metadata,plans,transitions,effects,roads,authored};
const output=`${JSON.stringify(result)}\n`,path=`${root}/rust/crates/core/tests/fixtures/weather.json`;
if(process.argv.includes('--check')) assert.equal(readFileSync(path,'utf8'),output,'weather fixture differs'); else writeFileSync(path,output);
console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} ${metadata.length} metadata cases, ${plans.length} plans, ${transitions.length} transition rows, ${roads.length} synthetic roads and ${authored.length} authored maps`);
