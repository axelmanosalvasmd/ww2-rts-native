# NET-79 prerequisite merge record

This record covers the in-progress merge of main into the branch based on PR #423 and PR #378. It records source reconciliation only. The parent task owns formatting, compilation, tests, the NET-79 changes and publication.

- First parent: `22a7f70d6189cad8dd5c7664463b72a57cd7e00b`, the clean merge of PR #378 (`743dd4c5f609bd908d1658fc13d579be6afae959`) into PR #423 (`4453acf5b95fe5303278ef21b80736fa70c276c7`).
- Incoming main: `f02e09e0549ae5e49b40b086d7f301706582ef51`.
- Merge base: `09340ac993ac594d1702229912fd00dac7073b00`.
- Git automatic merge tree, including conflict markers: `55cff8ead2c58e7fba2ae8da6a62aa22e5dd8941`.
- Fifteen paths conflicted. All other incoming paths retain Git's automatic merge result.

## Manual resolutions

Each numbered item identifies one conflict hunk, in source order within its path. The additional edits below are adaptations required by those resolutions.

| Path | Hunk | Resolution and reason |
| --- | --- | --- |
| `DESIGN.md` | 1 | Keep both the medic/house-shelling and FireAt decisions from the first parent and the incoming external AI checkpoint decision. They document separate features and evidence boundaries. |
| `DESIGN.md` | 2 | Keep both the seat-scheduling/Classic economy integration decisions and incoming World fort/borrowed-placement decisions. No earlier evidence is promoted. |
| `docs/contracts/README.md` | 1 | Keep the first parent's confirmed operative/spectator/cached-mask protocol description and add main's R1 ingress contract row. Both contract topics remain visible. |
| `docs/contracts/save-format.md` | 1 | Keep the hosted `ai` envelope representation. Main's detached external codec remains documented separately below, rather than replacing the hosted representation with a human-only claim. |
| `docs/contracts/save-format.md` | 2 | Keep both the hosted planner-memory section and main's IF-27 external codec section. Clarify that the detached codec has its own representation and does not replace the room codec; remove the two now-obsolete statements that all room AI content is refused. |
| `rust/crates/core/src/commander_assault.rs` | 1 | Keep the first parent's static saved-reason alias. It supports the strict fixed-name decoder retained below. |
| `rust/crates/core/src/commander_assault.rs` | 2 | Keep the field-specific saved-reason codec rather than main's general compiled-string codec. Both accept all nine emitted assault reasons; the field-specific codec also rejects unrelated compiled strings. |
| `rust/crates/core/src/commander_tactics.rs` | 1 | Keep the first parent's static saved-counter alias for its fixed-name codec. |
| `rust/crates/core/src/commander_tactics.rs` | 2 | Keep the field-specific counter codec. It retains `at` and `mg` round trips and rejects unrelated compiled strings. |
| `rust/crates/core/src/commander_tactics.rs` | 3 | Retain the Classic economy-aware construction call, node memory, difficulty, random owner and shared rush crew exclusion. Feed both World and ordinary/Classic construction the incoming mirrored view and mirrored eligible crews, so earlier queued orders reserve actors for the same look. |
| `rust/crates/core/src/horde_commander.rs` | 1 | Keep both recipient-delivered digging detection and incoming `same_intent` comparison, including numeric JSON equality and work-progress normalization. Retain serialization of the owner plan and unit. |
| `rust/crates/core/src/human_commander_policy_tests.rs` | 1 | Keep both complete tests: hosted planner checkpoint memory and refused garrison reservation for the current look. Restore a separate `#[test]` attribute between them. |
| `rust/crates/core/src/match_human_observation.rs` | 1 | Treat both Horde and AuthoredMission events as non-shot observations, retaining exhaustive event handling. |
| `rust/crates/core/src/match_tick.rs` | 1 | Retain both Horde and AuthoredMission event variants. |
| `rust/crates/core/src/recipient_snapshot.rs` | 1 | Combine the module description to cover the admitted ordinary, Classic and Horde projections and existing projectile/aviation rules. |
| `rust/crates/core/src/recipient_snapshot.rs` | 2 | Use incoming shared snapshot validation and cache-build accounting. Move the first parent's Horde economy/projectile and operative companion checks into that validator, so cached and uncached paths share them. |
| `rust/crates/core/src/recipient_snapshot.rs` | 3 | Keep incoming shared projection and privacy filtering. Admit the union of existing ordinary, Classic and Horde component contexts, preserving World/geography/Assault exclusions. Move both first-parent flight adapters into the shared projection and retain public crate visibility for `seen_by`. |
| `rust/crates/core/src/recipient_snapshot/shots.rs` | 1 | Keep both Horde and AuthoredMission shot references. |
| `rust/crates/core/src/recipient_snapshot/shots.rs` | 2 | Keep conversion for Horde, AuthoredMission and the already supported Aviation producer. Main's blanket Aviation refusal would undo the first parent's delivered aviation lifecycle. Existing aviation validation still runs in projection. |
| `rust/crates/core/src/recipient_snapshot/shots.rs` | 3 | Keep Horde mode/finite-position checks and AuthoredMission definition/tick checks as separate match arms. |
| `rust/crates/server/src/commander_ingress.rs` | 1 | Keep both complete tests: FireAt/compound public refusal and live garrison Move/attack-move exit with recipient snapshots. Restore their separate scopes and `#[test]` attributes. |
| `rust/crates/server/src/native_startup.rs` | 1 | Validate the bound authored mission definition, then retain the existing Horde-specific versus ordinary StartPlan selection. Neither binding check is bypassed. |
| `rust/crates/simtest/tests/common/companion_tick.rs` | 1 | Keep serialization of both Horde and AuthoredMission events. |
| `rust/crates/simtest/tests/match_tick_events.rs` | 1 | Keep serialization of both Horde and AuthoredMission events. |
| `rust/crates/simtest/tests/match_tick_events.rs` | 2 | Keep the original four-family fixture boundary explicit for Scenario, Horde and AuthoredMission output. |

