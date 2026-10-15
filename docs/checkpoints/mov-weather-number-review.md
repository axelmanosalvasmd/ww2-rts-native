# Weather Number assertion review, unapplied

Patch base: `7d6300798adcf9dd2a1bcf8a97727d2b3b231918`. The adjacent `mov-weather-number-unapplied.patch` is review data only. No Rust assertion is changed in this checkpoint. Applying it needs Root's decision; execution needs a new exact-head slot. Old517/c46/ec3 packets and7d source-only MOV12/13 remain intact.

## Actual observations and source semantics

Pinned test.js:7661 compares the first weather element with Number1 through `node:assert/strict` deep equality. The companion keeps that same assertion. Pinned sim.js:5907 computes the row from rounded rain, wetness and wind, and6050 delivers it. The unchanged original3a97 capture contains a successful snapshotFor action-end at case3/call964 with `wx:[1,0.3,0.3,0.9]`. Its finite rain Number decodes to binary64 `3ff0000000000000`. Reading that existing gzip runs no Source or native code and does not alter it.

At native `ec3ccfe3`, the real failure at mov_terrain_companion.rs:584 prints actual serde JSON Number(1.0), expected integer1. Native recipient_snapshot/public_state.rs:52-73 constructs wx from actual f64 rain/wet/wind, rejects nonfinite inputs and serializes the rounded f64 array. The lock pins serde_json1.0.154. Its local src/value/partial_eq.rs routes comparison with an integer literal through as_i64, which rejects N::Float; src/number.rs:162-168 reads N::Float unchanged as f64. The logged exact value1.0 therefore denotes binary64 `3ff0000000000000` too. This is source-derived bit equality for the actual logged value, not a fresh native to_bits receipt. No new native runtime result is claimed.

ECMAScript Number uses binary64 values, with distinct positive and negative zero. Node24.21 deepStrictEqual compares primitive values through Object.is and compares type tags strictly. An integral Number has no separate JavaScript integer type. Sources: [ECMAScript Number](https://tc39.es/ecma262/multipage/ecmascript-data-types-and-values.html#sec-ecmascript-language-types-number-type), [Number sameValue](https://tc39.es/ecma262/multipage/ecmascript-data-types-and-values.html#sec-numeric-types-number-sameValue), and [Node24.21 deep strict equality](https://nodejs.org/download/release/v24.21.0/docs/api/assert.html#assertdeepstrictequalactual-expected-message).

## Bounded proposed change

Only the existing weather scalar assertion changes: read wx[0] with typed as_f64, require finite, then compare its IEEE bits against1.0. Present integer1 and float1.0 denote the same Source Number for this check. No strings, booleans, null, missing fields, undefined sentinels or typed wrappers are converted. No rounding or tolerance is used. Any unequal finite bit pattern fails; positive and negative zero remain distinct bit patterns and both fail the expected positive-one assertion. Nonfinite values fail explicitly. The other wx values and every other field retain their existing checks; no broader weather-row equality is claimed.

The unapplied patch includes direct CHANGELOG and DESIGN text for a future application commit. It changes no recorder, original helper, scene, constructor, actor, fixture, native provider, runtime guard, field policy or Status cell. All thirteen captured physical scenes and exact motion/path comparisons remain byte-identical. The prior native failure remains failed and no generic absence/default normalization is proposed.

The Type/finite/bit path is inspected as source only. No new positive/negative control, formatter, lint, Cargo, rustc, Node, Source or native test is executed. Root reads the concrete patch before deciding whether to apply it and grant execution.
