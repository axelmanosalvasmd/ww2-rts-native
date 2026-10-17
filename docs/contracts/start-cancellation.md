# R1 asynchronous start cancellation

This is an implementation compatibility note for prototype `e4e579a`, not a new
protocol version or an agreed extension. Public admission remains the private
two-human Conquest R1 profile. World, hosted Horde and NET-78 remain outside this
change.

The pinned prototype claims a room as `play` before asynchronous map loading,
with no playable game while loading (`server.js:358-363`). A second start cannot
claim it. The host can end that pending start through the existing `end` message
(`server.js:696-697`); the obsolete operation checks its token before publishing
(`server.js:366-372`). Native generation runs on a blocking worker outside the
hub lock. The socket loop can process `end` while that worker is outstanding.

No message fields are added. A failed map load or match construction sends every
connected commander exactly
`{"t":"deny","cmd":"start","reason":"scenario"}`, including when the
request was a restart, then sends the existing lobby projection
(`server.js:365-369,380-390`). Failure restores the lobby or the previous match,
including its result and pause. Success uses the existing pause, playing lobby
and recipient start sequence (`server.js:395-412`). An ended pending start uses
the existing lobby result `{"ended":true,"you":null}` and cannot publish later
(`server.js:561-565,696-697`). No seeds, generation tokens or backup state cross
the wire.

`lobby::async_start_tests` provides the exact denial and ended-result checks:

- `net37_cancelled_generation_cannot_replace_new_start_or_restart_and_match_finishes`
  cancels initial and restarted generation, completes stale success and failure
  operations against a newer token, then runs the successful ordinary match
  through its VP result and all 120 ending callbacks.
- `net37_real_load_and_creation_failures_restore_lobby_or_previous_match_and_finish`
  invokes actual authored-map lookup and `Match::new` failures from both lobby
  and play, checks unchanged canonical checkpoints and delivery accounts, then
  runs the retained match through its ordinary result.
- `net37_async_socket_dispatch_stays_available_for_end_while_worker_is_blocked`
  holds the sole blocking worker with a channel, sends start, duplicate start,
  end and a new start through the socket dispatcher, and checks that only the
  replacement publishes. It then runs that match through its ordinary result.
- `net37_failed_restart_settles_old_writes_and_restores_replaced_seat_before_finishing`
  settles pre-claim snapshot writes while loading, replaces a disconnected
  seat, restores that seat's original-match handshake on failure, and finishes
  the retained match without a fault.

Every continuation collects both commanders' recipient snapshots on each
callback while a runtime remains attached and checks that no runtime fault
appears. Hosted Horde's missing-defender rollback branch remains guarded along
with hosted Horde admission; these tests make no claim about that branch.
