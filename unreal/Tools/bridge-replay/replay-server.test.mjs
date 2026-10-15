import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const sleep = ms => new Promise(r=>setTimeout(r,ms));
test('unchanged replay order, manual release and application ping keep a browser connected', async () => {
  assert.ok(process.env.TC_BRIDGE_TEST_WORK, 'Set TC_BRIDGE_TEST_WORK to a scratch directory');
  await fs.mkdir(process.env.TC_BRIDGE_TEST_WORK, {recursive:true});
  const dir = await fs.mkdtemp(path.join(process.env.TC_BRIDGE_TEST_WORK,'server-test-'));
  const lines = ['{"t":"s","tick":8398}', '{"t":"s","tick":8400,"falling":[]}'];
  await fs.writeFile(path.join(dir,'stream.jsonl'),lines.join('\n')+'\n');
  const child = spawn(process.execPath,[fileURLToPath(new URL('./replay-server.mjs',import.meta.url)), '--stream',path.join(dir,'stream.jsonl'),'--port-file',path.join(dir,'port'),'--release-after','600']);
  let socket;
  try {
    let port;
    for(let i=0;i<100;i++){try{port=Number(await fs.readFile(path.join(dir,'port'),'utf8'));break;}catch{} await sleep(20);}
    assert.ok(port);
    socket = new WebSocket(`ws://127.0.0.1:${port}/ws`);
    const received=[]; socket.addEventListener('message',e=>received.push(e.data));
    await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve);socket.addEventListener('error',reject);});
    socket.send('{"t":"ping","c":1234}');
    for(let i=0;i<100 && received.length<2;i++) await sleep(20);
    assert.deepEqual(received,[lines[0],'{"t":"pong","c":1234}']);
    await fetch(`http://127.0.0.1:${port}/replay/release`,{method:'POST'});
    for(let i=0;i<100 && received.length<3;i++) await sleep(20);
    assert.deepEqual(received,[lines[0],'{"t":"pong","c":1234}',lines[1]]);
  } finally {
    socket?.close(); child.kill('SIGTERM'); await new Promise(r=>child.once('exit',r)); await fs.rm(dir,{recursive:true});
  }
});
