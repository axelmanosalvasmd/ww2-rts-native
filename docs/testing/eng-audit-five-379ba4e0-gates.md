# Five recovered engineering checks

Baseline: frozen main `379ba4e06c794708623c005ab6052bebd03d8809`. Recovered checkpoint: `dd035fe2033739d9831f909d5ccc5066166153b6`. Role: ENG_AUDIT, exclusively ENG-04, ENG-13, ENG-14, ENG-48 and ENG-66. The normal merge preserves both histories. This plan is source-only and reports zero new executions.

| Row | Complete selected witness | Actual native consumer |
| --- | --- | --- |
| ENG-04 | One complete Y wall refuses a tank route and admits the rifle route. Compare every point, route end, status and expansion count. | `pathfinding::infantry_naval_and_vehicle_paths_match_original_find_path` |
| ENG-13 | MG refusal plus complete Engineer and Conscript wire jobs at five 1.5-second and five 3-second steps. Compare payment, jobs, events and all snapshots. | `field_works::paid_placement_and_timed_cells_match_original_source` |
| ENG-14 | Actual paid Dig admission refuses an all-woods plan and leaves the complete initial snapshot unchanged. | Same field-work test |
| ENG-48 | All six ground and 14 object/overlay letters, with and without overrides: 40 effective/ground-only material query pairs and full World state. | `world::mutation_state_journals_and_pending_match_original_source` |
| ENG-66 | Known clearing restores the road, preserves wear/health/materials beneath rubble, and retains the second-hit threshold; actual Dive support clears all four mines. | Field-work test plus `match_support::actual_support_admission_strikes_and_timers_match_source` |

Both packages use `autotests = false` and target `it` at `tests/it/main.rs`. Current module declarations compile all four consumers. Include both packages' existing `every_test_file_is_compiled` guards. The exact proposed six-instance filter is:

```bash
cargo nextest run --locked --cargo-profile ci-test -p ww2-core -p ww2-simtest --test it --build-jobs 4 --test-threads 1 -E 'test(=world::mutation_state_journals_and_pending_match_original_source) | test(=field_works::paid_placement_and_timed_cells_match_original_source) | test(=pathfinding::infantry_naval_and_vehicle_paths_match_original_find_path) | test(=match_support::actual_support_admission_strikes_and_timers_match_source) | test(=every_test_file_is_compiled)'
```

Proposed formatting and strict checks, subject to Root's exact grant:

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --profile ci-test -j 4 -- -D warnings
```

Run only through Root's current guard, assigned S1/S2/S3 target and exclusive lock, with actual granted CPU/memory/swap/quota and the 20 GiB disk floor checked before every job. First substantive failure stops the sequence and is retained. No Source regeneration, test-list/discovery, full or release command is requested. Logs and one-line dep-info binding to the actual producing head suffice.

The old six-test pass belongs to `df9a0cfff0ecf290491175e671fe01afa6368c2d`: 6 passed, 0 failed, 350 skipped across two binaries, plus fmt and strict workspace/all-target Clippy exit0. Raw logs are `/tmp/eng-integration-df9a0cff-0e8d6d9b/`; independent old-head review is `/tmp/roster16-reviewer1-eng396d-final/REVIEW.md`. Neither establishes a pass for this integration. Preserve Source generation/check at preparation491b, fixtures at03e6, the03e6 five-pass/one-map-count-failure, the6a6d single corrected pass and initial guard exit126. No failure is rerun here.

Fixture coverage retained: World7cases/186actions; field works26traces/396actions; pathfinding67scenes/1531queries (1109Found,300Unreachable,122ExpansionLimit,33authored maps); support269traces/5943checkpoints. Every old case remains. This plan introduces no runtime, provider, interface, ABI, protocol, private-view, terrain-policy or reference-pin change. All five selected checks have actual native consumers; wider integration capabilities do not add gates to these complete component predicates.
