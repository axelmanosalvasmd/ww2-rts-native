# Room AI on JSON WebSocket v0

Baseline: prototype `e4e579a`. This room implementation uses the prototype's existing message names, fields and recipient rules. It adds no wire fields or ABI calls. The ordinary trusted room remains private Conquest; Horde and World hosting are outside this change.

The host adds a seat with `{"t":"addAi"}` and sets its level with `{"t":"level","slot":1,"v":"hard"}`. Both are lobby-only. Valid levels are `easy`, `normal` and `hard`; a new AI uses `normal`. See prototype `server.js:639-643` and `shared/ai.js:233-239`.

A seated human sends `{"t":"leave"}` during play. The server keeps the seat and army, clears the token and socket, appends ` (AI)` to the name, resets the commander and delivered-view memory, and hands the seat to a Normal AI. The departing socket receives exactly `{"t":"left"}` and closes. A host sends `{"t":"handAi","slot":1}` to apply the same transition to an offline human. The slot must be an integer. Handover ends that seat's drop pause. See prototype `server.js:521-530`, `647-648` and `708-711`.

A matching nonempty reconnect token preserves the human seat, replaces its socket and name, cancels cleanup, and resets its helper state. The old socket receives exactly `{"t":"replaced"}` and closes. A former token cannot reclaim a seat after AI handover. See prototype `server.js:596-603`.

Lobby player objects retain the existing fields `name`, `connected`, `ai`, `team`, `faction` and `level`. AI seats report `connected:true`, `ai:true` and their level. Existing snapshots report those seats online, with ping `-1`. The server keeps AI seats after match completion. These shapes and values are identical to prototype `server.js:349`, `550-552`, `561-565` and `server/room-tick.js:30`.

The server advances the match before calling the shared AI schedule. Commanders act through the existing command queue and refresh their strategic packet only on the room's actual snapshot beat when they request a view. All requested packets refresh before any commander acts; other ticks retain the previous packet. See prototype `server/room-tick.js:12-21` and `shared/ai-schedule.js:18-34`.

Each AI consumes its selected recipient snapshot through `ai_view::SeatMemory`, followed by `CommanderViewMemory`. Public start terrain has authored mines hidden. Terrain changes and hurt alerts come only from that recipient's delivered rows. The policy receives no authoritative observation, hidden opponent state or foreign private plans. The server also delivers each AI's snapshot on intervening send beats to retain cell deltas and recipient hurt events; the shared schedule still controls when strategic packets become available to the commander.

Tests in `lobby::room_ai_tests` cover the exact control replies and lobby/snapshot fields, handover authorization, retained armies, delivery timing, privacy and match completion. The `ai_view::commander` tests check packet decoding and transactional memory updates.

Save/load pause reconstruction and host pause/resume ingress remain separate work. The existing drop-pause allowance, deadline and reconnect behavior remain in `RoomRuntime`; this change adds the AI-handover exit from that pause. Hermes' exact-head runtime ack is required before integration.
