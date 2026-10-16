# Ordinary logistics runtime

Baseline: native `58986df2`, prototype `e4e579a`. The stream implements owner
group H from issue #5 comment 6217590741. World admission stays off. Hermes'
exact-head runtime review remains a publication requirement for the orchestrator.

Ordinary Default and Conquest ticks now call the existing logistics owners at
their prototype boundaries: reserves and forced withdrawal before a unit's
orders (`shared/sim.js:5346-5347`), network refresh after point supply
(`:5657`), convoy service after income (`:5755`), and convoy loss notification
on retirement (`:5673`). Movement already charges actual powered travel
(`:5510-5514`). The constructor initializes reserves and stores, while instant
recruitment prepares eligible reserves before committing payment and publication.

The supply scheduler retains automatic convoy routing. Generic world-goal and
stall searches skip automatic trucks (`shared/sim.js:5350,5497`), and squad
reactions skip every convoy (`:5474`). Manual trucks keep ordinary routing.
Unrelated troop queues remain with their command owners; only truck queues need
waypoint projection for manual convoy interception. Forced withdrawal cancels
construction, digging and shared projects through the canonical work owners.

The wire object and exact recipient rules are in
[full-logistics.md](../contracts/full-logistics.md). Server enablement is in
[logistics-enable.md](../contracts/logistics-enable.md). Neither widens room
modes, World, spectators, AI or the C ABI. The cached unit-row visibility mask is
unchanged. Enemy logistics reserves, truck state and events never enter an
ordinary seat's logistics object; allied active stores expose only the stock
that seat can use.

## Each row literal test mapping

These mappings distinguish the ordinary implementation from complete parity
rows. They do not change parity statuses. Validation results must identify the
actual producing commit and executed selection.

- LOG-01: `logistics_enablement_matches_prototype_mode_table_and_strict_scenario_opt_in`
  proves the exact server table and scenario override. Ordinary enabled/off
  construction is covered by the core logistics tests. Classic, Annihilation,
  scenario execution and World room coverage remain held.
- LOG-05: existing `log05_direct_and_salvo_launch_only_funded_whole_rounds`,
  `log05_grenade_ability_spends_its_launched_rounds`,
  `log05_ammunition_abilities_refuse_shortage_before_state_changes`,
  `log05_log08_ground_aa_tracks_fractional_rounds_and_nonstacking_damage` and
  `cmb66_support_interception_checks_empty_ammo_setup_and_holdfire` cover
  represented weapon paths. Terrain firing remains unsupported, so the row is held.
- LOG-08: existing `log08_direct_and_salvo_reload_penalty_does_not_stack` and
  `log05_log08_ground_aa_tracks_fractional_rounds_and_nonstacking_damage` cover
  direct fire, salvos and AA. Terrain firing remains held.
- LOG-20: `ordinary_logistics_match_finishes_with_cache_depot_convoys_fuel_and_every_seat_each_tick`
  and existing `base45_paid_cache_requires_logistics_and_serves_only_when_finished`
  cover Conquest cache admission and lifetime. Classic now has a bounded runner,
  but its enabled logistics remains unsupported.
  `classic_and_annihilation_keep_physical_logistics_uninitialized` checks that
  the broader mode helper does not widen supply admission. World stays off.
- LOG-44: `intent_log44_exact_fields_rounding_and_private_allied_stock_in_complete_frames`
  and `intent_log44_nulls_grace_cut_boundaries_and_disabled_omission_match_prototype`
  prove the ordinary seat object. The full-match test collects every seat each
  tick through victory and ending ticks. World region rates and prototype
  spectator stripping remain held.
- MODE-11: server table and ordinary construction checks cover only the admitted
  Conquest feature switch. Other complete mode combinations remain held.
- MODE-75: dropped for this milestone because World admission is deferred.
- BASE-30: existing `log09_building_repair_uses_average_engineer_recovery_rate`
  covers ordinary recovery scaling. Classic Sudden Death and the complete repair
  row remain held.
- AI-13: held. Room AI and the complete union of supported commander commands
  need the AI and command streams.
- AI-65: held. This batch does not run whole AI matches across the required modes.
- NET-39: `trusted_conquest_logistics_survives_room_ticks_and_both_recipient_snapshots`
  covers the admitted trusted Conquest logistics option and delivered seats.
  The full createGame option matrix remains held.
- IF-24: `if24_storage::if24_repeated_terrain_hits_round_storage_after_each_of_6000_steps`
  checks repeated HP, wear and scar storage and the unrounded trench-collapse
  comparison. The original environment and map fixtures check all typed input arrays;
  `pathfinding::infantry_naval_and_vehicle_paths_match_original_find_path` and
  `pathfinding::named_fine_search_branches_match_original_find_path` compare the
  complete prototype search fixtures, including Float32 score comparisons.
  Existing `world_supply` LOG-26/27 fixtures cover World float storage without
  admitting a World room.
- MOV-03: `automatic_convoys_keep_supply_routes_while_manual_trucks_use_ordinary_routes`
  checks the newly exposed convoy sequence. The complete command, terrain-fire
  and movement sequence remains held.

The full-match test restores a checkpoint at tick 40 with an unfinished cache,
an active truck, reserves and supply state. It then continues every-seat delivery
through victory. The server save/load tests also run with logistics enabled.
Loaded-false restart override parity remains held.

Additional regressions are
`reserve_exhaustion_cancels_live_construction_then_recovers_through_tick_and_delivery`,
`queued_nontruck_orders_survive_logistics_preflight_ticks_and_recipient_delivery`
and `killed_convoy_is_notified_once_and_removed_from_recipient_delivery`.
The full-match test holds fire while testing physical logistics; it does not
qualify retained projectile delivery, which remains with its existing owner.
