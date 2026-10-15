# Engineering material and mine acceptance preparation

This branch, `test/engineering-material-mine-checks-0e8d6d9b`, starts at `6896521cd2e3e54575e7ab4aa8ade5f6ed295e49` and reads pinned Source `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

This private checkpoint prepares five complete acceptance checks through existing runtime providers. Source recording and Cargo/native execution are not authorized yet. Fixtures remain exact base, so the new witness-presence guards intentionally cannot pass until the approved recordings exist. No row is promoted. This checkpoint is not ready for a PR.

| Row | Exact full check | Existing provider and proposed complete witness | Pinned Source anchors |
| --- | --- | --- | --- |
| ENG-48 (`engineering.md:99`) | Port test: `worldMaterial` for every ground and object letter, with and without overrides, equals JS | `harness/world-fixture.mjs`; `core/tests/world.rs::mutation_state_journals_and_pending_match_original_source`. Append one 108-action World scene: 40 actual `worldMaterial` query pairs cover all six ground and 14 accepted object/overlay letters, default and overridden. Query effective and ground-only names after real valid mutations. N stays an overlay; A/K/Q are runtime mutations, never invalid authored v2 rows. Preserve all original 78 actions and complete state comparisons. Existing structures oracle retains section precedence/material witnesses. | Source `shared/world-layers.js:3-18,33-35`; `shared/sim.js:2504-2507,2680-2710`. |
| ENG-66 (`engineering.md:137`) | test.js "Mines" 'a bomb clears the mines'; test-engine-world.js 'clearing a mine does not reset the road damage threshold'; test-engine-acceptance.js 'clearing restores the same road surface' | `harness/field-works-fixture.mjs`, `harness/match-support-fixture.mjs`; existing field-work and support readers. Keep the original fresh-road known-clearing trace, with explicit final D/object-dot/no-mine checks. Append a paid Engineer trace on D road with R rubble and concrete/stone overrides: damage 120, lay four mines, clear them, verify nonzero wear/HP/flags/materials survive, then damage 85 must make ground + while R remains. Append one actual Dive support variation with four mine overlays around the requested point (41,41), then require zero overlays and owners after the real strike/timer provider. No direct-blast substitute for the named bomb check. | Source `test-engine-world.js:183-198`; `test-engine-acceptance.js:223-238`; `test.js:7562-7582`. |
| ENG-13 (`engineering.md:42`) | test.js "Digging" 'MG teams do not build'; "Fortifications" 'engineers build', 'conscripts build' | `harness/field-works-fixture.mjs`; `core/tests/field_works.rs::paid_placement_and_timed_cells_match_original_source`. Retain the original MG noBuilders trace. Append complete Engineer wire (five 1.5 s work steps) and Conscript wire (five 3 s steps), both requiring five X cells and no remaining job. All existing snapshots/events/payment assertions remain. | Source `test.js:584-588,592-607`; `shared/sim.js:426,5386-5413`. |
| ENG-14 (`engineering.md:43`) | test.js line 7728 check 'no trench among the trees' | Same existing field-work recorder and test. Append an all-O visible four-cell trench plan, retain an independent valid capture point at 4,4, and require blocked, no job, no payment and unchanged full snapshot. The scene uses actual paid Dig admission, not a second whitelist predicate. | Source `test.js:7738-7754`; `shared/sim.js:3635-3650`. |
| ENG-04 (`engineering.md:24`) | test.js "Fortifications" 'no way through for the tank', 'infantry walks through' | `harness/pathfinding-fixture.mjs`; `simtest/tests/pathfinding.rs::infantry_naval_and_vehicle_paths_match_original_find_path`. Append one authentic 20 by 20 Source map with a complete Y row. Record tank (20,8) and rifle (20,5), same target (20,35), through the existing findPath recorder and native search. Require zero tank waypoints and nonempty rifle waypoints, retaining exact route end/points/status/expansions. Old 1,529 queries remain. | Source `test.js:622-629`; `shared/sim.js` original findPath owner. |

## Layout and preservation

Both core and simtest have `autotests = false` and one `it` target. This extends existing test bodies and four existing recorders; it adds no test file, target, module registration or runtime helper. Both registration guards remain intact. Production source, terrain providers, interface/header/ABI, contract drafts, reference pin, Cargo files, workflows, DESIGN.md, CHANGELOG.md and the entire 80-row engineering table stay exact base. The only added release note is the unique changelog fragment. No gameplay or whole-match evidence decision is inferred.

Predicted new data: World 6 to 7 cases, 78 original plus 108 new actions and 40 material query pairs; field work 22 to 26 traces, 367 original plus 29 new actions; pathfinding 33 to 34 maps and 1,529 to 1,531 queries (predicted 1,109 Found, 300 Unreachable, 122 ExpansionLimit). Support keeps its original 5,922 checkpoints and adds one Dive variation; its exact new checkpoint count must come from the approved recording. These are static counts, not execution results.

Before accepting generated data, compare every old case/trace/query against saved baseline serialized bytes, with original order preserved, and compare all untouched fixture files byte for byte. Gzip support data must be compared after decompression by retained old trace identity, while retaining both raw compressed file hashes. Do not normalize failing pairs, exclude source files or erase old failures.

## Granted later, not executed here

Root alone grants the Source and native slots. Use x64 Node v24.21.0 and the clean exact Source pin. Run each recorder serially, preserve every command, exit, raw log and file hash, then verify generated bytes with --check. Do not redirect pathfinding directly onto its fixture until the recorder exits successfully; write scratch output and install it only after success.

```bash
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts node harness/world-fixture.mjs
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts node harness/field-works-fixture.mjs
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts node harness/pathfinding-fixture.mjs > /tmp/eng-material-mine-prep-0e8d6d9b/pathfinding.generated.json
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts node harness/match-support-fixture.mjs
```

Verify each installed recording with these exact byte checks, under the same Source grant:

```bash
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts node harness/world-fixture.mjs --check
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts node harness/field-works-fixture.mjs --check
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts node harness/pathfinding-fixture.mjs --check
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts node harness/match-support-fixture.mjs --check
```

After Root authorizes native execution, the exact four existing acceptance tests and two registration guards are:

```bash
cd rust
cargo nextest run --locked --cargo-profile ci-test -p ww2-core -p ww2-simtest --test it -E 'test(=world::mutation_state_journals_and_pending_match_original_source) | test(=field_works::paid_placement_and_timed_cells_match_original_source) | test(=pathfinding::infantry_naval_and_vehicle_paths_match_original_find_path) | test(=match_support::actual_support_admission_strikes_and_timers_match_source) | test(=every_test_file_is_compiled)'
```

The exact registration guard name is verified in core `tests/it/main.rs:95` and simtest `tests/it/main.rs:145`. Required formatting, strict workspace/all-target Clippy, full workspace nextest, workspace doctests and release/strict C gates need their own Root grant after the fixtures exist. No historical #240 or 2fd result qualifies this checkpoint. A mismatch requiring runtime, terrain or interface work holds the affected row; it does not authorize changing that provider or weakening its acceptance.

Only after all selected checks pass may the five owned status cells change. Freeze that actual complete head and obtain a separate final review before opening any PR. No push, PR, CI watcher, issue #5 post or merge is part of this preparation.

## Held rows

ENG-03/45 require moved-vehicle crushing; ENG-05 lacks the firing pit range consumer; ENG-15 lacks queued Dig; ENG-37 has Source/native validation reason differences. ENG-16,17,23,24,38,41,42,44,47,50,51,67,70,71,78,79,80 remain held for their full selected witnesses or owner decisions. The prior ten-row matrix and 27-row audit are retained at `/tmp/eng-next-after240-0e8d6d9b/`. No partial subset is used to pad this five-row batch.

Coordination was read through Axel 128 (`6185037590`). Its exact-head and owner-decision boundaries are retained. ABI 2 remains authoritative only for its approved six calls; no protocol/terrain draft is adopted. No public acknowledgment was posted, following the stream instruction.

## Recorded Source checkpoint

All four planned generate/check pairs passed with x64 Node v24.21.0 under the S3 six-GiB, zero-swap, CPU 8-11, 400% quota scope. World records seven cases and 186 actions; field work records 26 traces and 396 actions; pathfinding records 67 scenarios and 1,531 queries (1,109 Found, 300 Unreachable, 122 ExpansionLimit); support records 269 traces and 5,943 checkpoints. The new material scene has 108 actions and 40 query pairs, and the new Dive trace has 21 checkpoints.

Raw JSON array-entry comparison preserves all six original World cases, 22 field-work traces and 66 path scenarios as exact prefixes. All 268 original support trace identities and their raw decompressed JSON bytes survive in original relative order. Top-level metadata outside those arrays remains unchanged. The pathfinding output was installed only after its recorder exited successfully.

The source scope ended with zero memory-cap/OOM events and no remaining child. These results establish Source recording and fixture preservation, not native passes or row promotion. Native execution awaits this fixture commit and the separately recorded focused-stage result. The earlier predicted 33/34 path count denotes the native reader's changed-map counter; the actual serialized scenario count is 66/67.
