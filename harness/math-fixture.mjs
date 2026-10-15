// Writes rust/crates/core/tests/fixtures/math.txt: what Node computes for each function that
// rust/crates/core/src/math.rs ports from V8. `cargo test -p ww2-core --test it math` checks the port
// against it bit for bit. Run it from the repo root with an x86-64 build of the Node the game pins
// (24.21.0, V8 13.6): on the Vostro, or on an Apple Silicon Mac the darwin-x64 build under Rosetta:
//
//   node harness/math-fixture.mjs
//   arch -x86_64 path/to/node-v24.21.0-darwin-x64/bin/node harness/math-fixture.mjs
//
// Arm64 builds compute other last bits: clang fuses `a * b + c` in V8's fdlibm into one fused
// multiply-add (its default, and arm64 always has the instruction), which rounds once instead of
// twice. Baseline x86-64 has no such instruction, so x64 builds compute the C source as written, the
// same on every OS, and that is what the core ports.
//
// Each range records how to rebuild its inputs (seed, count, bounds), an FNV-1a 64-bit hash of the
// bits of every output, and its first 16 inputs and outputs for diagnosis. Each function also has a
// list of edge cases. Numbers are written as the hex of their IEEE 754 bits.
import { writeFileSync } from 'node:fs';
import { setFlagsFromString } from 'node:v8';

const OUT = new URL('../rust/crates/core/tests/fixtures/math.txt', import.meta.url);
const COUNT = 200_000;
const SAMPLES = 16;

// mulberry32, as reference/ww2-rts/shared/ai-schedule.js writes it. There `value` grows past 2^32 and
// each use reduces it mod 2^32, so it matches the Rust test's u32 version while it stays an exact
// integer: below 2^53, about 4.9 million draws.
const MAX_DRAWS = Math.floor(2 ** 53 / 0x6D2B79F5);
function seeded(seed) {
  let value = seed >>> 0;
  return () => {
    let t = value += 0x6D2B79F5;
    t = Math.imul(t ^ t >>> 15, t | 1);
    t ^= t + Math.imul(t ^ t >>> 7, t | 61);
    return ((t ^ t >>> 14) >>> 0) / 4294967296;
  };
}

// A uniform number in [0, 1] with about 53 random bits, from two draws.
const unit = rand => rand() + rand() / 4294967296;

// How an argument maps a unit number into its range: plain arithmetic, so the Rust test repeats it.
const KINDS = {
  real: (lo, hi, u) => lo + (hi - lo) * u,
  int: (lo, hi, u) => Math.floor(lo + (hi - lo) * u),
  half: (lo, hi, u) => Math.floor(lo + (hi - lo) * u) + 0.5,
};

const FUNCTIONS = {
  sin: x => Math.sin(x),
  cos: x => Math.cos(x),
  tan: x => Math.tan(x),
  atan: x => Math.atan(x),
  atan2: (y, x) => Math.atan2(y, x),
  exp: x => Math.exp(x),
  log: x => Math.log(x),
  pow: (x, y) => x ** y,
  hypot: (a, b) => Math.hypot(a, b),
  hypot3: (a, b, c) => Math.hypot(a, b, c),
  round: x => Math.round(x),
  max: (a, b) => Math.max(a, b),
  min: (a, b) => Math.min(a, b),
  sign: x => Math.sign(x),
};

// [label, function, seed, [kind, lo, hi] per argument]. The game ranges cover what shared/ calls;
// the wide and huge ones reach the large-argument reductions, overflow and underflow.
const real = (lo, hi) => ['real', lo, hi];
const RANGES = [
  ['sin.game', 'sin', 1, [real(-20, 20)]],
  ['sin.wide', 'sin', 2, [real(-1e6, 1e6)]],
  ['sin.huge', 'sin', 3, [real(-1e300, 1e300)]],
  ['cos.game', 'cos', 4, [real(-20, 20)]],
  ['cos.wide', 'cos', 5, [real(-1e6, 1e6)]],
  ['cos.huge', 'cos', 6, [real(-1e300, 1e300)]],
  ['tan.game', 'tan', 7, [real(-1.5, 1.5)]],
  ['tan.wide', 'tan', 8, [real(-1e6, 1e6)]],
  ['tan.huge', 'tan', 9, [real(-1e300, 1e300)]],
  ['atan.game', 'atan', 10, [real(-4, 4)]],
  ['atan.wide', 'atan', 11, [real(-1e6, 1e6)]],
  ['atan2.game', 'atan2', 12, [real(-1000, 1000), real(-1000, 1000)]],
  ['exp.game', 'exp', 13, [real(-50, 50)]],
  ['exp.wide', 'exp', 14, [real(-800, 800)]],
  ['log.game', 'log', 15, [real(0, 1e6)]],
  ['log.unit', 'log', 16, [real(0, 2)]],
  ['pow.game', 'pow', 17, [real(0, 100), real(-10, 10)]],
  ['pow.wide', 'pow', 18, [real(0, 1000), real(-200, 200)]],
  ['pow.int', 'pow', 19, [real(-10, 10), ['int', -30, 30]]],
  ['hypot.game', 'hypot', 20, [real(-1000, 1000), real(-1000, 1000)]],
  ['hypot3.game', 'hypot3', 21, [real(-1000, 1000), real(-1000, 1000), real(-1000, 1000)]],
  ['round.game', 'round', 22, [real(-1000, 1000)]],
  ['round.halves', 'round', 23, [['half', -1e6, 1e6]]],
  ['max.game', 'max', 24, [real(-1000, 1000), real(-1000, 1000)]],
  ['min.game', 'min', 25, [real(-1000, 1000), real(-1000, 1000)]],
  ['sign.game', 'sign', 26, [real(-1, 1)]],
];

