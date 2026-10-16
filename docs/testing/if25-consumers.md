# IF-25 ordinary consumers

This stream stacks on #416 at `9662e817dea43983459cd0f17fb02b0d42a626cb`. It completes the ordinary consumer inventory left in [the predecessor record](corememory-if25.md). IF-25 has 60 prototype sort expressions: 17 ordinary simulation, 25 ordinary AI, nine supply, and nine deferred World expressions. No extra UNASSIGNED row was taken because this row required runtime work across construction, engineering, aircraft and point holding.

The reference is `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`, inspected read-only in the canonical `t3-29229094/reference/ww2-rts` checkout. No prototype source was changed.

Acknowledged locally: #5 message `julio-ack-batch1-20261016`, owner confirmation `axel-owner-confirm-a-to-p-20261016`, and the IF-25 request for actual consumer witnesses, especially repeated regroup/crew ties. No issue comment is posted under the stream brief. Receipt does not imply Hermes' exact-head runtime acknowledgement.

## Scope and behavior

New ordinary consumers rebuild remembered bridges, fill spoiled ground, clear detected mines, select ready aircraft targets, fortify point holders, place Depots on canonical nodes and reserve richness-ranked nodes for Classic engineers. The internal observation carries public node richness, recipient-known claims and terrain, and own-aircraft readiness. The existing command and recipient wire shapes are unchanged. Runtime AI observations accept canonical finite off-map aircraft homes; ground positions, paths and quiet observations retain their existing bounds. New private policy fields have serde defaults, and direct continuation witnesses retain pending engineering jobs and engineer reservations.

Three existing ordering errors are repaired: remembered Classic buildings preserve discovery order, Classic resource pairs retain first discovery order, and fortification sorts its crew array even at an HQ whose next step is skipped. Depot claim publication follows successful construction; cancellation, retirement and elimination release claims.

The existing V8 numeric-difference sorting policy remains authoritative. Zero signs and NaN compare equal; chained numeric keys preserve prototype fallthrough. Infinities can legitimately produce NaN differences, so the comparator does not add a blanket panic on NaN. Constructor refusal tests cover nonfinite delivered state and the finite comparator fixture reports no NaN comparisons. No Source recorder is added.

The detached human planner emits delayed attempts and receives no synchronous canonical denial receipt. Its bridge/fill/demine pending jobs use the original 90-second expiry and observed digging/completion. A refused visible attempt cannot immediately trigger the prototype's approach fallback. Point-holder denial fallback is also outside this sort-consumer claim. These limits do not claim completion of broader AI behavior rows.

## Each literal consumer

Each named test exercises the actual producer or consumer. Earlier tests are inherited source mappings until included in this branch's recorded execution below. Tables list prototype lines at the pinned revision; native paths are retained in the predecessor inventory.

### Simulation: `shared/sim.js`

