# NET runtime batch 2

Base: `origin/main` at `a076f316cdd7c48b52af2f706a5a908e4e935f97`. Branch: `runtime/net-batch2-20261016`. This batch starts from main and imports no commits or helpers from PR #284, #199 or #201. The 16 proposed rows in #284 are outside this batch. All 66 previous exclusions are accounted for below: 8 proposed proven rows, 57 dropped rows and 1 differs candidate. Parity tables are unchanged.

Acknowledged owner decision [6206861267](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267) and runtime acknowledgment requirement [6207938018](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6207938018). No issue comment was posted. The agreed ABI 2, existing JSON v0 shapes and reference pin `e4e579a838aabae3b1d9dadbc17a0a87147bc52d` remain the baseline. World admission stays off. There are no FFI, header, contract, prototype, privacy-policy or parity-status edits.

## Proposed complete witnesses

- NET-06: `static_files::tests::intent_net06_static_prefixes_mime_and_confined_files_over_http`
- NET-07: `static_files::tests::intent_net07_static_etag_conditional_gzip_and_file_version_invalidation`
- NET-08: `static_files::tests::intent_net08_browser_page_routes_and_explicit_static_root_opt_in`
- NET-16: `intent_net16_binary_room_cap_rejects_new_rooms_and_keeps_existing_rooms_reachable`; `tests::max_rooms_defaults_floors_and_clamps`
- NET-43: `lobby::net_runtime_command_tests::intent_net43_buy_uses_host_keys_and_returns_resource_denial_after_real_purchase`; `lobby::net_runtime_command_tests::intent_net43_malformed_full_payload_and_unknown_name_cannot_reserve_input`; `lobby::net_runtime_command_tests::intent_net43_paused_orders_are_ignored_and_replacement_keeps_stable_issuer_and_feedback`
- NET-51: `lobby::net_runtime_tests::intent_net51_only_current_host_ends_match_and_frees_offline_seats_without_late_frames`
- NET-85: `lobby::net_runtime_tests::intent_net85_latency_echo_clamp_and_measured_play_telemetry_are_personal`
- NET-86: `lobby::net_runtime_tests::intent_net86_map_ping_validates_before_budget_and_reaches_only_ready_human_teammates`

Static tests use temporary supplied assets through the actual production HTTP router. They prove delivery and routing, without claiming a playable browser client or Unreal adoption. `WW2_STATIC_ROOT` is an explicit asset-root opt-in. NET-06 includes ordinary traversal plus Unix-only symlink checks. NET-07 uses a literal Source-compatible tag and controlled filesystem modification times; an equal-size/equal-time rewrite decisively exercises cache reuse before size/time invalidation.

NET-16 launches the real binary at the default cap, a small override and both clamp limits. It checks exact capacity refusal/close and existing-room reachability. Its pure parser cases cover decimal/radix values, zero/invalid fallback, infinities and ECMAScript whitespace.

NET-43 covers command entry and feedback for valid protocol messages with string tags, rather than claiming every gameplay family is implemented. A real constructor wallet pays for one queued rifle; a second purchase returns the core's `mp` denial to its issuer. Tests check host-stamped keys, whole malformed payload refusal, unknown tags, paused silence and replacement-socket delivery. The existing malformed/non-string-tag boundary is preserved. Unsupported core command families remain unsupported. Support admission uses the snapshot adapter's own allowed list before queue reservation. `every_support_kind_keeps_ordinary_r1_scheduled_snapshots_flowing` sends every catalog support kind with at least 200 MP and zero cooldown through ordinary and trusted R1 rooms, checks real admission or refusal, and collects scheduled snapshots without room faults. `recon_and_queued_cover_refuse_before_reservation_and_keep_ordinary_r1_snapshots_flowing` checks that ordinary and trusted R1 rooms continue delivering snapshots after both refusals.

Recon, queued Cover and the refused support kinds (Strafe, Bombing, Dive and Para) are a temporary server-runtime gap under NET-43; the prototype admits them.

NET-51 covers healthy current-host manual end, pause release, offline-seat cleanup, retention of socket write ledgers, absence of later frames and a fresh subsequent match. Native preparation is synchronous under the Hub lock; no asynchronous start can finish after end. `manual_end_restart_old_completion_cannot_consume_current_snapshot_ticket` holds an old snapshot write, manually ends and starts a new match on the same socket, reserves a new snapshot, then completes the old and new writes and checks continued delivery. General asynchronous mode startup remains outside NET-37's proof.

