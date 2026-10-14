// Writes the unit ability facts FTCAbilities (Client/TCAbilities.cpp) reads, and the cases its automation test checks,
// from the pinned reference, which it only reads:
//   unreal/Source/ThreeCrossroads/Client/TCAbilitiesData.inl   the table: shared/sim.js UNITS[t].ab (id, name, cd),
//       AB_MUN and abCost (the Classic and World Munitions price), AUTO_FLAG, RIDING_FLAG; client/main.js PRIORITY, AIMED,
//       FACTIONS names, setAim hints, aimShape radii and the throw marker colour; client/hud.js AIMED and the button
//       icon; client/keys.js the 'ability' binding; client/autocast.js its storage key;
//   unreal/Source/ThreeCrossroads/Client/TCAbilitiesCases.inl  expected answers made by running the reference's own
//       code on fixed snapshots: client/availability.js availability({ t: 'ability' }), client/main.js fKeyType,
//       useAbility, throwAt and the aimed-ability click, client/hud.js the ability buttons, client/autocast.js.
// It also fingerprints the reference code TCAbilities.cpp ports by hand (PORTED below). When the pin moves and one of
// them changed, it stops: port the change into TCAbilities.cpp, then update that fingerprint.
// Usage (from the repo root):
//   node unreal/Tools/export-ability-table.mjs [reference/ww2-rts]                 write both files
//   node unreal/Tools/export-ability-table.mjs [reference/ww2-rts] --check         exit 1 if either is not what it would write
//   node unreal/Tools/export-ability-table.mjs [reference/ww2-rts] --print-hashes  the current fingerprints
import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';

const here = import.meta.dirname;
const args = process.argv.slice(2);
const check = args.includes('--check'), printHashes = args.includes('--print-hashes');
const reference = resolve(args.find((a) => !a.startsWith('--')) ?? join(here, '../../reference/ww2-rts'));
const dataOut = join(here, '../Source/ThreeCrossroads/Client/TCAbilitiesData.inl');
const casesOut = join(here, '../Source/ThreeCrossroads/Client/TCAbilitiesCases.inl');
const fail = (msg) => { console.error(`export-ability-table: ${msg}`); process.exit(1); };

const git = (...a) => execFileSync('git', ['-C', reference, ...a], { encoding: 'utf8' }).trim();
const pin = git('rev-parse', '--short', 'HEAD');
if (git('status', '--porcelain', '--untracked-files=no')) fail(`${reference} has local changes: generate from a clean pin only`);

const load = (p) => import(pathToFileURL(join(reference, p)).href);
const sim = await load('shared/sim.js');
const { availability, cooldownSeconds } = await load('client/availability.js');
const { createAutocast } = await load('client/autocast.js');
const { bindings } = await load('client/keys.js');
const { UNITS, UNIT_TYPES, CFG, AB_MUN, abCost, AUTO_FLAG, RIDING_FLAG, UNIT_ROW } = sim;

