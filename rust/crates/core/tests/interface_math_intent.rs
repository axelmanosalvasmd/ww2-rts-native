//! IF-19: numeric primitives used by pinned Source's bitwise RNG and cell-noise expressions
//! (`shared/sim.js:869-873,2345-2347`). Expected values were checked with x64 Node 24.21.0.

use ww2_core::math;

#[test]
fn intent_if19_js_integer_math_and_remainder_edges() {
    let conversions: &[(f64, i32, u32)] = &[
        (0.0, 0, 0),
        (-0.0, 0, 0),
        (0.75, 0, 0),
        (-0.75, 0, 0),
        (3.875, 3, 3),
        (-3.875, -3, 4_294_967_293),
        (f64::NAN, 0, 0),
        (f64::INFINITY, 0, 0),
        (f64::NEG_INFINITY, 0, 0),
        (f64::from_bits(1), 0, 0),
        (-f64::from_bits(1), 0, 0),
        (2_147_483_647.75, i32::MAX, 2_147_483_647),
        (2_147_483_648.75, i32::MIN, 2_147_483_648),
        (-2_147_483_648.75, i32::MIN, 2_147_483_648),
        (-2_147_483_649.75, i32::MAX, 2_147_483_647),
        (4_294_967_295.75, -1, u32::MAX),
        (4_294_967_296.75, 0, 0),
        (4_294_967_299.75, 3, 3),
        (-4_294_967_299.75, -3, 4_294_967_293),
        (-4_294_967_295.75, 1, 1),
        (4_503_599_627_370_495.5, -1, u32::MAX),
        (4_503_599_627_370_497.0, 1, 1),
        (9_007_199_254_740_991.0, -1, u32::MAX),
        (9_007_199_254_740_992.0, 0, 0),
        (9_007_199_254_740_994.0, 2, 2),
        (-9_007_199_254_740_994.0, -2, 4_294_967_294),
        // 2^64 + 4096 is exact in f64 but is outside u64's range.
        (f64::from_bits(0x43F0_0000_0000_0001), 4_096, 4_096),
        (
            -f64::from_bits(0x43F0_0000_0000_0001),
            -4_096,
            4_294_963_200,
        ),
        // At exponent 83 a significand bit still contributes 2^31; at exponent 84 it vanishes.
        (
            f64::from_bits(0x4520_0000_0000_0001),
            i32::MIN,
            2_147_483_648,
        ),
        (
            f64::from_bits(0xC520_0000_0000_0001),
            i32::MIN,
            2_147_483_648,
        ),
        (f64::from_bits(0x4530_0000_0000_0001), 0, 0),
        (f64::MAX, 0, 0),
        (-f64::MAX, 0, 0),
    ];
    for &(input, signed, unsigned) in conversions {
        assert_eq!(math::to_int32(input), signed, "ToInt32({input:?})");
        assert_eq!(math::to_uint32(input), unsigned, "ToUint32({input:?})");
    }

    for (a, b, expected) in [
        (-0.0, 7.0, 0),
        (f64::NAN, 7.0, 0),
        (f64::INFINITY, 3.0, 0),
        (3.0, f64::NEG_INFINITY, 0),
        (4_294_967_295.0, 5.0, -5),
        (2_147_483_647.0, 2.0, -2),
        (2_147_483_648.0, 2.0, 0),
        (65_536.0, 65_536.0, 0),
        (65_537.0, 65_537.0, 131_073),
        (4_294_967_297.875, 4_294_967_295.875, -1),
        (-3.875, 2.875, -6),
        (-1.0, -1.0, 1),
        (2_654_435_761.0, 2_246_822_507.0, -1_850_702_085),
    ] {
        assert_eq!(math::imul(a, b), expected, "Math.imul({a:?}, {b:?})");
    }

    let extrema: &[(f64, f64, f64, f64)] = &[
        (0.0, -0.0, 0.0, -0.0),
        (-0.0, 0.0, 0.0, -0.0),
        (-0.0, -0.0, -0.0, -0.0),
        (0.0, 0.0, 0.0, 0.0),
        (
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        (3.0, -5.0, 3.0, -5.0),
    ];
    for &(a, b, maximum, minimum) in extrema {
        assert_eq!(math::max(a, b).to_bits(), maximum.to_bits());
        assert_eq!(math::min(a, b).to_bits(), minimum.to_bits());
    }
    for (a, b) in [
        (f64::NAN, f64::INFINITY),
        (f64::INFINITY, f64::NAN),
        (f64::NEG_INFINITY, f64::NAN),
    ] {
        assert!(math::max(a, b).is_nan());
        assert!(math::min(a, b).is_nan());
    }

    let remainders: &[(f64, f64, f64)] = &[
        (-5.5, 2.0, -1.5),
        (5.5, -2.0, 1.5),
        (-5.5, -2.0, -1.5),
        (5.5, 2.0, 1.5),
        (-4.0, 2.0, -0.0),
        (-4.0, -2.0, -0.0),
        (4.0, -2.0, 0.0),
        (-0.0, 3.0, -0.0),
        (3.0, f64::INFINITY, 3.0),
        (-3.0, f64::NEG_INFINITY, -3.0),
        (1.0, 0.0, f64::NAN),
        (f64::INFINITY, 2.0, f64::NAN),
        (1.0, f64::NAN, f64::NAN),
    ];
    for &(a, b, expected) in remainders {
        let remainder = a % b;
        if expected.is_nan() {
            assert!(remainder.is_nan(), "{a:?} % {b:?}");
        } else {
            assert_eq!(remainder.to_bits(), expected.to_bits(), "{a:?} % {b:?}");
        }
    }
}
