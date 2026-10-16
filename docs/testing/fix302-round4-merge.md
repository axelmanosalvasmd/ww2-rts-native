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

The first test build found one additional semantic merge correction at
`rust/crates/core/src/movement_runtime_tests.rs:167`. Main adds the
`drive_vehicle` argument `limit_travel`; the PR's direct rear-yield test used the
old signature. Pass `false`, matching ordinary `move_unit_with_march` policy.
The reverse movement, signed speed and facing assertions are unchanged. This
was a compiler error, not a textual conflict hunk; no tests ran in that attempt.

The first executable regression attempt passed encoding/privacy and malformed
order guards (2/3) but failed a new fixture assertion that every queued tick
still had a route. Queue consumption runs before movement: on the route's final
tick, Enter remains queued until the following tick. The corrected regression
requires multiple ticks with both the queued Enter and a live path, checks the
endpoint at that boundary, and still requires entry, organic VP finish, every
seat's delivery on every tick and a fault-free ending hold. No existing test
assertion or runtime guard was removed.

A second fixture assumption used the raw move click as the endpoint. Diagnostics
showed tick 45 at `(35, 25)` with admitted `world_goal = (35, 25)`, while the
click was `(31, 25)`. Existing destination settling selects nearby cover beside
the house. The regression now retains the admitted movement goal and requires
exact equality when the path completes, rather than assuming the raw click.
The diagnostic rerun selected one test and failed that same endpoint assertion.

Final focused regression: 286/286 passed (1 slow), 837 skipped. This includes
37 recipient-snapshot tests, 24 Tutorial tests across core and simtest (all twelve
core MODE-59 through MODE-70 runtime checks), and 225 other movement, garrison,
aircraft, rail and tick regressions. The new full-match Enter regression passes. Its deterministic setup uses seed
7301 and a five-VP goal; the second squad captures the neutral point and earns
those VP through ordinary ticks. No finish result or winning score is injected.

Command, run from `rust/`:

```text
cargo nextest run --locked --cargo-profile ci-test -p ww2-core -p ww2-simtest -j4 -E 'test(mov) | test(garrison) | test(match_carriers) | test(recipient_snapshot) | test(tutorial) | test(rail) | test(aviation) | test(aircraft) | test(match_tick)'
Summary [106.102s] 286 tests run: 286 passed (1 slow), 837 skipped
```

The run used shared slot S3 on the Vostro, with MemoryMax=6G, zero swap and four
jobs. S3 was released when Cargo exited. No full workspace tests or prototype
execution ran. Independent final source review accepted the change, with zero
reviewer executions or edits.

Final formatting and lint, from `rust/`, both passed in shared slot S2 with
the same memory/swap caps. S2 was released after each command:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets -j4 -- -D warnings
```
