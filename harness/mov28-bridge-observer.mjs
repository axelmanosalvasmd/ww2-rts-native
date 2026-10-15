// Append-only observers. They neither write engine state nor call extra providers.
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const data = new DataView(new ArrayBuffer(8));
export const bits = n => { data.setFloat64(0, n); return data.getBigUint64(0).toString(16).padStart(16, '0'); };
export const point = p => [bits(p.x), bits(p.z)];
export const raw = v => v === undefined ? { absent: true }
  : typeof v === 'number' ? { bits: bits(v), value: Number.isFinite(v) ? v : String(v) }
  : v instanceof Map ? { map: [...v].map(([k,value]) => [raw(k),raw(value)]) }
  : v instanceof Set ? { set: [...v].map(raw) }
  : ArrayBuffer.isView(v) ? { typed: v.constructor.name, values: [...v].map(raw) }
  : Array.isArray(v) ? v.map(raw)
  : v && typeof v === 'object' ? Object.fromEntries(Object.entries(v).map(([k,value]) => [k,raw(value)])) : v;
export const observation = { constructors: [], near: null, calls: [], active: false, instrumentation: null };
const identities = new WeakMap();
let nextIdentity = 1;
export const identity = value => {
  if (!value || typeof value !== 'object') return raw(value);
  if (!identities.has(value)) identities.set(value,nextIdentity++);
  return identities.get(value);
};
export function install(reference) {
  const sim = pathToFileURL(resolve(reference, 'shared/sim.js')).href;
  registerHooks({ load(url, context, next) {
    const result = next(url, context);
    if (url !== sim) return result;
    const original = readFileSync(resolve(reference, 'shared/sim.js'), 'utf8');
    const loaded = typeof result.source === 'string' ? result.source : Buffer.from(result.source).toString('utf8');
    assert.equal(loaded, original);
    const suffix = `\nimport {observation as __mov28,raw as __mov28raw,identity as __mov28identity} from ${JSON.stringify(import.meta.url)};\n`
      + `const __mov28create=createGame;createGame=(...args)=>{const g=__mov28create(...args);__mov28.constructors.push(__mov28raw({arguments:args,units:[...g.units.values()],players:g.players,seed:g.seed,nextId:g.nextId,wind:g.wind,army:g.army}));return g;};\n`
      + `const __mov28find=findPath;findPath=(g,from,to)=>{if(!__mov28.active)return __mov28find(g,from,to);const row={flagsIdentity:__mov28identity(g.flags),sourceFlagsIdentity:__mov28identity(g.vehicleSourceFlags),flags:[...g.flags],from:__mov28raw(from),to:__mov28raw(to),before:__mov28raw({stats:g.pathStats,vehicleFootprintKnown:g.vehicleFootprintKnown,navigationObserved:g.navigationObserved,worldKnown:g.worldKnown,infantry:g.infantryRegionVersion,vehicle:g.vehicleRegionVersion,obstruction:g.obstructionVersion,knowledge:g.navigationKnowledgeVersion,changedFrom:g.navigationChangedFromVersion,changedCells:g.navigationChangedCells,changes:g.navigationChanges})};__mov28.calls.push(row);const value=__mov28find(g,from,to);row.result=__mov28raw(value);row.after=__mov28raw({stats:g.pathStats,routeEnd:from.routeEnd});return value;};\n`
      + `const __mov28near=nearWalls;nearWalls=g=>{const value=__mov28near(g);if(__mov28.active)__mov28.near=[...value];return value;};\n`;
    assert.equal((original + suffix).slice(0, -suffix.length), original);
    observation.instrumentation = { appended: suffix, erasesExactly: true };
    return { ...result, source: original + suffix };
  } });
}