// ---------- the reference code TCAbilities.cpp ports by hand ----------
// [file, the line that starts it, fingerprint]. A fingerprint covers that line and what belongs to it (deeper-indented
// lines, then a closing bracket at its own indent).
const PORTED = [
  ['shared/sim.js', 'export const AB_MUN = ', '6e935b1d529a'],
  ['shared/sim.js', 'export const abCost = ', '4e1b82b2bdd0'],
  ['shared/sim.js', 'const AIMED_AB = ', 'fcb96ef42077'],
  ['shared/sim.js', 'function payAb(', '956a85f54c58'],
  ['shared/sim.js', 'export const isConstructionMode = ', 'b07f6f4a7d72'],
  ['shared/sim.js', '  const mine = (id) =>', '6bfbec3de4ff'],
  ['shared/sim.js', '  } else if (cmd.t === \'ability\') {', 'fe00feb0e119'],
  ['shared/sim.js', '  } else if (cmd.t === \'autocast\') {', '9191752717ae'],
  ['shared/sim.js', 'function playerRow(', '71f37f4f7639'],
  ['shared/sim.js', 'export const AUTO_FLAG = ', '286706222a12'],
  ['client/availability.js', 'export const cooldownSeconds = ', '4ce17c432a17'],
  ['client/availability.js', 'const resources = ', '0505b8bf95a2'],
  ['client/availability.js', 'export const snapshotUnits = ', '64f907aae690'],
  ['client/availability.js', 'export function availability(', 'edce3b256bee'],
  ['client/main.js', 'const FACTIONS = [', '568289f6b19a'],
  ['client/main.js', 'const facOf = ', '61ea046c271d'],
  ['client/main.js', 'const observing = ', '0b71dcff681f'],
  ['client/main.js', 'const explainUnavailable = ', '4fd42d615cdd'],
  ['client/main.js', 'function cancelAim(', 'bd2c46eba9ab'],
  ['client/main.js', 'function setAim(', 'd61cbc199f07'],
  ['client/main.js', 'const PRIORITY = ', '589392206ff0'],
  ['client/main.js', 'function fKeyType(', '27d70442984f'],
  ['client/main.js', 'const AIMED = {', '506d39c00312'],
  ['client/main.js', 'function useAbility(', 'd67267a072bc'],
  ['client/main.js', 'function throwAt(', 'cca1eaa6070d'],
  ['client/main.js', '    if (e.button === 2) { cancelAim(); return; }', 'd96a084f335d'],
  ['client/main.js', '    if (!g) { feedback.show(denySentence(\'blocked\')); return; }', '98f70339854e'],
  ['client/main.js', '    if (AIMED[kind]) {', 'df10c4ba2259'],
  ['client/main.js', '    if (targeting && e.button === 2) { cancelAim(); return; }', 'c6dca2e6d6ca'],
  ['client/main.js', '  if (kind === \'grenade\' || kind === \'barrage\'', '1ca0e03b6588'],
  ['client/main.js', '  if (!operative) autocast.adopt(', 'f8a1979efdcc'],
  ['client/main.js', '  autocast: (t) =>', '7cc45c6ad973'],
  ['client/hud.js', 'const AIMED = new Set(', 'e44b657e8ac8'],
  ['client/hud.js', '  const name = (t, slot = ctx.me)', 'ab2d75101d62'],
  ['client/hud.js', '    const types = ctx.PRIORITY.filter(', 'baddb7603bac'],
  ['client/hud.js', '        types.map((t) => { const ab = UNITS[t].ab;', '6fa1999e8bf5'],
  ['client/hud.js', '        if (UNITS[a]) b.oncontextmenu', '95b751680251'],
  ['client/hud.js', '    const fType = ctx.fType();', 'a94178336f75'],
  ['client/hud.js', '      else if (UNITS[a]) {', '64695030fc75'],
  ['client/autocast.js', 'const KEY = ', '02a7a5967f3e'],
  ['client/autocast.js', 'export function createAutocast(', '689d216fef34'],
  ['client/keys.js', 'export function match(', '233472259169'],
];
const sources = new Map();
const text = (file) => { if (!sources.has(file)) sources.set(file, readFileSync(join(reference, file), 'utf8')); return sources.get(file); };
function excerpt(file, anchor) {
  const lines = text(file).split('\n'), hits = [];
  lines.forEach((l, i) => { if (l.trimStart().startsWith(anchor.trimStart()) && (anchor !== anchor.trimStart() ? l.startsWith(anchor) : true)) hits.push(i); });
  if (hits.length !== 1) fail(`${file}: ${hits.length} lines start with ${JSON.stringify(anchor)} (want exactly one)`);
  const indent = (l) => l.length - l.trimStart().length, i0 = hits[0], ind = indent(lines[i0]), body = [lines[i0]];
  for (let i = i0 + 1; i < lines.length; i++) {
    const l = lines[i];
    if (!l.trim() || indent(l) > ind) { body.push(l); continue; }
    if (indent(l) === ind && /^[}\])]/.test(l.trimStart())) body.push(l);
    break;
  }
  while (body.length && !body.at(-1).trim()) body.pop();
  return body.join('\n');
}
const fingerprint = (file, anchor) => createHash('sha256').update(excerpt(file, anchor)).digest('hex').slice(0, 12);
const stale = [];
for (const [file, anchor, want] of PORTED) {
  const got = fingerprint(file, anchor);
  if (printHashes) console.log(`${got}  ${file}  ${anchor.trim()}`);
  else if (got !== want) stale.push(`${file} ${JSON.stringify(anchor.trim())}: ${want} -> ${got}`);
}
if (printHashes) process.exit(0);
if (stale.length) fail(`reference code ported by hand changed since the port (re-port it into TCAbilities.cpp, then update PORTED):\n  ${stale.join('\n  ')}`);

// ---------- facts read from the reference source text (module-private there) ----------
const literal = (file, re, what) => {
  const m = text(file).match(re);
  if (!m) fail(`${file}: ${what} was not found where the port expects it`);
  return new Function(`return (${m[1]});`)();
};
const PRIORITY = literal('client/main.js', /^const PRIORITY = (\[[^\]]*\]);/m, 'PRIORITY');
const MAIN_AIMED = literal('client/main.js', /^const AIMED = (\{[^}]*\});/m, 'AIMED');
const HUD_AIMED = literal('client/hud.js', /^const AIMED = new Set\((\[[^\]]*\])\);/m, 'AIMED');
const SIM_AIMED = literal('shared/sim.js', /^const AIMED_AB = new Set\((\[[^\]]*\])\);/m, 'AIMED_AB');
const FACTIONS = literal('client/main.js', /^const FACTIONS = (\[[\s\S]*?\n\]);/m, 'FACTIONS');
const HINTS = literal('client/main.js', /\$\('hint'\)\.textContent = (\{[^}]*\})\[kind\] \?\? '[^']*';/, 'setAim hints');
const HINT_SUFFIX = text('client/main.js').match(/\$\('hint'\)\.textContent \+= '([^']*)';/)?.[1];
if (HINT_SUFFIX === undefined) fail('client/main.js setAim: the hint suffix was not found');
const STORAGE_KEY = text('client/autocast.js').match(/^const KEY = '([^']*)';/m)?.[1];
if (STORAGE_KEY === undefined) fail('client/autocast.js KEY was not found');
const MARKER = excerpt('client/main.js', 'function throwAt(').match(/marker\(g\.x, g\.z, 0x([0-9a-f]{6})\)/)?.[1];
if (!MARKER) fail('client/main.js throwAt: the marker colour was not found');

