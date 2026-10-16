# Combat native intent window

The owned window is all 39 IDs in `AUDIT_COMBAT.json`, one native test window for `docs/parity/combat.md`. Axel approved rules 1 to 5 without added conditions in [6206861267](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267). Hermes 6206844573 notes are not requirements. Behavior qualification uses native tests and reviewer confirmation against prototype intent. Data tables and pure functions retain exact comparison.

This checkpoint adds 32 tests for 34 rows through existing native providers. Five rows have missing providers. Partial rows stay in progress individually. The first executable batch completed 38/39 tests at `27b29fe6`; reviewer confirmation is pending. No row is ported by this checkpoint. The physical scene helper declares component inputs, uses seeded native randomness and calls actual Match providers. It does not claim ordinary purchase/history except the paid-birth test, which executes recorded_buy.

Prototype references below are `shared/sim.js` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`, except explicit `projectiles.js` references. New test names below have literal prefix `match_firing::audit_native::` in the core library test binary.

| Row | Native test suffix | Prototype lines | Remaining qualification |
| --- | --- | --- | --- |
| CMB-01 | `cmb01_faction_locks_preserve_all_four_buy_sets` | 393-394,682 | Faction locks tested. Default lobby factions and purchase maximums need separate complete coverage. |
| CMB-02 | No runnable provider test | 690-693 | No Horde/Tutorial constructor or enemy-faction selector provider. No formula-only substitute. |
| CMB-03 | `cmb03_uk_support_admission_creates_fifteen_shells` | 415-418 | Native PASS and reviewer intent confirmation pending. |
| CMB-38 | `cmb38_unfinished_pillbox_cannot_fire_finished_one_launches` | 363-364,3705,4264-4273,5352 | Finished-fire gate and existing four-direction section-damage test. Default/ordered slit orientation is not established here. |
| CMB-42 | `cmb42_sniper_and_at_choose_their_jobs_over_nearer_rifles` | 3906-3913 | Native PASS and reviewer intent confirmation pending. |
| CMB-43 | `cmb43_paid_birth_has_full_health_clean_combat_and_ground_autoretreat` | 1773-1809 | Native PASS and reviewer intent confirmation pending. |
| CMB-44 | `cmb44_can_shoot_rejects_dead_allied_hidden_riding_air_and_outside_range` | 3819-3824 | Ground rejection predicates tested. Aircraft shooter branch and gun-pit extension remain unsupported. |
| CMB-45 | `cmb45_nearest_live_section_and_ship_bow_are_aimed_at` | 3826-3835,3933 | Nearest footprint and bow tested. Naval search reach needs its complete firing-suite witness. |
| CMB-46 | `cmb46_setup_movefire_suppression_buff_and_area_order_gate_volley` | 4158-4165,5534,5573-5577 | Readiness, suppression and buff tested. Shortage and salvo suppression independence need existing firing-suite coverage. |
| CMB-47 | `cmb47_living_models_launch_rounds_with_distinct_ids_and_scaled_suppression` | 4155-4179 | Living-model volley tested. Full accuracy modifiers and miss geometry need existing firing-suite coverage. |
| CMB-48 | `cmb48_round_launch_has_profile_height_lifetime_and_no_early_damage` | 4125-4135; projectiles.js:2-33 | Native PASS and reviewer intent confirmation pending. |
| CMB-49 | `cmb49_intervening_enemy_intercepts_but_ally_is_transparent` | 4224-4249,4319-4343 | Native PASS and reviewer intent confirmation pending. |
| CMB-50 | `cmb50_shallow_hard_contact_continues_same_round_then_contact_limit_stops_it` | 4285-4299; projectiles.js:92-99 | Native PASS and reviewer intent confirmation pending. |
| CMB-51 | `cmb51_52_arrival_uses_current_armor_facing_and_credits_damage_once` | 4195-4215 | Arrival facing and XP-once tested. AP/naval, flame, tech and memory branches need existing damage-suite coverage. |
| CMB-52 | `cmb51_52_arrival_uses_current_armor_facing_and_credits_damage_once` | 3915-3918,4205 | Native PASS and reviewer intent confirmation pending. |
| CMB-53 | `cmb53_direct_terrain_contact_wears_the_contacted_solid` | 4265-4284,4306-4316 | Solid wear tested. FireAt remains held and building-section terminal XP needs existing damage coverage. |
| CMB-54 | `cmb54_travel_suppresses_once_per_volley_and_retreat_and_sprint_are_immune` | 4216-4223,4329-4331 | Native PASS and reviewer intent confirmation pending. |
| CMB-55 | `cmb55_suppression_speed_thresholds_and_trench_rally` | 3817,5362 | Native PASS and reviewer intent confirmation pending. |
| CMB-56 | `cmb56_blast_friendly_fire_falloff_trench_and_retreat_multipliers` | 4715-4743 | Friendly fire, radial falloff, trench and retreat tested. Carried/deck, garrison, sections and tech need broader damage coverage. |
| CMB-57 | `cmb57_salvo_scatter_staggers_physical_shells_and_cooldown` | 4141-4154,4302-4305 | Native PASS and reviewer intent confirmation pending. |
| CMB-58 | `cmb58_salvo_timer_consumes_two_draws_only_when_shell_is_due` | 4146,5623-5629 | Native PASS and reviewer intent confirmation pending. |
| CMB-59 | No runnable provider test | 4363-4375 | Beam firing is explicitly unsupported by apply_unit_firing. No breath actuator is added. |
| CMB-60 | `cmb60_bunker_direct_and_offmap_damage_have_separate_scaling` | 31-35,973,4204,4721-4722 | Direct bunker, support blast and pure Horde ratio tested. Annihilation howitzer and live Horde context remain incomplete. |
| CMB-62 | No runnable provider test | 2813-2818,4180-4189,5458-5463,5559-5568 | FireAt parsing and terrain launch remain unavailable. Existing ignored combat_remaining::cmb62_tank_shells_single_house_same_house_first_wall_and_rifle_refusal is preserved. |
| CMB-63 | `cmb63_star_thresholds_reduce_actual_damage_and_suppression` | 153,894,960,2397,4213,4277,4305,4315 | Thresholds, actual damage and suppression tested. Accuracy effect and Horde wave initialization remain incomplete. |
| CMB-66 | `cmb66_support_interception_checks_empty_ammo_setup_and_holdfire` | 4695-4712 | Ammo, setup and Hold Fire tested through real support strike phase. Deterministic hit/miss, cover and truncated payload branches remain incomplete. |
| CMB-67 | No runnable provider test | 2226-2234 | Air weapon firing is explicitly unsupported. Existing ignored combat_remaining::cmb67_original_hold_fire_patrol_then_explicit_ground_attack_uses_ammo and automatic/area test remain preserved. |
| CMB-68 | `cmb68_enemy_carrier_hit_ejects_deck_riders_with_shock` | 2423-2440,4209,4727-4733,5356-5359 | Deck rider shock tested. Dead-carrier 30 percent loss and offshore drowning remain unsupported. |
| CMB-70 | `cmb70_dead_vehicle_settles_into_rule_wreck_infantry_does_not` | 157,2336,5164-5169,5679-5693 | Actual retirement and Q settlement tested. Oldest-of-40 and five-cell scorching require existing damage-suite coverage. |
| CMB-71 | `cmb71_74_trench_settles_and_front_arc_controls_cover` | 2306-2310,4161 | Native PASS and reviewer intent confirmation pending. |
| CMB-72 | `cmb72_quality_tracks_woods_crater_wear_and_wall_health` | 2266-2270 | Native PASS and reviewer intent confirmation pending. |
| CMB-73 | `cmb73_wall_distance_flank_corner_and_live_hull_cover` | 2313-2338 | Native PASS and reviewer intent confirmation pending. |
| CMB-74 | `cmb71_74_trench_settles_and_front_arc_controls_cover` | 60,2302-2308,4424,4723 | Trench facing/settling tested. Hidden/air/recon, movement and blast branches need complete coverage. |
| CMB-75 | `cmb75_all_house_types_lose_accuracy_protection_as_the_cell_is_damaged` | 138-146,2300,3507,4548,4723,4937-4940 | Every house accuracy pair tested. Vision, flame/blast, eviction and unreachable-attack exit remain incomplete. |
| CMB-76 | No runnable provider test | 428,445-447 | can_shoot_target reads the base weapon range without the gun-pit extension. No runtime repair in this test-only batch. |
| CMB-77 | `cmb77_sniper_camouflage_requires_stillness_quiet_and_distance` | 68,4421-4431,5883 | Native PASS and reviewer intent confirmation pending. |
| CMB-78 | `cmb78_retreat_clears_orders_paths_home_and_stops_firing` | 1725-1734,1983-1985,3335-3344,4217,5484,5536,5558 | Ground reset/path/no-fire tested. Damage/speed/arrival, house exit, mode-specific homes and air return need complete coverage. |
| CMB-79 | `cmb79_autoretreat_obeys_health_home_distance_and_switch` | 76,5366 | Native PASS and reviewer intent confirmation pending. |
| CMB-80 | `cmb80_home_reinforcement_restores_two_models_and_repairs_vehicle_for_mp` | 51,5645-5670 | Native PASS and reviewer intent confirmation pending. |

Existing native checks retained for the same batch:

- `economy::compiled_catalog_keeps_complete_post_initialization_tables` and `economy::prices_factions_population_builders_and_support_match_javascript` retain exact catalog and faction comparisons for CMB-01/03.
- `projectiles::profile_selection_and_constants_match_reference`, `projectiles::swept_contacts_match_reference_boundaries_movement_and_rotation` and `projectiles::contact_response_matches_reference_ricochet_thresholds` retain the cheap exact helper comparisons for CMB-48/49/50.
- `match_combat::audit_owned::cmb38_rifle_and_at_hit_actual_pillbox_front_and_rear_sections` retains the authored pillbox section consumer.
- `match_combat::ordered_damage_riders_terrain_and_death_match_original_source` retains the existing complete component corpus. Its result must be attributed to this head if selected, never borrowed from an older run.

Historical 7f8 autocast and 848 Source serialization failures remain unchanged in their original commits and artifacts. No Source recorder, Source job, capture, RNG-prefix/checkpoint or provenance bundle is added. The historical ignored protected-actuator consumers stay ignored.

Execution remains gated by a new literal Root grant for the frozen head, exact selection, external target and slot. One focused nextest command uses `--no-fail-fast`; failed rows are dropped individually without semantic retry. Root owns the slot ledger, public push, PR consolidation and merge. A reviewer must confirm complete row intent before a native PASS can promote it. Runtime changes and intentional differences retain their existing owner gates.

First native attempt: the single S2 batch on `4439a82820b55eab955743fd08bd4abb2dc31d71` / Rust `d02a909109a5c356b4f55e5759a541b5bd27a786` terminated with compile exit101. E0308 at `audit_combat_remaining.rs:342` is an inherited ignored aircraft consumer passing `&plane` to a value-ID method. No test was selected or executed: all 39 rows remain UNRUN, PASS0/testFAIL0. The ordinary `/tmp/native-feature-batches-0323/audit_combat/native.log`, `RESULT.json` and `LAUNCH-RESULT.json` retain the producing failure and confirmed inherited-lock release.

The successor removes only that erroneous borrow, as the compiler requests. It changes no assertion, input, ignore, runtime provider or guard. The 03:23 overseer correction authorizes this source repair, while a new exact-head Root grant is still required for execution. The failed producing head and its outputs remain unchanged.

The overseer03:35 test-PR/status-PR split supersedes the initial combined parity-file workflow. This test candidate leaves `docs/parity/combat.md` exact to its task parent. The full row mapping remains in this document. Root owns the separate proven-row ledger and later status PR, after test-PR merge and reviewer confirmation.

Executable native result at `27b29fe61e32781ed7bb6ec6e0bf0c0478840719` / Rust `61e73ff0f45eae6bb30eedfbb4db8ddf511eac12`: all 39 selected tests completed once, 38 PASS / 1 FAIL / 290 skipped, exit100. All seven retained checks passed; 31 of 32 new tests passed. CMB-53 failed the authored `wear[cell] > 0` assertion after actual wall-HP reduction passed. The normal 0335 output and terminal/release JSON remain unchanged.

Prototype sim.js:4274-4283 calls the 0.25-scaled HP damage bounded contact wear; it does not imply incrementing the separate road/crater/trench wear array for a sandbag on soil. The successor strengthens the HP assertion to exact22.5 contact plus90 terminal shell damage and requires unchanged ground wear. No runtime defect is claimed or repaired. The failed body stays preserved at27b29fe6, and the corrected witness requires a new exact-head grant. CMB-53 remains held for FireAt and broader section coverage. Other row gaps in the table stay unchanged; passing a component does not close them.
