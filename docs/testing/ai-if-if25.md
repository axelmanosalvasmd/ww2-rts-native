# IF-25 numeric comparison and stable consumers

Scope: Rust/core. Reference: `reference/ww2-rts` at
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. This change adds no ABI,
wire, public command, match flag or persisted state. It does not activate room AI,
World admission or full logistics. Other AI feature PRs remain independent.

## Comparator rule

Use the existing V8 TimSort in `sort.rs` for a numeric-difference comparator.
A result of NaN is equal, and both signed zero results are equal. Do not use
`total_cmp`, which orders NaN and distinguishes the zero signs. Stability keeps
the input sequence for ties. The comparator's explicit second key still applies
when the prototype uses `difference || secondDifference`.

`sort_by_ref` compares the original values while sorting indices, then moves
the owned values into place with swaps. Tests exercise its insertion and merge
paths and check retained float bits.

The row's suggested blanket debug assertion against NaN would reject a valid
prototype state: an AI scan with no own ground units has an Infinity distance
for every contact, so every numeric difference is NaN. This is now covered
through the real scan consumer. Nonfinite external coordinates are rejected
before the AI state changes. Derived nonfinite values follow the actual source
operation: NaN is equal in sort, while a strict `score < bestScore` selection
skips NaN and positive Infinity.

## Exact prototype values

The original `entrenchPlan` export and the comparator extracted directly from
`shared/supply-convoys.js:246` were evaluated on the Vostro with
`/usr/bin/node`, Node `v24.21.0`, V8 `13.6.233.17-node.53`. The prototype tree
was read only. No Source recorder or fixture bundle was created.

For provision keys, these are the exact zero-based output indices:

| Input keys | Output indices |
| --- | --- |
| `[0, -0, 0, -0]` | `[0, 1, 2, 3]` |
| `[3, NaN, 1, -0, 0, NaN, -1]` | `[0, 1, 6, 3, 4, 2, 5]` |
| `[Infinity, Infinity, -Infinity, NaN, 1, -0, 0]` | `[2, 5, 6, 4, 0, 1, 3]` |

The 96-element case uses `i % 17 === 0 ? NaN : (i * 37) % 13 - 6`.
Its complete original index sequence is a literal in
`sort/if25_tests.rs::if25_nan_merge_order_matches_pinned_prototype_above_one_run`.
The owned-value adapter must produce the same sequence and retain each value's
bits.

For `entrenchPlan(pattern, {x:20,z:20}, {x:40,z:20}, null)`:

| Pattern | Original ordered coordinates |
| --- | --- |
| line | `(30,20), (22,20), (38,20)` |
| double | `(30,20), (30,26), (22,20), (38,20), (22,26), (38,26)` |

All directions are positive zero; all fronts are `-Math.PI / 2`.
The native test compares those fields exactly and checks nonfinite admission.

## Tests and source mapping

| Test selector | Native behavior | Original source |
| --- | --- | --- |
| `if25_numeric_difference_matches_pinned_prototype_for_zero_nan_and_infinity` | Copy and owned comparators, exact identity sequence and retained float bits | `supply-convoys.js:246`, numeric-difference family |
| `if25_nan_merge_order_matches_pinned_prototype_above_one_run` | Both adapters on 96 elements with ties and NaNs | same comparator, original Node result |
| `if25_finite_consumer_keys_compare_without_nan_and_keep_ties` | Instrumented finite comparator asserts every exercised difference is non-NaN | same numeric-difference rule |
| `if25_sim_entrench_sort_matches_original_ties_and_rejects_nan` | Real native entrench plan, exact source ordering including equal distances, finite-input gate | `sim.js:477-509` |
| `if25_convoy_job_comparators_preserve_zero_ties_and_nan_order` | Real troops-to-jobs and priority sorts, signed-zero and NaN provisions, original insertion identities | `supply-convoys.js:246,265` |
| `if25_convoy_origins_keep_insertion_ties_and_nan_distance` | Real origin selection with a NaN distance between tied finite candidates | `supply-convoys.js:268-272` |
| `if25_ai_scan_uses_visible_insertion_order_for_equal_numeric_keys` | Real scan, reversed visibility order, signed-zero position bits, legitimate Infinity-minus-Infinity comparisons | `ai-human.js:131` |
| `if25_ai_rejects_nonfinite_positions_before_state_changes` | Invalid human view cannot initialize state or reach planner | human view admission before scan |
| `if25_ai_point_scores_keep_strict_scan_semantics_when_finite_inputs_overflow` | Admitted finite VP/manpower weights can produce NaN; real tactics skips that objective and chooses a finite alternative | `ai.js:454-459,473-478,752-773` |
| `if25_ai_observation_rejects_nan_score_inputs_and_positions` | Nonfinite coordinates, objective weights and reserve inputs fail before tactics | `Observation::new` admission |
| `if25_ordinary_matches_keep_ticks_and_complete_snapshots_admitted` | Eight ordinary ticks, both complete recipient projections per tick, unchanged logistics guard | ordinary match survival requirement |

