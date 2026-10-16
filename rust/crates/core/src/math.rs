//! The core's own math. On every platform these functions return the bits that x86-64 builds of
//! Node 24.21.0 (V8 13.6) return, so the server, the clients' previews and replays agree with each
//! other and with the JavaScript game. Rust's `f64` methods and the `libm` crate call or copy other
//! C libraries and differ from V8 in the last bit for sin, cos, tan, atan2, exp, log and hypot.
//!
//! Arm64 builds of Node differ from x64 ones: clang compiles V8's fdlibm with `a * b + c` fused
//! into one rounding (its default `-ffp-contract=on`), and arm64 always has the instruction.
//! Baseline x86-64 has none, so x64 Node computes the C source as written (checked on Linux and
//! macOS), and this port follows the source. On an Apple Silicon Mac the arm64 Node disagrees with
//! it in the last bit for some inputs of every fdlibm function here.
//!
//! `sin` through `pow` port fdlibm as V8 carries it in `src/base/ieee754.cc`, function by function
//! and constant by constant; the C macros that read and write the two 32-bit halves of a double
//! become `to_bits` and `from_bits`. `hypot` and `hypot3` port `Math.hypot` from
//! `src/builtins/math.tq`. `round`, `max`, `min` and `sign` give JavaScript's answers for -0 and
//! NaN, which Rust's methods do not.
//!
//! JavaScript has a single NaN, so a NaN result here may carry any payload; compare it as NaN.
//! `tests/math.rs` checks the floating-point functions against `harness/math-fixture.mjs`'s fixture.
//! `tests/interface_math_intent.rs` checks JavaScript's wrapping 32-bit conversions and `imul`.

// The following is adapted from fdlibm (http://www.netlib.org/fdlibm).
//
// ====================================================
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//
// Developed at SunSoft, a Sun Microsystems, Inc. business.
// Permission to use, copy, modify, and distribute this
// software is freely granted, provided that this notice
// is preserved.
// ====================================================
//
// The original source code covered by the above license above has been
// modified significantly by Google Inc.
// Copyright 2016 the V8 project authors. All rights reserved.

// fdlibm writes its constants with more digits than a double holds, and writes pi/2, ln 2 and the
// like as decimal literals. Each literal still parses to the bits in the hex comment beside it, and
// keeping the text as V8 has it makes the port easy to check line by line.
#![allow(clippy::excessive_precision, clippy::approx_constant)]

// ---------- word access (the C macros EXTRACT_WORDS, GET_HIGH_WORD, SET_LOW_WORD, ...) ----------

/// The high 32 bits of `x`: sign, exponent and the top 20 bits of the mantissa. Signed, as
/// fdlibm's `hx`, so `hx < 0` tests the sign.
fn high_word(x: f64) -> i32 {
    (x.to_bits() >> 32) as i32
}

/// The low 32 bits of the mantissa.
fn low_word(x: f64) -> u32 {
    x.to_bits() as u32
}

/// The double with these two halves (INSERT_WORDS).
fn from_words(high: i32, low: u32) -> f64 {
    f64::from_bits((u64::from(high as u32) << 32) | u64::from(low))
}

/// `x` with its high half replaced (SET_HIGH_WORD).
fn with_high_word(x: f64, high: i32) -> f64 {
    f64::from_bits((x.to_bits() & 0xFFFF_FFFF) | (u64::from(high as u32) << 32))
}

/// `x` with its low half replaced (SET_LOW_WORD).
fn with_low_word(x: f64, low: u32) -> f64 {
    f64::from_bits((x.to_bits() & 0xFFFF_FFFF_0000_0000) | u64::from(low))
}

/// `x * 2^n`, correctly rounded, as C's `scalbn` (which fdlibm calls). This is musl's version: it
/// scales in steps so a subnormal result is rounded once.
fn scalbn(x: f64, mut n: i32) -> f64 {
    let x1p1023 = f64::from_bits(0x7FE0_0000_0000_0000); // 2^1023
    let x1p53 = f64::from_bits(0x4340_0000_0000_0000); // 2^53
    let x1p_1022 = f64::from_bits(0x0010_0000_0000_0000); // 2^-1022
    let mut y = x;
    if n > 1023 {
        y *= x1p1023;
        n -= 1023;
        if n > 1023 {
            y *= x1p1023;
            n -= 1023;
            if n > 1023 {
                n = 1023;
            }
        }
    } else if n < -1022 {
        // Keep the final n below -53 so the last multiply rounds only once.
        y *= x1p_1022 * x1p53;
        n += 1022 - 53;
        if n < -1022 {
            y *= x1p_1022 * x1p53;
            n += 1022 - 53;
            if n < -1022 {
                n = -1022;
            }
        }
    }
    y * f64::from_bits(((0x3FF + n) as u64) << 52)
}

// ---------- argument reduction and kernels ----------

