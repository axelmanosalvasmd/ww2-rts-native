# SPECTATORS-2 native intent evidence

Base: `origin/runtime/spectators-20261016` at
`e614103611f044ffb4b32d658402c4c5d353e765` (PR #372).
Prototype: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
This test PR stacks on #372; merges after it. It adds no runtime behavior.

Owner clarification J is applied as confirmed in
[Axel's decision](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6218552386):
the current full map, with no delay outside World Conquest. World stays closed.
The inherited dead-identity spectator behavior from #372 is retained.
No parity status, ABI, header, wire shape, save schema or reference pin changes.

## Each row literal test mapping

- NET-57: `intent_net57_seated_fog_starts_deltas_and_reconnect_leave_watchers_unmasked`
  in `rust/crates/server/src/spectator_tests.rs`, registered by the existing
  `lobby::spectator_tests` module. Two seated starts contain exactly the `v` and
  `e` fog keys. An independent run decoder reconstructs the canonical visible
  and explored masks. An unchanged delivery omits fog. Moving the team's
  observers across the map and advancing six ordinary ticks produces string
  deltas that XOR into the current team mask, including both newly visible
  and newly hidden ground. Reconnecting sends fresh full masks and preserves
  exploration of ground that is now hidden. The next unchanged snapshot omits
  fog. A spectator receives no fog at start or on any delivered frame. Every
  tick collects both seats' and the spectator's snapshots without a fault.

Prototype review references: `server.js:217-220,455-466` selects the spectator
projection and omits its mask; `shared/sim.js:4620-4642` changes a team's version
only when visibility changes; `:4646-4690` defines the run alphabet, varints,
full masks and delta acknowledgement. The existing
`cmb132_fog_packets_and_run_roundtrips_match_original_source` component test
also checks 64 exact encoder cases and eight delivery actions against its
already committed prototype fixture. This stream adds no Source execution,
recorder or captured fixture.

## Dropped rows

- NET-40: complete beginMatch includes AI views, the Horde seat and operative
  starts. `lobby.rs::prepare_native_room` refuses AI commander seats, and
  operative admission and hosted Horde are outside this branch. Ordinary
  start coverage from #372 does not prove those branches or the complete row.
- NET-77: the admitted legacy scenario engine has no authored objectives and
  admits only public Say actions. `recipient_snapshot/public_state.rs` emits
  an empty objective list, and `legacy_scenario.rs` fixes migrated trigger
  recipients to public. Public/private authored objective and message
  filtering therefore needs additional runtime owners. Existing unit masks,
  private-key removal and shot tests from #372 remain partial evidence.
- NET-92: `lobby_saves.rs` refuses non-null AI memories, AI seats and surplus
  connected humans; `prepare_native_room` also refuses AI seats. Restoring
  surplus humans as spectators alone is small, but cannot complete this row.
  Hosted AI-memory restoration and fresh human AI helpers need a larger
  integration, so this stream leaves save restoration unchanged.

No differs candidates. Range empty after these decisions. The orchestrator
owns status updates, exact-head runtime acknowledgement for #372, hosted
checks, review and merge.