NET-85 drives the actual production meter with samples whose p50, p95 and maximum differ to prove p95 rounding/cadence telemetry; it is not a machine-performance measurement. NET-86 checks invalid-before-budget validation, inclusive map bounds, rounding, rolling-window/reconnect limits and ready human team routing, with lobby, unseated, AI-sender and AI-recipient exclusions. AI and spectator guard fixtures temporarily alter the admitted human roster without claiming support for those roles in native match admission. It relays only voluntarily submitted ping coordinates and preserves the prototype recipient rule.

## Dropped rows

- NET-04: Deflate negotiation and its frame threshold need a different WebSocket transport path; not implemented in this batch. [Upstream transport limitation](https://github.com/snapview/tungstenite-rs/blob/master/README.md).
- NET-11: needs contract: authorize authenticated raw-authoring map reads, currently forbidden to HTTP clients.
- NET-12: needs contract: authorize editor writes and persistent authored-map ownership before exposing mutable map data.
- NET-13: Editor password generation and comparison have no admitted raw-read/write consumer; depends on NET-11/12 authorization.
- NET-14: Tailscale discovery and native invite substitution remain unavailable.
- NET-18: Tutorial constructor, running-mode admission and auto-start remain unavailable.
- NET-20: ECMAScript name trimming and UTF-16 display-boundary policy remain unresolved; token identity is unchanged.
- NET-21: needs contract: approve in-play spectator admission and its recipient view.
- NET-25: needs contract: operative rejoin/identity scope; human AI/helper reset also remains unavailable.
- NET-26: needs contract: operative and spectator start projections.
- NET-28: needs contract: operative broadcast membership; save listing also remains unavailable.
- NET-29: needs contract: operative rename requires an admitted operative identity.
- NET-32: No genuine attached AI roster and think scheduler; a human-slot level refusal would be a partial witness.
- NET-35: needs contract: an admitted in-play spectator is required to prove the live sit guard.
- NET-36: General public start and complete Assault runtime/readiness remain unavailable.
- NET-37: General start claim/cancellation/rollback is unavailable; existing native preparation is synchronous and atomic.
- NET-38: World admission off.
- NET-39: World admission off; full option mapping also needs broader ordinary mode and logistics admission.
- NET-40: needs contract: all-role begin-match publication; AI/Horde view reset also remains unavailable.
- NET-41: General restart contexts remain unavailable beyond the trusted Conquest profile.
- NET-42: World admission off.
- NET-46: needs contract: genuine operative and active spectator sessions are required for the silent-command witness.
- NET-47: Logistics-enabled tick and snapshot admission are explicitly refused; away-wallet checks alone do not prove convoy pause.
- NET-48: Persistent human-to-AI handover, helper reset and actual AI scheduling remain unavailable.
- NET-49: Depends on the missing persistent AI handover in NET-48.
- NET-50: Horde runtime and next-wave control remain unavailable.
- NET-52: Airborne-plane snapshot admission/projection remains unavailable.
- NET-53: needs contract: settle the cached/uncached nonowner gate, operative and cargo flag discrepancy.
- NET-54: Air/logistics admission and complete owner-private list coverage remain unavailable; no recipient policy was expanded.
- NET-55: needs contract: authored side/team objective and announcement disclosure; current legacy scenario output is public-only with empty objectives.
- NET-56: World admission off; complete independent ordinary pending-terrain witnesses also remain absent.
- NET-57: needs contract: complete row includes the ordinary spectator no-fog path.
- NET-58: needs contract: recipient policy for live flight/local projectile actors; those actors remain unadmitted.
- NET-59: World admission off; full effect-reveal coverage remains incomplete.
- NET-63: needs contract: resolve NET-53 before adding an uncached production projection and claiming equivalence.
- NET-66: Genuine AI attachment is unavailable, so AI online/-1-ping semantics cannot be proved.
- NET-67: World admission off; spectator streams and native client resync throttle remain unavailable.
- NET-71: World admission off; autosave and diagnostic scheduler branches remain unavailable.
- NET-73: Actual AI/Horde think scheduling remains unavailable.
- NET-75: needs contract: a production spectator stream is required before wiring watcher backpressure.
- NET-76: World admission off; ordinary spectator admission also needs a recipient-policy contract.
- NET-77: needs contract: approve full-map ordinary spectator projection and private-key removal.
- NET-78: Horde/story end contexts and AI idle scheduling remain incomplete.
- NET-79: Host pause/resume is implemented and tested, but operative-input suppression remains unproved because operative admission is off.
- NET-80: Persistent AI takeover and loaded-save host pause remain unavailable.
- NET-82: needs contract: full close behavior includes persistent operative identities.
- NET-87: Native client reconnect/backoff consumer is outside the server runtime and remains unavailable.
- NET-88: Native client token/view persistence consumer remains unavailable.
- NET-89: needs contract: agree the native save schema and continuation guarantees before manual saves.
- NET-90: World admission off; native persistence is also unavailable.
- NET-91: needs contract: native save format/versioning and legacy-save compatibility decision.
- NET-92: needs contract: save restoration schema and unmatched-human spectator assignment.
- NET-93: Native save summaries depend on the missing NET-91 storage format and implementation.
- NET-94: Horde results, record persistence and front-owned shard writes remain unavailable.
- NET-95: Sandbox spawning and complete actor admission remain unavailable.
- NET-96: Sharded front/worker routing and IPC remain unavailable.
- NET-97: Diagnostic files, phase logging and native browser-metrics consumer remain unavailable.

## Differs candidate

- NET-15: Prefer truncating display text without splitting a Unicode scalar, keeping valid UTF-8 instead of emitting a lone UTF-16 surrogate. This visible policy choice needs owner agreement. It is not implemented here; native lossy conversion and NET-20 remain unresolved.

## Validation

Final rebased Rust tree `24f1fc628751ea5d2199e8452afb3b8664bb6fc2` passed all 83 server tests, zero failed and zero skipped. Formatting and strict locked workspace all-target Clippy passed. Raw log: `/tmp/ww2-net-runtime-final.log`; S1 was released with exit 0. The source review accepted the eight scoped intent claims; the final independent NET-43/51 review ran zero tests and made no edits or GitHub writes.

The earlier regression run found an obsolete four-command capability expectation in `native_ingress_socket_tests.rs`. It now checks the actual core `attack` visibility denial, `unseen`, instead of `blocked`. Initial test-only expectations for the snapshot tag and manual-end personal result were also corrected against actual output before this final successful run. Cargo runs use the shared S1/S2/S3 locks, the designated slot target, four build jobs, a 6 GiB scope with no swap and the 20 GiB free-disk floor. No full workspace test suite, CI wait, merge, browser run or Source recorder is part of this batch.

### Round-2 fix

After merging main `92688084369ac20ef1cf77b9e1a156a2f1f9fbcb`, Rust tree `2dd6090419e3dffcec9d16a8321f06d54d7ccc5d` passed all 11 focused command, snapshot, manual-end, NET-85 and NET-86 tests, formatting and strict locked workspace all-target Clippy on the Vostro. The new regression is `lobby::net_runtime_command_tests::every_support_kind_keeps_ordinary_r1_scheduled_snapshots_flowing`. It covers all eight catalog support kinds in ordinary and trusted R1 rooms, with sufficient MP, zero cooldown and scheduled snapshots for both recipients. Final checks used S2 with four build jobs, a 6 GiB scope, zero swap and the 20 GiB free-disk floor; the slot was released. Logs: `/tmp/fix296-focused-tests.log`, `/tmp/fix296-fmt.log` and `/tmp/fix296-clippy.log`.

Temporarily restoring the old Recon-only ingress guard reproduced a Bombing snapshot fault: `snapshot collection: native snapshot: Unsupported { unit: 0, capability: TeamEffects }`. That guard was restored to the fixed version before final verification. The reproducer log is `/tmp/fix296-regression-old-ingress.log`.

The broader 120-test server run stopped at the unchanged `actual_socket_start_decode_mutations_resync_reconnect_and_new_match`, `native_startup_tests.rs:549`: the post-resync `ClientView` equality check differed in cumulative `seen` fog state. That attempt ran 57 tests: 56 passed, one failed and 63 were unrun. It was not retried or repaired in this fix. The raw failure is `/tmp/fix296-server-suite-resync-failure.log`; its cause remains for follow-up. The final focused selection excludes that failed test.
