// node --test unreal/Tools/modes/summarize.test.mjs
// The row reader against a synthetic engine log in the engine's own shape (CRLF line ends, the [time][frame] prefix).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { client, row, mdRow } from './summarize.mjs';

const at = (n, s) => `[2026.10.14-12.00.${String(n).padStart(2, '0')}:000][${n}]${s}`;
const hostLines = [
  'LogInit: Command Line: -game',
  'LogTCGraphics: graphics high: render scale 100%',
  'LogTC: lobby: room m1 play, mode conquest, map default, host, seat 0 of 3 [Alder, Fern, AI 1 (AI)]',
  "LogTC: start #1: match 1, map 'Three Crossroads' 80x80, seat 0 (confirmed), spawn (5, 5)",
  "LogTC: modes: start: match 1 on 'Three Crossroads' (80x80), seat 0 of 3 (team 0), teams [0,0,1], lobby mode conquest, 7 authored point(s)",
  'LogTC: modes: strip at tick 2: conquest | team 0 0 / 1800 VP | line: Scores tied',
  'LogTC: modes: goal at tick 5: Objective: Hold | shown: Objetivo: Mantener',
  'LogTC: modes: say at tick 6: The bridge is wired',
  'LogTC: objectives: tick 2 vp [0,0,0] points [] winner -2 | strip: team 0 0 / 1800 VP | line: Scores tied',
  'LogTC: objectives: tick 10380 vp [1800,0,0] points [] winner 0 end vp (victory points)',
  "LogTC: modes: decided at tick 10375 (8 min 38 s): winner team 0 (won), reason vp; stamp 'Victory' 'x'",
  'LogTC: modes: summary (back to the lobby): mode conquest, match 1, 5 snapshot(s), strip lines 1',
  "LogTC: result lobby: line 'Victory', winner 0, reason vp, names [Alder, Fern, AI 1], you victory; report shown: 'decided on victory points after 8 min 39 s'",
  'LogTC: tutorial drive totals: seat 0, step 1: sent 2 (move 2)',
  'LogTC: rust view decoder (end): requested rust, in effect rust; snapshots adopted 5248 (x), refused 0',
  'LogTC: rust view shadow totals (end): messages 5531, apply errors 0; snapshots 5248: agree 5248, disagree 0',
];
const log = (lines) => lines.map((l, i) => at(i, l)).join('\r\n') + '\r\n';

test('CRLF logs: the coverage lines, the end, the decoder and the shadow are read', () => {
  const c = client(log(hostLines));
  assert.equal(c.started, true);
  assert.equal(c.mode, 'conquest');
  assert.equal(c.graphics, 'high');
  assert.equal(client(log(hostLines.filter((l) => !l.includes('graphics high:')))).graphics, undefined);
  assert.match(c.modesStart, /^match 1 on 'Three Crossroads'/);
  assert.deepEqual(c.strips, ['conquest | team 0 0 / 1800 VP | line: Scores tied']);
  assert.deepEqual(c.goals, ['Objective: Hold | shown: Objetivo: Mantener']);
  assert.deepEqual(c.says, ['The bridge is wired']);
  assert.match(c.decidedLine, /^at tick 10375/);
  assert.match(c.modesSummary, /^mode conquest/);
  assert.deepEqual(c.end, { tick: 10380, winner: 0, reason: 'vp', seconds: 519 });
  assert.equal(c.result.report, 'shown');
  assert.deepEqual(c.decoder, { when: 'end', requested: 'rust', effect: 'rust', adopted: 5248, refused: 0 });
  assert.equal(c.shadow.agree, 5248);
  assert.deepEqual(c.driveTotals, ['seat 0, step 1: sent 2 (move 2)']);
  assert.equal(c.driveProblems, undefined);
  assert.deepEqual(c.crashLines, []);
});

test('a World Conquest start line (its map note before the seat) counts as started', () => {
  const c = client(log(["LogTC: start #1: match 1, map 'World Conquest (Huge)' 512x512 (World: discovered cells only), seat 1 (confirmed), spawn (195, 961)"]));
  assert.equal(c.started, true);
  assert.equal(c.seat, 1);
  assert.equal(c.size, '512x512');
});

test('a build without coverage lines falls back to the objectives strip', () => {
  const c = client(log(hostLines.filter((l) => !l.includes('modes: '))));
  assert.deepEqual(c.strips, []);
  assert.equal(c.objectivesStrip, 'team 0 0 / 1800 VP | line: Scores tied');
});

test('row: cap from the host command line, crash lines, markdown', () => {
  const d = mkdtempSync(join(process.cwd(), '.summarize-test-'));
  try {
    writeFileSync(join(d, 'cmdlines.txt'), 'mode id: horde; room m1\nmodule: 2026-10-14, 1 bytes, md5 abc\nhost: x -TCEndAfter=900 y\n');
    writeFileSync(join(d, 'host.log'), log([...hostLines.slice(0, 6), 'LogCore: Error: appError called: Fatal error: [File:x]']));
    writeFileSync(join(d, 'exit.txt'), 'host exit 139\n');
    const r = row(d);
    assert.equal(r.id, 'horde');
    assert.equal(r.cap, 900);
    assert.equal(r.module, '2026-10-14, 1 bytes, md5 abc');
    assert.equal(r.endReached, 'no: time cap 900 s');
    assert.match(r.crashes, /^host exit 139 host: .*appError called/);
    assert.equal(r.graphics, 'high');
    assert.match(mdRow(r), /^\| horde \| high \| yes \(conquest on default\) \| 1 strip line\(s\), first "conquest \\\| team 0/);
    // a spectator (tour runs): its counts and exit join the row
    writeFileSync(join(d, 'host.log'), log(hostLines));
    writeFileSync(join(d, 'spectator.log'), log(hostLines));
    writeFileSync(join(d, 'exit.txt'), 'host exit 0\nspectator exit 0\n');
    const w = row(d);
    assert.match(w.adoptedRefused, /, spectator 5248\/0$/);
    assert.match(w.shadowAgree, /, spectator 5248 of 5248$/);
    assert.match(w.crashes, /^none \(exits host 0, spectator 0\)$/);
  } finally { rmSync(d, { recursive: true, force: true }); }
});
