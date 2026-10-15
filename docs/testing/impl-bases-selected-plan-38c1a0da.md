# IMPL_BASES selected-check plan

This is the source-only checkpoint requested before implementation and execution. It selects six original checks across five existing rows. No check has run, no fixture has been generated, and no row is promoted. The next implementation adds an authentic recorder and native replay tests within the existing ordinary construction consumer. It does not broaden match admission.

## Baseline and ownership

- Host: Vostro (`vostro`). Initial worktree status was clean.
- Native baseline: `34edf52a7eaa6cb6edba3e802c2190226228276f`.
- Exclusive branch: `impl/bases-owned-34edf52-38c1a0da`.
- Source: `/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts`, exact `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`; the native gitlink agrees.
- Ownership comes from `/home/judiazm/.t3/handoffs/ww2-rts-native-cargo-slots-0e8d6d9b.json`, `row_ownership.rows`. Root's correction transfers BASE-51 to BASE_AUDIT. BASE-51 was never edited.
- BASE-01 through BASE-19, BASE-25, BASE-34, BASE-42, BASE-43, BASE-51 and BASE-58 are outside this owner's scope. Already ported rows remain read-only even when the ledger assigns IMPL_BASES.

## Coordination readback

Read README, AGENTS, PARITY's judgment rules, the coordination and contract indexes, relevant DESIGN decisions, issue #5 body and comments, and the relevant replies on contract PRs #16 and #195. Ack `axel-interface-response-004` ([comment 6130643550](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6130643550)): retain simulation metres, the existing six-call ABI 2 boundary and current wire shapes. Ack `axel-decision-ack-waiver-and-ci-085` ([comment 6178373888](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6178373888)): its documentation/test waiver does not supply execution resources or remove Root's separate-review gate. Ack the pending-decision limits reiterated in [129](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6185258458): no whole-match criterion, gameplay or privacy approval is inferred.

These acknowledgments are local readback for Root. This task posts nothing. Contract drafts, transport #199 and consumer #201 remain outside scope.

## Selected predicates

The row IDs below are existing inventory IDs. Check numbers identify this plan's six checks, not new parity rows. Every original assertion in a selected check is retained; additional setup assertions establish authentic actor and producer identity.

| Check | Row | Original Source check | Required predicate and native consumer |
| --- | --- | --- | --- |
| 1 | BASE-26 | `test-fortress.js:8-18`, `setup`, including `the slit faces out from home, toward the enemy side` | At tier 1, the build returns `tier`. At authored tier 2, the same canonical rifle's pillbox build succeeds, creates an unfinished site and gives `abs(rot) < 0.2` on the original flat map. Compare paid wallet, actor identity, 2x2 footprint and linked structure as well. Use `Command::Build` through `Match::execute_command` and existing `execute_build`; no firing check or BASE-42 claim. |
| 2 | BASE-28 | `test-audit-combat.mjs:27-42`, complete `B05 construction refuses to entomb a live ground occupant while an adjacent site remains usable` | After one full step, building on the live rifle returns `blocked`, spends no MP and leaves its cell open; income has not reduced its wallet below the earlier value. The adjacent Barracks build succeeds. After 700 actual steps it is finished. A Move to (81,31) succeeds and after 600 actual steps the builder is within 0.5 m. Preserve every predicate, including completion and escape. Use actual Build/Move and `Match::advance_tick`, not a placement-only helper. |
| 3 | BASE-29 | `test-skirmish-bases.js:37-47`, Conquest iteration, `conquest construction completes` | An existing canonical rifle orders a Motor Pool at the original `siteNear` result. Build succeeds, an immediate tank purchase returns `needs`, and within the original 1,600-step bound the site reaches built=1. The subsequent purchase from that exact Motor Pool succeeds and creates a real tank. Use actual Build/Buy, the construction phase and the existing recruitment publication. Report the Conquest iteration only. |
| 4 | BASE-30 | `test-skirmish-bases.js:48-50`, Conquest iteration, `builders repair` | Continue the preceding real Motor Pool scene. Author its HP as 500, order the same rifle to Assist, execute exactly 100 full steps, and require HP greater than 500. Compare Source section HP, actor HP, crew and wallet at every captured boundary. Preserve the actual tank born in check 3. No fabricated completed building or component-only repair substitute. |
| 5 | BASE-48 | `test-tech.js:11-15`, `mortar needs Company HQ` | On the original ordinary Conquest setup with tech on, tier 1 and 10,000 MP, actual mortar Buy returns `tier`; wallet, population, IDs and producer queue remain unchanged. Retain the accompanying Motor Pool `tier` refusal. Use the existing Buy/Build command paths with the real constructor Barracks. |
| 6 | BASE-48 | `test-fortress.js:12-16`, `a pillbox needs Company HQ` | The actual pillbox Build returns `tier` at tier 1. After the original authored tier=2 change, the same input succeeds. Compare the denial and successful paid placement, not only `tier_of` data. Reuse check 1's recording while keeping this independently named predicate. |