## Consumer audit

The following existing numeric sorts now use the shared V8 comparator path:

| Consumer | Original sort |
| --- | --- |
| entrench segment order, with explicit segment-index fallback | `sim.js:509` |
| World neutral defender distances | `sim.js:1210` |
| withdrawal stores | `sim.js:1997` |
| house-edge entry distances | `sim.js:2460` |
| collapse section order | `sim.js:2622` |
| structure damage section order | `sim.js:4736` |
| splash building-cell damage order | `sim.js:4791` |
| nearest covered infantry for grenade autocast | `sim.js:3869` |
| human scan and nearby enemy order | `ai-human.js:131,201` |
| regrouping reinforcements | `ai.js:579` |
| convoy local stores, troops, targets, origins, return origin, unload members, route refresh and retreat origin | all eight floating comparator sites in `supply-convoys.js:196,246,265,272,314,345,478,616` |

The ninth convoy sort, `sources` at line 92, is a stable boolean ownership key
and remains `sort_by_key`. Existing formation, destination lateral order and
boarding-pair sorts already use `sort::sort_by`; this change keeps them on that
implementation. Integer index/ID sorts remain integer sorts.

AI held-threat selection has the source's explicit distance-then-ID tie key
(`ai.js:367`); bounded admitted positions keep that distance finite. Its
`total_cmp` therefore agrees with the source and is unchanged. The three
point-score selections were different: the source uses strict scans, and
finite but extreme weights can produce NaN or Infinity. They now share a
strict scan rather than ordering those derived values with `total_cmp`.

Coverage is a comparator policy audit plus direct consumer tests. It is not a
replay of all 23 `sim.js` and 28 `ai.js` sites named by the inventory, and it
does not prove that the other AI feature rows are implemented. Complete
logistics match execution remains refused. The direct convoy tests exercise
the isolated admitted provider API; they do not lift that match guard.

## Validation

Parent validation on the Vostro:

- At merge `fb1c2d214b0d86b24a929fbac4845f18d41d4cb5`, after merging main
  `1cb49541`, all 24 selected tests passed: the 15 stream tests, eight incoming
  Horde tests and test registration. Formatting and strict workspace Clippy
  passed too.
- At `f0781aa86d3724304a953f9009b879ee9111fc83`, after correcting worker
  scheduling fairness, all four NET-96 socket/hash tests passed again, along
  with `cargo fmt --all --check` and
  `cargo clippy --locked --workspace --all-targets -j4 -- -D warnings`.
  Its core sources are unchanged from the 24-test producer.
- Before the main refresh, all 263 selected existing core/server regression
  tests passed. That earlier broad run is separate from the integration
  selection above. The first focused compilation needed an explicit `f64`
  annotation in one new test, before any tests ran.

Test runs selected only `ww2-core` and `ww2-server` with four jobs. Every Cargo
invocation held one S1-S3 lock, used its slot target directory, applied the
6 GiB memory limit with no swap, and released its lock when Cargo finished.
The independent reviewer ran no Cargo and confirmed the scheduling issue
from source; the final correction removes its fixed-priority cause. The
publication documentation commit preserves the tested Rust sources, tests
and Cargo configuration of `f0781aa8`.
