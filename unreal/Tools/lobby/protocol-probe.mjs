// Live check of the lobby and match controls' messages against an isolated reference server (e4e579a): node clients
// send exactly the messages Client/TCMatchControls builds (the browser's), and the server's answers are recorded.
// usage: node protocol-probe.mjs <reference dir> <ws url> <out dir>
// Writes <out>/probe.jsonl (every message each client got, snapshots reduced to their mode, out and unit count) and
// <out>/probe-summary.json (one line per step: what was sent, what came back, pass or fail).
// No supply truck is spawned (the pinned server crashes on one in a sandbox).
import { createRequire } from 'node:module';
import { writeFileSync, appendFileSync, mkdirSync } from 'node:fs';
const [ref, wsUrl, out] = process.argv.slice(2);
mkdirSync(out, { recursive: true });
const require = createRequire(ref + '/server.js');
const WebSocket = require('ws');
const room = 'lp' + Date.now().toString(36).slice(-6);
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
const log = out + '/probe.jsonl';
writeFileSync(log, '');
const steps = [];
const step = (name, ok, detail) => { steps.push({ name, ok: !!ok, detail }); console.log((ok ? 'PASS ' : 'FAIL ') + name + (detail ? ': ' + JSON.stringify(detail) : '')); };
const listing = { public: false, title: 'Open skirmish', tutorial: false }; // client/main.js's default, sent in every hello

function client(name, extra = {}) {
  return new Promise((resolve) => {
    const ws = new WebSocket(`${wsUrl}?room=${room}`);
    const c = { name, ws, lobby: null, start: null, snap: null, got: [], closed: false, wait: [] };
    c.send = (m) => { appendFileSync(log, JSON.stringify({ who: name, sent: m }) + '\n'); ws.send(JSON.stringify(m)); };
    c.until = (pred, ms = 4000) => new Promise((res) => {
      const t0 = Date.now();
      const tick = () => { const hit = c.got.find(pred); if (hit) return res(hit); if (Date.now() - t0 > ms) return res(null); setTimeout(tick, 25); };
      tick();
    });
    c.mark = () => { c.got = []; };
    ws.on('open', () => c.send({ t: 'hello', name, token: 'probe-' + room + '-' + name, role: 'commander', commander: 0, spectate: false, listing, proto: 0, client: 'node-probe', ...extra }));
    ws.on('close', () => { c.closed = true; c.got.push({ t: '#closed' }); });
    ws.on('message', (raw) => {
      const m = JSON.parse(raw);
      let rec = m;
      if (m.t === 's') {
        rec = { t: 's', mode: m.mode, out: m.out, plans: Array.isArray(m.plans) ? m.plans.length : undefined };
        c.snap = m;
      } else if (m.t === 'start') {
        c.start = m;
        rec = { t: 'start', matchId: m.matchId, sandbox: m.sandbox, you: m.you, mode: m.mode?.kind ?? m.mode, names: m.names };
      } else if (m.t === 'lobby') {
        c.lobby = m;
        rec = { ...m, maps: m.maps?.length, saves: m.saves?.length, publicUrl: m.publicUrl ? '<publicUrl set>' : '' };
      }
      c.got.push(m);
      appendFileSync(log, JSON.stringify({ who: name, got: rec }) + '\n');
      if (m.t === 'lobby' && !c.ready) { c.ready = true; resolve(c); }
    });
  });
}
const lobbyWhere = (c, pred) => c.until((m) => m.t === 'lobby' && pred(m));

const host = await client('Alder');
step('hello with the default listing makes a room; the first seat hosts', host.lobby.you === 0 && host.lobby.amHost && host.lobby.listed === false, { you: host.lobby.you, amHost: host.lobby.amHost, listed: host.lobby.listed, code: host.lobby.code });
step('lobby carries publicUrl (string) and code', typeof host.lobby.publicUrl === 'string' && host.lobby.code === room, { publicUrlSet: !!host.lobby.publicUrl });
const guest = await client('Birch');
step('a second hello takes seat 1', guest.lobby.you === 1 && !guest.lobby.amHost);

