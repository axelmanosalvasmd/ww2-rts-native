# Rust lobby server

This is the first standalone `ww2-server` slice. It hosts lobbies and sessions over HTTP and WebSocket. Its default mode remains lobby-only; the owner-approved trusted R1 opt-in starts canonical matches and admits the four existing commander control families. Its Rust library can construct native authored matches from a separate private map catalogue. The reference is `ww2-rts` commit `e4e579a`, with the existing JSON `t` messages and optional `hello.proto: 0` from the protocol draft.

From `rust/`:

```sh
cargo run -p ww2-server -- --bind 127.0.0.1:3000
```

The address follows `--bind`, then `WW2_BIND`, then `HOST` and `PORT`, in that order. The default is `127.0.0.1:3000`, matching the reference; exposing another address is explicit. `HOST` accepts an IP address or hostname. Invalid or out-of-range ports fail at startup. `--bind 127.0.0.1:0` chooses a free port and prints its actual address. `PUBLIC_URL` sets the invitation URL in lobby messages. The library entry point accepts an already-bound listener, a room limit (default 32, clamped to 1-256), an invitation URL, and a shutdown future. Ctrl-C closes the listener and upgraded sockets.

## Available behavior

- `GET /health` reports `status: ok`, `service: ww2-server`, `scope: lobby`.
- `GET /api/rooms` returns the reference directory fields and `Cache-Control: no-store`. Only listed rooms with a connected human seat appear.
- `/maps` lists the pinned map names, with the reference's read-only behavior for every HTTP method.
- `GET /maps/<name>` and `GET /maps/<name>.json` return the reference's `mapForClient` projection for each of the 33 maps, with `Cache-Control: no-store`. The default standalone binary embeds these projections, so it needs neither Node nor a reference checkout at runtime. Hidden mines and their hidden material rows, authoring triggers and scenario scripts are excluded. The data is a client view, not an authoritative world from which to run matches.
- Invalid map names return 400, missing maps return 404, and unsupported methods return 405 with the reference response text. Any `x-edit-password` header on a map GET returns 403. There is no credential that unlocks a raw map. Map POST returns 405 because editor writes are not implemented; the reference has a password-protected editor POST route.
- `/ws?room=CODE` accepts case-sensitive ASCII alphanumeric room codes of 3-12 characters. Invalid codes close with 1008. Each inbound message and frame is limited to 16 KiB; overflow closes with 1009. Invalid JSON, non-object messages, unknown message types, and incorrectly typed lobby fields are ignored as in the reference. Binary JSON is accepted too.
- `hello` creates or joins a room, allocates a seat or spectator slot, and sends recipient-specific lobbies. Client tokens are truncated to 40 UTF-16 code units and stay private. A missing `proto` or integer zero is accepted; another value receives `deny {cmd: "join", reason: "version"}` then close 1008. A duplicate token gets its existing identity and closes the old socket after `replaced`. The old socket cannot change the room or disconnect the replacement.
- `name`, `addAi`, `level`, `kick`, `team`, `faction`, `map`, `mode`, `worldSize`, `army`, `sandbox`, `weather`, `defender`, `spectate`, and `sit` follow reference lobby ownership and validation. Host means the first connected human seat, then the first offline human seat, then the first connected spectator if no human seat exists. A numeric `host: -1` can therefore coexist with `amHost: true` for an observer.
- Latency `ping` returns `pong` with the same `c`. Map pings are ignored in the lobby. A disconnected human keeps its seat for 10 seconds; reconnect cancels expiry. Spectators release their identity immediately. An empty room expires after 60 seconds. The state transitions accept an explicit monotonic time; production maintenance runs every 100 ms.

The lobby JSON preserves the reference fields, with `proto: 0` added. `rules` remains absent until the match handshake has a complete compatibility hash; the current FFI hash covers formations only. `result`, `hordeBest`, and `saves` remain empty because this process cannot produce them.

## Explicit limits

