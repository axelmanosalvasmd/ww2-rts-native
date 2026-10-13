// Execute the pinned ground network and rate functions using existing independent objective inputs.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
const root=fileURLToPath(new URL('../',import.meta.url)),ref=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`),pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
assert.equal(execFileSync('git',['rev-parse','HEAD'],{cwd:ref,encoding:'utf8'}).trim(),pin);execFileSync('git',['diff','--exit-code',pin,'--','shared'],{cwd:ref});
const simUrl=pathToFileURL(`${ref}/shared/sim.js`).href;
registerHooks({load(url,c,next){const r=next(url,c);return url===simUrl?{...r,source:String(r.source)+'\nexport {groundSupply,supplySources,supplyHandoff,fullConvoyRoute,supplyContinueRoute,supplyHooks};\n'}:r;}});
const S=await import(simUrl),base=JSON.parse(readFileSync(`${root}/rust/crates/core/tests/fixtures/objectives.json`,'utf8'));
const bits=v=>{const b=Buffer.alloc(8);b.writeDoubleBE(v);return b.toString('hex');};
const rle=v=>{const out=[];for(const n of v){if(out.length&&out.at(-1)[0]===n)out.at(-1)[1]++;else out.push([n,1]);}return out;};
const rows=[];
function record(name,input,sourceCase=null,patch={}){
 const g={w:input.w,h:input.h,flags:Uint16Array.from(input.flags),height:input.heights?Int8Array.from(input.heights):undefined,chars:input.chars??Array(input.w*input.h).fill('.'),initialTerrain:{chars:input.initialChars??Array(input.w*input.h).fill('.'),height:input.initialHeights?Int8Array.from(input.initialHeights):undefined},players:structuredClone(input.players),points:structuredClone(input.points),units:new Map(input.units.map(u=>[u.id,structuredClone(u)]))};
 g.supplyNet=S.groundSupply(g,[...g.units.values()]);
 const queries=[{x:-1,z:1},{x:g.w*2,z:1},{x:1,z:g.h*2}];for(let c=0;c<g.w*g.h;c+=g.h===1?1:7)queries.push({x:(c%g.w+.5)*2,z:(Math.floor(c/g.w)+.5)*2});
 rows.push({name,sourceCase,input:sourceCase===null?input:null,patch,net:Object.entries(g.supplyNet).map(([team,n])=>({team:Number(team),steps:rle(n.steps),closed:rle(n.closed)})),queries,rates:g.players.map((_,owner)=>queries.map(at=>bits(S.supplyRateAt(g,owner,at))))});
}
for(let i=0;i<base.supplies.length;i++)record(base.supplies[i].name,base.supplies[i],i);
const line={w:400,h:1,flags:Array(400).fill(0),heights:null,players:[{team:0,spawn:{x:1,z:1},out:false},{team:1,spawn:{x:799,z:1},out:false}],points:[],units:[]};
record('long-rate-boundaries',line);
const damaged=structuredClone(line);damaged.chars=Array.from({length:400},(_,i)=>i%5===0?'+':i%5===1?'R':i%5===2?'F':'.');damaged.initialChars=Array.from({length:400},(_,i)=>i%7===0?'F':'.');damaged.heights=Array(400).fill(0);damaged.initialHeights=Array.from({length:400},(_,i)=>i%3===0?1:0);record('damage-costs-and-initial-ford',damaged);
for(const built of [undefined,0,.999999,1,2]){const input=structuredClone(line);input.units=[{id:1,type:'hq',owner:0,x:201,z:1,hp:100,air:null,riding:0,retreating:false,...(built===undefined?{}:{built})}];record(`hq-${built??'absent'}`,input);}
for(const patch of [{cut:false,contested:false},{cut:true,contested:false},{cut:false,contested:true}]){const input=structuredClone(line);input.points=[{id:'depot',x:401,z:1,owner:0,kind:'depot',...patch}];record(`depot-${patch.cut}-${patch.contested}`,input);}
const output=JSON.stringify({reference:pin,rows})+'\n',path=`${root}/rust/crates/core/tests/fixtures/supply-network.json`;if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),output);else writeFileSync(path,output);console.log(`${process.argv.includes('--check')?'Checked':'Recorded'} ${rows.length} original networks and ${rows.reduce((n,r)=>n+r.queries.length*r.rates.length,0)} exact rate lookups`);
