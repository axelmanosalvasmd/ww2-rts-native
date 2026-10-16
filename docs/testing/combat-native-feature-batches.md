# Native combat feature batch 1

This main-based test PR covers 46 of the 66 assigned CMB rows under [Axel decision 6206861267](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267). Two independent source reviews read the pinned prototype and accepted the native behavior witnesses and required existing exact comparisons below. Parity status changes belong to a separate PR after these tests merge.

Base: `40fe9e296b46060241693c6c627ecd615d4f8ea4`. Reference: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Extracted only owned tests and test registrations from `b2eefab2..06060d2b`; no inherited private runtime, recorder or captured fixture is imported. All registrations are under `cfg(test)`.

CMB-94 now expects the nearest medic, whose zero-range weapon object is truthy in the prototype. CMB-126 uses the aircraft publisher instead of the ground-only spawn helper. CMB-88 gets a native claims test because the old mapped test depends on an unpublished fixture. CMB-89 now isolates visible crowd value against the same sheltered alternative. CMB-103 asserts the minimum unit speed. Four incomplete tests, CMB-112/127/134/137, stay on the old branch and are excluded here.

The historical first-window result at `38317a14` was 46 passes and three failures, including CMB-94/126/137. The nine-test tail passed at `06060d2b`. Those runs remain historical; they do not qualify this extraction.

## Verification

On the Vostro, the extracted source passed `cargo fmt --all --check` and one focused `cargo nextest run --locked --cargo-profile ci-test` selection: 63 tests passed, zero failed, 592 skipped. This includes all 45 new tests and 18 existing exact or behavior checks. The test log contains every literal name below. It also includes `economy::ability_costs_match_reference_objects_and_nullish_overrides`; that exact cost check does not establish CMB-112 payment timing. No test retry, Source recorder or new capture ran. `cargo clippy --locked --workspace --all-targets -j4 -- -D warnings` also passed, in S1 with the same memory/swap caps; that lock was released on exit.

The job held S3 only while Cargo ran, under a user systemd scope with a 6 GiB memory limit and no swap, and released the lock when it exited. Raw logs and the two source-review receipts are at `/tmp/cmb-impl-batch1-20261016/` on this machine. The three historical failures remain attributed to their original heads.

## Covered rows

