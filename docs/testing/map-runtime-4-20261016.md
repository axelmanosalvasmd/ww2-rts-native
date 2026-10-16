# MAP-RUNTIME-4

Branch: `runtime/map-batch4-20261016`, created from `origin/main` at `9fe25aa829d980bf985eedad4db2537d08a8949b`. Batch 3 and its PR #320 were left alone. No parity status files, FFI, header, wire format, contracts, privacy rules or reference files changed.

Proven behavior:

- MAP-19: `map_authoring::tests::authored_selection_move_remove_prune_preserves_layers_and_rejects_unsupported_survivor`. Covers the lifecycle in `reference/ww2-rts/test-engine-authoring.js:25-32`, including unchanged ground and material overrides, moved section IDs/health/supports, atomic invalid moves, roundtrip, removal and dangling support pruning. Existing `structures` integration tests retain validation-limit and bank/support coverage.
- MAP-15: `map_save::tests::map15_map_loading_uses_the_existing_complete_scenario_validator`. Connects `AuthoritativeMap` to #298's validator rather than adding another validator. Checks valid definitions, invalid version/unit/side/recipient/reference and startup rejection. The retained `scenario_validation` tests cover the complete schema.

Supporting match check: `map_save::tests::edited_saved_map_ticks_and_collects_every_recipient_without_new_match_state` moves an authored house, prepares and reloads its save, advances five ordinary ticks and collects both recipients plus ordinary snapshot admission each tick. No new live state is introduced. Authored scenarios are kept out of matches by the shared startup planner; tick, snapshot, elimination and command guards were inspected. This is not a complete-match outcome run.

Unproven assigned rows:

- MAP-23: `map_save::tests::map23_save_projection_keeps_only_authored_fields_and_all_shipped_maps_round_trip` and `map23_editor_limit_counts_incoming_bytes_before_parsing_or_projection` cover the in-memory projection and cap. All 33 shipped authoring documents roundtrip, preserve static terrain and definitions, and produce stable second saves. The exact 8 MiB input boundary, malformed input and invalid maps are checked. The server has no save caller, authorization or persistence path, so the row remains held for server integration. Generated World documents are outside this save scope.
- MAP-57: `terrain.rs::height_at` is the simulation's bilinear level sampler, not the rendered-ground sampler. The missing component needs trench/road offsets, frozen water lines, carved beds and cliff surfaces from `client/relief.js` and `client/water-levels.js`.
- MAP-90: `match_world_rails.rs` computes track masks, station cells, bridge cells and the five-cell yard, but `Match::validate_construction_tick` rejects World contexts and construction does not consume the yard. World admission was not changed.

MAP-92 differs candidates: the five `map_world_validation_tests::map92_*` tests check native diagnostic categories and reachability, including an objective blocked away from spawns, cardinal detours, one-level crossings and rejected two-level/diagonal-only crossings. Native diagnostics are field-specific: invalid size gives `world.size: expected huge or massive`, while `shared/sim.js:1141` returns `invalid world regions`. Native also requires bounded integer IDs and ordered in-range integer bounds; the prototype only checks integer IDs and a four-element bounds array. These stronger pre-existing checks are separate from diagnostic wording. No native strings were copied or changed.

Validation history:

- First focused run: 5 passed, 1 failed, 19 unrun after fail-fast. The new projection test accidentally had only one non-Assault spawn. Added a third ordinary spawn and updated its expected projection. No runtime assertion was weakened.
- Second focused run: 22 passed, 1 failed, 2 unrun after fail-fast. The roundtrip comparison used floating-point equality for trench-facing cells initialized to NaN on No Man's Land. Changed that field's comparison to exact `f32::to_bits()` values, including NaN bits, without changing terrain or projection behavior.
- Third focused run: all 31 selected tests passed, and formatting passed. Strict Clippy found a needless borrow in the validator call and two `byte_char_slices` test loops. Removed the borrow and used byte strings for those loops.
- Pre-integration validation at `9fa5f78e` after the lint corrections and `git merge origin/main` (already up to date at `9fe25aa8`): 31 selected tests passed, 587 skipped, zero retries. Nextest run `335b6d2b-4e18-4255-a737-63cd5154aef4`, `ci-test` build profile. `cargo fmt --all --check` and `cargo clippy --locked --workspace --all-targets -j4 -- -D warnings` passed. Final run used S3 with `MemoryMax=6G`, `MemorySwapMax=0`; the slot was released at exit 0. The pre-run disk gate passed (at least 20 GiB available). No full workspace tests or Unreal execution ran.

Integration refresh: merged main `7ed527b0` cleanly after the branch's first push, producing `1b412507`. Fresh validation at `1b412507` passed: formatting, strict workspace/all-target Clippy, and all 31 focused tests (593 skipped), with zero retries. Nextest run `90fff076-fd8f-41e1-aafa-d2cc2d9f47c8`. S3 used the same 6 GiB/zero-swap scope and was released at exit 0. The subsequent documentation commit records these results and changes no Rust source. Earlier results remain attributed to their earlier code.

Focused command from `rust/`, inside the slot scope:

```sh
cargo nextest run --locked --cargo-profile ci-test -j4 -p ww2-core -E 'test(map_authoring::) or test(map_save::) or test(map92_) or test(authoritative_maps::) or test(scenario_validation::) or test(structures::) or test(legacy_scenario::)' --retries 0
```

Independent source review confirmed MAP-15/MAP-19 intent coverage, found the projection test setup error above, and held MAP-23 at its component boundary. Reviewer Cargo runs: zero. The stream owns local execution and PR creation only; the coordinator owns CI, runtime acknowledgment, review and merging.
