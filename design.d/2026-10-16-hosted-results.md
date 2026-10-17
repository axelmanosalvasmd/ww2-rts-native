# Hosted Horde record ownership

The native shard supervisor uses isolated game threads within one process. The front configuration creates a record-owner thread and distributes request handles to workers. A lookup or finished run is handled serially by that owner, which alone reads and writes horde-records.json. This preserves the prototype's forwarding-process persistence ownership and best-run comparison while using the existing native thread architecture. It introduces no wire meaning or record schema change.

Hosted Horde restart seeds follow the successful match epoch; failed preparation leaves that epoch and the previous runtime unchanged. An explicit derived weather seed prevents map-weather constructors from reusing their fallback seed of 1. Both seeds stay private and recorded with the canonical match.

The ROOM-AI integration retains the existing scripted director separately from the incoming human-seat planner owner. A game seat belongs to one of these owners. The ordinary recipient producer refreshes requested human views before the director and human schedule act; both consume detached packets, and all commands enter the same recorded queue. Save restoration retains human planner state and reconstructs the internal director under the prototype's omitted-memory fallback. The separate owners preserve both qualified implementations and their existing tests.