/// `__ieee754_rem_pio2(x, y)`: the remainder of `x` rem pi/2 in `y[0] + y[1]`, and the quotient's
/// last bits as the return value. Uses `kernel_rem_pio2` for large arguments.
fn ieee754_rem_pio2(x: f64, y: &mut [f64; 2]) -> i32 {
    // Table of constants for 2/pi, 396 hex digits (476 decimal) of 2/pi.
    const TWO_OVER_PI: [i32; 66] = [
        0xA2F983, 0x6E4E44, 0x1529FC, 0x2757D1, 0xF534DD, 0xC0DB62, 0x95993C, //
        0x439041, 0xFE5163, 0xABDEBB, 0xC561B7, 0x246E3A, 0x424DD2, 0xE00649, //
        0x2EEA09, 0xD1921C, 0xFE1DEB, 0x1CB129, 0xA73EE8, 0x8235F5, 0x2EBB44, //
        0x84E99C, 0x7026B4, 0x5F7E41, 0x3991D6, 0x398353, 0x39F49C, 0x845F8B, //
        0xBDF928, 0x3B1FF8, 0x97FFDE, 0x05980F, 0xEF2F11, 0x8B5A0A, 0x6D1F6D, //
        0x367ECF, 0x27CB09, 0xB74F46, 0x3F669E, 0x5FEA2D, 0x7527BA, 0xC7EBE5, //
        0xF17B3D, 0x0739F7, 0x8A5292, 0xEA6BFB, 0x5FB11F, 0x8D5D08, 0x560330, //
        0x46FC7B, 0x6BABF0, 0xCFBC20, 0x9AF436, 0x1DA9E3, 0x91615E, 0xE61B08, //
        0x659985, 0x5F14A0, 0x68408D, 0xFFD880, 0x4D7327, 0x310606, 0x1556CA, //
        0x73A8C9, 0x60E27B, 0xC08C6B,
    ];

    const NPIO2_HW: [i32; 32] = [
        0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C, //
        0x4025FDBB, 0x402921FB, 0x402C463A, 0x402F6A7A, 0x4031475C, 0x4032D97C, //
        0x40346B9C, 0x4035FDBB, 0x40378FDB, 0x403921FB, 0x403AB41B, 0x403C463A, //
        0x403DD85A, 0x403F6A7A, 0x40407E4C, 0x4041475C, 0x4042106C, 0x4042D97C, //
        0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB, 0x4046C6CB, 0x40478FDB, //
        0x404858EB, 0x404921FB,
    ];

    // invpio2:  53 bits of 2/pi
    // pio2_1:   first  33 bit of pi/2
    // pio2_1t:  pi/2 - pio2_1
    // pio2_2:   second 33 bit of pi/2
    // pio2_2t:  pi/2 - (pio2_1+pio2_2)
    // pio2_3:   third  33 bit of pi/2
    // pio2_3t:  pi/2 - (pio2_1+pio2_2+pio2_3)
    const HALF: f64 = 5.00000000000000000000e-01; /* 0x3FE00000, 0x00000000 */
    const TWO24: f64 = 1.67772160000000000000e+07; /* 0x41700000, 0x00000000 */
    const INVPIO2: f64 = 6.36619772367581382433e-01; /* 0x3FE45F30, 0x6DC9C883 */
    const PIO2_1: f64 = 1.57079632673412561417e+00; /* 0x3FF921FB, 0x54400000 */
    const PIO2_1T: f64 = 6.07710050650619224932e-11; /* 0x3DD0B461, 0x1A626331 */
    const PIO2_2: f64 = 6.07710050630396597660e-11; /* 0x3DD0B461, 0x1A600000 */
    const PIO2_2T: f64 = 2.02226624879595063154e-21; /* 0x3BA3198A, 0x2E037073 */
    const PIO2_3: f64 = 2.02226624871116645580e-21; /* 0x3BA3198A, 0x2E000000 */
    const PIO2_3T: f64 = 8.47842766036889956997e-32; /* 0x397B839A, 0x252049C1 */

    let hx = high_word(x);
    let ix = hx & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        // |x| ~<= pi/4, no need for reduction
        y[0] = x;
        y[1] = 0.0;
        return 0;
    }
    if ix < 0x4002D97C {
        // |x| < 3pi/4, special case with n=+-1
        if hx > 0 {
            let mut z = x - PIO2_1;
            if ix != 0x3FF921FB {
                // 33+53 bit pi is good enough
                y[0] = z - PIO2_1T;
                y[1] = (z - y[0]) - PIO2_1T;
            } else {
                // near pi/2, use 33+33+53 bit pi
                z -= PIO2_2;
                y[0] = z - PIO2_2T;
                y[1] = (z - y[0]) - PIO2_2T;
            }
            return 1;
        } else {
            // negative x
            let mut z = x + PIO2_1;
            if ix != 0x3FF921FB {
                // 33+53 bit pi is good enough
                y[0] = z + PIO2_1T;
                y[1] = (z - y[0]) + PIO2_1T;
            } else {
                // near pi/2, use 33+33+53 bit pi
                z += PIO2_2;
                y[0] = z + PIO2_2T;
                y[1] = (z - y[0]) + PIO2_2T;
            }
            return -1;
        }
    }
    if ix <= 0x413921FB {
        // |x| ~<= 2^19*(pi/2), medium size
        let t = x.abs();
        let n = (t * INVPIO2 + HALF) as i32;
        let fnn = f64::from(n);
        let mut r = t - fnn * PIO2_1;
        let mut w = fnn * PIO2_1T; // 1st round good to 85 bit
        if n < 32 && ix != NPIO2_HW[(n - 1) as usize] {
            y[0] = r - w; // quick check no cancellation
        } else {
            let j = ix >> 20;
            y[0] = r - w;
            let mut i = j - ((high_word(y[0]) >> 20) & 0x7FF);
            if i > 16 {
                // 2nd iteration needed, good to 118
                let t = r;
                w = fnn * PIO2_2;
                r = t - w;
                w = fnn * PIO2_2T - ((t - r) - w);
                y[0] = r - w;
                i = j - ((high_word(y[0]) >> 20) & 0x7FF);
                if i > 49 {
                    // 3rd iteration need, 151 bits acc, will cover all possible cases
                    let t = r;
                    w = fnn * PIO2_3;
                    r = t - w;
                    w = fnn * PIO2_3T - ((t - r) - w);
                    y[0] = r - w;
                }
            }
        }
        y[1] = (r - y[0]) - w;
        if hx < 0 {
            y[0] = -y[0];
            y[1] = -y[1];
            return -n;
        }
        return n;
    }
    // all other (large) arguments
    if ix >= 0x7FF00000 {
        // x is inf or NaN (fdlibm writes x - x for the NaN)
        y[0] = f64::NAN;
        y[1] = f64::NAN;
        return 0;
    }
    // set z = scalbn(|x|,ilogb(x)-23)
    let e0 = (ix >> 20) - 1046; // e0 = ilogb(z)-23;
    let mut z = from_words(ix.wrapping_sub(((e0 as u32) << 20) as i32), low_word(x));
    let mut tx = [0.0; 3];
    for t in tx.iter_mut().take(2) {
        *t = f64::from(z as i32);
        z = (z - *t) * TWO24;
    }
    tx[2] = z;
    let mut nx = 3;
    while tx[nx - 1] == 0.0 {
        nx -= 1; // skip zero term
    }
    let n = kernel_rem_pio2(&tx[..nx], y, e0, &TWO_OVER_PI);
    if hx < 0 {
        y[0] = -y[0];
        y[1] = -y[1];
        return -n;
    }
    n
}

/// `__kernel_cos(x, y)`: cos on [-pi/4, pi/4], where `y` is the tail of `x`.
///
/// cos(x) is approximated by a polynomial of degree 14 on [0, pi/4] (remez error below 2^-58).
/// cos(x+y) = 1 - (x*x/2 - (r - x*y)), and for x > 0.3 the subtraction is split around qx = x/4
/// (or 0.28125 for x > 0.78125) so `1-qx` and `x*x/2-qx` are exact.
fn kernel_cos(x: f64, y: f64) -> f64 {
    const ONE: f64 = 1.00000000000000000000e+00; /* 0x3FF00000, 0x00000000 */
    const C1: f64 = 4.16666666666666019037e-02; /* 0x3FA55555, 0x5555554C */
    const C2: f64 = -1.38888888888741095749e-03; /* 0xBF56C16C, 0x16C15177 */
    const C3: f64 = 2.48015872894767294178e-05; /* 0x3EFA01A0, 0x19CB1590 */
    const C4: f64 = -2.75573143513906633035e-07; /* 0xBE927E4F, 0x809C52AD */
    const C5: f64 = 2.08757232129817482790e-09; /* 0x3E21EE9E, 0xBDB4B1C4 */
    const C6: f64 = -1.13596475577881948265e-11; /* 0xBDA8FAE9, 0xBE8838D4 */

    let ix = high_word(x) & 0x7FFFFFFF; // ix = |x|'s high word
    if ix < 0x3E400000 {
        // |x| < 2^-27. The C code tests `(int)x == 0`, always true here, to raise inexact.
        return ONE;
    }
    let z = x * x;
    let r = z * (C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6)))));
    if ix < 0x3FD33333 {
        // |x| < 0.3
        ONE - (0.5 * z - (z * r - x * y))
    } else {
        let qx = if ix > 0x3FE90000 {
            // x > 0.78125
            0.28125
        } else {
            from_words(ix - 0x00200000, 0) // x/4
        };
        let iz = 0.5 * z - qx;
        let a = ONE - qx;
        a - (iz - (z * r - x * y))
    }
}

