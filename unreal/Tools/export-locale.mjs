// Writes unreal/Content/Data/locale-es.json, the Spanish FTCLocale (Client/TCLocale.cpp) reads, from the pinned
// reference, which it only reads:
//   entries  client/es.js as [English, Spanish] pairs in Object.entries order (the order client/i18n.js builds its
//            exact map and pattern list in; the pattern sort is stable, so ties keep this order). The owner kept the
//            reference's translations and its keys: the English text the game writes (parity gap 15, CLI-120).
//   oracle   expected answers made by running the reference's own client/i18n.js under Node with the language stored
//            as 'es' (its localStorage key 'ww2rts-lang'), for the automation test ThreeCrossroads.Locale.Reference:
//              sequence  one module, every sample in order, so its memo carries over as it does in a page;
//              isolated  a fresh module per sample (an empty memo), for the patterns and glue samples;
//              english   the same module stored as 'en' answers every sample unchanged (t() is the identity);
//              native    the native HUD shapes (NATIVE below: what this client draws on one line where the browser has
//                        separate elements) and their Spanish under Client/TCLocale.cpp's native glue (columns, a
//                        trailing [tag] or cooldown). The reference has no such rule, so these answers come from a port
//                        of i18n.js core() with the native glue added, using es.js only; that port must first answer
//                        every sequence and isolated sample exactly as the reference module did, or this stops.
//              names     (slice 12) every unit, ability, support, faction look, recruit group and tier name the HUD can
//                        show (shared/sim.js UNITS and SUPPORT, client/main.js FACTIONS, client/hud.js GROUPS,
//                        shared/tech.js TIER_NAMES), with the reference module's own answer; the port with the native
//                        glue must answer each one the same, or this stops. Rows are [English, Spanish, source].
// It also fingerprints client/i18n.js, which TCLocale.cpp ports by hand: when the pin moves and it changed, this stops.
// Port the change into TCLocale.cpp, then update PORTED.
// Usage (from the repo root):
//   node unreal/Tools/export-locale.mjs [reference/ww2-rts]                 write the file
//   node unreal/Tools/export-locale.mjs [reference/ww2-rts] --check         exit 1 if the file is not what it would write
//   node unreal/Tools/export-locale.mjs [reference/ww2-rts] --print-hashes  the current fingerprint
//   node unreal/Tools/export-locale.mjs [reference/ww2-rts] --probe TEXT...  the reference's and this port's Spanish
//   node unreal/Tools/export-locale.mjs [reference/ww2-rts] --classify LOG   the same for each text of a client log's
//                                                                           'locale: kept in English' lines
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';

const here = import.meta.dirname;
const argv = process.argv.slice(2);
// --probe and --classify take the rest of the command line as their texts or file
const modeAt = argv.findIndex((a) => a === '--probe' || a === '--classify');
const args = modeAt < 0 ? argv : argv.slice(0, modeAt), mode = modeAt < 0 ? null : argv[modeAt], modeArgs = modeAt < 0 ? [] : argv.slice(modeAt + 1);
const check = args.includes('--check'), printHashes = args.includes('--print-hashes');
const reference = resolve(args.find((a) => !a.startsWith('--')) ?? join(here, '../../reference/ww2-rts'));
const out = join(here, '../Content/Data/locale-es.json');
const fail = (msg) => { console.error(`export-locale: ${msg}`); process.exit(1); };

const git = (...a) => execFileSync('git', ['-C', reference, ...a], { encoding: 'utf8' }).trim();
const pin = git('rev-parse', '--short', 'HEAD');
if (git('status', '--porcelain', '--untracked-files=no')) fail(`${reference} has local changes: generate from a clean pin only`);

// client/i18n.js as ported (the whole file: the lookup, the pattern rules, the memo and the language choice)
const PORTED = 'bd146a0ce7f5';
const fingerprint = createHash('sha256').update(readFileSync(join(reference, 'client/i18n.js'))).digest('hex').slice(0, 12);
if (printHashes) { console.log(`${fingerprint}  client/i18n.js`); process.exit(0); }
if (fingerprint !== PORTED) fail(`client/i18n.js changed since the port (${PORTED} -> ${fingerprint}): re-port it into Client/TCLocale.cpp, then update PORTED`);

