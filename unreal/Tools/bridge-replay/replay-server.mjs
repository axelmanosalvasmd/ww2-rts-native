// A recorded-stream WebSocket server for the Great Bridge tick-8400 capture (no reference server, no simulation): every
// client that connects gets one recorded recipient stream's text payloads, unchanged and in order, as text frames.
// Node standard library only (a minimal RFC 6455 server: handshake, unmasked text frames out, masked frames in).
//
//   node replay-server.mjs --stream <stream.jsonl> --port-file <file> [--hold-tick 8400] [--release-after 60]
//        [--interval 100] [--log <file>]
//
// Lines before the snapshot whose tick is --hold-tick go out at once after the connection opens; the rest wait until
// --release-after seconds after the connection, then go out one per --interval ms. The socket stays open after the
// last line (the client keeps the last state on screen). Client messages are logged by their 't' only (never a token).
// Listens on 127.0.0.1, on a free port written to --port-file.
import http from 'node:http';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, appendFileSync, existsSync } from 'node:fs';

const arg = (name, fallback) => { const i = process.argv.indexOf(`--${name}`); return i > 0 ? process.argv[i + 1] : fallback; };
const streamFile = arg('stream'), portFile = arg('port-file'), logFile = arg('log');
const reference = arg('reference'), three = arg('three');
const releases = new Set();
const holdTick = Number(arg('hold-tick', 8400)), releaseAfter = Number(arg('release-after', 60)), interval = Number(arg('interval', 100));
if (!streamFile || !portFile) { console.error('usage: replay-server.mjs --stream <file> --port-file <file> [--hold-tick N] [--release-after S] [--interval MS] [--log <file>]'); process.exit(2); }
const lines = readFileSync(streamFile, 'utf8').split('\n').filter((l, i, a) => !(l === '' && i === a.length - 1));
const holdAt = lines.findIndex(l => { try { const m = JSON.parse(l); return m.t === 's' && m.tick === holdTick; } catch { return false; } });
const log = (s) => { const line = `${new Date().toISOString()} ${s}`; if (logFile) appendFileSync(logFile, line + '\n'); else console.log(line); };
log(`stream ${streamFile}: ${lines.length} payloads, hold before line ${holdAt + 1} (tick ${holdTick}), release ${releaseAfter} s after the connection, then every ${interval} ms`);

function frame(text) {
  const body = Buffer.from(text, 'utf8'), n = body.length;
  const head = n < 126 ? Buffer.from([0x81, n]) : n < 65536 ? Buffer.from([0x81, 126, n >> 8, n & 255]) : Buffer.concat([Buffer.from([0x81, 127]), (() => { const b = Buffer.alloc(8); b.writeBigUInt64BE(BigInt(n)); return b; })()]);
  return Buffer.concat([head, body]);
}