/// `__kernel_rem_pio2(x, y, e0, nx, prec, ipio2)`: reduces a large argument by pi/2, returning the
/// last three bits of N with y = x - N*pi/2. It computes the integer (mod 8) and fraction parts of
/// (2/pi)*x without the full product: the skipped part is a multiple of 8.
///
/// `x` holds the positive input in 24-bit pieces (`x.len()` is fdlibm's `nx`), the first scaled by
/// `2^e0`. `ipio2` holds 2/pi in 24-bit chunks. V8 calls this only with `prec = 2` (64 bits), so
/// `jk = init_jk[2] = 4` here and only that case of the final switch is kept.
fn kernel_rem_pio2(x: &[f64], y: &mut [f64; 2], e0: i32, ipio2: &[i32]) -> i32 {
    // PIo2[] is pi/2 cut into 24-bit chunks.
    const PIO2: [f64; 8] = [
        1.57079625129699707031e+00, /* 0x3FF921FB, 0x40000000 */
        7.54978941586159635335e-08, /* 0x3E74442D, 0x00000000 */
        5.39030252995776476554e-15, /* 0x3CF84698, 0x80000000 */
        3.28200341580791294123e-22, /* 0x3B78CC51, 0x60000000 */
        1.27065575308067607349e-29, /* 0x39F01B83, 0x80000000 */
        1.22933308981111328932e-36, /* 0x387A2520, 0x40000000 */
        2.73370053816464559624e-44, /* 0x36E38222, 0x80000000 */
        2.16741683877804819444e-51, /* 0x3569F31D, 0x00000000 */
    ];
    const TWO24: f64 = 1.67772160000000000000e+07; /* 0x41700000, 0x00000000 */
    const TWON24: f64 = 5.96046447753906250000e-08; /* 0x3E700000, 0x00000000 */

    let mut iq = [0i32; 20];
    let mut f = [0.0f64; 20];
    let mut fq = [0.0f64; 20];
    let mut q = [0.0f64; 20];

    // initialize jk
    let jk: usize = 4;
    let jp = jk;

    // determine jx,jv,q0, note that 3>q0
    let jx = x.len() - 1;
    let jv = ((e0 - 3) / 24).max(0);
    let mut q0 = e0 - 24 * (jv + 1);
    let jv = jv as usize;

    // set up f[0] to f[jx+jk] where f[jx+jk] = ipio2[jv+jk]
    for (j, fi) in (jv as i32 - jx as i32..).zip(f.iter_mut().take(jx + jk + 1)) {
        *fi = if j < 0 {
            0.0
        } else {
            f64::from(ipio2[j as usize])
        };
    }

    // compute q[0],q[1],...q[jk]
    for i in 0..=jk {
        let mut fw = 0.0;
        for j in 0..=jx {
            fw += x[j] * f[jx + i - j];
        }
        q[i] = fw;
    }

    let mut jz = jk;
    let mut z;
    let mut n;
    let mut ih;
    // recompute:
    loop {
        // distill q[] into iq[] reversingly
        z = q[jz];
        let mut i = 0;
        let mut j = jz;
        while j > 0 {
            let fw = f64::from((TWON24 * z) as i32);
            iq[i] = (z - TWO24 * fw) as i32;
            z = q[j - 1] + fw;
            i += 1;
            j -= 1;
        }

        // compute n
        z = scalbn(z, q0); // actual value of z
        z -= 8.0 * (z * 0.125).floor(); // trim off integer >= 8
        n = z as i32;
        z -= f64::from(n);
        ih = 0;
        if q0 > 0 {
            // need iq[jz-1] to determine n
            let i = iq[jz - 1] >> (24 - q0);
            n += i;
            iq[jz - 1] -= i << (24 - q0);
            ih = iq[jz - 1] >> (23 - q0);
        } else if q0 == 0 {
            ih = iq[jz - 1] >> 23;
        } else if z >= 0.5 {
            ih = 2;
        }

        if ih > 0 {
            // q > 0.5
            n += 1;
            let mut carry = 0;
            for v in iq.iter_mut().take(jz) {
                // compute 1-q
                let j = *v;
                if carry == 0 {
                    if j != 0 {
                        carry = 1;
                        *v = 0x1000000 - j;
                    }
                } else {
                    *v = 0xFFFFFF - j;
                }
            }
            // rare case: chance is 1 in 12
            match q0 {
                1 => iq[jz - 1] &= 0x7FFFFF,
                2 => iq[jz - 1] &= 0x3FFFFF,
                _ => {}
            }
            if ih == 2 {
                z = 1.0 - z;
                if carry != 0 {
                    z -= scalbn(1.0, q0);
                }
            }
        }

        // check if recomputation is needed
        if z == 0.0 {
            let j = iq[jk..jz].iter().fold(0, |j, &v| j | v);
            if j == 0 {
                // need recomputation; k = no. of terms needed
                let mut k = 1;
                while jk >= k && iq[jk - k] == 0 {
                    k += 1;
                }
                for i in jz + 1..=jz + k {
                    // add q[jz+1] to q[jz+k]
                    f[jx + i] = f64::from(ipio2[jv + i]);
                    let mut fw = 0.0;
                    for j in 0..=jx {
                        fw += x[j] * f[jx + i - j];
                    }
                    q[i] = fw;
                }
                jz += k;
                continue;
            }
        }
        break;
    }

    // chop off zero terms
    if z == 0.0 {
        jz -= 1;
        q0 -= 24;
        while iq[jz] == 0 {
            jz -= 1;
            q0 -= 24;
        }
    } else {
        // break z into 24-bit if necessary
        z = scalbn(z, -q0);
        if z >= TWO24 {
            let fw = f64::from((TWON24 * z) as i32);
            iq[jz] = (z - TWO24 * fw) as i32;
            jz += 1;
            q0 += 24;
            iq[jz] = fw as i32;
        } else {
            iq[jz] = z as i32;
        }
    }

    // convert integer "bit" chunk to floating-point value
    let mut fw = scalbn(1.0, q0);
    for i in (0..=jz).rev() {
        q[i] = fw * f64::from(iq[i]);
        fw *= TWON24;
    }

    // compute PIo2[0,...,jp]*q[jz,...,0]
    for i in (0..=jz).rev() {
        let mut fw = 0.0;
        let mut k = 0;
        while k <= jp && k <= jz - i {
            fw += PIO2[k] * q[i + k];
            k += 1;
        }
        fq[jz - i] = fw;
    }

    // compress fq[] into y[]
    let mut fw = 0.0;
    for i in (0..=jz).rev() {
        fw += fq[i];
    }
    y[0] = if ih == 0 { fw } else { -fw };
    fw = fq[0] - fw;
    for v in &fq[1..=jz] {
        fw += v;
    }
    y[1] = if ih == 0 { fw } else { -fw };
    n & 7
}

/// `__kernel_sin(x, y, iy)`: sin on [-pi/4, pi/4], where `y` is the tail of `x` and `iy == 0`
/// says the tail is zero.
///
/// sin(x) is approximated by a polynomial of degree 13 on [0, pi/4] (remez error below 2^-58),
/// and sin(x+y) = x + (S1*x^3 + (x^2*(r-y/2)+y)) with r = x^5*(S2+x^2*(S3+...)).
fn kernel_sin(x: f64, y: f64, iy: i32) -> f64 {
    const HALF: f64 = 5.00000000000000000000e-01; /* 0x3FE00000, 0x00000000 */
    const S1: f64 = -1.66666666666666324348e-01; /* 0xBFC55555, 0x55555549 */
    const S2: f64 = 8.33333333332248946124e-03; /* 0x3F811111, 0x1110F8A6 */
    const S3: f64 = -1.98412698298579493134e-04; /* 0xBF2A01A0, 0x19C161D5 */
    const S4: f64 = 2.75573137070700676789e-06; /* 0x3EC71DE3, 0x57B1FE7D */
    const S5: f64 = -2.50507602534068634195e-08; /* 0xBE5AE5E6, 0x8A2B9CEB */
    const S6: f64 = 1.58969099521155010221e-10; /* 0x3DE5D93A, 0x5ACFD57C */

    let ix = high_word(x) & 0x7FFFFFFF; // high word of x
    if ix < 0x3E400000 {
        // |x| < 2^-27. The C code tests `(int)x == 0`, always true here, to raise inexact.
        return x;
    }
    let z = x * x;
    let v = z * x;
    let r = S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)));
    if iy == 0 {
        x + v * (S1 + z * r)
    } else {
        x - ((z * (HALF * y - v * r) - y) - v * S1)
    }
}

