// A game state as JSON and back, for the states harness/sim-state.mjs writes. Maps, Sets and typed arrays keep
// their kind and their order; numbers JSON cannot hold (-0, Infinity, NaN) keep their value, and a field set to
// undefined stays present, because sim.js declares every unit field up front so V8 keeps units in one hidden
// class (a decoded unit missing them would make step() slower than in the live game). Object identity is not
// kept: two fields that shared one array come back as two equal arrays.
const TYPED = { Float32Array, Float64Array, Int8Array, Uint8Array, Int16Array, Uint16Array, Int32Array, Uint32Array };
export function encode(v) {
  if (typeof v === 'number') return Number.isFinite(v) && !Object.is(v, -0) ? v : { $num: Object.is(v, -0) ? '-0' : String(v) };
  if (v === null || typeof v !== 'object') return v;
  if (v instanceof Map) return { $map: [...v].map(([k, x]) => [encode(k), encode(x)]) };
  if (v instanceof Set) return { $set: [...v].map(encode) };
  if (ArrayBuffer.isView(v)) return { ['$' + v.constructor.name]: Array.from(v, encode) };
  if (Array.isArray(v)) return v.map(encode);
  const out = {};
  for (const [k, x] of Object.entries(v)) if (typeof x !== 'function') out[k] = x === undefined ? { $undefined: true } : encode(x);
  return out;
}
export function decode(v) {
  if (v === null || typeof v !== 'object') return v;
  if (Array.isArray(v)) return v.map(decode);
  if ('$num' in v) return Number(v.$num);
  if ('$undefined' in v) return undefined;
  if ('$map' in v) return new Map(v.$map.map(([k, x]) => [decode(k), decode(x)]));
  if ('$set' in v) return new Set(v.$set.map(decode));
  const typed = Object.keys(v).find(k => k.startsWith('$') && TYPED[k.slice(1)]);
  if (typed) return TYPED[typed.slice(1)].from(v[typed].map(decode));
  return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, decode(x)]));
}
