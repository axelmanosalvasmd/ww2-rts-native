# WEAK-3 engineering follow-up

These tests close coverage gaps from PR #294 using the prototype at `e4e579a`. Runtime code is unchanged. Parity table updates belong to the integration coordinator.

| Row | Tests | Prototype behavior |
| --- | --- | --- |
| ENG-07 | `eng07_bridge_on_dry_ground_is_refused_without_payment_or_work` and retained `eng07_paid_bridge_builds_five_cells_from_the_builders_bank` | `shared/sim.js:431,459-469,3264-3276`: bridge cells require river ground; refusal creates no paid work. |
| ENG-16 | Strengthened `eng16_bridge_preview_masks_sight_and_reorders_a_reversed_span_from_the_bank` | `shared/sim.js:459-469,2533-2551,3264-3276`: exact ordered `(cell, character)` lists for a reversed span with two banks and with only the left bank. Retains unseen-plan and disconnected-plan refusal checks. |
| ENG-42 | `eng42_blast_debits_building_once_at_nearest_cell_strength`, `eng42_blast_overflow_visits_nearest_cells_then_stable_distance_ties`, retained `intent_eng42_direct_building_hits_apply_weapon_slit_and_alliance_rules` | `shared/sim.js:4264-4273,4734-4794`: one nearest-cell-strength budget per building, nearest cells first, stable equal-distance order, overflow into later sections, no fire on K cells. |
| ENG-44 | `eng44_settled_house_rubble_keeps_material_covers_two_cells_and_only_fill_clears_it`, `eng44_house_rubble_skips_live_vehicles_and_ineligible_ground`, `eng44_failed_bridge_settles_without_rubble`, `eng44_dead_building_footprint_becomes_durable_rubble_until_fill` | `shared/sim.js:175,1103-1108,2562-2567,2750-2773,5154-5163`: settlement over open ground, road, crater, mud and existing rubble; material preservation; two-cell fall projection; live vehicle exclusion; bridge exclusion; destroyed building footprint rubble; cover and vehicle blocking with zero HP; persistence through blast and time; Fill clearing. |

The new checks use native command admission, terrain blasts, falling section updates, building retirement and Fill APIs. They add no Source recorder or captured fixture.
