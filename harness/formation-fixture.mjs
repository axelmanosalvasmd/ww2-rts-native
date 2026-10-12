// Writes rust/crates/core/tests/fixtures/formation.txt: what the pinned shared/formation.js returns, so
// `cargo test -p ww2-core --test formation` can check the Rust port bit for bit, and
// rust/crates/ffi/examples/link_check.c can check the C interface. Run it from the repo root with an
// x86-64 build of Node 24.21.0, like harness/math-fixture.mjs (facingSpots calls Math.cos and Math.sin,
// whose last bits differ on arm64 Node):
//
//   node harness/formation-fixture.mjs
//   arch -x86_64 path/to/node-v24.21.0-darwin-x64/bin/node harness/formation-fixture.mjs
//
// Each range records how to rebuild its cases (a seed, a count, unit counts, a placement style) and an
// FNV-1a 64-bit hash of each case's spots. The Rust test rebuilds the same cases with the same
// generator, written out again in rust/crates/core/tests/formation.rs, so the two must stay in step.
// Hand-written cases, and the first small case of each range, are written out in full. Numbers are the
// hex of their IEEE 754 bits.
import { writeFileSync } from 'node:fs';

const { facingSpots, normalizeFace, slotSize, SHAPES } = await import('../reference/ww2-rts/shared/formation.js');

const OUT = new URL('../rust/crates/core/tests/fixtures/formation.txt', import.meta.url);

// [label, seed, cases, fewest units, most units, placement styles to pick from]
const RANGES = [
  ['mixed', 1, 5000, 1, 80, ['scatter']],
  ['ties', 2, 3000, 2, 80, ['ties']],
  ['ordered', 3, 2000, 1, 80, ['ordered']],
  // 64 units and more reach TimSort's merges and galloping; the game allows large selections.
  ['large', 4, 300, 81, 400, ['scatter', 'ties', 'ordered', 'split']],
];

// mulberry32 as reference/ww2-rts/shared/ai-schedule.js computes it, with the state kept in 32 bits
// (the same numbers, without its limit of about 4.9 million exact draws).
function seeded(seed) {
  let value = seed >>> 0;
  return () => {
    value = (value + 0x6D2B79F5) >>> 0;
    let t = value;
    t = Math.imul(t ^ t >>> 15, t | 1);
    t ^= t + Math.imul(t ^ t >>> 7, t | 61);
    return ((t ^ t >>> 14) >>> 0) / 4294967296;
  };
}

const below = (rand, n) => Math.floor(rand() * n);
// The Formation menu's spacing steps (client/main.js:1445).
const SPREADS = [1, 1.25, 1.5, 1.75, 2, 2.25, 2.5];

function makeFace(rand) {
  const r = rand();
  if (r < 0.25) return (below(rand, 17) - 8) * (Math.PI / 2); // multiples of pi/2 from -4 pi to 4 pi
  if (r < 0.3) return rand() < 0.5 ? Math.PI : -Math.PI;
  return (rand() * 4 - 2) * Math.PI;
}

function makeReach(rand) {
  const r = rand();
  if (r < 0.25) return 0; // a plain click
  if (r < 0.32) return Infinity;
  if (r < 0.36) return NaN;
  if (r < 0.39) return -Infinity;
  if (r < 0.44) return -rand() * 40;
  return rand() * 160;
}

function makeSpread(rand) {
  const r = rand();
  if (r < 0.75) return SPREADS[below(rand, SPREADS.length)];
  if (r < 0.95) return 0.5 + rand() * 2.5;
  if (r < 0.97) return 0;
  if (r < 0.98) return NaN;
  return -1 - rand();
}

function makeSize(rand) {
  const r = rand();
  if (r < 0.55) return 3; // infantry
  if (r < 0.95) return 5 + rand() * 6; // vehicles and guns
  return rand() * 3; // narrower than any real slot
}