Additional adaptations within the conflicted paths:

- Derive serde serialization/deserialization for `DeliveredPlan` and `DeliveredIntent`, since main now saves the owning Horde plan. Default an absent `OwnPlan.delivered` to `None`, so earlier non-null AI checkpoints remain readable.
- Remove the old cached-only Horde flight insertion after moving it to the shared projection. This projects each recipient's flights once and also serves the uncached path.

## Test preservation and verification boundary

A source comparison retained both parents' function names across the twelve conflicted Rust paths except `base_commands_remain_outside_public_r1_ingress`. Incoming main deliberately replaces that obsolete blanket refusal with `base_commands_reserve_canonical_input_before_mutating_state` and `conquest_production_cancel_is_refused_without_reserving_input`. Both replacements remain. FireAt and compound orders still have their separate refusal test.

Main also deliberately removes `client_pause_and_resume_remain_unavailable_even_for_native_host` from the automatically merged `seat_lifecycle_tests.rs`. Its R1 ingress changelog records the replacement, and `r1_host_pause_freezes_commands_and_ticks_and_only_host_resumes_or_ends` retains real-socket host behavior and guest refusal. Restoring either old blanket-refusal test would contradict incoming main's admitted behavior.

The integration registry `rust/crates/core/tests/it/main.rs` and the core library retain both parent module-name sets. No registration was manually removed. Existing first-parent replacements for stale aircraft, support and Horde-hosting refusal tests remain intact rather than resurrecting old main restrictions.

The staged tree differs from Git's automatic merge tree only in the fifteen conflicted paths and this merge record (sixteen paths total). `git diff --check` passes. No Cargo, compiler, formatter, native test, push or GitHub write ran during this reconciliation. The merge is not runtime-qualified by this source audit. The parent task must run its authorized checks on the resulting head.

## Coordination

Issue #5 was read during reconciliation. Owner decision `axel-decision-merge-without-ack-20261016` (comment `6227193135`) acknowledges the updated merge rules: exact-head checks, mergeability, base-first stacks, main-CI pause and retained local reviews. Hermes now reviews after merge. World admission stays closed, and Unreal adoption remains owner-controlled. No coordination message was posted from this source-only subtask. PR #378 and PR #423 currently expose no comments or reviews through the read-only API query.

## Repairs exposed by compilation

The first focused build on S1 found two `UnitFacts` initializers in `ai_view/commander.rs` that lacked main's new `hold_fire`, `auto_cast` and `cooldown` fields. This file had merged automatically. Both recipient observation providers now populate those facts from delivered owner `holdFire`, `auto` and `cd` values, leaving foreign facts false or zero. Their existing privacy tests also check the new mappings. The source decoder and prototype `shared/ai-view.js:9-21` already define these fields; no authoritative-state read or protocol field is added.

The required formatter also reformatted the reconciled branches in `recipient_snapshot.rs` and `match_tick_events.rs` without changing their behavior. These changes follow the merge commit and are covered by the NET-79 changelog fragment.

