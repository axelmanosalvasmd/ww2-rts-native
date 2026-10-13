// Original functions execute unchanged. The loader only exports private helpers.
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { setFlagsFromString } from 'node:v8';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
if (process.version !== 'v24.21.0' || process.arch !== 'x64') throw new Error('Use x64 Node 24.21.0');
setFlagsFromString('--no-use-std-math-pow');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.argv.find(x => x.startsWith('--reference='))?.slice(12) ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
if (execFileSync('git', ['rev-parse', 'HEAD'], { cwd: reference, encoding: 'utf8' }).trim() !== pin) throw new Error('Wrong reference pin');
execFileSync('git', ['diff', '--exit-code', pin, '--'], { cwd: reference, stdio: 'pipe' });
const simUrl = pathToFileURL(`${reference}/shared/sim.js`).href;
registerHooks({ load(url, context, next) { const result = next(url, context); if (url !== simUrl) return result; return { ...result, source: `${result.source}\nexport { roleMul, armorMul, firedOnBy, retreatProtected, bunkerMul, flightDamage, flightSuppress };\n` }; } });
const sim = await import(simUrl);
const bits = n => { const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex'); };
const encode = v => typeof v === 'number' ? bits(v) : Array.isArray(v) ? v.map(encode) : v && typeof v === 'object' ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, encode(x)])) : v;
const helper = [];
const add = (op, args, result) => helper.push({ op, args, result: encode(result) });
for (const kind of sim.UNIT_TYPES) {
  for (const target of sim.UNIT_TYPES) add('role', [kind, target], sim.roleMul({ type: kind }, { type: target }));
  for (const multiplier of [0, 0.999999999, 1, 2.5, 5, 6]) add('vet', [kind, sim.UNITS[kind].cost * multiplier], sim.vet({ type: kind, xp: sim.UNITS[kind].cost * multiplier }));
  for (const mode of ['conquest', 'horde']) for (const defenders of [1, 2, 3]) add('bunker', [kind, mode, defenders], sim.bunkerMul({ mode: { kind: mode, defenders } }, { type: kind }));
}
for (const kind of ['tank', 'tiger', 'churchill', 'kaiju']) for (const angle of [0, Math.PI, Math.PI / 3, -Math.PI / 3, 2 * Math.PI / 3, 0.3, -1.2]) add('armor', [kind, angle], sim.armorMul({ type: kind, x: 3, z: 4, rot: 0.7 }, { x: 3 + Math.cos(angle), z: 4 + Math.sin(angle) }));
for (const retreating of [false, true]) for (const forced of [false, true]) for (const stranded of [false, true]) for (const speed of [0, 0.05, 0.050000000001, -0.1]) {
  const u = { retreating, logistics: { forced, stranded }, moveSpeed: speed, vx: 0, vz: 0 };
  add('retreat', [u], sim.retreatProtected(u));
}
for (const moveSpeed of [0, 0.01]) for (const vx of [0.05, 0.050000000001]) { const u = { retreating: true, logistics: { forced: true, stranded: false }, moveSpeed, vx, vz: 0 }; add('retreat', [u], sim.retreatProtected(u)); }
for (const delta of [99, 100, 101]) for (const last of [true, false]) for (const aims of [true, false]) for (const missing of [true, false]) {
  const u = { id: 2, hitBy: last ? 1 : 3, hitAt: 200 - delta };
  const t = { id: 1, targetId: aims ? 2 : 3, ...(missing ? {} : { shotAt: 200 - delta }) };
  add('threat', [200, u, t], sim.firedOnBy({ tick: 200 }, u, t));
}
function state(type, id, owner, options = {}) { return { id, owner, type, hp: 100, xp: 0, x: 13, z: 13, rot: 0, garrison: -1, sprint: 0, supp: 10, hitBy: 9, hitAt: -100, hitFrom: { x: -1, z: -2 }, retarget: 7, lastHit: 8, atHit: -100, deathImpulse: { dir: 0.2, impulse: 0.8 }, cargo: [], retreating: false, moveSpeed: 0, vx: 0, vz: 0, targetId: 5, riding: 0, orders: [{ t: 'move', x: 30, z: 30 }], path: [{ x: 31, z: 31 }], ...options }; }
function world(target, source, input) {
  const n = 16 * 16;
  const g = { w: 16, h: 16, chars: Array(n).fill('.'), flags: new Uint16Array(n), height: new Uint8Array(n), cellHp: new Float32Array(n), house: new Uint8Array(n), players: [{ team: 0 }, { team: input.allied ? 0 : 1 }], units: new Map([[source.id, source], [target.id, target]]), wrecks: [], tick: input.tick ?? 200, tech: !!input.tech, mode: { kind: input.mode ?? 'conquest', defenders: input.defenders ?? 1 } };
  if (input.tech) { g.players[0].armory = { inf: input.tech[0][0], guns: input.tech[0][1], armor: input.tech[0][2] }; g.players[1].armory = { inf: input.tech[1][0], guns: input.tech[1][1], armor: input.tech[1][2] }; }
  if (input.trench) { g.flags[6 * 16 + 6] = sim.TRENCH; target.still = input.still ?? 5; }
  if (input.cover) { g.chars[6 * 16 + 6] = 'H'; g.flags[6 * 16 + 6] = sim.COVER; g.cellHp[6 * 16 + 6] = input.coverHp ?? 100; }
  if (input.garrison) { target.garrison = 6 * 16 + 6; g.chars[target.garrison] = 'B'; g.cellHp[target.garrison] = 300; }
  if (input.behind) { g.chars[6 * 16 + 7] = '#'; g.flags[6 * 16 + 7] = sim.COVER; g.cellHp[6 * 16 + 7] = 80; }
  return g;
}
const damage = [], suppression = [];
function capture(input, suppress = false) {
  const events = []; let cargoIndex = 0; let active = false;
  const base = state(input.target ?? 'rifle', 2, 1, input.unit);
  const rawSource = state(input.source ?? 'tank', 1, 0, { xp: input.sourceXp ?? 17.3, air: input.airborne ? { state: 'station' } : null });
  const record = (target, key, value) => {
    if (!active) return;
    if (target === 'source' && key === 'xp') events.push({ kind: 'xp', value });
    if (target !== 'target') return;
    if (key === 'hp') events.push({ kind: 'hp', value });
    else if (key === 'cargo') events.push({ kind: 'rider', rider: base.cargo[cargoIndex++] });
    else if (key === 'lastHit') events.push({ kind: 'lastHit', value });
    else if (key === 'deathImpulse') events.push({ kind: 'impulse', value });
    else if (key === 'retarget') events.push({ kind: 'retarget', value });
    else if (key === 'hitBy') events.push({ kind: 'hitBy', value });
    else if (key === 'hitAt') events.push({ kind: 'hitAt', value });
    else if (key === 'hitFrom') events.push({ kind: 'hitFrom', value });
    else if (key === 'atHit') events.push({ kind: 'atHit', value });
    else if (key === 'supp') events.push({ kind: 'supp', value });
  };
  const proxy = (obj, target) => new Proxy(obj, { set(o, k, v) { record(target, k, v); o[k] = v; return true; } });
  const target = proxy(structuredClone(base), 'target'), source = proxy(rawSource, 'source');
  const g = world(target, source, input);
  const riders = [];
  for (const id of base.cargo) if (id !== 404 && !g.units.has(id)) { const u = state('rifle', id, 1, { riding: target.id, supp: id === 11 ? 80 : 5 }); riders.push(u); g.units.set(id, u); }
  const weapon = input.weapon ?? sim.UNITS[source.type].w;
  const p = { owner: input.owner ?? 0, shooter: 1, source, weapon, damageScale: input.scale, energy: input.energy ?? 1, ap: !!input.ap, apNaval: !!input.apNaval, apMultiplier: input.apMultiplier ?? 1.5, launchOrigin: { x: 20, z: 13 }, suppression: input.amount ?? 20, suppressionScale: input.suppScale, suppressed: new Set(input.duplicate ? [2] : []) };
  const at = { x: 13, z: 13 }, direction = input.direction ?? { x: 1, z: 0 };
  const cover = sim.coverMul(g, target, p.launchOrigin), behind = sim.coverBehind(g, target, p.launchOrigin);
  const unitInput = structuredClone({ ...target }), sourceInput = structuredClone(rawSource);
  // Record Set insertion before the original method mutates it.
  const oldAdd = p.suppressed.add.bind(p.suppressed);
  p.suppressed.add = value => { events.push({ kind: 'record', value }); return oldAdd(value); };
  active = true;
  const result = suppress ? sim.flightSuppress(g, p, target) : sim.flightDamage(g, p, target, at, direction);
  active = false;
  const keys = ['id', 'owner', 'type', 'hp', 'xp', 'x', 'z', 'rot', 'garrison', 'sprint', 'supp', 'hitBy', 'cargo', 'retreating', 'moveSpeed', 'vx', 'vz', 'logistics'];
  const row = { input, unit: Object.fromEntries(keys.filter(k => k in unitInput).map(k => [k, unitInput[k]])), source: { type: sourceInput.type, xp: sourceInput.xp, air: sourceInput.air }, weapon, cover, behind, result: result ?? null, events, ridersAfter: riders.map(u => ({ id: u.id, riding: u.riding, supp: u.supp, x: u.x, z: u.z, orders: u.orders, path: u.path })), targetAfter: { cargo: target.cargo } };
  (suppress ? suppression : damage).push(encodeOutputs(row));
}
function encodeOutputs(row) { return { ...row, result: encode(row.result), events: encode(row.events), ridersAfter: encode(row.ridersAfter), targetAfter: encode(row.targetAfter) }; }
for (const source of ['rifle', 'at', 'flamer', 'sniper', 'tank', 'gunboat']) for (const target of sim.UNIT_TYPES) for (const stars of [0, 3]) capture({ source, target, unit: { xp: sim.UNITS[target].cost * (stars ? 5 : 0) } });
for (const target of ['rifle', 'tank', 'tiger', 'gunboat', 'bunker', 'kaiju', 'hq', 'pillbox']) for (const hp of [-1, 0, 1, 100]) for (const scale of [0, 0.5, 1]) capture({ target, unit: { hp, cargo: target === 'tank' ? [10, 11, 404, 10] : [] }, scale, ap: true, apMultiplier: 2, tech: [[3, 2, 1], [1, 2, 3]] });
for (const target of ['tank', 'gunboat', 'rifle', 'hq']) for (const apNaval of [false, true]) for (const airborne of [false, true]) for (const direction of [{ x: 1, z: 0 }, { x: -1, z: 0 }, { x: 0, z: 1 }]) capture({ target, ap: true, apNaval, airborne, direction });
for (const target of ['bunker', 'kaiju', 'hq']) for (const defenders of [1, 2, 4]) capture({ target, mode: 'horde', defenders, unit: { xp: 5000, retreating: true } });
for (const forced of [false, true]) for (const stranded of [false, true]) for (const moveSpeed of [0, 1]) capture({ unit: { retreating: true, logistics: { forced, stranded }, moveSpeed } });
for (const target of ['rifle', 'tank']) for (const scale of [0, 1]) for (const allied of [false, true]) capture({ target, allied, scale, unit: { cargo: [10, 11, 404], hitBy: 1 } });
for (const owner of [-1, 0]) for (const energy of [0, 0.5]) capture({ target: 'tank', owner, energy, unit: { cargo: [10, 404], hitBy: 1 } });
for (const garrison of [false, true]) for (const trench of [false, true]) capture({ source: 'flamer', target: 'rifle', garrison, trench });
for (const target of ['rifle', 'mg', 'tank']) for (const hp of [0, 100]) for (const duplicate of [false, true]) for (const allied of [false, true]) for (const retreating of [false, true]) for (const sprint of [0, 1]) capture({ target, duplicate, allied, unit: { hp, retreating, sprint } }, true);
for (const flame of [false, true]) for (const garrison of [false, true]) for (const trench of [false, true]) for (const cover of [false, true]) for (const behind of [false, true]) for (const xp of [0, 100, 250, 500]) capture({ source: flame ? 'flamer' : 'tank', garrison, trench, cover, behind, unit: { xp, supp: xp ? 95 : 0, hitBy: xp ? 1 : 9 }, amount: 33.7, suppScale: 0.75 }, true);
for (const amount of [0, 1000]) capture({ amount }, true);
const data = `${JSON.stringify({ format: 1, reference: pin, node: process.version, arch: process.arch, helper, damage, suppression })}\n`;
const dest = `${root}/rust/crates/core/tests/fixtures/combat.json`;
if (process.argv.includes('--check')) { if (readFileSync(dest, 'utf8') !== data) throw new Error('Combat fixture differs'); } else writeFileSync(dest, data);
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} ${helper.length} helper, ${damage.length} damage, ${suppression.length} suppression cases`);
