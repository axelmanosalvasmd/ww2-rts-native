# Engineering intent batch 1

Base: `40fe9e296b46060241693c6c627ecd615d4f8ea4`. Reference: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

This test-only batch extracts selected tests from `0e2474ed` and `07f66f2f` onto main. It preserves shipping code, existing fixtures, parity tables and historical failures. Behavior checks follow [Axel's approved native-intent rules](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267). Existing exact component comparisons remain selected. No recorder, capture or provenance bundle is added.

The supplied REVIEWER_2 review accepted ENG-17/23/29/30/31/35/54/60/61/74/78/79. This batch adds the missing 10/11/23/24 house-size boundaries, all six house cover and vision consumers, and dry/partial/full-rain house damage for ENG-32/34/71. Classification retains the existing section-health and post-failure type checks. Cover is checked at full, half and one remaining HP. Rain cases inspect actual section damage after ignition and burning; the retained environment fixture independently checks progressive fire-only failure.

## Row to test map

Selectors beginning with `engineering_intent_tests::` or `field_works::` use the core library target. Other names use the core `it` target.

| Row | Native selectors | Pinned prototype intent |
| --- | --- | --- |
| ENG-17 | `field_works::match_projects::tests::original_checks::five_segment_line_pays_once_per_segment_and_completes_twenty_cells`; `field_projects::original_shared_pattern_order_front_and_segment_prices` | `shared/sim.js:512`; supported native materials retain exact minimum, rounding and proportional pricing. |
| ENG-23 | `field_works::match_projects::tests::original_checks::strongpoint_completes_twelve_trench_and_ten_forward_wire_cells`; `field_projects::original_shared_pattern_order_front_and_segment_prices` | `shared/sim.js:499-506`; exact geometry/material order plus actual completion. |
| ENG-29 | `engineering_intent_tests::eng29_quiet_trench_hides_at_25m_until_firing_and_is_seen_up_close` | `shared/sim.js:4421-4433` |
| ENG-30 | `engineering_intent_tests::eng30_trench_speed_and_actual_tick_suppression_rally` | `shared/sim.js:2366,5362` |
| ENG-31 | `engineering_intent_tests::eng31_trench_wear_caves_in_and_vehicle_contact_crushes_it` | `shared/sim.js:4361-4372,4723,4772,5513` |
| ENG-32 | `engineering_intent_tests::eng32_all_six_house_types_have_native_sections_and_keep_their_type` | `shared/sim.js:139-146,2274-2300` |
| ENG-34 | `engineering_intent_tests::eng34_house_cover_and_vision_follow_type_and_damage` | `shared/sim.js:2300,4201,4548,4723` |
| ENG-35 | `engineering_intent_tests::eng35_house_failure_evicts_and_fire_hurts_the_garrison` | `shared/sim.js:2417-2421,2585-2588,4815` |
| ENG-54 | `engineering_intent_tests::eng54_ground_cover_tracks_depth_and_remaining_cell_health` | `shared/sim.js:2264-2270,2347-2348` |
| ENG-60 | `engineering_intent_tests::eng60_vehicle_contact_forms_mud_with_wetness_and_crusher_scaling` | `shared/sim.js:5519-5524` |
| ENG-61 | `engineering_intent_tests::eng61_dry_dust_lasts_one_second_and_extends_target_visibility` | `shared/sim.js:4378,4431,5525` |
| ENG-71 | `engineering_intent_tests::eng71_idle_infantry_escapes_fire_and_burnout_changes_woods` | `shared/sim.js:4812-4843,6050` |
| ENG-78 | `engineering_intent_tests::eng78_wet_ground_leaves_roads_unchanged_and_blocks_ignition` | `shared/sim.js:2363-2371,4548,4799,5520-5525,5639` |
| ENG-79 | `engineering_intent_tests::eng79_mutation_rejects_invalid_layers_atomically_and_keeps_overlays_separate` | `shared/sim.js:2650-2663,2680-2740; shared/world-layers.js:53-66` |
| ENG-74 | `engineering_intent_tests::eng74_ground_weather_scaling_reaches_canonical_vision_and_speed` | `shared/sim.js:1745,4548,5485; shared/weather.js:81-82,105-109` |

Supporting selectors run in the same batch:

- `environment::authoritative_environment_matches_original_actions_and_order`: progressive fire-only house failure for ENG-71.
- `world::mutation_state_journals_and_pending_match_original_source`: exact initialized wear/hash and mutation state for ENG-54/79.
- `engineering_weather::eng74_weather_applies_to_ground_kinds_and_spares_aircraft`: all 47 unit kinds through five weather settings for ENG-74/78.

ENG-32 also uses ENG-34's six-type cover/vision matrix. ENG-71 also uses ENG-35's garrison fire and eviction assertions. ENG-78 also uses ENG-60/61/74 for wet wear, dust and sight. These are component/provider checks where named; speed assertions do not claim measured travel distances.

## Dropped from this test PR

These 26 rows remain incomplete. Dropping them from this batch does not change their parity status or propose removing the feature.

- ENG-03: Wire path-cost increment remains unasserted.
- ENG-05: Pit range and first-projectile sandbag exemption remain unconnected.
- ENG-07: Bridge construction and bank approach are unsupported.
- ENG-08: Paid Fill actuator is unsupported.
- ENG-11: Concrete wall construction and occupied-cell waiting are unsupported.
- ENG-12: Gate construction and hysteresis are unsupported.
- ENG-15: Bridge direction, concrete tier and retreating-builder clauses remain incomplete.
- ENG-16: Bank-connected bridge placement and shared client preview remain incomplete.
- ENG-24: Fill/concrete material eligibility and complete pattern allocation remain unsupported.
- ENG-27: Historical ghost-cleanup witness failed tick-zero ability validation; omitted.
- ENG-28: Historical directional-trench witness lacked trench-front data; omitted.
- ENG-33: Positive garrison entry and capacity actuator are unsupported.
- ENG-38: Nearest-section directional distribution and tick-driven sync remain unasserted.
- ENG-41: Aircraft exemption during bridge failure remains unasserted.
- ENG-42: Direct damage selection, pillbox direction and allied-fire control remain unasserted.
- ENG-44: Occupied landing, dead-building footprint and Fill-only clearing remain incomplete.
- ENG-46: Positive Fire-at terrain actuator is unsupported.
- ENG-52: Fill height, map exclusions, road recovery and spoiled queries remain unsupported.
- ENG-56: Bridge construction/repair graph and hull-width admission remain incomplete.
- ENG-57: Paid plank replacement depends on unsupported bridge construction.
- ENG-58: Bridge failure does not establish rail disconnection.
- ENG-63: Complete recipient mine metadata/navigation and sanitized map download remain unasserted.
- ENG-64: Hostile mine blast actuator is unsupported.
- ENG-67: Infantry passage, hull cover, crater destruction and occupied landing remain incomplete.
- ENG-70: Half-second fire scheduling and the complete timer/wind matrix remain unasserted.
- ENG-80: Joint environment draw consumption and independent Random-weather planner seed remain incomplete.

## Validation

The final focused selection passes: 19 passed, 0 failed, 287 skipped across the core library and `it` binaries, on the Rust tree committed with this document. Slot S1, `ci-test` profile, four build jobs, 6 GiB memory and no swap were used. No whole-workspace tests or Source jobs ran.

The initial selection stopped at a new ENG-71 expected-value error: nine passed, one failed and nine selected checks were unrun. That assertion assumed default B surface material (stone) rather than inferred shed section material (wood). Both native `structures.rs` and pinned `sim.js:2486,2504-2508` choose the section material. The corrected test asserts effective wood and the literal losses 10, 6.5 and 3 HP for dry, half-rain and full-rain one-second burns. No runtime code changed. The initial log remains at `/tmp/eng-intent-batch1-20261016/native-initial.log`; the complete passing run is `native.log` in that directory. An earlier wrapper attempt stopped before Cargo because it ran above the compiler-pin directory; moving its working directory to `rust/` resolved that setup error.

Historical 34-pass/2-fail and fourteen-pass results retain their old producing heads and are not reused as this batch's results. `cargo fmt --all --check` and `cargo clippy --locked --workspace --all-targets -j4 -- -D warnings` both pass in slot S2 under the same memory limits. The slot locks were released as each job exited.