// Each style returns where unit i stands, given the units placed so far.
const PLACE = {
  // a loose crowd
  scatter(rand, face, cx, cz) {
    const scatter = 5 + rand() * 60;
    return () => [cx + (rand() * 2 - 1) * scatter, cz + (rand() * 2 - 1) * scatter];
  },
  // rows across the facing, each unit within 1.2 cm of its row, some on top of another unit: the
  // first comparator rounds these to ties and is then not transitive
  ties(rand, face, cx, cz) {
    const dx = Math.cos(face), dz = Math.sin(face), rows = 1 + below(rand, 4);
    return (i, items) => {
      if (i > 0 && rand() < 0.1) { const j = below(rand, i); return [items[j].x, items[j].z]; }
      const row = below(rand, rows), across = (rand() * 2 - 1) * 30, depth = row * 4 + (rand() * 2 - 1) * 0.012;
      return [cx + dx * depth - dz * across, cz + dz * depth + dx * across];
    };
  },
  // already in ranks, facing the order's way, turned or reversed: long runs for TimSort
  ordered(rand, face, cx, cz) {
    const turn = [0, 0, Math.PI, Math.PI / 2, -Math.PI / 2, 0.3][below(rand, 6)];
    const ex = Math.cos(face + turn), ez = Math.sin(face + turn), cols = 1 + below(rand, 12);
    const spacing = 3 + rand() * 4, depth = 4 + rand() * 3, jitter = [0, 0.3][below(rand, 2)];
    return i => {
      const r = Math.floor(i / cols), c = i % cols, across = (c - cols / 2) * spacing;
      const fwd = -r * depth + (rand() < jitter ? (rand() * 2 - 1) * 0.01 : 0);
      return [cx + ex * fwd - ez * across, cz + ez * fwd + ex * across];
    };
  },
  // a loose crowd, then units in ranks: short runs, then a long one, so the run stack merges below its top
  split(rand, face, cx, cz, n) {
    const loose = PLACE.scatter(rand, face, cx, cz, n), ranks = PLACE.ordered(rand, face, cx, cz, n);
    const cut = Math.floor(n * (0.2 + rand() * 0.4));
    return (i, items) => i < cut ? loose(i, items) : ranks(i - cut, items);
  },
};

function makeCase(rand, fewest, most, styles) {
  const shape = SHAPES[below(rand, SHAPES.length)];
  const n = fewest + below(rand, most - fewest + 1);
  const style = styles[below(rand, styles.length)];
  const face = makeFace(rand), reach = makeReach(rand), spread = makeSpread(rand);
  const at = { x: rand() * 1200, z: rand() * 1200 };
  const kinds = 1 + below(rand, 3), palette = [makeSize(rand), makeSize(rand), makeSize(rand)];
  const backRate = [0, 0, 0.1, 0.3][below(rand, 4)];
  const cx = rand() * 1200, cz = rand() * 1200;
  const place = PLACE[style](rand, face, cx, cz, n);
  const items = [];
  for (let i = 0; i < n; i++) {
    const [x, z] = place(i, items);
    items.push({ id: below(rand, 4294967296), x, z, size: palette[below(rand, kinds)], back: rand() < backRate });
  }
  if (style === 'ordered') {
    const swaps = [0, 0, 1, 3][below(rand, 4)];
    for (let k = 0; k < swaps; k++) {
      const i = below(rand, n), j = below(rand, n);
      [items[i], items[j]] = [items[j], items[i]];
    }
    if (rand() < 0.25) items.reverse();
  }
  return { shape, face, reach, spread, at, items };
}

// Hand-written cases: the degenerate ones a client meets first, each shape, and the comparator's corners.
const unit = (id, x, z, size = 3, back = false) => ({ id, x, z, size, back });
const row = (n, f) => Array.from({ length: n }, (_, i) => f(i));
const six = [unit(11, 100, 100), unit(12, 104, 101), unit(13, 98, 96, 5.9), unit(14, 103, 92, 3, true),
  unit(15, 96, 104, 7.5), unit(16, 101, 99)];
const near = [unit(1, 0, 0), unit(2, 0.004, 0), unit(3, 0.008, 0)]; // 4 mm apart: a~b, b~c, but a<c
const permutations = list => list.length <= 1 ? [list] :
  list.flatMap((x, i) => permutations([...list.slice(0, i), ...list.slice(i + 1)]).map(rest => [x, ...rest]));
