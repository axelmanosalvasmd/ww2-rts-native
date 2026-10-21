// Oracle for unreal/Source/ThreeCrossroads/Client/TCAudio.cpp (ThreeCrossroads.Audio.MixerOracle): runs the pinned
// reference module client/audio.js itself under Node, with a fake Web Audio context that records every source start
// (buffer, time, offset, playback rate, gain, pan, loop) and stop, Math.random replaced by mulberry32(seed), and
// performance.now and setTimeout on the fake clock. Prints the scenario and the records as C++ tables.
//   node mixer-oracle.mjs <reference checkout>
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const REF = process.argv[2];
const AUDIO = join(REF, 'client/audio');
const index = JSON.parse(readFileSync(join(AUDIO, 'index.json'), 'utf8'));
const VOICE_DUR = 1.5; // the fake decodes every voice line as 1.5 s (the test sets the same)

let seed = 20261013;
Math.random = () => { seed |= 0; seed = seed + 0x6D2B79F5 | 0; let t = Math.imul(seed ^ seed >>> 15, 1 | seed); t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t; return ((t ^ t >>> 14) >>> 0) / 4294967296; };

let now = 0;
const timers = [];
globalThis.performance = { now: () => now * 1000 };
globalThis.setTimeout = (fn, ms) => { timers.push({ due: now * 1000 + ms, fn }); return timers.length; };
globalThis.addEventListener = () => {};
const rows = [];
const param = (v = 0) => ({ value: v, setTargetAtTime() {}, cancelScheduledValues() {}, setValueAtTime() {}, linearRampToValueAtTime() {} });
const node = (extra) => { const n = { connect(next) { n.next = next; return next; }, ...extra }; return n; };
const sources = [];
class FakeContext {
  constructor() { this.state = 'running'; this.sampleRate = 48000; this.destination = node({}); }
  get currentTime() { return now; }
  createGain() { return node({ gain: param(1) }); }
  createStereoPanner() { return node({ pan: param(0) }); }
  createBiquadFilter() { return node({ frequency: param(), Q: param(), type: 'lowpass' }); }
  createDynamicsCompressor() { return node({ threshold: param(), knee: param(), ratio: param(), attack: param(), release: param() }); }
  createBuffer(ch, len, rate) { const d = new Float32Array(len); return { duration: len / rate, sampleRate: rate, getChannelData: () => d, key: null }; }
  decodeAudioData(ab) { return Promise.resolve(ab); }
  resume() {}
  createBufferSource() {
    const src = node({ buffer: null, playbackRate: param(1), loop: false,
      start(t, off = 0) {
        // the gain and panner are connected after start(): read them once the op is done (resolve below)
        rows.push({ start: 1, key: src.buffer.key, at: t, offset: off, rate: src.playbackRate.value, src, loop: src.loop ? 1 : 0 });
        src.end = src.loop ? Infinity : t + (src.buffer.duration - off) / src.playbackRate.value;
        sources.push(src);
      },
      stop(t) { if (src.ended) return; src.stopped = true; rows.push({ start: 0, key: src.buffer.key, at: t }); src.end = Math.min(src.end, t); },
    });
    return src;
  }
}
globalThis.AudioContext = FakeContext;
globalThis.fetch = async (url) => {
  const path = decodeURIComponent(new URL(url).pathname);
  const rel = path.slice(path.indexOf('/client/audio/') + '/client/audio/'.length);
  if (rel === 'index.json') return { ok: true, json: async () => index };
  const sfx = rel.match(/^sfx\/(.+)\.mp3$/), voice = rel.match(/^voice\/[a-z]+\/(.+)\.mp3$/);
  const dur = sfx ? index.sfx[sfx[1]].dur : VOICE_DUR;
  return { ok: true, arrayBuffer: async () => ({ duration: dur, sampleRate: 48000, key: sfx ? sfx[1] : `voice:${voice[1]}`, getChannelData: () => new Float32Array(16) }) };
};

const { audio } = await import(pathToFileURL(join(REF, 'client/audio.js')).href);
// the environment beds' synthesized buffers: name them for the records
const realCreate = FakeContext.prototype.createBuffer;
FakeContext.prototype.createBuffer = function (ch, len, rate) { const b = realCreate.call(this, ch, len, rate); b.key = 'amb:wind'; return b; }; // the scenario's only synthesized bed is the wind (environment() is never called with others)

const flush = async () => { for (let i = 0; i < 50; i++) await new Promise(r => setImmediate(r)); };
// time moves on: ended sources and due timers, in time order
function advance(T) {
  for (;;) {
    const s = sources.filter(x => !x.ended && x.end <= T).sort((a, b) => a.end - b.end)[0];
    const tm = timers.filter(x => !x.done && x.due <= T * 1000).sort((a, b) => a.due - b.due)[0];
    if (!s && !tm) break;
    if (tm && (!s || tm.due / 1000 <= s.end)) { now = tm.due / 1000; tm.done = true; tm.fn(); continue; }
    now = s.end; s.ended = true; s.onended?.();
  }
  now = T;
}

