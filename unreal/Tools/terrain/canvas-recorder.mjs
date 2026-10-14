// The browser APIs client/ground.js touches under Node, for the painted-ground oracle (ground-oracle.mjs). Unlike the
// models tooling's inert shims (unreal/Tools/models/shims.mjs), this canvas keeps real pixels: putImageData writes into
// a backing store and getImageData reads it, so the reference raster() output can be read back byte for byte. The
// vector drawing ground.js does on top (shell holes, trench cuts, contour lines: overlays()) is recorded as a list of
// operations with their exact geometry, colours, line widths and clip, not rasterised (anti-aliasing is the browser's).
// Images never load: ground.js then keeps its flat colours (LOOK avg), which makes the raster deterministic without a
// JPEG decoder. The textured look is checked in a real browser instead (tiles-page.mjs).
export class RecordingContext {
  constructor(canvas) {
    this.canvas = canvas;
    this.fillStyle = '#000'; this.strokeStyle = '#000'; this.lineWidth = 1; this.lineCap = 'butt'; this.lineJoin = 'miter';
    this.imageSmoothingEnabled = true; this.imageSmoothingQuality = 'low';
    this.path = []; this.clipRect = null; this.stack = [];
  }
  createImageData(w, h) { return { width: w, height: h, data: new Uint8ClampedArray(w * h * 4) }; }
  putImageData(img, x, y) {
    const c = this.canvas, W = c.width;
    for (let j = 0; j < img.height; j++) {
      const yy = y + j;
      if (yy < 0 || yy >= c.height) continue;
      c.data.set(img.data.subarray(j * img.width * 4, (j + 1) * img.width * 4), (yy * W + x) * 4);
    }
  }
  getImageData(x, y, w, h) {
    const c = this.canvas, out = new Uint8ClampedArray(w * h * 4);
    for (let j = 0; j < h; j++) out.set(c.data.subarray(((y + j) * c.width + x) * 4, ((y + j) * c.width + x + w) * 4), j * w * 4);
    return { width: w, height: h, data: out };
  }
  drawImage() { throw new Error('drawImage: the oracle paints flat (no textures); this path should not run'); }
  save() { this.stack.push({ fillStyle: this.fillStyle, strokeStyle: this.strokeStyle, lineWidth: this.lineWidth, lineCap: this.lineCap, lineJoin: this.lineJoin, clipRect: this.clipRect }); }
  restore() { const s = this.stack.pop(); if (s) Object.assign(this, s); }
  beginPath() { this.path = []; }
  moveTo(x, y) { this.path.push(['M', x, y]); }
  lineTo(x, y) { this.path.push(['L', x, y]); }
  arc(x, y, r, a0, a1) { this.path.push(['A', x, y, r, a0, a1]); }
  rect(x, y, w, h) { this.path.push(['R', x, y, w, h]); }
  clip() { const r = this.path.find(p => p[0] === 'R'); this.clipRect = r ? r.slice(1) : null; }
  createRadialGradient(x0, y0, r0, x1, y1, r1) { return { type: 'radial', x0, y0, r0, x1, y1, r1, stops: [], addColorStop(o, c) { this.stops.push([o, c]); } }; }
  style(s) { return typeof s === 'string' ? s : { type: s.type, x0: s.x0, y0: s.y0, r0: s.r0, x1: s.x1, y1: s.y1, r1: s.r1, stops: s.stops }; }
  fill() { this.canvas.ops.push({ op: 'fill', style: this.style(this.fillStyle), path: this.path.slice(), clip: this.clipRect }); }
  stroke() { this.canvas.ops.push({ op: 'stroke', style: this.style(this.strokeStyle), lineWidth: this.lineWidth, cap: this.lineCap, join: this.lineJoin, path: this.path.slice(), clip: this.clipRect }); }
}

export class RecordingCanvas {
  constructor() { this._w = 0; this._h = 0; this.data = new Uint8ClampedArray(0); this.ops = []; this.style = {}; this.ctx = null; }
  get width() { return this._w; }
  set width(v) { this._w = v; this.data = new Uint8ClampedArray(this._w * this._h * 4); }
  get height() { return this._h; }
  set height(v) { this._h = v; this.data = new Uint8ClampedArray(this._w * this._h * 4); }
  getContext() { return (this.ctx ??= new RecordingContext(this)); }
  toDataURL() { return ''; }
}

// An image that never finishes loading: ground.js keeps `ready` false and paints its flat colours.
class PendingImage {
  constructor() { this.width = 0; this.height = 0; this.onload = null; this.onerror = null; this.src = ''; }
  addEventListener() {}
  removeEventListener() {}
}

export function installGroundShims() {
  if (globalThis.document || globalThis.Image) throw new Error('a document or Image already exists; refusing to shadow it');
  const canvases = [];
  globalThis.document = {
    createElement: (tag) => { if (tag !== 'canvas') throw new Error('unexpected element ' + tag); const c = new RecordingCanvas(); canvases.push(c); return c; },
    createElementNS: () => new PendingImage(),
  };
  globalThis.Image = PendingImage;
  globalThis.window = {}; // ground.js createGround leaves its debug handle at window.__ground
  return canvases;
}
