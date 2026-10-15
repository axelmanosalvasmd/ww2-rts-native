# Selected vehicle predicates: source-only checkpoint

Base: `91c1d4453b8ba74b511bb950b5c651f36398f78b`, branch `test/mov-vehicle-predicates-0e8d6d9b`. Source: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. The recovered checkout had no tracked or untracked dirty draft. Its reference submodule was initially absent and is now initialized at the unchanged pin.

This draft targets five existing selected row checks. It has not run a Source recorder, compiled Rust or executed a native test. The expected fixture `rust/crates/simtest/tests/fixtures/mov-vehicle-predicates.json` does not exist yet. No row is promoted. The native test reads that file at runtime and fails explicitly if it is missing.

## Selected batch

The recorder imports the original `runMovementCase` and `createMovementCase` through the original runner. It selects seven complete journeys across the original eleven lab types: tank, medium, tankdestroyer, tiger, churchill, armoredcar, halftrack, flaktrack, rocket, rifle and AT gun. This means 77 complete journeys, one engine acceleration/Stop sequence and six diagonal component checks. These are planned inputs and assertions, not measured passing counts.

All Source paths below are relative to `reference/ww2-rts` at the pin. The acceptance cells in `movement.md` are unchanged.

| Row | Complete selected predicates prepared | Exact Source evidence |
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

## Validation queued for Root

Static checks performed: JavaScript syntax only, direct Rust formatting/parser check and `git diff --check`. These do not establish Rust compilation, Source verdicts, native admission or row completion. `/usr/bin/node` is the verified x64 Node v24.21.0 binary; the shell's default Node is v26.11.1. Rust 1.99.0 with rustfmt is installed.

Read-only source review found and corrected an initial false assumption that Conquest startup had no base footprints. `Match::new` actually stamps bases (`match_runtime.rs:627-632,713-715,901-955`) and retained production rows require their actors (`match_production.rs:137-157`). The draft now retains those complete constructor bindings on both sides. This correction was made before any Source or native job; no failed execution is represented as a pass.

The follow-up independent source review found no remaining static blocker after constructor retention, explicit subject/engine selection, command-denial assertions and exact diagonal-input guards. That review does not establish compilation or runtime success.

Run these only inside Root's granted central resource scope, from this checkout:

```sh
/usr/bin/node harness/mov-vehicle-predicates-fixture.mjs
/usr/bin/node harness/mov-vehicle-predicates-fixture.mjs --check
```

Then, from `rust/`, run the sole new focused test:

```sh
cargo test --locked --profile ci-test -p ww2-simtest --test mov_vehicle_predicates original_vehicle_selected_journeys_through_real_admission_and_ground_orders -- --exact --nocapture
```

This command targets the recovered branch's existing test layout. If Root later carries it onto the consolidated test layout, register the same test in that layout and select its exact name through nextest. Do not add a second version of the test. Preserve any initial failure and diagnose it before changing setup or assertions. A successful focused run still needs separate source review and Root's integration gates before row status changes.

Local coordination readback acknowledges issue #5's requirement for exact Source citations, complete selected checks and explicit unsupported scope. Missing owner agreement remains pending. This source-only checkpoint makes no external acknowledgment post, push, PR or merge.