`start` and `restart` return `deny` with `reason: scenario`. `save` and `loadSave` return `reason: save`. These known commands return `reason: blocked`: `resync`, `fps`, `end`, `nextwave`, `pause`, `resume`, `handAi`, `leave`, and all 31 gameplay commands in `docs/parity/protocol-messages.md`. Denials are limited to four per rolling second per identity, including across reconnects. Tutorial auto-start is refused with `scenario`. The room stays in the lobby. Operative joins are refused with `deny {cmd: "join", reason: "commander"}` and close 1008.

These denials are explicit capability limits of this incomplete slice, not claims that the reference sends them in lobby state. There is no `start` envelope, snapshot stream, simulation clock, AI execution, pause, results, replay, save/load, Horde records, map editing, static browser hosting, diagnostics, sharding, bundled host integration or account authentication. Privacy decisions about in-match spectators, flag bits and companion hit direction remain untouched.

Axum's WebSocket transport does not negotiate permessage-deflate here, so messages use uncompressed text. Each client has a bounded queue of 64 outbound messages; a stalled client is closed with 1013. Those are transport limitations of the slice. Names that end exactly halfway through a UTF-16 surrogate pair are emitted with a replacement character, whereas the reference can serialize the lone surrogate. Token identity retains the original code units and is unaffected.

## Rebuilding the embedded maps

`harness/map-assets.mjs` checks that the source checkout is clean and exactly `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`, then imports that checkout's `mapForClient`. It writes client-safe map JSON, the Rust include index and the lobby catalogue together. These files remain public projections. Private authoring data lives in the separate `authoritative-maps` directory and is never selected by HTTP handlers.

```sh
node harness/map-assets.mjs
node harness/map-assets.mjs --check
```

`WW2_REFERENCE` selects an initialized reference checkout. `--check` compares generated content without writing it. Moving the reference pin remains a separate project decision.

## Verification

From the repository root, after building the server:

```sh
cargo +1.99.0 test --manifest-path rust/Cargo.toml -p ww2-server
cargo +1.99.0 clippy --manifest-path rust/Cargo.toml -p ww2-server --all-targets -- -D warnings
cargo +1.99.0 build --manifest-path rust/Cargo.toml -p ww2-server
node harness/server-lobby-check.mjs
node harness/map-delivery-check.mjs
```

`WW2_REFERENCE` may point to an initialized, read-only reference checkout. `WW2_SERVER_BINARY` may point to another built binary. The harness requires the reference's pinned `ws` 8.22.0 dependency. It launches both servers on ephemeral loopback ports and supplies test-only reference configuration to avoid creating credentials, logs or saves in that checkout.

The live harness compares 38 lobby and directory checkpoints from the two processes, normalizing only the draft `proto` addition and the deployment-specific `publicUrl`. It verifies all 33 catalogue entries against the source maps, then checks Rust version refusals, malformed-message survival, explicit start refusal and oversize closure. Rust unit tests cover stale socket isolation, reconnect expiry boundaries, observer hosting, permission checks, numeric JSON types, capacity, denial throttling and Unicode token separation. The map harness makes 96 exact HTTP comparisons, including all 33 maps with both URL forms, invalid names, missing maps, raw traversal paths, method errors and credential refusal. It also verifies reproducible embedded assets and checks legacy and layered World map projection rules against the pinned function. None of the 33 authored files is a generated World Conquest match; runtime World map generation remains outside this slice. These tests establish compatibility for the listed HTTP and lobby cases, not full protocol parity or a playable match.