/// `__kernel_tan(x, y, k)`: tan on [-pi/4, pi/4], where `y` is the tail of `x`. Returns tan when
/// `iy == 1` and -1/tan when `iy == -1`.
///
/// tan(x) is approximated by an odd polynomial of degree 27 on [0, 0.67434] (error below
/// 2^-59.2). For x in [0.67434, pi/4] it uses tan(x) = tan(pi/4 - y) = (1-tan(y))/(1+tan(y)).
fn kernel_tan(mut x: f64, mut y: f64, iy: i32) -> f64 {
    const T: [f64; 13] = [
        3.33333333333334091986e-01,  /* 3FD55555, 55555563 */
        1.33333333333201242699e-01,  /* 3FC11111, 1110FE7A */
        5.39682539762260521377e-02,  /* 3FABA1BA, 1BB341FE */
        2.18694882948595424599e-02,  /* 3F9664F4, 8406D637 */
        8.86323982359930005737e-03,  /* 3F8226E3, E96E8493 */
        3.59207910759131235356e-03,  /* 3F6D6D22, C9560328 */
        1.45620945432529025516e-03,  /* 3F57DBC8, FEE08315 */
        5.88041240820264096874e-04,  /* 3F4344D8, F2F26501 */
        2.46463134818469906812e-04,  /* 3F3026F7, 1A8D1068 */
        7.81794442939557092300e-05,  /* 3F147E88, A03792A6 */
        7.14072491382608190305e-05,  /* 3F12B80F, 32F0A7E9 */
        -1.85586374855275456654e-05, /* BEF375CB, DB605373 */
        2.59073051863633712884e-05,  /* 3EFB2A70, 74BF7AD4 */
    ];
    const ONE: f64 = 1.00000000000000000000e+00; /* 3FF00000, 00000000 */
    const PIO4: f64 = 7.85398163397448278999e-01; /* 3FE921FB, 54442D18 */
    const PIO4LO: f64 = 3.06161699786838301793e-17; /* 3C81A626, 33145C07 */

    let hx = high_word(x); // high word of x
    let ix = hx & 0x7FFFFFFF; // high word of |x|
    if ix < 0x3E300000 {
        // |x| < 2^-28. The C code tests `(int)x == 0`, always true here, to raise inexact.
        let low = low_word(x);
        if ((ix as u32 | low) | (iy + 1) as u32) == 0 {
            return ONE / x.abs();
        } else if iy == 1 {
            return x;
        } else {
            // compute -1 / (x+y) carefully
            let w = x + y;
            let z = with_low_word(w, 0);
            let v = y - (z - x);
            let a = -ONE / w;
            let t = with_low_word(a, 0);
            let s = ONE + t * z;
            return t + a * (s + t * v);
        }
    }
    if ix >= 0x3FE59428 {
        // |x| >= 0.6744
        if hx < 0 {
            x = -x;
            y = -y;
        }
        let z = PIO4 - x;
        let w = PIO4LO - y;
        x = z + w;
        y = 0.0;
    }
    let z = x * x;
    let w = z * z;
    // Break x^5*(T[1]+x^2*T[2]+...) into
    // x^5(T[1]+x^4*T[3]+...+x^20*T[11]) +
    // x^5(x^2*(T[2]+x^4*T[4]+...+x^22*[T12]))
    let mut r = T[1] + w * (T[3] + w * (T[5] + w * (T[7] + w * (T[9] + w * T[11]))));
    let v = z * (T[2] + w * (T[4] + w * (T[6] + w * (T[8] + w * (T[10] + w * T[12])))));
    let s = z * x;
    r = y + z * (s * (r + v) + y);
    r += T[0] * s;
    let w = x + r;
    if ix >= 0x3FE59428 {
        let v = f64::from(iy);
        return f64::from(1 - ((hx >> 30) & 2)) * (v - 2.0 * (x - (w * w / (w + v) - r)));
    }
    if iy == 1 {
        w
    } else {
        // if allow error up to 2 ulp, simply return -1.0 / (x+r) here
        // compute -1.0 / (x+r) accurately
        let z = with_low_word(w, 0);
        let v = r - (z - x); // z+v = r+x
        let a = -1.0 / w; // a = -1.0/w
        let t = with_low_word(a, 0);
        let s = 1.0 + t * z;
        t + a * (s + t * v)
    }
}

// ---------- the functions ----------

/// `Math.sin`. Reduces the argument to [-pi/4, pi/4] and picks the kernel by quadrant:
/// n = 0: S, 1: C, 2: -S, 3: -C.
pub fn sin(x: f64) -> f64 {
    let mut y = [0.0; 2];
    let ix = high_word(x) & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        // |x| ~< pi/4
        kernel_sin(x, 0.0, 0)
    } else if ix >= 0x7FF00000 {
        // sin(Inf or NaN) is NaN (fdlibm writes x - x)
        f64::NAN
    } else {
        // argument reduction needed
        let n = ieee754_rem_pio2(x, &mut y);
        match n & 3 {
            0 => kernel_sin(y[0], y[1], 1),
            1 => kernel_cos(y[0], y[1]),
            2 => -kernel_sin(y[0], y[1], 1),
            _ => -kernel_cos(y[0], y[1]),
        }
    }
}

/// `Math.cos`. Quadrants: n = 0: C, 1: -S, 2: -C, 3: S.
pub fn cos(x: f64) -> f64 {
    let mut y = [0.0; 2];
    let ix = high_word(x) & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        // |x| ~< pi/4
        kernel_cos(x, 0.0)
    } else if ix >= 0x7FF00000 {
        // cos(Inf or NaN) is NaN (fdlibm writes x - x)
        f64::NAN
    } else {
        // argument reduction needed
        let n = ieee754_rem_pio2(x, &mut y);
        match n & 3 {
            0 => kernel_cos(y[0], y[1]),
            1 => -kernel_sin(y[0], y[1], 1),
            2 => -kernel_cos(y[0], y[1]),
            _ => kernel_sin(y[0], y[1], 1),
        }
    }
}

/// `Math.tan`. Quadrants: n even: T, n odd: -1/T.
pub fn tan(x: f64) -> f64 {
    let mut y = [0.0; 2];
    let ix = high_word(x) & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        // |x| ~< pi/4
        kernel_tan(x, 0.0, 1)
    } else if ix >= 0x7FF00000 {
        // tan(Inf or NaN) is NaN (fdlibm writes x - x)
        f64::NAN
    } else {
        // argument reduction needed
        let n = ieee754_rem_pio2(x, &mut y);
        // 1 -> n even, -1 -> n odd
        kernel_tan(y[0], y[1], 1 - ((n & 1) << 1))
    }
}

/// `Math.atan`. Reduces |x| to one of five intervals by k = 4t+0.25:
/// [0,7/16] by the polynomial, [7/16,11/16] by atan(1/2) + atan((t-0.5)/(1+t/2)),
/// [11/16,19/16] by atan(1) + atan((t-1)/(1+t)), [19/16,39/16] by atan(3/2) +
/// atan((t-1.5)/(1+1.5t)), and beyond by atan(inf) + atan(-1/t).
pub fn atan(x: f64) -> f64 {
    const ATANHI: [f64; 4] = [
        4.63647609000806093515e-01, /* atan(0.5)hi 0x3FDDAC67, 0x0561BB4F */
        7.85398163397448278999e-01, /* atan(1.0)hi 0x3FE921FB, 0x54442D18 */
        9.82793723247329054082e-01, /* atan(1.5)hi 0x3FEF730B, 0xD281F69B */
        1.57079632679489655800e+00, /* atan(inf)hi 0x3FF921FB, 0x54442D18 */
    ];
    const ATANLO: [f64; 4] = [
        2.26987774529616870924e-17, /* atan(0.5)lo 0x3C7A2B7F, 0x222F65E2 */
        3.06161699786838301793e-17, /* atan(1.0)lo 0x3C81A626, 0x33145C07 */
        1.39033110312309984516e-17, /* atan(1.5)lo 0x3C700788, 0x7AF0CBBD */
        6.12323399573676603587e-17, /* atan(inf)lo 0x3C91A626, 0x33145C07 */
    ];
    const AT: [f64; 11] = [
        3.33333333333329318027e-01,  /* 0x3FD55555, 0x5555550D */
        -1.99999999998764832476e-01, /* 0xBFC99999, 0x9998EBC4 */
        1.42857142725034663711e-01,  /* 0x3FC24924, 0x920083FF */
        -1.11111104054623557880e-01, /* 0xBFBC71C6, 0xFE231671 */
        9.09088713343650656196e-02,  /* 0x3FB745CD, 0xC54C206E */
        -7.69187620504482999495e-02, /* 0xBFB3B0F2, 0xAF749A6D */
        6.66107313738753120669e-02,  /* 0x3FB10D66, 0xA0D03D51 */
        -5.83357013379057348645e-02, /* 0xBFADDE2D, 0x52DEFD9A */
        4.97687799461593236017e-02,  /* 0x3FA97B4B, 0x24760DEB */
        -3.65315727442169155270e-02, /* 0xBFA2B444, 0x2C6A6C2F */
        1.62858201153657823623e-02,  /* 0x3F90AD3A, 0xE322DA11 */
    ];
    const ONE: f64 = 1.0;

    let mut x = x;
    let hx = high_word(x);
    let ix = hx & 0x7FFFFFFF;
    if ix >= 0x44100000 {
        // if |x| >= 2^66
        let low = low_word(x);
        if ix > 0x7FF00000 || (ix == 0x7FF00000 && low != 0) {
            return x + x; // NaN
        }
        return if hx > 0 {
            ATANHI[3] + ATANLO[3]
        } else {
            -ATANHI[3] - ATANLO[3]
        };
    }
    // fdlibm's id: which of atanhi/atanlo the reduction used, -1 (None here) for none.
    let id: Option<usize>;
    if ix < 0x3FDC0000 {
        // |x| < 0.4375
        if ix < 0x3E400000 {
            // |x| < 2^-27. The C code tests `huge + x > one`, always true here, to raise inexact.
            return x;
        }
        id = None;
    } else {
        x = x.abs();
        if ix < 0x3FF30000 {
            // |x| < 1.1875
            if ix < 0x3FE60000 {
                // 7/16 <=|x|<11/16
                id = Some(0);
                x = (2.0 * x - ONE) / (2.0 + x);
            } else {
                // 11/16<=|x|< 19/16
                id = Some(1);
                x = (x - ONE) / (x + ONE);
            }
        } else if ix < 0x40038000 {
            // |x| < 2.4375
            id = Some(2);
            x = (x - 1.5) / (ONE + 1.5 * x);
        } else {
            // 2.4375 <= |x| < 2^66
            id = Some(3);
            x = -1.0 / x;
        }
    }
    // end of argument reduction
    let z = x * x;
    let w = z * z;
    // break sum from i=0 to 10 aT[i]z**(i+1) into odd and even poly
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));
    match id {
        None => x - x * (s1 + s2),
        Some(id) => {
            let z = ATANHI[id] - ((x * (s1 + s2) - ATANLO[id]) - x);
            if hx < 0 { -z } else { z }
        }
    }
}

