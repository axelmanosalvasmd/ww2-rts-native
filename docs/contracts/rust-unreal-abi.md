# Rust to Unreal interface

Status: **v0 draft, technical answers recorded.** Author: Julio. Consumer reviewer: Axel.
Axel answered the drafts' technical questions on #5 (`axel-interface-response-004`, 2026-10-12); the answers are recorded below. Under `docs/agent-coordination.md` a comment is not an agreed interface, so the status stays "draft" until Axel approves a contract PR.
Baseline: reference ww2-rts `e4e579a`; native `main` after PR #12. The first calls below are built and verified on Linux x86_64 (branch `julio/ffi-first-calls`); the rest of this document is not built yet.

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

Link the static library with the system libraries `cargo rustc -p ww2-ffi --release --target <triple> -- --print native-static-libs` lists. For `x86_64-unknown-linux-gnu` with rustc 1.99.0 that is `-lgcc_s -lutil -lrt -lpthread -lm -ldl -lc` (measured on the Vostro, 2026-10-12). The library is built with Rust's default `panic = "unwind"`, which the panic boundary needs; a build with `panic = "abort"` would turn a caught panic into a crash.

## First calls (built 2026-10-12)

`rust/crates/ffi` (`ww2-ffi`) holds the first five functions, for the #3 bridge test. `ww2_abi_version()` returns 1 and `ww2_rules_hash()` returns `0xed0ee23d32a621d1`.

```c
uint32_t ww2_abi_version(void);
uint64_t ww2_rules_hash(void);
int32_t ww2_last_error(char *buf, size_t capacity, size_t *out_len);
int32_t ww2_formation_slot_size(uint8_t infantry, double radius, double *out);
int32_t ww2_formation_spots(const struct ww2_formation_item *items, size_t count,
                            double at_x, double at_z, double face, double reach,
                            int32_t shape, double spread,
                            struct ww2_formation_spot *out, size_t out_capacity, size_t *out_len);
```

- **Header.** cbindgen 0.29.4 writes `rust/crates/ffi/include/ww2_core.h`, which is committed with the full comments. `cargo test -p ww2-ffi --test header` fails when it is stale; `WW2_WRITE_HEADER=1 cargo test -p ww2-ffi --test header` rewrites it.
- **Status codes.** `WW2_OK` 0, `WW2_ERR_PANIC` -1, `WW2_ERR_NULL_POINTER` -2, `WW2_ERR_BAD_ARGUMENT` -3 (an unknown shape), `WW2_ERR_CAPACITY` -4. Parse, state and version errors arrive with the functions that need them.
- **Panics.** Every exported function runs inside `catch_unwind`; the library does not rely on `panic = "abort"`. A panic returns `WW2_ERR_PANIC` and `ww2_last_error` gives its message. No handle exists yet, so nothing is poisoned.
- **Memory.** `ww2_formation_spots` writes into the caller's array, at most one spot per item, so nothing needs freeing and no free function exists yet. `ww2_bytes` and `ww2_bytes_free` arrive with the first result whose size the caller cannot know in advance.
- **Arguments.** Shapes are `WW2_SHAPE_LINE` 0, `WW2_SHAPE_BLOCK` 1, `WW2_SHAPE_COLUMN` 2 and `WW2_SHAPE_WEDGE` 3, in the Formation menu's order. Booleans (`infantry`, `back`) are `uint8_t`, nonzero meaning true, so no value from C is invalid in Rust. A null pointer is an error unless its length is 0.
- **`out_len`.** The draft's signature had no output count. It needs one: the JavaScript returns no spots at all for a line or block whose average slot times `spread` is NaN, or is 0 while the drag is 0 or not finite, so the count is `count` or 0.
- **Rules hash.** FNV-1a 64 of the rules compiled into the core, which today are only the formation constants: the four shape names, then the slot minimums (3 m, 5 m), the slot margin and rank gap (1.5 m), the line rank (10), the stretch limit (3) and the comparator's centimetre steps (100). The vision rules are loaded at run time and not covered.

**Fixture.** `harness/formation-fixture.mjs` records the pinned `facingSpots`, `normalizeFace` and `slotSize` with x64 Node 24.21.0 into `rust/crates/core/tests/fixtures/formation.txt` (215 KB): 10,300 seeded cases (10,000 with 1 to 80 units, 300 with 81 to 400) stored as one hash each, plus 32 cases written out in full, among them one unit with no drag (`one`), six units with no drag and with a 30 m drag (`line.click`, `line.drag`), empty and degenerate formations, and the comparator's near-ties. The file is the same byte for byte when written on the Mac (darwin-x64 Node under Rosetta) and on the Vostro (linux-x64 Node).

**Verified on the Vostro** (Ubuntu 24.04, x86_64, rustc 1.99.0, gcc 13.3.0), from a fresh copy of `rust/`, 2026-10-12:

```
cargo build -p ww2-ffi --release --target x86_64-unknown-linux-gnu
cargo rustc -p ww2-ffi --release --target x86_64-unknown-linux-gnu -- --print native-static-libs
  note: native-static-libs: -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc
cc -std=c11 -Wall -Wextra -pedantic -I crates/ffi/include crates/ffi/examples/link_check.c \
  target/x86_64-unknown-linux-gnu/release/libww2_ffi.a -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc -o link_check
./link_check crates/core/tests/fixtures/formation.txt
  ww2_abi_version() = 1 (header WW2_ABI_VERSION 1)
  ww2_rules_hash() = ed0ee23d32a621d1
  slot sizes: infantry radius 0.5 -> 3, vehicle radius 2.2 -> 5.9000000000000004
  unknown shape 9: -3 (unknown shape 9: expected a WW2_SHAPE_* from 0 to 3)
  no room for the spot: -4 (out has room for 0 spots but there are 1 items)
  null items: -2 (items is null)
  null out_len: -2 (out_len is null)
  case one: line, spread 1, reach 0, face 0, 1 units -> 1 spots, bit-equal
           7 50 60
  ...
  32 written cases, 215 spots: 0 differ from the JavaScript
  all checks passed
cargo test -p ww2-core   # formation: 10,300 seeded and 32 written cases bit-equal; math, grid pass
cargo test -p ww2-ffi    # the header check, the error cases and the panic boundary pass
```

The C program compiles without warnings and also passes on the Mac (`aarch64-apple-darwin`, Apple clang, linked with `-lSystem -lc -lm`); the Mac stays a planned target until the plugin links the library there.

## Later slices

The client view and its readers (units, cells, fog, regions) come with slice 1c, when Unreal first talks to the Rust server. The availability, placement, entrenchment and cover queries follow the client features that need them. Each slice adds functions and raises `ww2_abi_version`; each function arrives with a fixture recorded from the reference.

## Answered on #5 (2026-10-12)

1. **Client platforms:** Linux x86_64 first, then Win64 (MSVC) and macOS arm64.
2. **Client view:** the core owns the recipient's client view behind the separate `ww2-ffi` crate. Unreal keeps derived render and UI caches, not a second decoder of gameplay state or knowledge. The client's temporary decoding and availability code is replaced feature by feature, each with fixtures.
3. **Linking:** static libraries are acceptable.
4. **Static tables:** a versioned JSON buffer at startup (`ww2_tables_json`); typed results for high-frequency queries later.
5. **Map authoring:** in the rewrite's scope, but not a prerequisite for the first bridge or gameplay slice. Its interface (IF-12) is inventoried separately and deferred until scheduled.

The memory, freeing, error, threading and panic rules under Mechanics are the intended boundary, subject to verification once the crate exists.
