// Health probe for a reference server: open the WebSocket, say hello as a spectator with a throwaway token, and wait
// for the lobby. Usage: node tc-ws-probe.mjs <reference copy dir> <ws url> <room>. Prints one line; exit 0 on a lobby.
// The token is random per run, never printed, and holds nothing (a spectator has no seat).
import { createRequire } from 'node:module';
import { randomBytes } from 'node:crypto';
import { join } from 'node:path';

const [dir, url, room] = process.argv.slice(2);
if (!dir || !url || !room) { console.error('usage: tc-ws-probe.mjs <reference dir> <ws url> <room>'); process.exit(2); }
const WebSocket = createRequire(join(dir, 'package.json'))('ws');
const started = Date.now();
const ws = new WebSocket(`${url}?room=${room}`);
const fail = (why) => { console.log(`ws probe: FAIL ${why}`); process.exit(1); };
const timer = setTimeout(() => fail('no lobby within 5 s'), 5000);
ws.on('open', () => ws.send(JSON.stringify({ t: 'hello', name: 'health', token: randomBytes(16).toString('hex'), spectate: true })));
ws.on('message', (raw) => {
  let m; try { m = JSON.parse(raw); } catch { return; }
  if (m.t !== 'lobby') return;
  clearTimeout(timer);
  console.log(`ws probe: ok lobby room=${m.code} state=${m.state} mode=${m.mode} spectator=${m.spectator} maps=${(m.maps ?? []).length} in ${Date.now() - started} ms`);
  ws.close(); process.exit(0);
});
ws.on('error', (e) => fail(e.message));