/// `Math.atan2(y, x)`. Reduces y to positive by atan2(y,x) = -atan2(-y,x), and x to positive by
/// ARG(x+iy) = pi - arctan(y/(-x)) for x < 0. The zero, infinite and NaN cases follow C99 Annex F,
/// which `Math.atan2` also specifies.
pub fn atan2(y: f64, x: f64) -> f64 {
    const TINY: f64 = 1.0e-300;
    const PI_O_4: f64 = 7.8539816339744827900E-01; /* 0x3FE921FB, 0x54442D18 */
    const PI_O_2: f64 = 1.5707963267948965580E+00; /* 0x3FF921FB, 0x54442D18 */
    const PI: f64 = 3.1415926535897931160E+00; /* 0x400921FB, 0x54442D18 */
    const PI_LO: f64 = 1.2246467991473531772E-16; /* 0x3CA1A626, 0x33145C07 */

    let hx = high_word(x);
    let lx = low_word(x);
    let ix = hx & 0x7FFFFFFF;
    let hy = high_word(y);
    let ly = low_word(y);
    let iy = hy & 0x7FFFFFFF;
    // (lx | -lx) >> 31 is 1 when the low word is nonzero, lifting an Inf exponent above 0x7FF00000.
    if (ix as u32 | ((lx | lx.wrapping_neg()) >> 31)) > 0x7FF00000
        || (iy as u32 | ((ly | ly.wrapping_neg()) >> 31)) > 0x7FF00000
    {
        return x + y; // x or y is NaN
    }
    if (hx.wrapping_sub(0x3FF00000) as u32 | lx) == 0 {
        return atan(y); // x=1.0
    }
    let mut m = ((hy >> 31) & 1) | ((hx >> 30) & 2); // 2*sign(x)+sign(y)

    // when y = 0
    if (iy as u32 | ly) == 0 {
        return match m {
            0 | 1 => y,      // atan(+-0,+anything)=+-0
            2 => PI + TINY,  // atan(+0,-anything) = pi
            _ => -PI - TINY, // atan(-0,-anything) =-pi
        };
    }
    // when x = 0
    if (ix as u32 | lx) == 0 {
        return if hy < 0 {
            -PI_O_2 - TINY
        } else {
            PI_O_2 + TINY
        };
    }

    // when x is INF
    if ix == 0x7FF00000 {
        if iy == 0x7FF00000 {
            return match m {
                0 => PI_O_4 + TINY,        // atan(+INF,+INF)
                1 => -PI_O_4 - TINY,       // atan(-INF,+INF)
                2 => 3.0 * PI_O_4 + TINY,  // atan(+INF,-INF)
                _ => -3.0 * PI_O_4 - TINY, // atan(-INF,-INF)
            };
        } else {
            return match m {
                0 => 0.0,        // atan(+...,+INF)
                1 => -0.0,       // atan(-...,+INF)
                2 => PI + TINY,  // atan(+...,-INF)
                _ => -PI - TINY, // atan(-...,-INF)
            };
        }
    }
    // when y is INF
    if iy == 0x7FF00000 {
        return if hy < 0 {
            -PI_O_2 - TINY
        } else {
            PI_O_2 + TINY
        };
    }

    // compute y/x
    let k = (iy - ix) >> 20;
    let z = if k > 60 {
        // |y/x| >  2**60
        m &= 1;
        PI_O_2 + 0.5 * PI_LO
    } else if hx < 0 && k < -60 {
        0.0 // 0 > |y|/x > -2**-60
    } else {
        atan((y / x).abs()) // safe to do y/x
    };
    match m {
        0 => z,                // atan(+,+)
        1 => -z,               // atan(-,+)
        2 => PI - (z - PI_LO), // atan(+,-)
        _ => (z - PI_LO) - PI, // atan(-,-)
    }
}

