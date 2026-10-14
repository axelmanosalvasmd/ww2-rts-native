// Compare the lobby slice with the pinned server, then check Rust-only failure paths.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { readFile } from 'node:fs/promises';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const reference = resolve(process.env.WW2_REFERENCE || resolve(root, 'reference/ww2-rts'));
const require = createRequire(resolve(reference, 'package.json'));
const WebSocket = require('ws');
assert.equal(require('ws/package.json').version, '8.22.0', 'use the pinned reference ws dependency');
const binary = resolve(process.env.WW2_SERVER_BINARY || resolve(root, 'rust/target/debug/ww2-server'));
const children = [];
const clients = [];
async function launch(command, args, options, pattern) {
  const child = spawn(command, args, { ...options, stdio:['ignore','pipe','pipe'] }); children.push(child);
  return new Promise((accept, reject) => {
    let text = '';
    const timer = setTimeout(() => reject(new Error('server did not bind: ' + text)), 15_000);
    child.stdout.on('data', bytes => {
      text += bytes; const found = text.match(pattern);
      if (found) { clearTimeout(timer); accept('http://127.0.0.1:' + found[1]); }
    });
    child.stderr.on('data', bytes => { text += bytes; });
    child.on('error', reject);
    child.on('exit', code => { clearTimeout(timer); reject(new Error('server exited '+code+': '+text)); });
  });
}
function event(socket, name) {
  return new Promise((accept, reject) => {
    const timer = setTimeout(() => { socket.removeListener(name, done); reject(new Error('waiting for '+name)); }, 5_000);
    const done = (...v) => { clearTimeout(timer); accept(v); };
    socket.once(name, done);
  });
}
class Client {
  constructor(socket) {
    this.socket=socket; this.messages=[]; this.waiters=[];
    socket.on('message', data => {
      const value=JSON.parse(String(data)); this.messages.push(value);
      for (const waiter of [...this.waiters]) if(waiter.predicate(value)) { this.waiters.splice(this.waiters.indexOf(waiter),1); clearTimeout(waiter.timer); waiter.resolve(value); }
    });
    clients.push(this);
  }
  next(predicate) {
    return new Promise((resolve,reject) => {
      const waiter={ predicate,resolve,timer:setTimeout(() => reject(new Error('missing message; last='+JSON.stringify(this.messages.at(-1)))),5_000) };
      this.waiters.push(waiter);
    });
  }
  send(value) { this.socket.send(JSON.stringify(value)); }
}
async function connect(base, room) {
  const socket = new WebSocket(base.replace('http:', 'ws:') + '/ws?room='+room);
  const c = new Client(socket); await event(socket,'open'); return c;
}
function normalize(value) {
  value=structuredClone(value); delete value.proto; delete value.publicUrl; return value;
}
async function scenario(base) {
  const people=[]; const report=[];
  async function join(room,token,name,extra={}) {
    const c=await connect(base,room); const same=people.filter(x=>x.room===room);
    const waits=[...same.map(x=>x.c.next(v=>v.t==='lobby')),c.next(v=>v.t==='lobby')];
    c.send({t:'hello',token,name,...extra}); const updates=await Promise.all(waits);
    people.push({room,c}); report.push(updates.map(normalize)); return c;
  }
  async function change(c, value, room='LobA') {
    const same=people.filter(x=>x.room===room); const wait=same.map(x=>x.c.next(v=>v.t==='lobby'));
    c.send(value); report.push((await Promise.all(wait)).map(normalize));
  }
  async function close(c,room='LobA') {
    people.splice(people.findIndex(x=>x.c===c),1);
    const wait=people.filter(x=>x.room===room).map(x=>x.c.next(v=>v.t==='lobby'));
    const closed=event(c.socket,'close');c.socket.close();await closed;
    report.push((await Promise.all(wait)).map(normalize));
  }
  const a=await join('LobA','secret-a','<Alice>',{listing:{public:true,title:'<Room> & test'}});
  const b=await join('LobA','secret-b','Bob',{proto:0,client:'check/1'});
  const observer=await join('LobA','secret-observer','Watcher',{spectate:true});
  const other=await join('loba','secret-a','Other');
  assert.equal(other.messages.at(-1).players.length,1,'room case must isolate identity');
  await join('AAA','secret-third','Third',{listing:{public:true}});
  await change(a,{t:'addAi'});
  await change(a,{t:'level',slot:2,v:'hard'});
  await change(a,{t:'team',slot:1,v:0});
  await change(b,{t:'faction',slot:1,v:3});
  // Invalid values and unauthorized changes are ignored; a rename proves the surviving state.
  for (const msg of [{t:'team',slot:0,v:3},{t:'faction',slot:0,v:2},{t:'weather',v:'snow'},{t:'name',name:[]},{t:'faction',slot:1,v:'1'}]) b.send(msg);
  await change(b,{t:'name',name:'B & B'});
  for (const msg of [{t:'army',v:'massive'},{t:'weather',v:'snow'},{t:'sandbox',v:{}},{t:'defender',v:4},{t:'worldSize',size:'massive'},{t:'mode',v:'world'},{t:'mode',v:'assault'},{t:'map',name:'six-fronts'},{t:'army',v:'endless'},{t:'mode',v:'horde'},{t:'mode',v:'tutorial'},{t:'mode',v:'conquest'}]) await change(a,msg);
  await change(a,{t:'kick',slot:2});
  await change(b,{t:'spectate'});
  await change(b,{t:'sit'});
  let pong=b.next(v=>v.t==='pong'); b.send({t:'ping',c:{echo:[1,2]}}); assert.deepEqual(await pong,{t:'pong',c:{echo:[1,2]}});
  report.push(await (await fetch(base+'/api/rooms')).json());
  const replacement=await connect(base,'LobA');
  people.splice(people.findIndex(x=>x.c===b),1);
  const updates=people.filter(x=>x.room==='LobA').map(x=>x.c.next(v=>v.t==='lobby'));
  const replaced=b.next(v=>v.t==='replaced'), closed=event(b.socket,'close'), newLobby=replacement.next(v=>v.t==='lobby');
  replacement.send({t:'hello',token:'secret-b',name:'Back'});
  assert.deepEqual(await replaced,{t:'replaced'}); await closed;
  report.push((await Promise.all([...updates,newLobby])).map(normalize)); people.push({room:'LobA',c:replacement});
  assert.equal(replacement.messages.at(-1).you,1);
  await close(a);
  assert.equal(replacement.messages.at(-1).amHost,true,'connected human inherits host');
  const a2=await join('LobA','secret-a','Alice again'); assert.equal(a2.messages.at(-1).you,0,'reconnect keeps seat');
  await change(a2,{t:'spectate'});
  await change(replacement,{t:'addAi'});
  await change(replacement,{t:'spectate'});
  assert.equal(observer.messages.at(-1).host,-1);
  assert.equal(observer.messages.at(-1).amHost,true,'oldest connected observer hosts all-AI seats');
  report.push(await (await fetch(base+'/api/rooms')).json());
  for (const {c,room} of [...people]) await close(c,room);
  for (const packet of report) assert.ok(!JSON.stringify(packet).includes('secret-'),'no token escapes in output');
  return report;
}
async function listing_is_creation_only(base, server) {
  const people=[]; const report=[];
  const cases=[
    {code:'Listed17',listed:true,title:'Initial listed title',later:false},
    {code:'Hidden17',listed:false,title:'Initial unlisted title',later:true},
  ];
  try {
    for (const {code,listed,title,later} of cases) {
      const creator=await connect(base,code); people.push({code,c:creator});
      const lobby=creator.next(v=>v.t==='lobby');
      creator.send({t:'hello',token:'secret-listing-'+code+'-creator',name:'Creator',listing:{public:listed,title}});
      const initial=await lobby; assert.equal(initial.listed,listed,'creation sets listing');
      report.push(normalize(initial));
      const before=await (await fetch(base+'/api/rooms')).json();
      const entry=before.rooms.find(r=>r.code===code);
      if (listed) {
        assert.ok(entry,'listed room is visible at creation');
        assert.equal(entry.title,title,'creation sets the directory title');
        assert.equal(entry.humans,1);
      } else assert.equal(entry,undefined,'unlisted room is absent at creation');
      report.push(before);
      const joiner=await connect(base,code); people.push({code,c:joiner});
      const updates=[creator.next(v=>v.t==='lobby'),joiner.next(v=>v.t==='lobby')];
      joiner.send({t:'hello',token:'secret-listing-'+code+'-joiner',name:'Joiner',listing:{public:later,title:'Replacement title'}});
      const joined=await Promise.all(updates);
      for (const packet of joined) assert.equal(packet.listed,listed,'later hello cannot change listing');
      report.push(joined.map(normalize));
      const after=await (await fetch(base+'/api/rooms')).json();
      const retained=after.rooms.find(r=>r.code===code);
      if (listed) {
        assert.ok(retained,'later hello cannot hide the listed room');
        assert.equal(retained.title,title,'later hello cannot rename the listed room');
        assert.equal(retained.humans,2,'directory read follows the joining lobby');
      } else assert.equal(retained,undefined,'later hello cannot list the unlisted room');
      report.push(after);
    }
    // Remove connected seats before closing so directory cleanup has a lobby barrier.
    for (const {code,c} of people.filter(p=>p.code==='Listed17')) {
      const waits=people.filter(p=>p.code===code).map(p=>p.c.next(v=>v.t==='lobby'));
      c.send({t:'spectate'}); await Promise.all(waits);
    }
    const cleaned=await (await fetch(base+'/api/rooms')).json();
    assert.ok(cleaned.rooms.every(r=>!cases.some(c=>c.code===r.code)),'witness rooms leave no directory entries');
    for (const packet of report) assert.ok(!JSON.stringify(packet).includes('secret-'),'no token escapes in listing output');
    console.log(`PASS: listing_is_creation_only (${server}): 2 live creation-only directions, lobby and directory assertions completed`);
    return report;
  } finally {
    for (const {c} of people) {
      if (c.socket.readyState===WebSocket.OPEN) {
        const closed=event(c.socket,'close'); c.socket.close(); await closed;
      }
    }
  }
}
try {
  const ref=await launch(process.execPath,[resolve(reference,'server.js')],{cwd:reference,env:{...process.env,PORT:'0',HOST:'127.0.0.1',EDIT_PASSWORD:'lobby-test-only',PUBLIC_URL:'http://example.invalid',WW2_DIAG:'0',WW2_SHARDS:'1',SAVES_DIR:resolve(root,'harness/out/nonexistent-lobby-test-saves')}},/ww2-rts on http:\/\/127\.0\.0\.1:(\d+)/);
  const rust=await launch(binary,['--bind','127.0.0.1:0'],{cwd:root},/LISTENING http:\/\/127\.0\.0\.1:(\d+)/);
  const rustHealth=await (await fetch(rust+'/health')).json(); assert.equal(rustHealth.scope,'lobby');
  const expected=await scenario(ref); const actual=await scenario(rust); assert.deepEqual(actual,expected);
  const expectedListing=await listing_is_creation_only(ref,'Node'); const actualListing=await listing_is_creation_only(rust,'Rust'); assert.deepEqual(actualListing,expectedListing);
  const catalog=JSON.parse(await readFile(resolve(root,'rust/crates/server/src/map-catalog.json'),'utf8'));
  assert.deepEqual(await (await fetch(rust+'/maps')).json(),Object.keys(catalog).sort());
  for (const [name,counts] of Object.entries(catalog)) {
    const map=JSON.parse(await readFile(resolve(reference,'maps',name+'.json'),'utf8'));
    assert.deepEqual(counts,{normal:map.spawns.filter(s=>!s.assault).length,assault:map.spawns.length,horde:!!map.defend?.length});
  }
  for (const proto of [1,-1,'0',null,[],{}]) {
    const c=await connect(rust,'Version');const deny=c.next(v=>v.t==='deny'), closed=event(c.socket,'close');
    c.send({t:'hello',token:'v',name:'V',proto}); assert.deepEqual(await deny,{t:'deny',cmd:'join',reason:'version'}); assert.equal((await closed)[0],1008);
  }
  const invalid=await connect(rust,'!'); assert.equal((await event(invalid.socket,'close'))[0],1008);
  const c=await connect(rust,'Errors'); const lobby=c.next(v=>v.t==='lobby');
  c.socket.send('{bad json');c.socket.send('null');c.socket.send('[]');c.send({t:'hello',token:'e',name:'Error test',proto:0});await lobby;
  let pong=c.next(v=>v.t==='pong');c.socket.send('{bad json');c.send({t:'ping',c:7});assert.equal((await pong).c,7);
  const deny=c.next(v=>v.t==='deny');c.send({t:'start'});assert.deepEqual(await deny,{t:'deny',cmd:'start',reason:'scenario'});
  const closed=event(c.socket,'close');c.socket.send('x'.repeat(16*1024+1));assert.equal((await closed)[0],1009);
  const alive=await fetch(rust+'/health');assert.equal(alive.status,200);
  console.log(`PASS: ${actual.length} reference-matched lobby/directory checkpoints; 33 map metadata checks; live version, malformed, start-denial and 16 KiB overflow checks`);
} finally {
  for(const c of clients) c.socket.terminate();
  for(const child of children) child.kill('SIGTERM');
}
