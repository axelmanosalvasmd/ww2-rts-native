# NET second-stream source freeze

Owner `IMPL_NET_2` works from native baseline
`34edf52a7eaa6cb6edba3e802c2190226228276f` and read-only Source
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d` at
`/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts`.

This is a source-inspected plan with precise gaps, not an implementation or
runtime acceptance report. Ten candidate rows are selected below. None is newly
complete, no test has been added, and no parity status has changed. Cargo,
rustc, server execution and Source recorders have not run in this worktree.

## Ownership and coordination

The original automatic worktree was clean at `b57f56de`. Work moved to the new
exclusive worktree `impl-net-2-f4b22e5f`, branch
`impl/net-2-34edf52-f4b22e5f`, at the required baseline before edits.
The host reports `vostro`.

The Root-owned handoff ledger
`/home/judiazm/.t3/handoffs/ww2-rts-native-cargo-slots-0e8d6d9b.json`
assigns exactly 47 rows to this owner: NET-50 through NET-97 except NET-68.
NET-98 exists in neither that map nor the live server inventory. It is not
created or assigned here. NET-68 belongs to `NET_IMPL` and remains untouched.

Local acknowledgment of issue #5 requests and decisions:

- [004](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6130643550): retain JSON WebSocket v0 and the existing coordinate and recipient ownership boundaries. No wire or privacy change is proposed.
- [085](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6178373888): a documentation-only candidate still needs Root review and its applicable publication checks. This file grants no publication permission.
- [090](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6179562683) and [097](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6181065743): preserve private two-human opposing Conquest, Standard army, sandbox off, temporary logistics off, explicit host seed, existing four-family ingress and all admission guards. AI, spectators, operatives and World remain outside R1. Faulted restart remains refused.
- [129](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6185258458): incomplete row predicates stay held. No whole-match evidence amendment or gameplay/privacy choice is inferred.
- [Root's baseline readback](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6185610295), read at the final checkpoint: retain disjoint row ownership, at most three scoped native jobs and the same excluded decisions. Its pending main push is not treated as passed.

The contract index and PR #16, #195 and #198 replies were read. Their approvals
do not authorize new transport fields or client adoption. #199 and #201 are
unchanged. No acknowledgment, comment, PR or other message was posted externally.

## Selected acceptance predicates

Each entry includes the entire server behavior needed before a completion
claim. Client obligations and original acceptance text are retained separately
in [the full row map](net-2-row-plan.json), copied from the baseline inventory.
Rows with client acceptance cannot become complete merely because a server
component passes.

| Row | Required automated acceptance | Current gap |
| --- | --- | --- |
| NET-64 | Receive initial, unchanged, changed and removed rows over real sockets; compare exact row values, omission, ordered `gone`, `all` and count/XOR `held` after every frame. Independent recipients must retain independent history. Include every admitted stream role before calling the original row complete. | `UnitDeltaLedger` has a Source fixture consumer. The two-seat test proves part of resync isolation. It does not replace the full delivery sequence, and R1 excludes operative/spectator streams. |
| NET-65 | Observe first lists, unchanged omission, changed reappearance and full/reset behavior over sockets. Compare ordinary shared versions and World per-recipient content separately. Preserve Source's late-discovery behavior without fixing its stale-wreck question here. | `SnapshotDelivery` replays synthetic delivery inputs from Source. Live World delivery is not admitted; a complete socket list-change witness is missing. |
| NET-66 | Every normal, paused and catch-up snapshot contains roster-ordered `online` and `ping`; exercise connected/unknown RTT, clamped RTT, disconnect/reconnect, and AI true/-1. Both recipients see the same roster metadata. | `lobby.rs:1049-1054` uses ready sockets and optional RTT only. AI is refused at attachment and R1 admission, so its required branch cannot be proved by two-human sockets. |
| NET-67 | Deliberately miss a real changed row, detect the held mismatch in the receiving client, send a real `resync`, consume the full replacement and recover exact held state. The peer stays incremental. Verify the client's one-second request limit and the ordinary spectator stream branch. | Existing two-seat sockets send resync directly and check `all` plus peer isolation. They do not prove client corruption detection/throttling; spectators are outside R1. |
| NET-69 | A ready recipient receives frames; an absent, closing or unready socket is not selected. With no recipients, prove snapshot production is not invoked, rather than just observing an empty socket queue. Reconnection restores one eligible stream. | Selection filters ready connections. A controlled socket lifecycle test with a production-call witness is missing. It must use a test-only observation seam, not alter public interfaces or admit extra roles. |
| NET-70 | With actual socket recipients, inject explicit host times: initial due tick, just before/exactly at 50 ms, a 1 s stall capped at four ticks, dropped backlog, then normal 50 ms recovery. Assert native clock advancement and received tick identities. Preserve the 10 ms poll registration. | `CatchUpClock` fixtures prove the algorithm using a tick counter, not the socket-linked Hub. An explicit-clock transport harness is needed. Sleeping for one second is not equivalent proof. |
| NET-72 | Carry all recorded meter windows through a controlled room/socket harness: initial interval 2, at least 10 samples, last 50 snapshot samples, one-second checks, strict >40 overload up to 4, strict <24 recovery sustained for 10 s down to 2, equality/reset boundaries and winner override. Assert emitted frame ticks against the chosen cadence. | The meter replay and actual Hub caller exist separately. No complete socket witness connects the recorded duration inputs to delivery. Production timing stays measured; any explicit duration injection must remain test-only. |
| NET-74 | Real socket writes count until completion. Check just below/at/above 256 KiB in Source UTF-16 units, skipped ledgers untouched, out-of-order completion, exactly one catch-up below threshold with no shots/cells, and stale completion unable to schedule the replacement. Preserve all existing component cases. | Callback fixture coverage does not prove the transport sequence. A bounded test-only write-completion harness is needed. Source's own socket test controls inflight accounting; do not claim a simulated counter is an OS saturation measurement. |
| NET-80 | Real human EOF produces a 30 s drop pause once per player per match. Controlled time proves just-before/exact expiry; reconnect, host resume and AI handover each end it. A second drop does not pause; a fresh match restores each allowance. | Existing drop/reconnect tests cover one exit. Exact deadlines/reset and other exits remain incomplete; host resume is unavailable and R1 excludes AI handover. |
| NET-81 | Assert complete pause objects on every change and join, exact ceil-second `left`, repeated one-second pause plus frame delivery, no early repeat, and unpaused reset fields. Cover host and drop reasons and start/pause ordering. | Drop/join ordering is covered in existing socket source. Complete periodic boundary assertions are absent; host pause/resume are unavailable inputs. |

The anchors and dispositions for all 47 owned rows are in
`net-2-row-plan.json`. Unselected rows are triaged, not exhaustively audited.
Their original behavior, Source evidence, Rust requirement, Unreal requirement,
dependencies, acceptance and status are preserved without rewriting them.

Two additional dispatch gaps matter when choosing a later batch:
`lobby.rs:412-424` omits Source's in-play `pong.srv` (NET-85) and returns without
map-ping dispatch when coordinates are present (NET-86). Neither is fixed here.
Host `end`, `pause`, `resume`, save and load remain in the unavailable command
list at `lobby.rs:639-657`. These are explicit capability limits, not failing
runtime tests observed in this session.

## Work that can proceed without expanding R1

The next test-writing batch can target the two-human subsets of NET-64, NET-66,
NET-67, NET-69, NET-70, NET-72 and NET-74. Reuse the actual receiver/ClientView
and ordinary-binary test helpers. Preserve every old assertion and fixture.
Socket-visible assertions alone cannot prove skipped producer calls; a private
test seam must expose that fact. Tests must distinguish a controlled callback
or duration input from real OS backpressure or elapsed timing.

NET-64/65/66/67/80/81 still cannot be marked complete from those subsets.
Resolve their listed scope/consumer gaps separately, or obtain an explicit
owner-approved row disposition. Do not narrow their acceptance text, create
replacement IDs, add AI/spectator roles, or change dispatch merely to get five
completion claims. No request for an expansion is made by this plan.

## Pending commands and resource slots

At intake, S1/S2/S3 were assigned by Root to CMB/MAP/ENG. This worker has no
runtime slot. Root must grant a named focused slot before any command below,
with the exact frozen SHA, kernel CPU affinity, 6 GiB memory limit, no swap,
at most four build jobs and one test thread. Run serially, preserve failures,
and return exit status, logs and verified process/scope cleanup to Root.
The ledger is Root-owned and was not modified.

These existing checks are runnable after that grant, from this worktree. They
are prerequisite evidence only and do not implement the missing witnesses:

```sh
# In rust/, one command at a time inside the Root-granted scope:
cargo +1.99.0 test --locked --profile ci-test -p ww2-server --lib -j4 snapshot_delta::tests::original_source_traces_preserve_recipient_delta_history -- --exact --test-threads=1 --nocapture
cargo +1.99.0 test --locked --profile ci-test -p ww2-server --lib -j4 snapshot_lists::tests::original_trimmed_traces_preserve_source_stream_history -- --exact --test-threads=1 --nocapture
cargo +1.99.0 test --locked --profile ci-test -p ww2-server --lib -j4 tick_timing::tests::original_meter_windows_and_server_polling_loop -- --exact --test-threads=1 --nocapture
cargo +1.99.0 test --locked --profile ci-test -p ww2-server --lib -j4 snapshot_backpressure::tests::original_socket_callbacks_and_watcher_boundaries -- --exact --test-threads=1 --nocapture
cargo +1.99.0 test --locked --profile ci-test -p ww2-server --lib -j4 lobby::native_startup_tests::native_two_client_socket_tests::both_seated_sockets_move_and_stop_owned_units_in_one_match -- --exact --test-threads=1 --nocapture
cargo +1.99.0 test --locked --profile ci-test -p ww2-server --test trusted_room_process -j4 -- --test-threads=1 --nocapture
```

The last command launches the ordinary executable. It therefore requires the
same explicit server/native slot grant as compilation. No workspace, release,
test-list, standalone server or native format command is authorized by this
document. Root should choose only checks needed for a subsequent change.

Source regeneration/checking also requires a grant. The existing timing and
backpressure recorders require x64 Node `v24.21.0`. `/usr/bin/node24.21.0` is
absent on this host; Root must identify the permitted matching executable
before running these from the worktree root:

```sh
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts "$NET_SOURCE_NODE" harness/tick-timing-fixture.mjs --check
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts "$NET_SOURCE_NODE" harness/snapshot-backpressure-fixture.mjs --check
```

`NET_SOURCE_NODE` is an unassigned placeholder, not a verified command path.
No fixture regeneration is needed for this documentation-only freeze. A
future maintained socket recorder and native consumer need their own source
freeze and review before a slot run; their commands do not exist yet.

## Handoff boundary

Root must obtain a separate reviewer before publication. This branch is not
pushed and has no PR. If a later implementation shares `docs/parity/server.md`,
integrate the then-current main before PR creation, preserve both owners' cells
and rerun the owned-row diff check. This freeze changes no parity file, runtime,
test, fixture, reference file, contract, header, terrain or client source.

Validation here is static: exact baseline, clean initial worktree, Source pin
and clean Source status, 47 unique owned rows, ten selected rows, unchanged
original row fields/statuses, unchanged tracked runtime/test/fixture files and
`git diff --check`. No native, Source recorder, browser or Unreal result is
claimed.
