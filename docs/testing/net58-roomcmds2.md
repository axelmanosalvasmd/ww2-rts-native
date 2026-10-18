# NET-58 ROOM-COMMANDS-2 mapping

The `NET-58` row in `docs/parity/server.md` cites the prototype at
`reference/ww2-rts/shared/sim.js:5965-6009,6079-6088` (`e4e579a`).
The composed implementation retains the inherited tests below and adds focused
operative and Horde checks. This document maps source coverage; the stream PR
records the producing head and execution results.

| NET-58 clause | Registered native test | Evidence scope |
| --- | --- | --- |
| Ordinary position, allied shooter/target, seen shooter/target, reveal, and hidden-ID removal | `recipient_snapshot::shots::event_tests::nonpublic_regular_shots_independently_use_position_allies_or_seen_participants` | Each admission cause is exercised independently. Ordinary hidden IDs disappear; owners remain. |
| Public support shapes | `recipient_snapshot::shots::event_tests::support_arrival_bomb_chutes_and_shotdown_match_prototype_exact_shapes` | Literal support arrival, bomb, chutes and interception rows. |
| Public ordinary identity redaction | `recipient_snapshot::shots::event_tests::flak_public_event_hides_shooter_id_but_retains_prototype_owner_field` | Exact visible and hidden shooter rows. |
| Ordinary anti-air identities | `recipient_snapshot::shots::event_tests::anti_air_and_plane_down_match_shapes_and_redact_each_hidden_identity` | Exact anti-air and plane-down shapes and independent participant redaction. |
| Local events require ground fog and remove hidden paired fields | `recipient_snapshot::shots::event_tests::throw_uses_ground_cell_fog_and_redacts_both_hidden_shooter_fields` | Visible shooter or allied owner cannot expose a local event on hidden ground. |
| Operative ordinary shot and local throw use distinct rules | `recipient_snapshot::operative::tests::operative_shots_keep_prototype_nonlocal_and_local_identity_rules` | Exact small-arms hit including `m`; local throw redaction differs from ordinary fire. |
| Actual sniper and commando hits and misses | `recipient_snapshot::operative::tests::operative_sniper_and_commando_hits_and_misses_deliver_actual_small_arms_rows` | Both weapons fire through `Match::fire_companion`. Enemy damage and drawn-man hits are required. Exact projected rows cover all three seats; misses omit every hit field. Six further ticks collect every seat without a fault. |
| Operative launch and live flight have distinct exact shapes | `recipient_snapshot::operative::tests::operative_grenade_flight_and_launch_rows_match_literal_shapes_and_fog_rules` | Actual grenade creation, literal launch/live rows, ground fog and paired shooter redaction. |
| Actual operative grenade survives shooter death and resolves | `recipient_snapshot::operative::tests::operative_death_respawn_and_full_match_finish_keep_every_recipient_live` | Actual input produces fire and throw. A live grenade remains after shooter removal, emits terminal contact, disappears and releases retained shooter metadata. Every seat and an independent operative view are collected through respawn, natural victory and eight ending ticks. |
| Sandbox breath is public with ordinary redaction | `recipient_snapshot::shots::tests::sandbox_breath_is_public_but_hidden_actor_ids_are_removed` | Literal endpoint and shell rows on hidden ground; the nonsandbox, non-Horde refusal remains. |
| Horde breath and the non-World public rule | `recipient_snapshot::shots::tests::horde_breath_is_public_and_regular_public_status_excludes_world` | Literal Horde endpoint and shell rows for every seat. A pure projector check excludes public-only shots in World and retains reveal admission. It starts no World room. |
| Actual hosted Horde breath and ordinary match lifetime | `lobby::horde_host::tests::mode56_hosted_horde_runs_boss_combat_to_result_and_complete_hold_with_all_seat_frames` | Wave ten emits actual breath. Both connected seats receive a public Kaiju endpoint and every contiguous shell sample. The existing all-three-seat captures, bunker defeat, complete ending hold, result and lobby return remain required. |
| Live flight exact fields and privacy | `recipient_snapshot::flights::tests::live_flight_exact_prototype_shape_and_field_privacy` | Exact prototype fixture, current-cell fog, hidden shooter pair, reveal and future-launch exclusion. |
| Live contact clipping differs from launch extrapolation | `recipient_snapshot::flights::tests::live_flight_until_stops_at_fog_or_visible_static_contact_only` | Fog and visible contact shorten live projection; hidden bodies do not. The same event trajectory remains fog-only. |
| Original ballistic stamp survives projection | `recipient_snapshot::flights::tests::live_flight_fog_clipping_keeps_original_stamp_and_ballistic_trajectory` | Exact float-bit boundary catches rebasing a live flight to a current-row launch event. |
| Actual chronological batches survive ordinary delivery | `recipient_snapshot::shots::event_tests::event_batch_keeps_chronology_through_ordinary_ticks_and_every_recipient_snapshot` | Existing producer ordering through ordinary ticks and all recipients. |
| Rejected event or flight cannot publish partial delivery state | `recipient_snapshot::shots::event_tests::invalid_event_field_refuses_whole_batch_before_delivery_publication`; `recipient_snapshot::flights::tests::malformed_live_flight_refuses_before_delivery_memory_changes` | Later invalid values preserve delivery memory and canonical state. |

Registration is through `recipient_snapshot.rs`, `shots.rs`, `operative.rs` and
`flights.rs` for core unit tests, and `lobby.rs` plus `horde_host.rs` for hosted Horde tests. Cargo's
test inventory must confirm the composed registration before these tests count
as executed evidence. No new integration-test file is introduced.

NET-58 is claimed only for admitted non-World R1 matches after the listed tests
pass on the final head. The World policy fixture does not provide a running
World witness or change World admission. The event contract is
[`room-commands2-events.md`](../contracts/room-commands2-events.md).
