# MOV native intent tests

This is the test-only MOV_AUDIT contribution for the fifteen assigned remaining rows. The task-only integration boundary starts after `29ca35e0c266f86317610e9a481bf198be5868c4`. Import only that boundary, not the inherited unpublished branch.

[Axel's approval](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267) applies rules 1 to 5 without added conditions. Behavior qualification requires a native test and prototype-intent review. No new behavior Source recorder, captured fixture or provenance bundle is required or introduced. This test PR changes no parity file, status, runtime rule, contract or reference pin.

The mappings and results below moved from this task's additions to movement.md. The parity file is restored byte-for-byte to the pre-task parent, preserving all inherited authors' content. Prototype references use pinned `e4e579a`.

| Row | Literal native tests | Prototype reference |
| --- | --- | --- |
| `MOV-11` | `movement::mov_audit::mov11_walk_budget_waypoints_backing_block_and_yield`, `movement::mov_audit::mov11_bridge_groups_finish_all_widths_and_order_variants_with_deferred_recovery`, `movement::mov_audit::mov11_separation_checks_protected_cover_at_the_actual_displacement_phase` | movement.js:72-88 |
| `MOV-23` | `movement::mov_audit::mov23_powered_vehicle_displacement_respects_reduced_step_budget` | movement.js:39-40 |
| `MOV-24` | `movement::mov_audit::mov24_stall_recovery_kind_cooldown_and_post_separation_progress`, `movement::mov_audit::mov11_bridge_groups_finish_all_widths_and_order_variants_with_deferred_recovery` | sim.js:5496-5503, 5537-5543, 5609-5612 |
| `MOV-25` | `match_firing::mov_audit::mov25_stopped_hull_prioritizes_visible_antitank_then_recent_fire_then_target_then_order` | sim.js:4077-4081, 5547-5555, 4015-4018 |
| `MOV-26` | `match_firing::mov_audit::mov26_hurt_out_of_range_vehicle_backs_front_first_except_at_capture_point` | sim.js:4106-4110; CFG sim.js:136 |
| `MOV-27` | `pathfinding::mov_audit::mov27_exact_click_requires_full_navigation_radius` | vehicle-motion.js:18, 122-136; sim.js:3094-3095 |
| `MOV-28` | `pathfinding::mov_audit::mov28_bridge_width_and_incremental_clearance_cache` | vehicle-motion.js:137-188; sim.js:3085 |
| `MOV-29` | `pathfinding::mov_audit::mov29_observed_and_world_route_views_ignore_unreported_terrain_and_mines`, `pathfinding::mov_audit::mov29_match_selects_player_memory_and_world_team_memory_after_hidden_edits` | sim.js:3068-3083, 1509-1573, 1602-1662 |
| `MOV-30` | `pathfinding::mov_audit::mov30_bank_step_back_restores_walkable_first_leg_and_arrival` | sim.js:3072-3081 |
| `MOV-33` | `pathfinding::mov_audit::mov33_score_only_heap_ties_and_road_heuristic_lower_bound` | sim.js:3037-3065, 3107, 3134-3136, 739 |
| `MOV-35` | `pathfinding::mov_audit::mov35_smoothing_preserves_roads_wire_mines_and_bunker_clearance` | sim.js:3088, 3170-3181 |
| `MOV-36` | `pathfinding::mov_audit::mov36_disconnection_rejects_before_search_and_edits_reconnect` | sim.js:2907-3015, 3124-3128 |
| `MOV-37` | `pathfinding::mov_audit::mov37_coarse_routes_update_from_history_and_survive_cache_eviction` | navigation.js:1-293 |
| `MOV-38` | `pathfinding::mov_audit::mov38_small_maps_have_full_routes_world_and_large_maps_limit_to_32_cells` | navigation.js:347-389; sim.js:3093, 3169 |
| `MOV-40` | `pathfinding::mov_audit::mov40_budget_fifo_stale_dead_and_immediate_player_orders` | sim.js:3216-3254, 5337 |

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
