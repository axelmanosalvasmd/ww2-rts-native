# Rust to Unreal interface

Status: **v0 draft, technical answers recorded.** Author: Julio. Consumer reviewer: Axel.
Axel answered the drafts' technical questions on #5 (`axel-interface-response-004`, 2026-10-12); the answers are recorded below. Under `docs/agent-coordination.md` a comment is not an agreed interface, so the status stays "draft" until Axel approves a contract PR.
Baseline: reference ww2-rts `e4e579a`; native `main` after PR #7. No Rust code exists yet; nothing here is verified by a build.

## What the client needs from the core

[`docs/parity/client-imports.md`](../parity/client-imports.md) lists every client import from `shared/` at the pin: 86 names in 40 client files, 56 of them from `shared/sim.js`, plus the rules the browser client copies instead of importing. [`docs/parity/interface.md`](../parity/interface.md) groups them (IF-01 to IF-14):

| Group | Examples | Rows |
| --- | --- | --- |
| Static tables and constants | `UNITS`, `CFG`, `SUPPORT`, `FORTS`, `ENTRENCH`, `TECH`, `WEATHER`, `SLOTS`, `CELL`, `TICK` | IF-01 |
| Decoding server messages | unit row columns and flag bits, fog runs, cell state bits, the held-set check | IF-02 |
| Availability, prices, population | `priceOf`, `popCap`, `popUse`, `canBuild`, `buildKinds`, `productionAccess`, `techCost` | IF-03 |
| Previews | `placementCheck`, `entrenchPlan`, `segmentCost`, cover | IF-04, IF-05, IF-07 |
| Orders | `facingSpots` formation spots | IF-06 |
| Map presentation | `levelOf`, `worldLayers`, `roadMask`, `territoryEdges`, `insideKnownRegion`, house kinds | IF-08 |
| Soldiers and motion | squad-man slots, companion prediction, vehicle pose, shell flight | IF-09 to IF-11 |
| Authoring | `validateMap`, `createGame`, `playMatch` (map editor and fairness check) | IF-12, only if an Unreal editor is in scope |

## The core owns what the client knows

Issue #4 asks for one canonical decoder and knowledge state rather than two that drift. Axel accepted this design on #5: the core provides a **client view**, a handle that holds exactly what one recipient has been told.

- Unreal passes every server message, as received, to the view. The view decodes it with the same code the Rust server uses to encode it.
- Unreal reads what it draws from the view: units, changed terrain cells, fog, regions, resources.
- Every rules query in the table above runs on the view. The view can only contain what the server sent, so no query can reach hidden state (IF-14), and the client links no server code.

With the browser client, Unreal and the Rust server sharing one decoder, a protocol change is made once.

## Mechanics

- **Crates.** `rust/crates/core` stays free of engine, network, file-system and unsafe code. A separate crate, proposed name `rust/crates/ffi` (`ww2-ffi`), wraps it in `extern "C"` functions and owns every unsafe block and the panic boundary. Issue #4 asks for this boundary to be decided here rather than assumed; this is the proposal.
- **Header.** cbindgen generates `ww2_core.h` from `ww2-ffi`. The header is committed, and CI fails when it is stale, so Unreal builds without a Rust toolchain.
- **Names.** Every symbol starts with `ww2_`; types are `ww2_*` structs with fixed layout (`#[repr(C)]`).
- **Errors.** Every function returns `int32_t`: 0 for success, a negative `WW2_ERR_*` code otherwise (bad argument, parse failure, wrong state, version mismatch, panic). Results go through out-parameters. `ww2_last_error` copies a readable message for the calling thread.
- **Panics.** Every entry point runs inside `catch_unwind`. A panic returns `WW2_ERR_PANIC` and marks the handle it touched as poisoned; a poisoned handle accepts only `ww2_view_free`. Nothing unwinds into C++.
- **Memory.** Fixed-size results go into caller structs. Variable-size results come back as `ww2_bytes { ptr, len }` owned by Rust and released with `ww2_bytes_free`, or fill a caller array after a call that reports the needed count. Inputs are pointer and length pairs; strings are UTF-8 with explicit lengths, never NUL-terminated.
- **Handles.** `ww2_view *` is opaque, created by `ww2_view_new` and released by `ww2_view_free`. One thread uses a handle at a time (the game thread, or a worker that owns it). Functions on different handles may run on different threads. The core starts no threads and does no I/O.
- **Determinism.** Previews must agree with the server, so the client runs the same core with the same math module as the server and gets the same bits on Windows, macOS and Linux (`AGENTS.md`, `DESIGN.md`).
- **Versions.** `ww2_abi_version()` returns an integer that changes whenever a signature or struct layout changes; the header carries the same number as `WW2_ABI_VERSION`, and the plugin refuses to start on a mismatch. `ww2_rules_hash()` returns a hash of the rules data compiled into the core; the Rust server sends the same hash in `lobby.rules` (protocol v0), so a client built against different rules can warn instead of drawing wrong previews.
- **Units.** All values are in the simulation's units and axes: metres, `x` east, `z` south, height up, seconds. Conversion to Unreal space happens in one place on the Unreal side (`terrain.md`).

## Artifacts

| Target | Triple | Artifact | Built on |
| --- | --- | --- | --- |
| Windows client | `x86_64-pc-windows-msvc` | `ww2_ffi.lib` (static) | the Windows PC |
| Mac client | `aarch64-apple-darwin` | `libww2_ffi.a` (static) | the Mac |
| Linux client | `x86_64-unknown-linux-gnu` | `libww2_ffi.a` (static) | any Linux machine (the Vostro) |
| Server | `x86_64-unknown-linux-gnu`, plus the client targets | `ww2-server` binary (no FFI) | any |

Build: `cargo build -p ww2-ffi --release --target <triple>`. Julio owns these artifacts; Axel owns linking them into the Unreal plugin and packaging (issue #4). Linux x86_64 is the first verified client target, because Axel develops and tests the client on Linux; Windows and Mac are planned and not yet proven.

## First calls, for the #3 bridge test

1. `ww2_abi_version()` and `ww2_rules_hash()`: prove the library links and loads on each client platform, Linux first.
2. `ww2_formation_spots`: a pure function with real float math and a variable-size result, so it exercises the math module and the buffer rules. Input: a formation request (unit count, slot size, line start and end, facing). The expected output comes from the pinned JavaScript `facingSpots` for the same input, recorded as a fixture, so the Unreal result is checked against the reference, not against itself.

## Later slices

The client view and its readers (units, cells, fog, regions) come with slice 1c, when Unreal first talks to the Rust server. The availability, placement, entrenchment and cover queries follow the client features that need them. Each slice adds functions and raises `ww2_abi_version`; each function arrives with a fixture recorded from the reference.

## Answered on #5 (2026-10-12)

1. **Client platforms:** Linux x86_64 first, then Win64 (MSVC) and macOS arm64.
2. **Client view:** the core owns the recipient's client view behind the separate `ww2-ffi` crate. Unreal keeps derived render and UI caches, not a second decoder of gameplay state or knowledge. The client's temporary decoding and availability code is replaced feature by feature, each with fixtures.
3. **Linking:** static libraries are acceptable.
4. **Static tables:** a versioned JSON buffer at startup (`ww2_tables_json`); typed results for high-frequency queries later.
5. **Map authoring:** in the rewrite's scope, but not a prerequisite for the first bridge or gameplay slice. Its interface (IF-12) is inventoried separately and deferred until scheduled.

The memory, freeing, error, threading and panic rules under Mechanics are the intended boundary, subject to verification once the crate exists.