const url = (p) => pathToFileURL(join(reference, p)).href;
const ES = (await import(url('client/es.js'))).default;
const entries = Object.entries(ES);
// What TCLocale.cpp assumes of the dictionary, checked here so a pin that breaks it stops instead of mistranslating:
// plain string values (es.js allows functions; none at this pin), and pattern keys whose only braces are the {0}..{9}
// holes (anything else would be a regex quantifier in i18n.js but a literal in the port).
for (const [en, es] of entries) {
  if (typeof es !== 'string') fail(`es.js value for ${JSON.stringify(en)} is a ${typeof es}: TCLocale.cpp ports string values only`);
  if (/\{\d\}/.test(en) && /[{}]/.test(en.replace(/\{\d\}/g, ''))) fail(`es.js pattern key ${JSON.stringify(en)} has a brace that is not a hole`);
}
const exactKeys = entries.filter(([en]) => !/\{\d\}/.test(en)).map(([en]) => en);
const patternKeys = entries.filter(([en]) => /\{\d\}/.test(en)).map(([en]) => en);

// The samples. Deterministic: built from the dictionary in its order, nothing random.
const words = exactKeys.filter((k) => !/[.!?:·()]/.test(k) && k.length <= 24); // short labels, safe inside a hole
const fillers = ['3', '12.5', 'Zorblat', 'M4 Sherman', 'Panzer IV', 'Road', ...words.filter((_, i) => i % 37 === 0)];
const sequence = [];
const isolated = [];
const add = (s, iso) => { sequence.push(s); if (iso) isolated.push(s); };
// every exact key, then with a full stop or a bang when it has none (the stop is kept after the translation)
for (const k of exactKeys) add(k, false);
for (const [i, k] of exactKeys.entries()) if (!/[.!?]$/.test(k)) add(k + (i % 3 === 2 ? '!' : '.'), i % 7 === 0);
// every pattern, twice: its holes filled from the fillers (a number before %), then with dictionary words
for (const [i, k] of patternKeys.entries()) {
  for (const round of [0, 1]) {
    let h = 0;
    const s = k.replace(/\{(\d)\}(%?)/g, (_, n, pc) => {
      h++;
      if (pc) return (round ? '42' : '12.5') + '%';
      return round ? words[(i * 5 + h * 11) % words.length] : fillers[(i + h) % fillers.length];
    });
    add(s, true);
  }
}
// texts the game glues together: sentences, ': ' and ' · ' joins, a trailing (hotkey)
for (let i = 0; i < 60; i++) {
  const a = words[(i * 13) % words.length], b = words[(i * 29 + 7) % words.length];
  add(`${a}. ${b}`, true); add(`${a}: ${b}`, true); add(`${a} · ${b}`, true); add(`${a} (${b})`, true); add(`${a} (Q)`, true);
}
// whitespace kept around the text, texts without a letter, misses, case, unit and place names
for (const s of ['  Load  ', '\tSave match\n', ' Load　', '123', '', '  ', '42%', '·', 'Zorblat', 'load', 'LOAD',
  'M4 Sherman', 'Panzer IV', 'Road', 'Save match. Load', 'Save match.Load', 'Load (', '(Load)', 'a (b) (c)', 'Load: ',
  ': Load', 'Load ·', 'Save match! Load? Saved matches.', 'Load?', 'Load...', 'Ammunition: 12.5%', 'Ammunition: x%',
  'Ammunition: 1,5%', 'Ammunition: %', 'Not a key. Not a key either.']) add(s, true);

// the oracle: the reference's own i18n.js, the language stored as 'es' (then 'en'), no page (Node has no document)
let stored = 'es', fresh = 0;
globalThis.localStorage = { getItem: (key) => (key === 'ww2rts-lang' ? stored : null), setItem() {} };
const i18n = async () => import(url('client/i18n.js') + `?export-locale=${fresh++}`);
const es = await i18n();
if (es.lang !== 'es') fail('the reference i18n.js did not take the stored Spanish');
const oracle = { sequence: sequence.map((s) => [s, es.t(s)]), isolated: [] };
for (const s of isolated) oracle.isolated.push([s, (await i18n()).t(s)]);
stored = 'en';
const en = await i18n();
if (en.lang !== 'en' || sequence.some((s) => en.t(s) !== s)) fail('the reference i18n.js in English changed a text');
oracle.english = sequence.length;