The resumed clean rebuild found main's `if27_horde_pending_memory_round_trips_order_budget_and_supersession` fixture missing the optional `OwnPlan.delivered` field from the hosted AI stack. It now sets that field to `None`, matching the detached constructor and preserving the existing checkpoint and supersession assertions. Shared slot artifacts had initially omitted the NET-79 and decoder tests, so that earlier cached run is excluded from qualification. The affected package artifacts are rebuilt before final validation.

The next compiler pass required checked conversion from the simulation's signed tick to the command queue's unsigned key in NET-79. It also found the inherited operative replay test missing `Output::Watcher`; that returning commander now explicitly rejects watcher frames, matching the sibling commander drains. No existing replay assertion is removed.

The rebuilt recipient tests found `net63_hidden_invalid_units_have_the_same_cached_and_uncached_refusal` returning unit zero after the shared companion guard replaced main's unit-row refusal. On validation failure, the shared guard now identifies the first invalid live operative in deterministic control order, preserving the offending unit diagnostic. Failures without an identifiable live operative retain unit zero, and both snapshot paths still reject the state before projection. The existing test remains unchanged.

## Resume merge of current main c391c2e8

# NET-79 current-main core conflict resolution

Scope: the seven assigned core paths only. This is source reconciliation. The parent task owns staging, committing, formatting, compilation, native tests and publication.

- First parent: `61766704b40ddbaeeffa1b558d7c87afc202d382`.
- Incoming main: `c391c2e8d63fb9d4c2d66593329dfa404fad267f`.
- Merge base: `f02e09e0549ae5e49b40b086d7f301706582ef51`.
- Git automatic conflict tree (`AUTO_MERGE`): `df7f9d295986ac57d20930f23ead17b54d18c871`.
- Fifteen conflict hunks across seven core paths. Hunk numbers follow source order within each file. The original marked hunks and pre-resolution line numbers are also saved in `/tmp/net79-core-conflict-hunks.json`.

## Every manual conflict hunk

| Path under `rust/crates/core/src/` | Hunk | Resolution and reason |
| --- | --- | --- |
| `commander_tactics.rs` | 1 | Retain the first parent's blank line after medic rule lookup. This conflict changes whitespace only; the surrounding merged planner retains both parents' behavior. |
| `match_firing.rs` | 1 | Retain the first parent's `flight_contact_filtered` simulation call with explicit fractions and an all-visible predicate. It has main's ordinary simulation contact semantics. |
| `match_firing.rs` | 2 | Retain the first parent's crate-private `recipient_flight_contact` closure API and shared `flight_contact_filtered` helper. Main's seat-specific `visible_flight_contact` remains the wrapper above it. The operative-stack spectator still calls the closure API with all bodies visible, so replacing it with main's private seat-only helper would break that caller. |
| `match_firing.rs` | 3 | Retain optional fractions plus the visibility predicate. Recipient queries use current poses with `(1, 1)`; simulation queries interpolate from the prior map. This preserves main's separation of recipient and simulation poses while retaining spectator composition. |
| `match_firing.rs` | 4 | Retain the caller-supplied visibility filter. The seat wrapper supplies `seen_by`, which matches main; the spectator supplies all-visible. |
| `match_firing.rs` | 5 | Retain prior-pose lookup only when explicit simulation fractions are present. Recipient and spectator queries use current poses, matching main's recipient branch. |
| `match_tick.rs` | 1 | Keep the first parent's Horde economy validation, then main's World logistics refusal, ordinary options/convoy consistency check and supply validation. Main's automatically merged reserve, network, convoy and retirement phases remain intact. |
| `recipient_snapshot.rs` | 1 | Keep the first parent's ordinary/Classic/Horde scope and main's statement that unsupported actors remain explicit and no transport contract is added. |
| `recipient_snapshot.rs` | 2 | Keep main's logistics module registration. |
| `recipient_snapshot.rs` | 3 | Keep the first parent's Horde projectile validation and shared companion validation. On companion failure, retain the first invalid live operative identified in deterministic control order through `check_companion`. This includes main's missing-soldier case and preserves the existing NET-63 offending-unit diagnostic for broader invalid operative states. Failures with no identifiable live operative retain unit zero. |
| `recipient_snapshot.rs` | 4 | Keep Horde's dedicated projectile projection, including its empty-flight behavior. For non-Horde contexts, use main's shared `flights::project`, which now covers ordinary and operative flights together. Then append main's seat-private logistics projection. This removes the obsolete branch calling the operative-only projector that main already deleted. |
| `recipient_snapshot/composition.rs` | 1 | Retain the first parent's empty-flight fallback. Horde's shared projection intentionally omits `flights` when none exist; complete frames must still contain the existing empty list. The fallback leaves actual flight rows intact. |
| `recipient_snapshot/operative_flights.rs` | 1 | Use main's generic flight validation and conditional operative restrictions, adding the first parent's record-to-shooter recognition to the operative predicate. A shooter mentioned by a companion record still requires valid operative control, owner and combat backing. Ordinary flights receive generic validation without being forced into operative grenade shape. |
| `recipient_snapshot/shots.rs` | 1 | Retain conversions for Horde, Aviation and AuthoredMission. All three event producers remain represented. |
| `recipient_snapshot/shots.rs` | 2 | Add main's operative shooter validation for `Throw` events, preserving the existing owner, identity and coordinate validation. |

