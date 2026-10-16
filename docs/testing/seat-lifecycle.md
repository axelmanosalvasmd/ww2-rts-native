# Seat lifecycle runtime checks

This stream uses reference `e4e579a` and the existing JSON WebSocket v0
messages. It changes no contract, ABI, World admission or recipient visibility.
No open PR is a dependency.

Local readback of issue #5 decisions 004, 085, 090 and 097: preserve the
existing messages, keep AI/spectator/operative room admission closed, and
leave runtime acknowledgment and hosted merge gates to the orchestrator.
PR #198's merged ingress scope remains in place. No issue comment is posted.

Proposed row evidence:

- AI-19: `human_commander_state::tests::intent_ai19_lazy_opening_and_handover_reset`
  and `human_commander_state::tests::intent_ai19_detached_resets_survive_ordinary_ticks_and_recipient_snapshots`.
  The core state initializes lazily, starts its camera at the spawn, samples
  its opening delay from the private seat stream and discards attention on
  explicit reset. Eight ordinary ticks and both recipient snapshots exercise
  the reset lifecycle. No server AI scheduler or take/leave caller is enabled.
- NET-66: `lobby::seat_lifecycle_tests::net66_presence_and_ping_survive_scheduled_paused_catchup_and_reconnect_delivery`.
  Both human recipients receive the existing roster-ordered fields during
  ordinary delivery, pause, a controlled backlog-completion catch-up and
  reconnect. Unknown and clamped RTTs are covered. The same production
  projection checks AI online and -1 even with a stale RTT; AI attachment
  remains refused, so this is component evidence for that branch.

Dropped rows:

- NET-79: needs owner approval to admit socket pause/resume ingress.
  `lobby::seat_lifecycle_tests::client_pause_and_resume_remain_unavailable_even_for_native_host`
  checks the exact blocked denial for host and guest clients in running and
  host-paused rooms, with no peer broadcast or runtime state change.

NET-80 is dropped from the proven set: live AI handover needs room AI
activation, which is outside this batch. The human-only subset is covered by
`lobby::seat_lifecycle_tests::net80_human_drop_expiry_reconnect_host_release_and_fresh_match_allowance`:
first drop, just-before/exact 30-second expiry, reconnect, host resume, a used
allowance within the same match and a restored allowance in a new match.
Loaded-save pause remains excluded pending the save contract.

Socket pause/resume use the existing `UNAVAILABLE` rejection, as on main.
The existing host-side RoomRuntime pause APIs and disconnect/reconnect paths
remain available. The NET-80 subset uses the host API directly for release;
it does not admit a client message. No new core field, flag, command variant
or actor link is added. Pause deadlines use only the server-supplied clock.
