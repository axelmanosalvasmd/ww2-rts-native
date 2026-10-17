## Asynchronous ordinary start cancellation

The trusted R1 socket start claims the room before dispatching authored-map
loading and match construction to a blocking worker. A private identity token
binds completion to that claim. The hub lock and socket reader remain available
for host cancellation while generation runs. End clears the claim, and a stale
completion cannot replace either the ended lobby or a later start.

For a restart, the previous native runtime stays allocated but cannot tick,
receive commands or publish frames during generation. Its delivery accounts
continue to settle writes already in flight. This keeps rollback transactional
without copying canonical state or losing socket reservations. Failure restores
the same runtime and receipt, renews the handshake for sockets replaced during
generation, and applies drop pause to missing seats. Only publication advances
the match identity and deterministic seed sequence.

This implements the existing prototype start and denial shapes documented in
`docs/contracts/start-cancellation.md`. World, hosted Horde, NET-78, public room
admission and the FFI remain outside the change.