/// `Math.exp`. Reduces x = k*ln2 + r with |r| <= 0.5*ln2, approximates exp(r) with a rational
/// function of r^2 (Remes error below 2^-59), then scales by 2^k. Error below 1 ulp.
pub fn exp(x: f64) -> f64 {
    const ONE: f64 = 1.0;
    const HALF: [f64; 2] = [0.5, -0.5];
    const O_THRESHOLD: f64 = 7.09782712893383973096e+02; /* 0x40862E42, 0xFEFA39EF */
    const U_THRESHOLD: f64 = -7.45133219101941108420e+02; /* 0xC0874910, 0xD52D3051 */
    const LN2HI: [f64; 2] = [
        6.93147180369123816490e-01,  /* 0x3FE62E42, 0xFEE00000 */
        -6.93147180369123816490e-01, /* 0xBFE62E42, 0xFEE00000 */
    ];
    const LN2LO: [f64; 2] = [
        1.90821492927058770002e-10,  /* 0x3DEA39EF, 0x35793C76 */
        -1.90821492927058770002e-10, /* 0xBDEA39EF, 0x35793C76 */
    ];
    const INVLN2: f64 = 1.44269504088896338700e+00; /* 0x3FF71547, 0x652B82FE */
    const P1: f64 = 1.66666666666666019037e-01; /* 0x3FC55555, 0x5555553E */
    const P2: f64 = -2.77777777770155933842e-03; /* 0xBF66C16C, 0x16BEBD93 */
    const P3: f64 = 6.61375632143793436117e-05; /* 0x3F11566A, 0xAF25DE2C */
    const P4: f64 = -1.65339022054652515390e-06; /* 0xBEBBBD41, 0xC5D26BF1 */
    const P5: f64 = 4.13813679705723846039e-08; /* 0x3E663769, 0x72BEA4D0 */
    const E: f64 = 2.718281828459045; /* 0x4005BF0A, 0x8B145769 */
    const HUGE: f64 = 1.0e+300;
    const TWOM1000: f64 = 9.33263618503218878990e-302; /* 2**-1000=0x01700000,0*/
    const TWO1023: f64 = 8.988465674311579539e307; /* 0x1p1023 */

    let mut x = x;
    let mut hx = high_word(x) as u32;
    let xsb = ((hx >> 31) & 1) as usize; // sign bit of x
    hx &= 0x7FFFFFFF; // high word of |x|

    // filter out non-finite argument
    if hx >= 0x40862E42 {
        // if |x|>=709.78...
        if hx >= 0x7FF00000 {
            let lx = low_word(x);
            if ((hx & 0xFFFFF) | lx) != 0 {
                return x + x; // NaN
            }
            return if xsb == 0 { x } else { 0.0 }; // exp(+-inf)={inf,0}
        }
        if x > O_THRESHOLD {
            return HUGE * HUGE; // overflow
        }
        if x < U_THRESHOLD {
            return TWOM1000 * TWOM1000; // underflow
        }
    }

    // argument reduction
    let (hi, lo, k) = if hx > 0x3FD62E42 {
        // if  |x| > 0.5 ln2
        let (hi, lo, k) = if hx < 0x3FF0A2B2 {
            // and |x| < 1.5 ln2
            // V8 special-cases exp(1) to return E: the computation below gets its last bit wrong.
            if x == 1.0 {
                return E;
            }
            (x - LN2HI[xsb], LN2LO[xsb], 1 - xsb as i32 - xsb as i32)
        } else {
            let k = (INVLN2 * x + HALF[xsb]) as i32;
            let t = f64::from(k);
            (x - t * LN2HI[0], t * LN2LO[0], k) // t*ln2HI is exact here
        };
        x = hi - lo;
        (hi, lo, k)
    } else if hx < 0x3E300000 {
        // when |x|<2**-28. The C code tests `huge + x > one`, always true here, to raise inexact.
        return ONE + x;
    } else {
        (0.0, 0.0, 0)
    };

    // x is now in primary range
    let t = x * x;
    let twopk = if k >= -1021 {
        from_words(0x3FF00000_i32.wrapping_add(((k as u32) << 20) as i32), 0)
    } else {
        from_words(
            0x3FF00000_u32.wrapping_add(((k + 1000) as u32) << 20) as i32,
            0,
        )
    };
    let c = x - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    if k == 0 {
        return ONE - ((x * c) / (c - 2.0) - x);
    }
    let y = ONE - ((lo - (x * c) / (2.0 - c)) - hi);
    if k >= -1021 {
        if k == 1024 {
            return y * 2.0 * TWO1023;
        }
        y * twopk
    } else {
        y * twopk * TWOM1000
    }
}

