#!/usr/bin/env node
// Reads one run-mode.sh out dir (host.log, guest.log, exit.txt, captures) and writes its row: started, objectives shown,
// end condition reached, end report shown, snapshots adopted and refused under the Rust decoder, the Rust view shadow's
// agree counts, and crashes. Writes <dir>/row.json and prints a markdown row.
//   node summarize.mjs [--id <mode id>] [--cap <s>] <run dir>
//   node summarize.mjs --table <run dir> [<run dir> ...]   one markdown table from the row.json files
// Everything comes from the engine logs the clients wrote (LogTC lines) and the driver's exit.txt; nothing is inferred.
import { readFileSync, writeFileSync, existsSync, readdirSync } from 'node:fs';
import { join, basename } from 'node:path';

const args = process.argv.slice(2);
const opt = (k) => { const i = args.indexOf(k); if (i < 0) return undefined; const v = args[i + 1]; args.splice(i, 2); return v; };
const table = args.includes('--table') && args.splice(args.indexOf('--table'), 1);

const read = (f) => existsSync(f) ? readFileSync(f, 'utf8') : '';
const strip = (l) => l.replace(/^\[[^\]]*\]\[[^\]]*\]/, '');
const tc = (log) => log.split(/\r?\n/).map(strip).filter((l) => l.startsWith('LogTC: ')).map((l) => l.slice(7));
const last = (lines, re) => { for (let i = lines.length - 1; i >= 0; i--) { const m = lines[i].match(re); if (m) return m; } return null; };
const first = (lines, re) => { for (const l of lines) { const m = l.match(re); if (m) return m; } return null; };
const all = (lines, re) => lines.map((l) => l.match(re)).filter(Boolean);

