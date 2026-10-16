# Logistics runtime component checks, batch 4

Base: `9fe25aa829d980bf985eedad4db2537d08a8949b`. Prototype: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

This batch checks the existing convoy scheduler and remembered-terrain route provider. All Rust additions are behind `cfg(test)`. It introduces no shipping field, flag, command, event or link. The full-match survival rule therefore has no new state to admit: ordinary logistics ticks still reject in `match_tick.rs::validate_tick_input`, and complete logistics snapshots still reject in `recipient_snapshot/composition.rs::validate_active_lists`. These tests do not enable logistics rooms or establish full-match support.

The tests call the production job selector, scheduler, state machine and route provider. The scheduler harness supplies explicit route and handoff results, applies typed movement operations, and retains actor insertion order. It does not simulate ordinary unit movement. The route tests separately exercise the real terrain and pathfinding provider with caller-supplied remembered terrain and visible armed-threat facts.

| Row | Native tests | Prototype read scope |
| --- | --- | --- |
| LOG-33 | `supply_convoys::intent_tests::intent_log33_jobs_group_urgent_troops_reserve_members_and_fill_remote_stores` | `shared/supply-convoys.js:238-266` |
| LOG-35 | `supply_convoys::intent_tests::intent_log35_dispatch_carries_origin_search_and_later_players_with_shared_budget`; `intent_log35_cycle_counts_jobs_before_fleet_and_keeps_unfinished_queues` | `shared/supply-convoys.js:395-454,507-519` |
| LOG-36 | `supply_convoys::intent_tests::intent_log36_collect_load_unload_retarget_and_return_conserve_cargo`; `intent_log36_group_priority_handoff_capacity_and_staggered_waiting` | `shared/supply-convoys.js:275-360,507-547` |
| LOG-37 | All three `supply_routes::intent_tests::log37_*` tests; `supply_convoys::route_intent_tests::intent_log37_automatic_same_cell_failures_back_off_to_thirty_two_seconds_and_keep_cargo` | `shared/sim.js:1810-1944`; `shared/supply-convoys.js:219-236` |
| LOG-38 | `supply_routes::intent_tests::log38_continuation_checks_every_remaining_leg_for_known_obstacles_and_visible_weapon_radius`; all three `supply_convoys::route_intent_tests::intent_log38_*` tests | `shared/supply-convoys.js:459-505,529-534`; `shared/sim.js:1946-1965` |

LOG-33 covers grace and reserve thresholds, grouping, priority, job reservations before cargo exists, and finite-store demand. LOG-35 covers dispatch timing, demand before fleet growth, bounded origin search, carry-over and service of later players. LOG-36 covers loading, incremental unloading, scarce-cargo ordering, handoff refusal, moved recipients, capacity scaling, return surplus, staggered retries, manual completion and held state. LOG-37 covers per-owner threat epochs and knowledge invalidation, short hops, danger clearance, disconnection, long-route leg joining and automatic retry backoff. LOG-38 covers destination tolerance, safety-hook admission, every remaining route leg, bounded detour repair and stuck arrival near a source.

`blocked` is supported route-hook failure vocabulary. Both the prototype `supplyRoute` and native `SupplyRouting::route` directly classify only `dangerous` and `unreachable`; the scheduler test checks all three supplied failure reasons. Visibility filtering remains the existing Match adapter's responsibility.

LOG-39, LOG-43, LOG-45 and LOG-46 remain for the next batch under step 7's five-row limit. This batch leaves the parity tables unchanged and does not depend on open PRs #322 or #326. No Source recorder or new capture is added. No design decision, ABI, header, wire format, contract or privacy rule changes.
