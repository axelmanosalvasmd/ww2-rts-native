# AI native feature window 1 on current main

This test PR starts at literal main `40fe9e296b46060241693c6c627ecd615d4f8ea4`. It imports nine newly authored tests from the prior task: eight attention/scheduling tests and one delivered-sighting test. Five existing main tests supplement them. No shipping provider, inherited branch history, fixture or parity file is imported. The integrated head is unrun until Root's new grant.

Prior native proof remains at `0aaea3dab1b549cdf977e60d829d67387e2f290d`, Rust `2f9cf68cd6c5cdd3cee9819fbdf5a69984bfcd9a`:28 passed,0 failed,146 skipped. That head included receipt/sight providers absent from current main. Its passing AI-01/02/04/06 components and AI-05 successor cannot be transferred to this main test PR. Their exact source and original failures remain on preserved refs; no substitute provider is introduced under cfg(test).

The current full-row candidates are eight: AI-08/19/20/21/22/23/24/27. The other42 first-window rows remain held individually below; the successive13-row map accounts for all55 held rows across the full63-row assignment. Partial tests referenced on held rows may exist only on prior8cb and are historical scope references, not current registrations or current proof.

[Owner decision6206861267](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267) applies without added notes. Each candidate still needs actual integrated native PASS plus reviewer prototype confirmation. Every docs/parity file stays byte-exact main. No new Source recorder, capture, fixture or evidence bundle is used.