// The aimed ids must be one set everywhere the port treats them as one: the browser's two AIMED tables, the server's
// AIMED_AB, availability's queue rule and the server's queue rule.
const aimedIds = Object.keys(MAIN_AIMED).sort();
const same = (xs) => JSON.stringify([...xs].sort()) === JSON.stringify(aimedIds);
if (!same(HUD_AIMED) || !same(SIM_AIMED)) fail(`the aimed abilities differ: main.js ${aimedIds}, hud.js ${HUD_AIMED}, sim.js ${SIM_AIMED}`);
const availQueue = excerpt('client/availability.js', 'export function availability(').match(/action\.queue === true && (\[[^\]]*\])\.includes\(UNITS\[action\.unit\]\.ab\.id\)/)?.[1];
if (!availQueue || !same(new Function(`return ${availQueue}`)())) fail('client/availability.js: the queued-ability rule no longer names the aimed abilities');
const simQueue = excerpt('shared/sim.js', '  } else if (cmd.t === \'ability\') {').match(/\(ab\.id === '(\w+)' \|\| ab\.id === '(\w+)' \|\| ab\.id === '(\w+)'\)\)/);
if (!simQueue || !same(simQueue.slice(1))) fail('shared/sim.js command ability: the queued-ability rule no longer names the aimed abilities');
for (const id of aimedIds) if (typeof HINTS[id] !== 'string') fail(`client/main.js setAim has no hint for ${id}`);

// client/main.js aimShape: grenade rifle.ab.radius, barrage rocket.w.spread, satchel ranger.ab.radius
const shape = excerpt('client/main.js', '  if (kind === \'grenade\' || kind === \'barrage\'');
const wantShape = "kind === 'grenade' ? UNITS.rifle.ab.radius : kind === 'barrage' || kind === 'area' ? UNITS.rocket.w.spread : kind === 'satchel' ? UNITS.ranger.ab.radius : 2";
if (!shape.includes(wantShape)) fail('client/main.js aimShape: the aimed-ability preview radii moved');
const AIM_RADIUS = { grenade: UNITS.rifle.ab.radius, barrage: UNITS.rocket.w.spread, satchel: UNITS.ranger.ab.radius };
if (!same(Object.keys(AIM_RADIUS))) fail('aimShape radii do not cover the aimed abilities');

const key = bindings.filter((b) => b.id === 'ability');
if (key.length !== 1 || !/^Key[A-Z]$/.test(key[0].code) || key[0].shift || key[0].ctrl || key[0].alt || key[0].context !== 'global') fail(`client/keys.js 'ability' binding changed: ${JSON.stringify(key)}`);
const KEY = key[0];
// the other chords on the same key, for the header comment (the controller resolves them: client/keys.js match)
const sharing = bindings.filter((b) => b.code === KEY.code && b.id !== 'ability').map((b) => `${b.label} ${b.id} (${b.context})`);

// ---------- the table ----------
const ABS = UNIT_TYPES.filter((t) => UNITS[t].ab && UNITS[t].ab.id !== 'none');
for (const t of PRIORITY) if (!ABS.includes(t)) fail(`PRIORITY names ${t}, which has no ability`);
const ids = [...new Set(ABS.map((t) => UNITS[t].ab.id))];
const KNOWN = ['grenade', 'suppress', 'ap', 'smoke', 'barrage', 'satchel', 'ura'];
for (const id of ids) if (!KNOWN.includes(id)) fail(`ability ${id} is new: port its command handling (shared/sim.js command 'ability') first`);
const icon = excerpt('client/hud.js', '        types.map((t) => { const ab = UNITS[t].ab;');
if (!icon.includes("ab.id === 'smoke' ? 'smokeab' : ab.id")) fail('client/hud.js: the ability button icon rule moved');
const iconOf = (id) => (id === 'smoke' ? 'smokeab' : id);