## Automatic merge adaptations retained and source audit

No source edits were made outside those fifteen conflict spans. The following incoming automatic changes are intentionally retained and explain differences from the first parent:

- `operative_flights.rs` no longer contains the separate `project` function or its geometry imports. Main's `flights::project` now validates and projects mixed ordinary and operative flights. The resolved common projection calls that owner directly for non-Horde matches.
- `composition.rs` no longer blanket-refuses logistics. The new logistics module projects the existing owner-private rows.
- `recipient_snapshot.rs` no longer lists Logistics as missing after the ordinary projection owner has been integrated.
- `match_tick.rs` retains main's connected unit reserves, withdrawal, forced-movement state, supply-network beat, convoy tick and convoy-death recording. Horde event scheduling and operative lifecycle hooks remain from the first parent.

Compared function-name sets against both parent blobs in every assigned file. Every parent function remains except the equivalent main-only private `flight_contact_for` helper and the superseded first-parent operative-only `project` function. No test function was removed. No registry file was edited. The ordinary mixed-flight regression `mixed_ordinary_and_operative_grenades_keep_each_recipient_live_and_refuse_corruption`, unbacked operative row test and NET-63 exact offending-unit test remain in their automatically merged files.

Read current issue #5 replies and both prerequisite PR #423/#378 comments and reviews through read-only GitHub calls. Both PR queries returned empty comment/review arrays. The latest issue reply is Axel's `axel-native-slice1x-20261017` result report, not an ack request. Current merge rules retain World closure and Unreal's existing pin. No public interface or message was changed or posted by this subtask.

`git diff --check` passes for the seven owned paths, and none contains conflict markers. All seven differ from `AUTO_MERGE` because the conflict spans are resolved. The index still records them as unmerged until the parent stages the files. No Cargo, compiler, formatter, native tests, fetch, stage, commit, push or GitHub write ran in this subtask.

## Server control-flow resolutions for main c391c2e8

- `commander_ingress.rs` hunk 1: retain main's removal of `fire_at_and_compound_orders_remain_blocked_at_public_ingress`. Main now admits FireAt, and its separate `compound_orders_remain_blocked_at_public_ingress` preserves compound refusal; `room_command_ingress_tests` and `room_fireat_tests` exercise admitted FireAt. No replacement assertion is removed.
- `lobby.rs` hunk 1: retain the hosted Horde roster limit of five defenders, ordinary limit two.
- `lobby.rs` hunk 2: retain Horde command dispatch before save/trusted handling, so `nextwave` keeps its existing host path.
- `lobby.rs` hunk 3: retain room AI test registration alongside operative tests.
- `lobby.rs` hunk 4: retain Horde host, room AI combat and combined room AI/NET-79 registrations, then append main's room-command ingress registration. Each test module retains its `cfg(test)` attribute.
- `lobby_saves.rs` hunk 1: retain the prepared connected spectator set instead of replacing it with an empty vector. Main's operative commander remapping is retained in the automatic merge, preserving both loaded spectators and operative membership.
- `main.rs` hunk 1: combine host/Horde authored-map help with main's MAX_ROOMS, static-root and admitted logistics descriptions.
- `main.rs` hunk 2: retain authored-map config and main's explicit static-root/capacity config. Remove the now-unneeded Config default update since every field is supplied.
- `room_runtime.rs` hunk 1: retain main's latency telemetry method used by the merged pong path.

The incoming main changelog already records FireAt admission and the narrower compound refusal replacement. This merge does not claim either as a new NET-79 row.

# Operative test merge resolution

Baseline: first parent `61766704b40ddbaeeffa1b558d7c87afc202d382`, incoming main `c391c2e8`.

Only `rust/crates/server/src/operative_room_tests.rs` and `rust/crates/server/src/operative_server_tests.rs` were written. Both resolved files exactly match their first-parent stage-2 contents. No staging, commits, Cargo, formatter, fetch or GitHub writes ran.

## operative_room_tests.rs

Four conflict hunks, in source order:

