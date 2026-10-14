# Three Crossroads Native

The native rebuild of Three Crossroads, the WW2 RTS in [ww2-rts](https://github.com/axelmanosalvasmd/ww2-rts): an Unreal Engine 5.8 game client and a Rust server, sharing one Rust rules core.

**Status: step 1.** The Rust core has its math, spatial grid, vision, fog, movement and crowd separation ported and checked against the JavaScript, and the first C calls for Unreal; Axel's Unreal client runs against the Node server. An internal Rust recipient view is checked against recorded browser state; its six jointly approved ABI 2 C calls became authoritative through PR #16, merged at `d88ef6d`. The approved header is unchanged.

Axel's consumer source is published in draft [PR #201](https://github.com/axelmanosalvasmd/ww2-rts-native/pull/201) at `9551d3976e9145c81030cf5a81ec7f0764b04d12`, explicitly not for merge, and is not adopted on main. Axel reports Linux build and automation checks against Rust `92fb7fe`; the earlier live and packaged-client evidence uses `03a49ac` and was not rerun on that newer library. The draft uses Rust unit rows by default but retains the C++ fallback, row tails, cells, fog and independent row parsing. Fresh live adoption, Windows/macOS client and input checks, physical mouse/keyboard tests and broader rules-query integration remain pending.

The Rust server hosts lobbies and reconnectable sessions. A trusted host can attach a supported Default or Conquest match; seated players can send Move, attack-move, Stop and Stance through the existing recorded queue. Public network start/restart, other command families and unsupported contexts remain guarded. PR #198 merged this bounded input path and the internal table/terrain providers at `514e15f4`, after exact-head native and three-platform verification and Axel's no-objection review. Table transport #199 and Unreal adoption remain separate. Step 1 is two measuring tests that decide whether the plan goes ahead:

- #1: the plan, the measured limits and the decision.
- #2: sim test, the hot path in Rust against Node at 400, 1,000 and 2,000 units.
- #3: Unreal test, a real map and 2,000 soldiers drawn from today's server, with the Rust core linked on Windows and Mac.

Diagrams of the whole plan: https://claude.ai/artifact/WVbpVy8k7zF7oQrYYp1UER

## Why

In ww2-rts, one battle's tick runs on one server thread. A heavy battle of about 400 units already uses 34 of its 50 ms at p95. The browser client draws, builds terrain and runs game logic on one main thread too. The goal is thousands of units per battle and, later, a public release. Details and sources are in #1 and `DESIGN.md`.

## How it fits together

| Part | Runs on | Language | Job |
| --- | --- | --- | --- |
| Rules core | both | Rust | The game rules: same seed, same match. No engine code. |
| `ww2-server` | the host | Rust | Runs battles, one thread per room, a battle's tick split across cores. |
| Game client | each player's PC | Unreal 5.8, C++ | Draws the world, HUD, effects and sound. Never decides outcomes. |

The client links the rules core through a small C interface for placement checks, prices and moving your own units between updates. Everything else crosses the network.

## Layout

```
reference/ww2-rts/   the JavaScript game, pinned as a git submodule (read-only here)
rust/                Cargo workspace, pinned to Rust 1.99.0
  crates/core/       rules core: no engine, networking or file-system code
  crates/simtest/    loads harness states into the core and checks it against the JavaScript
  crates/server/     HTTP/WebSocket lobby, sessions and bounded trusted native matches
harness/             Node scripts that record fixtures from the reference game (run them with x64 Node)
unreal/              Unreal 5.8 project (created by #3), binary assets in Git LFS
DESIGN.md            decisions and measurements
PARITY.md            what the JavaScript prototype does, and how the native game is measured against it
docs/parity/         the inventory by area (modes, maps, combat, server, ...)
AGENTS.md            rules for agents working here
CHANGELOG.md         every change, in plain words
```

## Getting started

```
git clone --recurse-submodules https://github.com/axelmanosalvasmd/ww2-rts-native.git
# or, in an existing clone:
git submodule update --init
git lfs install --local
```

You need Rust (stable) for `rust/`, Node.js to run the reference game and its tests, Git LFS, and Unreal Engine 5.8 only for `unreal/`.

## Relation to ww2-rts

- ww2-rts stays the playable game until the native game replaces it, and gameplay changes keep landing there.
- `reference/ww2-rts` pins the ww2-rts commit the native game is measured against. That game is a prototype: `PARITY.md` lists what it does and how the native game carries it over, improves it or drops it. Moving the pin is its own commit, together with the port of whatever rules changed.
- The plan started as axelmanosalvasmd/ww2-rts#65.
