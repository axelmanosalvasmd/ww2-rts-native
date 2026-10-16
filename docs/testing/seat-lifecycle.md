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
- NET-79: `lobby::seat_lifecycle_tests::net79_host_controls_freeze_ticks_and_orders_with_operative_admission_closed`.
  Host messages pause and resume; non-host messages cannot do so. No tick,
  simulation RNG or queued-command progress occurs during pause. A frame and
  pause update go out at each one-second boundary. Already queued input runs
  after resume, and input received during pause is absent. Operative hello is
  refused before admission and its subsequent input cannot enter the match.
  The test reaches an ordinary Conquest victory and completes all 120 ending
  callbacks with recipient frames and no fault. Room AI remains unavailable.

NET-80 is dropped from the proven set: live AI handover needs room AI
activation, which is outside this batch. The human-only subset is covered by
`lobby::seat_lifecycle_tests::net80_human_drop_expiry_reconnect_host_release_and_fresh_match_allowance`:
first drop, just-before/exact 30-second expiry, reconnect, host resume, a used
allowance within the same match and a restored allowance in a new match.
Loaded-save pause remains excluded pending the save contract.

The new host controls use the existing RoomRuntime pause state. No new core
field, flag, command variant or actor link is added. Ordinary tick, snapshot,
command-admission and elimination guards therefore see the existing state;
the lifecycle test runs through resumed ticks, recipient publication and the
result hold. Pause deadlines use only the server-supplied clock.
