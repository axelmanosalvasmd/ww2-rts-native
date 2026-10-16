# Combat runtime batch 4

This batch starts from main `a076f316` and reads the prototype at `e4e579a`. It follows Axel's [native-intent evidence decision](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267). It changes no parity status, reference pin, FFI, C header, wire format, contract or privacy policy.

## Runtime changes

Compound clicks dispatch up to eight whitelisted parts through the existing internal command entry. Each leaf uses the enclosing issuer and queue choice. A malformed or refused leaf does not stop later parts; any success accepts the click, otherwise the first refusal is returned. Nested groups and unlisted command families are ignored. World recorded-input preflight uses the same leaf normalization and preserves each leaf's existing actuator guard. This proves the compound wrapper, not the separate combat, base or engineering behavior of each leaf. In particular, missing standalone `rally`, `fireat` and `garrison` handlers remain outside this change. Network admission is unchanged.

Retreat prepares every selected aircraft before committing ground routes, then sends eligible aircraft home and applies the existing ground retreat reset. It also cancels active construction through the canonical work owner, leaving the paid unfinished site and wallet intact. Foreign and missing IDs are not acted on. The old simtest case requiring mixed aircraft/ground Retreat to return Unsupported is removed; the new native test requires the supported behavior instead. The full selection still passes convoy and forced-logistics checks, including IDs beyond the troop execution cap. Aircraft placeholders preserve the original cap positions. Existing convoy and unsupported-mode guards remain in place.

The remaining checks exercise existing World guard visibility and command delegation to the field-work and support owners. CMB-147 and CMB-148 are index rows: these tests prove command admission, ownership, payment and actual owner progress, not every engineering material or off-map support effect. Queued standalone Dig uses main's canonical waiting-order owner after the repair: admission normalizes the order and defers payment without replacing active work.

## Row checks

- CMB-133: `match_world_memory::combat_guard_tests::intent_cmb133_world_guards_refresh_visibility_by_los_building_and_range`. A rifle guard needs line of sight; completed armed buildings and lookouts bypass it. Unfinished and unarmed non-lookout buildings do not observe. Out-of-range enemies leave the neutral actor's visible set.
- CMB-137: `combat_feature_tests::runtime3::cmb137_compound_clicks_preserve_outer_queue_issuer_limit_and_first_denial`. Checks both outer queue choices, issuer ownership, local malformed-part refusal, partial success, the first denial, the eight-part limit and ignored nested/unlisted commands. `world_tick::cmb137_world_recorded_compound_moves_pass_the_existing_actuator_boundary` also checks the real World recorded boundary and retention of unsupported-leaf admission.
- CMB-144: `combat_feature_tests::runtime3::cmb144_retreat_is_immediate_for_mixed_air_ground_and_working_units`. An owned working builder and airborne fighter retreat immediately despite `queue:true`; waiting work clears, the foreign squad and unfinished site remain unchanged, and construction is not refunded.
- CMB-147: `combat_feature_tests::field_work_commands::cmb147_field_work_commands_delegate_dig_patterns_and_joins`. Commands create paid dig jobs and canonical entrench projects, preserve planner selections, queue joins, enforce ownership and queue capacity, and advance real work.
- CMB-148: `combat_feature_tests::support_commands::cmb148_support_commands_delegate_all_catalog_families_to_existing_owner`. All eight catalog families debit the correct wallet and create the correct strike/cooldown state. Refused requests do not debit again. An admitted strafing request advances through activation, damage and removal.

Prototype evidence: `shared/sim.js:3335-3343,3397-3417,3571-3573,3635-3688,4450-4453`. The check for compound clicks is about dispatch semantics; it does not replace the independent leaf-order rows. No Source recorder, captured fixture or provenance bundle is added.

## Dropped from this batch

- CMB-84: waits on #299 (shared air firing).
- CMB-112: exact costs and World payment exist, but Classic ability execution is not admitted.
- CMB-127: no canonical active-wave, budget and total-remaining provider is available to the visibility pass. Counting fielded enemies would reveal stragglers too soon.
- CMB-130: World building memory exists, but Classic memory integration is missing. Adding ghost delivery would also need an agreed recipient interface.
- CMB-134: needs contract. Aircraft snapshot production is still explicitly unsupported; a visibility-predicate-only test cannot prove the complete recipient row. No recipient format or privacy change is made.

CMB-141, 142, 146, 149, 150, 151 and 158 remain for the next batch. CMB-159 and the excluded Unreal client rows are untouched.

## Verification

After merging main `09e03d07`, clean qualification passed on the Vostro: 19 selected tests passed, 774 were skipped, formatting passed, and strict workspace Clippy passed. The run held the S2 flock through package clean, rebuild and all checks, using only `/tmp/ww2-native-target-S2-0e8d6d9b`. There were 48,705,986,560 bytes free before cleaning. Cargo ran with four jobs, a 6 GiB memory limit and no swap.

Commands ran from `rust/` inside that guarded scope:

```sh
cargo clean -p ww2-core -p ww2-simtest
cargo fmt --all --check
cargo nextest run --locked --cargo-profile ci-test -j4 \
  -p ww2-core -p ww2-simtest --no-fail-fast \
  -E 'test(combat_feature_tests::runtime3) | test(intent_cmb133) | test(cmb147_field_work) | test(cmb148_support_commands) | test(cmb137_world_recorded) | test(match_commands::tests) | test(match_ground_orders::tests) | test(accepted_retreat_cancels_paid_work) | test(unsupported_mixed_aircraft_selections) | test(actual_dispatch_matches_ground_order_command_prefixes)'
cargo clippy --workspace --all-targets -j4 -- -D warnings
```

The first final attempt had discovered only 14 tests, omitting all five new core checks; it reported 13 passes and a World compound refusal, and its chained Clippy did not run. The package clean removed 2,039 files (5.1 GiB), restored all 19 expected tests and produced a passing World compound check without another runtime-source change. The underlying artifact mismatch was not traced further.

This is internal runtime coverage, not end-to-end room admission or a complete snapshot claim. Server admission still accepts only Move, Stop and Stance, so neither compound nor Retreat commands enter hosted rooms. World and aircraft snapshot guards remain unchanged. The new compound preflight retains each leaf's existing World actuator refusal.

The early focused runs also exposed invalid weather setup, NaN-sensitive test comparisons and an incorrect assumption about a leaf's denial code. Those were test corrections, not changes to leaf refusal behavior.
