# MOV native intent tests

This batch starts at main `40fe9e296b46060241693c6c627ecd615d4f8ea4`. It extracts only the owned test delta from `29ca35e0..7e4c66a0`, then adds focused native witnesses for the reviewer expectations. No inherited unpublished history, runtime behavior, parity status or contract changes are included.

[Axel's approved native intent rules](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267) apply. The prototype was read at literal pin `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`, including shared/movement.js, vehicle-motion.js, navigation.js and the routing, reaction and progress consumers in shared/sim.js. No prototype recorder or new captured fixture was run or added.

The six ignored diagnostics from the old batch retain their inputs and assertion bodies. Their outcomes below belong only to `e1c69647`. New tests use separate names when correcting an authored prerequisite. The old bridge journey is retained but contributes no journey proof to this batch.

## Current row map

Test names below are in `mov_audit` child modules of `movement`, `match_firing`, `pathfinding` or `navigation`. The module column makes each literal name fully qualified. Multiple tests cover a row where its behavior spans consumers.

| Row | Module | Enabled covering tests | Prototype at e4e579a |
| --- | --- | --- | --- |
| MOV-11 | `movement` | `mov11_walk_budget_waypoints_backing_block_and_yield`, `mov11_blocked_walk_discards_the_ordered_path_at_the_movement_consumer`, `mov11_blocked_yield_keeps_the_ordered_route_at_the_movement_consumer` | movement.js:75-87 |
| MOV-24 | `movement` | `mov24_stall_recovery_kind_cooldown_and_post_separation_progress`, `mov24_waiting_convoy_does_not_request_stall_recovery`, `mov24_progress_is_measured_after_real_crowd_separation` | sim.js:5496-5503,5537-5543,5608-5612 |
| MOV-25 | `match_firing` | `mov25_stopped_hull_prioritizes_visible_antitank_then_recent_fire_then_target_then_order` | sim.js:4015-4018,4077-4080,5547-5554 |
| MOV-26 | `match_firing` | `mov26_hurt_out_of_range_vehicle_backs_front_first_except_at_capture_point` | sim.js:4088-4109 |
| MOV-27 | `pathfinding` | `mov27_exact_click_requires_full_navigation_radius` | vehicle-motion.js:124-136 |
| MOV-28 | `pathfinding` | `mov28_bridge_width_and_incremental_clearance_cache` | vehicle-motion.js:138-187 |
| MOV-29 | `pathfinding` | `mov29_observed_and_world_route_views_ignore_unreported_terrain_and_mines`, `mov29_match_selects_player_memory_and_world_team_memory_after_hidden_edits`, `mov29_remembered_wall_and_discovered_mine_change_both_route_modes` | sim.js:1510-1538,1602-1662,3068-3083 |
| MOV-30 | `pathfinding` | `mov30_remembered_bank_requires_a_legal_axis_step_before_routing` | sim.js:3072-3081 |
| MOV-33 | `pathfinding` | `mov33_pinned_heap_ties_and_road_scaled_neighbor_scores` | sim.js:3037-3065,3107,3134-3136 |
| MOV-35 | `pathfinding` | `mov35_smoothing_guards_shortcuts_and_keeps_profile_specific_terrain_masks` | sim.js:3088,3170-3181 |
| MOV-36 | `pathfinding` | `mov36_disconnection_rejects_before_search_and_edits_reconnect`, `mov36_directed_hill_corner_is_walkable_from_its_high_endpoint` | sim.js:2907-3015,3124-3128 |
| MOV-37 | `navigation` | `mov37_history_updates_membership_and_bounds_coarse_route_storage`, `mov37_hill_portals_cross_chunks_only_in_the_legal_direction` | navigation.js:15-45,114-128,245-292 |
| MOV-38 | `pathfinding` | `mov38_small_maps_have_full_routes_world_and_large_maps_limit_to_32_cells`, `mov38_bounded_legs_reach_exact_goal_and_small_maps_keep_cost_choices` | navigation.js:347-389 |
| MOV-40 | `pathfinding` | `mov40_budget_fifo_stale_dead_and_immediate_player_orders`, `mov40_multiple_valid_waiters_precede_new_work_and_real_move_bypasses_zero_budget` | sim.js:3230-3254,3457,5337 |

MOV-23 is dropped. Main's movement consumer has no convoy/logistics fuel-step clamp or matching eligibility input. The retained component failure does not establish a connected logistics defect, and implementing that dependency would broaden this test batch into runtime work.

The new tests distinguish the required behavior from the old authored failures: real blocked-route clearing, actual post-separation progress, idle cooldown before and after expiry, radius boundary admission, live bunker refresh with reused clearance, wall/mine discovery in both knowledge modes, a bank that requires a legal first step, pinned heap scores, profile-specific smoothing, directed hill legality, coarse cache membership/storage, successive bounded legs and canonical FIFO/player command dispatch.

## Validation

Independent source review read the pinned prototype and the current assertions and found the intent covered for all fourteen listed candidates. Reviewer execution count: zero. MOV-23 remains excluded.

The first native run selected 24 enabled tests: 22 passed and two new authored checks failed. MOV-33 assumed a particular tied heap neighbor remained queued; MOV-38 incorrectly interpreted 32 diagonal-capable cell steps as a 64 m Euclidean bound. The corrected checks compare every retained neighbor against literal pinned scores and bound displacement on each cell axis. These were test-premise errors, not demonstrated runtime defects. The original result log is retained at `/tmp/mov-audit-batch1-first-results.log`.

Final focused run on Rust tree `24fdbcc36fc42c5c6d02d023d7c2eeed67c382ab`: 25 tests passed, zero failed, 297 skipped, with retries disabled. This is the filter `test(::mov_audit::)` in package `ww2-core`, using the pinned 1.99.0 toolchain and ci-test profile on the Vostro. The six retained ignored diagnostics did not run. Raw output: `/tmp/mov-audit-batch1-final-tests.log`.

Formatting passed. Initial strict Clippy found a byte-character-array style warning; the equivalent byte string fixes it, and all 25 focused tests passed again on the final Rust tree. Final strict workspace all-target Clippy passed with `-D warnings`. Formatting, tests and Clippy ran inside locked S1 scopes with MemoryMax=6G, MemorySwapMax=0, the slot target directory and at least 20 GiB free disk. Each slot lock was released when its command exited. No parity status changes here.

## Historical evidence

### Native intent batch e1c69647 (2026-10-16)

One Root-granted nextest invocation selected all 18 tests: 12 passed, 6 failed and 147 were skipped. Every selected test completed once with `--no-fail-fast --retries 0`. The producing commit is `e1c69647f941740ab7afeae5fc0af3a350946970`, Rust tree `0471a9ef38d69ef544be9de4c11a06cbb907eb1a`. The process and systemd launcher terminated with exit 100, and the inherited S3 flock was released before this row report. Normal output is retained at `/tmp/native-feature-batches-0315/mov_audit/native.log`; terminal metadata is in `RESULT.json` and `LAUNCH-RESULT.json` alongside it. These local paths are run output, not committed test inputs.

At the producing source head, all fifteen rows were in progress. This test-only split restores their pre-task statuses and promotes no row. Eight have passing native tests ready for prototype-intent review; seven are excluded for the reasons below. No failed assertion or input was repaired, no test was retried, and the earlier Source failures remain unchanged.

| Row | Native outcome | Promotion boundary |
| --- | --- | --- |
| `MOV-11` | Walk/backing/block/yield and protected-cover separation passed. Bridge matrix failed before its first scene. | Held: the authored central capture point is on river terrain, rejected as `points: must be on open ground`. No bridge journey ran. |
| `MOV-23` | Reduced powered-step assertion failed. | Held: the component truck has no enabled logistics context. This is not proof of an ordinary runtime defect; the logistics clamp and connected runner remain incomplete. |
| `MOV-24` | Stall recovery and post-separation progress passed. | Held: the mapped bridge/deferred-recovery journey did not run because of MOV-11's scene admission failure. |
| `MOV-25` | Hull priority, ordered facing and wall refusal passed. | Prototype-intent review pending. |
| `MOV-26` | Actual-damage backoff, front-first motion, timer and capture exception passed. | Prototype-intent review pending. |
| `MOV-27` | Clear exact click, blocked-radius snap and edge margin passed. | Prototype-intent review pending. |
| `MOV-28` | Narrow/wide bridge navigation and incremental/fresh clearance passed. | Prototype-intent review pending. |
| `MOV-29` | Both query checks and actual ordinary/World team-memory selection after hidden edits passed. | Prototype-intent review pending. |
| `MOV-30` | Authored bank prerequisite failed. | Held: the initial native leg is already walkable, so this scene does not exercise the intended fallback. |
| `MOV-33` | Score-only heap tie check passed, then expansion-count prerequisite failed (actual 2, expected 1). | Held: the heuristic checks did not execute. The authored count is not a demonstrated runtime defect. |
| `MOV-35` | Open-ground, mine and bunker checks passed, then a wire segment failed the blanket 2 m ray assertion. | Held: the test imposes a stronger condition on every adjacent segment than the smoother's shortcut guard. Later road/mud/naval checks did not execute. |
| `MOV-36` | Disconnected rejection, edit reconnection, fresh rebuild and hill cases passed. | Prototype-intent review pending. |
| `MOV-37` | History closure and reconnection passed, then raw local region IDs differed. | Held: incremental `[11,22,23]` and fresh `[11,12,13]` both produced goal 4842 and `limited=true`. Raw local IDs are not stable across rebuilds; the cache-eviction tail did not run. |
| `MOV-38` | Small-map full routes and World/large-map 32-cell limits passed. | Prototype-intent review pending. |
| `MOV-40` | FIFO, zero budget, dead/stale disposal and synchronous command-route checks passed. | Prototype-intent review pending. |

The six failed tests remain enabled at the producing source head. The later source-only test-PR split marks exactly those six diagnostics ignored with their actual failure reasons and preserves every body, input and assertion. The twelve passing cases remain enabled. No executable job has run on that later Rust tree, so it has no new default-suite green claim.