| Row | Native test | Prototype at e4e579a |
| --- | --- | --- |
| CMB-82 | `combat_feature_tests::cmb82_medics_choose_worst_heal_calm_and_recent_wounds_and_seek`; `combat_feature_tests::cmb82_medic_pass_is_every_ten_ticks_and_provisions_cap_healing` | shared/sim.js:109, 4947-4968, 5654 |
| CMB-84 | `combat_feature_tests::cmb84_hold_fire_blocks_automatic_fire_and_grenades_but_allows_attack` | shared/sim.js:5569, 3864, 2242, 4702, 2212, 4094 |
| CMB-85 | `combat_feature_tests::cmb85_hold_position_keeps_exact_destination_and_blocks_idle_spacing` | shared/sim.js:4051, 4090 |
| CMB-86 | `combat_feature_tests::cmb86_target_threat_hysteresis_visibility_and_explicit_lock` | shared/sim.js:3921, 3928-3946, 135-136 |
| CMB-87 | `combat_feature_tests::cmb87_retarget_waits_then_replaces_dead_target` | shared/sim.js:5569-5570 |
| CMB-88 | `combat_feature_tests::cmb88_claims_spread_fire_track_switches_and_rebuild_from_living_models` | shared/sim.js:3840-3852, 5338-5339 |
| CMB-89 | `combat_feature_tests::cmb89_salvo_crowds_ignore_hidden_and_riding_neighbors` | shared/sim.js:3932, 3936-3937 |
| CMB-90 | `combat_feature_tests::cmb90_attack_chases_stops_locks_drops_unseen_and_pauses_for_satchel` | shared/sim.js:5466-5471 |
| CMB-92 | `combat_feature_tests::cmb92_reaction_seeks_cover_but_crews_orders_and_hold_position_stay` | shared/sim.js:4088-4111 |
| CMB-93 | `combat_feature_tests::cmb93_idle_spacing_moves_newer_squad_and_respects_hold_position` | shared/sim.js:4114-4122 |
| CMB-94 | `combat_feature_tests::cmb94_threat_includes_zero_range_weapon_then_recent_hit_then_move_direction` | shared/sim.js:3989-3999 |
| CMB-95 | `combat_feature_tests::cmb95_spot_search_prefers_shelter_and_excludes_held_or_unreachable_cells` | shared/sim.js:2404, 3950-3986 |
| CMB-96 | `combat_feature_tests::cmb96_shared_plain_move_spreads_and_hold_position_keeps_single_click` | shared/sim.js:4023-4068 |
| CMB-97 | `combat_feature_tests::cmb97_take_cover_refusals_and_queue_cancellation` | shared/sim.js:3580-3601, 2411-2414 |
| CMB-98 | `combat_feature_tests::cmb98_hull_turns_to_visible_anti_tank_threat_before_ordered_face` | shared/sim.js:5551-5555, 5580, 4077-4081 |
| CMB-99 | `combat_feature_tests::cmb99_short_engaged_move_behind_reverses_long_move_does_not` | shared/sim.js:4071-4076, 135-136 |
| CMB-100 | `combat_feature_tests::cmb100_facing_validation_shared_spots_arrival_and_replacement` | shared/sim.js:3422-3423, 3431-3436, 4005-4018 |
| CMB-101 | `combat_feature_tests::cmb101_formation_shapes_rear_ranks_drag_and_spread` | shared/formation.js:8-53 |
| CMB-103 | `combat_feature_tests::cmb103_together_caps_group_pace_and_queued_leg_drops_it` | shared/sim.js:3446, 3449, 5483-5484 |
| CMB-104 | `combat_feature_tests::cmb104_109_113_aimed_families_launch_once_and_manual_halts_auto_keeps_route` | shared/sim.js:191, 248, 5422-5428 |
| CMB-106 | `combat_feature_tests::cmb106_ap_loads_once_and_sets_family_cooldown` | shared/sim.js:197, 288, 315, 273, 3623, 4166-4167, 4199 |
| CMB-107 | `combat_feature_tests::cmb107_smoke_uses_unit_id_radius_duration_and_cooldown` | shared/sim.js:200, 243, 3624, 5639 |
| CMB-108 | `combat_feature_tests::cmb108_barrage_uses_family_shell_count_and_reload` | shared/sim.js:209, 235, 294, 280, 5420-5421 |
| CMB-109 | `combat_feature_tests::cmb104_109_113_aimed_families_launch_once_and_manual_halts_auto_keeps_route` | shared/sim.js:311, 328, 324 |
| CMB-110 | `combat_feature_tests::cmb110_ura_clears_suppression_sprints_and_cools_down` | shared/sim.js:319, 3622, 5484, 4217 |
| CMB-111 | `combat_feature_tests::cmb111_mixed_selection_cooldown_retreat_missing_spot_and_deferred_queue` | shared/sim.js:3602-3628 |
| CMB-113 | `combat_feature_tests::cmb104_109_113_aimed_families_launch_once_and_manual_halts_auto_keeps_route` | shared/sim.js:5417-5431 |
| CMB-114 | `combat_feature_tests::cmb114_autocast_boolean_ownership_ability_and_free_default` | shared/sim.js:3629-3634, 1787 |
| CMB-115 | `combat_feature_tests::cmb115_autocast_staggers_and_preserves_moving_ura_route` | shared/sim.js:3856, 5369-5372 |
| CMB-116 | `combat_feature_tests::cmb116_grenade_requires_visible_sheltered_enemy_and_clear_blast` | shared/sim.js:3865-3871 |
| CMB-117 | `combat_feature_tests::cmb117_mg_suppression_requires_stationary_gun_and_approaching_enemy` | shared/sim.js:3872-3876 |
| CMB-121 | `combat_feature_tests::cmb121_barrage_requires_visible_crowd_or_dug_target_and_no_friend` | shared/sim.js:3884-3894 |
| CMB-122 | `combat_feature_tests::cmb122_satchel_requires_seen_ordered_structure_with_safe_reach` | shared/sim.js:3857, 3895-3899 |
| CMB-123 | `combat_feature_tests::cmb123_vision_is_team_shared_and_changes_on_native_pass` | shared/sim.js:5320, 4379-4441 |
| CMB-126 | `combat_feature_tests::cmb126_airborne_plane_seen_at_sixty_parked_plane_never_sees_or_is_seen` | shared/sim.js:67, 4392, 4420, 5945 |
| CMB-128 | `combat_feature_tests::cmb128_los_endpoints_woods_smoke_length_and_hills` | shared/sim.js:2790-2809, 2840-2856, 104, 70, 158, 175 |
| CMB-129 | `combat_feature_tests::cmb129_team_spot_visibility_requires_range_and_clear_ray` | shared/sim.js:1747-1751 |
| CMB-135 | `combat_feature_tests::cmb135_command_entry_rejects_winner_away_inactive_malformed_and_foreign_ids` | shared/sim.js:3408-3420 |
| CMB-136 | `combat_feature_tests::cmb136_queue_limit_automatic_step_yields_rally_waits_and_stop_clears` | shared/sim.js:3362-3387, 3437-3438 |
| CMB-138 | `combat_feature_tests::cmb138_mass_move_reaches_every_selected_actor_and_clamps_coordinates` | shared/sim.js:3421-3461 |
| CMB-139 | `combat_feature_tests::cmb139_attack_denies_missing_allied_hidden_and_unarmed` | shared/sim.js:3495-3510 |
| CMB-143 | `combat_feature_tests::tail::cmb143_escort_assigns_friendly_ground_mission_and_refuses_other_targets` | shared/sim.js:3514-3519 |
| CMB-152 | `lobby::combat_deny_tests::cmb152_four_denials_per_rolling_second_survive_reconnect` | shared/command-feedback.js:2-8 |
| CMB-153 | `combat_feature_tests::tail::cmb153_all_infantry_slot_counts_health_thinning_and_velocity_lag` | shared/squad-men.js:6-34 |
| CMB-157 | `combat_feature_tests::tail::cmb157_native_unit_row_fields_rounding_combat_flags_and_owner_values` | shared/sim.js:5883-5891 |
| CMB-160 | `combat_feature_tests::tail::cmb160_armor_flank_front_and_explicit_launch_end_are_deterministic` | shared/sim.js:561, 764-769, 3915-3916, 4148, 4171-4173 |

