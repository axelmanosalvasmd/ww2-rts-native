# IMPL_BASES selected-check plan

Root reviewed the source plan at `1207a469f0ceac9288c0f4439ed037d4d69bab4c` and authorized implementation of its six selected checks across five existing rows. The recorder, native child module and append-only registration are now written. No check has run, no fixture has been generated, and no row is promoted. Match admission and runtime code are unchanged.

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

Implementation paths:

- `harness/base-construction-selected-fixture.mjs`: unmodified Source creation, actual commands and full steps, plus the selected original assertions. No execution until a Source slot grant.
- `rust/crates/simtest/tests/fixtures/base-construction-selected.json.gz`: still absent. Generate only under a granted Source slot; no placeholder or expected output was fabricated.
- `rust/crates/simtest/tests/construction_tick/base_selected.rs`: native replay and six explicit predicate checks. A child module can use the existing parent's private projection helpers without exporting a production interface.
- `rust/crates/simtest/tests/construction_tick.rs`: append only the child-module registration. Do not rewrite its existing test or count assertions.
- This plan, a unique changelog fragment and a DESIGN entry receive evidence updates after actual results.

No runtime edit is made or currently justified by a reproduced failure. If a selected assertion fails, retain the failing Source/native capture and report the exact boundary before proposing a scoped fix. Do not make the fixture pass by changing a guard, an old assertion, the Source pin, a terrain owner or a public policy.

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

## Implementation checkpoint

The native child module defines six named tests matching the predicate names in the recorder. Four Source scenes supply those tests: pillbox facing/tier, B05, Motor Pool completion/repair and mortar/Motor Pool tier gating. Tests for the two shared scenes independently replay the same recording. Counts of unique Source steps and snapshots remain unknown until recording. B05 pins exactly 1 preparation step, 700 construction steps and 600 escape steps. Motor Pool completion retains the original 1,600-step bound followed by exactly 100 repair steps.

The parent `make`, `prime`, `compare`, patch reader and event projection are reused without editing them. Every recorded initial/action/tick frame receives the existing full comparison of actors, positions, HP, footprints, structure identities and sections, production state, wallet, story, terrain batches/journal/versions, wind/environment seed, quiet Match random streams and chronological effects. The child additionally compares cooldown and canonical population. Explicit predicate assertions use native witnesses collected after that comparison. Neither a component-only placement check nor a caller-provided completion replaces the actual tick loop.

The recorder and test loader require Source `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`, x64 Node `v24.21.0`, four cases and the six exact predicate names. The fixture is read at test runtime so this unrecorded checkpoint has no missing `include_bytes!` dependency. An absent fixture causes a clear test failure; it never skips or passes. This is not a compile claim. Formatting and native compilation have not run.

### Declared setup differences

- All scenes retain every constructor actor, HQ/Barracks footprint, footprint-memory entry and linked structure. No Source fixture wrapper, temporary producer, relabelled building or generic building spawn is used.
- B05 replaces Source's cleared field and temporary negative-ID producer with a paid unwrapped rifle purchase from the retained own HQ. The resulting newborn is the builder; the constructor rifles remain. Buyer payment, real maker ID and newborn identity are recorded before the original explicit (31,31), still=5, cooldown=1e9 and empty path/order setup. Native setup asserts the authentic purchase already has no route or orders instead of erasing either. Both player wallets receive B05's 10,000 MP setup. The unused nonconstruction Munitions prime is omitted; no Munitions rule is exercised. Every original B05 assertion and the complete 700/600-step sequence remain.
- The original two-spawn fortress/tech maps and three-spawn skirmish map are retained. Source's ordinary Conquest mode alias is omitted in options to match the existing helper's `Mode::Default` consumer. All those Source cases have an absent runtime mode object; this does not claim another mode consumer.
- Every constructor actor is assigned quiet Hold Fire, Hold Position, Auto off and Auto Retreat off. Weather evolution, point supply and physical logistics are disabled. The real newborn tank receives the same declared quiet controls before the 100 repair steps; it is retained and compared throughout. No actor HP, position, route, structure or terrain expected output is imported into native state.
- The common helper starts at 5,000 MP, then an explicit recorded prime sets the original selected checks' 10,000 MP. Tech starts at tier 1 where enabled. Only the fortress scene applies the original authored tier=2 input; it does not pretend to run tier research.
- Motor Pool placement uses the unchanged original Source `siteNear` result as the Build command's declared coordinates. Native Build independently validates and stamps those coordinates; this does not test a native `siteNear` search.
- Constructor randomness follows the existing seed-701 Weather-stream pairing. Source throws on any runtime `Math.random` call; native purpose streams remain unchanged throughout the quiet scene. Source's own environment seed is captured, never reset. These are selected component checks using full steps, with no whole-match claim.

