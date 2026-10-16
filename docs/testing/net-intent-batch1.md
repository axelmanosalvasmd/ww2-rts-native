# NET native-intent batch 1

Branch starts at main `40fe9e296b46060241693c6c627ecd615d4f8ea4`. Only owned tests and their registrations were extracted from `41f1d343..29c8d3ca`. The unpublished runtime work in parent `41f1d343` was not imported. New tests exercise additional behavior already on main. Shipping Rust, wire protocol, privacy rules, ABI, reference pin and parity tables remain unchanged.

The table covers all 82 assigned rows against `REVIEWER_1-expectations.md`. Sixteen rows have proposed complete native witnesses. The other 66 are excluded from this PR's proven count, with individual reasons below. Partial tests remain useful but qualify no additional row. These are test results for orchestrator review, not status promotions.

Axel approved native behavior tests plus prototype-intent review in [owner decision 6206861267](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267). Prototype anchors below refer to `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. No Source recorder, capture or provenance bundle was added.

NET-31 uses the synchronous catalogue under the Hub lock: stale or nonhost requests cannot mutate it and later valid mode requests win. NET-62 exercises the recipient producer's AI mask-omission option, not an AI scheduler. NET-81 covers pause payloads and delivery; it does not claim host pause wire admission, which is held under NET-79. NET-61/83/84 use the existing trusted fresh-scene finish actuator, the real 120-callback scheduler hold and actual result publication. They do not claim a naturally played whole match.

## Proposed complete witnesses

| Row | Literal tests | Prototype |
| --- | --- | --- |
| NET-01 | `net_batch50_bind::net_batch50_bind_defaults_overrides_and_port_zero_are_reachable`<br>`net_batch50_http::port_zero_directory_and_public_map_requests_use_the_real_router` | `server.js:29`, `server.js:812-815` |
| NET-05 | `lobby::lobby_socket_tests::net_batch50_socket::malformed_frames_before_and_after_hello_leave_the_room_usable`<br>`lobby::net_batch50::more::client_rejects_malformed_or_non_string_tags_without_changing_state` | `server.js:584-589`, `client/connection.js:35-39` |
| NET-09 | `lobby::net_batch50::creation_directory_capacity_and_invite_preserve_public_boundaries`<br>`lobby::net_batch50::directory_marks_playing_rooms_unjoinable_without_publishing_personal_fields`<br>`lobby::net_batch50::more::directory_counts_reserved_seats_and_hides_empty_or_full_rooms`<br>`net_batch50_http::port_zero_directory_and_public_map_requests_use_the_real_router` | `server.js:137-144` |
| NET-27 | `lobby::net_batch50::hello_rename_and_stale_membership_use_current_people`<br>`lobby::net_batch50::more::replaced_and_freed_connections_cannot_queue_army_orders_or_get_feedback` | `server.js:619-620` |
| NET-31 | `lobby::net_batch50::host_settings_validate_all_choices_and_serialize_replacement_requests`<br>`lobby::net_batch50::attached_match_blocks_lobby_mutations_and_preserves_command_window_on_rejoin`<br>`lobby::net_batch50::hello_rename_and_stale_membership_use_current_people` | `server.js:649-681` |
| NET-33 | `lobby::net_batch50::ai_level_kick_team_and_faction_enforce_authority_and_integer_types`<br>`lobby::net_batch50::more::in_play_kick_keeps_an_offline_seat_that_lobby_kick_would_remove` | `server.js:644-646` |
| NET-34 | `lobby::net_batch50::ai_level_kick_team_and_faction_enforce_authority_and_integer_types`<br>`lobby::net_batch50::attached_match_blocks_lobby_mutations_and_preserves_command_window_on_rejoin` | `server.js:655-658` |
| NET-44 | `lobby::net_batch50::command_preamble_blocks_finished_out_away_and_whole_malformed_orders` | `shared/sim.js:3408-3410` |
| NET-45 | `lobby::net_batch50::attached_match_blocks_lobby_mutations_and_preserves_command_window_on_rejoin` | `shared/command-feedback.js:2-7`, `server.js:596-602`, `server.js:713-714` |
| NET-60 | `lobby::net_batch50::more::contested_point_requires_two_visible_on_point_teams` | `shared/sim.js:6063-6068` |
| NET-61 | `lobby::net_batch50::more::end_hold_reveals_hidden_units_refuses_orders_and_preserves_personal_results` | `shared/sim.js:5810-5814`, `shared/sim.js:5945`, `server.js:743-753` |
| NET-62 | `lobby::net_batch50::more::ai_mask_omission_keeps_hidden_units_and_changed_terrain_private` | `shared/sim.js:6042`, `shared/sim.js:6077`, `server/room-tick.js:15-21` |
| NET-68 | `lobby::net_batch50::more::reconnect_start_clears_client_units_and_replaces_existing_delta_history` | `server.js:455-459`, `shared/sim.js:5948-5963`, `client/main.js:501-506` |
| NET-81 | `lobby::net_batch50::more::pause_fields_round_deadlines_and_survive_paused_join_and_periodic_delivery` | `server.js:467-478`, `server.js:481-490`, `server.js:514-516` |
| NET-83 | `lobby::net_batch50::more::end_hold_reveals_hidden_units_refuses_orders_and_preserves_personal_results` | `server.js:533-565` |
| NET-84 | `lobby::net_batch50::more::end_hold_reveals_hidden_units_refuses_orders_and_preserves_personal_results` | `server.js:347-348`, `server.js:752-753` |

## Excluded rows

Dropped from this test batch only. Existing parity statuses and old evidence remain unchanged.

| Row | Reason |
| --- | --- |
| NET-04 | No live permessage-deflate negotiation or threshold. |
| NET-06 | Browser static prefixes and MIME/path-confinement consumer absent. |
| NET-07 | Static ETag, conditional 304 and gzip cache absent. |
| NET-08 | Browser routes return 404. |
| NET-11 | Authenticated raw map reads unavailable. |
| NET-12 | Editor writes, persistence and invalidation unavailable. |
| NET-13 | Editor password generation and comparison unavailable. |
| NET-14 | Tailscale fallback and native invite-link substitution absent. |
| NET-15 | UTF-16 title truncation can split a surrogate; native lossy conversion differs and remains unresolved. |
| NET-16 | Binary MAX_ROOMS configuration absent; existing Config/Hub capacity tests are partial. |
| NET-18 | Tutorial auto-start unavailable. |
| NET-20 | Main retains trailing U+FEFF; UTF-16 display boundaries also remain unresolved. |
| NET-21 | In-play spectator admission unavailable. |
| NET-25 | Operative rejoin and human AI/helper resets unavailable. |
| NET-26 | Operative and spectator recipient starts unavailable. |
| NET-28 | Operative broadcasts and synchronous save listing unavailable. |
| NET-29 | Operative rename unavailable. |
| NET-32 | Lobby AI difficulty works, but an eligible in-play AI seat cannot be admitted; current in-play human-slot probe is not decisive. |
| NET-35 | Lobby transitions work, but an in-play spectator sit guard cannot be exercised through supported attachment. |
| NET-36 | General public start and Assault readiness unavailable. |
| NET-37 | Asynchronous start cancellation and general rollback unavailable. |
| NET-38 | Public World start unavailable. |
| NET-39 | General mode/scenario option mapping unavailable beyond trusted Conquest. |
| NET-40 | AI/Horde reset and all-role publication unavailable. |
| NET-41 | General restart contexts unavailable beyond trusted Conquest. |
| NET-42 | World remembered start map/spawn projection unavailable. |
| NET-43 | Supported ingress is limited to four army command families. |
| NET-46 | Operative and active spectator command paths unavailable. |
| NET-47 | Logistics away-state activity not covered by ordinary wallet check. |
| NET-48 | Leave with persistent AI takeover unavailable. |
| NET-49 | Offline-human army handover unavailable. |
| NET-50 | Horde nextwave runtime unavailable. |
| NET-51 | Manual end and pending-start cancellation unavailable. |
| NET-52 | Plane projection unavailable. |
| NET-53 | Cargo/uncached flag-retention distinction lacks an uncached production projection API. |
| NET-54 | Air/logistics/shared-production private lists not fully exercised. |
| NET-55 | Team ghosts/mines and authored scenario visibility lack complete coverage. |
| NET-56 | World logs, hidden pending footprints/mines and independent outboxes lack complete coverage. |
| NET-57 | Changed fog XOR and ordinary spectator path not fully exercised. |
| NET-58 | Shots/local events/live flight policy lacks complete coverage. |
| NET-59 | World strike and effect reveal policy lacks complete coverage. |
| NET-63 | Uncached projection and its flag discrepancy have no production API. |
| NET-66 | Attached AI online/-1 ping semantics unavailable. |
| NET-67 | World/shared spectator resync and client throttle unavailable. |
| NET-71 | Autosave/diagnostic scheduler branches unavailable. |
| NET-73 | AI/Horde think scheduling unavailable. |
| NET-75 | Watcher backpressure helper has no production stream caller. |
| NET-76 | Ordinary and World spectator streams unavailable. |
| NET-77 | Spectator snapshots unavailable. |
| NET-78 | Ordinary hold is exercised, but Horde/story result contexts are incomplete. |
| NET-79 | Host pause/resume wire controls and operative input unavailable. |
| NET-80 | AI takeover and loaded-save pause unavailable. |
| NET-82 | Operative close path unavailable; ordinary cleanup checks are partial. |
| NET-85 | In-play pong server timing fields absent. |
| NET-86 | Map ping routing and throttle absent. |
| NET-87 | Native reconnect/backoff transport consumer absent. |
| NET-88 | Persisted room/operative identity and view restoration absent. |
| NET-89 | Manual save unavailable. |
| NET-90 | World autosave unavailable. |
| NET-91 | Native persistence and retention unavailable. |
| NET-92 | Save load and seat restoration unavailable. |
| NET-93 | Save summary listing unavailable. |
| NET-94 | Horde records and shard-owned writes unavailable. |
| NET-95 | Sandbox spawn command unavailable. |
| NET-96 | Shard routing and IPC unavailable. |
| NET-97 | Diagnostics and browser metrics consumer unavailable. |

## Validation

Initial main-based extraction: 17 selected tests, 16 passed and one failed. The failed stale-membership test depended on the old parent's U+FEFF trimming. Its name-cleaning input was separated from the membership witness; NET-20 stays held, and no expected Unicode policy was changed. Raw log: `/tmp/ww2-net-initial-tests.log`.

First expanded run: 27 selected tests, 25 passed and two failed. One witness assumed a reconnect needed an `all` flag; production instead resets its ledger and sends every required row. The other witness failed to consume the replacement Close after `replaced`. The corrected tests check complete delivered rows and explicitly consume both outputs. Review also strengthened end-hold output counts and resumed the drop pause before stale command probes. Raw log: `/tmp/ww2-net-added-tests.log`.

Final focused run: 27 selected tests passed, zero failed and 68 existing tests skipped. Formatting check passed. Strict locked workspace all-target Clippy passed. S2 was released with exit 0. Final log: `/tmp/ww2-net-final-checks.log`. Every Cargo invocation uses an exclusive S1/S2/S3 lock, its designated external target, four build jobs, a 6 GiB scope with no swap, and the 20 GiB free-disk floor. No full workspace test suite, CI wait or merge is part of this task.
