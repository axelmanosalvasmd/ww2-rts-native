// Exercise the original mutable terrain and fog delivery collectors in order.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const reference=resolve(process.env.WW2_REFERENCE??`${root}/reference/ww2-rts`);
const pin='e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git=a=>execFileSync('git',['-C',reference,...a],{encoding:'utf8'}).trim();
const clean=()=>{assert.equal(git(['rev-parse','HEAD']),pin);assert.equal(git(['status','--porcelain','--untracked-files=no']),'');};
assert.equal(process.arch,'x64');assert.equal(process.version,'v24.21.0');clean();
assert.ok(process.argv.slice(2).every(a=>a==='--check'));
const sim=await import(pathToFileURL(resolve(reference,'shared/sim.js')));
const rows=Array(40).fill('.'.repeat(40));rows[5]=rows[5].slice(0,25)+'N'+rows[5].slice(26);
const map={name:'Recipient delivery',w:40,h:40,rows,spawns:[{x:2,y:2},{x:36,y:36},{x:2,y:36}],points:[{x:20,y:20}]};
const random=Math.random;Math.random=()=>.5;let g;try{g=sim.createGame(map,['A','Ally','Enemy'],false,[0,0,1],[0,1,2],{mode:'conquest',weather:'clear',tech:true,logistics:false});}finally{Math.random=random;}
const keys=['id','type','owner','x','z','rot','aim','hp','supp','cd','cooldown','targetId','still','shotAt','dust','buff','ap','reinf','healAt','retreating','holdFire','holdPos','autoRetreat','auto','xp','built','moveSpeed','vx','vz','travelDir','cargo','riding','garrison','amove','path','worldGoal','attackId','board','fireAt','nade','enter','orders','queue','productionJobs','prog','rally'];
const initial={map,tick:g.tick,chars:g.chars.join(''),inputs:[...g.units.values()].map(u=>Object.fromEntries(keys.filter(k=>u[k]!==undefined).map(k=>[k,u[k]]))),players:g.players.map(p=>({team:p.team,visible:[...p.visible],mp:p.mp,inc:p.inc,vp:p.vp,rally:p.rally}))};
const unit=[...g.units.values()].find(u=>u.owner===0&&!sim.UNITS[u.type].structure);
const actions=[
 {name:'initial-full',full:[true,true,true]},
 {name:'unchanged-delta'},
 {name:'visible-edit',cell:10*40+10,ch:'T'},
 {name:'drained-again'},
 {name:'vision-one',tick:1,vision:true},
 {name:'move-observer',tick:5,move:{id:unit.id,x:44,z:22},vision:true,omit:[false,true,false]},
 {name:'edit-after-move',cell:11*40+22,ch:'T',omit:[false,true,false]},
 {name:'delayed-peer',tick:9,vision:true},
 {name:'reconnect',full:[true,false,false]},
 {name:'full-did-not-drain'},
 {name:'nearby-edits',cell:11*40+22,ch:'.'},
 {name:'full-after-edit',full:[true,true,true]},
 {name:'final-drain'},
];
for(const action of actions){
 if(action.tick!==undefined)g.tick=action.tick;
 if(action.move)Object.assign(g.units.get(action.move.id),{x:action.move.x,z:action.move.z});
 if(action.cell!==undefined)sim.internals.setCell(g,action.cell,action.ch);
 if(action.vision)sim.internals.updateVision(g);
 const full=action.full??[false,false,false],omit=action.omit??[false,false,false];
 action.outputs=g.players.map((_,slot)=>({cells:sim.terrainFor(g,slot,full[slot]),...(!omit[slot]?{fog:sim.fogFor(g,slot,full[slot])}:{})}));
}
clean();const content=JSON.stringify({pin,initial,actions})+'\n',target=resolve(root,'rust/crates/core/tests/fixtures/recipient-delivery.json');
if(process.argv.includes('--check'))assert.equal(readFileSync(target,'utf8'),content);else writeFileSync(target,content);
console.log(`${actions.length} original delivery steps, ${actions.length*3} recipients`);
