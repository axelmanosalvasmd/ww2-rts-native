# Selected vehicle predicates: S1 recording and focused validation

Base: `91c1d4453b8ba74b511bb950b5c651f36398f78b`, branch `test/mov-vehicle-predicates-0e8d6d9b`. Source: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. The recovered checkout had no tracked or untracked dirty draft. Its reference submodule was initially absent and is now initialized at the unchanged pin.

Five complete selected row checks pass at frozen `1cc0995ffe70eb7805885212706939b2bd7e8b2b`: MOV-16, MOV-17, MOV-18, MOV-19 and MOV-22. The original source-only checkpoint was `66f65c03bc6fb546c0cd933de1b52426f8bb18d8`. Its ordinary merge of exact main `90ce32f59508a1c991397aa6d0bb7b562b19c695` is `3f4f46461ca00d89f6cc5666018f3c0b96db2d4e`, with those two ordered parents. The incoming consolidated test layout is retained, with the same consumer registered in `rust/crates/simtest/tests/it/main.rs`. No helper import adaptation was needed. Source generation and byte-equal regeneration pass inside Root's S1 scope. The first failure at frozen `1ac386e6` remains retained below. Row statuses remain unchanged for Root's heavy-queue integration.

## Selected batch

The recorder imports the original `runMovementCase` and `createMovementCase` through the original runner. It selects seven complete journeys across the original eleven lab types: tank, medium, tankdestroyer, tiger, churchill, armoredcar, halftrack, flaktrack, rocket, rifle and AT gun. Source generation and regeneration pass 77 complete journeys, one engine acceleration/Stop sequence, 74,970 actual Source steps and six diagonal component checks. The focused native test passes the entire packet, including 3,978 exact movement checkpoints, 113 accepted recorded commands and all six diagonal checks.

All Source paths below are relative to `reference/ww2-rts` at the pin. The acceptance cells in `movement.md` are unchanged.

| Row | Complete selected predicates passing in focus | Exact Source evidence |
|---|---|---|
| MOV-16 | All eleven types finish `clearance` and `corner`; every driven hull stays clear; no stationary spin exceeds the original limit. | `test-movement-lab.js:28-31`; `tools/movement-lab-scenarios.js:2-29`; `tools/movement-lab-runner.js:31-62` |
| MOV-17 | All eleven types finish the `cliff` journey without a trajectory height violation. All three selected diagonal height layouts fail in both directions. | `test-movement-lab.js:28-31,57-60`; independent trajectory reader `tools/movement-lab-runner.js:4-21,47,59`; solver `shared/vehicle-motion.js:58-80` |
| MOV-18 | Actual owned recipient rows show acceleration from rest, finite braking after Stop, zero final speed and the selected stopping-distance bound. The actual owned plan is idle after Stop. The light tank finishes `corner` within nine seconds. | `test-engine-movement.js:53-67`; `test-movement-lab.js:35-43` |
| MOV-19 | Every original type finishes `opposing` and `parked`. Protected blockers remain within 0.01 m of their initial pose. Hull, cliff and stationary-spin checks run throughout each journey. | `test-movement-lab.js:28-31`; `tools/movement-lab-scenarios.js:112-123`; `tools/movement-lab-runner.js:31-62` |
| MOV-22 | Every original type finishes `corner`, `road` and `queue`. The three queue destinations are checked in order, within 0.25 m and below 0.1 m/s. | `tools/movement-lab-scenarios.js:95-100,110-111`; `tools/movement-lab-runner.js:44-45,52-62`; `shared/movement.js:63-70` |

The native consumer checks the complete selected predicates continuously, compares full movement checkpoints once per simulation second plus the initial/final states, and compares the engine sequence's actual rounded recipient rows and owned plan. It uses `Match::advance_tick` and the recorded command queue. It does not substitute movement callbacks, search results or traffic decisions. A failure stops the batch; no predicate is removed to obtain a promotion.

## Authentic setup and declared isolation