export function client(log) {
  const raw = log.split(/\r?\n/), L = tc(log);
  const c = { present: log.length > 0 };
  if (!c.present) return c;
  c.graphics = raw.map(strip).find((l) => /^LogTCGraphics: graphics (high|low):/.test(l))?.match(/graphics (high|low):/)?.[1];
  c.graphicsSwitches = raw.map(strip).filter((l) => /^LogTCGraphics: graphics: .*switch to /.test(l));
  const st = first(L, /^start #(\d+): match (\d+), map '([^']*)' (\d+x\d+)[^,]*, seat (-?\d+)/);
  c.started = !!st; if (st) Object.assign(c, { start: +st[1], match: +st[2], mapTitle: st[3], size: st[4], seat: +st[5] });
  const play = first(L, /^lobby: room \S+ play, mode (\w+), map ([^,\s]+)/);
  if (play) { c.mode = play[1]; c.map = play[2]; }
  // the native coverage lines (Client/TCTutorial.cpp ModesFrame, FTCModeWatch): what the HUD showed
  c.modesStart = first(L, /^modes: start: (.*)$/)?.[1];
  c.strips = all(L, /^modes: strip(?: at tick \d+)?: (.*)$/).map((m) => m[1]);
  c.goals = all(L, /^modes: goal(?: at tick \d+)?: (.*)$/).map((m) => m[1]);
  c.says = all(L, /^modes: say(?: at tick \d+)?: (.*)$/).map((m) => m[1]);
  c.banners = all(L, /^objective banner at tick \d+: (.*)$/).map((m) => m[1]);
  const dec = first(L, /^modes: decided (.*)$/); if (dec) c.decidedLine = dec[1];
  const sum = last(L, /^modes: summary \(([^)]*)\): (.*)$/); if (sum) c.modesSummary = sum[2];
  // the objectives log (-TCObjectivesLog): the first decided snapshot
  // a build without the coverage lines still logs the objectives strip (-TCObjectivesLog): the first one stands in
  const objStrip = first(L, /^objectives: tick \d+ .*\| strip: (.*)$/); if (objStrip) c.objectivesStrip = objStrip[1];
  const won = first(L, /^objectives: tick (\d+).* winner (-?\d+) end (\w+) \(/);
  if (won) c.end = { tick: +won[1], winner: +won[2], reason: won[3], seconds: Math.round(+won[1] / 20) };
  const lastObj = last(L, /^objectives: tick (\d+)/); if (lastObj) c.lastTick = +lastObj[1];
  const epi = first(L, /^epilogue shot: screenshot requested at tick \d+: \S+ \('([^']*)', '([^']*)'\)/); if (epi) c.stamp = `${epi[1]}: ${epi[2]}`;
  const res = first(L, /^result lobby: line '([^']*)', winner (-?\d+), reason (\S*), names \[([^\]]*)\], you (\S*); report (shown|hidden): '([^']*)'/);
  if (res) c.result = { line: res[1], winner: +res[2], reason: res[3].replace(/,$/, ''), names: res[4], you: res[5].replace(/;$/, ''), report: res[6], sub: res[7] };
  const ended = first(L, /^end switch: host asks to end match (\d+) at tick (\d+): (\S+)/); if (ended) c.hostEnded = { tick: +ended[2], sent: ended[3] };
  const d = last(L, /^rust view decoder \(([^)]*)\): requested (\S+), in effect (\S+); snapshots adopted (\d+) .*refused (\d+)/);
  if (d) c.decoder = { when: d[1], requested: d[2].replace(/,$/, ''), effect: d[3].replace(/;$/, ''), adopted: +d[4], refused: +d[5] };
  const sh = last(L, /^rust view shadow totals \(([^)]*)\): messages (\d+), apply errors (\d+); snapshots (\d+): agree (\d+), disagree (\d+)/);
  if (sh) c.shadow = { when: sh[1], messages: +sh[2], applyErrors: +sh[3], snapshots: +sh[4], agree: +sh[5], disagree: +sh[6] };
  const tut = all(L, /^tutorial: (step .*|cleared .*|won .*|the script .*)$/).map((m) => m[1]);
  if (tut.length) c.tutorial = tut;
  const capt = all(L, /^capture play at tick (\d+)/); c.capturePlays = capt.length;
  const drive = all(L, /^tutorial drive: (.*)$/); if (drive.length) c.driveProblems = drive.map((m) => m[1]).slice(0, 5);
  // -TCTutorialDrive's own tally per step (TCTutorial.cpp): the seat sent these itself
  const totals = all(L, /^tutorial drive totals: (.*)$/); if (totals.length) c.driveTotals = totals.map((m) => m[1]);
  // crashes and errors: the engine's own words
  const bad = raw.filter((l) => /Fatal error|Assertion failed|Ensure condition failed|Unhandled Exception|Segmentation fault|SIGSEGV|appError|LowLevelFatalError/.test(l));
  const errs = raw.map(strip).filter((l) => /^Log\w+: Error: /.test(l) || /^LogTC: Error/.test(l));
  c.crashLines = bad.slice(0, 5); c.errorLines = errs.length; c.errorSample = [...new Set(errs)].slice(0, 5);
  c.oom = raw.filter((l) => /Out of memory|VK_ERROR_OUT_OF/.test(l)).length;
  c.exitRequested = first(L, /^(exit after result: quitting|controls check .*quit)/) ? 'after the result' : raw.some((l) => /TCExitAfter|exit after \d+ s/.test(l)) ? 'exit timer' : undefined;
  return c;
}

function exits(dir) {
  const t = read(join(dir, 'exit.txt')), e = {};
  for (const m of t.matchAll(/^(host|guest|spectator) exit (-?\d+)/gm)) e[m[1]] = +m[2];
  e.guestStopped = /guest stopped by the driver/.test(t);
  return e;
}

function shots(dir) {
  const out = [];
  for (const sub of ['host', 'guest', 'spectator']) { const d = join(dir, sub); if (existsSync(d)) for (const f of readdirSync(d)) if (f.endsWith('.png')) out.push(`${sub}/${f}`); }
  return out.sort();
}

