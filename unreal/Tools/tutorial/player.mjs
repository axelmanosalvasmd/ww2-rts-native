// A browser-shaped Tutorial player for an isolated reference server: the public lobby's Tutorial button (a new private
// room, hello with listing { public: false, title: 'Tutorial', tutorial: true }), then the snapshot-only policy of
// policy.mjs. Records every message it receives (raw text, one per line) to <out>.jsonl.
// Usage: node player.mjs <reference copy dir> <ws url> <out base> [room] [name]
import { createRequire } from 'node:module';
import { randomBytes } from 'node:crypto';
import { createWriteStream } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
const [dir, url, out, roomArg, nameArg] = process.argv.slice(2);
const WebSocket = createRequire(join(dir, 'package.json'))('ws');
const { createWorldState } = await import(pathToFileURL(join(dir, 'client/world-state.js')));
const { decide } = await import('./policy.mjs');
const CELL = 2;
const room = roomArg || randomBytes(6).toString('hex'); // public-lobby.js code(): six random bytes in hex
const rec = createWriteStream(out + '.jsonl');
const world = createWorldState();
const memo = {};
let me = -1, last = -1, sent = 0, refused = 0;
const ws = new WebSocket(`${url}?room=${room}`);
const t0 = Date.now();
const say = (s) => console.log(`[${((Date.now() - t0) / 1000).toFixed(1)} s] ${s}`);
ws.on('open', () => ws.send(JSON.stringify({ t: 'hello', name: nameArg || 'Rookie', token: randomBytes(20).toString('hex'), role: 'commander', commander: 0, spectate: false, listing: { public: false, title: 'Tutorial', tutorial: true } })));
ws.on('message', (raw) => {
  const text = raw.toString(); rec.write(text + '\n');
  let m; try { m = JSON.parse(text); } catch { return; }
  if (m.t === 'lobby') { me = m.you; say(`lobby room=${m.code} state=${m.state} mode=${m.mode} map=${m.mapName} you=${m.you}`); if (m.state === 'lobby' && m.result) { say('result ' + JSON.stringify(m.result).slice(0, 200)); finish(); } }
  else if (m.t === 'start') { world.reset(); me = m.you; say(`start match ${m.matchId} map ${m.map?.name} you ${m.you} names ${JSON.stringify(m.names)} teams ${JSON.stringify(m.teams)}`); }
  else if (m.t === 'deny') { refused++; say(`deny ${m.cmd} ${m.reason}`); }
  else if (m.t === 's') {
    world.apply(m);
    if (m.mode && m.mode.step !== last) { last = m.mode.step; say(`tick ${m.tick} step ${m.mode.step + 1}/${m.mode.steps}: ${m.mode.goal}`); }
    for (const sh of m.shots ?? []) if (sh.k === 'say') say(`  say @tick ${m.tick}: ${sh.text}`);
    if (m.winner != null) { if (!memo.won) { memo.won = 1; say(`winner ${m.winner} end ${JSON.stringify(m.end)}`); } return; }
    for (const c of decide(m, memo, CELL, me)) { ws.send(JSON.stringify(c)); sent++; }
  }
});
function finish() { say(`done: sent ${sent} commands, ${refused} refused`); rec.end(() => process.exit(0)); setTimeout(() => process.exit(0), 2000); }
setTimeout(() => { say('time limit'); finish(); }, 15 * 60 * 1000);
