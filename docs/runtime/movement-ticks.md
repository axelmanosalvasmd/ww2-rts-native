# Movement ticks

Reference: `reference/ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
This batch depends on the ordinary logistics integration in PR #375 and the main compilation fix in PR #373. It changes no C interface, recipient field or World admission rule.

## eachrowliteraltestmapping

| Row | Native intent tests | Scope |
| --- | --- | --- |
| MOV-04 | `mov04_ordinary_ticks_select_pace_retreat_ura_and_pre_rally_suppression`; `mov04_ordinary_ticks_multiply_ground_slope_and_weather`; `mov04_detached_march_multiplies_the_selected_component_speed`; `mov04_ground_factor_branches_footprint_average_and_naval_bypass_move_the_unit`; existing `speed_component_pace_and_retreat_sprint_suppression_precedence`; `mov04_ordinary_fuel_cap_limits_momentum_and_uses_emergency_only_when_forced`; existing `original_world_march_home_visibility_cover_and_first_contact` | Speed precedence, suppression before rally, terrain, slope, weather, road march eligibility and displacement, and the final fuel cap including emergency fuel. Ordinary cases collect every seat on each tick. March eligibility and its multiplier use detached component tests, without admitting a World room. |
| MOV-61 | `mov61_ordinary_ticks_keep_hidden_clearance_and_cover_out_of_delivered_trajectories`; `mov61_traffic_filters_hidden_enemies_but_includes_visible_enemies_and_allies` | Both named prototype traffic privacy scenes run ordinary ticks and collect both seats. Hidden changes preserve delivered trajectories, fog and remembered terrain. Revealing the changes reverses the voluntary choice before contact. A separate component checks unit visibility membership. |
| MOV-71 | `mov71_ordinary_movement_crushes_wears_raises_dust_and_triggers_only_hostile_mines`; `mov71_lethal_mine_contact_keeps_the_dust_raised_before_the_explosion` | Actual movement crushes all four cover types, wears soil and mud with heavy/light and wet/low-ground factors, makes mud, raises dry dust and suppresses ford/mud dust, and detonates only hostile mines. Every ordinary case continues through both-seat snapshots. The lethal contact test inspects dust before death cleanup removes the actor. |
| LOG-21 | `log21_network_is_initialized_then_refreshes_only_on_twentieth_ticks_for_active_teams`; `log21_zero_network_beat_freezes_existing_cache_without_disabling_creation` | Constructor initialization, unchanged cached connectivity until tick 20, refreshed connectivity at tick 20, inactive-team removal at tick 40, correct phase position, and zero-beat freeze. The freeze hook is a transient method argument and adds no saved state. |

Prototype speed evidence: `shared/sim.js:317-319,3817,5480-5486`; ground effects: `shared/sim.js:5513-5533`; traffic privacy: `shared/sim.js:5487-5495` and `test-engine-traffic-privacy.js:95-187`; network scheduling: `shared/sim.js:825,5108,5656-5657`.

MOV-01 and MOV-67 remain next work. This batch does not claim complete tick-phase coverage or all placement callers. MOV-03 remains excluded because the logistics prerequisite still lacks the terrain-firing unit turn. No row is dropped and no deliberate gameplay difference is proposed.

The earlier carrier and ground-order fixtures expected ground effects to be unavailable even though contact was connected. Their assertions now require that phase to be available. Their existing behavior assertions remain intact.
