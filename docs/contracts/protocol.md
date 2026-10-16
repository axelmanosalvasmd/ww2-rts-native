# Network protocol

Status: **v0 draft, technical answers recorded.** Author: Julio. Consumer reviewer: Axel.
Axel answered the drafts' technical questions on #5 (`axel-interface-response-004`, 2026-10-12); the answers are recorded below. Under `docs/agent-coordination.md` a comment is not an agreed interface, so the status stays "draft" until Axel approves a contract PR.
Baseline: reference ww2-rts `e4e579a`; native `main` after PR #7.

## What v0 is

v0 is the WebSocket protocol the JavaScript game speaks at the pin, written down so three pairs can talk during the transition:

- the Unreal client against the reference Node server (issue #4, slice 1a);
- the pinned browser client against the Rust server (slice 1b);
- Unreal clients against the Rust server (slice 1c).

The JavaScript game is a prototype (`AGENTS.md`), but v0 keeps its protocol because the browser client is the only working client until Unreal replaces it. Improvements to the protocol itself (binary encoding, orders instead of positions for your own units, UDP through relays) belong to later versions, each in its own contract PR. Nothing in v0 redesigns the reference handshake; the changes below are additions the reference server and browser client ignore, or server-side corrections that keep the browser client working.

## Field reference

[`docs/parity/protocol-messages.md`](../parity/protocol-messages.md) is the field-level reference for v0: transport, all 60 client-to-server and 11 server-to-client message types, the snapshot envelope, unit rows and flag bits, unit deltas, terrain rows, the fog run encoding, the companion status block and the session lifecycle, each with its source line at `e4e579a`. Where that file marks something **unverified**, v0 does not fix it either; the recorded fixtures below settle it.

The behavior both sides must reproduce (who may send what, when, and with what effect) is in [`docs/parity/server.md`](../parity/server.md) (NET rows) and [`docs/parity/fps.md`](../parity/fps.md) (FPS rows).

## Conventions every implementation follows

- **Transport:** `ws://<host>/ws?room=<code>` (`wss://` behind HTTPS); UTF-8 JSON text frames, one object per frame, discriminated by the string field `t`; permessage-deflate for messages of 1 KiB or more; at most 16 KiB per inbound message.
- **Identity:** the `hello` token is the only identity in v0. A reconnect sends the same token and gets the same seat back.
- **Units:** positions are metres on the ground plane, `x` east and `z` south, usually rounded to 0.1 m; a cell is 2 m and cell index is `c = y * w + x`; durations are seconds unless the field says ticks; the simulation runs at 20 ticks per second. Converting to Unreal axes and centimetres belongs to `terrain.md`.
- **Unknown input:** both sides ignore unknown fields. The server ignores unknown message types outside play; during play the reference answers them with `deny {reason:'blocked'}`, and the Rust server does the same.
- **Snapshots are deltas.** A client keeps the unit rows it holds, applies `gone` and `units`, and checks its held set against `held = [count, xor of ids]`. On a mismatch it sends `resync` (at most once a second) and the next snapshot carries `all: true`. `wrecks` and `nodes` are absent when unchanged.

## Additions in v0

These are new relative to the reference. The pinned browser client and Node server ignore them, so no ww2-rts change is needed.

1. **Version check.** `hello` gains `proto: 0` (integer) and `client: string` (for example `"unreal/0.1"`). The Rust server accepts a `hello` without `proto` as the browser client. A `proto` it does not support gets `deny {cmd:'join', reason:'version'}` and close code 1008. `lobby` gains `proto: 0` so a client can tell which server it reached; the reference sends no `proto`, which a client reads as "reference server".
2. **Rules hash.** `lobby` gains `rules: string`, a hash of the rules data compiled into the Rust server's core. A client compares it with `ww2_rules_hash()` from the core it links (`rust-unreal-abi.md`) and warns on a mismatch, because its previews would then disagree with the server. The reference sends no `rules`.
3. **Closed list of deny reasons.** `deny.reason` is one of the strings the reference sends (listed per command in the field reference, plus `version`). A client shows a generic sentence for a reason it does not know, as the browser client does today.

## Corrections the Rust server makes

The recorded owner decisions retain prototype operative hit positions, ordinary spectator visibility and the cached nonowner mask.

1. **The companion's hit direction.** Rust retains the exact reference behavior: `operative.from` sends the attacker's position rounded to metres while `tick - hitAt <= 10`, even when the attacker is hidden; otherwise it is null (`server.js:294-306`). The earlier proposal to replace it with a point 20 m along the bearing is not adopted. The browser client uses the bearing (`client/operative-view.js:342`).
2. **Spectators outside World Conquest.** Owner clarification J on 2026-10-16 selects (c), the reference behavior: the whole map with no delay outside World. The proposed observer delay and seat 0 team view are not adopted. World alone uses seat 0's team view and remains closed on R1. See [ordinary spectator delivery](spectators-v0.md) for prototype-identical fields and tests.
3. **Owner-only flag bits.** Rust uses one nonowner mask, exactly the cached reference expression `2047 | 32768 | (7 << 20) | RIDING_FLAG` (`shared/sim.js:5903`). It clears gate-open, companion and cargo bits for allies and enemies. The uncached reference path's different mask (`shared/sim.js:6044`) is recorded as `differs` by the owner decision. Nonowner cooldown is 0, and target identity is retained only when visible to the recipient (`shared/sim.js:5899-5905`).

## Error and recovery behavior

As at the pin (field reference, section 6):

- A refused command gets `deny {cmd, reason}`, at most 4 per second per player; success has no reply and shows in later snapshots.
- A dropped socket keeps its seat (10 s in the lobby, the rest of the match in play) and may pause the match for up to 30 s. The client retries after 1, 2, 4, 8 and 8 s with the same token; on rejoin it gets `start` with the same `matchId`, then `pause` if paused.
- A second socket with the same token takes the seat; the old one gets `replaced` and is closed.
- A spectator more than 4 MiB behind is closed with 1013 and starts over on reconnect.

## Fixtures and verification

The first recorder and receiver replay are implemented in `harness/protocol-record.mjs`, `harness/view-check.mjs` and `simtest --view`. They cover Conquest and World Conquest messages to two human clients, with two AI seats in Conquest, movement, full and delta snapshots, held-set checks, resync and World discovery. Complete sanitized messages and browser-derived expected reports are committed as gzip fixtures. The replay compares unit rows, raw terrain-row caches and raw fog masks after each message; it does not prove every composed terrain or gameplay query.

The remaining headless-client work from issue #4 is:

1. Extend the existing recorder to every lobby setting and refusal reason, reconnect with the same token, drop pause, `leave`, spectators, companions, save and load. The current streams cover hello/lobby/start, snapshots, one refusal and resync.
2. The Rust server replays the client side of each transcript and must produce the same server messages, field for field, except where a correction above applies.
3. The pinned browser client plays a short match against the Rust server as a smoke test.

The in-process recorder does not exercise WebSocket framing, close codes, subprotocols or deflate negotiation. Those field-reference items still need transport tests. The Rust server and live browser-to-Rust tests are also pending.

## Answered on #5 (2026-10-12)

1. **JSON over WebSocket** works in the Unreal client on Linux. Keep the reference wire shapes for this phase.
2. **`proto` and `client`:** the Unreal client will send `proto: 0` and a client identifier in its next integration follow-up; the build running on 2026-10-12 does not send them yet.

## Still open (product and security decisions)

1. Spectators (correction 2): resolved to reference behavior by owner clarification J, 2026-10-16.
2. Flag bits for allies (correction 3): resolved to the cached prototype mask by owner clarification, 2026-10-16.
3. The companion's hit direction (correction 1): the proposed point 20 m along the bearing.