1. Keep the first-parent `assert_lobbies` helper and complete tests for NET-25 live takeover, NET-28/29 broadcasts, NET-82 offline cleanup, unsaved operative full lifecycle, and the renamed/strengthened FPS-39 loaded lifecycle. Incoming main contributes the old narrow loaded-test name `operative_load_merge_retains_membership_and_remaps_the_commander_person`; its behavior is retained by `fps39_loaded_operatives_keep_identity_through_finish_start_and_restart`, not registered again as a duplicate fixture.
2. Keep the first-parent hold-fire/autocast/auto-retreat setup in the loaded fixture. It prevents combat from disrupting the deterministic natural-victory continuation.
3. Keep the first-parent post-load VP arrangement and observer initialization rather than main's short nine-callback loop opening. Both parents' failed-load atomicity, commander remapping, retained soldier identity, reset sequence, role start shape, operative resume refusal and host resume assertions are identical before this continuation.
4. Keep the complete first-parent natural victory, all 120 ending callbacks, operative lobby retention, new-match and restart continuation. It collects both canonical seat snapshots and both independent operative views at every advancing tick, retains no-fault assertions and checks contiguous ticks. Incoming main's nine-callback recipient loop and terminal tick-at-least-eight check are superseded by the longer continuation and its terminal tick bound of at least finish tick plus 59. No loaded admission or recipient behavior is dropped.

The shared `assert_every_recipient_tick` helper is identical in both parents. Five first-parent tests remain, with no duplicate helpers.

Preserved tests:
- `net25_operative_live_takeover_preserves_person_and_ignores_old_socket_close`
- `net28_net29_operative_broadcasts_keep_change_order_and_shifted_commander_identity`
- `net82_operative_offline_identity_survives_lobby_cleanup_deadline`
- `operative_room_members_deliver_every_tick_through_natural_finish_lobby_and_restart`
- `fps39_loaded_operatives_keep_identity_through_finish_start_and_restart`

## operative_server_tests.rs

One marker-delimited conflict hunk:

1. Retain first-parent module registrations for `lifecycle`, `room_protocol`, `supplemental` and `timing`. Incoming main has only `room_protocol`; retain it once, without dropping any sibling test modules.

Additional automatic-merge adaptations:

- Remove the second equivalent `Output::Watcher` match arm inserted by the automatic merge into `messages`.
- Remove the second equivalent `Output::Watcher` match arm inserted by the automatic merge into `drain_commanders`.
- Both helpers continue to panic on watcher frames, preserving each parent's role refusal. No output is ignored.

All nine parent test names remain. The first parent's broader operative whitelist assertions, command matrix and canonical gameplay/denial-state equality checks remain; incoming main has a strict subset of these checks.

Preserved tests:
- `operative_join_checks_each_requirement_and_keeps_four_nonseat_members`
- `operative_lobby_tracks_commander_identity_across_nonfinal_seat_removal`
- `operative_presence_prevents_room_expiry_and_never_grants_host`
- `operative_permission_whitelist_is_silent_except_name_and_latency_ping`
- `operative_input_has_independent_twenty_millisecond_boundaries_and_pause_gate`
- `operative_running_join_moves_and_delivers_through_pause_victory_and_next_match`
- `operative_reconnect_keeps_soldier_resets_sequence_and_resyncs_only_its_stream`
- `operative_trusted_room_admission_preserves_refusal_and_live_token_takeover`
- `operative_truncated_utf16_tokens_keep_distinct_soldiers_and_input`

## Verification boundary

Source comparisons verify both resolved files are byte-identical to the first-parent files, contain no conflict markers, and retain the intended module registrations. This reconciliation is source-only. Parent owns final compilation, formatting, tests and merge completion.

## Rebuilt merged-head failures

The rebuilt `88a65bbd` selection ran 259 tests: 257 passed and two failed. The NET-79 full-checkpoint difference contained only lazy `routing_fog` cache refresh (retained pose, team visibility and cache key/version). The witness now collects a current frame for every canonical seat before pause and checkpoint capture, retaining its full equality assertion on every paused callback. Current frames do not refresh initialized AI observations.

`eng12_unrepresented_modes_and_logistics_cannot_publish_paid_or_waiting_gates` exposed a separate guard integration issue. The fixture changes mode tags to Horde without constructing canonical Horde ownership. Shared ordinary actor lookup now recognizes its roster shape. The gate-specific admission predicate now requires `validate_horde_economy` for Horde, so paid and queued gate creation still reject malformed state. Canonical Horde gate support and ordinary logistics refusal remain unchanged; the existing test and assertions remain intact.