// the scenario: [t, op, name, x, z, hasPos, a, b, c]
const ops = [];
const op = (t, o, name = '', x = 0, z = 0, hasPos = 0, a = 0, b = 0, c = 0) => { if (ops.length && t < ops.at(-1)[0]) throw new Error(`scenario goes back in time at ${t}`); ops.push([t, o, name, x, z, hasPos, a, b, c]); };
op(0, 'start', '', 0, 0, 0, 3, 1);          // the UK (no voices: American), seat 1
op(0, 'listener', '', 10, 10, 0, 60, 0.3);
op(0.5, 'play', 'rifle', 12, 10, 1);
op(0.515625, 'play', 'rifle', 12, 10, 1);   // inside rifle's 0.04 s gap
op(0.625, 'play', 'rifle', 40, 30, 1);
for (let i = 0; i < 6; i++) op(0.75 + i * 0.046875, 'play', 'rifle', 10 + (i % 3) * 25, 10 + i * 4, 1); // max 6: the quietest gives way
op(1, 'voice', 'move');
op(1.5, 'voice', 'move');                   // too soon
op(4, 'voice', 'move');                     // another line than last time
op(7, 'alert', 'attack');
op(7.6, 'tick');
op(8, 'alert', 'attack');                   // inside the 6 s gap
op(8.5, 'alert', 'unitLost');
op(9.1, 'tick');
op(9.25, 'ui', 'error');
op(9.375, 'ui', 'error');                   // inside 0.3 s
op(9.5, 'ui', 'bogus');                     // clicks
op(10, 'play', 'fire_loop', 15, 12, 1, 0.8, 14, 1);
op(10, 'play', 'blast_small', 11, 10, 1);
op(10.25, 'play', 'match_start', 0, 0, 0, 0, 0, 0.7);
op(11, 'bed', 'tank_engine', 20, 10, 0, 30, 15);
op(11.5, 'listener', '', 30, 10, 0, 120, -0.5);
op(12, 'play', 'bomb', 35, 12, 1);
op(12.5, 'play', 'whistle', 30, 40, 1, 0, 0, 0.7);
// a crowded moment: the 32-voice cap gives way to louder sounds
const many = ['bomber', 'plane_flyby', 'rockets', 'bomb', 'blast_large', 'tankgun', 'flak', 'vehicle_destroyed', 'smoke', 'sniper', 'atgun', 'whistle', 'mg'];
for (let k = 0; k < 4; k++) many.forEach((name, j) => op(20 + k * 0.25, 'play', name, 30 + ((j * 7 + k * 5) % 40) - 20, 10 + ((j * 3 + k * 11) % 30) - 15, 1));
op(22, 'end');

audio.bind({ value: 0, addEventListener() {}, blur() {} });
for (const [t, o, name, x, z, hasPos, a, b, c] of ops) {
  advance(t);
  if (o === 'start') { audio.start({ faction: a, slot: b }); await flush();}
  else if (o === 'listener') audio.listener(x, z, a, b);
  else if (o === 'play') audio.play(name, hasPos ? { x, z } : null, { ...(a ? { delay: a } : {}), ...(b ? { dur: b } : {}), ...(c ? { gain: c } : {}) });
  else if (o === 'voice') audio.voice(name);
  else if (o === 'alert') audio.alert(name);
  else if (o === 'ui') audio.ui(name);
  else if (o === 'bed') audio.bed(name, [{ x, z }, { x: a, z: b }]);
  else if (o === 'end') audio.end();
  for (const r of rows) {
    if (!r.src) continue;
    // src -> [a loop's own envelope] -> gain -> panner -> bus: the level is the gain just before the panner
    let n = r.src.next, last = null, pan = null;
    while (n) { if (n.pan) { pan = n; break; } if (n.gain) last = n; n = n.next; }
    r.gain = last?.gain.value ?? 0; r.pan = pan?.pan.value ?? 0; delete r.src;
  }
}
const f = (v) => (Number.isInteger(v) ? `${v}.0` : String(v));
console.log('// generated by mixer-oracle.mjs: the scenario');
console.log('static const FTCAudioOracleOp OracleOps[] = {');
for (const [t, o, name, x, z, hasPos, a, b, c] of ops) console.log(`\t{ ${f(t)}, TEXT("${o}"), TEXT("${name}"), ${f(x)}, ${f(z)}, ${hasPos}, ${f(a)}, ${f(b)}, ${f(c)} },`);
console.log('};');
console.log('// client/audio.js under Node: every start (key, at, offset, rate, gain, pan, loop) and stop (key, at)');
console.log('static const FTCAudioOracleRow OracleRows[] = {');
for (const r of rows) console.log(`\t{ ${r.start}, TEXT("${r.key}"), ${f(r.at)}, ${f(r.offset ?? 0)}, ${f(r.rate ?? 0)}, ${f(r.gain ?? 0)}, ${f(r.pan ?? 0)}, ${r.loop ?? 0} },`);
console.log('};');
console.log(`// stats: ${JSON.stringify(audio.stats())}`);