const EDGES = [
  ['one', 'line', 1, 0, 0, { x: 50, z: 60 }, [unit(7, 40, 40)]], // one unit, zero-length line
  ['line.click', 'line', 1, 0, 0.7, { x: 120, z: 80 }, six], // zero-length line: no drag
  ['line.drag', 'line', 1, 30, 0.7, { x: 120, z: 80 }, six], // a 30 m drag
  ['one.wedge', 'wedge', 1.5, Infinity, -2, { x: 5, z: 5 }, [unit(8, 0, 0, 9.5)]],
  ['empty', 'line', 1, 0, 0, { x: 0, z: 0 }, []],
  ['block', 'block', 1.25, 12, Math.PI / 2, { x: 300, z: 300 }, row(7, i => unit(20 + i, 290 + i * 2, 310 - i))],
  ['column', 'column', 1, 50, -Math.PI / 2, { x: 300, z: 300 }, row(7, i => unit(30 + i, 290 + i * 2, 310 - i, 5))],
  ['wedge', 'wedge', 2, 0, 3, { x: 300, z: 300 }, row(10, i => unit(40 + i, 280 + i * 3, 300 + (i % 3)))],
  ['spread0.click', 'line', 0, 0, 1, { x: 10, z: 10 }, six], // fit is 0/0: no ranks, no spots
  ['spread0.drag', 'block', 0, 20, 1, { x: 10, z: 10 }, six],
  ['spread.nan', 'line', NaN, 20, 1, { x: 10, z: 10 }, six],
  ['size0', 'line', 1, 0, 1, { x: 10, z: 10 }, row(4, i => unit(50 + i, i, 0, 0))],
  ['face.nan', 'line', 1, 10, NaN, { x: 10, z: 10 }, six],
  ['face.pi', 'line', 1, 10, Math.PI, { x: 10, z: 10 }, six],
  ['face.-pi', 'line', 1, 10, -Math.PI, { x: 10, z: 10 }, six],
  ['face.-0', 'line', 1, 10, -0, { x: 10, z: 10 }, six],
  ['reach.inf', 'line', 1, Infinity, 2, { x: 10, z: 10 }, six],
  ['reach.nan', 'block', 1, NaN, 2, { x: 10, z: 10 }, six],
  ['reach.neg', 'line', 1, -25, 2, { x: 10, z: 10 }, six],
  ['same.spot', 'line', 1, 0, 0.3, { x: 10, z: 10 }, row(5, i => unit(60 + i, 7, 7, 3, i === 2))],
  // Math.round(0.5) is 1 but Math.round(-0.5) is -0, so a ranks before b while b does not rank before a
  ['half.cm', 'line', 1, 0, 0, { x: 10, z: 10 }, [unit(1, 0, 0), unit(2, 0.005, 0)]],
  ['half.cm.swapped', 'line', 1, 0, 0, { x: 10, z: 10 }, [unit(2, 0.005, 0), unit(1, 0, 0)]],
  ...permutations(near).map((list, k) => [`near.${k}`, 'line', 1, 0, 0, { x: 10, z: 10 }, list]),
  // 70 units, two runs and a merge, a few millimetres between ranks
  ['merge.70', 'line', 1, 40, 0.25, { x: 500, z: 500 },
    row(70, i => unit(100 + i, 480 + (i % 10) * 3 + (i * 7 % 5) * 0.002, 470 + Math.floor(i / 10) * 4 - (i % 3) * 0.003, 3 + (i % 4)))],
];

const f64 = new Float64Array(1);
const u64 = new BigUint64Array(f64.buffer);
const bitsOf = x => { if (Number.isNaN(x)) return 0x7ff8000000000000n; f64[0] = x; return u64[0]; };
const hex = x => bitsOf(x).toString(16).padStart(16, '0');