let connections = 0;
const server = http.createServer((req, res) => {
  const url = new URL(req.url, 'http://localhost');
  if (url.pathname === '/replay/release' && req.method === 'POST') {
    for (const release of releases) release();
    res.writeHead(200, {'content-type':'application/json'}); res.end(JSON.stringify({connections})); return;
  }
  // Optional read-only browser assets. This is a replay server, never a simulation server.
  if (reference && three) {
    if (url.pathname === '/maps' || url.pathname === '/api/rooms') {
      res.writeHead(200, {'content-type':'application/json'}); res.end(url.pathname === '/maps' ? '[]' : '{"rooms":[]}'); return;
    }
    let file;
    if (url.pathname === '/') file = path.join(reference, 'client/public-lobby.html');
    if (url.pathname === '/play') file = path.join(reference, 'client/index.html');
    for (const [prefix, base] of [['/client/',path.join(reference,'client')],['/shared/',path.join(reference,'shared')],['/vendor/',path.join(three,'build')],['/vendor-jsm/',path.join(three,'examples/jsm')]]) {
      if (url.pathname.startsWith(prefix)) {
        const candidate = path.resolve(base, decodeURIComponent(url.pathname.slice(prefix.length)));
        if (candidate.startsWith(path.resolve(base) + path.sep)) file = candidate;
      }
    }
    if (file && existsSync(file)) {
      const types = {'.html':'text/html','.js':'text/javascript','.css':'text/css','.json':'application/json','.png':'image/png','.jpg':'image/jpeg','.webp':'image/webp','.svg':'image/svg+xml','.mp3':'audio/mpeg'};
      try { res.writeHead(200, {'content-type':types[path.extname(file)] ?? 'application/octet-stream'}); res.end(readFileSync(file)); return; } catch {}
    }
  }
  log(`http ${req.method} ${url.pathname}: 404`); res.writeHead(404); res.end();
});
server.on('upgrade', (req, socket) => {
  const key = req.headers['sec-websocket-key'];
  if (!key) { socket.destroy(); return; }
  const accept = createHash('sha1').update(key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
  socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
  const id = ++connections, opened = Date.now();
  log(`connection ${id} open (${req.url.split('?')[0]}, room ${new URL(req.url, 'http://x').searchParams.get('room')})`);
  let sent = 0, closed = false, inbox = Buffer.alloc(0);
  const counts = {};
  const send = (upTo) => { while (!closed && sent < upTo) socket.write(frame(lines[sent++])); };
  send(holdAt < 0 ? lines.length : holdAt);
  log(`connection ${id}: ${sent} payloads sent at once (through the snapshot before tick ${holdTick})`);
  let pace, released = false;
  const release = () => {
    if (released || closed) return;
    released = true;
    pace = setInterval(() => {
      if (closed || sent >= lines.length) { clearInterval(pace); if (!closed) log(`connection ${id}: all ${sent} payloads sent; holding the socket open`); return; }
      send(sent + 1);
      if (sent === holdAt + 1) log(`connection ${id}: tick ${holdTick} sent (payload ${sent}) ${((Date.now() - opened) / 1000).toFixed(2)} s after the connection`);
    }, interval);
  };
  releases.add(release);
  const timer = setTimeout(release, releaseAfter * 1000);
  socket.on('data', (chunk) => {
    inbox = Buffer.concat([inbox, chunk]);
    while (inbox.length >= 2) {
      const op = inbox[0] & 15, masked = inbox[1] & 128;
      let len = inbox[1] & 127, at = 2;
      if (len === 126) { if (inbox.length < 4) return; len = inbox.readUInt16BE(2); at = 4; }
      else if (len === 127) { if (inbox.length < 10) return; len = Number(inbox.readBigUInt64BE(2)); at = 10; }
      const need = at + (masked ? 4 : 0) + len;
      if (inbox.length < need) return;
      const mask = masked ? inbox.subarray(at, at + 4) : null, body = Buffer.from(inbox.subarray(at + (masked ? 4 : 0), need));
      if (mask) for (let i = 0; i < body.length; i++) body[i] ^= mask[i & 3];
      inbox = inbox.subarray(need);
      if (op === 8) { log(`connection ${id}: client close; message types ${JSON.stringify(counts)}`); closed = true; socket.end(); return; }
      if (op === 9) { socket.write(Buffer.concat([Buffer.from([0x8a, body.length]), body])); continue; }
      if (op !== 1) continue;
      let t = '?'; try { const msg = JSON.parse(body.toString('utf8')); t = msg.t ?? '?'; if (t === 'ping') socket.write(frame(JSON.stringify({t:'pong', c:msg.c}))); } catch { t = 'unparsable'; }
      counts[t] = (counts[t] ?? 0) + 1;
      if (counts[t] <= 3 || t === 'resync') log(`connection ${id}: client sent '${t}'`);
    }
  });
  socket.on('close', () => { closed = true; clearTimeout(timer); clearInterval(pace); releases.delete(release); log(`connection ${id} closed after ${sent} payloads; client message types ${JSON.stringify(counts)}`); });
  socket.on('error', (e) => log(`connection ${id} error ${e.message}`));
});
server.listen(0, '127.0.0.1', () => { const port = server.address().port; writeFileSync(portFile, String(port)); log(`listening on 127.0.0.1:${port}`); });
