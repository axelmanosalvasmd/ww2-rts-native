# Classic bases intent checks

Reference: `reference/ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
This batch starts from the #373 hotfix branch on main `57ebeb8e`.
No parity status files are changed.

The runtime change preserves the prototype's insertion order for discovered enemy pairs and excludes living, untransported ground troops from node footprints. A private terrain copy supplies occupancy to the existing site search. Walking distances still use the canonical terrain. There is no new persistent field or wire shape.

| Row | Native intent tests | Prototype evidence |
| --- | --- | --- |
| BASE-10 | `base10_four_players_place_fuel_in_first_discovered_enemy_pair_order`; `base10_node_search_skips_live_ground_troops_without_changing_walk_terrain`; `base10_three_players_get_home_and_contested_rates_with_legal_footprints`; `base10_fuel_search_improves_walking_balance_around_a_river_detour`; existing `mode37_classic_setup_has_clock_hq_engineers_and_separated_home_fuel_nodes` | `shared/sim.js:1688-1722`: home offsets, enemy pair order, fuel flanks, walking score and exclusion distances. `:1012-1018`: occupied-footprint exclusion. The tests cover pair order, living/dead/riding occupancy, rates, footprint size, spawn/point/node separation, two fuel nodes in 1v1 and a river detour. |
| BASE-29 | `base29_classic_paid_site_uses_only_nearby_crew_and_unlocks_training_and_prerequisites`; `base29_unfinished_classic_pillbox_does_not_fire_at_a_visible_enemy`; `base56_classic_deadline_stops_sites_clears_paid_jobs_and_decays_every_producer` | `shared/sim.js:164`, `:5352`, `:5375-5380`, `:5587-5603`: crew reach, diminishing returns, 25% initial health plus 75% construction health, no unfinished production/fire/prerequisite, and construction stopping in Sudden Death. |
| BASE-56 | `base56_classic_deadline_stops_sites_clears_paid_jobs_and_decays_every_producer`; existing `mode38_classic_sudden_death_clears_queues_decays_and_blocks_paid_actions` | `shared/sim.js:164-166`, `:5756-5762`, `:3689`, `:3727`, `:3739`, `:3807-3812`: deadline, queue clearing without refund, producer decay and paid-command denials. The new test crosses the deadline through normal ticks, checks all five producer kinds including an unfinished site, and continues ticking. The existing test checks the repair cap. |

These are behavior tests, not exact whole-match replay or an exact JavaScript node-list fixture. Native walking paths retain their existing deterministic implementation. There is no all-shipped-map execution claim.

Dropped rows:

- BASE-30: Classic logistics startup and full logistics tick/snapshot delivery remain guarded. Existing component repair and recovery checks do not qualify that complete lifecycle.
- BASE-52: the row includes deferred World startup and unforced skirmish fallback/clearance branches. Classic HQ and starting-force checks alone do not complete it.
- BASE-53: existing setup checks do not establish the complete arc geometry, gaps, four corner Flak placements and cramped fallbacks across Assault, Annihilation and Horde.
- BASE-59: complete recipient snapshots reject Classic; node publication and shared Horde producer access remain absent.

World admission stays closed. The World clause of BASE-56 is deferred under the owner decision; the proven scope is the Classic deadline. Classic recipient snapshots remain guarded. No server admission, FFI, header, save format, wire field, recipient policy or contract changes are included. Runtime integration still requires Hermes' exact-head acknowledgment and hosted checks, owned by the orchestrator.
