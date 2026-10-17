//! Equality of prototype number strings and fixed-shape JSON fields.

use crate::{game::Point, math};
use serde::Serialize;
use serde_json::Value;

/// Shortest round-trip printing distinguishes finite doubles except signed zero.
/// Every NaN payload prints `NaN` in a property key.
pub(crate) fn number_key(value: f64) -> u64 {
    if value == 0.0 {
        0
    } else if value.is_nan() {
        f64::NAN.to_bits()
    } else {
        value.to_bits()
    }
}

pub(crate) fn point_key(point: Point) -> (u64, u64) {
    (number_key(point.x), number_key(point.z))
}

/// Keep the saved integer cache keys. Outside this range, skip caching rather
/// than alias distinct JavaScript number strings through a saturating cast.
pub(crate) fn handoff_key(values: [f64; 5]) -> Option<[i64; 5]> {
    let mut key = [0; 5];
    for (out, value) in key.iter_mut().zip(values) {
        let rounded = math::round(value * 4.0);
        if !rounded.is_finite()
            || !(-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&rounded)
        {
            return None;
        }
        *out = rounded as i64;
    }
    Some(key)
}

/// Compare fixed native field shapes as JSON numbers: signed zero prints `0`,
/// all non-finite floats print `null`, and finite doubles retain their identity.
/// Object field names and array order still participate in equality.
pub(crate) fn json_fields_equal<T: Serialize>(left: &T, right: &T) -> bool {
    fn equal(left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
            (Value::Array(a), Value::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
            }
            (Value::Object(a), Value::Object(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .all(|(key, value)| b.get(key).is_some_and(|other| equal(value, other)))
            }
            _ => left == right,
        }
    }
    match (serde_json::to_value(left), serde_json::to_value(right)) {
        (Ok(left), Ok(right)) => equal(&left, &right),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn if26_number_string_keys_fold_zero_and_nan_without_rounding_neighbors() {
        // Source String(number): 0, 0, NaN, NaN, 1, 1.0000000000000002,
        // 1e-7, its next representable value, Infinity, -Infinity.
        let values = [
            0.0,
            -0.0,
            f64::from_bits(0x7ff8_0000_0000_0001),
            f64::from_bits(0xfff8_0000_0000_4321),
            1.0,
            f64::from_bits(1.0_f64.to_bits() + 1),
            1e-7,
            f64::from_bits(1e-7_f64.to_bits() + 1),
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];
        let mut groups = BTreeMap::new();
        let actual: Vec<_> = values
            .map(|value| {
                let next = groups.len();
                *groups.entry(number_key(value)).or_insert(next)
            })
            .into();
        assert_eq!(actual, [0, 0, 1, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(
            point_key(Point {
                x: -0.0,
                z: f64::NAN
            }),
            (0, number_key(f64::NAN))
        );
    }

    #[test]
    fn if26_handoff_keys_round_at_quarter_meters_without_integer_saturation() {
        let base = [0.0, -0.0, 0.124, -0.125, 15.0];
        assert_eq!(handoff_key(base), Some([0, 0, 0, 0, 60]));
        assert_eq!(
            handoff_key([0.125, -0.126, 0.374, 0.375, 15.01]),
            Some([1, -1, 1, 2, 60])
        );
        let mut nearby = base;
        nearby[4] = 15.124;
        assert_eq!(handoff_key(base), handoff_key(nearby));
        nearby[4] = 15.125;
        assert_ne!(handoff_key(base), handoff_key(nearby));
        for value in [
            f64::NAN,
            f64::INFINITY,
            f64::MAX,
            2_f64.powi(61),
            -2_f64.powi(62),
        ] {
            assert_eq!(handoff_key([value; 5]), None);
        }
        assert_eq!(handoff_key([-2_f64.powi(61); 5]), Some([i64::MIN; 5]));
    }

    #[test]
    fn if26_json_field_equality_uses_js_number_printing_and_preserves_shape() {
        assert!(json_fields_equal(&[-0.0, f64::NAN], &[0.0, f64::INFINITY]));
        assert!(json_fields_equal(
            &serde_json::json!([1, 0]),
            &serde_json::json!([1.0, -0.0])
        ));
        assert!(!json_fields_equal(
            &[1.0],
            &[f64::from_bits(1.0_f64.to_bits() + 1)]
        ));
        assert!(!json_fields_equal(
            &serde_json::json!([null]),
            &serde_json::json!(["NaN"])
        ));
        assert!(!json_fields_equal(
            &serde_json::json!([1, 2]),
            &serde_json::json!([2, 1])
        ));
        assert!(!json_fields_equal(
            &serde_json::json!({"x":0}),
            &serde_json::json!({"z":0})
        ));
        assert!(!json_fields_equal(
            &Some(Point {
                x: f64::NAN,
                z: 0.0
            }),
            &None
        ));
    }
}
