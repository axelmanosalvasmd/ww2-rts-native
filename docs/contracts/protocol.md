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

The reference leaks some information to clients that should not have it. The proposals below correct these without changing message shapes, so the browser client would keep working. Axel keeps all three pending (#5): each lands only as its own documented security change, with browser and Unreal fixtures, once both owners decide it. They block no other work, and until then the Rust server behaves as the reference does.

1. **The companion's hit direction.** At the pin, `operative.from` sends the hidden attacker's position to the metre for 10 ticks after a hit (NET and FPS limitations). The browser client uses only its bearing (`client/operative-view.js:342`). The Rust server sends a point 20 m from the soldier along that bearing instead: the hit indicator is unchanged and the attacker's range stays hidden.
2. **Spectators outside World Conquest.** At the pin, anyone who joins a running match with a new token becomes a spectator and sees the whole map, so a player can watch the enemy from a second tab. Tokens are free to make, so no rule based on who joined when closes this. Options for Axel: (a) an observer delay: spectator snapshots run a fixed time behind the match (for example 60 s), which the browser client handles unchanged; (b) spectators get seat 0's team view, as in World Conquest; (c) keep the reference behavior until accounts exist. Julio proposes (a).
3. **Owner-only flag bits.** For rows the receiver does not own, the reference's cached snapshot path clears the gate-open, companion and cargo bits (`shared/sim.js:5903`), while its uncached path keeps them (`:6044`). The Rust server applies one mask on every path; which of those bits allies may see is a question for Axel.

## Error and recovery behavior

As at the pin (field reference, section 6):

- A refused command gets `deny {cmd, reason}`, at most 4 per second per player; success has no reply and shows in later snapshots.
- A dropped socket keeps its seat (10 s in the lobby, the rest of the match in play) and may pause the match for up to 30 s. The client retries after 1, 2, 4, 8 and 8 s with the same token; on rejoin it gets `start` with the same `matchId`, then `pause` if paused.
- A second socket with the same token takes the seat; the old one gets `replaced` and is closed.
- A spectator more than 4 MiB behind is closed with 1013 and starts over on reconnect.

## Fixtures and verification

Pending. The plan, part of the headless test client issue #4 asks for:

1. A recorder in this repo runs the pinned Node server in process, as the reference tests do, drives scripted clients, and writes sanitized transcripts (one JSON object per line, tokens replaced) for: hello and lobby, every lobby setting, start, 200 ticks of snapshots, a refused command of each kind, `resync`, reconnect with the same token, a drop pause, `leave`, a spectator, a companion, save and load.
2. The Rust server replays the client side of each transcript and must produce the same server messages, field for field, except where a correction above applies.
3. The pinned browser client plays a short match against the Rust server as a smoke test.

The transcripts also settle the field reference's unverified items (close codes, subprotocols, deflate parameters).

## Answered on #5 (2026-10-12)

1. **JSON over WebSocket** works in the Unreal client on Linux. Keep the reference wire shapes for this phase.
2. **`proto` and `client`:** the Unreal client will send `proto: 0` and a client identifier in its next integration follow-up; the build running on 2026-10-12 does not send them yet.

## Still open (product and security decisions)

1. Spectators (correction 2): observer delay, seat 0's team view, or the reference behavior.
2. Flag bits for allies (correction 3): whether teammates see open gates, companions and carried squads.
3. The companion's hit direction (correction 1): the proposed point 20 m along the bearing.