Dependency APIs were checked against the official [Axum 0.8.9 WebSocket documentation](https://docs.rs/axum/0.8.9/axum/extract/ws/index.html) and [Tokio 1.53.2 TCP listener documentation](https://docs.rs/tokio/1.53.2/tokio/net/struct.TcpListener.html). crates.io metadata identified those stable releases before they were pinned. `serde_json` stays on the workspace's 1.0.154 version.

## Native authored-match construction

`authoritative_maps::load(name)` returns a validated immutable original map, and `create_match(name, options)` constructs independent canonical core state. The binary embeds all 33 original maps, including hidden mines and authoring triggers. Neither function reads files or invokes Node at runtime. Ordinary lobby mode refuses room start; the trusted R1 opt-in below uses canonical construction, runtime admission and recipient delivery before publication.

The private assets are generated separately:

```sh
WW2_REFERENCE=/path/to/pinned/ww2-rts node harness/server-authored-maps.mjs
WW2_REFERENCE=/path/to/pinned/ww2-rts node harness/server-authored-maps.mjs --check
```

HTTP `/maps` routes keep using `mapForClient` projections from their own index. Editor headers still receive 403; they cannot unlock the private native match catalogue.

## Trusted private R1 hosting

Owner decision [097](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6181065743) approves this explicit opt-in. After building the ordinary binary, run from repository root:

```sh
rust/target/ci-test/ww2-server --bind 127.0.0.1:3000 --trusted-room ABC --seed 3936341
```

The shown path uses `cargo build --locked --profile ci-test -p ww2-server` from `rust/`. Release builds use their ordinary release path. The code is case-sensitive, 3-12 ASCII alphanumeric characters. Both flags are required together; the seed accepts the full u64 range. The host serves exactly that private room code, rejects public/tutorial listing, and keeps `/api/rooms` empty. This is a controlled-host mode, not a production deployment or account authentication.

Exactly two human commander tokens on distinct teams may start. The current connected host seat starts or restarts. The room must select Conquest, Standard army and sandbox off, a committed bound map and an existing weather choice. Shuffle, tech, point supply and living weather are on. Physical logistics is temporarily off, an explicit deviation from Source network Conquest; reserve/convoy and other capability guards stay. Unsupported settings or canonical map/weather admission return existing scenario denial before a runtime, identity, account or start is published. Exact admission failures are written only to the host log. At the recorded seed, No Man's Land currently refuses each weather choice with `native startup authoring map binding changed`; its assets and binding guard are preserved.

An absent or unready retained seat does not block start or healthy restart: the new match auto-pauses using the existing drop lifecycle and resumes when that same token returns. New tokens cannot join a running room. Restart builds a fully independent canonical match, then publishes it atomically with a fresh matchId; failed preparation preserves the old runtime and authority. A latched fault stops advancement and refuses manual restart pending a separate recovery policy. No failed tick is retried and no automatic restart occurs.

Every successful publication retains a complete private option/derived-seed receipt in its runtime and logs `NATIVE_MATCH` JSON to stderr. The explicit seed is reused for each fresh identity; weather seed None uses the existing constructor derivation, whose actual result is recorded. `NATIVE_REFUSED` and `NATIVE_FAULT` are host diagnostics. Tokens and private authoring data are excluded. No seed, receipt, capability, terminal-fault field or new envelope is sent to clients. Start keeps its existing 13 fields, and ingress remains move/amove/stop/stance. These host logs are private operator output.

`tests/trusted_room_process.rs` launches the ordinary executable and uses real TCP/WebSocket clients with ClientView decoding. Its assertions cover the chosen profile, private admission, both seat controls, absent-seat start/reconnect, healthy restart and genuine authored mine-contact fault refusal. Internal tests separately compare failed preparation and old-write authority. Verification results are recorded in DESIGN; test source alone does not establish a pass or actual Unreal consumer adoption.

Set `WW2_DIAG=1` to append one JSON record per active room every five seconds to `logs/diag-<room>.jsonl`, relative to the server's working directory. Records include tick throughput and percentiles, maintenance scheduler delay, late ticks, snapshot cadence and each player's RTT, ping silence, queued UTF-8 payload bytes and clamped browser ping metrics. `WW2_TICKLOG=1` prints phase p50/p95 values every thirty seconds. Both options default off. Library hosts can set `Config.diagnostic_dir` and `Config.tick_log`. File errors or a full 64-record output queue discard diagnostic records while gameplay continues.
