# NET-79 combined room pause

`NET-79: net79_host_pause_freezes_room_ai_armies_and_operatives_with_one_second_frames`

The test starts ordinary and Horde rooms through the host socket message path. Each has a Hard defender AI, a host commander, an operative and a spectator. Horde also has its internal scripted seat; the host ends the opening break so that seat issues real orders before pause. A fixed seed and explicit millisecond/nanosecond clock inputs make the checks deterministic.

Before pausing, the test requires actual defender AI command admission, changed AI commanders, refreshed recipient views and operative movement. A host stance order is queued immediately before the host sends `pause`. Across 62 due callbacks, the complete canonical checkpoint, cloned AI commanders, delivered AI view ticks and operative input sequence/throttle time must stay unchanged. Attempts to change stance, stop, buy or submit operative movement must have no effect. Repeated host pauses preserve the original cadence, and operative/spectator resume attempts leave the room paused.

The initial host pause sends exactly the prototype pause object to each connected role. Each later one-second boundary sends exactly that object followed by one frame with the frozen simulation tick. Every callback between those boundaries is silent. Three periodic deliveries reach the commander, operative and spectator independently. After host resume, the queued stance executes on the next simulation tick, operative input and movement continue, AI commanders and recipient views advance, and every canonical seat can still produce a recipient frame without fault.

Prototype evidence at `e4e579a`:

- `server.js:470-491`: pause/resume fields, host pause and change broadcasts.
- `server.js:500-519`: one-second pause broadcasts and frames to commanders, spectators and operatives.
- `server.js:631,700-703,712`: operative and army input gates and host controls.
- `server.js:782` and `server/room-tick.js:15-22`: pause returns before simulation and room/Horde AI scheduling.

The immediate host-control broadcast now includes operatives, repairing the missing transition messages. The existing pause objects and one-second cadence are retained. It composes #366's merged host pause with #423's room/Horde AI and #378's operative lifecycle/timing. The branch directly stacks on #423 and #378. Their prerequisite stacks remain required until merged. The main merge record is [gap-net79-merge.md](gap-net79-merge.md).

Owner F/G/K/L/J/P permissions are acknowledged. The current issue #5 merge decision replaces the older premerge Hermes acknowledgement gate; CI and merge remain the orchestrator's responsibility. World rooms remain outside this witness. No parity status file is edited.

Validation is recorded in the PR body at its producing head.