// ---- native glue (Client/TCLocale.cpp NativeColumns and NativeTail): a port of i18n.js core() with the native rules
// added, checked against the reference module before its native answers are kept
const NATIVE = [
  // the shapes slice 10's live Spanish run logged as kept in English (slice-10/run/final/kept-english-host.txt), as drawn
  'Three Crossroads native slice | connecting to ws://127.0.0.1:37265/ws (room s10final) | rtt -', 'room  |  | mode  | guest',
  'Three Crossroads native slice | connected, waiting for the room | rtt -', 'Three Crossroads native slice | in room | rtt 3 ms',
  'room s10final | lobby | mode conquest | host', 'Lobby  room s10final  |  Conquest  |  Default', 'Add AI', 'Add AI sent (room had 2 seats)',
  '2. Birch  (online)  ', 'Start sent: conquest on default, 3 seats', 'Lobby  room s10final  |  Conquest  |  Default  |  sandbox',
  'room s10final | play | mode conquest | host', '1. Alder (you)  (online)  team 0', '2. Birch  (online)  team 0', 'A match is running: joining it...',
  'Three Crossroads | seat 0 | tick 0 | MP 0 | units held 0 (own 0) | selected 0',
  'F1 controls | Space newest alert | Alt+H alert history | Shift+Space follow | H home | Ctrl+A army | . idle',
  'Nothing selected: left click or drag over your units', 'Groups: Ctrl+1..9 sets, 1..9 recalls (twice centres), Shift adds, Alt moves',
  'Recon Flight [Z] ', 'Artillery Barrage [C] ', 'Strafing Run [V] ', 'Smoke Barrage [B] ', 'Bombing Run [N] ', 'Dive Bomber [U] ', 'Paratroopers [P] ',
  'Fighter Cover [I] ', 'End match', 'Alder', '1 ms', 'Birch', 'Three Crossroads | seat 0 | tick 4 | MP 150 +4/s | units held 10 (own 5) | selected 0',
  'Recruit  |  150 MP  |  army 3/12', '3 selected: 2 Rifle Squad, 1 MG Team', 'hp 275 / 275  |  0 firing',
  'Formation line, spacing 1.00x, march together on, snap to trenches on  |  right-drag sets facing and width', 'Grenade (F) [auto]',
  'Suppressive Fire [auto]', 'Grenade 21s [auto]', 'Suppressive Fire (F) [auto]', 'Grenade [auto]', '2 selected: 1 MG Team, 1 Rifle Squad',
  'MG Team (unit 3)  hp 75 / 75  |  no target', 'Three Crossroads | seat 0 | tick 536 | MP 162 +4.1/s | units held 11 (own 3) | selected 1', 'Suppression 12',
  // the mixed lines slice 10 reported, as drawn: the score strip notice (rate, bonus, notice joined by '  |  '), a unit
  // row, the newest alert's hint
  'Chalk wins in 6:20 at this rate', 'Chalk wins in 6:20 at this rate  |  Blue catch-up +0.4 MP/s each',
  'Orange wins in 17:26 at this rate  |  Red catch-up +0.4 MP/s each  |  Point captured', 'MG Team (unit 3)  hp 75 / 75  |  firing at unit 18',
  'Space: look there', 'Our base is under attack!  (Space: look there)',
  // what the call sites would draw with the browser's English (the list in the slice 11 report)
  'Space or click: look there', 'End match (back to lobby)', '+ Add AI player', '3 units selected', '275/275 hp', 'Grenade (F) [A]', 'Team 0',
  'Recruit  |  150 MP  |  3/12', '162 MP  |  +4.1/s',
  // seen in slice 11's live Spanish run (slice-11/run/live1): the support bar with costs, the logistics rows
  'Recon Flight [Z] 60 MP', 'Fighter Cover [I] 120 MP', 'Rifle Squad #1: Ammunition: 100%  |  Provisions: 120 s  |  Supply line: 100%',
  'Rifle Squad (unit 1)  hp 73 / 100  |  firing at unit 19  |  1 stars', 'Supply overlay: No', 'Truck select: No',
  // slice 12: what slice 11's live Spanish runs still logged in English (slice-11/lead/live/*/kept-english-host.txt and
  // its HUD pictures), as drawn
  'HE Shell [A]', '88 mm AP Round [A]', 'Unload (Shift+E) 0/7', 'Unload (Shift+E) 0/4', 'Mount up (Shift+Q) 7 free',
  'Groups  1: 3   2: 5', '+2 more with supply lines',
  'Recruit  |  361 MP  |  army 6/12', 'Three Crossroads | seat 0 | tick 980 | MP 361 +5.4/s | units held 13 (own 8) | selected 6',
  '2. Birch  ',
  'Rifle Squad  100 MP', 'Ranger Squad  200 MP', 'Flamethrower Squad  180 MP', 'Sniper  160 MP', 'Medic Team  120 MP',
  // slice 12: what the HUD draws with the call sites in slice-12/l10n/CALL-SITES.md (the browser's English; in Spanish
  // the carrier buttons start with FTCLocale::TitleName, the Spanish head of their tooltip)
  'Montar (Shift+Q) 7 free', 'Descargar (Shift+E) 0/7',
  'Control group  1: 3   2: 5', 'Ctrl+1-9: Set a control group; Shift adds to it', 'Recruit  |  361 MP  |  6/12 units', '361 MP',
  'Barracks  |  361 MP  |  6/12 units', 'Rally (Shift+H): choose where new recruits gather',
  'Flamethrower Team  180 MP', 'Sniper Team  160 MP', 'Medics  120 MP', 'M4 Sherman  380 MP', 'Grenadiers  100 MP', 'Sanitary Team  120 MP', '2  Rifle Squad  |  1  M4 Sherman  |  1  M5 Stuart  |  1  Tiger  |  1  .30 cal MG',
  'M4 Sherman #16  |  600/600 hp', 'M4 Sherman #16: Ammunition: 100%  |  Provisions: 120 s  |  Fuel: 180 s  |  Emergency fuel: 60 s  |  Supply line: 100%',
];
if (new Set(NATIVE).size !== NATIVE.length) fail(`NATIVE lists a shape twice: ${NATIVE.find((s, i) => NATIVE.indexOf(s) !== i)}`);
const nativeEntries = Object.entries(ES);
const nExact = new Map(), nPatterns = [];
const HOLE = '((?:(?![.!?] ).)+?)';
for (const [en, esv] of nativeEntries) {
  if (!/\{\d\}/.test(en)) { nExact.set(en, esv); continue; }
  const order = [];
  const src = en.replace(/[.*+?^$()|[\]\\]/g, '\\$&').replace(/\{(\d)\}(%?)/g, (_, n, pc) => (order.push(+n), pc ? '([\\d.,]+)%' : HOLE));
  nPatterns.push({ re: new RegExp(`^${src}$`, 's'), order, es: esv, lit: en.replace(/\{\d\}/g, '').length });
}
nPatterns.sort((a, b) => b.lit - a.lit);
function makePort(native) {
  const memo = new Map();
  function columns(s) {
    let at = s.indexOf('  |  '), sep = 5;
    if (at <= 0) { at = s.indexOf(' | '); sep = 3; }
    if (at <= 0) { const m = /^(.+?)( {2,})(.+)$/s.exec(s); if (m) { at = m[1].length; sep = m[2].length; } }
    if (at > 0 && at + sep < s.length) { const a = spanish(s.slice(0, at)); return a + s.slice(at, at + sep) + spanish(s.slice(at + sep)); }
    // the first ' [tag]' that ends the text or is followed by a space and more text
    const m = /^(.+?) \[([^[\]]+)\](?: (.+))?$/s.exec(s);
    if (m) return spanish(m[1]) + ' [' + spanish(m[2]) + ']' + (m[3] === undefined ? '' : ' ' + spanish(m[3]));
    return null;
  }
  function tail(s) {
    let m = /^(.+)( \d+(?:\.\d+)?s)$/s.exec(s);
    if (m) return spanish(m[1]) + m[2];
    m = /^(.+)( #\d+)$/s.exec(s);
    if (m) return spanish(m[1]) + m[2];
    // slice 12: an order button's name, ' (key)' and a value starting with a digit (hud.js orderBtn: title, kbd, .val)
    m = /^(.+) \(([^()]+)\) (\d.*)$/s.exec(s);
    if (m) { const name = spanish(m[1]), key = spanish(m[2]); return name + ' (' + key + ') ' + spanish(m[3]); }
    return null;
  }
  function core(s) {
    const hit = nExact.get(s) ?? (/[.!?]$/.test(s) ? nExact.get(s.slice(0, -1)) : undefined);
    if (hit !== undefined) return hit === nExact.get(s) ? hit : hit + s.slice(-1);
    if (native) { const c = columns(s); if (c !== null) return c; }
    for (const p of nPatterns) {
      const m = p.re.exec(s);
      if (!m) continue;
      const args = [];
      p.order.forEach((n, i) => { args[n] = spanish(m[i + 1]); });
      return p.es.replace(/\{(\d)\}/g, (_, n) => args[n] ?? '');
    }
    let m = /^(.+?[.!?]) (\S.*)$/s.exec(s);
    if (m) return spanish(m[1]) + ' ' + spanish(m[2]);
    m = /^(.+?)(: | · )(.+)$/s.exec(s);
    if (m) return spanish(m[1]) + m[2] + spanish(m[3]);
    m = /^(.+) \(([^()]+)\)$/s.exec(s);
    if (m) return spanish(m[1]) + ' (' + spanish(m[2]) + ')';
    if (native) { const t = tail(s); if (t !== null) return t; }
    return s;
  }
  function spanish(s) {
    if (typeof s !== 'string' || !/[A-Za-z]/.test(s)) return s;
    let out = memo.get(s);
    if (out === undefined) {
      const [, pre, body, post] = /^(\s*)([\s\S]*?)(\s*)$/.exec(s);
      out = pre + core(body) + post;
      if (memo.size > 20000) memo.clear();
      memo.set(s, out); memo.set(out, out);
    }
    return out;
  }
  return { spanish, clear: () => memo.clear() };
}
// the port, with the native glue on, answers every reference sample as the reference module did
{
  const seq = makePort(true);
  const bad = oracle.sequence.filter(([s, want]) => seq.spanish(s) !== want);
  if (bad.length) fail(`the i18n port with the native glue differs from the reference on ${bad.length} sequence samples, e.g. ${JSON.stringify(bad[0])}`);
  const iso = makePort(true);
  const badIso = oracle.isolated.filter(([s, want]) => { iso.clear(); return iso.spanish(s) !== want; });
  if (badIso.length) fail(`the i18n port with the native glue differs from the reference on ${badIso.length} isolated samples, e.g. ${JSON.stringify(badIso[0])}`);
}
{
  const plain = makePort(false), glue = makePort(true);
  oracle.native = NATIVE.map((s) => { plain.clear(); glue.clear(); return [s, glue.spanish(s), plain.spanish(s)]; });
}

// ---- names (slice 12): every unit, ability, support, faction look, recruit group and tier name the HUD can show, read
// from the reference's own tables, with the reference module's own answer (a fresh module per name, language 'es').
// Some stay English in the browser because es.js has no entry; they stay English here too. The port with the native
// glue must answer every one of them as the reference does (the glue must never change a name), or this stops.
stored = 'es';
const sim = await import(url('shared/sim.js'));
const tech = await import(url('shared/tech.js'));
// a data literal declared at the top of a client module that cannot be imported under Node (it needs a page)
const literal = (file, re, what) => {
  const m = re.exec(readFileSync(join(reference, file), 'utf8'));
  if (!m) fail(`${file}: ${what} not found: update the name sources here`);
  return new Function(`return ${m[1]};`)();
};
const FACTIONS = literal('client/main.js', /\nconst FACTIONS = (\[[\s\S]*?\]);\n/, 'const FACTIONS');
const GROUPS = literal('client/hud.js', /\nconst GROUPS = (\[[^\]]*\]);\n/, 'const GROUPS');
if (!Array.isArray(FACTIONS) || FACTIONS.length < 4 || FACTIONS.some((f) => typeof f.name !== 'string' || typeof f.names !== 'object')) fail('client/main.js FACTIONS is not a list of {name, names}');
if (!Array.isArray(GROUPS) || GROUPS.some((g) => typeof g !== 'string')) fail('client/hud.js GROUPS is not a list of names');
const nameRows = [], seenName = new Set();
const addName = (text, source) => { if (typeof text === 'string' && text && !seenName.has(text)) { seenName.add(text); nameRows.push([text, source]); } };
for (const [k, u] of Object.entries(sim.UNITS)) { addName(u.name, `shared/sim.js UNITS.${k}.name`); addName(u.ab?.name, `shared/sim.js UNITS.${k}.ab.name`); }
for (const [k, s] of Object.entries(sim.SUPPORT)) addName(s.name, `shared/sim.js SUPPORT.${k}.name`);
for (const f of FACTIONS) for (const [k, n] of Object.entries(f.names)) addName(n, `client/main.js FACTIONS ${f.name}.names.${k}`);
GROUPS.forEach((g, i) => addName(g, `client/hud.js GROUPS[${i}]`));
tech.TIER_NAMES.forEach((n, i) => addName(n, `shared/tech.js TIER_NAMES[${i}]`));
oracle.names = [];
for (const [text, source] of nameRows) oracle.names.push([text, (await i18n()).t(text), source]);
{
  const glue = makePort(true);
  const bad = oracle.names.filter(([s, want]) => { glue.clear(); return glue.spanish(s) !== want; });
  if (bad.length) fail(`the native glue changes ${bad.length} reference names, e.g. ${JSON.stringify(bad[0])}`);
}

