// Execute the original private scheduler functions with explicit eligibility
// and bounded search-result adapters. Combat eligibility is not tested here.
// x64 Node 24.21.0; --check compares the committed fixture without writing it.
import { fileURLToPath, pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { compileFunction } from 'node:vm';
if (process.arch !== 'x64' || process.version !== 'v24.21.0') throw Error('Use x64 Node v24.21.0');
const reference = resolve(process.env.WW2_REFERENCE ?? fileURLToPath(new URL('../reference/ww2-rts/', import.meta.url)));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
if (git(['rev-parse', 'HEAD']) !== pin) throw Error(`Reference HEAD must be ${pin}`);
if (git(['status', '--porcelain', '--untracked-files=no'])) throw Error('Reference has tracked changes');
const source = readFileSync(resolve(reference, 'shared/sim.js'), 'utf8');
const start = source.indexOf('const pathGoalKey =');
const end = source.indexOf('// ---------- commands', start);
if (start < 0 || end < start || !source.includes('pathFailures.delete(from); // a fresh immediate command resets earlier retry failures')) throw Error('Reference scheduler boundaries changed');
const { TICK } = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const original = source.slice(start, end);
const factory = compileFunction(original + '\nreturn {beginPathTick, requestStepPath};', ['pathFailures','pathWork','pathStatsFor','findPath','knowledgeOf','currentPathGoal','TICK'], { filename: 'reference-path-scheduler.js' });
const kinds = ['recovery','world','build','dig','nade','enter','board','amove','fireAt','attackId'];
const field = kind => kind === 'world' ? 'worldGoal' : kind;
const value = (kind, goal) => ['build','board','attackId','enter','fireAt'].includes(kind) ? 10 : {...goal};
const scenarios=[];
const goal=(x,z)=>({x,z});
function scenario(name) {
  const input={name,actions:[],expected:[]};scenarios.push(input);
  const units=new Map(), pathFailures=new WeakMap(), pathWork=new WeakMap();
  const g={units,tick:0,versions:new Map(),pathStats:{calls:0,failed:0,deferred:0,dropped:0}};
  const searches=[];
  function search(g,u,to) {
    pathFailures.delete(u); // The original findPath entry clears this before searching.
    const outcome=u.result??'success';
    searches.push([u.id,to.x,to.z,outcome]);g.pathStats.calls++;
    if(outcome==='failed'||outcome==='failed-success')g.pathStats.failed++;
    if(outcome==='deferred')g.pathStats.deferred++;
    return outcome==='success'||outcome==='failed-success'?[{...to}]:[];
  }
  const active=(u,kind)=>kind==='recovery'||(['build','board','attackId'].includes(kind)?u[field(kind)]!==0:['enter','fireAt'].includes(kind)?u[field(kind)]!==-1:u[field(kind)]!==null);
  const current=(g,u,kind)=>u.hp>0&&u.eligible&&active(u,kind)?u.goals[kind]??null:null;
  const funcs=factory(pathFailures,pathWork,g=>g.pathStats,search,()=>({version:(g,owner)=>g.versions.get(owner)??0}),current,TICK);
  function step(action) {
    input.actions.push(action);const u=units.get(action.id);searches.length=0;
    switch(action.op) {
      case 'unit': {
        const next={id:action.id,owner:action.id,incarnation:action.incarnation??1,hp:1,eligible:true,result:'success',goals:{},path:[goal(-1,-1)],repath:0,stuck:3};
        for(const kind of kinds) {next.goals[kind]=goal(action.id+10,action.id+20);next[field(kind)]=value(kind,next.goals[kind]);}
        units.set(action.id,next);break;
      }
      case 'goal':u.goals[action.kind]=action.goal;u[field(action.kind)]=value(action.kind,action.goal);break;
      case 'hp':u.hp=action.hp;break;
      case 'eligible':u.eligible=action.value;break;
      case 'remove':units.delete(action.id);break;
      case 'result':u.result=action.value;break;
      case 'knowledge':g.versions.set(action.id,action.version);break;
      case 'direct':search(g,u,action.goal);break;
      case 'begin':g.tick=action.tick;g.pathBudget=action.budget;g.pathRetryLimit=action.retry;funcs.beginPathTick(g);break;
      case 'request':funcs.requestStepPath(g,u,action.goal,action.kind);break;
      default:throw Error(action.op);
    }
    const work=pathWork.get(g);
    input.expected.push({searches:[...searches],used:work?.used??0,queue:work?.queue.length??0,pending:work?.queued.size??0,stats:{...g.pathStats},
      units:[...units.values()].sort((a,b)=>a.id-b.id).map(u=>({id:u.id,path:u.path,repath:u.repath,stuck:u.stuck,
        orders:Object.fromEntries(kinds.filter(k=>k!=='recovery').map(k=>[field(k),u[field(k)]])),
        moveOutcome:u.moveOutcome??null,moveOutcomeTick:u.moveOutcomeTick??null,moveResult:u.moveResult??null}))});
  }
  return step;
}
let step=scenario('replacement-keeps-new-entry-position');
for(const id of [1,2])step({op:'unit',id});
step({op:'begin',tick:1,budget:0,retry:20});
for(const id of [1,2])step({op:'request',id,kind:'world',goal:goal(id+10,id+20)});
step({op:'goal',id:1,kind:'world',goal:goal(99,99)});
step({op:'request',id:1,kind:'world',goal:goal(99,99)});
step({op:'request',id:1,kind:'world',goal:goal(99,99)});
step({op:'begin',tick:2,budget:1,retry:20});
step({op:'begin',tick:3,budget:1,retry:20});
step=scenario('stale-and-dead-drop-before-budget');
for(const id of [1,2,3])step({op:'unit',id});
step({op:'begin',tick:1,budget:0,retry:20});
for(const id of [1,2,3])step({op:'request',id,kind:'world',goal:goal(id+10,id+20)});
step({op:'goal',id:1,kind:'world',goal:goal(90,90)});step({op:'hp',id:2,hp:0});
step({op:'begin',tick:2,budget:0,retry:20});step({op:'eligible',id:3,value:false});step({op:'begin',tick:3,budget:0,retry:20});
step=scenario('deleted-unit-drops-without-budget');step({op:'unit',id:1});step({op:'begin',tick:1,budget:0,retry:20});
step({op:'request',id:1,kind:'world',goal:goal(11,21)});step({op:'remove',id:1});step({op:'begin',tick:2,budget:0,retry:20});
step=scenario('queued-replacement-uses-current-unit');step({op:'unit',id:1});step({op:'begin',tick:1,budget:0,retry:20});
step({op:'request',id:1,kind:'world',goal:goal(11,21)});step({op:'unit',id:1,incarnation:2});step({op:'begin',tick:2,budget:1,retry:20});
step=scenario('retry-boundary-knowledge-direct-and-recovery');
step({op:'unit',id:1});step({op:'begin',tick:1,budget:100,retry:2});step({op:'result',id:1,value:'failed'});
step({op:'request',id:1,kind:'world',goal:goal(11,21)});
step({op:'request',id:1,kind:'recovery',goal:goal(11,21)});
step({op:'request',id:1,kind:'world',goal:goal(11,21)});
step({op:'direct',id:1,goal:goal(11,21)});
step({op:'request',id:1,kind:'world',goal:goal(11,21)});
step({op:'knowledge',id:1,version:1});step({op:'request',id:1,kind:'world',goal:goal(11,21)});
step({op:'result',id:1,value:'deferred'});step({op:'request',id:1,kind:'world',goal:goal(11,21)});
step({op:'result',id:1,value:'failed'});step({op:'request',id:1,kind:'world',goal:goal(11,21)});
step({op:'request',id:1,kind:'world',goal:goal(11,21)});
for(const kind of kinds) {
  step=scenario(`drop-kind-${kind}`);step({op:'unit',id:1});step({op:'begin',tick:7,budget:100,retry:2});step({op:'result',id:1,value:'failed'});
  for(let i=0;i<3;i++)step({op:'request',id:1,kind,goal:goal(11,21)});
}
// A known-terrain wrapper can count an earlier failed attempt, then return a
// nonempty path. The original scheduler retains both signals independently.
step=scenario('failed-attempt-with-returned-path');step({op:'unit',id:1});step({op:'begin',tick:8,budget:100,retry:2});step({op:'result',id:1,value:'failed-success'});
step({op:'request',id:1,kind:'world',goal:goal(11,21)});step({op:'request',id:1,kind:'world',goal:goal(11,21)});
step=scenario('recovery-with-failed-attempt-and-returned-path');step({op:'unit',id:1});step({op:'begin',tick:9,budget:100,retry:2});step({op:'result',id:1,value:'failed-success'});
for(const kind of ['world','recovery','world','world'])step({op:'request',id:1,kind,goal:goal(11,21)});
step=scenario('default-twenty-retries');step({op:'unit',id:1});step({op:'begin',tick:20});step({op:'result',id:1,value:'failed'});
for(let i=0;i<20;i++)step({op:'request',id:1,kind:'amove',goal:goal(11,21)});
step=scenario('unit-replacement-clears-weak-failure-state');step({op:'unit',id:1});step({op:'begin',tick:20,budget:100,retry:2});step({op:'result',id:1,value:'failed'});step({op:'request',id:1,kind:'world',goal:goal(11,21)});
step({op:'unit',id:1,incarnation:2});step({op:'result',id:1,value:'failed'});step({op:'request',id:1,kind:'world',goal:goal(11,21)});
let seed=0x5a8f31;
function pick(n){seed=(seed+0x6d2b79f5)>>>0;let t=seed;t=Math.imul(t^t>>>15,t|1);t^=t+Math.imul(t^t>>>7,t|61);return Math.floor(((t^t>>>14)>>>0)/4294967296*n);}
step=scenario('seeded-mixed-requests');
for(let id=1;id<=6;id++)step({op:'unit',id});
step({op:'begin',tick:0,budget:2,retry:3});
for(let i=0;i<450;i++) {
  const id=1+pick(6),kind=kinds[pick(kinds.length)],choice=pick(10);
  if(i%8===0)step({op:'begin',tick:i+1,budget:pick(5),retry:pick(5)});
  else if(choice<4)step({op:'request',id,kind,goal:goal(id+10,id+20)});
  else if(choice===4)step({op:'goal',id,kind,goal:goal(pick(30),pick(30))});
  else if(choice===5)step({op:'result',id,value:['success','empty','failed','deferred'][pick(4)]});
  else if(choice===6)step({op:'knowledge',id,version:pick(5)});
  else if(choice===7)step({op:'hp',id,hp:pick(2)});
  else if(choice===8)step({op:'eligible',id,value:!!pick(2)});
  else step({op:'direct',id,goal:goal(id+10,id+20)});
}
const output=JSON.stringify({reference:pin,node:process.version,arch:process.arch,seed:'0x5a8f31',tickSeconds:TICK,scenarios})+'\n';
if(process.argv.includes('--check')) {
  if(output!==readFileSync(new URL('../rust/crates/simtest/tests/fixtures/path-requests.json',import.meta.url),'utf8'))throw Error('Path request fixture differs');
  console.log('Path request fixture matches');
} else process.stdout.write(output);
