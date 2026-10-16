# FIX-302 round 4 merge

PR #302 parent: `96608f2659c76099641634c932f51cab16205d42`.
Merged origin/main: `b6d05874078f183fc95421cd25e13762b253c57c`.
This is a merge, with both parents retained and no rebase.

One conflict hunk required resolution:

| Conflict location | Main side | PR #302 side | Resolution |
| --- | --- | --- | --- |
| `rust/crates/core/src/match_tick.rs:633` (conflict start; main parent line 599, PR parent line 633) | Calls `apply_tutorial_garrison(id)?` after aimed abilities, then the boarding comment. | Calls `apply_unit_entry_with_routes(id)?` outside World mode after aimed abilities, then the boarding comment. | Keep both calls. Run Tutorial garrison first, preserving its scripted entry completion. Then run movement house entry outside World mode, preserving ordinary queued entry and routing. Keep the PR boarding comment and the complete boarding block. Resolved calls are at lines 636 and 638. |

No other conflict hunks occurred. Automatic merges retain main's queued Assist,
Cover and Dig snapshot adapters from #338, Tutorial runtime from #287, and the
PR's movement, carrier, aircraft and guard changes. No test or guard was removed.

Queued Enter uses option (a): the existing kind 9 and stored house coordinates.
At pinned prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`,
`shared/sim.js:3382-3386` defines that queued garrison encoding and
`shared/sim.js:5894` packs the waiting row. The existing owner selection at
`shared/sim.js:6025-6027` and the snapshot table in
`docs/parity/protocol-messages.md:204-208` keep these rows private to the owner.
No wire meaning or player command behavior changes.
