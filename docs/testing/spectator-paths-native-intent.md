# SPECTATOR-PATHS intent tests

Base: #377 at `07c754fee34cad80bd6873c8f74c436f6c741b81`, stacked on #372.
Reference: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
Owner decisions J and P use the prototype's current full-map spectator view
outside World. World stays closed.

| Requested spectator clause | Registered test | Source and scope |
| --- | --- | --- |
| NET-26: spectator starts | `intent_net26_spectator_start_precedes_lobby_and_pause_without_changing_seat_memory` | `server.js:455-466,612-615`. Strengthens #372's existing test: exact lobby/live start equality, common seated fields, start/pause/lobby order on late join, reconnect replacement, untouched seat terrain memory, ordinary recipient delivery. |
| NET-46: live spectator army commands | `intent_net46_spectator_army_commands_are_silent_and_never_queued` | `server.js:630-636,712-714`. Strengthens #372's existing test with valid Move/attack-move/Stop/Stance payloads, silence for all sockets, zero queued orders, and a positive seated control using the same Move. |
| NET-82: spectator close never pauses | `intent_net82_spectator_close_removes_identity_without_creating_or_releasing_drop_pause` | `server.js:568-575,718-734`. Removes a running spectator immediately, rejoins its token afresh, preserves another seat's actual drop pause, then reconnects and continues ordinary snapshots. |
| NET-77: authored objective and message privacy | `intent_net77_authored_objectives_and_private_messages_filter_spectators_through_ticks_and_restore` | `server.js:201-204,247-265,455-466`; `shared/scenarios.js:83-87,132-175`. Executes public/side/team messages and objectives for an owner, ally and enemy. Every ordinary tick collects all three seat snapshots plus the shared spectator frame, comparing exact objects; saves after activation and restores before completion. |

The first two tests already existed in #372. This PR strengthens their evidence;
it does not present their inherited behavior as a new implementation. The NET-82
and NET-77 tests are new. The table proves the requested spectator clauses only.
Full NET-26/46/82 also include operative and seated lifecycle branches owned by
other streams, including #374/#378. Those complete rows are not promoted here.
No parity status file changes.

The NET-77 runtime admits only timed, once-only match triggers and the Say,
objectiveActivate and objectiveComplete actions. Private side/team recipients
work within that subset. Other actions, conditions, scopes, localization without
English, World, AI and new operative admission remain refused. This limitation
does not substitute an empty objective list for NET-77's authored privacy check.
The inherited spectator tests retain coverage for full-map visibility, private
keys, stance/cooldown masks, public ground shots, dead identities and the ending
hold. See [spectator-missions-v0.md](../contracts/spectator-missions-v0.md).

The mission owner is registered through `core/src/lib.rs`; its four focused
component tests check exact Say shape, projection, once-only order, completed
objective protection, fractional clock boundaries, unsupported definitions and tampered restoration. The
server tests are registered through `lobby.rs` and `room_runtime.rs`.
`authored_mission_admission_refuses_unimplemented_actions_and_changed_map_bindings`
checks explicit refusal and private-map binding. New mission state uses the
existing Match tick, snapshot and checkpoint admission paths. No FFI/header,
reference, ABI or new public message type changes.
