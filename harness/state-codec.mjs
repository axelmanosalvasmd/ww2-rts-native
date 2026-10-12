// A game state as JSON and back, for the states harness/sim-state.mjs writes. Maps, Sets and typed arrays keep
// their kind and their order; numbers JSON cannot hold (-0, Infinity, NaN) keep their value, and a field set to
// undefined stays present, because sim.js declares every unit field up front so V8 keeps units in one hidden
// class (a decoded unit missing them would make step() slower than in the live game). Object identity is not
// kept: two fields that shared one array come back as two equal arrays.
// With { runs: true }, typed arrays are written as runs, { runs: [value, count, value, count, ...] }, which keeps
// a map's per-cell arrays small in committed fixtures. decode reads both forms.
const TYPED = { Float32Array, Float64Array, Int8Array, Uint8Array, Int16Array, Uint16Array, Int32Array, Uint32Array };
export function encode(v, opts = {}) {
  if (typeof v === 'number') return Number.isFinite(v) && !Object.is(v, -0) ? v : { $num: Object.is(v, -0) ? '-0' : String(v) };
  if (v === null || typeof v !== 'object') return v;
  if (v instanceof Map) return { $map: [...v].map(([k, x]) => [encode(k, opts), encode(x, opts)]) };
  if (v instanceof Set) return { $set: [...v].map(x => encode(x, opts)) };
  if (ArrayBuffer.isView(v)) return { ['$' + v.constructor.name]: opts.runs ? { runs: runsOf(v) } : Array.from(v, x => encode(x)) };
  if (Array.isArray(v)) return v.map(x => encode(x, opts));
  const out = {};
  for (const [k, x] of Object.entries(v)) if (typeof x !== 'function') out[k] = x === undefined ? { $undefined: true } : encode(x, opts);
  return out;
}
function runsOf(a) {
  const runs = [];
  for (let i = 0; i < a.length;) {
    let j = i + 1;
    while (j < a.length && Object.is(a[j], a[i])) j++;
    runs.push(encode(a[i]), j - i);
    i = j;
  }
  return runs;
}
export function decode(v) {
  if (v === null || typeof v !== 'object') return v;
  if (Array.isArray(v)) return v.map(decode);
  if ('$num' in v) return Number(v.$num);
  if ('$undefined' in v) return undefined;
  if ('$map' in v) return new Map(v.$map.map(([k, x]) => [decode(k), decode(x)]));
  if ('$set' in v) return new Set(v.$set.map(decode));
  const typed = Object.keys(v).find(k => k.startsWith('$') && TYPED[k.slice(1)]);
  if (typed) {
    const raw = v[typed], items = Array.isArray(raw) ? raw : raw.runs.flatMap((x, i) => (i % 2 ? [] : Array(raw.runs[i + 1]).fill(x)));
    return TYPED[typed.slice(1)].from(items.map(decode));
  }
  return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, decode(x)]));
}