The original lab copies a rifle template into other unit types. At the lab's Map insertion boundary, the recorder replaces those template defaults with original `internals.spawnUnit(g, owner, type)` output. It preserves the object identity held by event closures and only reapplies the declared pose and hold controls. The original factory's later authored pose changes remain intact. Source command and step bodies are unmodified. Constructor randomness is fixed only during construction; full steps use the original functions.

The lab's Map clear is also adapted to remove only starting mobile forces. Both sides retain the real constructor HQ/barracks actors, production/rally owners, footprint cells and linked World structures. These inert bases have no lab arrival goal. Native setup compares their actual Source ID, kind, owner, pose, health and footprint rather than recreating or deleting them. An explicit subject ID identifies the moving actor; the engine sequence selects its tank and rifle separately. The original runner still checks every actor, including retained bases, and its selected journey verdicts are unchanged. This is an adapted authentic scene using the original selected predicates, not a byte-identical replay of the lab's discarded-base setup.

The original lab map has zero objectives and the factory explicitly clears objectives before stepping. Native map parsing requires 1-9 capture points (`rust/crates/core/src/map.rs:266-280`). The facade adds a valid distant binding objective before construction, then preserves the original factory's explicit zero-objective isolation. Native setup likewise clears the objective list and matching capture clocks. Both use the same valid authored terrain, spawns and constructor bindings. This is a declared component-scene setup, not proof of objective gameplay or the original invalid zero-point map being admitted natively.

Native replacement actors go through public `StagedUnit::ordinary`, `prepare_publication` and `publish_with`, including real scatter, canonical unit/control defaults, identity/allocator validation, maintained grid insertion and combat/firing/ability/recovery owner initialization. Startup terrain and structure bindings remain intact. The test requires `validate_tick_state()` before its first step and no denial for any recorded Source-accepted command. Recorded path budget/retry inputs enter through `initialize_tick_scene`; commands enter through `enqueue_command` with their actual boundary tick and sequence. The six diagonal input identities are asserted separately from their expected false results.

Actual recipient unit rows and plans come from `SnapshotCache::recipient`. The test consumes these represented fields only. It does not serialize that projection's missing capabilities as a full wire snapshot or claim server transport, Unreal adoption or a complete match.

## Held rows and runtime boundaries

| Row or gap | Held scope and exact boundary |
|---|---|
| MOV-15 | The selected engine test checks the rounded delivered hull during its corner journey (`test-engine-movement.js:112-128`). Continuous physical hull checks in the five-row batch do not replace that selected delivered-footprint check. No promotion proposed. |
| MOV-20 | Both reverse lab journeys, delivered short-reverse facing and the complete `Short retreats` engaged/calm control remain outside this batch (`test-movement-lab.js:28-31`; `test-engine-movement.js:69-74`; `test.js:2725-2738`). Existing native reverse selection reads canonical target/history state (`match_commands.rs:1508-1554`); no runtime gap is asserted without a failed authentic test. |
| MOV-21 | Its selected Hot Gates mountain-road convoy is a real Horde scene (`test-horde-navigation.mjs:49-74`). The current tick guard refuses Horde at `rust/crates/core/src/match_tick.rs:288-296`. The separate reserve/convoy guard is at lines 298-300. Recasting it as Conquest would omit that selected mode, so this row stays held. |
| Contact | Movement onto crushable cells or hostile mines is refused at `rust/crates/core/src/match_ground_contact.rs:55-78`. The selected static maps contain open ground, road, solid wall and heights. Whether every delivered journey avoids the guarded contacts remains a validation question. |
| Yielding, WIRE, cliff variants, default budget | PR #234's partial walk/request packet does not close these checks. This batch adds only the exact selected cliff layouts and journeys above. If an actual selected journey fails a yielding or cliff predicate, retain its scene/tick and exact Source/native lines for Root and owner review. No mover, terrain, guard or budget change is authorized here. The default 4096-search exhaustion check remains outside this batch (`shared/sim.js:3230-3247`). |

