// Compare real HTTP output with the pinned server. No editor writes are sent to the reference.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { request } from 'node:http';
import { tmpdir } from 'node:os';
import { readFile } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { assets, loadReference } from './map-assets.mjs';

const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const reference=resolve(process.env.WW2_REFERENCE || resolve(root,'reference/ww2-rts'));
const binary=resolve(process.env.WW2_SERVER_BINARY || resolve(root,'rust/target/debug/ww2-server'));
const children=[];
async function launch(command,args,options,pattern) {
  const child=spawn(command,args,{...options,stdio:['ignore','pipe','pipe']});children.push(child);
  return new Promise((accept,reject)=> {
    let text='';const timer=setTimeout(()=>reject(new Error('server did not bind: '+text)),15_000);
    child.stdout.on('data',data=>{ text+=data;const found=text.match(pattern);if(found){clearTimeout(timer);accept('http://127.0.0.1:'+found[1]);} });
    child.stderr.on('data',data=>{text+=data;});
    child.on('error',reject);child.on('exit',code=>{clearTimeout(timer);reject(new Error('server exited '+code+': '+text));});
  });
}
function http(base,path,{method='GET',headers={}}={}) {
  return new Promise((accept,reject)=> {
    const url=new URL(base);
    const req=request({hostname:url.hostname,port:url.port,path,method,headers},res=> {
      const chunks=[];res.on('data',b=>chunks.push(b));res.on('end',()=>accept({status:res.statusCode,type:res.headers['content-type'],cache:res.headers['cache-control'],body:Buffer.concat(chunks).toString()}));
    });
    req.on('error',reject);req.setTimeout(5_000,()=>req.destroy(new Error('HTTP timeout')));req.end();
  });
}
function parsed(response) {
  return {...response,body:response.type==='application/json' ? JSON.parse(response.body) : response.body};
}
function projectionCases(project) {
  const legacy={w:4,h:1,rows:['N#..'],spawns:[],triggers:[{secret:'trigger'}],scenario:{secret:'scenario'}};
  const v2={w:4,h:1,world:true,worldVersion:2,rows:['NN#.'],spawns:[],
    layers:{ground:['D...'],objects:['N.#.'],mines:['NN..'],materials:[{c:0,material:'steel'},{c:2,material:'stone'}],
      objectMaterials:[{c:0,material:'steel'},{c:2,material:'stone'}],mineMaterials:[{c:0,material:'steel'},{c:1,material:'wood'}],groundMaterials:[{c:0,material:'road'}]},
    triggers:[{secret:'trigger'}],scenario:{secret:'scenario'}};
  const a=project(legacy),b=project(v2);
  assert.deepEqual(a.rows,['.#..']);assert.deepEqual(a.layers.mines,['....']);
  assert.deepEqual(b.rows,['D.#.']);assert.deepEqual(b.layers.objects,['..#.']);assert.deepEqual(b.layers.mines,['....']);
  assert.deepEqual(b.layers.materials,[{c:2,material:'stone'}]);assert.deepEqual(b.layers.objectMaterials,[{c:2,material:'stone'}]);
  assert.deepEqual(b.layers.mineMaterials,[]);assert.deepEqual(b.layers.groundMaterials,[{c:0,material:'road'}]);
  for(const m of [a,b]) {assert.equal(m.triggers,undefined);assert.equal(m.scenario,undefined);}
  assert.ok(legacy.triggers && v2.scenario,'projection must not mutate the authoring input');
}
try {
  const generated=await assets(reference);
  for(const [path,content] of generated) assert.equal(await readFile(resolve(root,path),'utf8'),content,`${path} differs from the reproducible generator`);
  const project=await loadReference(reference);projectionCases(project);
  const ref=await launch(process.execPath,[resolve(reference,'server.js')],{cwd:reference,env:{...process.env,PORT:'0',HOST:'127.0.0.1',EDIT_PASSWORD:'map-read-test-only',PUBLIC_URL:'http://example.invalid',WW2_DIAG:'0',WW2_SHARDS:'1',SAVES_DIR:resolve(root,'harness/out/nonexistent-map-test-saves')}},/ww2-rts on http:\/\/127\.0\.0\.1:(\d+)/);
  const rust=await launch(binary,['--bind','127.0.0.1:0'],{cwd:tmpdir()},/LISTENING http:\/\/127\.0\.0\.1:(\d+)/);
  const list=parsed(await http(ref,'/maps'));assert.deepEqual(parsed(await http(rust,'/maps')),list);
  let matched=1;
  for(const name of list.body) {
    const raw=JSON.parse(await readFile(resolve(reference,'maps',name+'.json'),'utf8'));
    const expected=project(raw);
    for(const suffix of ['', '.json']) {
      const path='/maps/'+name+suffix;
      const fromReference=parsed(await http(ref,path));const fromRust=parsed(await http(rust,path));
      assert.deepEqual(fromRust,fromReference,path);assert.deepEqual(fromRust.body,expected,path+' safe projection');matched++;
    }
  }
  const cases=[
    ['/maps/missing'],['/maps/missing.json'],['/maps/Default'],['/maps/a_b'],['/maps/%64efault'],['/maps/%2fdefault'],['/maps/default.json.json'],
    ['/maps/'],['/maps//default'],['/maps/default/extra'],['/maps/../Cargo.toml'],['/maps/%2e%2e/Cargo.toml'],['/maps/..%2fCargo.toml'],
    ['/maps/a/../../maps/default'],['/maps/%2e%2e/maps/default'],['/maps\\default'],['/maps/default?raw=true'],
    ['/maps/default?edit=true'],['/maps/default',{headers:{'x-edit-password':'wrong'}}],['/maps/missing',{headers:{'x-edit-password':''}}],
    ['/maps/default',{method:'PUT'}],['/maps/default',{method:'PATCH'}],['/maps/default',{method:'DELETE'}],['/maps/default',{method:'OPTIONS'}],['/maps/default',{method:'HEAD'}],
    ['/maps/Default',{method:'DELETE'}],['/maps',{method:'DELETE'}],['/maps',{method:'POST'}],['/maps/'+ 'a'.repeat(33)],
  ];
  for(const [path,options] of cases) {
    assert.deepEqual(parsed(await http(rust,path,options)),parsed(await http(ref,path,options)),`${options?.method||'GET'} ${path}`);matched++;
  }
  // This explicit unsupported route must never write or unlock an authoring map.
  const post=await http(rust,'/maps/default',{method:'POST',headers:{'x-edit-password':'map-read-test-only'}});
  assert.equal(post.status,405);assert.equal(post.body,'method not allowed');
  const raw=await http(rust,'/maps/default',{headers:{'x-edit-password':'map-read-test-only'}});
  assert.equal(raw.status,403);
  console.log(`PASS: ${list.body.length} maps, both URL forms, ${matched} exact HTTP comparisons, reproducible assets, legacy/layered-world privacy cases, editor write/raw access refused`);
} finally {
  for(const child of children)child.kill('SIGTERM');
}