| Row | Native or retained historical component test | Prototype at e4e579a | Scope and disposition |
| --- | --- | --- | --- |
| `AI-01` | `human_commander_policy::tests::actual_recipient_decode_preserves_quiet_observer_precision_and_owner_privacy` | `shared/ai-view.js:103-195; shared/ai.js:256-257` | Hold on current main: this prior native witness requires inherited receipt providers absent from main40fe. No runtime or replacement provider is imported. |
| `AI-02` | `human_commander_policy::tests::actual_receipt_adapter_retains_all_present_fields_and_public_metadata` | `shared/ai-view.js:9-21,143` | Hold on current main: this prior native witness requires inherited receipt providers absent from main40fe. No runtime or replacement provider is imported. |
| `AI-03` | `human_commander_policy::tests::actual_receipt_adapter_retains_all_present_fields_and_public_metadata` | `shared/ai-view.js:23-57,164-177` | Drop: active aircraft and complete private logistics producer coverage are missing. |
| `AI-04` | `human_commander_policy::tests::actual_receipt_adapter_retains_all_present_fields_and_public_metadata` | `shared/ai-view.js:144,161-163` | Hold on current main: this prior native witness requires inherited receipt providers absent from main40fe. No runtime or replacement provider is imported. |
| `AI-05` | `human_commander_policy::tests::actual_received_terrain_keeps_public_bridge_map_and_delivered_history` | `shared/ai-view.js:74-137; shared/world-layers.js:16-24; shared/sim.js:2731-2740` | Drop complete row: ordinary history passed only on prior0aaea3da; complete World unknown terrain and World human takeover remain guarded. |
| `AI-06` | `human_commander_policy::tests::intent_ai06_world_objectives_use_first_team_seat_and_never_cut` | `shared/ai-view.js:146-148` | Hold on current main: this prior native witness requires inherited receipt providers absent from main40fe. No runtime or replacement provider is imported. |
| `AI-07` | `match_human_observation::tests::original_observations_preserve_own_goals_visibility_sightings_and_complete_packet` | `shared/ai-view.js:149-158` | Drop: public receipt retention is covered, but complete planner effect decoding/consumption is missing. |
| `AI-08` | `match_human_observation::tests::intent_ai08_delivered_sightings_expire_and_stillness_restarts` | `shared/ai-view.js:59-72` | Candidate with original_observations_preserve_own_goals_visibility_sightings_and_complete_packet: value, motion tolerance, lost sight and 60-second expiry. |
| `AI-09` | None for complete behavior | `shared/ai-view.js:179-192` | Drop: known-building ghosts and resource-node claim policy are absent from the quiet planner. |
| `AI-10` | `match_human_observation::tests::owner_hurt_projection_preserves_reset_append_and_raw_ordinals` | `shared/ai-human.js:55-117; shared/ai.js:248-252` | Drop: live Horde wave guard and full host event integration are missing. |
| `AI-11` | `human_commander_policy::tests::actual_planner_moves_replay_as_human_commands_through_full_native_ticks` | `shared/ai.js:253; shared/ai-human.js:291-293` | Drop: real canonical Move admission is tested, but the complete AI command family and operation caller are missing. |
| `AI-12` | None for complete behavior | `shared/ai.js:260-287; shared/ai-human.js:168` | Drop: broad local spending/flag mirror and refused-garrison planning branch are absent. |
| `AI-13` | None for complete behavior | `shared/ai.js:41-1068` | Drop: raw_command supports Move, Cover, Buy and Technology only; complete output union is absent. |
| `AI-14` | `human_commander_policy::tests::actual_native_handover_waits_for_delivery_before_its_first_due_plan` | `shared/ai-schedule.js:25-35` | Drop: handover component exists, but real all-seat observe-loop then act-loop server integration is missing. |
| `AI-15` | `match_commander::tests::actual_source_observations_and_vision_phase_match` | `server/room-tick.js:14,45-46; shared/ai-schedule.js:65` | Drop: fixed delivery component is present; adaptive 2-to-4 live snapshot scheduling is not implemented here. |
| `AI-16` | None for complete behavior | `shared/ai-schedule.js:17-23` | Drop: live ordinary/Horde brain selection is not connected. |
| `AI-17` | `human_commander_state::tests::intent_ai17_scripted_stagger_and_four_tick_view_window` | `shared/ai-schedule.js:12-16,29-31` | Drop complete row: pure stagger/lead window is tested; real Horde always-observe and Normal-only seat binding are missing. |
| `AI-18` | None for complete behavior | `shared/ai.js:173-211` | Drop: Horde movement FIFO, four-order flush, 240-order bound and wave reset are absent. |
| `AI-19` | `human_commander_state::tests::intent_ai19_lazy_opening_and_handover_reset` | `shared/ai-human.js:55-78` | Candidate for lazy spawn camera, private opening timing and reset; live seat lifecycle remains a separate caller limit. |
| `AI-20` | `human_commander_state::tests::intent_ai20_camera_memory_expires_and_own_units_stay_available` | `shared/ai-human.js:12-14,92-100,180-188` | Candidate with actual_receipt_adapter_filters_camera_units_without_losing_own_rows: camera-seen enemies, 30-second memory and own rows. |
| `AI-21` | `human_commander_state::tests::intent_ai21_alert_floor_area_throttle_and_urgent_resume` | `shared/ai-human.js:102-163,175-177` | Candidate with original_constructed_caps_locality_and_urgent_resume_preserve_motor_state: alert floor, area throttle and preserved interrupted orders. |
| `AI-22` | `human_commander_state::tests::intent_ai22_scan_prioritizes_unseen_dots_and_scales_with_input_load` | `shared/ai-human.js:127-147` | Candidate: nearest unknown dots, own groups/base rotation and fourfold scan load. |
| `AI-23` | `human_commander_state::tests::intent_ai23_plan_waits_for_delivery_at_reached_tick` | `shared/ai-human.js:271-279` | Candidate: stale views do not invoke the callback; fresh delivered tick releases the plan once. |
| `AI-24` | `human_commander_state::tests::intent_ai24_fight_response_retreats_hurt_and_uses_only_idle_nearby_units` | `shared/ai-human.js:190-207` | Candidate: retreat hurt victim; attack camera-seen foe; exclude busy, already ordered and distant own units. |
| `AI-27` | `human_commander_state::tests::intent_ai27_peak_minute_and_locality_block_without_consuming_inputs` | `shared/ai-human.js:271-298` | Candidate with original_constructed_caps_locality_and_urgent_resume_preserve_motor_state: peak/minute caps, five-tick locality, motor delay and one input per tick. |
| `AI-28` | None for complete behavior | `shared/ai.js:291-308` | Drop: logistics retreat/recovery policy is absent. |
| `AI-29` | None for complete behavior | `shared/ai.js:350-391` | Drop: defense/wait/push situation state and threat selection are absent. |
| `AI-30` | None for complete behavior | `shared/ai.js:356-381` | Drop: contact/paradrop notice memory is absent; damage-alert timing is a different feature. |
| `AI-31` | `commander_ai::original_thinker_orders_aim_and_waypoint_draws_match` | `shared/ai.js:762-778` | Drop: quiet scores exist, but wait threat avoidance and Assault/Horde 70-metre restrictions are absent. |
| `AI-32` | None for complete behavior | `shared/ai-mind.js:21-94` | Drop: loss respect, recapture lessons and adaptive purchase substitution are absent. |
| `AI-33` | `match_commander::tests::germany_purchase_publishes_real_mg_before_next_delivered_movement` | `shared/ai.js:392-438` | Drop: Germany MG component exists; complete adaptive chain, faction swaps, fallback and engineer policy are absent. |
| `AI-34` | None for complete behavior | `shared/ai.js:500-536` | Drop: off-map support decision chain is absent. |
| `AI-35` | None for complete behavior | `shared/ai.js:603-617` | Drop: watched target ability selection and per-look cast caps are absent. |
| `AI-36` | None for complete behavior | `shared/ai.js:671-681` | Drop: strategic hurt/last-model/suppression retreat and reinforcement waiting are absent. |
| `AI-37` | None for complete behavior | `shared/ai.js:343-346,491-492,846-847` | Drop: fight-triggered and Horde auto-retreat arming policy is absent. |
| `AI-38` | None for complete behavior | `shared/ai.js:673-681` | Drop: busy-unit guards exist; medic centre movement and vehicle shelling chores are absent. |
| `AI-40` | None for complete behavior | `shared/ai.js:790-809` | Drop: enemy objectives are guarded; assault value/ratio/failure/smoke gates are absent. |
| `AI-41` | None for complete behavior | `shared/ai.js:570-585` | Drop: depot/point defense allocation is absent. |
| `AI-42` | None for complete behavior | `shared/ai.js:311-316,586-589,753-761` | Drop: mode target lists, weather army threshold and push gating are absent. |
| `AI-43` | None for complete behavior | `shared/ai.js:591-602` | Drop: aircraft planning is explicitly unsupported. |
| `AI-44` | None for complete behavior | `shared/ai.js:810-845` | Drop: Hard focus-fire selection and onward-order preservation are absent. |
| `AI-45` | None for complete behavior | `shared/ai-mind.js:133-270; shared/ai.js:855-862` | Drop: staged assault state machine, manual release and timing are absent. |
| `AI-46` | None for complete behavior | `shared/ai.js:618-628,848-853` | Drop: remembered armor dispatch and unsafe rifle-order filtering are absent. |
| `AI-47` | None for complete behavior | `shared/ai.js:863-875` | Drop: quiet Move/attack-move order exists; retreat/operation ordering and full decision record are absent. |
| `AI-48` | None for complete behavior | `shared/ai.js:41-143` | Drop: bridge rebuilding, fill and demine planning with 90-second dispatch memory are absent. |
| `AI-49` | None for complete behavior | `shared/ai.js:718-748` | Drop: immediate Cover component exists; paid mine/trench/wire/traps and house decisions are absent. |
| `AI-50` | None for complete behavior | `shared/ai.js:902-958` | Drop: HQ fortification stages and reserves are absent. |
| `AI-51` | `match_commander::tests::later_research_attempts_use_delivered_labs_and_real_typed_admission` | `shared/ai.js:966-979,331-332` | Drop: bounded research component exists; full mode/Horde exclusion, savings and Armory progression coverage is incomplete. |
| `AI-52` | None for complete behavior | `shared/ai.js:878-900` | Drop: base construction/repair/site-search policy is absent. |
| `AI-53` | None for complete behavior | `shared/ai.js:981-1004` | Drop: forward-HQ placement and exclusions are absent. |