const esc = (v) => String(v).replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/[^\x20-\x7e]/g, (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, '0')}`);
const s = (v) => `TEXT("${esc(v)}")`;
const b = (v) => (v ? 'true' : 'false');
const int = (v, what) => { if (!Number.isInteger(v)) fail(`${what} is not an integer: ${v}`); return String(v); };
const real = (v, what) => { if (!Number.isFinite(v)) fail(`${what} is not finite: ${v}`); return Number.isInteger(v) ? `${v}.0` : String(v); };
const classicMun = (t) => { const ab = UNITS[t].ab, c = abCost({ mode: { kind: 'classic' } }, ab); if (abCost({ mode: { kind: 'world' } }, ab) !== c || abCost({ mode: { kind: 'skirmish' } }, ab) !== 0) fail(`abCost of ${t} is not "Classic and World price, else free"`); return c; };
for (const t of ABS) if (classicMun(t) !== (UNITS[t].ab.mun ?? AB_MUN[UNITS[t].ab.id] ?? 0)) fail(`abCost of ${t} disagrees with ab.mun ?? AB_MUN`);
if (FACTIONS.length !== 4) fail(`FACTIONS has ${FACTIONS.length} entries, the port expects 4`);

const data = `// Generated by unreal/Tools/export-ability-table.mjs from the reference at ${pin}. Do not edit: when the pin moves,
// run the tool again (node unreal/Tools/export-ability-table.mjs reference/ww2-rts). Included by TCAbilities.cpp only.
#pragma once

namespace TCAbilitiesData
{
	inline const TCHAR* const ReferencePin = ${s(pin)};

	// shared/sim.js AUTO_FLAG (an own unit's autocast is on) and RIDING_FLAG; client/availability.js reads bit 1 as
	// retreating (v.flags & 1)
	constexpr int32 AutoFlag = ${int(AUTO_FLAG, 'AUTO_FLAG')}, RidingFlag = ${int(RIDING_FLAG, 'RIDING_FLAG')}, RetreatingFlag = 1;
	// shared/sim.js UNIT_ROW: where a snapshot row keeps its cooldown and flags
	constexpr int32 RowCd = ${int(UNIT_ROW.cd, 'UNIT_ROW.cd')}, RowFlags = ${int(UNIT_ROW.flags, 'UNIT_ROW.flags')};
	// client/keys.js binding 'ability': ${KEY.code}, label ${KEY.label}, no modifiers, context global. The same key is also
	// ${sharing.join('; ')}.
	inline const TCHAR* const KeyLabel = ${s(KEY.label)};
	constexpr TCHAR KeyLetter = TEXT('${KEY.code.slice(3)}');
	// client/autocast.js KEY: where the browser keeps the remembered autocast choices
	inline const TCHAR* const AutocastStorageKey = ${s(STORAGE_KEY)};
	// client/main.js setAim: the suffix every aim's hint gets; throwAt's marker colour
	inline const TCHAR* const AimHintSuffix = ${s(HINT_SUFFIX)};
	constexpr uint32 ThrowMarker = 0x${MARKER};

	// client/main.js PRIORITY: the types with an Orders button (client/hud.js drawOrders) and that F picks from, in order
	inline const TCHAR* const Priority[] = { ${PRIORITY.map(s).join(', ')} };

	// The aimed abilities (client/main.js AIMED, client/hud.js AIMED, shared/sim.js AIMED_AB): used on a clicked spot.
	// Hint: client/main.js setAim's hint for the aim. Radius: client/main.js aimShape's preview ring.
	struct FAimRow { const TCHAR* Id; const TCHAR* Hint; double Radius; };
	inline const FAimRow Aimed[] = {
${aimedIds.map((id) => `\t\t{ ${s(id)}, ${s(HINTS[id])}, ${real(AIM_RADIUS[id], 'aim radius ' + id)} },`).join('\n')}
	};