const f64 = new Float64Array(1);
const u64 = new BigUint64Array(f64.buffer);
const NAN_BITS = 0x7ff8000000000000n;
const bitsOf = x => { if (Number.isNaN(x)) return NAN_BITS; f64[0] = x; return u64[0]; };
const fromBits = bits => { u64[0] = BigInt.asUintN(64, bits); return f64[0]; };
const hex = x => bitsOf(x).toString(16).padStart(16, '0');
// The double whose high 32 bits are `high`: how fdlibm states its thresholds.
const word = (high, low = 0) => fromBits(BigInt(high) << 32n | BigInt(low));
// x and its two neighbouring doubles (x positive and finite).
const around = x => [fromBits(bitsOf(x) - 1n), x, fromBits(bitsOf(x) + 1n)];
const signed = xs => xs.flatMap(x => [x, -x]);
const cross = (...lists) => lists.reduce((acc, list) => acc.flatMap(a => list.map(b => [...a, b])), [[]]);

// FNV-1a 64 over the little-endian bytes of each output's bits.
function fnv1a(hash, bits) {
  for (let shift = 0n; shift < 64n; shift += 8n) {
    hash ^= (bits >> shift) & 0xffn;
    hash = BigInt.asUintN(64, hash * 0x100000001b3n);
  }
  return hash;
}

const MAX = Number.MAX_VALUE;
const SPECIAL = [0, -0, Infinity, -Infinity, NaN, 5e-324, -5e-324, 2.225073858507201e-308, 2.2250738585072014e-308,
  MAX, -MAX, 1, -1, 0.5, -0.5, 2 ** -28, 2 ** -27, 1e-300, -1e-300, 1e300, -1e300];
const TRIG = [
  ...SPECIAL,
  // next to multiples of pi/2, small to large
  ...signed([1, 2, 3, 4, 5, 8, 32, 1000, 2 ** 19, 2 ** 20, 1e6, 1e15]
    .flatMap(k => around(k * (Math.PI / 2)))),
  // fdlibm's thresholds: pi/4, 3pi/4, 2^19*pi/2, the kernels' 2^-27, 2^-28, 0.3, 0.78125 and 0.6744
  ...signed([0x3FE921FB, 0x3FE921FC, 0x4002D97C, 0x413921FB, 0x413921FC, 0x3E400000, 0x3E300000, 0x3FD33333,
    0x3FE90000, 0x3FE59428].flatMap(high => around(word(high)))),
  ...signed([0x3FE921FB, 0x413921FB].map(high => word(high, 0xFFFFFFFF))),
  // large arguments for __kernel_rem_pio2
  ...signed([1e22, 1e100, 1e200, 2 ** 1000, 2 ** 1023, 1.2345678901234567e300]),
];

