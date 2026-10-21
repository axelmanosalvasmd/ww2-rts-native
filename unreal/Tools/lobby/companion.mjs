// Node companions for the live native runs (the browser's hello fields; no truck is ever spawned).
// usage: node companion.mjs <reference dir> <ws url> <mode> <room> [seconds]
//   guest <room> <s>     a seated guest 'Birch' that closes its socket after s seconds (goes offline)
//   host <room> <s>      a host 'Hazel' in a new room: adds an AI, waits for a second human, then starts; stays s seconds
//   rooms <tag> <s>      three public rooms (Conquest with two humans, Horde, Conquest) kept open s seconds
//   list                 prints GET /api/rooms
import { createRequire } from 'node:module';
const [ref, wsUrl, mode, room, secs = '60'] = process.argv.slice(2);
const require = createRequire(ref + '/server.js');
const WebSocket = require('ws');
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
const def = { public: false, title: 'Open skirmish', tutorial: false };
function client(code, name, listing = def) {
  return new Promise((resolve) => {
    const ws = new WebSocket(`${wsUrl}?room=${code}`);
    const c = { ws, lobby: null, send: (m) => ws.send(JSON.stringify(m)) };
    ws.on('open', () => c.send({ t: 'hello', name, token: 'companion-' + code + '-' + name, role: 'commander', commander: 0, spectate: false, listing }));
    ws.on('message', (raw) => { const m = JSON.parse(raw); if (m.t === 'lobby') { const first = !c.lobby; c.lobby = m; if (first) resolve(c); } if (m.t === 'ping' || m.t === 's') return; });
  });
}
const log = (...a) => console.log(new Date().toISOString(), ...a);
if (mode === 'guest') {
  const g = await client(room, 'Birch');
  log('guest seated', g.lobby.you, 'in', room);
  await sleep(+secs * 1000);
  g.ws.close(); log('guest closed its socket (offline)');
  await sleep(500); process.exit(0);
} else if (mode === 'host') {
  const h = await client(room, 'Hazel');
  log('host', h.lobby.you, 'amHost', h.lobby.amHost);
  h.send({ t: 'addAi' });
  const t0 = Date.now();
  // wait for the second human to have watched once (spectate) and sat again, so its Watch and Take a seat come first
  let watched = false;
  while (Date.now() - t0 < 120000 && !(watched && h.lobby.players.filter(p => !p.ai && p.connected).length >= 2)) { if (h.lobby.spectators?.length) watched = true; await sleep(100); }
  log('players', h.lobby.players.map(p => p.name).join(', '));
  await sleep(8000);
  h.send({ t: 'start' }); log('start sent');
  await sleep(+secs * 1000);
  h.ws.close(); process.exit(0);
} else if (mode === 'rooms') {
  const tag = room;
  const a = await client('ra' + tag, 'Alder', { public: true, title: 'Friday night', tutorial: false });
  const a2 = await client('ra' + tag, 'Birch', def);
  const b = await client('rh' + tag, 'Cedar', { public: true, title: 'Hold the bunker', tutorial: false });
  b.send({ t: 'mode', v: 'horde' });
  const c = await client('rc' + tag, 'Dace', { public: true, title: 'Open skirmish', tutorial: false });
  log('rooms up', ['ra' + tag, 'rh' + tag, 'rc' + tag].join(' '));
  await sleep(+secs * 1000);
  for (const x of [a, a2, b, c]) x.ws.close();
  await sleep(300); process.exit(0);
} else if (mode === 'list') {
  const http = wsUrl.replace(/^ws/, 'http').replace(/\/ws$/, '');
  console.log(await (await fetch(http + '/api/rooms')).text());
}
