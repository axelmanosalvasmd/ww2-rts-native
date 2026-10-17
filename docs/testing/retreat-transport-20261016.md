# Retreat and transport intent

The RETREAT-TRANSPORT stream uses prototype `e4e579a`.

This runtime batch adds boat landing admission, prototype rail denial reasons,
canonical rail eligibility, convoy home-query priority and the forced-withdrawal
progress update before firing. It adds no persistent state, command variant,
recipient field, FFI entry point or C header change.

## Row mapping

| Row | Native intent tests | Scope |
| --- | --- | --- |
| BASE-57 | `match_reactions::retreat_home_tests::base57_skirmish_nearest_finished_accessible_hq_and_spawn_fallback`; `base57_horde_shared_defender_hq_excludes_attacker_and_enemy`; `base57_classic_nearest_own_finished_production_and_spawn_fallback`; `base57_convoy_nearest_active_allied_source_precedes_mode_home_with_fallback`; `base57_world_own_territory_production_then_nearest_own_region_component` | Read-only home selectors cover ordinary, shared Horde, Classic, convoy and World branches. |
| MOV-75 | `match_carriers::transport_intent::mov75_unload_uses_nearest_free_cells_and_inclusive_shore_reach_per_carrier`; `mov75_board_sail_refuse_deep_unload_then_land_with_every_seat_snapshots` | Exact 5 m boundary, deep refusal, mixed carriers, wadeable surf, nearest-free cells, unchanged rider HP and an ordinary boat trip with complete snapshots for every seat. |
| MOV-81 | `match_world_rails::command_tests::mov81_command_boards_eight_nearest_and_clears_old_orders`; `mov81_command_excludes_ineligible_actors_and_accepts_thirty_meter_boundary`; `mov81_command_selects_owned_stations_and_checks_live_regions_and_bridges`; `mov81_command_cooldown_is_twenty_seconds_per_station_and_team`; `mov81_command_semantic_refusals_clear_follow_but_validation_errors_preserve_it` | Existing World core command component, living actors, actual boarding and canonical metadata. No World room admission. |
| LOG-14 | `match_withdrawal::intent_tests::log14_forced_progress_resync_requires_path_and_no_supply_hold`; `log14_forced_progress_survives_ordinary_ticks_and_each_seat_snapshot`; `log14_forced_point_capturers_do_not_capture_or_contest_mixed_groups`; `log14_forced_units_ignore_enemy_reactions_and_retreating_units_do_not_fire`; `log14_forced_aa_and_flak_fire_only_when_stranded`; `log14_withdrawal_entry_drops_pending_grenades_and_forced_chases_are_cancelled`; `log14_forced_world_region_capturers_do_not_capture_or_contest_mixed_groups`; MOV-81 forced-passenger test above; existing `match_reactions::intent_tests::cmb78_forced_withdrawal_protection_requires_moving_and_not_stranded` | Each guard has a counterfactual. World capture and train refusal use components. Ordinary ticks consume existing authored reserves; partial rows report missing logistics. Complete snapshots are checked after temporarily removing those reserves, then the same reserves are restored. Complete reserve delivery remains refused. |

MOV-48 receives additional witnesses for house exit, measured 1.5x ground
displacement, Classic completion through ticks and aircraft flight home with
complete every-seat delivery. Its complete World return-through-legs witness
remains unfinished, so this batch does not claim MOV-48.

CMB-146 receives ordinary Board/Unload lifecycle and Rail command dispatch
coverage. Boarding from a garrison or while doing construction or mass
entrenchment remains unfinished. This batch does not claim CMB-146.

## Prototype readback

- Home selection: `shared/sim.js:388-390,1725-1735`. Accessibility means the
  prototype's `productionAccess` ownership rule. Home selection does not search
  for a reachable building. Convoys prefer the nearest active allied source,
  with stable store order breaking equal distances.
- Retreat: `shared/sim.js:3335-3344,5536`. The new tests exercise existing house
  exit, movement and aircraft actuators. Classic ticks are supported, but its
  recipient snapshot admission stays refused and is asserted explicitly.
- Unload: `shared/sim.js:111,2422-2433,3419,3564-3570`. Nearest-free placement
  may place multiple squads in the same cell, matching the prototype. A shore
  refusal still clears the selected boat's follow order.
- Rail: `shared/sim.js:114,1252-1267,3419,3540-3563`. Selection uses the canonical
  command owner filter. Eligible ground units within 30 m board in distance
  order, up to eight. Ownership and live bridges gate the route. The station's
  team clock admits a new departure at exactly 20 seconds.
- Withdrawal: `shared/sim.js:1312,1983-1985,2245,4703,5429,5474,5544,5558,5712`.
  Movement refreshes stranded and retreating state before ground firing. A path
  stuck for more than one second loses retreat status; making progress restores
  it unless supply hold is set.

The existing match owners and deterministic ordering are retained. The
three result strings are documented in
[the transport contract note](../contracts/retreat-transport.md).

## Admission and delivery limits

World core components are exercised without admitting a World server room.
Classic and complete logistics snapshots remain refused. The forced-withdrawal
lifecycle explicitly checks those logistics refusals. It does not establish a
complete logistics match, convoy runner or reserve delivery. The boat lifecycle
runs ordinary ticks and captures complete recipient frames for every seat while boarding, while aboard, after a shore refusal and after landing.

The #373 elimination serialization hotfix is inherited unchanged. No parity
status table or prototype file changes. Hermes' exact-head runtime acknowledgement,
hosted CI, integration and merge remain with the orchestrator.