BASE-26 remains a selected facing/build check. BASE-28 does not include the separate Classic cache-invalidation check. BASE-29/30 cover only Conquest, excluding Assault, Annihilation and Horde. BASE-48 does not cover the concrete fortification branch. Cancellation and every previously held row remain held. BASE-51 and all existing ported rows remain untouched.

Ack [130](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6185630559): the whole-match, gameplay, privacy, spectator, terrain, #199 and #201 decisions remain pending. The duplicate old BASE rows mentioned there are outside this implementation task; no inventory cleanup was performed. This is local readback only.

## Execution queue for Root

Recovery owns all three slots. Source, Cargo, rustc, nextest, Clippy, formatting and native execution remain at zero for this task. No slot is inferred from a ledger entry for another BASE stream. Only static source inspection, byte preservation, hashing, diff checks and prose lint were performed. Archived build outputs are historical evidence, with no current-binary availability claim. No completed gate is repeated because its binary was removed.

Request one focused serial slot after Root reviews this implementation. Root supplies the exact systemd scope and CPU set. Preserve MemoryMax=6G, MemorySwapMax=0, CPUQuota=400%, four build jobs and four test threads; verify kernel affinity and scope limits before execution. The guard uses three coordinator Cargo locks and forces `/tmp/ww2-native-target-0e8d6d9b`. No task target directory or `--target-dir` override is permitted.

Inside that granted scope, from this worktree, run in order and stop at the first failure:

```sh
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node24.21.0 harness/base-construction-selected-fixture.mjs
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node24.21.0 harness/base-construction-selected-fixture.mjs --check
```

Freeze the authentic generated fixture in a successor commit and report its hash, all case/action/step/snapshot counts and the exact source hash before Cargo. The implementation commit intentionally has no fixture and is not integration-ready. Then, from `rust/`, inside the granted scope:

```sh
df -h /
CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py +1.99.0 test --locked --profile ci-test -p ww2-simtest --test it construction_tick::base_selected:: -j4 -- --nocapture --test-threads=4
df -h /
CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py +1.99.0 test --locked --profile ci-test -p ww2-simtest --test it construction_tick::original_paid_construction_replays_full_match_ticks -j4 -- --exact --nocapture --test-threads=4
```

The first command targets these six tests, under `construction_tick::base_selected::`:

```text
base26_pillbox_slit_faces_out_from_home
base28_b05_occupied_adjacent_complete_and_escape
base29_conquest_construction_completes_and_buys_from_real_motorpool
base30_conquest_builders_repair_after_real_tank_birth
base48_mortar_needs_company_hq
base48_pillbox_needs_company_hq
```

Root must separately authorize formatting, strict lint, full workspace, release and platform commands. The pending formatting check, also inside a granted scope and from `rust/`, is:

```sh
df -h /
python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py +1.99.0 fmt --all -- --check
```

The guard checks at least 20 GiB free immediately before Cargo. If it refuses, stop and report the refusal without cleaning another stream. Record exact SHA, command exits, scope/caps, memory events and process cleanup before releasing any slot. No result has been assumed from these command lines.

Root owns separate review before push or PR. This task performs neither. Before a shared parity document is eventually changed or submitted, integrate Root's latest approved main and prove the complete cell union without overwriting other owners' edits. No parity file is modified here.

## Source hashes at implementation freeze

SHA256 values below identify the authored code and inspected inputs. They are byte checks, not execution results. `Source/` resolves against the pinned reference path above. The new gzip fixture has no hash because it does not exist yet.

```text
5869e46f9d01fa98721cd688df021c8b47a9d01b1e599a766828e0ddef3b1284  harness/base-construction-selected-fixture.mjs
f18fced132e8b962e48c918d8806c3aa9c031ef4783b53c748f124f6a5ddae86  rust/crates/simtest/tests/construction_tick/base_selected.rs
d9f7d2ebb1f9d19b5aef359ab3a2df875e936b04f0fb341ff70a0c8d9b000812  rust/crates/simtest/tests/construction_tick.rs
641f59ab6203e8031debda917099c8b2559f8410958ab37cd81278cad69110b3  harness/construction-tick-fixture.mjs
2f66a66c0feee03de75283c4c92ac53df96bfa5ef9a5b5799780bdaa7fdb0720  rust/crates/simtest/tests/fixtures/construction-tick.json
7831cff6eb9df90f5c73cc849636c968599dd73f9ad86a2f93b3da17b4ca0207  Source/shared/sim.js
11f3b12ba1c4ea366001c3ba29146e50c0be8a8c59442b4759358501d30248f1  Source/shared/grid.js
af14eb064947e50dcaf3df31ef6118f073febc7ed80f87cbcc4f6a913dcd6e9c  Source/test-audit-combat.mjs
c72e951d7ac3e06ceb157ee1e2048543f6d6ed0fb02a19c781c59a85515a5d76  Source/test-skirmish-bases.js
12b04c0407b36bd184fba70b1ebc54c1c08dc7a7540f1d2668b4144372c9b971  Source/test-fortress.js
05d4c359046b49313aff3fb7ab6d8afe4340c9b0ea99493200dc8fff10a49ff9  Source/test-tech.js
6c666543534964cb2488ba90c13e0d7ff93218c5cf4b47431fb8279d4bf4b20d  Source/test-fixtures.js
```

