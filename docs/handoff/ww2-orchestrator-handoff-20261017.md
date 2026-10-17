# ww2-rts-native parity orchestration: handoff (2026-10-17)

Written by the Claude orchestrator on the Vostro for the next agent. Everything below is pushed to `origin`; nothing important is only on the Vostro.

## Goal

Resolve all 942 parity rows in `docs/parity/{modes,maps,combat,movement,bases,engineering,logistics,ai,server,fps,interface}.md`. Each row must end `ported`, `accepted`, `differs` or `dropped`. A row is `ported` when a Rust test shows its behavior and a reviewer has checked it against the prototype (`reference/ww2-rts`, pinned at `e4e579a`; never edit it). Done when every row is resolved, main CI is green on Linux, Windows and macOS, and no test or status PR is open.

Three of the 942 entries are fenced historical copies (BASE-18, BASE-49 and BASE-50 duplicates), so there are 939 distinct rows.

## State right now

- **main is frozen** at `61947c5b8a8438eb6ccbefa02e7d269c4e032472`, with **776 rows `ported`**. The per-PR merge queue is stopped. Nothing merges to main until the one-pass integration below lands.
- **Every remaining row has its proof in an open PR.** No row still needs new Rust work. The rest of the job is merging and marking rows.
- Count ported rows on main:
  ```
  for f in modes maps combat movement bases engineering logistics ai server fps interface; do git show origin/main:docs/parity/$f.md | grep -E '^\| [A-Z]+-[0-9]+ \|' | awk -F'|' '{s=$(NF-1); gsub(/^ +| +$/,"",s); print s}'; done | sort | uniq -c
  ```

## Why the switch to one-pass integration

Per-PR CI was the bottleneck. Each PR needs 9 jobs (3 on scarce macOS runners), and every merge made sibling PRs conflict, so each one rebuilt and reran CI. Julio asked to skip the queue and "merge all and fix all in one pass". main has **no branch protection**, so a validated integration branch can be pushed straight to main. Because every PR's exact head commit is merged unchanged, GitHub marks those PRs as merged automatically once their heads are on main.

## Integration branches (pushed)

Each branch starts at the freeze SHA and merges the listed PRs' exact heads with `git merge --no-ff`, with conflicts resolved and test rewrites listed in `changelog.d/2026-10-17-integrate-<chain>.md`.

| Branch | Head | PRs included | Validation |
|---|---|---|---|
| `integrate/ai-20261017` | `bc30b710` | #342 #318 #417 #407 #406 #412 | Done: 1,994 passed, fmt and strict clippy clean |
| `integrate/rooms-20261017` | `57344c0f` | #374 #378 #384 #385 #387 #393 #398 #410 | All merged; last commit is unvalidated post-merge test fixes (WIP) |
| `integrate/logistics-20261017` | `02a122fe` | #404 #408 #390 #405 #414 #399 #413 | All merged; final full test run was interrupted |
| `integrate/save-20261017` | `0e32cb44` | #376 #383 #386 #394 #409 #415 #416 #424 | 8 of 10; **#418 and #434 not yet merged** |

## What to do next

1. **Finish the chains.**
   - `save`: merge the current heads of #418, then #434 (additive C ABI 2.1, then 2.2).
   - `rooms` and `logistics`: run full validation.
   - Validation for each branch: `cargo nextest run -j4 -p ww2-core -p ww2-server -p ww2-simtest`, `cargo fmt --all --check`, and `cargo clippy --workspace --all-targets -- -D warnings`, all from `rust/` (toolchain 1.99 via `rust-toolchain.toml`). Fix failures on the branch.