	// One unit type with an ability (shared/sim.js UNITS[type].ab, id not 'none'), in UNIT_TYPES order: its type, ability
	// id and name, cooldown seconds (ab.cd), the Munitions it costs in Classic and World (abCost: ab.mun ?? AB_MUN[id]
	// ?? 0; free elsewhere), client/hud.js's icon, and client/main.js FACTIONS[0..3].names[type] (empty: none, the
	// HUD then writes UNITS[type].name, given as UnitName).
	struct FAbilityRow { const TCHAR* Type; const TCHAR* UnitName; const TCHAR* Id; const TCHAR* Name; int32 Cd, ClassicMun; const TCHAR* Icon; const TCHAR* FactionNames[4]; };
	inline const FAbilityRow Abilities[] = {
${ABS.map((t) => { const d = UNITS[t], ab = d.ab; return `\t\t{ ${[s(t), s(d.name ?? t), s(ab.id), s(ab.name), int(ab.cd, t + '.ab.cd'), int(classicMun(t), t + ' abCost'), s(iconOf(ab.id)), `{ ${FACTIONS.map((f) => s(f.names?.[t] ?? '')).join(', ')} }`].join(', ')} },`; }).join('\n')}
	};
}
`;

// ---------- the cases: the reference's own code on fixed snapshots ----------
// A row in UNIT_ROW order: id, type, owner, x, z, rot, aim, hp, supp, target, cover, cd, flags, stars, built.
const row = (id, type, owner, x, z, cd = 0, flags = 0) => [id, type, owner, x, z, 0, 0, 100, 0, 0, 0, cd, flags, 0, 1];
const view = (r) => ({ id: r[0], type: r[1], owner: r[2], x: r[3], z: r[4], hp: r[7], cd: r[UNIT_ROW.cd], flags: r[UNIT_ROW.flags] });

// client/main.js and client/hud.js functions, evaluated as they are written at the pin, against one fixture.
const mainSrc = ['const PRIORITY = ', 'function fKeyType(', 'const AIMED = {', 'function useAbility(', 'function throwAt('].map((a) => excerpt('client/main.js', a)).join('\n');
const clickSrc = excerpt('client/main.js', '    if (AIMED[kind]) {').trim();
const hudTypesSrc = excerpt('client/hud.js', '    const types = ctx.PRIORITY.filter(').trim().replace(/^const types = (ctx\.PRIORITY\.filter\(\(t\) => sel\.some\(\(v\) => v\.type === t\)\)), dig = .*$/, 'return $1;');
if (!hudTypesSrc.startsWith('return ctx.PRIORITY.filter(')) fail('client/hud.js drawOrders types: unexpected form');
const hudTipSrc = excerpt('client/hud.js', '        types.map((t) => { const ab = UNITS[t].ab;').trim().replace(/\.join\(''\) \+$/, '');
const hudButtonSrc = excerpt('client/hud.js', '      else if (UNITS[a]) {').trim();
const hudNameSrc = excerpt('client/hud.js', '  const name = (t, slot = ctx.me)').trim();

function sandbox(fx) {
  const sent = [], feedback = [], marks = [], aims = [];
  const s = fx.snap;
  const units = new Map((s?.units ?? []).map((r) => [r[0], view(r)]));
  const selected = new Set(fx.selected);
  const available = (action) => availability(s, CFG, { ...action, slot: fx.me, teams: fx.teams, watching: !!fx.watching, ids: [...selected], naval: false });
  const env = {
    UNITS, units, selected, available, sendCmd: (m) => sent.push(JSON.parse(JSON.stringify(m))), blip: () => {}, marker: (x, z, c) => marks.push([x, z, c]),
    explainUnavailable: (r) => { if (!r.ok) feedback.push(r.reason); return !r.ok; }, setAim: (kind, unit) => aims.push({ kind, unit }), cancelAim: () => aims.push({ cancel: true }),
  };
  const api = new Function('env', `with (env) { ${mainSrc}\n return { fKeyType, useAbility, throwAt, PRIORITY, AIMED }; }`)(env);
  const click = new Function('env', 'api', 'kind', 'aimedUnit', 'e', 'g', `with (env) { const { AIMED, throwAt } = api; ${clickSrc} }`);
  return { env, api, sent, feedback, marks, aims, click: (kind, unit, shift, g) => click(env, api, kind, unit, { shiftKey: shift }, g) };
}

// client/hud.js drawOrders for the ability buttons: types, then each button's tip and icon, its text, F label, auto look
function hudButtons(fx) {
  const sb = sandbox(fx), s = fx.snap;
  const factions = fx.factions ?? [];
  const look = (slot) => FACTIONS[factions[slot] ?? (slot < 0 ? 0 : slot % 3)];
  const ctx = { me: fx.me, look, PRIORITY: sb.api.PRIORITY, fType: () => sb.api.fKeyType(), selected: sb.env.selected };
  const sel = [...sb.env.selected].map((id) => sb.env.units.get(id)).filter(Boolean);
  const name = new Function('ctx', 'UNITS', `${hudNameSrc}; return name;`)(ctx, UNITS);
  const types = new Function('ctx', 'sel', hudTypesSrc)(ctx, sel);
  const btns = new Function('types', 'UNITS', 'AIMED', 'name', 'orderBtn', `return ${hudTipSrc};`)(types, UNITS, new Set(HUD_AIMED), name, (data, ico, key, tip) => ({ ico, tip }));
  const check = (action) => availability(s, CFG, { ...action, slot: fx.me, teams: fx.teams, watching: !!fx.watching, ids: [...sb.env.selected] });
  const fType = ctx.fType();
  const run = new Function('a', 'sel', 's', 'check', 'cooldownSeconds', 'abCost', 'UNITS', 'AUTO_FLAG', 'fType', 'label', 'setText', 'b',
    `let result = { ok: true, reason: '' }, txt = ''; if (false) {} ${hudButtonSrc} return { result, txt };`);
  return types.map((t, i) => {
    let kbd = '', auto = false;
    const b = { querySelector: () => ({ kbd: true }), classList: { toggle: (c, on) => { if (c === 'auto') auto = on; } } };
    const { result, txt } = run(t, sel, s, check, cooldownSeconds, abCost, UNITS, AUTO_FLAG, fType, (id) => (id === 'ability' ? KEY.label : id), (el, v) => { if (el?.kbd) kbd = v; }, b);
    return { type: t, icon: btns[i].ico, tip: btns[i].tip, text: txt, key: kbd, auto, ok: result.ok, reason: result.reason };
  });
}

const cases = [];
const add = (c) => cases.push(c);
const base = (kind, units, extra = {}) => ({ mode: { kind }, mp: 500, units, out: [false, false], ...extra });

// availability({ t: 'ability' })
const avail = (name, fx, unit, queue) => {
  const r = availability(fx.snap, CFG, { t: 'ability', unit, queue, slot: fx.me, teams: fx.teams, watching: !!fx.watching, ids: fx.selected });
  add({ case: 'availability', name, ...fx, unit, queue: queue === true, expect: { ok: r.ok, reason: r.reason } });
};
const fx = (snap, selected, extra = {}) => ({ snap, selected, me: 0, teams: [0, 1], ...extra });
const skirmishRifles = base('skirmish', [row(1, 'rifle', 0, 10, 10), row(2, 'rifle', 0, 20, 10, 12), row(3, 'mg', 0, 30, 10, 5), row(9, 'rifle', 1, 50, 50)]);
avail('no snapshot', fx(null, [1]), 'rifle');
avail('spectating', fx(skirmishRifles, [1], { watching: true }), 'rifle');
avail('seat out', fx({ ...skirmishRifles, out: [true, false] }, [1]), 'rifle');
avail('no squad of the type', fx(skirmishRifles, [3]), 'rifle');
avail('foe of the type is not crew', fx(skirmishRifles, [9]), 'rifle');
avail('one ready', fx(skirmishRifles, [1, 2]), 'rifle');
avail('all on cooldown', fx(skirmishRifles, [2]), 'rifle');
avail('cooldown, queued aimed', fx(skirmishRifles, [2]), 'rifle', true);
avail('cooldown, queue on an instant ability', fx(skirmishRifles, [3]), 'mg', true);
avail('retreating', fx(base('skirmish', [row(1, 'rifle', 0, 10, 10, 0, 1)]), [1]), 'rifle');
avail('retreating, queued', fx(base('skirmish', [row(1, 'rifle', 0, 10, 10, 0, 1)]), [1]), 'rifle', true);
avail('min cooldown of the active ones', fx(base('skirmish', [row(1, 'mg', 0, 1, 1, 30), row(2, 'mg', 0, 2, 2, 7), row(3, 'mg', 0, 3, 3, 2, 1)]), [1, 2, 3]), 'mg');
avail('classic short of munitions', fx(base('classic', [row(1, 'rifle', 0, 10, 10)], { mun: 14 }), [1]), 'rifle');
avail('classic enough munitions', fx(base('classic', [row(1, 'rifle', 0, 10, 10)], { mun: 15 }), [1]), 'rifle');
avail('classic no munitions field', fx(base('classic', [row(1, 'ranger', 0, 10, 10)]), [1]), 'ranger');
avail('world free rocket barrage', fx(base('world', [row(1, 'rocket', 0, 10, 10)], { mun: 0 }), [1]), 'rocket');
avail('world mortar price', fx(base('world', [row(1, 'mortar', 0, 10, 10)], { mun: 3 }), [1]), 'mortar');
avail('skirmish ignores munitions', fx(base('skirmish', [row(1, 'mortar', 0, 10, 10)], { mun: 0 }), [1]), 'mortar');
avail('classic queued aimed skips munitions', fx(base('classic', [row(1, 'rifle', 0, 10, 10)], { mun: 0 }), [1]), 'rifle', true);
avail('negative manpower', fx(base('skirmish', [row(1, 'mg', 0, 10, 10)], { mp: -1 }), [1]), 'mg');

// client/main.js fKeyType, useAbility (F), the aimed click (throwAt)
const mixed = base('classic', [row(1, 'rifle', 0, 10, 10, 9), row(2, 'mg', 0, 12, 10), row(3, 'tank', 0, 14, 10), row(4, 'at', 0, 16, 10, 0, 1), row(5, 'rifle', 0, 40, 40), row(6, 'rifle', 0, 5, 5, 0, 1)], { mun: 100 });
const fkeys = [
  ['first ready in priority', fx(mixed, [3, 2, 1])],
  ['first in priority when none is ready', fx(base('skirmish', [row(1, 'rifle', 0, 1, 1, 4), row(2, 'tank', 0, 2, 2, 9)]), [2, 1])],
  ['retreating squad skipped', fx(mixed, [4, 3])],
  ['nothing with a button', fx(base('skirmish', [row(1, 'sniper', 0, 1, 1), row(2, 'gunboat', 0, 2, 2)]), [1, 2])],
  ['empty selection', fx(mixed, [])],
];
for (const [name, f] of fkeys) add({ case: 'fkey', name, ...f, expect: { type: sandbox(f).api.fKeyType() ?? '' } });

const uses = [
  ['instant: ready ones only', fx(base('skirmish', [row(1, 'mg', 0, 1, 1), row(2, 'mg', 0, 2, 2, 3), row(3, 'mg', 0, 3, 3, 0, 1), row(4, 'mg', 0, 4, 4)]), [1, 2, 3, 4]), 'mg'],
  ['instant: refused on cooldown', fx(base('skirmish', [row(1, 'tank', 0, 1, 1, 8)]), [1]), 'tank'],
  ['instant: classic short of munitions', fx(base('classic', [row(1, 'conscript', 0, 1, 1)], { mun: 9 }), [1]), 'conscript'],
  ['aimed: arms the aim on cooldown', fx(base('skirmish', [row(1, 'rifle', 0, 1, 1, 20)]), [1]), 'rifle'],
  ['aimed: arms the aim', fx(base('classic', [row(1, 'mortar', 0, 1, 1)], { mun: 0 }), [1]), 'mortar'],
  ['aimed: all retreating', fx(base('skirmish', [row(1, 'ranger', 0, 1, 1, 0, 1)]), [1]), 'ranger'],
  ['spectator', fx(base('skirmish', [row(1, 'mg', 0, 1, 1)]), [1], { watching: true }), 'mg'],
  ['F with a mixed selection', fx(mixed, [3, 2, 1]), null],
  ['F with nothing to use', fx(mixed, []), null],
];
for (const [name, f, type] of uses) {
  const sb = sandbox(f), t = type ?? sb.api.fKeyType();
  sb.api.useAbility(t);
  add({ case: 'use', name, ...f, unit: type ?? '', expect: { type: t ?? '', sent: sb.sent, feedback: sb.feedback, aims: sb.aims } });
}

const throws = [
  ['nearest ready thrower', fx(base('skirmish', [row(1, 'rifle', 0, 0, 0), row(2, 'rifle', 0, 30, 30), row(3, 'rifle', 0, 28, 28, 5), row(4, 'rifle', 0, 29, 29, 0, 1)]), [1, 2, 3, 4]), 'grenade', 'rifle', false, { x: 31.5, z: 30.25 }],
  ['queued: cooldown ones count', fx(base('skirmish', [row(1, 'rifle', 0, 0, 0), row(3, 'rifle', 0, 28, 28, 5)]), [1, 3]), 'grenade', 'rifle', true, { x: 31.5, z: 30.25 }],
  ['tie keeps selection order', fx(base('skirmish', [row(7, 'ranger', 0, 10, 0), row(5, 'ranger', 0, -10, 0)]), [7, 5]), 'satchel', 'ranger', false, { x: 0, z: 0 }],
  ['refused: cooldown keeps the aim', fx(base('skirmish', [row(1, 'rifle', 0, 0, 0, 5)]), [1]), 'grenade', 'rifle', false, { x: 3, z: 4 }],
  ['refused: classic munitions', fx(base('classic', [row(1, 'howitzer', 0, 0, 0)], { mun: 24 }), [1]), 'barrage', 'howitzer', false, { x: 60, z: 4 }],
  ['classic queued skips munitions', fx(base('classic', [row(1, 'howitzer', 0, 0, 0)], { mun: 0 }), [1]), 'barrage', 'howitzer', true, { x: 60, z: 4 }],
  ['the selection lost the type', fx(base('skirmish', [row(1, 'mg', 0, 0, 0)]), [1]), 'grenade', 'rifle', false, { x: 1, z: 1 }],
  ['medium tank HE shell aims as grenade', fx(base('skirmish', [row(1, 'medium', 0, 0, 0), row(2, 'rifle', 0, 1, 1)]), [1, 2]), 'grenade', 'medium', false, { x: 20, z: 0 }],
];
for (const [name, f, kind, unit, shift, g] of throws) {
  const sb = sandbox(f);
  sb.click(kind, unit, shift, g);
  add({ case: 'throw', name, ...f, kind, unit, shift, ground: g, expect: { sent: sb.sent, feedback: sb.feedback, aims: sb.aims, marks: sb.marks } });
}

// client/hud.js ability buttons
const buttons = [
  ['mixed classic', fx(mixed, [1, 2, 3, 4, 5, 6], { factions: [1, 0] })],
  ['autocast on for every one', fx(base('skirmish', [row(1, 'mg', 0, 1, 1, 0, AUTO_FLAG), row(2, 'mg', 0, 2, 2, 0, AUTO_FLAG | 1)]), [1, 2], { factions: [2, 0] })],
  ['autocast on for some', fx(base('skirmish', [row(1, 'mg', 0, 1, 1, 0, AUTO_FLAG), row(2, 'mg', 0, 2, 2, 6)]), [1, 2])],
  ['all retreating shows their cooldown', fx(base('skirmish', [row(1, 'tank', 0, 1, 1, 7, 1), row(2, 'tank', 0, 2, 2, 3, 1)]), [1, 2], { factions: [3, 0] })],
  ['free world barrage', fx(base('world', [row(1, 'rocket', 0, 1, 1), row(2, 'churchill', 0, 2, 2)], { mun: 0 }), [1, 2], { factions: [3] })],
  ['default faction is the seat', fx(base('skirmish', [row(1, 'rifle', 4, 1, 1), row(2, 'rifle', 4, 2, 2, 11)]), [1, 2], { me: 4, teams: [0, 1, 2, 3, 4] })],
];
for (const [name, f] of buttons) add({ case: 'buttons', name, ...f, expect: hudButtons(f) });

// client/autocast.js: toggle, then adopt over the held rows (a fresh client remembers nothing)
{
  const storage = new Map(), out = [];
  const store = { getItem: (k) => storage.get(k) ?? null, setItem: (k, v) => storage.set(k, v) };
  const ac = createAutocast({ storage: store, send: (m) => out.push(JSON.parse(JSON.stringify(m))) });
  const steps = [];
  const units1 = [row(1, 'mg', 0, 1, 1), row(2, 'mg', 0, 2, 2, 0, AUTO_FLAG), row(3, 'rifle', 0, 3, 3), row(8, 'mg', 1, 4, 4)];
  const step = (op, argsIn, run) => { const before = out.length; const r = run(); steps.push({ op, ...argsIn, result: r === undefined ? null : r, sent: out.slice(before), storage: storage.get(STORAGE_KEY) ?? null }); };
  // a toggle's list: the selected own units of the type (client/main.js actions.autocast), given as rows
  const toggle = (type, rows, classic) => step('toggle', { type, units: rows, classic }, () => ac.toggle(type, rows.map(view), classic));
  // adopt: the native port names ids in ascending order, so the fixtures hold their rows that way
  const adopt = (rows, classic) => { if (rows.some((r, i) => i && r[0] <= rows[i - 1][0])) fail('adopt fixture rows must be in ascending id order'); step('adopt', { units: rows, me: 0, classic }, () => ac.adopt(rows, 0, classic)); };
  toggle('mg', units1.filter((r) => r[1] === 'mg' && r[2] === 0), false);
  toggle('mg', [], false);
  adopt(units1, false);
  const units2 = [...units1, row(10, 'mg', 0, 5, 5, 0, AUTO_FLAG), row(11, 'mg', 0, 6, 6), row(12, 'rifle', 0, 7, 7, 0, AUTO_FLAG)];
  adopt(units2, false);
  toggle('rifle', [row(3, 'rifle', 0, 3, 3, 0, AUTO_FLAG), row(12, 'rifle', 0, 7, 7, 0, AUTO_FLAG)], true);
  const units3 = [...units2, row(13, 'rifle', 0, 8, 8, 0, AUTO_FLAG), row(14, 'mg', 0, 9, 9, 0, AUTO_FLAG)];
  adopt(units3, true);
  step('reset', {}, () => ac.reset());
  adopt(units3, false);
  add({ case: 'autocast', name: 'toggle, adopt and reset', steps });
}

// MSVC limits one string literal piece to about 16 KB (a wide character takes two bytes) and a concatenated literal to
// 64 KB: each case is written as adjacent raw literals of at most 2,000 characters, all ASCII.
const pieces = (str) => Array.from({ length: Math.ceil(str.length / 2000) }, (_, i) => str.slice(i * 2000, (i + 1) * 2000));
const json = (c) => JSON.stringify(c).replace(/[^\x20-\x7e]/g, (ch) => `\\u${ch.charCodeAt(0).toString(16).padStart(4, '0')}`);
for (const c of cases) {
  if (json(c).includes(')TCJSON"')) fail('a case holds the raw string delimiter');
  if (json(c).length > 30000) fail(`case ${c.name} is too long for one MSVC string literal`);
}
const casesText = `// Generated by unreal/Tools/export-ability-table.mjs from the reference at ${pin}. Do not edit: when the pin moves,
// run the tool again. The expected answers come from running the reference's own code (client/availability.js,
// client/main.js fKeyType, useAbility, the aimed click and throwAt, client/hud.js's ability buttons, client/autocast.js)
// on these fixtures; snapshot rows are in shared/sim.js UNIT_ROW order. Included by TCAbilities.cpp's automation test.
#pragma once

namespace TCAbilitiesCases
{
	inline const TCHAR* const ReferencePin = ${s(pin)};
	inline const TCHAR* const Cases[] = {
${cases.map((c) => `\t\t${pieces(json(c)).map((part) => `TEXT(R"TCJSON(${part})TCJSON")`).join('\n\t\t\t')},`).join('\n')}
	};
}
`;

if (check) {
  const have = (p) => { try { return readFileSync(p, 'utf8'); } catch { return ''; } };
  const bad = [[dataOut, data], [casesOut, casesText]].filter(([p, t]) => have(p) !== t).map(([p]) => p);
  if (bad.length) fail(`not what the reference at ${pin} gives (run the tool without --check): ${bad.join(', ')}`);
  console.log(`export-ability-table: ${dataOut} and ${casesOut} match the reference at ${pin}`);
} else {
  writeFileSync(dataOut, data);
  writeFileSync(casesOut, casesText);
  console.log(`export-ability-table: wrote ${dataOut} (${ABS.length} unit types with an ability, ${PRIORITY.length} with a button) and ${casesOut} (${cases.length} cases) from the reference at ${pin}`);
}