// --probe text...: the reference module's answer and this port's (with the native glue), each on a fresh memo
// --classify file: the same for each text of a 'locale: kept in English' log (one quoted text per line)
if (mode) {
  const glue = makePort(true), keptNames = new Map(oracle.names.filter(([s, es]) => s === es).map(([s, , src]) => [s, src]));
  let texts = modeArgs;
  if (mode === '--classify') {
    if (modeArgs.length !== 1 || !existsSync(modeArgs[0])) fail('--classify takes one log file');
    texts = readFileSync(modeArgs[0], 'utf8').split('\n').map((l) => /'(.*)'\s*$/.exec(l)?.[1]).filter((s) => s !== undefined);
  }
  for (const s of texts) {
    glue.clear();
    const ref = (await i18n()).t(s), port = glue.spanish(s), bare = s.replace(/ \[[^\]]+\]$/, '');
    const why = port !== s ? 'translated' : keptNames.has(bare) ? `kept English, as in the browser (${keptNames.get(bare)} has no es.js entry)` : 'kept English';
    console.log(`${JSON.stringify(s)}\n  reference ${JSON.stringify(ref)}\n  native    ${JSON.stringify(port)}\n  ${why}`);
  }
  process.exit(0);
}

// one pair per line (diffable when the pin moves); every string goes through JSON.stringify
const rows = (pairs) => `[\n${pairs.map((p) => JSON.stringify(p)).join(',\n')}\n]`;
const head = { reference: pin, source: 'client/es.js', ported: PORTED, exact: exactKeys.length, patterns: patternKeys.length };
const text = `${JSON.stringify(head).slice(0, -1)},\n"entries": ${rows(entries)},\n"oracle": {"english": ${oracle.english},\n`
  + `"sequence": ${rows(oracle.sequence)},\n"isolated": ${rows(oracle.isolated)},\n"native": ${rows(oracle.native)},\n"names": ${rows(oracle.names)}}}\n`;
JSON.parse(text); // well formed
if (check) {
  if (!existsSync(out) || readFileSync(out, 'utf8') !== text) fail(`${out} is not what this pin writes: run without --check`);
  console.log(`export-locale: ${out} is current (${pin})`);
} else {
  writeFileSync(out, text);
  const translated = oracle.sequence.filter(([s, t]) => s !== t).length;
  console.log(`wrote ${entries.length} entries (${exactKeys.length} exact, ${patternKeys.length} patterns) and ${sequence.length} + ${isolated.length} oracle samples (${translated} changed) from ${pin} to ${out}`);
  const nChanged = oracle.native.filter(([s, g]) => g !== s).length, nByGlue = oracle.native.filter(([, g, p]) => g !== p).length;
  console.log(`native: ${oracle.native.length} HUD shapes, ${nChanged} translated at least in part, ${nByGlue} of them by the native glue`);
  const kept = oracle.names.filter(([s, es]) => s === es).map(([s]) => s);
  console.log(`names: ${oracle.names.length} from the reference's tables, ${kept.length} kept English by the reference itself: ${kept.join(', ')}`);
}
