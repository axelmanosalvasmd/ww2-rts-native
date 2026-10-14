// Oracle for Client/TCRoomBrowser: runs the pinned reference's own client/public-lobby.js and client/room-session.js
// under node (a minimal DOM shim), fed by a directory recorded live from an isolated reference server.
// usage: node directory-oracle.mjs <reference dir> <ws url> <out.json>
// It opens WebSocket clients (the browser's hello fields) to make public, unlisted and running rooms, records
// GET /api/rooms, closes them, then renders that body with public-lobby.js and records rows, status, filters and
// Quick play's destination.
import { createRequire } from 'node:module';
import { writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
const [ref, wsUrl, out] = process.argv.slice(2);
const require = createRequire(ref + '/server.js');
const WebSocket = require('ws');
const http = wsUrl.replace(/^ws/, 'http').replace(/\/ws$/, '');
const tag = Date.now().toString(36).slice(-4);
const sleep = (ms) => new Promise(r => setTimeout(r, ms));

function client(room, name, listing, extra = {}) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`${wsUrl}?room=${room}`);
    const c = { ws, lobby: null, msgs: [] };
    ws.on('open', () => ws.send(JSON.stringify({ t: 'hello', name, token: 'tok-' + room + '-' + name, role: 'commander', commander: 0, spectate: false, listing, ...extra })));
    ws.on('message', (raw) => { const m = JSON.parse(raw); c.msgs.push(m.t); if (m.t === 'lobby') { c.lobby = m; resolve(c); } });
    ws.on('error', reject);
  });
}
const send = (c, m) => c.ws.send(JSON.stringify(m));
const def = { public: false, title: 'Open skirmish', tutorial: false };

// rooms: two public Conquest rooms (one with two humans), a public Horde room, an unlisted room, a public room in play,
// a public full room (an AI fills the seats the map has)
const A = await client('qa' + tag, 'Alder', { public: true, title: 'Friday night', tutorial: false });
const A2 = await client('qa' + tag, 'Birch', def);
const B = await client('qb' + tag, 'Cedar', { public: true, title: 'Open skirmish', tutorial: false });
const H = await client('qh' + tag, 'Dace', { public: true, title: 'Hold the bunker', tutorial: false });
send(H, { t: 'mode', v: 'horde' }); await sleep(300);
const U = await client('qu' + tag, 'Elm', def);
const P = await client('qp' + tag, 'Fir', { public: true, title: 'Running <now>', tutorial: false });
send(P, { t: 'addAi' }); await sleep(200); send(P, { t: 'start' }); await sleep(800);
const F = await client('qf' + tag, 'Gum', { public: true, title: 'Full house', tutorial: false });
for (let i = 0; i < 6; i++) { send(F, { t: 'addAi' }); await sleep(100); }
await sleep(400);
const body = await (await fetch(http + '/api/rooms', { cache: 'no-store' })).text();
for (const c of [A, A2, B, H, U, P, F]) c.ws.close();
const recorded = JSON.parse(body);
// keep only this run's rooms (another run's rooms may still be listed on the same server)
const mine = { rooms: recorded.rooms.filter(r => r.code.endsWith(tag)) };

// --- a minimal DOM for public-lobby.js ---
class El {
  constructor(tag) { this.tag = tag; this.children = []; this.own = ''; this.dataset = {}; this.attrs = {}; this.hidden = false; this.disabled = false; this.value = ''; this.checked = false; this.className = ''; }
  set textContent(t) { this.own = String(t); this.children = []; }
  get textContent() { return this.own + this.children.map(c => c.textContent).join(''); }
  append(...k) { this.children.push(...k); }
  replaceChildren(...k) { this.children = k.flatMap(x => x.tag === '#frag' ? x.children : [x]); }
  setAttribute(k, v) { this.attrs[k] = v; }
  querySelectorAll(sel) { const all = []; const walk = (e) => { for (const c of e.children) { if (c.tag === sel) all.push(c); walk(c); } }; walk(this); return all; }
}
const ids = {};
const byId = (id) => (ids[id] ??= new El(id));
let assigned = null, rooms = mine;
globalThis.document = { getElementById: byId, createElement: (t) => new El(t), createDocumentFragment: () => new El('#frag'), activeElement: null, hidden: false };
globalThis.localStorage = { getItem: () => null, setItem: () => {} };
globalThis.location = { hash: '', search: '', assign: (u) => { assigned = u; }, replace: () => {} };
globalThis.fetch = async () => ({ ok: true, json: async () => JSON.parse(JSON.stringify(rooms)) });
const intervals = [];
globalThis.setInterval = (f, ms) => { intervals.push(ms); return 0; };
const counts = {}; for (const k of ['crypto']) counts[k] = 0;
Object.defineProperty(globalThis, 'crypto', { value: { getRandomValues: (a) => { for (let i = 0; i < a.length; i++) a[i] = i * 51; return a; } }, configurable: true });
byId('modeFilter').value = ''; byId('openOnly').checked = false; byId('nickname').value = '';
await import(pathToFileURL(ref + '/client/public-lobby.js').href);

const rowsNow = () => byId('matches').children.map(tr => {
  const td = tr.children;
  const title = td[0].own, map = td[0].children[0].textContent, mode = td[1].textContent, seats = td[2].own, counts = td[2].children[0].textContent;
  const status = td[3].textContent, open = td[3].className === 'open' ? 'open' : '';
  const btn = td[4].children[0];
  return [title, map, mode, seats, counts, status, open, btn.disabled ? 'disabled' : 'enabled', btn.attrs['aria-label']].join('|');
});
const result = { tag, body: JSON.stringify(mine), recordedRooms: recorded.rooms.length, refreshEveryMs: intervals[0] };
result.rows = rowsNow();
result.status = byId('connection').textContent;
result.emptyHidden = byId('empty').hidden;
byId('modeFilter').value = 'horde'; byId('modeFilter').onchange();
result.hordeRows = byId('matches').children.length;
byId('modeFilter').value = ''; byId('openOnly').checked = true; byId('openOnly').onchange();
result.openRows = byId('matches').children.length;
byId('openOnly').checked = false; byId('modeFilter').onchange();
byId('nickname').value = '  Rook  ';
await byId('quickPlay').onclick();
result.quickPlay = assigned;
// roomAddress cases from the reference module itself
const { roomAddress } = await import(pathToFileURL(ref + '/client/room-session.js').href);
const cases = ['', '#', '#main', '#abc', '#ABC', '#Ab3', '#ab', '#abcdefghijkl', '#abcdefghijklm', '#ab-c', '#abc&seat=guest', '#abc&seat=Guest_2-x', '#abc&seat=bad seat', '#abc&seat=' + 'x'.repeat(33), '#abc&seat=', '#&seat=two', '#main&seat=two', '#abc&x=1&seat=s1', '#abc&seat=a&seat=b', '#abc&seat=a%2Db', 'abc', '#ABC&seat=a+b'];
result.addresses = cases.map(h => { const a = roomAddress(h); return [h, a.room, a.seat, a.hash]; });
// server.js's listing title cleanup, run as the server writes it
const clean = (t) => (typeof t === 'string' ? t : 'Open skirmish').replace(/[<>&"'\x00-\x1f]/g, '').trim().slice(0, 48) || 'Open skirmish';
result.serverTitle = [' <b>Tom\'s & co</b>\t', clean(' <b>Tom\'s & co</b>\t')];
writeFileSync(out, JSON.stringify(result, null, 1));
console.log(JSON.stringify({ rows: result.rows.length, status: result.status, quickPlay: result.quickPlay, horde: result.hordeRows, open: result.openRows }));
process.exit(0);
