# FIX-342: preserve the first recipient refusal

Classification: (a), an expectation made stale by a deliberate merged behavior change. PR #351 introduced construction reserves in `3261125c`; #342 incorporated them in `dd3f7de1`. Its changelog already records protecting construction and fortification funds once three combat squads remain.

`commander_construction.rs` chooses the missing motorpool after the starting HQ and barracks and reserves its 200 MP cost. `commander_tactics.rs` passes that reserve to shopping. `commander_shopping.rs` subtracts it before buying once three squads exist. The seed-7 opening plans have 164 MP for seat 2 and 166 MP for seat 1, so their previously affordable purchases disappear. The human motor queue then reaches the same movement orders sooner.

The simtest runner has explicit tick, observation, policy and command loops in `ai_match.rs`. It never calls `ai_schedule::Schedule`, and its implementation is unchanged from the test's introduction at `2d2a40ba`. The scheduler port is outside this failure's execution path.

The Vostro reproduction at `f27dc9c8` failed with the exact hosted result: `AI runner tick 298 recipient pass: Unsupported { unit: 0, capability: ShotsAndFlights }`. Identical temporary diagnostics in a separate worktree established the following controlled comparison. The only behavioral change in the counterfactual was passing `(0.0, 0.0)` as Default/Conquest shopping reserves; all construction, tactics, movement, firing, scheduler and snapshot implementations otherwise stayed at `f27dc9c8`.

| Observation | Current code | Shopping reserves disabled |
| --- | --- | --- |
| Seat 2 opening purchase | None | Conscript, accepted at tick 72 |
| Seat 1 opening purchase | None | Rifle, accepted at tick 84 |
| Seat 2 first move click | 80 | 94 |
| Seat 1 first move click | 94 | 104 |
| First flights, rifle squad 5 toward MG squad 9 | 298 | 309 |
| First recipient refusal | 298 | 310 |
| Original exact tick-310 assertion | Fails | Passes |

Recipient capture runs on even ticks, which explains the counterfactual flight at 309 and refusal at 310. The repair changes only the expected tick to 298 and adds its cause beside the assertion. The runner still returns immediately on the first error, and the assertion still compares its complete error text. No range, skipped error, actor removal or expanded snapshot admission is introduced. AI-65 remains held. Temporary diagnostics and the reserve override are excluded from the repair.

Source hunk list for fresh Hermes review:

- `rust/crates/simtest/src/ai_match.rs`, `seed7_full_run_preserves_the_first_unsupported_recipient_failure`: two explanatory comment lines and the exact expected tick, 310 to 298. This is the only manually edited Rust hunk in FIX-342.
- `rust/crates/core/src/elimination_modes/tests.rs`, `classic_save_load_preserves_elimination_state_and_next_tick`: inherit #373's 50-line checkpoint test through the required hotfix merge. The serde derives were already identical at `f27dc9c8`, so the merge changes no production Rust implementation.

The hotfix merge is `3a14d517339ecfc1ad8748feef4932f59faa5cce`, with parents `f27dc9c8b75f55a764043e126a4e0ce79e1c69b0` and `4edfcd5e8b795aa47fde114de7d51d7eb6430078`. Its tree, `b8c36c4709fea16e926a578947df8f028dc4e2f1`, exactly matches `git merge-tree --write-tree` for those parents. Zero conflicts and zero hand-edited merge hunks. The incoming save-format documentation and changelog are retained verbatim.

Final qualification covers `ai_match`, `ai_schedule`, `ai_seat_view`, Classic economy and the inherited Classic checkpoint test in `ww2-core` and `ww2-simtest`, plus formatting and strict workspace Clippy from `rust/`. The stacked #318 refresh also checks commander, human policy, Match observation and support snapshot tests. Final checks use a shared flock slot, its matching target directory, a 6 GiB memory limit, zero swap and four build jobs.

Merges after #373; needs fresh Hermes ack at the final heads, with this source hunk list and final-head check receipts.