/// `Math.log`. Reduces x = 2^k * (1+f) with sqrt(2)/2 < 1+f < sqrt(2), approximates log(1+f) by
/// s = f/(2+f) and a degree-14 polynomial (Remes error below 2^-58.45), and adds k*ln2 in two
/// parts so the sum stays exact. Error below 1 ulp.
pub fn log(x: f64) -> f64 {
    const LN2_HI: f64 = 6.93147180369123816490e-01; /* 3fe62e42 fee00000 */
    const LN2_LO: f64 = 1.90821492927058770002e-10; /* 3dea39ef 35793c76 */
    const TWO54: f64 = 1.80143985094819840000e+16; /* 43500000 00000000 */
    const LG1: f64 = 6.666666666666735130e-01; /* 3FE55555 55555593 */
    const LG2: f64 = 3.999999999940941908e-01; /* 3FD99999 9997FA04 */
    const LG3: f64 = 2.857142874366239149e-01; /* 3FD24924 94229359 */
    const LG4: f64 = 2.222219843214978396e-01; /* 3FCC71C5 1D8E78AF */
    const LG5: f64 = 1.818357216161805012e-01; /* 3FC74664 96CB03DE */
    const LG6: f64 = 1.531383769920937332e-01; /* 3FC39A09 D078C69F */
    const LG7: f64 = 1.479819860511658591e-01; /* 3FC2F112 DF3E5244 */

    let mut x = x;
    let mut hx = high_word(x);
    let lx = low_word(x);

    let mut k = 0;
    if hx < 0x00100000 {
        // x < 2**-1022
        if ((hx & 0x7FFFFFFF) as u32 | lx) == 0 {
            return f64::NEG_INFINITY; // log(+-0)=-inf
        }
        if hx < 0 {
            return f64::NAN; // log(-#) = NaN (V8 returns a signaling NaN; JavaScript sees NaN)
        }
        k -= 54;
        x *= TWO54; // subnormal number, scale up x
        hx = high_word(x);
    }
    if hx >= 0x7FF00000 {
        return x + x;
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000FFFFF;
    let mut i = (hx + 0x95F64) & 0x100000;
    x = with_high_word(x, hx | (i ^ 0x3FF00000)); // normalize x or x/2
    k += i >> 20;
    let f = x - 1.0;
    if (0x000FFFFF & (2 + hx)) < 3 {
        // -2**-20 <= f < 2**-20
        if f == 0.0 {
            if k == 0 {
                return 0.0;
            }
            let dk = f64::from(k);
            return dk * LN2_HI + dk * LN2_LO;
        }
        let r = f * f * (0.5 - 0.33333333333333333 * f);
        if k == 0 {
            return f - r;
        }
        let dk = f64::from(k);
        return dk * LN2_HI - ((r - dk * LN2_LO) - f);
    }
    let s = f / (2.0 + f);
    let dk = f64::from(k);
    let z = s * s;
    i = hx - 0x6147A;
    let w = z * z;
    let j = 0x6B851 - hx;
    let t1 = w * (LG2 + w * (LG4 + w * LG6));
    let t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    i |= j;
    let r = t2 + t1;
    if i > 0 {
        let hfsq = 0.5 * f * f;
        if k == 0 {
            f - (hfsq - s * (hfsq + r))
        } else {
            dk * LN2_HI - ((hfsq - (s * (hfsq + r) + dk * LN2_LO)) - f)
        }
    } else if k == 0 {
        f - s * (f - r)
    } else {
        dk * LN2_HI - ((s * (f - r) - dk * LN2_LO) - f)
    }
}

/// `Math.pow` and `**`, as V8's `math::pow` (`src/numbers/ieee754.cc`) computes them: the
/// JavaScript special cases first, then V8's own fdlibm pow.
///
/// Node's default build (flag `use_std_math_pow`) hands the last step to the platform's
/// `std::pow` instead, so its last bits depend on the OS: on macOS, about one game-range result
/// in ten differs from this function in the last bit. The core keeps V8's pow, which is the same
/// everywhere and is what x64 Node computes with `--no-use-std-math-pow`.
pub fn pow(x: f64, y: f64) -> f64 {
    if y.is_nan() {
        // 1. If exponent is NaN, return NaN.
        return f64::NAN;
    }
    if y.is_infinite() && (x == 1.0 || x == -1.0) {
        // 9b and 10b: abs(base) = 1 with an infinite exponent is NaN.
        return f64::NAN;
    }
    // V8's optimizing compilers turn x ** 2 into x * x and x ** 0.5 into sqrt, so math::pow does too.
    if y == 2.0 {
        return x * x;
    } else if y == 0.5 {
        if x.is_infinite() {
            return f64::INFINITY;
        }
        // The + 0 makes (-0) ** 0.5 give +0 rather than -0.
        return (x + 0.0).sqrt();
    }
    legacy_pow(x, y)
}

/// `legacy::pow(x, y)`: computes log2(x) in two pieces w1 + w2 (w1 with 29 trailing zero bits),
/// forms y*log2(x) = n + y' in simulated extra precision with |y'| <= 0.5, and returns
/// 2^n * exp(y'*ln2). The special cases follow ES2019 12.6.4; pow(integer, integer) is exact
/// whenever the result is representable.
fn legacy_pow(x: f64, y: f64) -> f64 {
    const BP: [f64; 2] = [1.0, 1.5];
    const DP_H: [f64; 2] = [0.0, 5.84962487220764160156e-01]; // 0x3FE2B803, 0x40000000
    const DP_L: [f64; 2] = [0.0, 1.35003920212974897128e-08]; // 0x3E4CFDEB, 0x43CFD006
    const ZERO: f64 = 0.0;
    const ONE: f64 = 1.0;
    const TWO: f64 = 2.0;
    const TWO53: f64 = 9007199254740992.0; // 0x43400000, 0x00000000
    const HUGE: f64 = 1.0e300;
    const TINY: f64 = 1.0e-300;
    // poly coefs for (3/2)*(log(x)-2s-2/3*s**3
    const L1: f64 = 5.99999999999994648725e-01; // 0x3FE33333, 0x33333303
    const L2: f64 = 4.28571428578550184252e-01; // 0x3FDB6DB6, 0xDB6FABFF
    const L3: f64 = 3.33333329818377432918e-01; // 0x3FD55555, 0x518F264D
    const L4: f64 = 2.72728123808534006489e-01; // 0x3FD17460, 0xA91D4101
    const L5: f64 = 2.30660745775561754067e-01; // 0x3FCD864A, 0x93C9DB65
    const L6: f64 = 2.06975017800338417784e-01; // 0x3FCA7E28, 0x4A454EEF
    const P1: f64 = 1.66666666666666019037e-01; // 0x3FC55555, 0x5555553E
    const P2: f64 = -2.77777777770155933842e-03; // 0xBF66C16C, 0x16BEBD93
    const P3: f64 = 6.61375632143793436117e-05; // 0x3F11566A, 0xAF25DE2C
    const P4: f64 = -1.65339022054652515390e-06; // 0xBEBBBD41, 0xC5D26BF1
    const P5: f64 = 4.13813679705723846039e-08; // 0x3E663769, 0x72BEA4D0
    const LG2: f64 = 6.93147180559945286227e-01; // 0x3FE62E42, 0xFEFA39EF
    const LG2_H: f64 = 6.93147182464599609375e-01; // 0x3FE62E43, 0x00000000
    const LG2_L: f64 = -1.90465429995776804525e-09; // 0xBE205C61, 0x0CA86C39
    const OVT: f64 = 8.0085662595372944372e-0017; // -(1024-log2(ovfl+.5ulp))
    const CP: f64 = 9.61796693925975554329e-01; // 0x3FEEC709, 0xDC3A03FD =2/(3ln2)
    const CP_H: f64 = 9.61796700954437255859e-01; // 0x3FEEC709, 0xE0000000 =(float)cp
    const CP_L: f64 = -7.02846165095275826516e-09; // 0xBE3E2FE0, 0x145B01F5 =tail cp_h
    const IVLN2: f64 = 1.44269504088896338700e+00; // 0x3FF71547, 0x652B82FE =1/ln2
    const IVLN2_H: f64 = 1.44269502162933349609e+00; // 0x3FF71547, 0x60000000 =24b 1/ln2
    const IVLN2_L: f64 = 1.92596299112661746887e-08; // 0x3E54AE0B, 0xF85DDF44 =1/ln2 tail

    let hx = high_word(x);
    let lx = low_word(x);
    let hy = high_word(y);
    let ly = low_word(y);
    let mut ix = hx & 0x7fffffff;
    let iy = hy & 0x7fffffff;

    // y==zero: x**0 = 1
    if (iy as u32 | ly) == 0 {
        return ONE;
    }

    // +-NaN return x+y
    if ix > 0x7ff00000
        || (ix == 0x7ff00000 && lx != 0)
        || iy > 0x7ff00000
        || (iy == 0x7ff00000 && ly != 0)
    {
        return x + y;
    }

    // determine if y is an odd int when x < 0
    // yisint = 0 ... y is not an integer
    // yisint = 1 ... y is an odd int
    // yisint = 2 ... y is an even int
    let mut yisint = 0;
    if hx < 0 {
        if iy >= 0x43400000 {
            yisint = 2; // even integer y
        } else if iy >= 0x3ff00000 {
            let k = (iy >> 20) - 0x3ff; // exponent
            if k > 20 {
                let j = ly >> (52 - k);
                if (j << (52 - k)) == ly {
                    yisint = 2 - (j & 1) as i32;
                }
            } else if ly == 0 {
                let j = iy >> (20 - k);
                if (j << (20 - k)) == iy {
                    yisint = 2 - (j & 1);
                }
            }
        }
    }

    // special value of y
    if ly == 0 {
        if iy == 0x7ff00000 {
            // y is +-inf
            if ((ix - 0x3ff00000) as u32 | lx) == 0 {
                return f64::NAN; // inf**+-1 is NaN (fdlibm writes y - y)
            } else if ix >= 0x3ff00000 {
                // (|x|>1)**+-inf = inf,0
                return if hy >= 0 { y } else { ZERO };
            } else {
                // (|x|<1)**-,+inf = inf,0
                return if hy < 0 { -y } else { ZERO };
            }
        }
        if iy == 0x3ff00000 {
            // y is  +-1
            return if hy < 0 { ONE / x } else { x };
        }
        if hy == 0x40000000 {
            return x * x; // y is  2
        }
        if hy == 0x3fe00000 && hx >= 0 {
            // y is  0.5, x >= +0
            return x.sqrt();
        }
    }

    let mut ax = x.abs();
    // special value of x
    if lx == 0 && (ix == 0x7ff00000 || ix == 0 || ix == 0x3ff00000) {
        let mut z = ax; // x is +-0,+-inf,+-1
        if hy < 0 {
            z = ONE / z; // z = (1/|x|)
        }
        if hx < 0 {
            if ((ix - 0x3ff00000) | yisint) == 0 {
                z = f64::NAN; // (-1)**non-int is NaN
            } else if yisint == 1 {
                z = -z; // (x<0)**odd = -(|x|**odd)
            }
        }
        return z;
    }

    let mut n = (hx >> 31) + 1;

    // (x<0)**(non-int) is NaN
    if (n | yisint) == 0 {
        return f64::NAN;
    }

    let mut s = ONE; // s (sign of result -ve**odd) = -1 else = 1
    if (n | (yisint - 1)) == 0 {
        s = -ONE; // (-ve)**(odd int)
    }

    // log2(|x|) as t1 + t2, where t1 has a zero low word
    let (t1, t2) = if iy > 0x41e00000 {
        // |y| is huge
        // if |y| > 2**31
        if iy > 0x43f00000 {
            // if |y| > 2**64, must o/uflow
            if ix <= 0x3fefffff {
                return if hy < 0 { HUGE * HUGE } else { TINY * TINY };
            }
            if ix >= 0x3ff00000 {
                return if hy > 0 { HUGE * HUGE } else { TINY * TINY };
            }
        }
        // over/underflow if x is not close to one
        if ix < 0x3fefffff {
            return if hy < 0 {
                s * HUGE * HUGE
            } else {
                s * TINY * TINY
            };
        }
        if ix > 0x3ff00000 {
            return if hy > 0 {
                s * HUGE * HUGE
            } else {
                s * TINY * TINY
            };
        }
        // now |1-x| is tiny <= 2**-20, suffice to compute log(x) by x-x^2/2+x^3/3-x^4/4
        let t = ax - ONE; // t has 20 trailing zeros
        let w = (t * t) * (0.5 - t * (0.3333333333333333333333 - t * 0.25));
        let u = IVLN2_H * t; // ivln2_h has 21 sig. bits
        let v = t * IVLN2_L - w * IVLN2;
        let t1 = with_low_word(u + v, 0);
        (t1, v - (t1 - u))
    } else {
        n = 0;
        // take care subnormal number
        if ix < 0x00100000 {
            ax *= TWO53;
            n -= 53;
            ix = high_word(ax);
        }
        n += (ix >> 20) - 0x3ff;
        let j = ix & 0x000fffff;
        // determine interval
        ix = j | 0x3ff00000; // normalize ix
        let k: usize;
        if j <= 0x3988E {
            k = 0; // |x|<sqrt(3/2)
        } else if j < 0xBB67A {
            k = 1; // |x|<sqrt(3)
        } else {
            k = 0;
            n += 1;
            ix -= 0x00100000;
        }
        ax = with_high_word(ax, ix);

        // compute ss = s_h+s_l = (x-1)/(x+1) or (x-1.5)/(x+1.5)
        let u = ax - BP[k]; // bp[0]=1.0, bp[1]=1.5
        let v = ONE / (ax + BP[k]);
        let ss = u * v;
        let s_h = with_low_word(ss, 0);
        // t_h=ax+bp[k] High
        let t_h = from_words(
            ((ix >> 1) | 0x20000000) + 0x00080000 + ((k as i32) << 18),
            0,
        );
        let t_l = ax - (t_h - BP[k]);
        let s_l = v * ((u - s_h * t_h) - s_h * t_l);
        // compute log(ax)
        let mut s2 = ss * ss;
        let mut r = s2 * s2 * (L1 + s2 * (L2 + s2 * (L3 + s2 * (L4 + s2 * (L5 + s2 * L6)))));
        r += s_l * (s_h + ss);
        s2 = s_h * s_h;
        let t_h = with_low_word(3.0 + s2 + r, 0);
        let t_l = r - ((t_h - 3.0) - s2);
        // u+v = ss*(1+...)
        let u = s_h * t_h;
        let v = s_l * t_h + t_l * ss;
        // 2/(3log2)*(ss+...)
        let p_h = with_low_word(u + v, 0);
        let p_l = v - (p_h - u);
        let z_h = CP_H * p_h; // cp_h+cp_l = 2/(3*log2)
        let z_l = CP_L * p_h + p_l * CP + DP_L[k];
        // log2(ax) = (ss+..)*2/(3*log2) = n + dp_h + z_h + z_l
        let t = f64::from(n);
        let t1 = with_low_word(((z_h + z_l) + DP_H[k]) + t, 0);
        (t1, z_l - (((t1 - t) - DP_H[k]) - z_h))
    };

    // split up y into y1+y2 and compute (y1+y2)*(t1+t2)
    let y1 = with_low_word(y, 0);
    let p_l = (y - y1) * t1 + y * t2;
    let mut p_h = y1 * t1;
    let z = p_l + p_h;
    let j = high_word(z);
    let i = low_word(z);
    if j >= 0x40900000 {
        // z >= 1024
        if ((j - 0x40900000) as u32 | i) != 0 {
            return s * HUGE * HUGE; // if z > 1024, overflow
        }
        if p_l + OVT > z - p_h {
            return s * HUGE * HUGE; // overflow
        }
    } else if (j & 0x7fffffff) >= 0x4090cc00 {
        // z <= -1075
        if ((j as u32).wrapping_sub(0xc090cc00) | i) != 0 {
            return s * TINY * TINY; // z < -1075, underflow
        }
        if p_l <= z - p_h {
            return s * TINY * TINY; // underflow
        }
    }

    // compute 2**(p_h+p_l)
    let i = j & 0x7fffffff;
    let mut k = (i >> 20) - 0x3ff;
    let mut n = 0;
    if i > 0x3fe00000 {
        // if |z| > 0.5, set n = [z+0.5]
        n = j + (0x00100000 >> (k + 1));
        k = ((n & 0x7fffffff) >> 20) - 0x3ff; // new k for n
        let t = from_words(n & !(0x000fffff >> k), 0);
        n = ((n & 0x000fffff) | 0x00100000) >> (20 - k);
        if j < 0 {
            n = -n;
        }
        p_h -= t;
    }
    let t = with_low_word(p_l + p_h, 0);
    let u = t * LG2_H;
    let v = (p_l - (t - p_h)) * LG2 + t * LG2_L;
    let mut z = u + v;
    let w = v - (z - u);
    let t = z * z;
    let t1 = z - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    let r = (z * t1) / ((t1 - TWO) - (w + z * w));
    z = ONE - (r - z);
    let j = high_word(z).wrapping_add(((n as u32) << 20) as i32);
    if (j >> 20) <= 0 {
        z = scalbn(z, n); // subnormal output
    } else {
        z = with_high_word(z, high_word(z).wrapping_add(((n as u32) << 20) as i32));
    }
    s * z
}

/// `Math.hypot(a, b)`, as V8's `FastMathHypot` (`src/builtins/math.tq`) computes it: Infinity
/// wins over NaN, and the sum of squares is scaled by the larger magnitude so it cannot overflow.
pub fn hypot(a: f64, b: f64) -> f64 {
    let a = a.abs();
    let b = b.abs();
    if a == f64::INFINITY || b == f64::INFINITY {
        return f64::INFINITY;
    }
    let largest = max(a, b);
    if largest.is_nan() {
        return f64::NAN;
    }
    if largest == 0.0 {
        return 0.0;
    }
    ((a / largest) * (a / largest) + (b / largest) * (b / largest)).sqrt() * largest
}

/// `Math.hypot(a, b, c)`, as `FastMathHypot` computes it: the same rules as `hypot`, and the third
/// square absorbs the rounding error of the first sum (compensated summation).
pub fn hypot3(a: f64, b: f64, c: f64) -> f64 {
    let a = a.abs();
    let b = b.abs();
    let c = c.abs();
    if a == f64::INFINITY || b == f64::INFINITY || c == f64::INFINITY {
        return f64::INFINITY;
    }
    let largest = max(max(a, b), c);
    if largest.is_nan() {
        return f64::NAN;
    }
    if largest == 0.0 {
        return 0.0;
    }
    let power_a = (a / largest) * (a / largest);
    let power_b = (b / largest) * (b / largest);
    let compensation = (power_a + power_b) - power_a - power_b;
    let power_c = (c / largest) * (c / largest) - compensation;
    (power_a + power_b + power_c).sqrt() * largest
}

/// `Math.round`: halves go toward +Infinity (`round(-2.5)` is -2), and inputs in [-0.5, -0] give
/// -0. This is V8's `Float64Round`; `f64::round` sends halves away from zero instead.
pub fn round(x: f64) -> f64 {
    let up = x.ceil();
    if up - 0.5 <= x { up } else { up - 1.0 }
}

/// `Math.max(a, b)`: NaN if either is NaN, and +0 is above -0. `f64::max` ignores NaN instead.
pub fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else if b > a {
        b
    } else if a.is_sign_positive() {
        a // equal, so only the sign of a zero can differ
    } else {
        b
    }
}

