# Native combat feature batches

Owner decision: [Axel rules 1 to 5](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267). Behavior checks use native tests and prototype intent review. Existing exact pure-function/component comparisons and historic failures remain unchanged. No behavior recorder, captured fixture or provenance bundle is added.

Physical checkout: `t3-6fa79355`. Input head: `b2eefab2293b4ad50ef4f63868319cd0d12a9992`. The first candidate window contains 50 assigned IDs; the second contains 16. All new tests are unrun. CMB-88 retains only its producing-head result. No other row is promoted. Root owns execution grants, combined combat allocation, review and publication.

The literals below name actual registered tests. A partial or held row is excluded from promotion even if its listed component test passes. The prototype references are the original row references at `e4e579a`, read alongside the native providers.

| Row | Native test | Prototype reference | Qualification or remaining gap |
| --- | --- | --- | --- |
| CMB-82 | `combat_feature_tests::cmb82_medics_choose_worst_heal_calm_and_recent_wounds_and_seek`; `combat_feature_tests::cmb82_medic_pass_is_every_ten_ticks_and_provisions_cap_healing` | shared/sim.js:109, 4947-4968, 5654 | Unrun. Medic cadence/provisions and healing/seeking are both represented. |
| CMB-84 | `combat_feature_tests::cmb84_hold_fire_blocks_automatic_fire_and_grenades_but_allows_attack` | shared/sim.js:5569, 3864, 2242, 4702, 2212, 4094 | Unrun. Ground hold-fire only; aircraft, AA and flak interception are not covered by this new test. |
| CMB-85 | `combat_feature_tests::cmb85_hold_position_keeps_exact_destination_and_blocks_idle_spacing` | shared/sim.js:4051, 4090 | Unrun. Exact goal/spacing checked; explicit Take Cover while held still needs a witness. |
| CMB-86 | `combat_feature_tests::cmb86_target_threat_hysteresis_visibility_and_explicit_lock` | shared/sim.js:3921, 3928-3946, 135-136 | Unrun. Threat/hysteresis/lock covered; role and armor combinations retain existing exact component tests. |
| CMB-87 | `combat_feature_tests::cmb87_retarget_waits_then_replaces_dead_target` | shared/sim.js:5569-5570 | Unrun; full intent review and actual native result pending. |
| CMB-88 | `match_firing::six_shooters_rebuild_and_update_actual_volley_claims` | shared/sim.js:3840-3852, 5338-5339 | Historical pass at 715c6c54 and single-row review retained; no successor execution claimed. |
| CMB-89 | `combat_feature_tests::cmb89_salvo_crowds_ignore_hidden_and_riding_neighbors` | shared/sim.js:3932, 3936-3937 | Unrun. Visible/riding crowd and cover covered; owner filtering and house/trench factors still need review. |
| CMB-90 | `combat_feature_tests::cmb90_attack_chases_stops_locks_drops_unseen_and_pauses_for_satchel` | shared/sim.js:5466-5471 | Unrun; full intent review and actual native result pending. |
| CMB-92 | `combat_feature_tests::cmb92_reaction_seeks_cover_but_crews_orders_and_hold_position_stay` | shared/sim.js:4088-4111 | Unrun. Cover/crew/order/hold gates covered; hurt-vehicle and rally reaction witnesses remain incomplete. |
| CMB-93 | `combat_feature_tests::cmb93_idle_spacing_moves_newer_squad_and_respects_hold_position` | shared/sim.js:4114-4122 | Unrun; full intent review and actual native result pending. |
| CMB-94 | `combat_feature_tests::cmb94_threat_prefers_visible_armed_then_recent_hit_then_move_direction` | shared/sim.js:3989-3999 | Unrun; full intent review and actual native result pending. |
| CMB-95 | `combat_feature_tests::cmb95_spot_search_prefers_shelter_and_excludes_held_or_unreachable_cells` | shared/sim.js:2404, 3950-3986 | Unrun. Shelter/held/blocker checks covered; capture-circle, height and straight-line cases need review. |
| CMB-96 | `combat_feature_tests::cmb96_shared_plain_move_spreads_and_hold_position_keeps_single_click` | shared/sim.js:4023-4068 | Unrun. Shared spread/hold covered; World unseen and water/cliff fallback remain incomplete. |
| CMB-97 | `combat_feature_tests::cmb97_take_cover_refusals_and_queue_cancellation` | shared/sim.js:3580-3601, 2411-2414 | Unrun; full intent review and actual native result pending. |
| CMB-98 | `combat_feature_tests::cmb98_hull_turns_to_visible_anti_tank_threat_before_ordered_face` | shared/sim.js:5551-5555, 5580, 4077-4081 | Unrun. Visible anti-tank target priority covered; recent-fire expiry and other hull profiles remain incomplete. |
| CMB-99 | `combat_feature_tests::cmb99_short_engaged_move_behind_reverses_long_move_does_not` | shared/sim.js:4071-4076, 135-136 | Unrun. Reverse order selection covered; physical reverse speed remains a MOV concern. |
| CMB-100 | `combat_feature_tests::cmb100_facing_validation_shared_spots_arrival_and_replacement` | shared/sim.js:3422-3423, 3431-3436, 4005-4018 | Unrun. Validation/shared spots/infantry arrival/replacement covered; queued, attack-move and vehicle cases remain incomplete. |
| CMB-101 | `combat_feature_tests::cmb101_formation_shapes_rear_ranks_drag_and_spread` | shared/formation.js:8-53 | Unrun; full intent review and actual native result pending. |
| CMB-102 | None yet | client/main.js:1413-1437 | Held: client formation menu and saved groups belong to Unreal; no native sim behavior implements that UI. |
| CMB-103 | `combat_feature_tests::cmb103_together_caps_group_pace_and_queued_leg_drops_it` | shared/sim.js:3446, 3449, 5483-5484 | Unrun; full intent review and actual native result pending. |
| CMB-104 | `combat_feature_tests::cmb104_109_113_aimed_families_launch_once_and_manual_halts_auto_keeps_route` | shared/sim.js:191, 248, 5422-5428 | Unrun. Launch/route preservation covered; blast damage to allies and per-model ammo remain incomplete. |
| CMB-106 | `combat_feature_tests::cmb106_ap_loads_once_and_sets_family_cooldown` | shared/sim.js:197, 288, 315, 273, 3623, 4166-4167, 4199 | Unrun. Load/cooldown covered; vehicle damage multiplier/consumption and naval AP remain incomplete. |
| CMB-107 | `combat_feature_tests::cmb107_smoke_uses_unit_id_radius_duration_and_cooldown` | shared/sim.js:200, 243, 3624, 5639 | Unrun. Cloud id/radius/duration/cooldown covered; rain decay still needs a native witness. |
| CMB-108 | `combat_feature_tests::cmb108_barrage_uses_family_shell_count_and_reload` | shared/sim.js:209, 235, 294, 280, 5420-5421 | Unrun. Launch/reload covered; destroyer behavior remains unrepresented in this scene. |
| CMB-109 | `combat_feature_tests::cmb104_109_113_aimed_families_launch_once_and_manual_halts_auto_keeps_route` | shared/sim.js:311, 328, 324 | Unrun. Launch family/route preservation covered; house destruction remains incomplete. |
| CMB-110 | `combat_feature_tests::cmb110_ura_clears_suppression_sprints_and_cools_down` | shared/sim.js:319, 3622, 5484, 4217 | Unrun. Suppression clear/sprint/cooldown covered; movement speed and suppression immunity need review. |
| CMB-111 | `combat_feature_tests::cmb111_mixed_selection_cooldown_retreat_missing_spot_and_deferred_queue` | shared/sim.js:3602-3628 | Unrun. Cooldown/retreat/mixed/deferred queue covered; ammo/Munitions and real work cancellation remain incomplete. |
| CMB-112 | `combat_feature_tests::cmb112_munitions_costs_respect_mode_and_explicit_family_override` | shared/sim.js:519-529 | Unrun. Pure mode cost checks retain existing exact economy fixture; pay-on-arrival still needs an integrated construction-mode witness. |
| CMB-113 | `combat_feature_tests::cmb104_109_113_aimed_families_launch_once_and_manual_halts_auto_keeps_route` | shared/sim.js:5417-5431 | Unrun. Manual halt/auto route retention covered; unreachable spot and actual construction cancellation remain incomplete. |
| CMB-114 | `combat_feature_tests::cmb114_autocast_boolean_ownership_ability_and_free_default` | shared/sim.js:3629-3634, 1787 | Unrun; full intent review and actual native result pending. |
| CMB-115 | `combat_feature_tests::cmb115_autocast_staggers_and_preserves_moving_ura_route` | shared/sim.js:3856, 5369-5372 | Unrun. Staggered Ura activation and cooldown gate covered; retreat and aimed-in-flight scheduler gates need review. |
| CMB-116 | `combat_feature_tests::cmb116_grenade_requires_visible_sheltered_enemy_and_clear_blast` | shared/sim.js:3865-3871 | Unrun. Shelter/visibility/friend rejection covered; garrison and nearest-candidate cases remain incomplete. |
| CMB-117 | `combat_feature_tests::cmb117_mg_suppression_requires_stationary_gun_and_approaching_enemy` | shared/sim.js:3872-3876 | Unrun. Approach/retreat/stationary gates covered; explicit shootability obstruction still needs a witness. |
| CMB-121 | `combat_feature_tests::cmb121_barrage_requires_visible_crowd_or_dug_target_and_no_friend` | shared/sim.js:3884-3894 | Unrun. Crowd/dug/friend checks covered; buildings/min-range exclusions need review. |
| CMB-122 | `combat_feature_tests::cmb122_satchel_requires_seen_ordered_structure_with_safe_reach` | shared/sim.js:3857, 3895-3899 | Unrun. Ordered structure/visibility/reach covered; garrison and allied blast safety need review. |
| CMB-123 | `combat_feature_tests::cmb123_vision_is_team_shared_and_changes_on_native_pass` | shared/sim.js:5320, 4379-4441 | Partial native team-sharing test; scheduled tick cadence still needs a full runner witness. |
| CMB-126 | `combat_feature_tests::cmb126_airborne_plane_seen_at_sixty_parked_plane_never_sees_or_is_seen` | shared/sim.js:67, 4392, 4420, 5945 | Unrun; full intent review and actual native result pending. |
| CMB-127 | `combat_feature_tests::cmb127_ordinary_bunker_visible_through_fog_but_riders_hidden` | shared/sim.js:4418-4419, 4437-4440 | Partial ordinary bunker/rider visibility test; Horde stragglers and Kaiju reveal are absent in vision.rs. |
| CMB-128 | `combat_feature_tests::cmb128_los_endpoints_woods_smoke_length_and_hills` | shared/sim.js:2790-2809, 2840-2856, 104, 70, 158, 175 | Unrun. Endpoint/woods/smoke/hill checks covered; diagonal/border/off-map rays retain existing exact fixtures. |
| CMB-129 | `combat_feature_tests::cmb129_team_spot_visibility_requires_range_and_clear_ray` | shared/sim.js:1747-1751 | Unrun. Range/blocked ray covered; high-ground/garrison/rain exclusions need review. |
| CMB-130 | None yet | shared/sim.js:4444-4448, 4456 | Held: vision.rs explicitly lacks remembered enemy buildings; no ghost store to exercise. |
| CMB-133 | None yet | shared/sim.js:4450-4453 | Held: vision.rs explicitly lacks neutral guard actor visibility. |
| CMB-134 | `combat_feature_tests::cmb134_snapshot_filters_visibility_locked_target_private_timers_and_flags` | shared/sim.js:5899-5905, 5942-5946 | Unrun. Ground visibility/locked-target/private cd/flags covered; air serializer remains explicitly unsupported. |
| CMB-135 | `combat_feature_tests::cmb135_command_entry_rejects_winner_away_inactive_malformed_and_foreign_ids` | shared/sim.js:3408-3420 | Unrun. Entry/malformed checks covered; forced supply, operative/riding and large ID-cap cases need review. |
| CMB-136 | `combat_feature_tests::cmb136_queue_limit_automatic_step_yields_rally_waits_and_stop_clears` | shared/sim.js:3362-3387, 3437-3438 | Unrun. Move queue/cap/drift/rally/stop covered; other queued command families remain incomplete. |
| CMB-137 | `combat_feature_tests::cmb137_compound_partial_success_outer_queue_and_first_refusal` | shared/sim.js:3397-3406 | Unrun. Partial success/outer queue/first reason covered; ownership and eight-part truncation need review. |
| CMB-138 | `combat_feature_tests::cmb138_mass_move_reaches_every_selected_actor_and_clamps_coordinates` | shared/sim.js:3421-3461 | Unrun. 80 actors/clamp covered; explicit attack-move halt/resume remains incomplete. |
| CMB-139 | `combat_feature_tests::cmb139_attack_denies_missing_allied_hidden_and_unarmed` | shared/sim.js:3495-3510 | Unrun. Missing/allied/hidden/unarmed checks covered; plane target restriction needs a witness. |
| CMB-141 | None yet | shared/sim.js:3462-3477 | Held: Command has no FireAt variant or parser/dispatcher branch. |
| CMB-142 | None yet | shared/sim.js:3478-3494, 2448-2462 | Held: Command has no Garrison variant or entry dispatcher. |
| CMB-143 | None yet | shared/sim.js:3514-3519 | Next window: aircraft Escort branch exists; native mission test still to write. |
| CMB-146 | None yet | shared/sim.js:3520-3570 | Held for MOV transport owner; this inventory entry delegates its behavior there. |
| CMB-147 | None yet | shared/sim.js:3635-3679 | Held for ENG field works owner; this inventory entry delegates its behavior there. |
| CMB-148 | None yet | shared/sim.js:3680-3688 | Held for MOV support owner; no new support dispatcher scope taken here. |
| CMB-149 | None yet | shared/sim.js:3689-3811 | Held for BASE owner; this inventory entry delegates build/production behavior there. |
| CMB-150 | None yet | shared/sim.js:3411-3417, 3812 | Held for MODE/TOOL/LOG owners; this inventory entry delegates recovery/spawn/convoys there. |
| CMB-151 | None yet | client/availability.js:6-23 | Held: the acceptance check needs the Unreal denial-sentence table, absent here. |
| CMB-152 | None yet | shared/command-feedback.js:2-8 | Next window: locate the native server denial throttle; do not substitute command denials for rate limiting. |
| CMB-153 | None yet | shared/squad-men.js:6-34 | Next window: pure squad geometry keeps exact existing Source comparison; no new fixture required. |
| CMB-154 | None yet | DESIGN.md:2556-2563 | Held: visual infantry gait/corpses are client-only. |
| CMB-155 | None yet | DESIGN.md:2565-2567 | Held: Unreal picking/selection has no Rust sim equivalent. |
| CMB-156 | None yet | CONTEXT.md:132-137 | Held: control groups are client-only with no sim check. |
| CMB-157 | None yet | shared/sim.js:5883-5891 | Next window: actual native unit-row layout and flags test, preserving the existing serializer. |
| CMB-158 | None yet | shared/sim.js:5305-5786 | Held for aligned integration owner: no full seeded replay claim from component tests. |
| CMB-159 | None yet | shared/sim.js:772, 2345, 4148, 4171-4172, 4705, 4710, 4773, 4933, 5273-5287, 5625 | Held: production purpose streams differ from the global Source stream; retain historical aligned/unaligned results and require owner handling before a difference claim. |
| CMB-160 | None yet | shared/sim.js:561, 764-769, 3915-3916, 4148, 4171-4173 | Next window: existing pure armor and projectile fixtures remain exact-comparison checks. |

Proposed first-window selection, pending Root grant: one `cargo nextest run --locked -p ww2-core -p ww2-simtest --no-fail-fast --test-threads 1` with filter `test(combat_feature_tests::) | test(=match_firing::six_shooters_rebuild_and_update_actual_volley_claims) | (package(ww2-core) & test(formation::)) | test(=economy::ability_costs_match_reference_objects_and_nullish_overrides)`. No discovery or per-row invocation is authorized by this document.
