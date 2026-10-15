# NET2 socket test implementation freeze

This unrun successor to plan `6bd3336dcf8fa2de803a7672868c1aa54ba5c713`
adds five tests for approved two-human R1 subsets. It promotes no row. The
[original 47-row predicate map](net-2-row-plan.json) and the plan's whole-row
acceptance table are unchanged. The implementation starts from that exact
commit in exclusive branch `impl/net-2-34edf52-f4b22e5f` on Vostro.

## New witnesses

Four unit tests live in `rust/crates/server/src/native_net2_socket_tests.rs`,
registered under `lobby::native_startup_tests::native_net2_socket_tests`:

- `r1_eligibility_skips_production_and_reconnects_real_receivers`: both ready seats receive one shared production; canceling both connections while holding the Hub lock allows due ticks to advance without invoking the snapshot producer or changing recipient ledgers. Actual socket close callbacks then establish two absent seats; a paused frame request still produces nothing. Same-token reconnects restore one live frame per recipient without a fault.
- `r1_socket_ticks_follow_explicit_clock_boundaries_and_stall_cap`: explicit timestamps check the immediate tick, just-before/exact 50 ms, a 1 s stall capped at four ticks, dropped backlog and normal recovery. Exact canonical clocks, next deadlines and both receivers' snapshot tick identities are asserted. This protects the polling algorithm's socket integration, not OS timer precision.
- `r1_meter_thresholds_drive_real_socket_cadence_and_ending_hold`: the existing `RoomRuntime::record_timing` API consumes controlled duration inputs for nine/ten sample admission, unsent samples, budget equality, overload, the 4-tick cap, recovery equality, an interrupted low window, nine/ten seconds and the 2-tick floor. Actual Hub polls and both receivers check intervals 3, 4 and 2. Canonical victory setup then checks the existing winner predicate and half-speed ending delivery at a tick outside the 4-tick cadence.
- `r1_backpressure_preserves_skipped_history_and_drains_one_catchup`: controlled outstanding reservations establish one below, exactly at and above the 256 KiB UTF-16 accounting threshold while actual snapshots use the socket pump. The blocked seat retains its ledger and terrain-send flag; its peer receives ticks 6 and 8. An out-of-order completion at the cap produces nothing; draining one below the cap produces exactly one current frame with empty shots/cells. UTF-16 Unicode accounting, healthy restart/old-epoch drain and same-token takeover/stale completion preserve the replacement's history and authority.

One ordinary-executable test lives in
`rust/crates/server/tests/trusted_room_process/net2.rs`, registered under
`net2` in the existing `trusted_room_process` target:

- `ordinary_r1_socket_metadata_resync_and_drop_pause_use_live_receivers`: the existing `BinaryServer`, `Client` and R1 `begin` helpers drive the ordinary executable. Both humans receive online/unknown RTT, clamped RTT and independent resync metadata. Genuine EOF produces paused frames with the disconnected seat offline and an unchanged simulation tick; same-token reconnect receives start/pause/resume/lobby, then live snapshots at the same match identity. Every received snapshot is replayed through `ClientView` with count/XOR held checks. This does not measure exact one-second timer boundaries or establish an AI/client reconnect implementation.

All five tests are new source, not passes. The old receiver, executable helper,
old tests, assertions and fixtures remain byte-identical except for additive
child-module registrations in their two parent test files.

## Harness and observation boundary

The controlled tests bind real loopback TCP sockets and reuse the existing
`Receiver`, WebSocket upgrade handler, complete socket loop, R1 admission,
canonical start constructor, recipient producers, decoder and write-completion
routing. A test-local Axum router exposes the existing `/ws` handler. It has no
maintenance task: the test invokes the actual `Hub::poll_native` with explicit
host times. It does not copy the scheduling or selection implementation and
never sends constructed snapshot JSON.

The only production-file additions are twelve lines under `cfg(test)` in
`room_runtime.rs`: a per-runtime counter, initialization, getter and increment
immediately before `ordinary_snapshot_raw_lists()` in nonempty `collect_frame`.
The zero-recipient early return precedes the counter. The counter observes
attempted producer entry, including any later refusal; it does not select
recipients, alter a clock, inject results or change non-test builds.

The cadence test supplies duration samples through the existing public runtime
API. Actual poll samples are still measured. Its controlled duration timestamps
are in the future relative to poll time zero, so no meter check occurs halfway
through an asserted burst. Fifty fresh samples replace the snapshot window
before each new p95 condition, so those conditions do not depend on earlier
measured samples. These controlled inputs establish neither a benchmark nor
a full replay of every historical meter trace or real-world recovery latency.