## Root-approved component-map preparation after a1678

Source generation at `a1678fdee932c92c003e7bcdd2a8e559ffdf2dce` exited 1 before creation because the original component maps have no name. The original packet remains immutable at `/tmp/impl-bases-a1678-S1-h7ttp1tq/`; it includes exact caps, log hashes and completed cleanup. Root also verified that the zero-point maps fail both playable-map parsers. No Source case, tick, fixture or native instance completed at that head.

Root specifically authorizes retaining the raw original maps and errors while preparing the map consumed by both Source and native with only `name: "Base selected"` and one neutral zero-MP/zero-VP point: cell (40,4) for 80-cell maps and (32,4) for B05. This supersedes only the map-input statement in the earlier setup list. Actors, constructor footprints, producer identity, commands, authored poses, options and original predicates remain frozen.

The raw Source path calls the original unchecked constructor with the original map before the amended path runs. It is component evidence without native admission. The amended path still asserts normal Source validation. Every existing projected field in every initial/action/full-step frame is compared by its complete JSON byte string. All original predicate assertions execute in both paths. Complete action records, aliases, checkpoints and every non-map setup/result field are also compared without normalization. A single mismatch or further substantive failure stops the recorder; it tries no alternative point or setup.

The fixture, if recording succeeds, retains both complete control and amended records plus original maps, name-only validation errors, point-state traces and equal full-frame digest receipts. The native checker requires both original parser refusals and consumes only the amended map through the existing helper. The point must remain neutral and uncaptured with zero MP/VP at each frame. No whole-match outcome, native admission of raw controls or standing preparation exception is claimed.

This recorder/checker source child must be committed before Source execution. After successful raw/amended invariance, authentic generation and byte-check, the fixture must be frozen before Cargo. S1 is assigned CPU 0-3 with MemoryMax=6G, swap=0, quota=400%, jobs=4 and test threads=4. The current per-slot guard, `WW2_CARGO_SLOT=S1`, exclusive target and compiler wrapper apply; record `df -h /` immediately before each Cargo command and stop on the 20 GiB floor. Only fmt, the six named child tests and `every_test_file_is_compiled` are authorized. No old construction replay, full/release/list command, publication or status promotion is added to this grant.

Earlier source hashes above identify the uncorrected a1678 inputs. New hashes and actual run counts belong to the new immutable execution packet. No equality or pass is claimed until those runs finish.

## Authentic Source pair captured before Cargo

Recorder/checker input head: `26276eec39035f3cef0ded38966a6b21fd8d3684`. Actual `/usr/bin/node` is v24.21.0/x64. Generation and exact byte-check both exited 0 under S1, CPU 0-3, 6 GiB, swap 0 and 400% quota. Source was tracked clean at the unchanged pin. No Cargo or native execution preceded this fixture freeze.

Each invocation ran four raw controls and four amended scenes, with all six original predicates passing in both paths. Each path contains 24 actions, 2,318 actual full steps and 2,341 snapshots. The two paths have 2,341 byte-identical full-frame pairs. Generation executes 4,636 Source steps; independent byte-check executes another 4,636, for 9,272 actual Source steps across both invocations. These counts distinguish duplicated verification from unique recorded content.

| Scene | Actions per path | Full steps per path | Compared frame pairs |
| --- | --- | --- | --- |
| Pillbox facing/tier | 4 | 0 | 5 |
| B05 | 9 | 1,301 | 1,308 |
| Conquest Motor Pool completion/repair | 8 | 1,017 | 1,024 |
| Mortar/Motor Pool tier | 3 | 0 | 4 |

B05 preserves 1 preparation step, 700 build steps and 600 escape steps. The Motor Pool completes in 917 steps, followed by the exact 100-step repair sequence with its authentic tank retained. The approved point is neutral, uncaptured and worth zero MP/VP at every recorded frame. Both paths pass every original selected assertion and every non-map setup/result comparison; no normalization was applied.

The authentic gzip fixture is 703,420 bytes, SHA256 `0eebee9ebd0020adb973a8e6f8fe099bf424baf1ca8b856d34a209e10b6757e6`. Source evidence is at `/tmp/impl-bases-26276-S1-75l4dlp_/`, separate from the immutable original failure packet. The original 113-action/1,919-step/2,013-snapshot construction fixture and parent test bytes remain exact. Native raw-map admission is still expressly refused. Formatting, six selected native tests and the actual simtest registry guard remain pending after this fixture commit.
