# ROOM-AI resumed qualification

Resume branch `runtime/room-ai2-20261016` starts at WIP `c647b2733fd3051c326a5406e489f7d7ef20954b`. The original failure and diagnostic runs remain attributed to that WIP in [the blocked note](room-ai-20261016-blocked.md). They do not qualify this branch.

This PR stacks on #369 and #342; merges after #373. Prototype remains `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

## Merge record

- Merge `8acca003` imports #369 branch `runtime/flights-events-20261016` at `f1214e12654b98344b6e874a905713cb53f837f0`. Automatic merge, no hand-edited hunks.
- Merge `0a7a2011` imports #373 branch `fix/main-elimination-serde-20261016` at `4edfcd5e8b795aa47fde114de7d51d7eb6430078`. Two conflicted hunks were resolved additively. In `lobby.rs`, retain both `room_ai_tests` and `save_load_tests` behind their own test attributes, plus the unconditional `save_runtime` module. In `room_runtime.rs`, retain all AI access/activation/reset/schedule methods and all save/hold/event restoration methods, closing the AI method before the save methods. No assertion was removed.
- Merge `9a30fc4a` imports #342 branch `runtime/ai-batch6-20261016` at `f27dc9c8b75f55a764043e126a4e0ce79e1c69b0`. Automatic merge, no hand-edited hunks. Its scheduler and test registrations remain intact.
- Merge `81cd6485` imports main `fd714e36430cdfac864e6af00f346067c84d7863`. Automatic documentation-only merge, no hand-edited hunks.

## Intended row mapping

- AI-63: `ai63_hosted_save_restores_planners_resets_humans_and_restarts_commanders`; `ai63_planner_checkpoint_retains_full_memory_without_random_streams`; `ai63_mind_checkpoint_retains_every_notice_and_lesson`.
- NET-48: `net48_leave_preserves_army_clears_identity_and_ignores_departed_socket` plus the ordinary full-match witness.
- NET-49: `net49_hand_ai_requires_host_offline_integer_seat_and_resumes_its_drop_pause`; `net49_handover_preserves_host_pause_and_other_seats_drop_pause`; `net49_new_online_host_can_handover_before_old_socket_disconnect_callback` plus the ordinary full-match witness.

Full-match witnesses: `room_ai_collects_all_requested_seats_before_planning_and_survives_natural_finish` and `room_ai_host_callbacks_survive_natural_finish_and_final_snapshots`, standard Conquest map, seed 3936341, standard army and victory target, both humans handed to AI in the direct runtime witness. The Hub witness keeps the human host connected and acknowledges its snapshots while the other seat is handed to AI. Both must reach natural victory and collect every seat's snapshots through advancing terminal ticks without fault. The Hub witness uses only actual snapshot cadence and recipient selection during play. Its extra final recipient checks begin after victory is fixed, when commanders have stopped.

AI-62 remains unclaimed as a complete row: ordinary level selection, Normal handover defaults and equal resource/view rules are tested, but hosted Horde is outside this batch. NET-40 remains unclaimed as a complete row: ordinary AI startup and loaded ordering are covered, but Horde, spectator and operative publication remain separate streams. No row is promoted from partial evidence. No parity status file is edited by this stream.

## Validation

Final qualification runs: focused room and core checkpoint tests, affected server package tests, formatting and strict workspace all-target Clippy under a shared slot with 6 GiB, zero swap and four jobs. The final results are reported in the PR body with the producing commit and Rust tree. No full workspace test suite or prototype execution is planned.

Acknowledged: owner L, recipient-view room AI only, as relayed in #5 comment 6217590741 and confirmed in 6218552386. The live-flight and AI seat-delivery dependencies are preserved. Hermes' exact-head runtime acknowledgment, CI, parity status updates and merge remain with the orchestrator. No issue comments are posted.

Independent source review found four missing valid assault reason names in the new checkpoint adapter. All live emitted names now round-trip in the complete planner-memory test; the all-memory fallback remains only for actual clone failures. The review also requested the Hub-driven full-match witness above. The reviewer did not execute tests.

The first resumed build acquired S3 and failed before tests. Merging #342 changed `Context.nodes` from positions to typed node facts; the WIP decoder now maps delivered positions, rates and fuel flags to that type without inferring visibility from a public position. The first Serde derives also inferred input borrows for static rule names; type aliases keep the custom owned decoders authoritative. No native test passed or failed in that compile attempt. Its log is `/tmp/roomai2-focused-initial.log`.

The second attempt compiled and ran 19 of 20 focused tests: 17 passed and 2 failed; nextest canceled one remaining process test. The original natural-finish witness passed beyond the old tick-309 failure. The new Hub witness incorrectly required a delivered packet on tick 1, before its first send beat; it now waits for the initial tick-2 beat. The active-match save witness exposed missing native string restoration: authored house structures use `house` (`structures.rs:26`), and physical grenades use `boom` (`match_firing.rs:2497`), neither previously in the compiled string registry or catalog. Both fixed compiled names are now admitted; unknown names report their value. The map, combat, standard army and victory target are retained. Log: `/tmp/roomai2-focused-second.log`.

The third run passed 25 of 27 focused checks, including the real WebSocket AI start, all existing save/load tests and the original full-match witness. Its two new witness assertions were overbroad. `Hub::poll_native` expires rooms after 60 seconds without any human sockets (`lobby.rs:1396,2066-2071`), so the Hub fixture now keeps a connected host and completes its real snapshot receipts. The save round-trip difference was inspected from both full values: only `routing_fog.kept` and `routing_fog.teams` changed when the loaded start published fresh recipient observations (`match_routing.rs:429-442`). The test still compares the complete checkpoint byte-equal before observation, then compares the whole loaded checkpoint against an independently observed recipient baseline. The full AI memory entry remains byte-equal before its opening look. Log: `/tmp/roomai2-focused-third.log`.

The fourth attempt failed during server compilation without rebuilding core. Its cached S3 core dep-info omitted `ai_view/commander.rs`, despite the current checkout declaring and exporting that module and its checkpoint APIs. The compiler consequently reported those current APIs absent. No test ran. Qualification now refreshes the current checkout's crate entrypoint timestamps inside the held slot, forcing artifacts to match this source before tests and Clippy. Source bytes and the Rust tree are unchanged. Log: `/tmp/roomai2-focused-fourth.log`.

After rebuilding core, the fifth run passed the actual Hub natural-finish witness. Its remaining loaded-checkpoint assertion used an AI-only baseline (`omit:true`), whereas a start includes browser fog (`omit:false`). `recipient_snapshot/delivery.rs:87-98` skips team-fog publication for the AI-only selection. The expected start baseline now uses the actual start selection, retaining the whole-checkpoint comparison and every memory/reset assertion. Log: `/tmp/roomai2-focused-fifth.log`.

The sixth run passed the whole loaded-checkpoint comparison, human-memory reset, restored difficulty and fresh commander state. Its resumed-tick check then exposed a WIP runtime bug: `RoomAi::observe` used game tick as the shot-array generation. A scheduled frame with hurt events followed by a current frame at that same tick replaced the array with an empty one under the old generation, so `HumanState::read_alerts` correctly refused the shrinking batch (`human_commander_state.rs:409-414`). Each delivered frame now gets a checked monotonic generation, matching the existing hurt-batch contract that identity changes whenever the array is replaced. The snapshot probe and all post-load assertions remain. Log: `/tmp/roomai2-focused-sixth.log`.
