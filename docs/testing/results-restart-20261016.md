# Results and restart intent checks

Prototype: `reference/ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`, inspected from the existing pinned checkout. These tests exercise native behavior. No Source recorder, fixture capture or parity status edit is included.

## Row mapping

- MODE-92: `results_restart_tests::mode92_actual_death_cleanup_selects_last_bunker_or_production_building`. Real death cleanup and victory select the final bunker in Assault and Annihilation and the final production building in Classic. The Classic case retains a barracks after its HQ falls. The caller never supplies the finish position. Post-finish ticks retain the report position. Prototype: `shared/story.js:28-37`, `shared/sim.js:5765,5774,5781`.
- MODE-93: `results_restart_tests::mode93_conquest_runner_samples_team_floor_at_tick_one_200_and_finish` and `results_restart_tests::mode93_ordinary_runner_counts_held_points_and_mode_specific_living_structures`. Actual ticks sample at 1 and 200, then at victory on 201. Sparse team IDs preserve empty array positions. Conquest floors the team total after summing its players. Assault and Annihilation count living bunkers; Classic counts buildings. Held points follow their owner's team, dead structures disappear from the final sample, and ending ticks add no samples. Conquest also collects recipient snapshots before and after victory. Prototype: `shared/story.js:52-63`, `shared/sim.js:5318,5788-5789,5810-5814`.
- MODE-94: `lobby::trusted_room_tests::mode94_restart_keeps_settings_uses_fresh_seeds_and_runs_recipient_frames` and `lobby::trusted_room_tests::mode94_map_and_match_creation_failures_preserve_running_game_and_next_seed`. The trusted Conquest host starts a fresh game with unchanged settings and fresh match, random-weather and environment seeds. Guest restart is refused. Real map lookup and post-load Match creation failures retain the running checkpoint, receipt, delivery accounts, lobby and next epoch. The old game continues sending recipient frames after refusal. A subsequent successful restart uses the unconsumed pending seed. Prototype: `server.js:359-389,688-689`.

The existing generic `fallen.structure` component check remains in `match_story::intent_tests::fallen_locations_remain_separate_and_explicit_finish_spots_override_the_centre`. Ordinary ending selectors consume the specific bunker or production-building position.

The existing binary regression `trusted_room_process::ordinary_binary_two_seats_move_stop_takeover_and_restart_with_private_seed` also covers MODE-94 through the real server. Its restart expectations now require a fresh recorded seed, including wrapping from `u64::MAX` to zero, unchanged remaining settings, a fresh environment seed and matching startup data for both recipients. The old identical-seed assertion failed at `trusted_room_process.rs:181` before this correction.

The existing NET-41 regression
`lobby::net_runtime2::net41_r1_restart_repeats_settings_resets_pause_and_reconnects_absent_seat`
now checks the regenerated spawn list against the next deterministic seed's
match and requires the fresh seed and environment in the private receipt. It
still checks the same map, roster, factions, explicit rain weather and other
settings, host-only restart, pause reset, absent-seat pause/reconnect and
faulted-restart refusal. This replaces the old identical-spawn and
identical-receipt assertions deliberately invalidated by MODE-94. Hosted run
`38940398529` failed only this test on all three platforms; the unmodified
`afdb004b` reproduced the same spawn assertion locally.

## Dropped from this batch

- MODE-90: `lobby::result_message` accepts only Conquest and refuses territory timelines. Existing Conquest and host-end checks do not prove complete ordinary-mode result publication.
- MODE-96: `SnapshotCache::new` still rejects Assault, Classic and Annihilation, so the complete ordinary-mode `mode`, `end` and `out` contract is unproved.
- NET-37: trusted startup prepares synchronously. The asynchronous claim, cancellation token, all-seat denial and rollback lifecycle is not implemented or proved.
- NET-78: the ordinary human-seat hold already has a 120-callback test, but hosted AI attachment is guarded. AI idling during the hold remains unproved.

Hosted Horde-only branches are excluded as requested. Tutorial, World room admission, hosted AI, operatives, spectator policy, new wire shapes, FFI and headers are outside this batch. No dropped row is promoted by a partial witness. No differs candidate is proposed.

## Validation

Run from `rust/` with Rust 1.99.0, an S1-S3 flock, that slot's target directory, MemoryMax=6G and MemorySwapMax=0. Use `--cargo-profile ci-test -j4` for focused tests. The focused expression is `test(results_restart_tests) | test(trusted_room_tests) | test(net_runtime2) | binary(trusted_room_process)`. Final validation and producing commit are recorded in the PR body.

Coordination readback: the #5 requests about main's elimination serde failure and exact-head validation are acknowledged by retaining #373's fix and validating this branch after merging current main. This stream posts no issue replies. Runtime owner acknowledgment and hosted checks remain the orchestrator's gates.