| Line | Consumer | Direct witness or deferred boundary |
| --- | --- | --- |
| 509 | `src/field_projects.rs::entrench_plan`: distance then original segment index | Direct: `if25_sim_entrench_sort_matches_original_ties_and_rejects_nan` |
| 1209 | `src/match_world.rs`: nearest home for neutral region | Deferred: neutral-region startup and World admission. World room admission remains closed. |
| 1210 | `src/match_world.rs`: descending neutral-region distance | Deferred: guard fortification setup. World room admission remains closed. |
| 1256 | `src/match_world_rails.rs::WorldRails::route`: descending queue cost, then pop | Deferred: rail routing; existing component `if25_rail_route_equal_cost_pops_last_stable_todo_entry` is retained. World room admission remains closed. |
| 1697 | `src/elimination_modes.rs`: enemy spawns by distance, retaining player order | `if25_classic_node_pairs_keep_enemy_ties_and_first_discovery_order` |
| 1699 | `src/elimination_modes.rs`: ascending two-seat pair | `if25_classic_node_pairs_keep_enemy_ties_and_first_discovery_order` |
| 1727 | `src/match_ground_orders.rs`: convoy retreat home from active source insertion order | `if25_match_convoy_retreat_equal_distance_keeps_active_source_insertion_order` |
| 1728 | `src/match_reactions.rs::reaction_home`: nearest own finished HQ, strict smaller-distance scan | `if25_reaction_home_equal_distance_keeps_first_finished_own_hq` |
| 1997 | `src/match_withdrawal.rs`: nearby stores retain store insertion order | `if25_match_withdrawal_equal_distance_keeps_store_insertion_order` |
| 2460 | `src/match_garrison.rs::garrison_entry_cell`: stable distance in breadth-first edge order | Direct: `if25_garrison_equal_distance_keeps_breadth_first_edge_order_through_match`, including ordinary ticks and every-seat snapshots |
| 2622 | `src/environment_lifecycle.rs::sync_building_sections_emitting`: equal distance retains authored section order | Direct: `if25_building_loss_equal_distance_keeps_authored_section_order`, reversing the canonical section sequence and checking the first actual damage recipient |
| 3283 | `src/match_construction.rs::execute_build`: nearest eligible canonical node, retaining node order | `if25_depot_placement_keeps_canonical_node_ties_through_ticks_and_snapshots` |
| 3533 | `src/match_carriers.rs::carrier_board_command`: distance retains selected squad/carrier Cartesian-product order | Direct: `if25_boarding_equal_distance_uses_selected_squad_and_carrier_order`, four input permutations and nonfinite-input refusal before command mutation |
| 3549 | `src/match_world_rails.rs::apply_rail_order`: nearest origin station | Deferred: rail order admission. World room admission remains closed. |
| 3551 | `src/match_world_rails.rs::apply_rail_order`: destination station by clicked distance | Deferred: rail order admission. World room admission remains closed. |
| 3559 | `src/match_world_rails.rs::apply_rail_order`: passenger distance and car capacity | Deferred: rail boarding admission. World room admission remains closed. |
| 3809 | `src/production.rs::prepare_queue_unit`: strict shorter integer queue scan retains producer input order | Direct: `if25_production_shortest_queue_keeps_producer_input_order_on_ties`, both producer orders and three consecutive purchases |
| 3869 | `src/match_abilities.rs`: nearest covered grenade target in candidate order | `if25_grenade_autocast_equal_distance_keeps_candidate_order` |
| 4032 | `src/command_destinations.rs::end_spots`: projected lateral position | `if25_end_spots_equal_lateral_projection_keeps_request_order` |
| 4736 | `src/match_combat.rs`: nearest surviving structural section in authored order | `if25_explosion_equal_distance_keeps_surviving_authored_section_order` |
| 4791 | `src/environment.rs`: descending building-cell blast weight | `if25_blast_cell_equal_weights_keep_row_scan_order` |
| 5801 | `src/elimination_modes.rs`: reuses the teammate vector after each unit's stable distance sort | Direct: `if25_elimination_reuses_teammate_sort_order_for_later_equal_distance_transfer`; its Annihilation ticks do not claim ordinary recipient support |
| 5859 | `src/observed_memory.rs` and `src/world.rs`: journal/pending cell indices use integer ordering | `if25_observed_pending_replays_log_indices_after_reverse_retouch` |

### AI: `shared/ai.js`

