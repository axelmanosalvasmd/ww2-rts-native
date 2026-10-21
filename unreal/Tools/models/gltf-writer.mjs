// Minimal deterministic glTF 2.0 binary (.glb) writer. No dependencies; the same input gives the same bytes: JSON keys
// are emitted in insertion order, every number comes from the input arrays, chunks are padded as the spec requires
// (JSON with spaces, BIN with zeros) and buffer views are packed in the order accessors are added.
const FLOAT = 5126, UNSIGNED_SHORT = 5123, UNSIGNED_INT = 5125;
const ARRAY_BUFFER = 34962, ELEMENT_ARRAY_BUFFER = 34963;
const SIZE = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT4: 16 };

export class Gltf {
  constructor(asset) {
    this.json = { asset: { version: '2.0', ...asset }, scene: 0, scenes: [{ nodes: [] }], nodes: [], meshes: [], materials: [], accessors: [], bufferViews: [], buffers: [] };
    this.chunks = []; this.byteLength = 0; this.extensionsUsed = new Set();
  }
  // a typed array as one accessor in its own buffer view; minmax: write min and max (required for POSITION)
  accessor(array, type, { minmax = false, target = ARRAY_BUFFER, normalized = false } = {}) {
    const componentType = array instanceof Float32Array ? FLOAT : array instanceof Uint16Array ? UNSIGNED_SHORT : array instanceof Uint32Array ? UNSIGNED_INT : null;
    if (componentType === null) throw new Error(`unsupported accessor array ${array?.constructor?.name}`);
    const n = SIZE[type], count = array.length / n;
    if (!Number.isInteger(count)) throw new Error(`accessor length ${array.length} is not a multiple of ${n}`);
    const bytes = new Uint8Array(array.buffer, array.byteOffset, array.byteLength).slice();
    const view = { buffer: 0, byteOffset: this.byteLength, byteLength: bytes.byteLength };
    if (target) view.target = target;
    this.chunks.push(bytes); this.byteLength += bytes.byteLength;
    const pad = (4 - (this.byteLength % 4)) % 4;
    if (pad) { this.chunks.push(new Uint8Array(pad)); this.byteLength += pad; }
    this.json.bufferViews.push(view);
    const acc = { bufferView: this.json.bufferViews.length - 1, componentType, count, type };
    if (normalized) acc.normalized = true;
    if (minmax) {
      const min = new Array(n).fill(Infinity), max = new Array(n).fill(-Infinity);
      for (let i = 0; i < count; i++) for (let k = 0; k < n; k++) { const v = array[i * n + k]; if (v < min[k]) min[k] = v; if (v > max[k]) max[k] = v; }
      if (count === 0) { min.fill(0); max.fill(0); }
      acc.min = min; acc.max = max;
    }
    this.json.accessors.push(acc);
    return this.json.accessors.length - 1;
  }
  indices(array) { return this.accessor(array, 'SCALAR', { target: ELEMENT_ARRAY_BUFFER }); }
  material(m) { this.json.materials.push(m); return this.json.materials.length - 1; }
  mesh(m) { this.json.meshes.push(m); return this.json.meshes.length - 1; }
  node(n) { this.json.nodes.push(n); return this.json.nodes.length - 1; }
  root(i) { this.json.scenes[0].nodes.push(i); }
  use(ext) { this.extensionsUsed.add(ext); }
  toGlb() {
    const json = this.json;
    json.buffers = this.byteLength ? [{ byteLength: this.byteLength }] : [];
    if (!this.byteLength) { delete json.buffers; delete json.bufferViews; delete json.accessors; }
    for (const key of ['meshes', 'materials', 'nodes']) if (json[key] && !json[key].length) delete json[key];
    // fixed top-level key order, whatever was added when
    const order = ['asset', 'extensionsUsed', 'scene', 'scenes', 'nodes', 'meshes', 'skins', 'materials', 'textures', 'images', 'samplers', 'accessors', 'bufferViews', 'buffers'];
    if (this.extensionsUsed.size) json.extensionsUsed = [...this.extensionsUsed].sort();
    const sorted = {};
    for (const k of order) if (json[k] !== undefined) sorted[k] = json[k];
    for (const k of Object.keys(json)) if (!(k in sorted)) sorted[k] = json[k];
    this.json = sorted;
    let text = Buffer.from(JSON.stringify(this.json), 'utf8');
    const jsonPad = (4 - (text.length % 4)) % 4;
    if (jsonPad) text = Buffer.concat([text, Buffer.alloc(jsonPad, 0x20)]);
    const bin = Buffer.concat(this.chunks.map((c) => Buffer.from(c.buffer, c.byteOffset, c.byteLength)));
    const total = 12 + 8 + text.length + (bin.length ? 8 + bin.length : 0);
    const header = Buffer.alloc(12);
    header.writeUInt32LE(0x46546c67, 0); header.writeUInt32LE(2, 4); header.writeUInt32LE(total, 8);
    const jh = Buffer.alloc(8); jh.writeUInt32LE(text.length, 0); jh.writeUInt32LE(0x4e4f534a, 4);
    const parts = [header, jh, text];
    if (bin.length) { const bh = Buffer.alloc(8); bh.writeUInt32LE(bin.length, 0); bh.writeUInt32LE(0x004e4942, 4); parts.push(bh, bin); }
    return Buffer.concat(parts);
  }
}

// Read a .glb back into { json, bin } (for the structural checks)
export function readGlb(buf) {
  if (buf.readUInt32LE(0) !== 0x46546c67) throw new Error('not a glb (magic)');
  if (buf.readUInt32LE(4) !== 2) throw new Error('glb version is not 2');
  if (buf.readUInt32LE(8) !== buf.length) throw new Error(`glb length ${buf.readUInt32LE(8)} != file ${buf.length}`);
  let at = 12, json = null, bin = null;
  while (at < buf.length) {
    const len = buf.readUInt32LE(at), type = buf.readUInt32LE(at + 4), body = buf.subarray(at + 8, at + 8 + len);
    if (len % 4) throw new Error('chunk length not 4-byte aligned');
    if (type === 0x4e4f534a) json = JSON.parse(body.toString('utf8'));
    else if (type === 0x004e4942) bin = body;
    at += 8 + len;
  }
  return { json, bin };
}
