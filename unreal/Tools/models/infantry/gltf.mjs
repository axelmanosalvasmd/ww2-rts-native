// A minimal deterministic glTF 2.0 binary (.glb) writer: typed arrays in, one buffer, no extensions.
// Same input, same bytes (no timestamps, fixed key order), so the exported assets are reproducible build outputs.
const COMPONENT = { Float32Array: 5126, Uint32Array: 5125, Uint16Array: 5123, Uint8Array: 5121 };
const COUNT = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT4: 16 };

export class Glb {
  constructor(generator) {
    this.json = { asset: { version: '2.0', generator }, scene: 0, scenes: [{ nodes: [] }], nodes: [], meshes: [], materials: [], accessors: [], bufferViews: [], buffers: [] };
    this.chunks = [];
    this.length = 0;
  }
  view(bytes, target) {
    const pad = (4 - (this.length % 4)) % 4;
    if (pad) { this.chunks.push(new Uint8Array(pad)); this.length += pad; }
    const view = { buffer: 0, byteOffset: this.length, byteLength: bytes.byteLength };
    if (target) view.target = target;
    this.chunks.push(new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength));
    this.length += bytes.byteLength;
    this.json.bufferViews.push(view);
    return this.json.bufferViews.length - 1;
  }
  accessor(array, type, { target, minmax = false, normalized = false } = {}) {
    const n = COUNT[type], count = array.length / n;
    if (!Number.isInteger(count)) throw new Error(`accessor length ${array.length} is not a multiple of ${n}`);
    const acc = { bufferView: this.view(array, target), componentType: COMPONENT[array.constructor.name], count, type };
    if (normalized) acc.normalized = true;
    if (minmax) {
      const min = Array(n).fill(Infinity), max = Array(n).fill(-Infinity);
      for (let i = 0; i < array.length; i++) { const k = i % n; min[k] = Math.min(min[k], array[i]); max[k] = Math.max(max[k], array[i]); }
      acc.min = min.map(Math.fround); acc.max = max.map(Math.fround);
    }
    this.json.accessors.push(acc);
    return this.json.accessors.length - 1;
  }
  image(bytes, mimeType, name) {
    this.json.images ??= []; this.json.textures ??= []; this.json.samplers ??= [{ magFilter: 9729, minFilter: 9987, wrapS: 10497, wrapT: 10497 }];
    this.json.images.push({ bufferView: this.view(bytes), mimeType, name });
    this.json.textures.push({ sampler: 0, source: this.json.images.length - 1, name });
    return this.json.textures.length - 1;
  }
  bytes() {
    this.json.buffers = [{ byteLength: this.length }];
    const text = new TextEncoder().encode(JSON.stringify(this.json));
    const jsonLength = Math.ceil(text.length / 4) * 4, binLength = Math.ceil(this.length / 4) * 4;
    const out = new Uint8Array(12 + 8 + jsonLength + 8 + binLength), view = new DataView(out.buffer);
    view.setUint32(0, 0x46546c67, true); view.setUint32(4, 2, true); view.setUint32(8, out.length, true);
    view.setUint32(12, jsonLength, true); view.setUint32(16, 0x4e4f534a, true);
    out.fill(0x20, 20, 20 + jsonLength); out.set(text, 20);
    let at = 20 + jsonLength;
    view.setUint32(at, binLength, true); view.setUint32(at + 4, 0x004e4942, true); at += 8;
    for (const chunk of this.chunks) { out.set(chunk, at); at += chunk.byteLength; }
    return out;
  }
}

// rotation quaternion [x, y, z, w] of the upper 3x3 of a column-major matrix (assumed orthonormal)
export function quaternionOf(e) {
  const m11 = e[0], m12 = e[4], m13 = e[8], m21 = e[1], m22 = e[5], m23 = e[9], m31 = e[2], m32 = e[6], m33 = e[10];
  const trace = m11 + m22 + m33;
  let x, y, z, w;
  if (trace > 0) { const s = 0.5 / Math.sqrt(trace + 1); w = 0.25 / s; x = (m32 - m23) * s; y = (m13 - m31) * s; z = (m21 - m12) * s; }
  else if (m11 > m22 && m11 > m33) { const s = 2 * Math.sqrt(1 + m11 - m22 - m33); w = (m32 - m23) / s; x = 0.25 * s; y = (m12 + m21) / s; z = (m13 + m31) / s; }
  else if (m22 > m33) { const s = 2 * Math.sqrt(1 + m22 - m11 - m33); w = (m13 - m31) / s; x = (m12 + m21) / s; y = 0.25 * s; z = (m23 + m32) / s; }
  else { const s = 2 * Math.sqrt(1 + m33 - m11 - m22); w = (m21 - m12) / s; x = (m13 + m31) / s; y = (m23 + m32) / s; z = 0.25 * s; }
  const n = Math.hypot(x, y, z, w);
  return [x / n, y / n, z / n, w / n];
}
