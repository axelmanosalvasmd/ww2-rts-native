# WEATHER-SERVER

This branch stacks on #315; merges after #373. It completes the remaining
server portion of MODE-07 at reference `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
The three core weather checks from #315 remain registered and unchanged.

## Each row literal test mapping

MODE-07:
- `lobby::trusted_room_tests::mode07_server_start_restart_records_weather_inputs_and_replays_all_seats`: all seven choices, deterministic injected seed samples 0 and 71 at the server entropy boundary, start/restart epochs, private receipt inputs, reconstruction from the receipt, exact start messages, eight real ticks and both recipient snapshots after every tick, no fault. Rain and Mud initialization are also checked.
- `lobby::trusted_room_tests::mode07_server_weather_lobby_is_host_only_validated_and_locked_during_play`: host choice, guest and invalid refusal, production entropy path, live setting lock for both seats, unchanged runtime and receipt, blocked denial messages.
- `lobby::trusted_room_tests::mode07_weather_entropy_failure_preserves_lobby_and_running_match`: atomic start/restart entropy failure, unchanged epoch, receipt and socket accounting, no publication.
- `lobby::trusted_room_tests::trusted_canonical_map_weather_matrix_records_options_before_publication`: existing 33-map by seven-choice matrix now checks explicit 31-bit server weather seed recording instead of the old None expectation.

The same row retains and reruns these inherited core witnesses:

- `mode_intent_batch1::mode07_core_accepts_fresh_recorded_weather_seeds_and_replays_without_other_stream_changes`.
- `mode_intent_batch1::mode07_fixed_and_map_weather_hold_through_ticks_with_rain_and_mud_initialization`.
- `mode_intent_batch1::mode07_random_weather_transitions_once_at_the_announced_core_tick`.

The existing `trusted_room_process` tests now expect explicit seed receipts on
binary starts and restarts. All other options must remain equal across restart;
the sampled weather seed may change and must match the actual startup seed.
The seven-weather binary admission loop checks the same receipt invariants.

The inherited core checks prove fixed/map initialization and both announced
random-weather transitions. These server tests do not advance an ordinary room
through four to seven minutes. They prove the production startup inputs reach
the same deterministic core and survive ordinary ticking and both snapshots.
The Unreal picker is client-owned. Owner decision B accepts its Rust portion;
no Unreal implementation or execution is claimed here.

MODE-09 is left to ROOM-AI. Its local branch `runtime/room-ai2-20261016`,
inspected at `81cd6485` with in-progress edits, already owns Normal AI activation,
token/socket cleanup and matching-seat drop-pause resume in `lobby.rs` and
`room_runtime.rs`. Its resumed log maps these to NET-48/49 and retains host
pause and other-seat pause tests. This is a scope check, not qualification of
that other worktree.

## Integration

Starting #315 head: `829429ca`. Merge of main `fd714e36` was automatic with no
hand-edited hunks. #373 hotfix was already an ancestor through #315, so its
explicit merge reported already up to date. No prototype, ABI or parity table
is edited by this stream.

## Validation

At producing head `db7499e8852ccf3a9b0cb2626582155256bae7b8`, Rust tree
`3459913c7c0c0a9bfaf3caf4a7b387734151858b`, the Vostro S3 run passed:

- 10 focused tests: three core MODE-07 checks, six trusted-room checks and NET-41.
- All 141 server package tests, including actual binary lifecycle and weather admission.
- `cargo fmt --all --check`.

The strict locked workspace all-target Clippy log reached its completion line,
but the terminal wrapper returned 143 without a separate Cargo exit capture.
A fresh S1 recheck recorded `fmt=0` and `clippy=0`, printed
`SLOT_RELEASED`, and returned wrapper exit 0. Raw completed test
and first lint output: `/tmp/weather-db7499e8-tests-and-lint-wrapper143.log`.
The recheck log and captured codes are `/tmp/weather-lint-final.log` and
`/tmp/weather-lint-exit.txt`. Both use the same producing head and Rust tree.
Clippy command: `cargo clippy --locked --workspace --all-targets --profile ci-test -j4 -- -D warnings`.

Cargo uses `ci-test`, four build jobs and four test workers, the matching shared
slot target, 6 GiB memory and zero swap. An independent source-only review
ACCEPTed `db7499e8`; it is separate from execution and owner runtime approval.
No full workspace test suite, Source recorder or Unreal run is claimed.
Owner L/B decisions and #5 coordination were read. No issue comment is posted.
Hermes' exact-head runtime acknowledgment, hosted checks and merge remain with
the orchestrator.

The first validation attempt acquired S3 and completed formatting, then stopped
at nextest argument parsing because `-j4` and an added `--test-threads 4` named
the same option. No compilation or test ran in that attempt. Its log is
`/tmp/weather-focused-initial-arg-failure.log`. The corrected command keeps
`-j4` and sets `CARGO_BUILD_JOBS=4` in the environment.

At source head `9a301e11271a4ca24670b02139a4fbaf79ade224`, the corrected
S3 run compiled the current worktree and passed six focused weather tests.
The full server run stopped at the old complete-receipt equality assertion in
`net41_r1_restart_repeats_settings_resets_pause_and_reconnects_absent_seat`:
68 passed, one failed, 72 unrun. The two receipts differed only in weather seed
fields after the match ID was normalized. This check now expects the published
new identity and a valid recorded seed, requires derived/input seed equality,
and still compares every other receipt value exactly. Runtime code is unchanged.
The raw log is `/tmp/weather-9a301e11-net41-old-receipt-failure.log`.
The filter and mappings now use compiled module `trusted_room_tests`, rather
than the source filename `trusted_room_lobby_tests`.

Before publication, this stream's two private checkpoint commits were combined
so runtime behavior and every revised expectation ship in the same commit.
The lower #315 history and automatic main merge remain intact. Producer
`db7499e8` (including the earlier `9a301e11` checkpoint) is preserved locally at
`preserve/weather-server-db7499e8-20261016`. The final Rust tree stays exactly
`3459913c7c0c0a9bfaf3caf4a7b387734151858b`; the final documentation records the
producing-head results and makes no new execution claim for the commit identity.
The final fetch found main unchanged at `fd714e36` and required no merge.
