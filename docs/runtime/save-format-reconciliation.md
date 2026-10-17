# Save stack reconciliation, 2026-10-16

Merge order: #373, #376, #383, #386. No rebase or force push is used. The common prototype pin is `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. The [save contract](../contracts/save-format.md#reconciled-save-stack-373-376-383-386) records the format and prototype load references.

## #376: editor maps and records

Original head: `bb82c1ddee783c608d25da715ca7d2a67a41c821`. Incoming #373: `4edfcd5e8b795aa47fde114de7d51d7eb6430078`. `git merge-tree --write-tree` produced conflict-free tree `900fd92b4168f1c323887ebfc4e2844fda36ecde`, retained exactly by merge commit `351664a4`. No merge hunks were hand-edited.

The automatic result still had two elimination contract rows and contradictory v1/v2 policy. Commit `55d88d28` removes the duplicate, adopts #373's optional elimination policy and prototype references, restores `saves.rs` to version 1, and removes its v2-only elimination-key presence check. The editor lifecycle test still compares the original run, a complete saved checkpoint and a legacy body without elimination through victory; its version assertion is now 1 and its obsolete incomplete-v2 case is removed. The core float-bit test is renamed to describe the optional owner rather than a removed v2 transition. The editor contract, design fragment and changelog use the same policy. Existing runtime guards and all elimination continuation assertions remain.

## #383: objectives and announcements

Original head: `4d5642211448de9cc04300a2c51c1502a1da71cb`. Merge against reconciled #376 `55d88d28` produced tree `949847e2e4e53ce9cae9d26f528a90542ad2fe75`, with one conflict in `server/src/save_load_tests.rs`: both branches appended a test after the same final attribute. Merge commit `a5e0ed20` retains both complete functions, each with its own closing brace and `#[test]`: the authored objective save/downgrade/pending-announcement test and the MODE-55 lobby record test. No assertions are removed. Every other merged path is automatic.

The initial unreconciled #376/#383 simulation also conflicted in `docs/contracts/save-format.md` and `server/src/saves.rs`, because one branch globally assigned version 2 to elimination while the other reserved it for authored missions. Settling ordinary elimination on v1 removes those textual conflicts and preserves authored downgrade refusal.

The #383 checkpoint follow-up changes these source hunks: `Objective.scope`; typed `TriggerState` and arrival records; required facts/owners/deferred fields; area and point-owner initialization; time-trigger contextual counters, cooldown, previous predicate and completion facts; capture history updates; canonical checkpoint consistency checks; one Match initialization call; Serde derives for Scope/ConditionState; and one resumed-checkpoint regression. The executor remains time-only, match-scoped and once-only, with deferred arrivals refused. The save and scenario contracts list the shared fields and prototype persistence lines.

Before publication #376 also merges current main `fd714e36430cdfac864e6af00f346067c84d7863`. Its merge-tree tree is `394306ec262bad2d5c3d2b41aef8279e00160447`; merge commit `0640ef9f` retains it exactly, without manual hunks. Its only incoming files are the existing owner-closure status documentation and fragment. #383 already contains those main changes and merges that final #376 head as an ancestry-only update.

The contract-index descriptions are also reconciled: #376 names ordinary v1 with optional elimination; #383 names ordinary v1 and shared authored v2. These replace the stale global v2 writer description inherited from SAVE-2.
