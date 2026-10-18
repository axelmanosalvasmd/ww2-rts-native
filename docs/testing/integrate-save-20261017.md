# Save chain integration, 2026-10-17

Base: `61947c5b8a8438eb6ccbefa02e7d269c4e032472`. Branch: `integrate/save-20261017`.

Ordinary saves remain v1 and authored saves remain v2 under `docs/contracts/save-format.md`. World admission remains refused. The branch preserves each PR head with ordinary merge commits; no PR is opened.

## Manual resolutions

- #376: retain main's protocol approval text and append the editor contract link. Retain main's Classic partial-delivery checks and evidence. Compose editor routes with static-file fallbacks, editor state with worker fronts and test rigs, and editor configuration with static-root and room-capacity settings. Load the selected edited or embedded map before deriving its logistics option.
- #383: accept equivalent formatted routing and map-loading composition. Keep one copy of each static/editor configuration field and worker initializer.
- #386: retain full scenario condition/action/arrival execution with strict checkpoint field decoding, legacy-state refusal and the required execution clock. Retain canonical timed-state validation where that evaluator supports the definition; use the full engine's contextual consistency checks for other definitions. Keep authored and legacy execution mutually exclusive. Update the earlier group/reinforcement refusal tests to the engine behavior; preserve their other admission assertions. Keep the v1/v2 format contract.

- #394: retain both room-command ingress and scenario-start test module registrations.

## Rewritten tests

- `authored_scenarios_validate_ordinary_sides_and_localization_before_startup`: groups and reinforcement are admitted by the full engine; invalid sides, localization and World targets remain refused.
- `map15_map_loading_uses_the_existing_complete_scenario_validator`: the validated authored group now publishes and survives ordinary ticks and both recipient snapshots instead of being refused by the timed subset.

- `localization_side_key_is_text_and_not_an_authored_side_reference`: retain the translation-key check with the required English fallback.

- `authored_mission_admission_refuses_unimplemented_actions_and_changed_map_bindings` becomes `authored_mission_admission_runs_engine_actions_and_refuses_changed_map_bindings`: verify the newly admitted canonical destruction while retaining stale-clock, legacy-trigger, World and map-binding checks.

## Verification

The first checkpoint at `9fd2f291` compiled and ran 28 tests: 26 passed and two engine startup tests failed because the old timed validator rejected areas/groups. Independent source review identified the same gate. The repair uses the full ordinary engine validator while retaining the English-fallback restriction. Rerun pending.

The next checkpoint at `6110d917` ran 1,522 tests: 1,521 passed and one room-preparation test failed on the separate timed-only map-binding validator. The repair compares the full saved definition directly with the raw map and preserves null-scenario legacy triggers.

At `c6f4f680`, both repaired scenario startup and spectator action tests passed. The run reached 1,641 passes, one catalog-refusal assertion and four signal-aborted tests before the 6 GiB scope reported OOM. Map lookup now precedes readiness so a missing map retains its specific refusal; subsequent three-package runs use the authorized `-j2` setting.

The repaired four-merge checkpoint at `3a12e1ad` passed all 1,883 tests, with 10 skipped, using two test processes under the 6 GiB cap.

- #409 merges automatically.
- #415: retain the complete authored v2 field table and AI-memory defaults, update the contract to reflect hosted AI restoration, and combine AI payloads with `version_for(state)`. Retain surplus spectators and main's transactional operative membership/remapping. Keep the existing precise odd-tick observation assertion rather than replacing it with an unwrap.
- #416: keep one complete save/AI contract section with the required authored clock. Preserve main's World construction/escape path and mirrored construction input, while passing the incoming seat RNG to ordinary construction. Combine Classic observation admission with main's adaptive odd-tick observation support. Retain both garrison-reservation and AI-66 tests, the field-live module, and both spectator-mission/core-memory room-runtime modules.
- #424: retain main's precise Classic partial-capability lists and multi-leg retreat journeys, port incoming own/hidden-unit, private-work and cell assertions into those helpers, retain node-pair ordering and all AI/movement test modules, and compose aircraft cleanup with Classic Depot-node release. Tactics and fortification resolutions are recorded below.
- #424 tactics/fortifications: combine node assignments with mirrored construction input, retain engineering state, medic/front logic and aircraft dispatch. Preserve World's early HQ skip while ordinary HQs sort crews before skipping, matching repeated IF-25 ties.