The backpressure test reserves accounting for artificial outstanding writes,
like pinned Source's `test-snapshot-backpressure.mjs:28-36`. These reservations
are never sent as packets. Real snapshots reserve, serialize, write, complete
and decode through the ordinary pipeline, so these assertions check
callback/accounting integration without claiming OS send-buffer saturation or
deferred actual callbacks. After a same-token takeover, the real socket loop
may retire its old account before the test completes that old ticket; either
a retired or drained old account is allowed, while the replacement must retain
its own history and receive no catch-up.

Source expectations were read from pinned `server.js:470-519`, `:621-629`,
`:684-687`, `:737-800`, `server/room-tick.js:14-45`, `tickmeter.js:11-42` and
`test-snapshot-backpressure.mjs`. The existing timing and backpressure fixtures
and their consumers remain unchanged. No Source recorder was run.

## Per-row limits

NET-69 gains the controlled producer-entry observation plus actual cancellation,
EOF and reconnect receivers. NET-70 gains socket-linked explicit-clock
boundaries; production's 10 ms maintenance registration is unchanged. NET-72
gains selected threshold-to-delivery conditions and terminal cadence, rather
than a complete historical-window/socket matrix. NET-74 gains controlled
callback/accounting integration and replacement authority, rather than OS
backlog proof. They all remain at their prior statuses.

The executable witness supplies additional human-only NET-66/67/80/81 evidence.
AI online/ping, World/personal lists, spectator/operative streams, complete end
families, client corruption detection/throttling, host pause/resume and AI
handover remain held. No admission guard, wire shape, privacy policy, runtime
policy, terrain provider, reference pin or whole-match criterion changed.
NET-68 remains owned elsewhere; NET-98 remains absent.

Local acknowledgment: issue #5's
[130](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6185630559)
retains the pending owner decisions and exact-head evidence boundaries. The
request to inspect duplicate BASE text belongs to its owner and is not acted
on here. No external acknowledgment, comment, push or PR was posted.

## Pending commands

Root has not granted this worker a freed runtime slot. Do not execute these
until Root assigns the exact frozen head and CPU set in a systemd scope.
Retain `MemoryMax=6G`, `MemorySwapMax=0`, `CPUQuota=400%`, the assigned CPU set,
the four-build-job envelope and four test threads. Explicit `-j3` below keeps
effective Cargo builds at the new three-job cap. The existing guard supplies
`CARGO_TARGET_DIR=/tmp/ww2-native-target-0e8d6d9b` and checks the 20 GiB free-space
floor immediately before exec. Do not pass `--target-dir` or use a task target.
Stop on a disk-floor refusal and report it; do not clean another stream.

From this worktree's `rust/` directory, inside that granted scope, each command
is a separate invocation with its own preceding disk record:

```sh
df -h /
python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py +1.99.0 fmt --all -- --check

df -h /
python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py +1.99.0 test --locked --profile ci-test -p ww2-server --lib -j3 lobby::native_startup_tests::native_net2_socket_tests:: -- --test-threads=4 --nocapture

df -h /
python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py +1.99.0 test --locked --profile ci-test -p ww2-server --test trusted_room_process -j3 net2::ordinary_r1_socket_metadata_resync_and_drop_pause_use_live_receivers -- --exact --test-threads=4 --nocapture

df -h /
python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py +1.99.0 clippy --locked --profile ci-test -p ww2-server --all-targets -j3 -- -D warnings
```

Expected new test inventory is four library tests plus one process test.
Compilation, formatting, lint and all five executions remain pending. A format
or compile repair needs a successor freeze and does not inherit a pass.
These commands request no workspace test, release build or Source execution.
Root chooses which stages to grant, and receives exact argv, exits, logs, head,
kernel limits and verified process/scope cleanup afterward. Do not repeat old
completed gates just because disk recovery removed their binaries.

## Static freeze checks

Before commit, inspect the complete diff, verify the 47-row map and server
inventory are unchanged, compare all old helper/test/fixture bytes, remove the
twelve known test-only insertions from `room_runtime.rs` and require exact
baseline equality, then run `git diff --check` and prose lint. Those checks
cannot prove compilation, format compliance or socket execution. Root retains
separate review and publication ownership.