export function row(dir, id, cap) {
  const cmd = read(join(dir, 'cmdlines.txt'));
  id ??= cmd.match(/^mode id: (\S+);/m)?.[1] ?? basename(dir);
  // the cap the driver ran with: TC_MODES_CAP or the mode's own (the host's -TCEndAfter on cmdlines.txt)
  cap ??= cmd.match(/-TCEndAfter=(\d+)/)?.[1];
  const host = client(read(join(dir, 'host.log'))), guest = client(read(join(dir, 'guest.log'))), ex = exits(dir);
  const spectator = client(read(join(dir, 'spectator.log')));
  const r = { id, dir, cap: cap ? +cap : undefined, graphics: host.graphics ?? 'unrecorded', requestedGraphics: cmd.match(/^graphics: (high|low)$/m)?.[1], host, guest, ...(spectator.present && { spectator }), exits: ex, shots: shots(dir) };
  r.started = !!host.started && (!guest.present || !!guest.started);
  const shownOf = (c) => {
    const parts = [];
    const cut = (t, n = 110) => t.length > n ? `${t.slice(0, n - 3)}...` : t;
    if (c.strips?.length) parts.push(`${c.strips.length} strip line(s), first "${cut(c.strips[0])}"`);
    if (c.goals?.length) parts.push(`goal "${cut(c.goals[0])}"${c.goals.length > 1 ? ` (+${c.goals.length - 1} more)` : ''}`);
    if (c.says?.length) parts.push(`${c.says.length} scripted line(s), first "${cut(c.says[0])}"`);
    if (!parts.length && c.objectivesStrip) parts.push(`objectives strip "${c.objectivesStrip}"`);
    const steps = (c.tutorial ?? []).filter((t) => t.startsWith('step ')).length;
    if (steps) parts.push(`${steps} tutorial step line(s)`);
    return parts.join('; ');
  };
  r.objectives = { host: shownOf(host), guest: shownOf(guest) };
  r.module = cmd.match(/^module: (.*)$/m)?.[1];
  const end = host.end ?? guest.end;
  r.endReached = end ? `${end.reason}, winner team ${end.winner}, at ${Math.floor(end.seconds / 60)} min ${end.seconds % 60} s (tick ${end.tick})`
    : host.hostEnded ? `no: time cap ${r.cap ?? '?'} s, ended by the host at tick ${host.hostEnded.tick}` : `no: time cap ${r.cap ?? '?'} s`;
  const rep = host.result;
  r.endReport = rep ? (rep.report === 'shown' ? `shown: "${rep.line}" / "${rep.sub}"` : `hidden (${rep.line || 'no line'})`) : 'no result lobby';
  const n = (c) => c.decoder ? `${c.decoder.adopted}/${c.decoder.refused}` : '-';
  const s = (c) => c.shadow ? `${c.shadow.agree} of ${c.shadow.snapshots}${c.shadow.disagree ? `, ${c.shadow.disagree} disagree` : ''}` : '-';
  r.adoptedRefused = `host ${n(host)}${guest.present ? `, guest ${n(guest)}` : ''}${spectator.present ? `, spectator ${n(spectator)}` : ''}`;
  r.shadowAgree = `host ${s(host)}${guest.present ? `, guest ${s(guest)}` : ''}${spectator.present ? `, spectator ${s(spectator)}` : ''}`;
  const crash = (k, c) => ((ex[k] !== undefined && ex[k] !== 0 ? `${k} exit ${ex[k]}` : '') + (c.crashLines?.length ? ` ${k}: ${c.crashLines[0].trim()}` : '')).trim();
  // no exit.txt: the driver itself never finished (the run was interrupted from outside), so nothing says how it ended
  if (!existsSync(join(dir, 'exit.txt'))) { r.crashes = 'no exit recorded: the run was interrupted before its driver finished'; return r; }
  r.crashes = [crash('host', host), guest.present ? crash('guest', guest) : '', spectator.present ? crash('spectator', spectator) : ''].filter(Boolean).join('; ') || 'none (exits ' + Object.entries(ex).filter(([k]) => k !== 'guestStopped').map(([k, v]) => `${k} ${v}`).join(', ') + ')';
  return r;
}

const esc = (s) => String(s ?? '').replace(/\|/g, '\\|');
export function mdRow(r) {
  return `| ${r.id} | ${r.graphics ?? 'unrecorded'} | ${r.started ? `yes (${r.host.mode ?? '?'} on ${r.host.map ?? '?'})` : 'no'} | ${esc(r.objectives.host || 'nothing logged')} | ${esc(r.endReached)} | ${esc(r.endReport)} | ${r.adoptedRefused} | ${r.shadowAgree} | ${esc(r.crashes)} | ${basename(r.dir)} |`;
}
export const HEADER = '| Mode | Graphics | Started | Objectives shown (host) | End condition reached | End report shown | Adopted/refused | Shadow agree | Crashes | Run |\n| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |';

const main = process.argv[1] && import.meta.url.endsWith(basename(process.argv[1]));
if (main && table) {
  console.log(HEADER);
  for (const d of args) { const f = join(d, 'row.json'); if (existsSync(f)) console.log(mdRow(JSON.parse(readFileSync(f, 'utf8')))); }
} else if (main) {
  const id = opt('--id'), cap = opt('--cap'), dir = args[0];
  const r = row(dir, id, cap);
  writeFileSync(join(dir, 'row.json'), JSON.stringify(r, null, 1) + '\n');
  console.log(HEADER); console.log(mdRow(r));
}
