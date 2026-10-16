# MOV-FOLLOWUP native intent tests

Baseline: native `6d21066fa26b8d67124b446a23163b7f0cc20214`, including movement merge #302 at `7285fb4d`. Prototype: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. These are test additions. They add no runtime state or admission capability.

## Row mapping

- ENG-33: `eng33_cmb142_all_nine_squad_kinds_enter_at_the_door_and_finish`, `eng33_cmb142_nearest_free_cells_reserve_before_arrival_and_exclude_enclosed_cells`, `eng33_entry_radius_is_inclusive_at_exactly_three_point_two_metres`, `eng33_queued_entry_activates_after_move_and_destroyed_house_cancels_it`, and `mov48_eng33_manual_and_wounded_garrisons_exit_then_reach_home_through_finish`.
- CMB-142: the two shared ENG-33/CMB-142 tests and `cmb142_denials_preserve_eligibility_visibility_retreat_and_queue_limit`.
- MOV-04: `speed_component_pace_and_retreat_sprint_suppression_precedence`, `speed_component_wire_slope_and_snow_multiply_actual_displacement`, and `speed_component_vehicle_road_grade_and_snow_keep_their_own_factors`. Retained tests `mov08_world_march_moves_calm_owned_squad_faster_than_combat_squad`, `original_world_march_home_visibility_cover_and_first_contact`, `log07_ordinary_depletion_caps_travel_and_charges_actual_driving`, `log07_forced_escape_scales_retreat_speed_and_empty_fuel_strands`, and `mov23_reserve_vehicle_caps_residual_motion_at_full_reduced_and_zero_fuel` cover the existing World march policy and fuel wrapper.

The new tests are registered under `#[cfg(test)]` in `core/src/lib.rs`. The speed matrix has 21 cases. Expected speeds use literal prototype constants in the prototype's operation order, with exact speed and final-coordinate assertions. No Source recorder, fixture capture or provenance bundle is added or run.

Prototype review: `shared/sim.js:317-329` supplies squad eligibility and Ura speed; `:2448-2462` selects the nearest unoccupied house edge cell; `:3478-3494` supplies garrison command admission and reservations; `:5433-5439` supplies the inclusive 3.2 m entry check and removed-house cancellation. Speed composition is at `:3817` and `:5483-5486`, ground sampling and World march at `:2363-2388`. Retreat exit and home selection are at `:1725-1734` and `:3335-3344`.

## Ordinary survival

Every garrison scenario retains the constructor roster. Additional actors use `publish_starting_unit`, which supplies combat, firing, ability and recovery state. Every actual tick captures complete recipient snapshots for both seats. After the tested behavior, a canonically published rifle captures the neutral objective. Actual point income reaches the configured VP goal, without installing VP or a winner. Each scenario continues for eight ending ticks and asserts no tick fault. Manual and wounded garrison retreats run in both Default and Conquest.

The radius test checks exactly 3.2 m and the next representable position outside it. Reservation tests check cells before either squad arrives, then check occupied capacity. A separate water-enclosed cell proves that nearest distance alone cannot select a cell without an open four-neighbour. Queue tests retain Enter behind a moving route and check the existing owner-only kind 9 projection. Removal uses canonical `World::set_cell` after entry reservation and before arrival.

MOV-04's fuel checks call the existing reserve-aware movement wrapper directly. The World checks use existing detached component state. They neither admit full logistics ticks nor require World recipient snapshots. Ordinary movement and retreat survival are covered by the garrison continuations.

## Dropped rows

- MOV-01: complete observable dependencies across every named tick phase are not established by this batch. A phase-name vector would not prove composition.
- MOV-03: the complete per-unit sequence includes fuel use and reserve integration; enabled logistics ticks remain explicitly guarded.
- MOV-42: complete World route-leg outcomes and owner plans need World admission and recipient delivery, which stay closed.
- MOV-48: this batch proves the garrison exit and ordinary retreat continuation; full mode-specific home selection and aircraft transitions remain with open #339 and are absent from this baseline.

No differs candidates. No overlap with the runtime changes in #301, #355 or #357. No parity-table, DESIGN, reference, FFI, header, wire, contract, privacy or public-ingress changes.

## Validation

Final focused checks pass on the published Rust tree, after merging `origin/main` (already current at `6d21066f`):

```text
cargo nextest run --locked --cargo-profile ci-test -p ww2-core -j4 --no-fail-fast
  -E 'test(mov_followup) | test(mov08_world_march) | test(original_world_march_home) | test(log07_) | test(mov23_reserve_vehicle)'
17 tests run: 17 passed, 1048 skipped
cargo fmt --all --check: PASS
cargo clippy --workspace --all-targets -j4 -- -D warnings: PASS
```

Cargo ran on the Vostro in S1, with its matching target directory, Rust 1.99.0, four jobs, a 6 GiB memory limit and zero swap. Each command released its lock immediately afterward. Only `ww2-core` tests were selected; no workspace test run was performed. The new nine tests all passed. The other eight selections are retained march/fuel checks. Hosted CI and owner acceptance remain separate.

Acknowledged `axel-owner-decision-batch3-relay-20261016` and the later #302/#355 readbacks on issue #5: pending capability groups remain closed, queued Enter keeps existing kind 9, and this stream does not approve any interface change. No issue comment was posted.

Earlier diagnostic runs found fixture errors: the initial test helper omitted recovery state, `#` is traversable cover rather than a movement blocker, and the weather key is lowercase `snow`. The helper now uses the canonical publisher, the enclosed-cell case uses water, and the weather lookup uses the actual key. Runtime guards and assertions were not weakened.
