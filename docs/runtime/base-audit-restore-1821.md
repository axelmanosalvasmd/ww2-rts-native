# BASE_AUDIT recovery checkpoint

Frozen delivery base: `379ba4e06c794708623c005ab6052bebd03d8809`. Own checkout: `/home/judiazm/.t3/worktrees/ww2-rts-native/t3-9add0087`. The separate delivery branch `impl/base-audit-restore-1821-9add` ends at `2bfbcebee1520310bd4a08b8d7a28c97b798dcc0`. Its private proposal changes only LOG-04/10/15 Rust and Status cells, adds their component evidence, and updates CHANGELOG/DESIGN. All 55 logistics Acceptance cells and every other table cell remain exact. This source/test child is `test/log24-causal-source-1821-9add`; it must not be published with an absent fixture.

## Concrete test preparation

`match_logistics::causal_tests::real_enemy_move_drives_network_grace_and_owner_cut` is one cfg(test) library test. It uses actual Match construction, accepted commands, movement/path service, computed network rates, canonical reserves, real convoy updates and the existing recipient projector. It never uses the old scalar `TestWorld::supply_rate` or edits cut_for. The component clock advances by one on each composed turn. All ten original actors remain; HP and nonmoving actor poses are checked. Only selected temporal values are compared against the separate original-game recorder. This is not a full native runner or all-frame match comparison.

`harness/log24-causal-history-fixture.mjs` prepares one Source scene with nine commands, a 900-tick closure bound and 240 further ticks. It requires both temporal checkpoints, exact original actor identity/HP preservation, real closure within eight metres, unchanged terrain, no trucks and opponent reserve omission. All phase/oracle assertions fail normally. No fixture is synthesized when recording is unavailable. File inspection returns ENOENT for `rust/crates/core/tests/fixtures/log24-causal-history.json`.

The Source scene uses the same retained-constructor arena and peaceful command inputs on both sides. The original selected test uses remote placement; this composed witness instead moves an actual enemy toward a constructor troop. Independent whole-selected-check review must decide that setup correspondence. The existing original-function masks retain the held-point exemption. LOG-24 remains in progress regardless of this unrun draft.

## Assigned batch

| Row | Complete check or exact remaining gap |
| --- | --- |
| LOG-04 | Complete existing price component proposal at 2bfbcebe. `logistics::modes_initialization_and_weapon_prices_match_javascript`, with production/recorder/fixture bytes unchanged from producing main396f to frozen main379. |
| LOG-10 | Complete existing recovery component proposal at 2bfbcebe. `logistics::reserve_transitions_and_transfers_match_original_javascript` covers half-reserve recovery, emergency-zero, partial relief and blocked/stranded latch preservation/release. |
| LOG-15 | Complete existing transfer component proposal at 2bfbcebe. The same transition test covers complete stock/reserve state and exact transfer/query results. |
| LOG-24 | Causal Source/native draft above is unrun. Its setup correspondence and genuine recipient grace/cut proof require review; static mask passes stay valid. |
| BASE-05 | Existing point supply operation at `core/src/match_runtime.rs:805` is present. Complete `test.js:7954-8011` still needs a retained-constructor bridge/enemy/collapse/rubble/paid-fill history and corresponding cut income/VP/snapshot observations. No actor or terrain history is injected here. |
| BASE-09 | `164ca5435eefb619791bbb5494d0feb69b0a7f06` passed visibility/distance then failed natural final-hit removal after 900 approach plus 1,200 combat ticks. Eight accepted commands, zero completed scenes, no fixture, no native test. Original failed visibility at254c, diagnosis72cff and recovery0cc remain read-only. No retry. Classic/Horde/allied/out-killer complete-row correspondence remains open. |
| BASE-12 | `core/src/match_construction.rs:760-795` expressly refuses depot providers. Actual paid node claim, duplicate denial, cancel/destruction release and unseen/remembered enemy occupancy from `test.js:1691-1701`, `:1745-1753`, `:4143-4187` need a canonical node-bound caller. |
| BASE-13 | `core/src/production.rs:575-578` admits ordinary recruitment only in Conquest; startup rejects Classic at `match_runtime.rs:164-173`. Pure price queries pass separately. Full `test.js:2019-2051` requires genuine insufficient-Fuel refusal, paid tank queue, exact MP/Fuel charge and node income, plus the selected availability history. |
| BASE-19 | Tutorial startup is rejected by the same guard. Source `sim.js:3798-3803` requires an actual no-maker purchase, step-out and personal-rally path. Supplied Tutorial completion facts do not exercise that caller. |

Three of nine assigned rows have a complete existing proposal. Even a future causal LOG-24 PASS would make four, so the requested five-row minimum needs at least one held BASE caller completed and reviewed. Root has not granted runtime scope or execution for those new consumers. This is an exact gap, not a request to lower predicates or use diagnostic defaults.

## Pending execution selection

No slot is claimed. Root must grant the frozen recorder/inputs before either Source command, then freeze the genuine fixture before native work. Commands below are a pending selection, not an execution receipt:

```sh
WW2_REFERENCE=/home/judiazm/.t3/worktrees/ww2-rts-native/t3-29229094/reference/ww2-rts /usr/bin/node harness/log24-causal-history-fixture.mjs
WW2_REFERENCE=/home/judiazm/.t3/worktrees/ww2-rts-native/t3-29229094/reference/ww2-rts /usr/bin/node harness/log24-causal-history-fixture.mjs --check
```

After a separate exact-head slot grant and the required disk/resource preflight, the intended focused native command from `rust/` is:

```sh
df -h /
python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py +1.99.0 nextest run --locked --cargo-profile ci-test --build-jobs 4 --test-threads 1 -p ww2-core --lib -E 'test(=match_logistics::causal_tests::real_enemy_move_drives_network_grace_and_owner_cut)'
```

Exactly one new native test is selected. Root may separately select the two unchanged logistics integration tests for final-head qualification. Mechanical formatting and strict locked core all-target Clippy need their own grant. Stop at the first substantive failure. No old BASE09 attempt, discovery/list, full/release job, cleanup scan or sealing stage is proposed.

## Receipt and coordination

Read ASSIGNMENTS and `roster16_restore_1821` before writing; assigned nine rows already belong to BASE_AUDIT. Neither roster nor ledger is edited. Read README, DESIGN, AGENTS, PARITY, coordination/contracts and current issue5 through Hermes173 (`6199107614`) plus lifecycle result `6199190039`. Local ack of receipt: the general-default decision remains pending Axel, MODE/CMB diagnostics confer no row eligibility, reference e4e579a and ABI2 remain unchanged. Result6199190039 requests no action. PR16/195 replies retain the existing interface/schema limits. No public acknowledgment or owner consent is inferred.

Fresh execution: Source 0, Node 0, Cargo 0, rustc 0, native/game/server 0, tests 0, discovery 0, Rust fmt/Clippy 0, CI 0. Read-only Git/API/text/fixture inspection and local edits only. A prose-only Unslop scan of the logistics document flagged the protected table's existing todo status words; no protected status was changed to satisfy that prose scan. Historical three-platform test results remain attributed to main396f run38740304188 and its recovery review. No nested agents, push, PR, merge, public post or ledger write.