Check 2 retains B05's complete assertion sequence but replaces its fixture purchase setup, which violates this task's producer rule. Source `test-fixtures.js:5-25` supplies a temporary negative-ID producer after clearing actors. The new recorder instead retains canonical constructor HQ/Barracks and buys the rifle through unwrapped `sim.command` from that actual HQ. Native replay performs the same purchase through its existing command consumer. Both record the successful purchase, buyer debit, maker ID and newborn identity before the original explicit pose/control setup. No producer is manufactured, relabelled or discarded. This setup difference is disclosed; the result is selected-predicate evidence, not byte-identical execution of the original test file.

Checks 3 and 4 select only the Conquest iteration of the original four-mode loop. Assault, Annihilation and Horde remain unproved. No result from that iteration may mark the full BASE-29 or BASE-30 row complete. Check 1 and check 6 share one physical scene; they are two original named assertions, not two independent runtime observations.

## Implementation boundary

The current ordinary native consumer already exposes `execute_build`, `execute_assist`, `execute_command`, `recorded_buy` and the sole `advance_tick`. `match_construction.rs:158-165` explicitly restricts its phase to ordinary Conquest, and `:788-790` rejects depot, shipyard and supply-cache construction. Those guards stay intact. The existing full-step replay at `simtest/tests/construction_tick.rs:795` verifies one Construction receipt after Firing and before Separation. Preserve that test, its 113 actions, 1,919 steps and 2,013 snapshots, and the old recorder/fixture bytes.

Planned new paths:

- `harness/base-construction-selected-fixture.mjs`: unmodified Source creation, actual commands and full steps, plus the selected original assertions. No execution until a Source slot grant.
- `rust/crates/simtest/tests/fixtures/base-construction-selected.json.gz`: new deterministic Source capture, generated only under a granted slot.
- `rust/crates/simtest/tests/construction_tick/base_selected.rs`: native replay and six explicit predicate checks. A child module can use the existing parent's private projection helpers without exporting a production interface.
- `rust/crates/simtest/tests/construction_tick.rs`: append only the child-module registration. Do not rewrite its existing test or count assertions.
- This plan, a unique changelog fragment and a DESIGN entry receive evidence updates after actual results.

No runtime edit is currently justified by a reproduced failure. If a selected assertion fails, retain the failing Source/native capture and report the exact boundary before proposing a scoped fix. Do not make the fixture pass by changing a guard, an old assertion, the Source pin, a terrain owner or a public policy.

Every building uses the actual Source placement or native canonical construction publication: catalog-sized footprint, correct centre, built/HP values, owner, production/rally state, footprint memory, structure ID and section entity links. Existing bootstrap actors and structural state remain intact. Newborns come from the actual producer. Source setup records authored money, tech, quiet controls and unit poses separately from expected output. No expected structures, path, HP or terrain state is installed into native state. Constructor-only random input follows the existing documented component-fixture practice; native streams are not reseeded to fit output, and no whole-match evidence is claimed.

## Full owned-row disposition

