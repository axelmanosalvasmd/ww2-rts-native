// The only browser-only APIs the reference model builders touch under Node, shimmed inertly. Each shim is listed in
// the manifest (shims[]) and in README.md. Nothing here decodes or draws pixels. atob, Blob, Response and
// DecompressionStream (the Blender payload decoders in client/models/blender-baked.js) are native in Node 18+ and are
// not shimmed.
//
// Images report a successful load on the next tick without decoding anything. That runs the reference's own texture
// callbacks (client/surfaces.js loadTexture ready(), foliage.js, structures.js), so each material ends in the state it
// reaches in a browser once textures arrive: the texture bound to material.map (whose image.src is the reference file
// path the exporter records) and the textured color multipliers the callbacks set. Canvas-drawn textures (surfaces.js
// brickCanvas) get an inert canvas, so they are recorded as procedural textures, not baked.
export const SHIMS = [
  { api: 'document.createElement(tag)', why: 'surfaces.js brickCanvas, aircraft.js shadowMat and three.js create canvases; returns an inert canvas whose 2D context draws nothing and whose getImageData returns zeroed pixels (the same stub as the reference test.js "Structure pieces rebuilt ..." check)' },
  { api: 'document.createElementNS(ns, "img")', why: 'three.js ImageLoader (TextureLoader) creates an <img>; returns an inert element that keeps its src and fires "load" on the next tick without decoding, so the reference texture callbacks bind material.map and the exporter can read which file each material uses' },
  { api: 'globalThis.Image', why: 'client/ground.js (imported by client/props.js) constructs Image objects at module load; same inert image as above' },
];

const pixels = (_x, _y, w = 1, h = 1) => ({ width: w, height: h, data: new Uint8ClampedArray(Math.max(1, w * h) * 4) });
const pen = new Proxy({}, { get: (_, k) => (k === 'getImageData' || k === 'createImageData' ? pixels : k === 'measureText' ? () => ({ width: 1 }) : () => pen), set: () => true });

class InertImage {
  constructor() { this.listeners = new Map(); this.width = 1; this.height = 1; this.naturalWidth = 1; this.naturalHeight = 1; this.complete = false; this.style = {}; this._src = ''; this.onload = null; this.onerror = null; }
  addEventListener(type, fn) { if (!this.listeners.has(type)) this.listeners.set(type, []); this.listeners.get(type).push(fn); }
  removeEventListener(type, fn) { const l = this.listeners.get(type); if (l) { const i = l.indexOf(fn); if (i >= 0) l.splice(i, 1); } }
  setAttribute() {}
  decode() { return Promise.resolve(); }
  get src() { return this._src; }
  set src(url) {
    this._src = String(url);
    setImmediate(() => {
      this.complete = true;
      const event = { type: 'load', target: this };
      for (const fn of [...(this.listeners.get('load') ?? [])]) fn.call(this, event);
      if (typeof this.onload === 'function') this.onload(event);
    });
  }
}

export function installShims() {
  if (globalThis.document || globalThis.Image) throw new Error('a document or Image already exists; refusing to shadow it');
  globalThis.document = {
    createElement: () => ({ width: 0, height: 0, style: {}, getContext: () => pen, toDataURL: () => '' }),
    createElementNS: () => new InertImage(),
  };
  globalThis.Image = InertImage;
}

// lets pending inert image loads (and the callbacks they trigger) run
export const settle = async () => { for (let i = 0; i < 4; i++) await new Promise((r) => setImmediate(r)); };
