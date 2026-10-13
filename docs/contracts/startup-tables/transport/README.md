# Opt-in network delivery of startup tables

Status: **transport proposal, unagreed and unimplemented**. Neither owner has approved these wire additions. Main baseline is `4867984d4ca50d18db7a34a80fc27be387f6d299`, reference `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Content/schema v1 and separate identity values were accepted in [Axel's content response](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6156314777), which explicitly left delivery and consumption pending. The isolated internal producer successor is `b68289136e0572fdc0fcbc705514a509083c167b`; its predecessor's Mac byte correspondence is recorded, while exact successor/platform gates remain separate.

This draft proposes a client opt-in field and one server message over the existing socket. The authoritative host supplies the exact core-produced JSON bytes, so reconnect can verify that host's full table identity. No new HTTP route, table asset, local table copy, FFI call or ABI/header change is proposed. The six ABI 2 view calls remain unchanged. Publication of this draft does not authorize implementation or adoption.

## Actual source and the choice

The current header declares ABI 2, formation calls and six recipient-view calls. It has no `ww2_tables_json`, `ww2_bytes` or `ww2_bytes_free`. `ww2_rules_hash()` covers formation only. The broader ABI document describes future variable-result ownership, but those types/functions do not exist. FFI builds are static libraries (`ffi/Cargo.toml`); cbindgen and `ffi/tests/header.rs` enforce the committed header. Current CI builds the library on three operating systems and runs C linkage on Linux. There is no Unreal source tree or published consumer source in this checkout.

The current server sends JSON objects through a bounded 64-item per-connection output channel and serializes each into one text frame (`server/src/lobby.rs:9-33`, `lib.rs:171-188`). Inbound message/frame limits are 16 KiB (`lib.rs:35,126-127`). That inbound limit is not a table output allowance. Actual lobby output has no full-table identity; `protocol.md`'s proposed `lobby.rules` is not implemented and is formation-only. A local FFI table buffer by itself therefore cannot detect the authoritative host changing full table content.

The material alternative is local FFI delivery plus separate host identity wiring or deployment identity distribution. It would need an ABI 3 transition, a new owned-buffer result and matching Rust free, as well as a host identity mechanism. This draft selects socket delivery because one negotiated envelope supplies both authoritative bytes and their identity. It does not adopt the FFI alternative or change the broader draft's future memory rules.

## Proposed hello and envelope

A consumer opting into this feature sends the existing hello fields plus the integer `tablesVersion: 1`:

```json
{"t":"hello","token":"same-existing-token","name":"Soldier","proto":0,"tablesVersion":1}
```

[`hello.schema.json`](hello.schema.json) defines this opt-in profile. Existing hello fields and unknown-field handling stay as they are; this schema does not rewrite token truncation, roster, role or capability admission. An absent `tablesVersion` selects existing v0 behavior and receives no tables message. A present unsupported value or wrong type receives existing `deny {"cmd":"join","reason":"version"}` and close 1008 before seat/account publication. This opt-in extends the v0 envelope and requires explicit owner approval despite preserving `proto: 0`.

For an accepted opt-in, the server sends exactly one tables object on each connection's hello, including same-token reconnect:

```json
{
  "t": "tables",
  "schemaVersion": 1,
  "contentSha256": "5de90a7990ec5e8a2903436ac4809302464be29b546d79591a2adc08c68a2efd",
  "json": "exact core-produced UTF-8 JSON text, including its one final LF"
}
```

The abbreviated `json` above illustrates the field type, not valid table content. [`example-tables.json`](example-tables.json) contains the complete accepted static payload inside the exact proposed envelope. It is a generated sanitized review fixture, not an emitted server or Unreal trace. [`tables.schema.json`](tables.schema.json) specifies the closed envelope shape. The message contains no recipient, seed, match identity, live actor or private map data.

`schemaVersion` is the core producer's data version and must equal the inner payload's `schemaVersion`. `contentSha256` is computed from that producer's actual bytes, never substituted with a hard-coded expected hash. The outer `json` field is a JSON string, not a parsed nested table object. UTF-8 encoding that decoded string must reproduce the producer bytes exactly. Reformatting inner JSON, changing line endings, stripping the LF or normalizing text is forbidden.

Hash scope is the complete decoded inner UTF-8 bytes, including the final LF. It excludes the outer object's fields, outer quotes/escapes, file-format LF, websocket framing and compression. The lowercase 64-hex hash is outside the content it hashes, as agreed. The producer's formation hash remains lowercase 16-hex `ed0ee23d32a621d1` and is checked separately.

## Bounds and order

Proposed decoded table content cap: 1,048,576 UTF-8 bytes. Proposed complete tables text-frame cap: 2,097,664 UTF-8 bytes (twice that cap plus 512 bytes for the fixed envelope). The envelope schema's string `maxLength` counts characters; the receiver must additionally enforce these byte caps before copying/parsing the inner payload. Hello keeps the existing 16 KiB inbound limit. This proposes no increase to gameplay input limits. The present inner content is 30,618 bytes; the complete example frame is 35,948 bytes.

The server must prepare and validate the core buffer and hash before publishing the opted-in connection's seat/account replacement or enqueueing its first lobby/start/s. It queues tables first on the same ordered output channel. Existing lobby/start/pause/resume ordering follows afterward; this extension does not reorder those messages relative to each other. JSON ping/pong and websocket control traffic may operate while the table gate is pending. A producer, bound or enqueue failure refuses the opted-in join, closes the connection, and must not continue a partial handshake or leave the connection eligible for snapshots. Runtime/backend failure uses close 1011; stalled output retains the existing close 1013 path. Existing policy denials remain existing policy denials.

The consumer routes tables through its one static parser/cache module before processing any lobby, start or snapshot. It publishes the immutable cache only after all checks succeed, then continues the ordinary ordered receiver path. Parsing must finish before dispatching the next game message; no unbounded pre-ready queue or timeout-as-success is allowed. After static verification, pass the original tables envelope through the unchanged recipient bridge before declaring ready. Current ClientView returns `Applied::Ignored` for unknown string message types, so this must preserve the view; a bridge error refuses readiness. No new decoder semantics are introduced. Lobby/start/s arriving first is a missing-table-handshake refusal, which also prevents an old server that ignores the opt-in field from silently proceeding.

No table acknowledgment message is proposed. Server-side enqueue order and client-side synchronous acceptance are the gate. Socket delivery success is not proof of consumer adoption.

## Expected identities and linked core compatibility

The consumer release has a reviewed identity record, generated from the accepted producer proof: expected schema version, full-content SHA256, formation hash, producer source head and linked static-library artifact/source identity. This record contains identities only, not a second JSON table asset or copied rules. Axel's published consumer build must show how its private build configuration receives that record; no such build step exists in this checkout today.

Before accepting a payload, compare the envelope version and hash with that release record. Then hash the exact decoded bytes and compare them with both the release expectation and envelope hash. Only then parse and validate the accepted content schema and all four number tags. Check inner/outer versions and the `formation.hash` value as well. Never trust a sender's self-reported hash as the sole expectation.

Separately require linked library ABI, header and wrapper to agree on 2 before bridge calls, and require payload `formation.hash` to equal runtime `ww2_rules_hash()`. The unchanged six-call ClientView decoder consumes the agreed v0 messages and does not derive rules from a local Catalog. It retains sole recipient row/knowledge ownership; `UNIT_ROW` and flag metadata in the cache cannot become a second Unreal decoder. Formation previews use the canonical linked Rust formation calls, not remote-table formulas.

ABI 2 has no runtime full-catalog hash accessor. The library/header check plus formation hash is therefore not sufficient proof for future Catalog-dependent queries. Current adoption must include the static library's exact build/link artifact identity in the reviewed release pairing. Any later rules/view-query interface must explicitly define and verify its compatibility with the remote table identity before use. This draft does not enable those queries or broaden the six calls.

## Reconnect, ownership and refusal

Every reconnect requests tables again before any current-state start. It must re-check the actual host version/hash and link compatibility. The first accepted cache pins the session's version and content hash. Identical reconnect content can reuse that immutable cache after complete verification. Changed content, even if otherwise supported by the parser, refuses the session; it does not hot-reload or replace the cache. A separately reviewed new session/release may establish another expected identity. An unsolicited tables message during an already-ready connection is a refusal rather than an update command.

Rust owns the canonical immutable producer bytes. The server constructs the envelope directly from those bytes without independently serializing tables. The socket owns its outbound frame until write completion. Unreal copies the decoded UTF-8 bytes into its own temporary validation storage, builds its immutable cache atomically, then releases temporary network/parse storage using its own allocator. No Rust pointer crosses this mechanism, so no C allocation/free contract is added. The core performs no I/O and starts no threads; network lifetime and the game-thread publication boundary remain adapter responsibilities.

Unknown version, expected/envelope/computed hash mismatch, schema failure, malformed tag, size violation, missing handshake or changed reconnect identity is a hard refusal. No partial cache, Blueprint/local fallback, decoder start or match is permitted. The consumer logs reason, expected/received version, expected/envelope/computed hash, byte count and linked ABI/formation identity; unavailable fields use `missing` or `invalid`. Logs contain no token, name, room transcript, seed or private state. A consumer refusal closes with 1008 and enters a terminal incompatible-session state instead of an automatic reconnect loop. Connection drop handling remains the existing server policy.

Server producer failures are logged against its source identity and validation reason. The future implementation must retain existing Rust panic boundaries; this proposal adds no C entry point, changes no poison rule and supplies no successful substitute data after a producer error.

## Concrete approval and proof

Both owners must approve the hello field, envelope, limits, exact-byte hash scope, release-expectation source, linked-core compatibility gate, startup order, reconnect behavior and refusal policy before active server or consumer changes. Content/schema approval alone and publication authorization do not approve these wire changes.

After approval, Rust owns the one producer, metadata/hash calculation, opt-in server admission and ordered delivery. Axel owns the Unreal parser/cache, refusal logs, release identity record and actual RTS/HUD reads. Linux x86_64 comes first at one published producer/server head and one published consumer head: complete Node-versus-Rust field/IEEE754 equality, exact core-to-frame-to-cache bytes/hash, fresh startup, same-token mid-match and paused reconnect, unchanged content reuse and changed content refusal, all error cases, and proof the renderer/HUD reads that cache. Then repeat byte/hash and cache/refusal proof on Win64 MSVC and macOS arm64, with packaged acceptance tracked separately. Decoder-only tests, historical Editor reports or metadata alone do not count as adoption.

The source comparison must show the new table gate precedes the existing initial resume/play-lobby/start and reconnect start/pause/drop-resume/later-lobby sequences, without changing ordinary v0 snapshot fields, recipient filtering or event ordering. Legacy no-opt-in browser/headless sockets must retain their complete existing traces. Existing unsupported roles, public start/restart, editor/structure capabilities and privacy questions remain guarded.

To reproduce the complete draft example, run `python3 docs/contracts/startup-tables/transport/generate-example.py --check` from the repository root. This checks envelope byte restoration and hash identity without Rust execution or any transport implementation.