## Required existing companions

These existing tests are part of the qualification, including exact comparisons for pure functions and single-component fixtures. They supply AP shot consumption, sprint movement and immunity, real aimed-order work cancellation, and complete represented snapshot fields.

| Rows | Tests |
| --- | --- |
| CMB-86, CMB-160 | `combat::role_armor_veterancy_retreat_and_threat_match_reference` |
| CMB-95, CMB-138 | `match_commands::tests::original_command_and_waiting_order_phases` |
| CMB-95 | `match_reactions::tests::source_reactions_retreat_and_spacing_match_canonical_state` |
| CMB-100 | `formation::normalize_face_matches_node` |
| CMB-101 | `formation::seeded_cases_match_node`; `formation::written_cases_match_node`; `formation::slot_size_matches_node` |
| CMB-106, CMB-113, CMB-115, CMB-116, CMB-117, CMB-121, CMB-122 | `match_abilities::tests::conquest_abilities_match_original_source_phases` |
| CMB-106 | `combat::damage_and_every_scalar_mutation_match_original_functions` |
| CMB-110 | `movement::movement_matches_the_reference_call_by_call`; `combat::suppression_combination_guards_and_write_order_match_reference` |
| CMB-113 | `match_abilities::tests::manual_aimed_ability_cancels_real_paid_work_without_refund` |
| CMB-128 | `vision::rows::cmb128_line_of_sight_matches_original_source` |
| CMB-153 | `fps_effects::drawn_squad_slots_match_all_original_cases` |
| CMB-157 | `recipient_snapshot::tests::represented_fields_match_actual_cached_reference_snapshots` |
| CMB-160 | `projectiles::flight_and_launch_match_reference_at_and_before_stamp`; `projectiles::contact_response_matches_reference_ricochet_thresholds` |

## Next rows

All 20 rows below stay out of this PR's proven list. CMB-112/127/134/137 are dropped from the extracted tests; the other 16 have no complete mapped test to extract. No deliberate gameplay difference is proposed.