// Watch as a spectator, then Take a seat
guest.mark(); host.mark();
guest.send({ t: 'spectate' });
let g = await lobbyWhere(guest, (m) => m.spectator === true);
step('spectate: the guest watches (spectator, no seat, listed among spectators)', g && g.you === -1 && g.spectators.includes('Birch') && g.players.length === 1, g && { you: g.you, spectators: g.spectators, players: g.players.length });
guest.mark();
guest.send({ t: 'sit' });
g = await lobbyWhere(guest, (m) => m.spectator === false);
step('sit: the guest takes a seat again', g && g.you === 1 && g.players.length === 2 && g.spectators.length === 0, g && { you: g.you, players: g.players.map(p => p.name) });
guest.mark();
guest.send({ t: 'sit' });
g = await guest.until((m) => m.t === 'lobby', 800);
step('sit while seated: the server ignores it (no lobby)', !g);

// Rename (the field holds at most 16; the server cleans <>&"\' and trims)
guest.mark(); host.mark();
guest.send({ t: 'name', name: ' <B>irch & co' });
let h = await lobbyWhere(host, (m) => m.players[1]?.name !== 'Birch');
step('name: the server keeps the cleaned name and tells everyone', h && h.players[1].name === 'Birch  co', h && { name: h.players[1].name });

// a host-only control from the guest does nothing
host.mark();
guest.send({ t: 'restart' });
h = await host.until((m) => m.t === 'start' || m.t === 'lobby', 800);
step('restart from a guest in the lobby: ignored', !h);

// Horde, sandbox, start
host.send({ t: 'mode', v: 'horde' });
await lobbyWhere(host, (m) => m.mode === 'horde');
host.send({ t: 'sandbox', v: true });
await lobbyWhere(host, (m) => m.sandbox === true);
host.mark(); guest.mark();
host.send({ t: 'start' });
const st = await host.until((m) => m.t === 'start', 8000);
step('start: a sandbox match (start.sandbox true; the mode comes with the snapshots)', st && st.sandbox === true, st && { matchId: st.matchId, sandbox: st.sandbox });
const firstMatch = st?.matchId;
let s = await host.until((m) => m.t === 's' && m.mode?.kind === 'horde', 4000);
step('Horde snapshot: a break before wave 1 (mode.active false, timeLeft > 0)', s && !s.mode.active && s.mode.timeLeft > 0, s && { active: s.mode.active, timeLeft: s.mode.timeLeft, wave: s.mode.wave });

// nextwave from the guest (not host): nothing; then pause blocks it; then the host's cuts the break
guest.send({ t: 'nextwave' });
await sleep(600);
s = host.snap;
step('nextwave from a guest: ignored (break goes on)', s && !s.mode.active && s.mode.timeLeft > 0, s && { active: s.mode.active, timeLeft: s.mode.timeLeft });
host.send({ t: 'pause' });
const pz = await host.until((m) => m.t === 'pause' && m.paused, 2000);
host.send({ t: 'nextwave' });
await sleep(500);
const tlPaused = host.snap?.mode?.timeLeft;
step('pause, then nextwave while paused: ignored', pz && tlPaused > 0, { paused: !!pz, timeLeft: tlPaused });
host.mark();
host.send({ t: 'resume' });
await host.until((m) => m.t === 'pause' && !m.paused, 2000);
host.mark();
host.send({ t: 'nextwave' });
s = await host.until((m) => m.t === 's' && m.mode?.kind === 'horde' && (m.mode.active || m.mode.timeLeft === 0), 3000);
step('nextwave from the host between waves: the break ends (timeLeft 0, then the wave is active)', !!s, s && { active: s.mode.active, timeLeft: s.mode.timeLeft, wave: s.mode.wave });
const w1 = await host.until((m) => m.t === 's' && m.mode?.kind === 'horde' && m.mode.active, 4000);
step('wave 1 on the map', !!w1, w1 && { wave: w1.mode.wave, left: w1.mode.left });