| Line | Consumer | Direct witness or deferred boundary |
| --- | --- | --- |
| 45 | Remembered bridge-cell ordering | `if25_active_field_bridge_ingestion_and_gap_selection_keep_remembered_ties`; fallback `if25_ai_bridge_ingestion_sorts_new_cells_and_preserves_prior_memory_ties` |
| 65 | Nearest safe remembered blown bridge | `if25_active_field_bridge_ingestion_and_gap_selection_keep_remembered_ties`; fallback `if25_ai_bridge_ingestion_sorts_new_cells_and_preserves_prior_memory_ties` |
| 72 | Nearest idle bridge crew | `if25_active_field_bridge_crew_ties_keep_delivered_unit_order`; fallback `if25_ai_bridge_ingestion_sorts_new_cells_and_preserves_prior_memory_ties` |
| 113 | Nearest idle fill crew | `if25_active_field_fill_reuses_earlier_far_cell_sort_for_later_equal_distance`; fallback `if25_ai_fill_repeated_sort_keeps_the_earlier_far_cell_order_on_a_later_tie` |
| 132 | Nearest idle mine-clear crew | `if25_active_field_demine_reuses_earlier_far_mine_sort_for_later_equal_distance`; fallback `if25_ai_demine_repeated_sort_keeps_earlier_mine_order_on_a_later_tie` |
| 316 | `src/commander_classic_rules.rs::targets`; Horde's observed bunker selector | `if25_ai_classic_targets_recon_depots_and_raiders_keep_stable_ties` |
| 367 | `src/commander_tactics.rs`: held-threat distance then explicit ID fallback | `intent_ai29_watched_home_threat_precedes_committed_wait_and_uses_id_ties` |
| 450 | `src/commander_tactics.rs`: visible enemy vector traversal retains observation sequence | `if25_ai_visible_enemy_traversal_preserves_delivered_order_for_guard_value` |
| 555 | `src/commander_classic_rules.rs`: unknown enemy-spawn recon | `if25_ai_classic_targets_recon_depots_and_raiders_keep_stable_ties` |
| 562 | `src/commander_classic_rules.rs`: eligible known depot by spawn distance | `if25_ai_classic_targets_recon_depots_and_raiders_keep_stable_ties` |
| 564 | `src/commander_classic_rules.rs`: armored-car rank then depot distance | `if25_ai_classic_targets_recon_depots_and_raiders_keep_stable_ties` |
| 579 | `src/commander_tactics.rs`: guard responders by distance | Direct: `if25_ai_guard_dispatch_keeps_equal_distance_observation_order`, retaining tied responders and their value budget when an existing order suppresses reissue |
| 596 | Aircraft attack target nearest the front | `if25_ai_aircraft_attack_and_intercept_keep_front_distance_ties_and_ready_gate` |
| 599 | Fighter target nearest the front | `if25_ai_aircraft_attack_and_intercept_keep_front_distance_ties_and_ready_gate` |
| 634 | Rail-hop destination | Deferred: own-station rail planner. World room admission remains closed. |
| 686 | Region movement target | Deferred: World region/base planner. World room admission remains closed. |
| 701 | Known building inside locked target region | Deferred: World region/base planner. World room admission remains closed. |
| 726 | Enemy spawn nearest held point | `if25_ai_holder_spawn_ties_drive_mines_trenches_wire_traps_and_housing` |
| 756 | `src/commander_tactics.rs`: reuses stable target list across attacking units | `if25_ai_attack_targets_reuse_previous_units_sorted_tie_order` |
| 884 | `src/commander_construction.rs`: free builder distance from spawn | Direct: `if25_construction_builder_ties_keep_available_order_and_repairs_keep_observation_order` |
| 888 | `src/commander_construction.rs`: unfinished rank then builder distance | `if25_construction_builder_ties_keep_available_order_and_repairs_keep_observation_order` |
| 921 | `src/commander_fortification.rs`: enemy-spawn distance from each HQ | `if25_ai_hq_threat_and_later_crew_ties_keep_repeated_sort_history` |
| 926 | `src/commander_fortification.rs`: crew distance from each HQ | `if25_ai_hq_threat_and_later_crew_ties_keep_repeated_sort_history` |
| 973 | `src/commander_tactics.rs` and `src/commander_ai.rs`: minimum integer research level in authored line order | `if25_ai_research_equal_levels_keep_authored_line_order`; `if25_quiet_research_equal_armory_levels_select_first_authored_line` |
| 992 | `src/commander_forward_hq.rs`: HQ distance from army front | `if25_forward_hq_nearest_hq_ties_keep_observed_hq_order` |
| 1000 | `src/commander_forward_hq.rs`: available builder distance from site | Direct: `if25_forward_hq_builder_ties_keep_available_order` |
| 1024 | `src/commander_construction.rs`: Classic repair distance from each engineer | Direct: `if25_classic_repair_ties_keep_observation_order_for_each_engineer`, reversing available engineer order independently of equal-distance building observation order |
| 1027 | Richness-adjusted Classic node score, retaining engineer/node assignment memory | `if25_classic_depot_richness_ties_keep_node_and_engineer_order_with_reservations` |

### Supply: `shared/supply-convoys.js`

| Line | Consumer | Direct witness |
| --- | --- | --- |
| 92 | `sources`: own-owner rank, then store insertion order | `if25_convoy_sources_rank_owner_before_preserving_insertion_ties`; excludes inactive, non-source and enemy stores |
| 196 | `local_service`: nearby-store distance before sequential stock use | `if25_convoy_local_service_equal_distance_consumes_first_inserted_store`, both insertion orders; NaN distance fails the radius filter before sorting |
| 246 | `targets`: provisions before troop grouping | `if25_convoy_job_comparators_preserve_zero_ties_and_nan_order`, signed-zero and mixed-NaN identities |
| 265 | `targets`: final priority | `if25_convoy_job_comparators_preserve_zero_ties_and_nan_order` checks emitted job identities after the actual final sort |
| 272 | `origins`: eligible source/store distance | `if25_convoy_origins_keep_insertion_ties_and_nan_distance` |
| 314 | `finish`: fallback sources, own-owner rank followed by stable distance | `if25_convoy_return_refresh_and_retreat_share_stable_source_ties`, both own-source insertion orders after an earlier allied source |
| 345 | `deliver`: member provisions before sequential cargo consumption | `if25_convoy_delivery_sorts_members_by_provisions_and_keeps_signed_zero_ties`, both member orders and an earlier richer member |
| 478 | `refresh`: fallback source when origin disappears | `if25_convoy_return_refresh_and_retreat_share_stable_source_ties`, checking actual origin, destination and returning state |
| 616 | `prepare_movement`: retreat-source route preflight | `if25_convoy_return_refresh_and_retreat_share_stable_source_ties`, checking the actual route target and manual transition |

