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
