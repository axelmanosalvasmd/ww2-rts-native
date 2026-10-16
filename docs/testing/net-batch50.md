# First remaining NET behavior batch

This batch owns the first 50 candidates of the 82 remaining NET rows in
Root's `NET_IMPL.json`, based on main `66a3c3c0`. The author checkout stays
on its preserved `41f1d343` source plus this private test-only child. Root
combines publication for `docs/parity/server.md`, with at most 50 rows and
one open PR for that file. This document changes no row status or check text.

Axel approved rules 1 to 5 without added conditions in
[6206861267](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267).
Behavior qualification uses native tests and a reviewer reading the prototype
for intent. Hermes's notes in 6206844573 were not adopted. Exact comparisons
remain for tables, pure functions and single-component fixtures. This batch
adds no Source recorder, captured fixture or provenance bundle. All prototype
references below are at the existing `e4e579a` pin.

At producing `0f537aeb`, one granted native batch ran all 17 checks:
14 passed, three failed and 76 other tests were skipped. The privacy and
wallet checks stopped on JSON float-versus-integer assertions; the terrain
check wrongly expected a full read to drain the delta outbox. Original raw
outputs stay at `/tmp/native-feature-batches-0345/net_impl/`. Process and
inherited S2 lock release were reported to Root before these results.

The successor compares numeric values without changing expected numbers,
and checks that the first delta drains the retained baseline before requiring
the next unchanged delta to be empty. These three corrected checks are UNRUN;
all other passes retain attribution to `0f537aeb`, not this child. NET-53 drops
from that producing head's potential candidates; all its dependent coverage
stays held. A new literal Root grant is required for successor execution.

A candidate becomes eligible only after its mapped native checks pass and the
required prototype review confirms coverage.
Partial checks never count as complete rows. Failed or incomplete rows stay
in progress with their reasons while the other rows continue. No runtime,
wire, FFI, reference, provider, privacy, role or default policy changes here.

The literal batch selection is `package(ww2-server) & test(net_batch50)`.
One Root-granted focused nextest invocation runs the whole selection with
`--no-fail-fast`, four build jobs and one test thread. This selects only the
new modules below. It does not rerun the historical 13 or discover tests in a
separate job. There are 17 native checks. The original one-job result stays
at its producing head; this correction has no execution. No fmt, lint, Source
or CI command ran in either preparation. A fresh literal head/tree/selection/slot
grant is required.
The proposed command, inside Root's assigned scope, is:

```sh
python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py +1.99.0 nextest run --manifest-path rust/Cargo.toml --locked --cargo-profile ci-test -p ww2-server --lib -j4 --test-threads 1 --no-fail-fast --retries 0 -E 'package(ww2-server) & test(net_batch50)'
```

All shell and subprocess working directories remain
`/tmp/ww2-net-first-five-b326`. Root grants the exact frozen head, Rust tree,
selection and slot before this command can run. Every job needs actual assigned
four-CPU/6GiB/swap0/400% limits, jobs4/threads1, current guard, inherited
exclusive flock, assigned external target, free-g and df with at least20GiB,
and the global maximum of three concurrent jobs. No retry is permitted.
The native batch continues other tests after individual test failures. When
it ends, notify Root of terminal state and inherited-lock release before
reporting row results. A compile or infrastructure failure prevents execution
and is reported as such; it is never counted as row passes.

Test names in the table use these exact prefixes:

- `L`: `lobby::net_batch50::`
- `S`: `lobby::lobby_socket_tests::net_batch50_socket::`
- `H`: `net_batch50_http::`

