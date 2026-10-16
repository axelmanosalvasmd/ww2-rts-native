The native commander already implements detached sightings, camera attention and paced inputs, but lacked direct checks for several behavioral boundaries. Add nine tests for these components, with explicit controls for expiry, screen edges, alerts, scan load, fresh deliveries and difficulty thresholds. Five existing main tests supplement the feature batch.

The proposed proof rows and literal covering tests are:

- AI-08: `match_human_observation::tests::intent_ai08_delivered_sightings_expire_and_stillness_restarts`.
- AI-20: `human_commander_state::tests::intent_ai20_camera_memory_expires_and_own_units_stay_available`.
- AI-21: `human_commander_state::tests::intent_ai21_alert_floor_area_throttle_and_urgent_resume`.
- AI-22: `human_commander_state::tests::intent_ai22_scan_prioritizes_unseen_dots_and_scales_with_input_load`.
- AI-23: `human_commander_state::tests::intent_ai23_plan_waits_for_delivery_at_reached_tick`.
- AI-24: `human_commander_state::tests::intent_ai24_fight_response_retreats_hurt_and_uses_only_idle_nearby_units`.
- AI-27: `human_commander_state::tests::intent_ai27_peak_minute_and_locality_block_without_consuming_inputs`.

At producing commit `5479cfbe`, all 14 focused tests passed once with no failures or retries. The prototype-intent review accepted the seven rows listed above. AI-19 remains held: its passing reset-helper check does not exercise the human take/leave lifecycle. Prior 28/28 proof belongs solely to `0aaea3da` and is not claimed for this PR.

The full assignment has63 remaining rows. The prior source branch proposed12, but four required inherited shipping receipt providers absent from main. This PR imports only the independent test delta on main40fe: AI-01/02/04/06 join the held rows. No shipping provider or prior81-line diagnostic helper is required or imported. Every docs/parity file stays exact main; no status changes are included.

Held56 reasons, detailed individually in docs/testing/ai-native-feature-window1.md and ai-native-feature-window2.md:

- AI-01/02/03/04/05/06/07: complete receipt, private logistics, effects or World terrain producers are missing from this main integration; ordinary prior receipts remain historical evidence.
- AI-09/10/11/12/13/14/15/16/17/18: complete known-building, live Horde, command mirror/union and server scheduling/brain callers are missing. AI-17 has only its pure stagger/window component test here.
- AI-19: the human take/leave lifecycle caller is not exercised by the reset-helper test.
- AI-28/29/30/31/32/33/34/35/36/37/38: supply, situations, strategic notice, wait/mode restrictions, learning, full buying/support/abilities/retreat/chores policies are missing.
- AI-40/41/42/43/44/45/46/47/48/49/50/51/52/53: enemy-objective, defense, base attack, aircraft, focus/operation/armor, full emission, engineering, fortification and full research/base-building policies are missing.
- AI-54/55/56/57/58/59/60/61/62/63/64/65/66: full Classic/Assault/Horde/World policies, planner difficulty branches, live AI seats, durable saves, full privacy mode matrix, whole-match runner and complete planner draw accounting are missing.

Original failures, fixtures and prior producing heads remain unchanged. This adds no runtime, interface, reference, default, gameplay or client adoption change. Normal feature mappings replace a new Source recorder/capture/provenance bundle.