const EDGES = {
  sin: TRIG.map(x => [x]),
  cos: TRIG.map(x => [x]),
  tan: TRIG.map(x => [x]),
  atan: [...SPECIAL, ...signed([0x3FDC0000, 0x3FE60000, 0x3FF30000, 0x40038000, 0x44100000, 0x3E400000]
    .flatMap(high => around(word(high))))].map(x => [x]),
  atan2: cross(...Array(2).fill([0, -0, Infinity, -Infinity, NaN, 1, -0.5, 5e-324, 1e-300, 1e300, -1e300])),
  exp: [...SPECIAL,
    ...around(709.782712893384), ...around(-745.1332191019411), ...around(word(0x40862E42)),
    ...signed([1, 2, 50, 100, 0.34657359027997264, 1.0397207708399179]), 709.5, 709.7, -708.4, -709.1, -720, -740, -745,
    ...signed([0x3FD62E42, 0x3FD62E43, 0x3FF0A2B2, 0x3E300000].flatMap(high => around(word(high))))].map(x => [x]),
  log: [...SPECIAL, ...around(1), ...around(2), Math.E, ...around(Math.SQRT2), ...around(Math.SQRT1_2),
    1 + 2 ** -20, 1 - 2 ** -20, 1 + 2 ** -21, 10, 1e-310, 2.5e-320,
    ...[0x3FF6147A, 0x3FF6B851].flatMap(high => around(word(high)))].map(x => [x]),
  pow: cross(
    [0, -0, 1, -1, 0.5, -2, -3, Infinity, -Infinity, NaN, 5e-324, 1e300, 1.0000001, 0.9999999],
    [0, -0, 1, -1, 2, -2, 0.5, -0.5, 3, -3, 1.5, Infinity, -Infinity, NaN, 1e300, 2147483649, 0.3]),
  hypot: cross(...Array(2).fill([0, -0, Infinity, -Infinity, NaN, -3, 5e-324, 1e300, MAX])),
  hypot3: cross(...Array(3).fill([-0, Infinity, NaN, 3, 1e300])),
  round: [0, -0, NaN, Infinity, -Infinity, 0.5, -0.5, 1.5, -1.5, 2.5, -2.5, 0.49999999999999994, -0.49999999999999994,
    0.5000000000000001, -0.5000000000000001, 0.25, -0.25, 5e-324, -5e-324, 3.7, -3.7, 2 ** 52, 2 ** 52 + 1,
    2 ** 52 - 0.5, -(2 ** 52 - 0.5), 2 ** 53, MAX, -MAX].map(x => [x]),
  max: cross(...Array(2).fill([0, -0, 1, -1, NaN, Infinity, -Infinity])),
  min: cross(...Array(2).fill([0, -0, 1, -1, NaN, Infinity, -Infinity])),
  sign: [0, -0, NaN, Infinity, -Infinity, 5e-324, -5e-324, 1, -2, 1e300].map(x => [x]),
};

function main() {
  if (process.arch !== 'x64') {
    throw new Error(`run this with an x64 Node, not ${process.arch}: arm64 builds fuse multiply-adds (see the top)`);
  }
  // Node's default Math.pow and ** call the OS pow (macOS libm, glibc, the Windows CRT), so their last
  // bits vary by platform. The core ports V8's own pow, which is what Node runs with this flag. V8
  // reads the flag on every pow call, so setting it here covers every pow below.
  setFlagsFromString('--no-use-std-math-pow');

  const lines = [
    '# Written by harness/math-fixture.mjs; regenerate with `node harness/math-fixture.mjs`. Do not edit.',
    '# Numbers are the hex of their IEEE 754 bits. NaN is always 7ff8000000000000: JavaScript has one NaN.',
    "# pow is V8's own pow (Node with --no-use-std-math-pow); Node's default pow is the OS library's.",
    '# range <label> <function> <seed> <count> <FNV-1a 64 of the outputs> <kind:lo:hi per argument>',
    '# sample <label> <inputs> <output>: the first 16 of each range',
    '# edge <function> <inputs> <output>',
    `node ${process.version}`,
    `v8 ${process.versions.v8}`,
    `arch ${process.arch}`,
  ];
  for (const [label, name, seed, args] of RANGES) {
    if (COUNT * args.length * 2 > MAX_DRAWS) throw new Error(`${label} needs more draws than mulberry32 keeps exact`);
    const f = FUNCTIONS[name], rand = seeded(seed), samples = [];
    let hash = 0xcbf29ce484222325n;
    for (let i = 0; i < COUNT; i++) {
      const inputs = args.map(([kind, lo, hi]) => KINDS[kind](lo, hi, unit(rand)));
      const output = f(...inputs);
      hash = fnv1a(hash, bitsOf(output));
      if (i < SAMPLES) samples.push(`sample ${label} ${[...inputs, output].map(hex).join(' ')}`);
    }
    const bounds = args.map(([kind, lo, hi]) => `${kind}:${lo}:${hi}`).join(' ');
    lines.push(`range ${label} ${name} ${seed} ${COUNT} ${hash.toString(16).padStart(16, '0')} ${bounds}`, ...samples);
  }
  for (const [name, cases] of Object.entries(EDGES)) {
    for (const inputs of cases) lines.push(`edge ${name} ${[...inputs, FUNCTIONS[name](...inputs)].map(hex).join(' ')}`);
  }
  writeFileSync(OUT, `${lines.join('\n')}\n`);
  console.log(`wrote ${OUT.pathname}: ${RANGES.length} ranges, ${lines.filter(l => l.startsWith('edge')).length} edge cases`);
}

main();
