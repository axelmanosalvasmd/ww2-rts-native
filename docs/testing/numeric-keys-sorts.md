# IF-25 numeric sort audit

Reference: `e4e579a`. The inventory has 23 `.sort` expressions in `shared/sim.js`, 28 in `shared/ai.js` and 9 in `shared/supply-convoys.js`. This audit covers numeric comparison semantics in existing native consumers. It does not claim that every feature containing a prototype sort has a native runtime planner.

`core/src/sort.rs` runs the existing V8 TimSort with a numeric result: negative sorts before, positive sorts after, and zero or NaN compares equal. `sort_by_ref` preserves the same comparison sequence for owned records. `first_by` sorts before selecting, including NaN cases where a minimum scan can disagree. `then` implements numeric `a || b`, including NaN falling through to the second expression. No extra unit-ID fallback is added unless the prototype has one.

## Simulation consumers

All paths below are under `rust/crates/core/src/`. Prototype lines refer to `shared/sim.js`.

| Prototype line | Native consumer or boundary | Comparison |
| --- | --- | --- |
| 509 | `field_projects.rs`, entrench plan | Distance then explicit original-index fallback |
| 1209, 1210 | `match_world.rs`, guard fortification | Finite nearest home scan; stable descending region distance. World admission remains closed |
| 1256 | `match_world_rails.rs`, `WorldRails::route` | Stable descending queue cost, then pop. Equal cost selects the last queued entry |
| 1697, 1699 | `elimination_modes.rs`, node setup | Stable enemy-spawn distance; ascending seat pair uses integer min/max |
| 1727 | `match_ground_orders.rs`, convoy retreat | Stable source-store distance |
| 1728 | `match_reactions.rs`, `ground_home` | Strict smaller-distance scan preserves first tie on validated finite positions |
| 1997 | `match_withdrawal.rs` | Stable store distance, preserving insertion order |
| 2460 | `match_garrison.rs`, `garrison_entry_cell` | Stable distance preserves breadth-first house edge order |
| 2622 | `environment_lifecycle.rs` | Stable section distance preserves authored section order |
| 3283 | No native Depot-node placement selector | `match_construction.rs` keeps Depot-node placement behind its separate provider boundary. This consumer is not proven |
| 3533 | `match_carriers.rs` | Stable squad/carrier pair distance |
| 3549, 3551, 3559 | `match_world_rails.rs`, `apply_rail_order` | Stable origin, destination and passenger distance. World admission remains closed |
| 3809 | `production.rs`, construction queue selection | Strict shorter queue scan on integer lengths keeps first equal queue |
| 3869 | `match_abilities.rs` | Stable grenade-target distance |
| 4032 | `command_destinations.rs` | Stable projected line position |
| 4736 | `match_combat.rs` | Stable surviving structural-section distance |
| 4791 | `environment.rs` | Stable descending fire-cell weight |
| 5801 | `elimination_modes.rs`, transfers | Reuses the sorted teammate array across transfers, so later equal distances retain earlier selection order |
| 5859 | `observed_memory.rs` / `world.rs` terrain replay | Integer journal sequence and ordered pending indices. No floating comparator |

## AI consumers

Prototype lines refer to `shared/ai.js`. Native files use the same core path prefix.

| Prototype line | Native consumer or boundary | Comparison |
| --- | --- | --- |
| 45 | AI remembered-bridge planner absent | Ascending integer cell sort belongs to that absent planner; no consumer is claimed |
| 65, 72, 113, 132 | Bridge rebuild, fill and mine-clear planners absent | These feature planners need separate implementation; no consumer is claimed |
| 316 | `commander_classic_rules.rs`, `targets`; `horde_commander.rs` observed bunker | Stable target distance |
| 367 | `commander_tactics.rs`, held-ground defense | Distance then prototype's explicit unit-ID fallback |
| 450 | `commander_tactics.rs` visible enemy traversal | Delivered vector order. No numeric reordering of radius results |
| 555, 562, 564 | `commander_classic_rules.rs` recon, depot raid, raiders | Stable distance; raiders use armored-car rank then distance without an ID fallback |
| 579 | `commander_tactics.rs` guard dispatch | Stable responder distance |
| 596, 599 | Aircraft attack/fighter planners absent | These feature planners need separate implementation; no consumer is claimed |
| 634 | AI rail-hop planner absent | World admission remains closed; authoritative rail orders have the simulation mapping above |
| 686, 701 | AI World region/base planner absent | World admission remains closed |
| 726 | Point-holder fortification planner absent | This feature planner needs separate implementation; no consumer is claimed |
| 756 | `commander_tactics.rs` base attack | Reuses the sorted target array across units, preserving previous sort order for later ties |
| 884, 888 | `commander_construction.rs` | Stable builder distance; repair uses unfinished rank then distance |
| 921, 926 | `commander_fortification.rs` | Stable enemy spawn and crew distance; the crew array remains sorted between HQs |
| 973 | `commander_tactics.rs` / `commander_ai.rs` research | Strict minimum integer technology level in authored line order |
| 992, 1000 | `commander_forward_hq.rs` | Stable HQ and builder distance in delivered order |
| 1024, 1027 | Classic repair/node planners absent | `commander_construction.rs` returns its budget plan before Classic actions; no consumer is claimed |