| Rows | Disposition and remaining predicate |
| --- | --- |
| BASE-20 | Held: complete explicit-maker/shortest-queue construction-mode command checks and canonical mode consumer. |
| BASE-21 | Held: actual construction-mode timed births, complete producer and unit-family coverage. |
| BASE-22 | Held: complete waiting-job cancellation, recipient ownership and canonical mode consumer. |
| BASE-23 | Held: complete personal/building rally validation and birth-routing consumer. |
| BASE-24 | Held: complete Horde purchase, payment, rally and assistance consumer. |
| BASE-26 | Selected check 1 only. Full builder/queue/payment/prerequisite/placement and other mode checks remain. |
| BASE-27 | Held: complete preview/server reasons, World territory and coastal checks. |
| BASE-28 | Selected complete B05 predicates in check 2 with disclosed authentic setup. The distinct Classic cache-invalidation check and wider stamp variants remain. |
| BASE-29 | Selected Conquest construction iteration in check 3. Other modes and the unfinished-pillbox firing check remain. |
| BASE-30 | Selected Conquest repair iteration in check 4. Logistics, supported failed-section recovery, other modes and Sudden Death remain. |
| BASE-31 | Held: `cancel` has no native command consumer here. B06 also requires section/debris retirement, later shelling and terrain delivery. No partial cancellation proof. |
| BASE-32 | Held: selected original check uses Classic; existing native Destroy does not establish that complete mode lifecycle or client key binding. |
| BASE-33 | Held: all named mode variants, actual destruction/rubble and rebuild sequence. |
| BASE-35, BASE-36, BASE-40, BASE-41, BASE-49, BASE-50 | Already ported in this baseline. Read-only; no new status, assertion or fixture changes. |
| BASE-37 | Held: complete original Classic prerequisite check lacks a Classic construction consumer. |
| BASE-38 | Held: World airborne support requirement and actual finished nearby Airfield. |
| BASE-39 | Held: coastal placement and real water birth; ordinary construction explicitly refuses Shipyards. |
| BASE-44 | Held: full wall/gate fortification-built cell check. |
| BASE-45 | Held: unfinished-cache truck refusal and authentic logistics consumer. |
| BASE-46 | Held: original World production-locked capture/recapture sequence. |
| BASE-47 | Held: full tech-switch modes and Horde initial tiers. |
| BASE-48 | Selected mortar and pillbox tier checks 5/6. Concrete fortification gating and other listed consumers remain. |
| BASE-52 | Held: complete startup mode matrix, canonical footprints and force composition. |
| BASE-53 | Held: original fortified-start mode matrix and complete cell-log comparison. |
| BASE-54 | Held: complete guard-fort distribution and retained-health ownership transfer. |
| BASE-55 | Held: both actual World recoveries and every named refusal. |
| BASE-56 | Held: actual Classic deadline/decay and complete World deadline exclusion. |
| BASE-57 | Held: original Classic nearest-finished-producer retreat and remaining home variants. |
| BASE-59 | Held: complete Classic public snapshot, filtering, queues/jobs/tech/nodes/rally. |

## Execution queue for Root

At this checkpoint S1, S2 and S3 belong to CMB, MAP and ENG. No slot was requested implicitly or taken. Root alone writes the slot ledger. Source, Cargo, rustc and native execution remain at zero for this task. Static reading, diff inspection, hashing and prose checks do not establish runtime success.

After implementation is frozen, one focused serial slot is requested for these commands in order. Use the exact CPU set, process scope and limits Root grants, with kernel-affinity and memory/swap verification before running anything:

```sh
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node24.21.0 harness/base-construction-selected-fixture.mjs
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node24.21.0 harness/base-construction-selected-fixture.mjs --check
```

Then, from `rust/`, under the same granted scope:

```sh
cargo +1.99.0 test --locked --profile ci-test -p ww2-simtest --test it construction_tick::base_selected:: -j4 -- --nocapture --test-threads=1
cargo +1.99.0 test --locked --profile ci-test -p ww2-simtest --test it construction_tick::original_paid_construction_replays_full_match_ticks -j4 -- --exact --nocapture --test-threads=1
```

These are future targets; the new recorder, fixture and test module do not exist at the plan checkpoint. Expected new predicate count is six; expected test names and actual case/step counts must be frozen with implementation before the grant. Do not invent pass counts now. Formatting, strict lint, full workspace, release and platform jobs require their own explicit Root scope. Stop and preserve the first failure. Record exact SHA, command exits, scope/caps, memory events and process cleanup before releasing any slot.

Root must assign a separate reviewer before push or PR. This task performs neither. Before a shared parity document is eventually changed or submitted, integrate Root's latest approved main and prove the complete cell union without overwriting other owners' edits. No parity file is modified in this checkpoint.