| Row | Literal native test mapping | Coverage and individual eligibility | Prototype reference |
| --- | --- | --- | --- |
| NET-01 | H`port_zero_directory_and_public_map_requests_use_the_real_router` | Partial: real ephemeral bind/HTTP works; `/play` and browser page are absent. Keep held. | server.js:29,812-815 |
| NET-04 | H`port_zero_directory_and_public_map_requests_use_the_real_router` | Not covered: no permessage-deflate negotiation/threshold implementation. Keep held. | server.js:577-578 |
| NET-05 | S`malformed_frames_before_and_after_hello_leave_the_room_usable` | Candidate: malformed JSON, primitives, arrays and non-string `t` stay silent; valid hello, rename and another player's control still work. | server.js:584-589; test-audit-server.mjs:59-62 |
| NET-06 | H`port_zero_directory_and_public_map_requests_use_the_real_router` | Boundary only: browser module trees are not served. Keep held. | server.js:130-132,163-187 |
| NET-07 | H`port_zero_directory_and_public_map_requests_use_the_real_router` | Not covered: no static-file ETag, 304 or gzip cache. Keep held. | server.js:172-186 |
| NET-08 | H`port_zero_directory_and_public_map_requests_use_the_real_router` | Boundary only: `/`, `/play` and `/?edit` remain 404. Keep held. | server.js:163-164 |
| NET-09 | L`creation_directory_capacity_and_invite_preserve_public_boundaries`; L`directory_marks_playing_rooms_unjoinable_without_publishing_personal_fields`; H`port_zero_directory_and_public_map_requests_use_the_real_router` | Candidate: listed connected humans, exact public field set, World fields, AI counts, hidden rooms, play state/unjoinable, no token/name leak and HTTP no-store. | server.js:137-145 |
| NET-11 | H`port_zero_directory_and_public_map_requests_use_the_real_router` | Partial: public map, hidden mines/scripts, name/method errors; correct-password raw reads are unavailable. Keep held. | server.js:148-162; shared/world-layers.js:68-80 |
| NET-12 | H`port_zero_directory_and_public_map_requests_use_the_real_router` | Boundary only: POST returns 405; editor validation/writes/shard refresh are not implemented. Keep held. | server.js:95-129 |
| NET-13 | H`port_zero_directory_and_public_map_requests_use_the_real_router` | Boundary only: every editor header is rejected; password creation and constant-time comparison are absent. Keep held. | server.js:89-94 |
| NET-14 | L`creation_directory_capacity_and_invite_preserve_public_boundaries` | Candidate: explicit configured invite URL reaches the lobby. Existing main reads PUBLIC_URL; optional Tailscale lookup is absent and no client-rendering result is claimed. | server.js:41-45,345; client/main.js:98,365-367 |
| NET-15 | L`creation_directory_capacity_and_invite_preserve_public_boundaries`; L`listing_defaults_sanitizing_and_creation_only_fields` | Candidate for native creation intent: defaults, public flag, title filtering/fallback/limit and creation-only immutability. Original split-surrogate failure remains historical; no exact UTF16 API is adopted. Reviewer must judge this bounded native display behavior before promotion. | server.js:590-595 |
| NET-16 | L`creation_directory_capacity_and_invite_preserve_public_boundaries` | Partial: configured cap rejects only new rooms and preserves existing-room access. Binary does not read MAX_ROOMS. Keep held. | server.js:31,591 |
| NET-18 | L`unimplemented_roles_and_public_lifecycle_remain_explicit_refusals` | Not covered: Tutorial auto-start is unavailable. Keep held. | server.js:593-594,616 |
| NET-20 | L`hello_rename_and_stale_membership_use_current_people`; S`malformed_frames_before_and_after_hello_leave_the_room_usable` | Partial: handshake types, pre-hello silence, token limit, sanitizing/fallback work. Full split-surrogate display behavior remains unresolved. Keep held. | server.js:194,588-589,595 |
| NET-21 | L`watch_and_sit_obey_capacity_and_assign_fresh_seat_defaults` | Partial: lobby spectator/seat limits work; active-match spectator admission is unavailable. Keep held. | server.js:204,609-611 |
| NET-25 | L`hello_rename_and_stale_membership_use_current_people`; L`attached_match_blocks_lobby_mutations_and_preserves_command_window_on_rejoin` | Partial: membership replacement, socket retirement, state and budget survive. Operative rejoin and live AI/commander reset remain incomplete. Keep held. | server.js:596-603 |
| NET-26 | L`trusted_lifecycle_publishes_start_before_snapshots_and_rejoins_same_match` | Partial: seated reconnect start, old pause, resume and lobby ordering work; spectator/operative starts unavailable. Keep held. | server.js:612-615 |
| NET-27 | L`hello_rename_and_stale_membership_use_current_people` | Candidate: replaced connection cannot rename/kick or disconnect its replacement; removed membership cannot rename or sit. | server.js:620 |
| NET-28 | L`hello_rename_and_stale_membership_use_current_people`; L`host_settings_validate_all_choices_and_serialize_replacement_requests` | Partial: synchronous Hub serializes changes and broadcasts to seats; save listings and operative roster updates absent. Keep held. | server.js:338-351; server/saves.js:39-49 |
| NET-29 | L`hello_rename_and_stale_membership_use_current_people`; S`malformed_frames_before_and_after_hello_leave_the_room_usable` | Partial: ordinary rename/broadcast and malformed silence; operative rename path absent. Keep held. | server.js:635,638 |
| NET-31 | L`host_settings_validate_all_choices_and_serialize_replacement_requests`; L`attached_match_blocks_lobby_mutations_and_preserves_command_window_on_rejoin`; L`hello_rename_and_stale_membership_use_current_people` | Candidate: choices/type guards, host authority, synchronous request ordering, stale identity refusal and play guards. Every catalogue map is checked against the Horde defender restriction; synchronous native selection avoids pending asynchronous requests. | server.js:649-681 |
| NET-32 | L`ai_level_kick_team_and_faction_enforce_authority_and_integer_types`; L`attached_match_blocks_lobby_mutations_and_preserves_command_window_on_rejoin` | Candidate: all difficulty values, AI slot, host-only and play guards. | server.js:642-643 |
| NET-33 | L`ai_level_kick_team_and_faction_enforce_authority_and_integer_types`; L`attached_match_blocks_lobby_mutations_and_preserves_command_window_on_rejoin` | Candidate: host/lobby, AI/offline removal, connected-human preservation and strict slot types. | server.js:644-646 |
| NET-34 | L`ai_level_kick_team_and_faction_enforce_authority_and_integer_types`; L`attached_match_blocks_lobby_mutations_and_preserves_command_window_on_rejoin` | Candidate: team/faction ranges, own faction versus host permissions, malformed integer rows and play refusal. | server.js:655-658 |
| NET-35 | L`watch_and_sit_obey_capacity_and_assign_fresh_seat_defaults`; L`attached_match_blocks_lobby_mutations_and_preserves_command_window_on_rejoin` | Candidate: watch limit, seat limit, vacated seat, reset team/faction and play refusal. The real default-map public limit blocks sit until a connected seat switches to watching. | server.js:704-707 |
| NET-36 | L`trusted_lifecycle_publishes_start_before_snapshots_and_rejoins_same_match`; L`unimplemented_roles_and_public_lifecycle_remain_explicit_refusals` | Partial: bounded trusted host start works; public start and Assault readiness are unavailable. Keep held. | server.js:331-332,682-683 |
| NET-37 | L`unimplemented_roles_and_public_lifecycle_remain_explicit_refusals` | Not covered: asynchronous load cancellation and prior-game rollback are not implemented. Keep held. | server.js:358-390 |
| NET-38 | L`unimplemented_roles_and_public_lifecycle_remain_explicit_refusals` | Not covered: public World runtime start is unavailable. Keep held. | server.js:364-365 |
| NET-39 | L`trusted_lifecycle_publishes_start_before_snapshots_and_rejoins_same_match` | Partial: approved R1 constructor only; general mode/defender/logistics option mapping is incomplete. Keep held. | server.js:353-356,380-381 |
| NET-40 | L`trusted_lifecycle_publishes_start_before_snapshots_and_rejoins_same_match` | Partial: trusted identity/reset/publication barrier; AI, Horde and other recipient roles remain incomplete. Keep held. | server.js:392-413,781 |
| NET-41 | L`trusted_lifecycle_publishes_start_before_snapshots_and_rejoins_same_match` | Partial: healthy trusted host restart resets identity/tick/pause; broader mode support and load cancellation remain held. | server.js:688-689 |
| NET-42 | L`trusted_lifecycle_publishes_start_before_snapshots_and_rejoins_same_match`; L`terrain_and_fog_start_baselines_do_not_repeat_in_unchanged_snapshots` | Partial: ordinary setup decodes through existing ClientView with recipient, map, names, teams, factions, terrain/fog. World known-map/spawn privacy is not exposed by server. Keep held. | server.js:454-468 |
| NET-43 | L`attached_match_blocks_lobby_mutations_and_preserves_command_window_on_rejoin` | Partial: actual supported ingress/parser denial; full command family and successful live buy feedback are incomplete. Keep held. | server.js:712-715 |
| NET-44 | L`command_preamble_blocks_finished_out_away_and_whole_malformed_orders` | Candidate: winner, invalid slot, out/away, non-object and whole malformed order rejection, with successful control command. | shared/sim.js:3408-3410 |
| NET-45 | L`attached_match_blocks_lobby_mutations_and_preserves_command_window_on_rejoin` | Candidate: four-per-second denial window, exact boundary and preservation through actual replacement. Eight successful pings between denials and rejoin leave the budget unchanged. | shared/command-feedback.js:1-8; server.js:714 |
| NET-46 | L`unimplemented_roles_and_public_lifecycle_remain_explicit_refusals` | Not covered: operative joins unavailable; active spectators unavailable and fallback unavailable-command handling is not equivalent to silence. Keep held. | server.js:630-636,712 |
| NET-47 | L`away_seat_stops_ordinary_wallet_income_and_cannot_command` | Partial: actual tick stops MP/mun/VP with a connected income control; canonical logistics pause not covered. Keep held. | server.js:784; shared/sim.js:3409,5733-5752; shared/supply-convoys.js:109,365 |
| NET-48 | L`unimplemented_roles_and_public_lifecycle_remain_explicit_refusals` | Boundary only: leave is denied; persistent AI takeover is absent. Keep held. | server.js:521-531,708-711 |
| NET-49 | L`unimplemented_roles_and_public_lifecycle_remain_explicit_refusals` | Boundary only: handAi is denied; offline army handover absent. Keep held. | server.js:521-531,647-648 |
| NET-50 | L`unimplemented_roles_and_public_lifecycle_remain_explicit_refusals` | Boundary only: nextwave is denied; Horde network runtime absent. Keep held. | server.js:698-699 |
| NET-51 | L`unimplemented_roles_and_public_lifecycle_remain_explicit_refusals` | Boundary only: manual end is denied; this does not qualify natural match completion as manual end. Keep held. | server.js:561-566,696-697 |
| NET-52 | L`recipient_rows_hide_other_owners_cooldowns_stances_and_unseen_target_ids` | Partial: actual own/visible enemy/hidden enemy/reveal rows; live plane projection remains unsupported. Keep held. | shared/sim.js:5941-5945,6042 |
| NET-53 | L`recipient_rows_hide_other_owners_cooldowns_stances_and_unseen_target_ids` | Candidate for supported ordinary rows: owner cooldown/private stances, non-owner mask and hidden/visible target identity. Gate/operative/cargo rows are not enabled by this test; broader provider support needs reviewer assessment. | shared/sim.js:5899-5905 |
| NET-54 | L`recipient_wallets_support_and_unit_lists_stay_with_the_owning_seat` | Partial: unequal private MP/income/support, correct ordinary mun/fuel omission, positive owner plans and other owner-ID lists; active air/logistics/all queue states not covered. Keep held. | shared/sim.js:6016-6028,6054,6062 |
| NET-55 | L`recipient_wallets_support_and_unit_lists_stay_with_the_owning_seat` | Not covered by that owner-list check: team covers/works/ghosts/mines/scenario/say need their own complete feature matrix. Keep held. | shared/sim.js:6036-6037,6055-6061,6071-6072 |
| NET-56 | L`terrain_and_fog_start_baselines_do_not_repeat_in_unchanged_snapshots` | Partial: ordinary complete baseline and unchanged outbox; hidden changes/mines and World memory not exercised. Keep held. | shared/sim.js:2650-2663,5837-5875,1430-1443 |
| NET-57 | L`terrain_and_fog_start_baselines_do_not_repeat_in_unchanged_snapshots` | Partial: baseline object and unchanged-fog omission; changed-mask XOR and non-World spectator path need complete coverage. Keep held. | shared/sim.js:4646-4690 |
| NET-58 | L`recipient_rows_hide_other_owners_cooldowns_stances_and_unseen_target_ids` | Not covered by unit-row filtering: regular shots, projectile/flight extrapolation and paired owners require separate event checks. Keep held. | shared/sim.js:5965-6009,6079-6088 |
| NET-59 | L`recipient_rows_hide_other_owners_cooldowns_stances_and_unseen_target_ids` | Not covered by unit-row filtering: smokes/fires/falling and World strike policy require separate effects checks. Keep held. | shared/sim.js:6045,6050-6052,6075 |

Historical 186's thirteen passes and the original 8348 address-witness failure
remain unchanged. This suite does not replay those tests or replace their
results. Test-only authored visibility lists exercise recipient filtering;
they are inputs, not a claim that the vision provider was executed. No
unsupported role is silently projected as a commander. No intentional
runtime difference or player-visible rule change is made in this batch.

The remaining 32 assigned rows stay queued after this first window:
NET-60,61,62,63,66,67,68,71,73,75,76,77,78,79,80,81,82,83,84,85,86,87,
88,89,90,91,92,93,94,95,96,97. This source preparation does not start a second
batch or infer qualification for them.