PR #234 is background evidence only: 26 scenes, 134 actions, zero promotions, current head `a671c30c62d996e44febfbc346c5974059446636` and tested source head `24025a5e079faa4cfe9fd4da7b0efce45d00ecb7`. Its recorder and consumer were inspected, but no old fixture or test is copied or counted as a new complete-row pass.

MOV-09 and new rows remain paused. MOV-31 through MOV-39 and MOV-91/MOV-98 files belong to the other audit and are untouched. No runtime, reference, terrain provider, guard, header, ABI, interface, DESIGN or shared CHANGELOG file is changed.

## S1 validation

Static checks performed: JavaScript syntax only, direct Rust formatting/parser check and `git diff --check`. These do not establish Rust compilation, Source verdicts, native admission or row completion. `/usr/bin/node` is the verified x64 Node v24.21.0 binary; the shell's default Node is v26.11.1. Rust 1.99.0 with rustfmt is installed.

Root grants standing S1 for this stream: 6 GiB, no swap, four build jobs, four test threads, CPUs 0-3 and 400% CPU quota. Each job asserts kernel `memory.max=6442450944`, `memory.swap.max=0`, `cpu.max=400000 100000`, affinity `[0,1,2,3]` and both thread/job environment values before execution. Source generation and check run through `/usr/bin/node` inside separate transient scopes. Generation peaks at 295,964,672 bytes and check at 286,859,264 bytes; both exit zero with all memory event counters zero. Both scopes are inactive after completion. Logs and the scope runner are retained under `harness/out/mov-s1/`.

The generated fixture has 4,867,857 bytes and SHA256 `035b57ca31a5d151f7a3378653f9d4d0a20236d9a2b6cca08d751ca4f431e9e9`. Recorder SHA256 is `e2141c1c64c33236a85f6b83392e9e58c525b7ef0a55cd021bfa96d126bb35a4`; the unchanged original consumer at frozen 1ac386e6 is `17552ea35997b6633b6bb638196b032d50ede849214e7b05a133b95dd72aecb5`. The recorder's final console label distinguishes 77 lab journeys from the engine sequence. No serialized input, Source body or predicate changed for that wording correction.

### First native failure and numeric comparison correction

The first focused run at frozen `1ac386e6a03b299d7ddd123bcb348911dd3fd134` compiled the `ci-test` target in 1 minute 6 seconds. Nextest ran only the named vehicle test: zero passed, one failed and 162 other simtest tests skipped. All 77 lab journeys completed their native checks before the last case reached `tank/engine-braking`, tick 6. Its raw serde JSON assertion at `mov_vehicle_predicates.rs:450` compared native `[11,30.0,30.0,0.0,0.5]` with Source `[11,30,30,0,0.5]` and similarly integral rifle coordinates. The numeric values were equal; serde's integer and float Number variants were different. The later engine predicates and native diagonal stage had not completed. This is not a passing batch or a runtime guard failure.

The Source row producer rounds double values (`shared/sim.js:5876-5891`); native `recipient_snapshot.rs:210-211,359-382` produces the same rounded doubles. JavaScript JSON writes integral Number values without a fractional suffix. The new consumer's `delivered_bits` comparison therefore retains the exact row count, five-column shape, integer actor identity and all four measured f64 bit patterns. It changes only the comparison of represented unit rows. Every original movement, braking, arrival, admission, plan and diagonal assertion remains. Source recorder and fixture bytes are unchanged. The corrected consumer SHA256 is `ecb4f21dc3bca5388a7d4ccb943dcb14f3536229bfcdbc0386a6a578065cfaaf`.

The failed run's peak memory is 1,956,352,000 bytes, with all memory event counters zero and the scope inactive afterward. Original log: `harness/out/mov-s1/04-focus-1ac386e6.log`, SHA256 `21a01dd77f957605c6903a42895756b6df0ad63ee32f38cd211c1766897c17ab`. It is retained without rewriting or retrying that head. Independent read-only review confirms the new comparison preserves every selected numeric predicate. The successor was frozen and recorded before its own Cargo invocation.

### Passing successor and heavy-queue handoff