// sandbox spawn, own and enemy, never a truck
await sleep(400);
const plansBefore = host.snap?.plans?.length;
host.mark();
host.send({ t: 'spawn', unit: 'rifle', enemy: false, x: 100, z: 100 });
host.send({ t: 'spawn', unit: 'mg', enemy: true, x: 120, z: 100 });
const deny = await host.until((m) => m.t === 'deny', 1200);
const grown = await host.until((m) => m.t === 's' && Array.isArray(m.plans) && m.plans.length > plansBefore, 2000);
step('spawn own rifle and enemy MG in a sandbox: no deny, one more own unit', !deny && grown, { deny, before: plansBefore, after: grown?.plans?.length });
host.send({ t: 'spawn', unit: 'hq', enemy: false, x: 100, z: 100 });
const denyHq = await host.until((m) => m.t === 'deny' && m.cmd === 'spawn', 1500);
step('spawn of a structure (hq): denied', denyHq && denyHq.cmd === 'spawn', denyHq);

// the guest goes offline; the host hands the seat to an AI
host.mark();
guest.ws.close();
h = await lobbyWhere(host, (m) => m.players[1] && !m.players[1].connected);
step('the guest drops: the lobby shows seat 1 offline (in play)', h && h.state === 'play' && !h.players[1].ai, h && { state: h.state, seat1: h.players[1] });
host.mark();
host.send({ t: 'handAi', slot: 1 });
h = await lobbyWhere(host, (m) => m.players[1]?.ai === true);
step('handAi: seat 1 becomes an AI', h && h.players[1].ai === true, h && { seat1: h.players[1] });

// restart: the same match from scratch, a new matchId
host.mark();
host.send({ t: 'restart' });
const rs = await host.until((m) => m.t === 'start', 8000);
step('restart: a new start with another matchId', rs && rs.matchId !== firstMatch, rs && { before: firstMatch, after: rs.matchId });

// a watcher joins the running match; leave from a watcher is ignored; the host leaves; the watcher then hosts and ends
const watcher = await client('Cedar');
step('a hello during play seats nobody: spectator', watcher.lobby.spectator === true && watcher.lobby.you === -1, { spectator: watcher.lobby.spectator });
watcher.mark();
watcher.send({ t: 'leave' });
const wl = await watcher.until((m) => m.t === 'left', 800);
step('leave from a spectator: ignored (the browser hides Leave for them)', !wl);
host.mark(); watcher.mark();
host.send({ t: 'leave' });
const left = await host.until((m) => m.t === 'left', 3000);
const closed = await host.until((m) => m.t === '#closed', 3000);
step('leave: the server answers left and closes the socket', left && closed);
const wv = await lobbyWhere(watcher, (m) => m.players[0]?.ai === true);
step('the leaver\'s seat goes to an AI; with only AI seated the spectator hosts', wv && wv.amHost === true && wv.players.every(p => p.ai), wv && { amHost: wv.amHost, players: wv.players.map(p => [p.name, p.ai]) });
watcher.mark();
watcher.send({ t: 'end' });
const back = await lobbyWhere(watcher, (m) => m.state === 'lobby');
step('end from the watching host: back to the lobby, result ended', back && back.result?.ended === true, back && { state: back.state, result: back.result });
watcher.mark();
watcher.send({ t: 'sit' });
const sat = await lobbyWhere(watcher, (m) => m.spectator === false);
step('sit in the lobby after the match: the watcher takes a seat', sat && sat.you >= 0, sat && { you: sat.you, players: sat.players.length });
watcher.ws.close();
await sleep(300);
writeFileSync(out + '/probe-summary.json', JSON.stringify({ room, passed: steps.filter(s => s.ok).length, total: steps.length, steps }, null, 1));
console.log(`${steps.filter(s => s.ok).length} of ${steps.length} passed (room ${room})`);
process.exit(steps.every(s => s.ok) ? 0 : 1);