/// `Math.min(a, b)`: NaN if either is NaN, and -0 is below +0. `f64::min` ignores NaN instead.
pub fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else if b < a {
        b
    } else if a.is_sign_negative() {
        a // equal, so only the sign of a zero can differ
    } else {
        b
    }
}

/// `Math.sign`: 1 or -1, with zeros (keeping their sign) and NaN passed through. `f64::signum`
/// returns 1 for +0 and -1 for -0 instead.
pub fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// JavaScript's ToUint32 (`x >>> 0`): truncate toward zero, then wrap modulo 2^32.
/// NaN, infinities and both zeros become 0.
pub fn to_uint32(x: f64) -> u32 {
    let bits = x.to_bits();
    let exponent = ((bits >> 52) & 0x7FF) as i32 - 1023;
    // For |x| < 1 the integer part is zero. From 2^84 onward every finite f64 is a
    // multiple of 2^32. That upper cutoff also covers NaN and infinities.
    if !(0..84).contains(&exponent) {
        return 0;
    }
    let significand = (bits & 0x000F_FFFF_FFFF_FFFF) | 0x0010_0000_0000_0000;
    let magnitude = if exponent <= 52 {
        (significand >> (52 - exponent)) as u32
    } else {
        (significand as u32) << (exponent - 52)
    };
    if bits >> 63 != 0 {
        magnitude.wrapping_neg()
    } else {
        magnitude
    }
}

/// JavaScript's ToInt32 (`x | 0`): the same low 32 bits as ToUint32, interpreted as signed.
pub fn to_int32(x: f64) -> i32 {
    to_uint32(x) as i32
}

/// `Math.imul(a, b)`: convert both numbers to u32, multiply modulo 2^32, then interpret as signed.
pub fn imul(a: f64, b: f64) -> i32 {
    to_uint32(a).wrapping_mul(to_uint32(b)) as i32
}
