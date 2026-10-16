# Exact lobby text proposal, not adopted

This is private review material for the remaining NET-20/29 UTF16 clauses.
The Rust package in this directory is not registered in Cargo or imported
by any server, core, FFI or client module. Its twelve controls are authored
and unrun. PR #270 and all parity statuses remain unchanged.

`exact_text.rs` retains the earlier inactive exact-unit storage, cleaner and
shape-preserving writer draft. It adds a concrete decoder for one JSON
string token. The decoder returns exact code units and a byte boundary;
it accepts literal supplementary characters, paired escapes and lone
escaped units without FFFD substitution. It rejects unescaped controls,
unknown escapes and malformed hex. The caller must still validate the
surrounding object, keys, types, delimiters and complete message. This is
not a complete inbound parser or a client decoder implementation.

The grammar readback uses [RFC 8259 section 7](https://www.rfc-editor.org/rfc/rfc8259#section-7).
Section 8.2 explicitly discusses grammar-permitted unpaired escapes and
interoperability problems. The proposed unit preservation follows the
pinned Source's JSON.parse plus UTF16 slicing, rather than assuming the
current scalar-only consumers accept those values. The ECMAScript
[JSON definitions](https://tc39.es/ecma262/multipage/structured-data.html#sec-json.parse)
were also opened during this readback. No Source, browser or native runtime
was executed.

## Complete-message parser proposal

`exact_json.rs` now consumes the complete message structure using the exact
string primitive for both keys and values. Its AST retains arbitrary lone
units even in unknown fields. Duplicate decoded keys replace the prior value
by exact code-unit equality; distinct D800/D801 keys remain distinct. Arrays,
objects, separators, trailing input and a caller-owned container-depth budget
are explicit. Numbers use the existing serde_json Number parser rather than
introducing a new numeric policy. This is not a claim of complete Source
numeric equivalence.

Scalar conversion is a separate fallible operation over every key and value.
It refuses a lone unit without FFFD substitution. The runtime must not call
that conversion on the whole message and thereby recreate the old rejection;
an adopted exact consumer needs typed routing into its exact authoritative
owners. That routing and client adoption remain unapplied. Five additional
controls cover exact fields/unknown strings, duplicate/lone keys, ordinary
scalar shape and escape boundaries, malformed whole messages and depth.
No one has compiled or executed them.

## Actual provider and consumer seams

These anchors describe private source at `40903f94`, not an adopted contract.
They are a correctness review list, not an inventory of build inputs.

| Existing seam | Current representation | Required proposal work before adoption |
| --- | --- | --- |
| `lib.rs` socket callback, `serde_json::from_slice` before Hub dispatch | Full scalar-only Value | The inactive full-message AST now covers escaped keys and duplicate-key replacement, but routing into real exact owners, malformed-message silence and byte/depth policy still need a complete adoption diff. No runtime call is replaced. |
| `lobby.rs` Person.token, token slicing in hello and trusted admission | `Vec<u16>` cut to 40 units | Keep actual D800/D801 identity. Do not round-trip tokens through scalar strings or lossy FFFD. Existing scalar input still comes through the parser above. |
| `lobby.rs` Person.name, Room.title, clean, hello and rename | Scalar String, lossy conversion after slicing | Adopt exact authoritative units at every text owner together; preserve filtering, FEFF trim, NEL retention, fallback and exact unit limits. |
| `lobby.rs` lobby players/spectators and HTTP directory | Names/titles inserted into Value strings | Preserve the same fields and JSON string types with exact encoded units and actual emitted-byte accounting. |
| `native_startup.rs` envelope, `room_runtime.rs` collect_starts, reconnect names | `Vec<String>` and `&[String]` | Carry exact names by slot into actual startup/reconnect envelopes. No invented scalar fallback or renamed player. |
| `room_runtime.rs` Pause.by and pause_message | String copied from `&str` | Retain exact initiating name while preserving host/drop reason, deadlines and control policy. |
| `lobby.rs` result_message and RoomResult.report | Names retained inside a Value | Keep exact historical names independently of later live rename and roster cleanup. Lossy Values cannot be the only retained owner. |
| `core/client_view.rs` apply, before selecting `t` | Full-message serde_json Value | A coordinated exact-string decoder must accept these messages before any view dispatch. No private-view or six-call ABI change is applied by this proposal. |
| Unreal PR #201 inspected at `e06fc433` | OnText then Unreal JSON deserialization; typed lobby/report strings | Hermes must read the actual decoder/render path at the proposed client head. The earlier source read is not current client execution or compatibility proof. |

`TextMessage` checks that each internal override targets an existing string
field. It rejects missing, wrong-type and duplicate targets. Its internal
path map never appears on the wire. It does not guarantee that a caller
remembered every name/title field, so complete provider and client fixtures
are still required. No message shape, field, endpoint, role, admission,
privacy rule or ABI is added here.

## Concrete coordination request before adoption

Root can review this code and the existing client-impact diff before asking
Hermes to review a single producer/consumer adoption proposal. That proposal
must include the full storage and typed inbound routing changes, ordinary lobby
and directory writers, start/reconnect/pause/result owners, the current
recipient decoder and the actual client handling of paired/lone units.
Byte accounting must use actual emitted UTF8 JSON bytes through the existing
queue/limit owners. Keep every ordinary literal and owner guard unchanged.

The existing client-impact draft remains at
`/tmp/ww2-net-impl-r16-b326-handoff/utf16-review/CLIENT-IMPACT.diff`, with an
explicit source baseline. It records the current decoder constraint; it is
not approval to change the wire writer alone. Exact new client-head
readback, execution grants and complete network/client proof remain pending.
No adoption or row promotion follows from this proposal.
