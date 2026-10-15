# BASE-15 complete pending-drop checkpoint

BASE-15 is ported after complete-row independent ACCEPT review of integrated head `ce3d50260d464a6bbad5dc461c8c522549e55f95` and Root verification of its actual gates. This docs/status successor preserves the exact recorder, replay, authentic fixture, registries and providers. All acceptance cells, BASE-51 and every other parity cell remain unchanged.

## Producing heads and observed results

The unchanged recorder is `harness/base-pending-paratroopers-fixture.mjs`, the replay is `rust/crates/simtest/tests/base_pending_paratroopers.rs`, and its runtime input is `tests/fixtures/base-pending-paratroopers.json`.

Source generation and exact regeneration check passed at `162e7bd7bf9506b0fd54f0544592fd201dbca6c8`, using pinned Source `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. The authentic fixture contains 3 scenes, 26 Support/Buy commands and 289 complete ticks. The observed post-constructor random-draw count is zero. Fixture commit `dd93d575658daf848cc28f1e133f8c918010fbce` preceded Cargo.

Integrated native producing head ce3d5026 passed fmt check (exit0), the lifecycle plus both actual registry guards (exit0, 3 selected, 3 PASS, 0 failed, 313 skipped across two binaries) and fresh strict locked workspace/all-target Clippy with `-D warnings` (exit0). The exact selected names were:

- `ww2-core::it every_test_file_is_compiled`
- `ww2-simtest::it base_pending_paratroopers::original_pending_paratrooper_population_lifecycle`
- `ww2-simtest::it every_test_file_is_compiled`

The ce3d5026 nextest run was `6e99a2e7-4f57-4ab3-9841-5d932032ba15`. Every Cargo invocation used the current S1 guard after a disk-floor check, with CPU0-3, 6 GiB, swap0, quota400%, four build jobs and one test thread. The compiler/dep-info confirmation was read before the authorized own-package clean for Clippy; it does not claim those artifacts remain available. Runtime fixture attribution is separate from compiler inputs. These are results on the named producing heads, not new execution on the docs/status successor.

## Complete original check and fixture context

The original selected check is `test.js:2147-2174`, including insufficient capacity, accepted delivery and accepted interception. It requires whole-stick refusal before payment, accepted reservation, denied recruitment without debit, actual arrival or interception, disappearance of the pending strike, complete reservation release and a successful purchase after interception. Unchanged economy-state checks cover the broader row's default1 and fractional Conscript0.75 population, ownership, structures, queues and pending/live drops (37 Source traces and 217 income steps).

The original `fresh` helper at `test.js:210-216` clears all constructor entities and footprints and moves spawns off-map. `test-fixtures.js:1-20` supplies a temporary producer for purchases; the interception arm assigns Cover directly. The composed recorder and replay instead retain canonical constructors, buy from their real HQ and request actual opposing Fighter Cover. They do not clear actors/structures, synthesize a maker or inject cover, population, pending/live flags or a completed drop.

The original `popOf(g,0) === g.units.size` is a setup-specific witness that no pending reservation remains: that cleared scene contains only owner0 non-structure squads with population1. The actual population rule excludes foreign actors and structures. The retained scene therefore compares owned field population plus real queues and non-live drops, separately checking the entire global ID-set delta and unchanged foreign/structure identities. The raw owned-population/global-count pairs remain explicit: 12/19 after delivery, 9/16 after interception and 10/17 after the released-slot purchase. The difference is five foreign constructor actors and two owned structures. Independent whole-row review accepted this complete predicate correspondence under PARITY; no formula or population rule changed.

## Actual native providers and completed checks

| Selected predicate | Pinned Source anchor | Existing actual native consumer | Completed replay assertion |
| --- | --- | --- | --- |
| Reject when only stick-1 slots remain, before payment | sim.js:3680-3688; test.js:2148-2151 | match_commands.rs:935-953 dispatches Support to match_support.rs:193-294, which checks actual visibility and support_population before charge | Genuine HQ Buy reaches pop10; the three-squad para request refuses Population with exact wallet/ID/queue/strike preservation |
| Accepted launch reserves all three pending squads | sim.js:532-536,3688; test.js:2153-2158 | match_support.rs:330-350 publishes canonical non-live Strike; match_recruitment.rs:114-177 derives PendingDrop facts from that real Strike | Real Buy reaches pop9; actual para admission reserves population12 and checks canonical strike metadata without setting live/delay |
| Real Buy respects the reservation without debit | sim.js:3781-3797; test.js:2159-2162 | match_commands.rs:924-934 invokes recorded_buy; match_recruitment.rs:346-377 uses actual units/queues/drops; production.rs:564-607 denies Pop before maker/publication | Actual HQ Buy refuses Population without debit, newborn/ID advance or queue change |
| Delayed delivery publishes exactly the whole stick | sim.js:5243-5263; test.js:2164-2165 | match_tick.rs:663-687 schedules step_support_strikes; match_support.rs:925-979 changes real strike lifecycle, drop_support at403-515 publishes canonical units and every sidecar | Complete ticks to delay+0.2 verify three unique newborn IDs/types/owners, valid sidecars, actual Chutes and preservation of old actors |
| Actual opposing cover intercepts, without newborns | sim.js:4697-4720,5248-5251; test.js:2163-2165 | match_support.rs:983-995 materializes a requested Cover; intercept_support at674-761 consumes that zone and emits ShotDown; real timer cleanup at1167 | Actual owner1 Cover arrives after its delay; subsequent para produces fighters ShotDown and zero drop births |
| Pending strike and reservation disappear after either outcome | sim.js:535-536,5300; test.js:2166-2167 | match_support.rs:942-952 removes the live drop's pending weight and1058-1062 retires zero-left live strikes; EconomyState.population at258-278 reads actual non-live drops | No para Strike or pending population remains; total roster delta is checked separately from owned field population |
| Interception releases all slots for a later real Buy | test.js:2168-2171 | Existing command/recruitment/production path above, reading the now-empty actual strike set | Actual post-interception HQ Buy creates one canonical newborn, charges payment and reaches pop10 (12-3+1) |

Native anchors above are relative to `rust/crates/core/src/`; Source anchors are relative to the pinned reference root. `validate_support_strike_inputs` at match_support.rs:763-864 preflights the shared ID range and all sidecars. The actual runner invokes support, anti-air and timers in Source order at match_tick.rs:663-687. No missing native provider or new runtime policy was needed. The recorder calls unchanged createGame/command/step; native calls unchanged Match::new/execute_command/advance_tick. Each completed command and tick compares the Source/native projection.

## Observed canonical paired scenes

Both sides use the declared 80x80 open authored map, ordinary Conquest, standard army, two opposing seats, factions0/1, seed701, shufflefalse, weatherfalse, techfalse, supplyfalse, logisticsfalse and sandboxfalse. Spawns remain cells(12,40) and(68,40); the remote neutral point is cell(40,8). The target/drop/cover position is world(45,81), visible from the real home HQ. Both sides start with MP10000 per seat and the same peaceful original controls. All ten constructor actors, canonical home HQ/Barracks, footprints, cells, links and original identities remain present. Baseline owner0 field population3, capacity12 and empty queues are asserted.

1. **Whole-stick refusal:** seven actual HQ rifle purchases raise owner0 population to10. A three-squad para request refuses Pop without debit or changes to the actor ID set, next ID, producer queue or strike set.
2. **Delivery:** six purchases reach pop9. Actual para admission reserves all three squads, raising population to12 without a birth. A real HQ Buy is denied without debit or ID/queue change. After 124 complete ticks, exactly three unique rifle/rifle/mg births and actual Chutes events occur; no para strike or pending reservation remains. Owned field population is12 and global count rises16 to19, with all original identities preserved.
3. **Interception:** six purchases reach pop9. Owner1 requests actual Cover; 41 full ticks materialize its delayed zone before owner0 requests para. The pending stick reserves population12 and blocks a real Buy without debit. Another124 full ticks produce actual fighters ShotDown and consumed-cover cleanup, with no para newborn or remaining reservation. A subsequent real HQ Buy charges MP and creates one canonical newborn: owned population10, global count17.

These observed scenes total 26 commands and 289 complete ticks. Every tick checks the remote point remains neutral/uncaptured with progress0, no point income or victory points, no winner and no actor/structure interaction within40m. Both seats' base-only income is compared; denied-Buy debit is checked immediately around the command. Checks also cover actual queues/jobs/rallies, linked structures and footprints, foreign poses/health, full constructor identities, publication sidecars, producer metadata for genuine purchases, absence of synthetic drop producers, cover/drop lifecycle, RNG and Chutes/ShotDown events. Logistics is disabled on both sides, with no physical reserve records. Native randomness is never reset or imported from expectations.

## Historical preparation and current limits

The plan began at `130f31446de14df8499e73067b9fa52b106cd329`, from fixed main `446671b977abfa6bf689fcb8853833d36519faab`; its absent-fixture and unexecuted wording described that preparation stage. Recorder implementation at162e7bd7 was followed by successful generation/check and fixture freeze at dd93d575. The first fmt-check failure (exit1) remains historical. Authorized formatter-only child `842ebb1d85982b233f7cfdd8c888612cf472871c` preserved the fixture and all literals/assertions and passed the original three-test selection. The normal merge with instructed main a20cac1b produced ce3d5026, whose fresh 3/0/313 result and other gates are recorded above.

Independent review `/tmp/review-base15-ce3d-whole-row-r1/REVIEW.md` returned ACCEPT and PORT eligible on complete-row source correspondence. Root separately verified ce3d5026's actual fmt/native/Clippy results and authorized this single-row status successor. Review itself ran no Source or native tests. No checks are repeated for this documentation change. Final docs-child review, publication and fresh hosted checks remain Root-owned delivery steps.

This evidence proves the selected predicate in the declared admitted arena. It does not claim literal cleared-fixture/global-size equality, zero-point-map admission, whole-match parity, new public admission behavior or client adoption. All runtime, Source, guards, terrain, ABI and interface bytes remain unchanged.
