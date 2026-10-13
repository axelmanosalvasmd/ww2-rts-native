# Rust lobby server

This is the first standalone `ww2-server` slice. It hosts lobbies and sessions over HTTP and WebSocket. It does not start or simulate matches. The reference is `ww2-rts` commit `e4e579a`, with the existing JSON `t` messages and optional `hello.proto: 0` from the protocol draft.

From `rust/`:

```sh
cargo run -p ww2-server -- --bind 127.0.0.1:8080
```

`WW2_BIND` also sets the address. The default is `127.0.0.1:8080`; exposing another address is explicit. `--bind 127.0.0.1:0` chooses a free port and prints its actual address. `PUBLIC_URL` sets the invitation URL in lobby messages. The library entry point accepts an already-bound listener, a room limit (default 32, clamped to 1-256), an invitation URL, and a shutdown future. Ctrl-C closes the listener and upgraded sockets.

## Available behavior

- `GET /health` reports `status: ok`, `service: ww2-server`, `scope: lobby`.
- `GET /api/rooms` returns the reference directory fields and `Cache-Control: no-store`. Only listed rooms with a connected human seat appear.
- `GET /maps` lists the pinned map names. The committed catalogue contains spawn counts and Horde eligibility for all 33 maps. It is metadata, not the map contents.
- `/ws?room=CODE` accepts case-sensitive ASCII alphanumeric room codes of 3-12 characters. Invalid codes close with 1008. Each inbound message and frame is limited to 16 KiB; overflow closes with 1009. Invalid JSON, non-object messages, unknown message types, and incorrectly typed lobby fields are ignored as in the reference. Binary JSON is accepted too.
- `hello` creates or joins a room, allocates a seat or spectator slot, and sends recipient-specific lobbies. Client tokens are truncated to 40 UTF-16 code units and stay private. A missing `proto` or integer zero is accepted; another value receives `deny {cmd: "join", reason: "version"}` then close 1008. A duplicate token gets its existing identity and closes the old socket after `replaced`. The old socket cannot change the room or disconnect the replacement.
- `name`, `addAi`, `level`, `kick`, `team`, `faction`, `map`, `mode`, `worldSize`, `army`, `sandbox`, `weather`, `defender`, `spectate`, and `sit` follow reference lobby ownership and validation. Host means the first connected human seat, then the first offline human seat, then the first connected spectator if no human seat exists. A numeric `host: -1` can therefore coexist with `amHost: true` for an observer.
- Latency `ping` returns `pong` with the same `c`. Map pings are ignored in the lobby. A disconnected human keeps its seat for 10 seconds; reconnect cancels expiry. Spectators release their identity immediately. An empty room expires after 60 seconds. The state transitions accept an explicit monotonic time; production maintenance runs every 100 ms.

The lobby JSON preserves the reference fields, with `proto: 0` added. `rules` is absent because no rules core is linked. `result`, `hordeBest`, and `saves` remain empty because this process cannot produce them.

## Explicit limits

`start` and `restart` return `deny` with `reason: scenario`. `save` and `loadSave` return `reason: save`. These known commands return `reason: blocked`: `resync`, `fps`, `end`, `nextwave`, `pause`, `resume`, `handAi`, `leave`, and all 31 gameplay commands in `docs/parity/protocol-messages.md`. Denials are limited to four per rolling second per identity, including across reconnects. Tutorial auto-start is refused with `scenario`. The room stays in the lobby. Operative joins are refused with `deny {cmd: "join", reason: "commander"}` and close 1008.

These denials are explicit capability limits of this incomplete slice, not claims that the reference sends them in lobby state. There is no `start` envelope, snapshot stream, simulation clock, AI execution, pause, results, replay, save/load, Horde records, map fetch/edit, static browser hosting, diagnostics, sharding, bundled host integration or account authentication. Privacy decisions about in-match spectators, flag bits and companion hit direction remain untouched.

Axum's WebSocket transport does not negotiate permessage-deflate here, so messages use uncompressed text. Each client has a bounded queue of 64 outbound messages; a stalled client is closed with 1013. Those are transport limitations of the slice. Names that end exactly halfway through a UTF-16 surrogate pair are emitted with a replacement character, whereas the reference can serialize the lone surrogate. Token identity retains the original code units and is unaffected.

## Verification

From the repository root, after building the server:

```sh
cargo +1.99.0 test --manifest-path rust/Cargo.toml -p ww2-server
cargo +1.99.0 clippy --manifest-path rust/Cargo.toml -p ww2-server --all-targets -- -D warnings
cargo +1.99.0 build --manifest-path rust/Cargo.toml -p ww2-server
node harness/server-lobby-check.mjs
```

`WW2_REFERENCE` may point to an initialized, read-only reference checkout. `WW2_SERVER_BINARY` may point to another built binary. The harness requires the reference's pinned `ws` 8.22.0 dependency. It launches both servers on ephemeral loopback ports and supplies test-only reference configuration to avoid creating credentials, logs or saves in that checkout.

The live harness compares 38 lobby and directory checkpoints from the two processes, normalizing only the draft `proto` addition and the deployment-specific `publicUrl`. It verifies all 33 catalogue entries against the source maps, then checks Rust version refusals, malformed-message survival, explicit start refusal and oversize closure. Rust unit tests cover stale socket isolation, reconnect expiry boundaries, observer hosting, permission checks, numeric JSON types, capacity, denial throttling and Unicode token separation. This establishes lobby compatibility for these cases, not full protocol parity or a playable match.

Dependency APIs were checked against the official [Axum 0.8.9 WebSocket documentation](https://docs.rs/axum/0.8.9/axum/extract/ws/index.html) and [Tokio 1.53.2 TCP listener documentation](https://docs.rs/tokio/1.53.2/tokio/net/struct.TcpListener.html). crates.io metadata identified those stable releases before they were pinned. `serde_json` stays on the workspace's 1.0.154 version.
