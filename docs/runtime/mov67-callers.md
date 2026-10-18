# MOV-67: non-World placement callers

This branch stacks on #404 (movement ticks), #393 (sandbox) and #386 (scenario engine). It merges after #373. The caller integrations are needed because #404 had neither sandbox spawning nor authored reinforcement execution. The complete row is large enough for one focused PR; no additional row is claimed.

The reference remains `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Prototype `shared/sim.js:1769-1772` assigns relocation fields and immediately updates the spatial index. Native existing actors use `unit_placement::place_unit`; new actors use `StagedUnit` and canonical publication, which commits their final pose and all index memberships together. Neither path needs an intermediate scatter pose in the live index.

## eachrowliteraltestmapping

MOV-67 (non-World):

| Caller and prototype anchor in shared/sim.js | Executable witness |
| --- | --- |
| Sandbox own/enemy spawns, 4354 | `match_sandbox::tests::mov67_sandbox_placements_index_every_spawn_kind_before_ticks_and_delivery` |
| Authored setup and successive hold/move/attackMove arrivals, 4908 | `authored_scenario::engine_tests::mov67_scenario_setup_and_successive_arrivals_index_resolved_poses_before_ticks` |
| Bunker relocation, 986; finished buildings, 1027 | `unit_placement::callers::mov67_bunker_and_building_publication_are_indexed_before_the_next_tick` |
| Unfinished building publication, 1027 | `match_construction::tests::mov67_unfinished_building_publication_indexes_the_footprint_pose_immediately` |
| Base and rearming aircraft, 2202 | `unit_placement::callers::mov67_base_and_rearming_planes_reindex_both_offmap_and_airfield_poses` |
| Operative respawn at led squad, 2174 | `unit_placement::callers::mov67_operative_squad_respawn_publishes_the_squad_pose_without_a_tick_rebuild` |
| Ground production step-out, 1096 | `mov_runtime_batch::mov69_actual_recruitment_uses_free_ring_exits_and_spawn_ghosts` |
| Naval production step-out, 1096 | `mov_runtime_batch::mov104_naval_purchases_require_sea_launch_from_shipyard_and_respect_destroyer_max` |
| Aircraft production at maker, 1074 | `mov_runtime_batch::mov51_aircraft_purchases_stay_at_maker_with_full_reserves_and_no_rally_walk` |
| Garrison entry and exit, 5437/2420 | `match_carriers::tests::intent_mov70_squad_enters_house_at_door_then_exits_to_free_cell` |
| Trapped setup relocation, 1598 | `match_carriers::tests::intent_mov70_destroyed_door_cancels_entry_and_setup_escapes_blocked_cell` |
| Rider following pose and rotation, 5359 | `match_carriers::tests::intent_mov74_rider_follows_carrier_pose_and_skips_ordinary_turn` |
| Normal/forced carrier exit, 2428 | `match_carriers::tests::source_commands_boarding_riding_and_unload_match_each_checkpoint`; `intent_mov76_wreck_throws_rider_out_hurts_it_and_deep_water_drowns_it` |
| Paradrops, 5261 | `match_support::cmb66_tests::mov67_paradrop_publication_keeps_roster_poses_and_all_grid_memberships_current` |
| Tutorial house placement, shared/tutorial.js via the same helper | `match_tutorial::tests::mode68_tutorial_house_garrisons_and_artillery_cooldown_advances` |
| Field assignment and query semantics of the common helper, 1772 | `unit_placement::tests::mov67_source_placements_keep_fields_and_every_grid_query_current` (existing component fixture) |

The six new witnesses compare the live grid with a fresh rebuild immediately after actual caller execution, before a later tick can repair an omitted update. The comparator checks owner membership, ordered/unordered bucket membership, heavy membership, existence, radius membership and nearest distance across the map and off-map aircraft buckets. Different first-seen bucket traversal order is not treated as missing membership. Existing component coverage retains exact prototype query-order assertions.

Sandbox checks every non-structure type for both own and enemy spawns, including aircraft and idle trains. Scenario checks sequential occupancy, resolved positions and all three arrival orders. Bunker/building and aircraft checks deliberately move between different buckets. The operative case checks the complete squad pose, owner and one-soldier HP debit with zero MP.

The sandbox, scenario, bunker/building and aircraft witnesses continue ordinary ticks and capture every seat without faults. The scenario witness reaches VP victory and ending ticks. Operative placement is an existing core component here; its ordinary recipient delivery remains owned by #367, which is not imported or claimed. This PR introduces no operative state, new wire fields or ABI changes.

## Deferred clause

World neutral-region guards (`sim.js:1191`), World rail stop unloading (`1295`) and automatic train placement (`3558`) require running World admission. Owner decision C keeps that admission closed for R1. This PR proves only the non-World portion of MOV-67; it does not promote the complete row or alter parity-status tables.

## Merge and validation

Every manual merge resolution is recorded in [mov67-callers-merges.md](mov67-callers-merges.md). Sandbox and scenario contract entries, existing privacy checks and the shared authored-v2 save schema remain in place. Owner decisions A-P in #5 comments 6217590741 and 6218552386 are acknowledged; exact-head runtime acknowledgement remains the integration coordinator's gate.

Validation results are recorded in the PR at their producing head. No Source recorder, new fixture bundle, Unreal execution or World-room execution is included.