| Row | Reason |
| --- | --- |
| CMB-102 | Formation menu, preview and saved groups require the client. |
| CMB-112 | Exact costs are covered, but payment and pay-on-arrival are not witnessed; main rejects Classic abilities. |
| CMB-127 | The test covers bunker and rider visibility, but Horde straggler and Kaiju reveal remain absent. |
| CMB-130 | No remembered-enemy-building witness in this extraction. |
| CMB-133 | No neutral-guard visibility witness in this extraction. |
| CMB-134 | The ground privacy test cannot cover aircraft; recipient snapshots reject aircraft. |
| CMB-137 | Compound Orders parsing and dispatch are absent on main. |
| CMB-141 | No Fire-at dispatcher or test on main. |
| CMB-142 | No Garrison entry dispatcher or test on main. |
| CMB-146 | Transport behavior belongs to the MOV stream. |
| CMB-147 | Field works belong to the ENG stream. |
| CMB-148 | Support commands belong to the MOV stream. |
| CMB-149 | Build and production behavior belong to the BASE stream. |
| CMB-150 | Recovery, spawn and convoy behavior belong to the MODE, TOOL and LOG streams. |
| CMB-151 | No Unreal denial-sentence table comparison. |
| CMB-154 | Client infantry gait and corpse presentation have no native witness. |
| CMB-155 | Client picking and selection have no native witness. |
| CMB-156 | Client control groups have no native witness. |
| CMB-158 | No complete seeded native replay or outcome witness. |
| CMB-159 | No deterministic purpose-stream witness in this extraction. Native purpose streams are permitted by PARITY.md. |

## CMB-WEAK follow-up

The follow-up to merged PR #278 strengthens five row witnesses. It replaces the CMB-123 direct vision-pass test with `cmb123_vision_is_team_shared_and_refreshes_through_match_ticks`, which observes team visibility at ticks 201 and 205 and its cached value between them. The earlier receipt above belongs to the original test body and name.

| Row | Strengthened witness |
| --- | --- |
| CMB-86 | `cmb86_target_threat_hysteresis_visibility_and_explicit_lock`: a closer challenger inside the margin stays rejected, then one outside it takes over |
| CMB-104 | `cmb104_109_grenades_satchel_and_petard_impact_hurt_both_teams_and_structures`: Rifle Grenade and Medium HE impact enemy and friendly infantry, a vehicle and a registered structural section |
| CMB-109 | The same impact test covers Ranger and Commando satchels and Churchill Petard |
| CMB-123 | `cmb123_vision_is_team_shared_and_refreshes_through_match_ticks`: automatic refresh through the Match tick, shared with a teammate that has no observer |
| CMB-139 | `cmb139_attack_denies_missing_allied_hidden_and_unarmed`: a visible airborne enemy rejects a ground attack order with Needs and preserves the prior ground target |

The impact test places the structure beside the trajectory so direct contact cannot satisfy its blast assertion. It also checks an outside-radius unit and verifies that grenade-ticket cleanup cannot apply damage twice.

CMB-84 gains `cmb84_aircraft_hold_fire_guards_patrol_anti_air_and_support_interception`, including positive anti-air and interception controls. The complete row is dropped from this follow-up's proven set: automatic aircraft ground fire, explicit ground attack and fighter hunting remain unavailable in `match_aviation.rs`. Implementing those actuators is outside this small test follow-up. This does not change a parity status.

Prototype source read: `shared/sim.js:2199-2255`, `3495-3510`, `3928-3946`, `4379-4441`, `4702-4742`, `5320`, `5416-5428` and `5614-5622` at `e4e579a`. The follow-up passed all 46 `combat_feature_tests::` library tests on the Vostro (162 library tests skipped), in S3 under the required 6 GiB memory and zero-swap limit. An initial build found a Hold Position fixture-field error; the first executed selection then found stale startup production rows in the vision fixture. Both setup errors were corrected before the complete passing module run. `cargo +1.99.0 fmt --all --check --manifest-path rust/Cargo.toml` and strict locked workspace/all-target Clippy also passed, in S3 under the same limits. Both successful jobs released their slot on exit.
