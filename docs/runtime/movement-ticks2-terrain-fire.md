# Movement ticks and terrain fire

Reference: `reference/ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
Base: PR #388 at `e0f02fc9342f103e20c58091b59f8fe5c6ce7bef`, including
PR #375's ordinary logistics integration. This stream stacks on #388 (#375);
merges after #373. No parity status is edited.

Terrain FireAt now uses the recorded command owner and deferred path FIFO.
A direct gun approaches live terrain until its line is clear, including the
first wall of the same structure. A salvo can keep firing at open ground.
The unit turn resolves this order before reaction and movement, then launches
from the resulting pose after ground contact. Direct shells consume one whole
round. Terrain salvos clamp the volley to funded whole rounds. Neither path
launches with less than one round.

Direct terrain shells choose their reload penalty before the ammunition debit;
terrain salvos choose it after the debit. Exhaustion and low ammunition double
reload once, with the direct shell's suppression multiplier applied separately.
The unit runner retains the pre-rally suppression for this terrain-shell rate.
Terrain-order validation visits the current unit roster, so historical command metadata cannot refuse successor ticks after the shooter retires. No new saved field is needed. Retained ground flights use the existing recipient
wire format documented in [terrain-fire-flights.md](../contracts/terrain-fire-flights.md).
Aircraft flight/event delivery remains guarded.

## eachrowliteraltestmapping

| Row | Native intent tests | Scope |
| --- | --- | --- |
| MOV-01 | `mov01_grid_rebuild_precedes_vision_and_the_first_unit_turn`; `mov01_fifo_is_served_before_movement_and_ordinary_phase_order_survives_600_ticks`; `mov01_projectiles_remove_grenade_tickets_before_support_aa_and_vehicle_retirement`; `mov01_finishing_build_crews_enable_aa_in_the_same_late_phase`; `mov01_cover_intercepts_before_decay_and_new_smoke_drifts_after_old_rain_decay`; existing `mov24_progress_is_measured_after_real_crowd_separation` | Grid before vision/first firing, clock, deferred FIFO, unit turns, crews, separation and infantry progress, flights/grenade/salvo cadence, strikes and AA, cover and cooldown decay, smoke decay before wind drift, and death/wreck cleanup. Grid/FIFO cases collect both seats. Aviation/support-air causal cases run canonical ticks without claiming their guarded recipient events. The 600-tick case is native intent evidence, not a JavaScript equality replay. |
| MOV-03 | `mov03_terrain_orders_move_stop_launch_damage_and_finish_with_every_seat`; `mov03_terrain_shooter_retirement_keeps_its_flights_and_successor_ticks_deliverable`; the MOV-01 cases above; inherited `mov04_ordinary_ticks_select_pace_retreat_ura_and_pre_rally_suppression`, `mov61_ordinary_ticks_keep_hidden_clearance_and_cover_out_of_delivered_trajectories`, `mov71_ordinary_movement_crushes_wears_raises_dust_and_triggers_only_hostile_mines`; `mov03_aimed_ability_walks_into_range_before_throw_and_continues_every_seat_delivery`; existing `queued_ground_orders_match_original_full_steps_and_arrival_order`, `original_paid_dig_lifecycles`, `intent_mov70_squad_enters_house_at_door_then_exits_to_free_cell`, `intent_mov73_boat_needs_explicit_target_and_boards_from_clear_bank`, `live_recovery_matches_original_movement`, `automatic_convoys_keep_supply_routes_while_manual_trucks_use_ordinary_routes` | Adds the missing terrain-order/launch turn to the ordinary per-unit sequence. Queued Move then FireAt moves, stops, launches, damages terrain, survives a retained-flight checkpoint, and finishes through eight ending ticks with both seats captured every tick. The pending aimed-ability route now reaches ordinary firing without a runner refusal. `terrain_fire_out_of_range_routes_moves_and_stops_to_launch_during_ordinary_ticks`, `terrain_fire_blocked_sight_prevents_launch_and_requests_a_route`, `terrain_fire_can_shell_through_the_first_wall_of_its_own_structure_only` and `terrain_fire_destroyed_target_cancels_direct_fire_but_salvo_keeps_shelling_open_ground` cover the new order branches. `terrain_fire_path_fifo_discards_replaced_goal_and_services_current_goal`, `terrain_fire_path_fifo_discards_stopped_orders_without_spending_budget` and `terrain_fire_unreachable_path_drops_fire_order_at_the_retry_limit` check deferred service and failure. Existing owners retain build/dig/nade/entry/boarding/amove/explicit attack, reaction, traffic/stall recovery, powered movement, grid/contact, stillness/progress and target choice. |
| LOG-05 | `log05_terrain_shells_and_salvos_launch_only_funded_whole_rounds`; `log05_accepted_explosive_abilities_debit_launches_while_ap_and_suppress_only_gate_ammo`; inherited `log05_direct_and_salvo_launch_only_funded_whole_rounds`, `log05_grenade_ability_spends_its_launched_rounds`, `log05_ammunition_abilities_refuse_shortage_before_state_changes`, `log05_log08_ground_aa_tracks_fractional_rounds_and_nonstacking_damage`, `cmb66_support_interception_checks_empty_ammo_setup_and_holdfire` | Direct volleys, partial salvos, direct terrain shells, terrain salvos, fractional AA reserve and flak debit, grenade/satchel/barrage launch debit, and AP/suppress admission. Fractional leftovers cannot fund a projectile. |
| LOG-08 | `log08_terrain_reload_uses_shell_predebit_and_salvo_postdebit_shortage_without_stacking`; inherited `log08_direct_and_salvo_reload_penalty_does_not_stack`, `log05_log08_ground_aa_tracks_fractional_rounds_and_nonstacking_damage` | Terrain and direct-fire reload rates, salvo rates, AA damage rate, exhaustion/low-ammo nonstacking, and the exact 25% terrain boundary. |

Prototype anchors: command `shared/sim.js:3461-3480`; routing goal `:3210-3211`;
same-structure line `:2811-2820`; shell launch `:4181-4188`; per-unit order
`:5374-5583`; whole tick `:5305-5340,5585-5692`; ammunition
`shared/logistics.js:57-68,126-129`; direct/salvo/flak/AA/ability sites
`shared/sim.js:4141-4178,2245-2247,4703,3613,5419-5427`.

## Dropped row

MOV-67 is dropped from the complete proven set. Its World rail-stop and guard
placement callers require closed World admission. Sandbox spawning and authored
scenario reinforcement also remain absent. This batch routes Tutorial house
placement through the existing helper and adds fresh-grid checks to the actual
recruitment, step-out, naval, building, aircraft-base, carrier, bunker and
paradrop callers. That partial coverage does not establish the whole row.

## Validation

The PR records the actual producing head and native command results. No Source recorder, Source execution or whole-workspace test run is
part of this stream. Hermes' exact-head runtime acknowledgement and hosted CI
remain with the integration coordinator.
