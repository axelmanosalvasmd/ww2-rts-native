# MODE runtime B

Baseline: main `9f468518`, prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
The assigned branch was rebased onto current main before editing. No private
runtime history, Source recorder, captured fixture or provenance bundle is imported.

Acknowledged [Axel's native intent decision 6206861267](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267).
The coordinator owns parity status changes, final review, CI, runtime acknowledgment
and merging. All parity tables and agreed interfaces remain unchanged.

## Runtime scope

`Match::new_horde` builds the internal defender/Horde seats, shared deployment,
fortified base and route-based entry gates. Defenders recruit through the same
owner-0 facilities while paying from their own wallets and owning their recruits. `Match::step_horde` is an explicit
late wave phase over canonical Match units, controls, wallets and aircraft.
It owns breaks, announced profiles, seeded bounded purchases, reserves, entries,
wave clearing, support budgets and aircraft spawning. Its deterministic profile
and purchase streams reproduce the prototype's standalone mulberry32 functions.
Native setup retains the core's existing independent startup random streams.

This is a callable wave phase, not a playable all-mode tick or a hosted Horde
match. Ordinary `advance_tick`, aircraft combat/tick services, recipient snapshots
and public server admission keep their guards. Aircraft initialization and the
no-logistics reserve initializer admit the explicit Horde spawn context only.
Boss spawning is preparatory; breath, bounty, scaled damage and public announcement
projection are not claimed.

`scenario_validation::validate_scenario` validates authored schema without
admitting or executing authored missions. It covers stable IDs, all collections,
localized UTF-16 text bounds, authored unit types, targets, conditions, actions,
work bounds and dependency cycles. Existing map admission is preserved.

Shared story services now recognize each consistent mode tag and report its
correct name. Classic timeline samples count all living buildings. Supporting
tests cover counters, fallen-location memory, explicit finish positions and
sampling. These do not complete MODE-91/92/93.

## Proven rows

All eleven rows below pass native intent checks. PR review and runtime
acknowledgment remain with the coordinator. Test names are relative to
`match_horde::tests` unless otherwise stated; all MODE-80 names are under
`scenario_validation::tests`.

| Row | Intent test |
|---|---|
| MODE-42 | `horde_setup_builds_one_fortified_shared_base_and_internal_enemy`; `mode42_defenders_recruit_from_shared_facilities_with_their_own_wallet_and_army`; `mode42_gates_follow_sixteen_metres_of_medium_route_including_multiple_legs` |
| MODE-43 | `mode43_break_starts_announced_seeded_wave_and_retains_two_profiles`; `mode43_and47_seeded_ordinary_wave_has_the_announced_exact_purchase_sequence` |
| MODE-46 | `mode46_complete_unlock_weights_and_late_heavy_profile_bias` |
| MODE-47 | `mode47_profile_purchases_bound_work_memory_and_unaffordable_remainder`; `mode43_and47_seeded_ordinary_wave_has_the_announced_exact_purchase_sequence` |
| MODE-48 | `mode48_reserve_enters_last_first_on_tenth_ticks_with_stars_and_field_cap` |
| MODE-49 | `mode49_left_counts_live_ground_reserve_and_budget_then_clears_planes_and_heals` |
| MODE-50 | `mode50_support_wallet_replaced_at_each_wave_scaled_by_defenders_and_army` |
| MODE-51 | `mode51_planes_start_at_ten_scale_to_eight_every_third_fighter_and_do_not_count_left` |
| MODE-80 | `scenario_validation::tests::mode80_migration_preserves_authored_definitions_and_legacy_actions`; `mode80_collection_limits_and_stable_ids_are_exact`; `mode80_localized_text_and_recipient_side_rules_are_bounded`; `mode80_authored_units_share_global_ids_and_128_limit_and_exclude_non_ground_types`; `mode80_capture_ids_default_to_points_and_reject_collisions_with_regions`; `mode80_all_condition_references_counts_depth_and_repeat_limits_are_checked`; `mode80_actions_rosters_boxes_and_total_tick_work_use_exact_limits`; `mode80_dependency_graph_rejects_direct_indirect_and_objective_cycles`; `mode80_invalid_condition_and_action_fields_fail_without_executing` |
| MODE-89 | `lobby::native_tests::mode_server_lifecycle::mode89_host_holds_for_120_callbacks_with_60_frames_and_team_or_draw_outcomes` |
| MODE-95 | `lobby::native_tests::mode_server_lifecycle::mode95_disconnect_propagates_each_tick_freezes_income_and_vp_and_refuses_commands` |

The Horde checks exercise a real constructor and explicit wave calls. They
stage wave numbers to reach unlock, veterancy and aircraft boundaries without
claiming combat survival through those waves. Purchasing includes a fixed
seeded sequence and a witness that distinguishes weighted from uniform picks.
The gate witness includes a tank-trap band that separates the medium and infantry
route prefixes. Tests cover one through five defenders, the 60/240 field caps, the 32-purchase
and 240-reserve limits, all 19 unlocks, faction/limit bypass and plane cleanup.

Server tests drive the real Hub poll loop, commands, disconnect, pause expiry
and reconnect. They check all 120 ending callbacks, 60 steps and frames, final
frame ordering, nontrivial team IDs and draw outcomes. Away-seat tests separately
prove refusal of already queued commands and new ingress, frozen income/VP,
and recovery after reconnect.

## Dropped rows

All 32 assigned rows are accounted for: eleven proven and 21 dropped this batch.
No deliberate gameplay difference is proposed.

| Row | Reason |
|---|---|
| MODE-41 | Needs contract: Horde server admission and fresh authored-map reload remain guarded. |
| MODE-44 | Needs contract: public `nextwave` remains unavailable. |
| MODE-45 | Profile generation is covered, but break-only snapshot publication is not connected. |
| MODE-52 | Kaiju breath, scaled damage, bounty and public event projection are incomplete. |
| MODE-53 | Horde reveal is not connected to the recipient/fog path. |
| MODE-54 | Horde no-income/bounty and complete defeat/result lifecycle are unqualified; hosted lifecycle needs contract. |
| MODE-55 | Needs contract: Horde persistence, lobby record and result fields remain absent. |
| MODE-56 | Needs contract: Horde server seat admission and scheduling remain absent. |
| MODE-79 | Needs contract: all-mode triggers require the excluded World-admission change. |
| MODE-81 | Authored scenario setup and server rollback require general scenario admission. |
| MODE-82 | General contexts, repeat scheduling and atomic batch execution remain absent. |
| MODE-83 | General runtime condition evaluation remains absent. |
| MODE-84 | General action execution and arrival queuing remain absent. |
| MODE-85 | General arrival placement, retry, limits and expiry remain absent. |
| MODE-86 | General objective state and recipient filtering remain absent. |
| MODE-90 | Needs contract: the serializer is Conquest-only; Horde result and host `end` remain unavailable. |
| MODE-91 | Counter-service coverage does not wire missing cancellation/refund and World recovery spending events. |
| MODE-92 | Fallen-location service coverage does not supply the missing Classic/Annihilation ending callers. |
| MODE-93 | All-mode sampling services do not supply the missing mode runner schedules. |
| MODE-94 | Needs contract: public restart remains guarded and trusted restart deliberately repeats its fixed seed. |
| MODE-96 | Needs contract and missing modes: complete Horde/Tutorial and other mode snapshots remain guarded. |

## Prototype review and validation

Read pinned `shared/sim.js:843-969`, `:5765-5785`, `:5810-5815`,
`shared/scenarios.js:2-81`, `shared/story.js:5-70`, and server ending/away
callers `server.js:742-784`. Review found and resolved the Horde aircraft reserve
initializer's Conquest-only guard before native execution. A proposed generic
reason-based finish fallback was removed during review: Source finish always
uses the supplied position or map centre, while mode callers choose the fallen
bunker or production building themselves.

The first compile stopped on an unparenthesized expression in a server test JSON
macro. After correction, the first executed selection passed all ten Horde and
three story-service tests, then stopped at a schema test whose invalid explicit
side expected the later reinforcement error. Source checks the action recipient
first; only that expected message was corrected. Ten tests were unrun in that
stopped selection. A later shared-recruitment extension adds MODE-42 coverage.
The next selection passed 25/26 tests. The away test repeated its 100 ms host
clock input, so one intended callback never ran and reconnect reached tick 12
instead of 13. The fixture now advances each declared callback and asserts the
simulation tick at every away iteration. No server runtime change was needed.
Final qualification passes on the committed Rust contents: all 26 focused
library tests, 22 retained core/simtest integration checks, `cargo fmt --all --check`,
and `cargo clippy --locked --workspace --all-targets -j4 -- -D warnings`.
Nextest used the `ci-test` Cargo profile, four build jobs and zero retries.
The Vostro run held slot S3 with a 6 GiB memory cap, zero swap and only its
assigned target directory. The final script exited 0 and released S3.
Logs are under `/tmp/mode-runtime-b-20261016/`.
No whole-workspace test run, Source execution, browser or Unreal test is claimed.