Frozen `1cc0995ffe70eb7805885212706939b2bd7e8b2b` passes the sole selected nextest test: one passed, zero failed, 162 other simtest tests skipped. It executes all 78 cases through real native tick admission, the full engine recipient-row/plan sequence and the exact six diagonal checks. Source seed is 7301; the packet retains 74,970 full step inputs, 3,978 sampled movement checkpoints and 113 accepted commands. Compilation takes 35.12 seconds and the focused test takes 6.686 seconds on the Vostro. These are execution records, not a movement benchmark or whole-match comparison.

The successful S1 run peaks at 1,884,160,000 bytes. All memory event counters remain zero, actual affinity is CPUs 0-3 and the transient scope is inactive afterward. Success log: `harness/out/mov-s1/07-focus-1cc0995f.log`, SHA256 `0c1bb03190377ac06d87eb6891aba03127fed20d2d33d3a391038b124953cbcd`. The numeric-comparison source review is `harness/out/mov-s1/08-comparison-source-review.md`, SHA256 `6a65320617dd5a1d6904769bce61267f00549b22f185c166d7e96e3d69d2f689`. Source/test/fixture bytes remain frozen; subsequent evidence-only notes do not constitute another execution.

The batch is ready for Root's new-fixture heavy queue. No full-workspace, release or standalone list job ran. MOV-15, MOV-20 and MOV-21 remain held; no runtime, constructor, routing or terrain guard was weakened or removed. No row status is promoted before Root completes the remaining integration gates.

Post-focus independent source review verified the retained passing-log hash, packet counts and unchanged recorder/consumer/fixture/registration bytes against tested 1cc0995f. It found no final blocker or omitted selected predicate. The review ran no jobs. The final evidence-only successor changes this report, the five owned Evidence cells and the unique fragment; it carries the tested code rather than claiming another execution.

Owned paths relative to exact main 90ce32f5 are:

- `harness/mov-vehicle-predicates-fixture.mjs`
- `rust/crates/simtest/tests/mov_vehicle_predicates.rs`
- `rust/crates/simtest/tests/fixtures/mov-vehicle-predicates.json`
- `rust/crates/simtest/tests/it/main.rs` (two registration lines)
- `docs/parity/movement.md` (only five owned Evidence cells)
- `docs/parity/mov-vehicle-predicates.md`
- `changelog.d/2026-10-15-mov-vehicle-selected-predicates.md`

Read-only source review found and corrected an initial false assumption that Conquest startup had no base footprints. `Match::new` actually stamps bases (`match_runtime.rs:627-632,713-715,901-955`) and retained production rows require their actors (`match_production.rs:137-157`). The draft now retains those complete constructor bindings on both sides. This correction was made before any Source or native job; no failed execution is represented as a pass.

The follow-up independent source review found no remaining static blocker after constructor retention, explicit subject/engine selection, command-denial assertions and exact diagonal-input guards. That review does not establish compilation or runtime success.

Run these only inside Root's granted central resource scope, from this checkout:

```sh
/usr/bin/node harness/mov-vehicle-predicates-fixture.mjs
/usr/bin/node harness/mov-vehicle-predicates-fixture.mjs --check
```

Then, from `rust/`, run the sole new focused test:

```sh
cargo nextest run --locked --cargo-profile ci-test -p ww2-simtest --test it --test-threads 4 -E 'test(=mov_vehicle_predicates::original_vehicle_selected_journeys_through_real_admission_and_ground_orders)'
```

This command selects only the registered vehicle consumer in the incoming consolidated layout. Preserve any initial failure and diagnose it before changing setup or assertions. A successful focused run still needs separate source review and Root's integration gates before row status changes. Full, release and list jobs are not authorized here; the new fixture enters Root's heavy queue after focus and review.

Local coordination readback acknowledges issue #5's requirement for exact Source citations, complete selected checks and explicit unsupported scope, including readback 125 at exact main 90ce32f5. Missing owner agreement remains pending. This stream makes no external acknowledgment post, push, PR or external merge.
