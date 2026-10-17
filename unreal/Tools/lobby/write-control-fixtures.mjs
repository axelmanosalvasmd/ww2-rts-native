// Writes the generated test fixtures of Client/TCMatchControls.cpp: lobby messages recorded by protocol-probe.mjs from
// the isolated reference server (publicUrl replaced by a placeholder, the map and save lists dropped), the messages the
// probe sent (JSON.stringify, as the browser sends them), and the Spanish of every text these controls and the room
// browser show, answered by the reference's own client/i18n.js spanish() under node.
// usage: node write-control-fixtures.mjs <reference dir> <probe.jsonl> <TCMatchControls.cpp> [--check]
import { readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
const [ref, jsonl, cpp, mode] = process.argv.slice(2);
const rows = readFileSync(jsonl, 'utf8').trim().split('\n').map((l) => JSON.parse(l));
const got = (who) => rows.filter((r) => r.who === who && r.got).map((r) => r.got);
const lobbies = (who) => got(who).filter((m) => m.t === 'lobby');
const pick = (who, pred, what) => { const m = lobbies(who).find(pred); if (!m) throw new Error('no lobby for ' + what); const c = { ...m }; delete c.maps; delete c.saves; delete c.hordeMaps; c.publicUrl = 'https://public.example'; return JSON.stringify(c); };
let afterWatch = false;
const fixtures = {
  HostLobby: pick('Alder', (m) => m.state === 'lobby' && m.players.length === 2 && m.players.every((p) => p.connected && !p.ai) && m.mode === 'conquest', 'host lobby'),
  GuestWatching: pick('Birch', (m) => m.state === 'lobby' && m.spectator === true, 'guest watching'),
  GuestSeated: pick('Birch', (m) => { if (m.spectator) afterWatch = true; return afterWatch && m.state === 'lobby' && !m.spectator && m.you === 1; }, 'guest seated'),
  GuestRenamed: pick('Birch', (m) => m.players[1]?.name === 'Birch  co' && m.state === 'lobby', 'guest renamed'),
  GuestPlay: pick('Birch', (m) => m.state === 'play' && m.you === 1, 'guest in play'),
  HostPlayOffline: pick('Alder', (m) => m.state === 'play' && m.players[1] && !m.players[1].connected && !m.players[1].ai, 'host with an offline seat'),
  HostPlayHanded: pick('Alder', (m) => m.state === 'play' && m.players[1]?.ai === true, 'host after handAi'),
  WatcherPlay: pick('Cedar', (m) => m.state === 'play' && m.spectator && !m.amHost, 'watcher in play'),
  WatcherHostPlay: pick('Cedar', (m) => m.state === 'play' && m.spectator && m.amHost, 'watcher hosting in play'),
  WatcherEnded: pick('Cedar', (m) => m.state === 'lobby' && m.result?.ended === true && m.spectator, 'watcher after end'),
};
const sent = [...new Set(rows.filter((r) => r.sent && r.sent.t !== 'hello').map((r) => JSON.stringify(r.sent)))];
const { spanish } = await import(pathToFileURL(ref + '/client/i18n.js').href);
const texts = ['Watch as a spectator', 'Take a seat', 'Spectators see the whole map and command nothing. With only AI players seated, a spectator hosts',
  "Spectators share one team's explored world and command nothing", 'Name', 'Copy', 'Copied', 'Press Ctrl+C', 'Copy the invite link',
  'Invite', 'Send this link to friends. It always opens this room.', 'Room', 'Join', 'New', 'Join the room code typed on the left', 'A private room with a random code',
  'Menu', 'Menu: restart, end or leave the match', 'Pause match', 'Resume match', 'Save match', "Save this match to load it again later from this room's lobby",
  'Restart match', 'End match (back to lobby)', 'Leave game', 'Click again to confirm', 'Hand Birch to AI', 'Send next wave', 'Skip the break',
  'Sandbox', 'Enemy', 'Mine', 'Spawn', 'Then click the map, once per unit. Right-click stops', 'Rifle Squad', 'Medium Tank',
  'You left the match; an AI took over your army. Reload to rejoin the lobby when the match is over.',
  'Browse matches', 'Quick Play', 'Tutorial', 'Refresh', 'All modes', 'Open seats only', 'Connecting…', '5 public matches · Live', '1 public match · Live', 'Offline',
  'Match / map', 'Mode', 'Seats', 'Status', 'In progress', 'Open', 'Full', '2 humans', '1 human · 2 AI', 'Join Friday night',
  'No matches fit these filters.', 'No public matches yet. Create one and invite the first players.', 'Cannot reach the match server. Retry with Refresh.',
  'Create a match', 'Match name', 'Visibility', 'Public: shown in the match browser', 'Unlisted: share the room link', 'Create match',
  'Have a room code?', 'Room code', 'Join room', 'Your name', 'Open skirmish', 'The directory is unavailable or still loading. Please try again.', 'World Conquest', 'Horde'];
const es = texts.map((s) => [s, spanish(s)]);
const lit = (s) => 'TEXT("' + [...String(s)].map((ch) => { const c = ch.codePointAt(0); if (ch === '"' || ch === '\\') return '\\' + ch; if (c < 0x20 || c > 0x7e) return '\\u' + c.toString(16).padStart(4, '0').toUpperCase(); return ch; }).join('') + '")';
const block = ['// BEGIN GENERATED FIXTURES (round-15 lobby tools/write-control-fixtures.mjs: lobbies recorded live from an isolated',
  '// reference server at e4e579a by protocol-probe.mjs, the messages it sent, and client/i18n.js spanish() under node)',
  'namespace TCCtlFx', '{',
  ...Object.entries(fixtures).map(([k, v]) => `\tconst TCHAR* const ${k} = ${lit(v)};`),
  '\tconst TCHAR* const Sent[] = {', ...sent.map((s) => `\t\t${lit(s)},`), '\t};',
  '\tstruct FEs { const TCHAR* En; const TCHAR* Es; };', '\tconst FEs Spanish[] = {', ...es.map(([a, b]) => `\t\t{ ${lit(a)}, ${lit(b)} },`), '\t};',
  '}', '// END GENERATED FIXTURES'].join('\n');
const src = readFileSync(cpp, 'utf8');
const re = /\/\/ BEGIN GENERATED FIXTURES[\s\S]*?\/\/ END GENERATED FIXTURES/;
if (!re.test(src)) { console.error('no generated fixtures block'); process.exit(2); }
const next = src.replace(re, block);
if (mode === '--check') { if (next !== src) { console.error('fixtures differ'); process.exit(1); } console.log('fixtures up to date'); }
else { writeFileSync(cpp, next); console.log(`wrote ${Object.keys(fixtures).length} lobbies, ${sent.length} sent messages, ${es.length} Spanish answers`); }
