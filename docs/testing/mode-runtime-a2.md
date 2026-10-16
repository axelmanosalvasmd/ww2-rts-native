# MODE runtime A2

Scope: core portions of MODE-07 and MODE-09, based on main `caeb67545c6241574e92132ac376ce8e113a2de1` and prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. No code is imported from PRs #301, #298 or #287.

## Behavior and tests

MODE-07, core only:

- `mode07_core_accepts_fresh_recorded_weather_seeds_and_replays_without_other_stream_changes`: explicit weather seeds are stored, replay the same plan and leave deployment, wind and other RNG state unchanged. An omitted weather seed remains deterministic. This does not generate fresh server entropy.
- `mode07_fixed_and_map_weather_hold_through_ticks_with_rain_and_mud_initialization`: every fixed weather kind and map-selected kind enters the canonical Match state, Rain starts raining and soaked, Mud starts soaked, and fixed choices hold beyond the random transition window.
- `mode07_random_weather_transitions_once_at_the_announced_core_tick`: ordinary ticks retain the initial weather until the announced tick, then apply Fog-to-Clear or Rain-to-Mud once within the prototype's time windows.

The existing exact weather tests retain the prototype comparisons for plans and warning rows. Source: `shared/weather.js:60-78` and `shared/sim.js:801-809`.

MODE-09, core only:

- `mode09_human_handover_resets_to_normal_and_applies_orders_after_accepted_input`: replacing a prior Hard commander yields a Normal commander with fresh memory and no delivered view. Accepted input stays queued, the new AI chooses unused keys, and its orders pass through ordinary tick admission into moving-unit paths.
- `mode09_handover_rejects_invalid_out_finished_and_exhausted_seats_without_mutation`: reject invalid, eliminated or finished seats and sequence exhaustion without changing the queue or RNG.

`Match::handover_to_normal_ai` creates the existing bounded opening policy. It does not activate server AI, change seat ownership, reset a separately owned human camera controller or supply a complete strategic policy. The host must retain the returned commander and schedule it through the existing `tick_opening_commander` entry point. Source: `shared/ai.js:220-239` and `server.js:521-526`.

These tests cover the requested core portions. Neither entire row can be marked ported until its server work and prototype-intent review are complete.

## Deferred rows

- MODE-06: complete all-mode army scaling needs the Horde/Tutorial and other mode constructors under review. Existing lobby Endless rejection is insufficient to prove it.
- MODE-11: all-mode defaults, Horde tech tiers, scenario overrides and World logistics remain incomplete. World startup still refuses logistics.
- MODE-75: World administrative elimination still refuses riders, cargo and shared work. Source deletes units and lets later carrier phases repair surviving links; wreck collateral and transactional cleanup need integration beyond this batch.
- MODE-30: runtime damage hardcodes zero Horde defenders, explosion scaling is missing, and current actor admission does not support Horde. Integrate after the independent mode work lands.
- MODE-01, MODE-02, MODE-16, MODE-17: seven-mode construction, fallback and shared enemy/spawn setup need the pending Horde/Tutorial constructors.

MODE-04, MODE-10, MODE-21, MODE-22 and MODE-97 to MODE-103 are outside this stream.

## Validation

Initial checks in locked slot S2 passed all 25 focused nextest tests (the five new tests and 20 retained weather/opening-policy regressions), formatting and strict workspace Clippy.

After merging main `09e03d07dfb7a0458c0d795db1fbb7a9a3912e51`, source head `73ae6bb2690254e56f4ec8af1d93c078bae3c31c` (Rust tree `041c2186e8979d062c2b26c2e7533962feb45adf`) passed the same 25 tests, `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -j4 -- -D warnings` in locked slot S3. This run used ci-test, four build jobs, four test workers, zero retries, 6 GiB memory and zero swap on the Vostro. S3 was released with exit zero. The published head preserves that exact Rust tree. No full workspace test run or Source recorder ran. Hosted checks remain a separate coordinator gate.

The first S2 invocation passed formatting, then nextest rejected a duplicated `--test-threads` argument before compilation: nextest's `-j4` already sets test threads. It produced no test result. The corrected invocation omits the duplicate option; both logs are retained under `/tmp/ww2-mode-a2-20261016/`.

The first compiled S2 run executed 15 of 25 tests: 13 passed, including both handover tests, and two weather tests failed because their shared test map had no capture points. The map loader correctly reported `needs 1-9 capture points`; 10 tests were unrun after fail-fast. The scene now includes one neutral capture point, with no loader or runtime guard change. The successful fresh run described above used that repaired scene.

The coordinator owns review, runtime acknowledgment, parity status, hosted checks and merge.