## Ordinary lifecycle and privacy

- `if25_ai_engineering_remembered_bridge_crew_survives_rebuild_ticks_and_all_seat_snapshots`: actual public bridge destruction and rebuilding, pending/active continuation, completed crew cleanup and every-seat snapshots.
- `if25_ai_engineering_delivered_fill_and_mines_survive_ordinary_ticks_and_all_seat_snapshots`: actual recipient facts, emitted Dig, canonical admission, active and completed work, private-state roundtrip, every-seat snapshots.
- `if25_ai_ready_aircraft_producer_planner_commands_survive_ordinary_ticks_and_all_seat_snapshots`: own Base readiness, actual Attack/Move missions and subsequent snapshots.
- `if25_ai_holder_work_from_delivered_point_survives_ordinary_ticks_and_all_seat_snapshots`: emitted mines command, canonical work state and later snapshots.
- `if25_depot_placement_keeps_canonical_node_ties_through_ticks_and_snapshots`: canonical nodes, both tie orders, payment, own claim, hidden opponent exclusion, cancellation and every-seat snapshots.
- `if25_depot_claim_skips_claimed_nodes_and_releases_on_ordinary_death`: next eligible node, canonical retirement and claim release.
- `if25_classic_ghost_discovery_order_reaches_observation_and_tied_raid_selection`: reverse discovery order through ordinary vision and the actual observation adapter, tied raid selection and duplicate-identity refusal.
- `if25_delivered_engineering_facts_use_remembered_terrain_and_public_map_only`: road rubble, flooded/low ground, dry-cover preservation, known-depot claims and malformed metadata refusal.

World inventory remains deferred: simulation 1209, 1210, 1256, 3549, 3551 and 3559; AI 634, 686 and 701. Detached existing rail tests remain component evidence only. No World room, tick, snapshot or rail-order admission is added.

## Main integration

Main advanced during qualification. Frozen main `9e5bf12439cead3ede90fdf7e62b73fcb7ed1881` adds the full delivered field policy. Complete field packets now use that policy exclusively, preserving its preflight, budget and denial fallback behavior; field-absent component observations use the earlier fallback. The five new active-field witnesses above exercise every engineering sort site on the live dispatcher. Holder ties and lifecycle exercise the active callback as well. The [complete merge record](if25-main-merge.md) explains all nine conflicting files and every additional edit. World components are retained, with no new World admission.

## Validation

The following producing receipt predates the main integration. The PR body supplies the exact integrated head, native selection and results; the earlier pass below does not qualify the merge.

The producing code commit is `e2ce89a9278b39139e11d73969c46d5d6952db0c`, Rust tree `bb8e46950d73774db8a3eb80a3601db427f32912`. This receipt update preserves that complete Rust tree.

```text
cargo fmt --all --check: PASS
cargo clippy --locked --workspace --all-targets -j4 -- -D warnings: PASS
cargo nextest run --locked --cargo-profile ci-test -j4 --no-fail-fast
  -p ww2-core
  -E 'test(if25_) | test(intent_ai29_watched_home_threat) |
      test(intent_ai66_classic_site_draws)'
Summary: 66 tests run, 66 passed, 1162 skipped
SLOT=S1 released, exit=0
```

The checks used pinned Rust 1.99.0, a matching shared slot target, four build jobs, a 6 GiB memory limit and zero swap. Raw logs: `/tmp/if25-final-v7.log` and `/tmp/if25-clippy-v2.log`. The Clippy receipt covers the same Rust tree; its scope started before the code commit. No full-workspace test run was performed.

Earlier broad affected-suite execution at Rust tree `14c7336a2524f2113ef68cad29f715aac9019d86` passed 250 of 251 tests. Its sole failure was AI66's old empty-Depot-proposal expectation. The updated test retains the original RNG assertions and requires the newly implemented build; it passes in the final 66. Earlier compiler/fixture failures remain diagnostic in `/tmp/if25-focused-v*.log`, not passing receipts. The broad run is not claimed as a full pass at this final tree.

Independent source review accepted the 51 ordinary consumer mappings, discovery-order correction, bridge continuation and finite off-map aircraft exception. It ran no native checks. No interface or owner consent is inferred. Current main `b113bb959f95499f04b47213bd27efb805f18e95` is already an ancestor, so the final main merge made no changes or conflict resolutions. The orchestrator owns hosted CI, Hermes' final-head acknowledgement, status cells and merge.
