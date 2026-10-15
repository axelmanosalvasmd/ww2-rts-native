// Test-only, read-only observation of the actual createGame return value.
// No engine formulas, defaults, Source-defined getters or expected-state imports.
// Typed-array length/buffer/byteOffset use read-only intrinsic accessors.
export function observeStartup(game) {
  const active = new Set(), first = new Map(), aliases = [];
  const number = value => {
    const bytes = new DataView(new ArrayBuffer(8));
    bytes.setFloat64(0, value);
    return {bits: bytes.getBigUint64(0).toString(16).padStart(16, '0'),
      value: Number.isFinite(value) ? value : String(value)};
  };
  const encode = (value, path) => {
    if (value === undefined) return {$undefined: true};
    if (typeof value === 'number') return number(value);
    if (value === null || typeof value === 'boolean' || typeof value === 'string') return value;
    if (typeof value !== 'object') throw Error(`Unsupported startup value at ${path}: ${typeof value}`);
    if (active.has(value)) throw Error(`Startup cycle at ${path}`);
    if (first.has(value)) aliases.push({path, first: first.get(value)});
    else first.set(value, path);
    active.add(value);
    let result;
    if (value instanceof Map) {
      if (Reflect.ownKeys(value).length) throw Error(`Map own properties at ${path}`);
      result = {$map: [...value].map(([k, v], i) => [encode(k, `${path}.key[${i}]`), encode(v, `${path}.value[${i}]`)])};
    } else if (value instanceof Set) {
      if (Reflect.ownKeys(value).length) throw Error(`Set own properties at ${path}`);
      result = {$set: [...value].map((v, i) => encode(v, `${path}[${i}]`))};
    } else if (ArrayBuffer.isView(value)) {
      const kind = Object.getPrototypeOf(value).constructor.name;
      if (!['Uint8Array', 'Uint16Array', 'Int8Array', 'Float32Array'].includes(kind)) throw Error(`Unsupported typed array ${path}`);
      if (Reflect.ownKeys(value).length !== value.length) throw Error(`Typed-array own properties at ${path}`);
      result = {$typed: kind, values: Array.from(value, (v, i) => encode(v, `${path}[${i}]`))};
      if (kind === 'Float32Array') {
        const words = new Uint32Array(value.buffer, value.byteOffset, value.length);
        result.storageBits = Array.from(words, word => word.toString(16).padStart(8, '0'));
      }
    } else if (Array.isArray(value)) {
      if (Reflect.ownKeys(value).length !== value.length + 1) throw Error(`Sparse or extended array at ${path}`);
      result = Array.from({length: value.length}, (_, i) => {
        const descriptor = Object.getOwnPropertyDescriptor(value, String(i));
        if (!descriptor || !('value' in descriptor)) throw Error(`Array accessor or hole at ${path}[${i}]`);
        return encode(descriptor.value, `${path}[${i}]`);
      });
    } else {
      const proto = Object.getPrototypeOf(value);
      if (proto !== Object.prototype && proto !== null) throw Error(`Unsupported object prototype at ${path}`);
      result = {};
      for (const key of Reflect.ownKeys(value)) {
        if (typeof key !== 'string') throw Error(`Symbol startup key at ${path}`);
        const descriptor = Object.getOwnPropertyDescriptor(value, key);
        if (!('value' in descriptor)) throw Error(`Startup accessor at ${path}.${key}`);
        Object.defineProperty(result, key, {value: encode(descriptor.value, `${path}.${key}`), enumerable: true});
      }
    }
    active.delete(value);
    return result;
  };
  const state = encode(game, '$');
  return {format: 'r1-complete-startup-observation-v1', state, aliases,
    inventory: {game: Object.keys(state), players: state.players.map(Object.keys),
      units: state.units.$map.map(([id, unit]) => ({id, fields: Object.keys(unit)})),
      points: state.points.map(Object.keys),
      structures: state.structures.$map.map(([id, structure]) => ({id, fields: Object.keys(structure), sections: structure.sections.map(Object.keys)}))}};
}