The eight-merge focused build found a missing `VBLOCK` import: main had extracted runtime geography validation, while #409 adds a separate editor route validator using that flag. Restore the import without changing either validator.

The next focused build found #415's new test Person initializer missing main's `map_pings` field. Initialize it to the same empty history as ordinary Person creation; runtime and assertions are unchanged.

## Completed save-chain validation, 2026-10-17

The final two exact heads are merged with no-ff ancestry: #418 at
`c4a9176448130a36f1f6d9a97b4248929df12784` and #434 at
`b2ece3c642672a6eae97bcc0ccfec3c225de16fb`. The merge resolutions retain
Classic partial-capability, privacy and multi-leg retreat assertions, plus
both additive ABI query sets. Existing ABI layouts and World admission remain
unchanged.

Qualified source commit: `e28ceb34d4ea2ec93b91aed2091f55565947d89d`.
Qualified Rust tree: `af9e969e99864c72d9c1d39bd199337bf7f87a7e`.
Machine: Debian cloud, 9 logical CPUs, 9.7 GiB RAM, no swap. Rust 1.99.0,
cargo-nextest 0.9.148. Other heavy work was paused. The source tree and
Cargo.lock were unchanged throughout qualification.

- Serial prebuild passed with `cargo test --locked --no-run -j1 -p ww2-core -p ww2-server -p ww2-simtest`.
- All three repaired focused regressions passed.
- Full nextest run: **1,959 passed, zero failures, 10 pre-existing ignored tests**, 656.765 seconds.
- `cargo fmt --all --check`: passed.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo test --locked --workspace --doc`: passed; all four crates contain zero runnable doctests.

The nextest invocation used `--locked --profile ci -j4` and all three packages
above. Its compiler profile was the default test profile with
`CARGO_PROFILE_TEST_OPT_LEVEL=2`, debug information disabled, and incremental
compilation disabled. Test-profile debug assertions and overflow checks were
retained. This is an equivalent-settings default-test build, not a named
`ci-test` execution. Final combined-workspace qualification will use the named
`ci-test` compiler profile, include FFI tests, and run the release-library check.

A cloud-only nextest tool configuration reserved all four scheduling slots for
large fixture consumers and trusted binary socket tests:

```toml
[[profile.ci.overrides]]
filter = 'test(engineering_consumers::support_selected::) | test(movement_existing_checks::) | binary(trusted_room_process)'
threads-required = 'num-test-threads'
```

This was supplied with `--tool-config-file cloud:<absolute-config-path>`.
All other tests kept four-way scheduling. No assertions, retries, ignore lists
or timeouts were changed. The resource policy follows nextest's documented
[heavy-test scheduling](https://nexte.st/docs/configuration/threads-required/).

Earlier unsuccessful runs are not qualifications. The first complete diagnostic
on `0c70dcd0` had 1,949 passes and 10 unsuccessful executions: three semantic
failures and seven SIGKILL terminations. The three repairs are named in
`changelog.d/2026-10-17-integrate-save-validation.md`. The subsequent unrestricted
four-slot run on the repaired source had 1,954 passes, four SIGKILL terminations
and one socket-read timeout. The support fixture expands to 417,298,622 bytes
before JSON-tree allocation. Serial diagnosis passed both the bombing fixture
and unchanged mine-contact socket test; the resource-scheduled complete run
above then passed every enabled test. This scheduling change is explicit rather
than treating a filtered rerun as a full-suite result.

The trusted AI regression checks successful one-human-plus-AI setup, then the
original full-roster refusal and spectator matrix. A third AI gets the existing
`{"t":"deny","cmd":"addAi","reason":"blocked"}` frame. The AI lifecycle
witness reads the normal completed `Output::Snapshot` JSON delivery path, not an
omniscient substitute. General lobby capacity and World admission are unchanged.