2. **Final integration.** On a new branch from the freeze SHA, merge the four chain branches. Then merge the exact current heads of the cross-chain PRs, roughly in this order: #422, #431, #436, #420, #423, #441, #426, #432, #430, #411, #419, #442. Each of these stacks on PRs from several chains. Resolve conflicts, run the full validation above, and fix what breaks.
3. **Land it.** Merge `origin/main` once more in case it moved, then push the result to `main`. Check that GitHub marked the included PRs merged. Close any PR whose newer head was not included (comment "content merged via integration at <sha>"), and close #438 (an older train PR that is now superseded).
4. **Main CI.** Watch the Actions run on main for all three OS. Fix platform-specific failures in small follow-up commits or PRs.
5. **Mark rows.** Run status sweeps (see "Status sweeps" below) until every row proven by a merged PR is marked, and merge those docs-only PRs.
6. **Final audit.** List the rows that are still not `ported`/`accepted`/`differs`/`dropped` and resolve each under the decisions below. Done when none remain, main CI is green on 3 OS, and no test or status PR is open (#199 and #201 are not ours: #201 is Axel's Unreal client PR).

## Recorded decisions (issue #5)

- **Owner decisions A to P** (Axel confirmed, message `axel-owner-confirm-a-to-p-20261016`):
  - A: the differs list (CMB-159, MODE-21, MODE-22, IF-05, NET-15, NET-20, IF-10, IF-23, MAP-22, NET-04).
  - B: Unreal-only rows are `accepted` on the Rust side.
  - C: World rows are `dropped` for milestone R1 (World admission stays CLOSED).
  - E/O: projectile flights and events, with prototype fields and a per-field recipient rule.
  - F: host-only pause. G: operatives. H: full logistics via the recipient view. I: versioned JSON save. J: reference spectators. K: hosted Horde. L: room AI only through the recipient view. M: prototype commands on R1. N: prototype wire shapes. P: prototype privacy.
- **`julio-373-merged-decisions-20261016`:**
  - Rows that are proven except for a World clause count as `ported` for R1, with the deferred World clause named in the row's Acceptance check cell.
  - IF-30 is `differs` (whole-match outcome comparison).
  - IF-04, IF-11 and IF-12 are additive C ABI (2.1 and 2.2: existing functions and layouts unchanged; Axel reviews new signatures before Unreal adopts them; the Unreal FFI pin stays at `9e618217` until Axel repins).
- **AI-65 is `differs`** for the same reason as IF-30 (Rust uses independent seeded RNG streams), proven by whole-match outcome tests in #436.
- **`julio-merge-without-ack-20261017` and Axel's `axel-decision-merge-without-ack-20261016`:** runtime PRs merge without Hermes' exact-head ack. Hermes (Axel's coordination agent) does post-merge review and sends findings as follow-up requests on #5.
- **Save format** (`docs/contracts/save-format.md`): ordinary saves stay v1 (optional `elimination`), and authored/scenario saves are v2.

## Status sweeps

A status sweep is a docs-only PR that marks rows proven by merged PRs. Use `docs/handoff/scripts/ww2-status-flip.py`:

```
python3 docs/handoff/scripts/ww2-status-flip.py docs/parity/<file>.md <pr>:<ROW,ROW>
```

It reads the PR body's `ROW-ID: test` lines, sets Status to `ported` and cites the tests. Afterwards:
- Verify every cited name is a real test: `grep -rqE "fn <name>\b" rust/`.
- Remove cited values that are not tests (the script sometimes picks up backticked non-test values).
- Edit only the Status and Acceptance cells, and add a `changelog.d/` fragment.

Rows split across several PRs (for example the operative and spectator clauses of NET-26/46/82) flip only once all those PRs have merged.

## Project rules to keep (from AGENTS.md)

- Every change needs a `changelog.d/` fragment.
- No em dashes anywhere.
- The core stays deterministic: no wall clock, seeded randomness only, sorted keys where order matters.
- Never weaken or delete a test, except one pinning deliberately replaced behavior: rewrite it in the same change and list it in the changelog.
- Never edit `reference/ww2-rts`.
- Never force-push shared branches.
- Measure performance claims (fixed seed, unit count, mean and p95, the machine).

## Scripts

These are in `docs/handoff/scripts/`, and were written for the Vostro (paths may need adjusting):
- `ww2-status-flip.py` flips rows in status PRs.
- `ww2-merge-queue.sh` was the merge queue: it merges green MERGEABLE PRs base-first and pauses on a red main.
- `ww2-merge-when-green.sh` merges one PR once its checks pass (docs-only PRs have no checks).

## Known gotchas

- Many PRs with long CI starve the macOS runners. Cancel runs on conflicting heads.
- `find` on the Vostro is `bfs`: `-newermt` needs an ISO timestamp, not "-30 minutes".
- Delegated Codex agents stop below 20G free disk. Keep at least one cargo target dir per slot and clear incremental caches when disk is low.
