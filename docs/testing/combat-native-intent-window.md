# Combat native intent batch 1

This test-only candidate starts at main `40fe9e296b46060241693c6c627ecd615d4f8ea4`.
It extracts only `audit_combat_native.rs` and its `#[cfg(test)]` module registration
from owned boundary `dcf84cd6..f46265f4`, then strengthens six existing witnesses.
It imports no older recorder, ignored consumer, runtime or ancestry changes.
The reference remains `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

Axel's [owner decision 6206861267](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267)
approves native behavior tests with prototype-intent review. Pure helpers and
data tables retain their existing exact comparisons. No parity status changes
belong to this PR. The coordinator owns the later status PR after merge.

Two independent read-only reviews checked the prototype and current native
tests. They confirmed the 12 complete claims below after the added controls.
The other tests remain useful partial coverage, with their rows explicitly
dropped from the complete-row claims.

Every test suffix below has prefix `match_firing::audit_native::`. Prototype
lines refer to pinned `shared/sim.js`, except the explicitly named projectile
helper lines.

| Row | Test suffix | Prototype lines | Batch claim |
| --- | --- | --- | --- |
| CMB-01 | `cmb01_faction_locks_preserve_all_four_buy_sets` | 393-394,682 | Proven with the supporting checks below. |
| CMB-02 | No runnable provider test | 690-693 | Dropped: Horde/Tutorial constructors and enemy-faction selection are unavailable. |
| CMB-03 | `cmb03_uk_support_admission_creates_fifteen_shells` | 415-418 | Proven with the supporting checks below. |
| CMB-38 | `cmb38_unfinished_pillbox_cannot_fire_finished_one_launches` | 363-364,3705,4264-4273,5352 | Dropped: Slit orientation and actual front/rear section damage are not covered by the extracted test. |
| CMB-42 | `cmb42_sniper_and_at_choose_their_jobs_over_nearer_rifles` | 3906-3913 | Proven with the supporting checks below. |
| CMB-43 | `cmb43_paid_birth_has_full_health_clean_combat_and_ground_autoretreat` | 1773-1809 | Dropped: Air auto-retreat and mode-dependent ability defaults are not covered. |
| CMB-44 | `cmb44_can_shoot_rejects_dead_allied_hidden_riding_air_and_outside_range` | 3819-3824 | Dropped: Aircraft shooter and gun-pit range branches are unavailable. |
| CMB-45 | `cmb45_nearest_live_section_and_ship_bow_are_aimed_at` | 3826-3835,3933 | Dropped: Naval search reach and failed nearest-section selection are not exercised. |
| CMB-46 | `cmb46_setup_movefire_suppression_buff_and_area_order_gate_volley` | 4158-4165,5534,5573-5577 | Dropped: Shortage reload and salvo suppression independence are not exercised. |
| CMB-47 | `cmb47_living_models_launch_rounds_with_distinct_ids_and_scaled_suppression` | 4155-4179 | Dropped: Combined accuracy modifiers and miss dispersion are not fully exercised. |
| CMB-48 | `cmb48_round_launch_has_profile_height_lifetime_and_no_early_damage` | 4125-4135; projectiles.js:2-33 | Dropped: Aircraft/garrison launch heights and full travel/expiry behavior are not exercised. |
| CMB-49 | `cmb49_intervening_enemy_intercepts_but_ally_is_transparent` | 4224-4249,4319-4343 | Dropped: Terrain first-contact, start-cell and moving-body consumer paths are incomplete. |
| CMB-50 | `cmb50_shallow_hard_contact_continues_same_round_then_contact_limit_stops_it` | 4285-4299; projectiles.js:92-99 | Proven with the supporting checks below. |
| CMB-51 | `cmb51_52_arrival_uses_current_armor_facing_and_credits_damage_once` | 4195-4215 | Dropped: Complete arrival damage modifiers and side effects are not exercised. |
| CMB-52 | `cmb51_52_arrival_uses_current_armor_facing_and_credits_damage_once` | 3915-3918,4205 | Proven with the supporting checks below. |
| CMB-53 | `cmb53_direct_terrain_contact_wears_the_contacted_solid` | 4265-4284,4306-4316 | Dropped: FireAt execution and terminal building-section XP are not exercised. |
| CMB-54 | `cmb54_travel_suppresses_once_per_volley_and_retreat_and_sprint_are_immune` | 4216-4223,4329-4331 | Proven with the supporting checks below. |
| CMB-55 | `cmb55_suppression_speed_thresholds_and_trench_rally` | 3817,5362 | Proven with the supporting checks below. |
| CMB-56 | `cmb56_blast_friendly_fire_falloff_trench_and_retreat_multipliers` | 4715-4743 | Dropped: Complete blast target, building and mode branches are not exercised. |
| CMB-57 | `cmb57_salvo_scatter_staggers_physical_shells_and_cooldown` | 4141-4154,4302-4305 | Dropped: Range-dependent spread, ammo and arrival blast behavior are not exercised. |
| CMB-58 | `cmb58_salvo_timer_consumes_two_draws_only_when_shell_is_due` | 4146,5623-5629 | Proven with the supporting checks below. |
| CMB-59 | No runnable provider test | 4363-4375 | Dropped: Beam firing is explicitly unsupported. |
| CMB-60 | `cmb60_bunker_direct_and_offmap_damage_have_separate_scaling` | 31-35,973,4204,4721-4722 | Dropped: Annihilation howitzer and live Horde scaling are not exercised. |
| CMB-62 | No runnable provider test | 2813-2818,4180-4189,5458-5463,5559-5568 | Dropped: FireAt parsing and terrain firing are unsupported. |
| CMB-63 | `cmb63_star_thresholds_reduce_actual_damage_and_suppression` | 153,894,960,2397,4213,4277,4305,4315 | Dropped: Accuracy effect and Horde wave initialization are not exercised. |
| CMB-66 | `cmb66_support_interception_checks_empty_ammo_setup_and_holdfire` | 4695-4712 | Dropped: Fighter cover, deterministic outcomes and truncated payload branches are not exercised. |
| CMB-67 | No runnable provider test | 2226-2234 | Dropped: Aircraft weapon firing is explicitly unsupported. |
| CMB-68 | `cmb68_enemy_carrier_hit_ejects_deck_riders_with_shock` | 2423-2440,4209,4727-4733,5356-5359 | Dropped: Carrier-death damage, carried/deck blast distinctions and drowning are incomplete. |
| CMB-70 | `cmb70_dead_vehicle_settles_into_rule_wreck_infantry_does_not` | 157,2336,5164-5169,5679-5693 | Proven with the supporting checks below. |
| CMB-71 | `cmb71_74_trench_settles_and_front_arc_controls_cover` | 2306-2310,4161 | Proven with the supporting checks below. |
| CMB-72 | `cmb72_quality_tracks_woods_crater_wear_and_wall_health` | 2266-2270 | Proven with the supporting checks below. |
| CMB-73 | `cmb73_wall_distance_flank_corner_and_live_hull_cover` | 2313-2338 | Proven with the supporting checks below. |
| CMB-74 | `cmb71_74_trench_settles_and_front_arc_controls_cover` | 60,2302-2308,4424,4723 | Dropped: Trench visibility, movement and complete protection behavior are not exercised. |
| CMB-75 | `cmb75_all_house_types_lose_accuracy_protection_as_the_cell_is_damaged` | 138-146,2300,3507,4548,4723,4937-4940 | Dropped: Garrison vision, blasts, eviction and attack-driven exit are not exercised. |
| CMB-76 | No runnable provider test | 428,445-447 | Dropped: Targeting reads base range and has no gun-pit extension. |
| CMB-77 | `cmb77_sniper_camouflage_requires_stillness_quiet_and_distance` | 68,4421-4431,5883 | Dropped: Aircraft/recon visibility and owner flag are not exercised. |
| CMB-78 | `cmb78_retreat_clears_orders_paths_home_and_stops_firing` | 1725-1734,1983-1985,3335-3344,4217,5484,5536,5558 | Dropped: Complete retreat damage, movement, arrival, house and mode/air branches are incomplete. |
| CMB-79 | `cmb79_autoretreat_obeys_health_home_distance_and_switch` | 76,5366 | Dropped: Convoy/Horde exclusions are not exercised. |
| CMB-80 | `cmb80_home_reinforcement_restores_two_models_and_repairs_vehicle_for_mp` | 51,5645-5670 | Dropped: Mode-specific home rules and Horde exclusion are not exercised. |

Supporting existing tests, selected afresh for this candidate:

- CMB-01: `economy::compiled_catalog_keeps_complete_post_initialization_tables`, `economy::prices_factions_population_builders_and_support_match_javascript` and server `lobby::lobby_socket_tests::net22_websocket_lowest_unused_team_is_independent_of_seat_and_faction_cycle` cover all faction buy sets and the slot default cycle.
- CMB-03: the same exact catalog/support checks cover the doctrine values alongside real admission.
- CMB-42/52: `combat::role_armor_veterancy_retreat_and_threat_match_reference` covers the exact role and armor matrix, including Kaiju, alongside native selection, arrival and blast-facing controls.
- CMB-50: `projectiles::profile_selection_and_constants_match_reference` and `projectiles::contact_response_matches_reference_ricochet_thresholds` cover material values and every response threshold alongside the actual continuation resolver.
- CMB-54: `combat::suppression_combination_guards_and_write_order_match_reference` covers the exact suppression modifiers, exclusions, cap and metadata alongside two-round deduplication and near-band controls.
- CMB-55: native speed, actual launch accuracy, reload and full-tick trench/open-ground recovery are checked at the suppression thresholds.
- CMB-58: actual salvo bookkeeping consumes two combat draws only when a shell is due. The native stream design remains unchanged.
- CMB-70: `match_combat::ordered_damage_riders_terrain_and_death_match_original_source` covers retirement, five-cell scorch and oldest-of-40 eviction alongside native physical settlement.
- CMB-71: trench/local/garrison cases and the actual suppression consumer check the stronger local/directional protection. CMB-75's retained component supplies all house protection values without claiming that whole row.
- CMB-72: exact expected cover-quality values cover the default, woods, wall health floor, crater wear and cap.
- CMB-73: actual off-ray corners, flank controls, live hull, airborne exclusion and settled/moving wrecks extend the distance controls.

The five rows without a provider test are CMB-02/59/62/67/76. They are dropped
from this batch rather than adding runtime behavior. No deliberate gameplay
difference is proposed. All other dropped rows retain their useful partial
components, without claiming that a passing component proves a complete row.

Validation on the Vostro:

- Initial extraction: 32 selected tests passed.
- First strengthened batch: 40 selected, 39 passed, one authored CMB-73 fixture error. The ground-only `initial_unit` helper returned `UnsupportedAir`; no plane-cover assertion ran. The failure log remains separate.
- Corrected fixture: use existing `initial_aircraft` only for the local read-only cover query. It admits no sortie or aircraft firing. Fresh focused run: 40 selected, 40 passed, 351 skipped. This includes all 32 native combat tests and eight supporting checks.
- `cargo fmt --all --check` and `cargo clippy --locked --workspace --all-targets -j4 -- -D warnings` passed.

Each Cargo invocation holds one shared S1/S2/S3 lock only while Cargo runs,
uses that slot's external target directory, and runs with 6 GiB MemoryMax,
zero swap and at least 20 GiB free disk. No Source job or full workspace test
run was performed. Historical private-branch results are not claimed for this
candidate.
