// Calls unchanged prototype aircraft initialization, mission, movement and AA helpers.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {gzipSync} from 'node:zlib';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`),pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');
registerHooks({load(url,context,next){const r=next(url,context);if(url!==pathToFileURL(resolve(reference,'shared/sim.js')).href)return r;return {...r,source:String(r.source)+'\nexport {airBase,sendPlane,stepPlane,antiAir};\n'};}});
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const buf=new DataView(new ArrayBuffer(8)),bits=n=>{buf.setFloat64(0,n);return buf.getBigUint64(0).toString(16).padStart(16,'0');};
const unit=u=>({id:u.id,owner:u.owner,kind:u.type,x:bits(u.x),z:bits(u.z),rot:bits(u.rot),hp:bits(u.hp),supp:bits(u.supp),air:u.air?.state??null,still:bits(u.still),vx:bits(u.vx),vz:bits(u.vz),moveSpeed:bits(u.moveSpeed),travelDir:bits(u.travelDir),path:u.path,orders:u.orders,auto:u.auto,autoRetreat:u.autoRetreat,aim:bits(u.aim),cooldown:bits(u.cooldown),retarget:bits(u.retarget),targetId:u.targetId,hitBy:u.hitBy,hitAt:bits(u.hitAt)});
const map={name:'Aircraft source arena',w:40,h:40,rows:Array(40).fill('.'.repeat(40)),spawns:[{x:3,y:3},{x:36,y:36}],points:[{id:'front',x:20,y:20}]};
const constructors=[];
for(const kind of ['fighter','attacker','bomber'])for(const owner of [0,1])for(const index of [0,3,19]){
 const old=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(map,['A','B'],false,[0,1],[0,1],{weather:false,logistics:false,tech:false});}finally{Math.random=old;}
 g.units.clear();g.nextId=1000;const u=sim.internals.spawnUnit(g,owner,kind,index);
 constructors.push({kind,owner,index,map,spawn:g.players[owner].spawn,base:{x:u.x,z:u.z},expected:unit(u)});
}
const fixture={reference:pin,node:process.version,arch:process.arch,constructors};
const path=`${root}/rust/crates/simtest/tests/fixtures/match-aviation.json.gz`,compressed=gzipSync(JSON.stringify(fixture)+'\n',{level:9});
if(process.argv.includes('--check'))assert.deepEqual(readFileSync(path),compressed);else writeFileSync(path,compressed);
console.log(`${constructors.length} actual aircraft constructors`);