The native coordinated-assault responder sort in `commander_assault.rs` also keeps delivered ties. `commander_mind.rs` retains its existing ID fallback for the separate loss-lesson selector; that selector is not one of these 28 expressions.

## Supply consumers

Prototype lines refer to `shared/supply-convoys.js`; all nine consumers are in `supply_convoys.rs`.

| Prototype line | Native function | Comparison |
| --- | --- | --- |
| 92 | `sources` | Stable own-owner rank; boolean keys cannot produce NaN |
| 196 | replenishment nearby-store selection | Stable distance |
| 246 | `targets` troop grouping | Stable provisions |
| 265 | `targets` final ordering | Stable priority |
| 272 | `origins` | Stable distance |
| 314 | `finish` origin selection | Stable source distance after own-owner ranking |
| 345 | `deliver` | Stable member provisions |
| 478 | withdrawn truck source selection | Stable source distance |
| 616 | convoy retreat command | Stable source distance |

## Focused evidence and limits

- `sort::if25_tests::if25_numeric_difference_matches_pinned_prototype_for_zero_nan_and_infinity` checks signed-zero ties and mixed NaN/infinity ordering for copied and owned values.
- `sort::if25_tests::if25_nan_merge_order_matches_pinned_prototype_above_one_run` checks a 96-entry merge case.
- `sort::if25_tests::if25_finite_consumer_keys_compare_without_nan_and_keep_ties` asserts zero NaN comparisons for its finite fixture. It is not a global runtime NaN counter.
- `sort::if25_tests::if25_sorted_first_and_chained_comparators_follow_v8_nan_and_stable_ties` checks sorted-first selection and JavaScript fallback behavior.
- `supply_convoys::if25_tests` checks actual job and source consumers with ties and NaN.
- `commander_construction::tests::if25_construction_builder_ties_keep_available_order_and_repairs_keep_observation_order` reverses the separate available-builder slice, while equal-distance repairs retain canonical observation order. The observation remains valid and sorted by unit ID.
- `commander_forward_hq::tests::if25_forward_hq_builder_ties_keep_available_order` reverses the separate available-builder slice for equal-distance builders, while the observation remains valid and sorted by unit ID.
- `elimination_modes::tests::if25_elimination_reuses_teammate_sort_order_for_later_equal_distance_transfer` first transfers to the later teammate, then checks that an equidistant successor follows the same teammate, with three native Annihilation ticks. Annihilation recipient snapshots remain unsupported; the garrison test below supplies ordinary all-seat delivery evidence.
- `match_world_rails::intent_tests::if25_rail_route_equal_cost_pops_last_stable_todo_entry` changes line insertion order and checks both routes.
- `mov_followup_garrison_tests::if25_garrison_equal_distance_keeps_breadth_first_edge_order_through_match` reserves the requested equal-distance house cell, enters it, runs an ordinary match through victory and ending ticks, and captures snapshots for every seat throughout.

No new runtime state, recipient fields or commands are added by this sorting pass. Tests that intentionally admit component NaN values exercise the required V8 equal-comparison behavior. A blanket debug assertion rejecting NaN would contradict that behavior, so none is added. Observation validation rejects nonfinite AI inputs; the focused finite comparator test checks the zero-NaN diagnostic only for its fixture. Execution results belong to the final producing-head receipt, not this source audit.

IF-25 remains incomplete as a full inventory row while the explicitly absent feature consumers above have no runtime implementation or consumer proof. This pass supplies concrete comparator evidence for existing consumers and does not mark the row ported.