// FNV-1a 64 over the little-endian bytes of the bits of each spot's id, x and z.
function hashSpots(spots) {
  let hash = 0xcbf29ce484222325n;
  for (const spot of spots) {
    for (const value of spot) {
      const bits = bitsOf(value);
      for (let shift = 0n; shift < 64n; shift += 8n) {
        hash ^= (bits >> shift) & 0xffn;
        hash = BigInt.asUintN(64, hash * 0x100000001b3n);
      }
    }
  }
  return hash.toString(16).padStart(16, '0');
}

const run = c => facingSpots(c.items, c.at, c.face, c.reach, { shape: c.shape, spread: c.spread });

function writeCase(lines, label, c) {
  const spots = run(c);
  lines.push(`case ${label} ${c.shape} ${[c.spread, c.reach, c.face, c.at.x, c.at.z].map(hex).join(' ')} ${c.items.length} ${spots.length}`);
  for (const u of c.items) lines.push(`item ${u.id} ${[u.x, u.z, u.size].map(hex).join(' ')} ${u.back ? 1 : 0}`);
  for (const [id, x, z] of spots) lines.push(`spot ${id} ${hex(x)} ${hex(z)}`);
}

const FACES = [0, -0, Math.PI, -Math.PI, Math.PI / 2, -Math.PI / 2, 3 * Math.PI, -3 * Math.PI, 2 * Math.PI, 1e-300,
  1e6, 1e22, NaN, Infinity, -Infinity, ...row(40, i => (i - 20) * 0.7853981633974483 + i * 1e-3)];
const RADII = [0, -0, 0.5, 0.75, 0.7500000000000001, 1.75, 2.2, 3.1, -1, NaN, Infinity];

function main() {
  if (process.arch !== 'x64') {
    throw new Error(`run this with an x64 Node, not ${process.arch}: arm64 builds compute other cos and sin bits`);
  }
  const lines = [
    '# Written by harness/formation-fixture.mjs from reference/ww2-rts/shared/formation.js; regenerate with x64 Node. Do not edit.',
    '# Numbers are the hex of their IEEE 754 bits; NaN is always 7ff8000000000000.',
    '# range <label> <seed> <cases> <fewest units> <most units> <styles>, then its cases\' hashes, eight per line',
    '# case <label> <shape> <spread> <reach> <face> <at x> <at z> <items> <spots>, then its item and spot lines',
    '# item <id> <x> <z> <size> <back>; spot <id> <x> <z>',
    '# face <angle> <normalizeFace(angle), or null>; slot <infantry> <radius> <slotSize>',
    `node ${process.version}`,
    `v8 ${process.versions.v8}`,
    `arch ${process.arch}`,
  ];
  let cases = 0;
  for (const [label, seed, count, fewest, most, styles] of RANGES) {
    const rand = seeded(seed), hashes = [];
    let sample = null;
    for (let i = 0; i < count; i++) {
      const c = makeCase(rand, fewest, most, styles);
      hashes.push(hashSpots(run(c)));
      if (!sample && c.items.length <= 30) sample = [`${label}#${i}`, c];
    }
    lines.push(`range ${label} ${seed} ${count} ${fewest} ${most} ${styles.join(',')}`);
    for (let i = 0; i < hashes.length; i += 8) lines.push(`hashes ${hashes.slice(i, i + 8).join(' ')}`);
    if (sample) writeCase(lines, ...sample);
    cases += count;
  }
  for (const [label, shape, spread, reach, face, at, items] of EDGES) writeCase(lines, label, { shape, spread, reach, face, at, items });
  for (const a of FACES) {
    const f = normalizeFace(a);
    lines.push(`face ${hex(a)} ${f === null ? 'null' : hex(f)}`);
  }
  for (const infantry of [true, false]) {
    for (const radius of RADII) lines.push(`slot ${infantry ? 1 : 0} ${hex(radius)} ${hex(slotSize({ infantry, radius }))}`);
  }
  const text = `${lines.join('\n')}\n`;
  writeFileSync(OUT, text);
  console.log(`wrote ${OUT.pathname}: ${cases} seeded cases, ${EDGES.length} hand-written, ${text.length} bytes`);
}

main();
