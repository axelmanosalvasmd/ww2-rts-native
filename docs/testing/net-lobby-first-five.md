# Five lobby network components: NET-19 boundary repair pending

Rows NET-19/22/23/24/30 use six real WebSocket tests in
`rust/crates/server/src/lobby_socket_tests.rs`. Each connects a TCP socket,
performs the ordinary HTTP upgrade, sends masked client frames and reads
server frames. Socket EOF reaches the ordinary close handler. Only the
server clock is controlled through a `cfg(test)` field; maintenance uses the
existing `poll_native` path. Assertions inspect delivered lobby messages and,
where a field is private, the actual Hub object after a network barrier.

Reference: ww2-rts `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
Baseline: native main `7c9df2d581a79de48fbec0cb4d7ab8e98b488d43`.
The historical Rust tree qualified by strict Clippy is
`55676b9ea76a7abedbed4ff6c90c21e51f93a905`, preserved by `88a5b7c1`.
This private repair changes only the NET19 test body and related docs.
NET22/23/24/30 remain qualified; NET19 returns to in progress until its
decisive boundary repair has a scoped result and independent review.
All Acceptance cells and every other row remain unchanged.

## Complete component mapping

| Row | Pinned Source and selected assertion | Actual network proof |
| --- | --- | --- |
| NET-19 | `server.js:529,612,723,728,780`; RL `test.js:5772-5780` | Historical real hello/EOF and deletion pass, but retention at 60,000 ms has no expiry callback. The unrun repair closes rooms at 0/1 ms; one due callback at 60,001 must delete the older age-60,001 room while retaining the younger at exact age 60,000. A due callback at 60,051 must remove the younger. |
| NET-22 | Pure `newPlayer`, `server.js:330-333`; RL `test.js:5824-5843`, selected unused-team assertion at 5839 | Initial teams/factions are `(0,0)/(1,1)/(2,2)`. Changing Ana's team to 4 makes the fourth seat take team 0, faction 0. Real EOF/kick/refill then yields `(4,0)/(2,2)/(0,0)/(1,0)`. This distinguishes team allocation from seat index while preserving faction cycling. |
| NET-23 | Add-AI factory, `server.js:639-641`; SRV `test.js:5109-5130` | Actual host frames create AI 1 and AI 2 at Normal. Change the surviving AI 2 to Hard, remove AI 1, then add another AI: its name is AI 2 and its default is Normal, while the survivor stays Hard. Actual AI objects have empty tokens and no socket. |
| NET-24 | `seats`, `server.js:334-336`, join/addAi/sit consumers at 609/639/706; HORDE selected five-seat assertion at 7482 and `test-public-lobby.js:57-66` | All 33 authored maps across Conquest, Annihilation, Classic and Assault, plus World 6, Horde 5 and Tutorial 5: 135 cases. Expected ordinary capacities come from original authored spawns, independently of Hub capacity. Each case fills real human sockets, observes the overflow join as spectator and rejects over-capacity addAi/sit. The Horde case also checks valid defender map, Standard army and no record. |
| NET-30 | `hostOf`/`isHost`, `server.js:197-206`; RL `test.js:5781-5797` and spectator-host component at 5947 | Actual EOF moves host to the next connected human; offline humans take priority over watchers until ordinary cleanup. With only AI seats left, the first of two connected watchers can add AI, the other cannot. Closing that watcher transfers authority to the next. Existing operative admission rejects before creating a person or entering either host collection. |

The four qualified rows describe complete factory, capacity and host
components. NET19 expiry qualification remains pending the repair below. Result bookkeeping, starting a match, full-map snapshot delivery
and endgame assertions elsewhere in the enclosing Source scenarios belong
to separate components. These checks add no new operative, mode, start,
pause or privacy admission policy.

## Producing-head results

All names below have the prefix `lobby::lobby_socket_tests::`.

| Function suffix | Actual producing head | Result |
| --- | --- | --- |
| `net19_websocket_close_at_zero_expires_only_after_sixty_seconds` | `de30e9a11a6654227f417c308fd696c6d4247300` | Historical exact test once: 1 passed, 0 failed, 64 filtered. This does not distinguish strict from inclusive expiry at the full-grace boundary. |
| `net22_websocket_lowest_unused_team_is_independent_of_seat_and_faction_cycle` | `7cca098d003feb4cfd0076fb4a4d698b1a448baa` | Exact test once: 1 passed, 0 failed, 57 filtered. |
| `net23_websocket_ai_numbering_and_defaults_follow_actual_remaining_ai` | `7cca098d003feb4cfd0076fb4a4d698b1a448baa` | Exact test once: 1 passed, 0 failed, 57 filtered. |
| `net24_websocket_all_authored_mode_capacities_bound_join_ai_and_sit` | `9aa846d55f787f89af7493f85504f2b7f0186328` | PASS, all 135 printed cases. This was one of seven passing tests in the original eight-test batch; NET19 failed there. |
| `net30_websocket_host_passes_to_connected_human_with_kick_guards` | `9aa846d55f787f89af7493f85504f2b7f0186328` | PASS in that same original batch. |
| `net30_websocket_host_recomputes_across_humans_offline_fallback_and_watchers` | `7cca098d003feb4cfd0076fb4a4d698b1a448baa` | Exact test once: 1 passed, 0 failed, 57 filtered. |

At preserved `88a5b7c1`, the publication branch transferred these six bodies
without assertion changes. This repair changes only NET19; the other five
function bodies remain byte-identical to `88a5b7c1`.
At `88a5b7c1`, the three older transferred bodies were equal after removing
mechanical whitespace. All six bodies were byte-identical from `7cca098d`
to `14bdb0df`; the latter only deleted an unused six-line helper and updated
CHANGELOG/DESIGN. Those historical equalities do not describe the new NET19
body, whose qualification is pending.
These historical passes are not relabeled as new test runs on a later head.

Strict locked core/server all-targets Clippy passes once at
`14bdb0dfa5f9b97628687621add79d8120625f35`:

```text
cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --locked --profile ci-test -p ww2-core -p ww2-server --all-targets -j4 -- -D warnings
Checking ww2-server v0.1.0 (/tmp/ww2-net-first-five-b326/rust/crates/server)
Finished `ci-test` profile [optimized] target(s) in 0.94s
exit 0
```

The job uses the current S1 guard and exclusive target/flock. Its actual
kernel limits are CPU affinity 0-3, memory.max 6442450944,
memory.swap.max 0 and cpu.max 400000/100000, with four Cargo jobs and one
test thread configured. Immediate disk preflight reports 48 GiB free,
above the 20 GiB floor. The process exits and releases the lock before
handoff. No tests, Source recorder or formatter are repeated for this child.

## Existing main corroboration

NET22 also retains the Source-recorded six-seat allocation/refill fixture
in `harness/net-session-batch-fixture.mjs:208-225` and
`rust/crates/server/tests/fixtures/net-session-batch.json`. Its existing test,
`session_batch_socket_tests::source_recorded_session_admission_batch_uses_real_tcp`,
passes in all three completed hosted jobs of
[run 38783662474](https://github.com/axelmanosalvasmd/ww2-rts-native/actions/runs/38783662474)
(jobs 116590023494, 116590023561 and 116590023573).
Those logs name temporary merge `3deca64592d1f823e5b931b87509023f2720a667`.
Its whole tree, `bcc3ee9c4eb4e2b23bbdd5e2c7dfc99e3850e7ed`, equals main
`7c9df2d5`. This is existing-main evidence, not fresh hosted proof for this PR.

## NET19 R1 repair, authored and unrun

The independent exact-`88a5b7c1` review identifies a decisive witness gap,
not a runtime defect. A poll at 59,951 sets CatchUpClock due at 60,001;
the poll at 60,000 runs zero expiry callbacks. The historical pass therefore
cannot distinguish the correct strict comparison from an inclusive one.
The previous complete WebSocket expiry claim is withdrawn pending repair.

The single repaired scenario keeps every original assertion and uses two
actual rooms with real hello/EOF at clocks 0 and 1. No empty timestamp is
assigned directly. Ordinary maintenance at 59,951 and 60,000 preserves the
original observations. The existing clock getter confirms due=60,001 before
the next poll. That due callback must remove the age-60,001 room and its
room-order entry while retaining the second room with actual emptySince=1,
exact age=60,000 and an intact room-order entry. The clock advances to its
next due time, 60,051. Polling there must remove the younger room and advance
the next due time to 60,101.

The older room's removal is observable evidence that expiry ran in the same
maintenance callback that retained the younger room. Changing only the
runtime comparison to inclusive would remove both and fail the younger
lookup/retention assertion. This is source reasoning, not an executed mutant
or native result. The test calls neither expire directly nor any clock reset,
and adds no callback counter or runtime state. It reads the existing public
clock getter only. The new body is uncompiled/unrun; old passes, failed
schedules and lint results keep their producing heads. No other test is
selected for repetition.

## Retained failures and limits

At `9aa846d5`, the first NET19 schedule polled at 60,000 ms after a long
backlog, leaving CatchUpClock's next due callback at 60,050 ms. Polling at
60,001 therefore ran no callback. The approved correction adds one ordinary
poll at 59,951 ms, placing the next callback at 60,001 while preserving the
60,000/60,001 assertions. The affected test then passes at `de30e9a1`.
This establishes a test scheduling correction, not an expiry-provider defect.

The initial format check at `0210274f` reported layout differences. Its
mechanical child `7cca098d` preserves every non-whitespace test byte.
Strict Clippy there then failed with unused `Client::closed` at line 106.
The reviewed six-line deletion at `14bdb0df` removes that unused helper,
without lint suppression, and strict Clippy passes. Raw failures remain
attributed to their producing heads.

The private `0c0c85d1` sanitation/spectator failures and 93-step fixture
remain preserved outside this PR. They were never baseline-main tests and
are not claimed as passing. The separate text/UTF16 branch and held
NET-20/21/25/27/29 rows remain outside this five-row delivery. This PR makes
no new hosted-result, client-adoption or owner-acceptance claim.
