# Three Crossroads Native

The native rebuild of Three Crossroads, the WW2 RTS in [ww2-rts](https://github.com/axelmanosalvasmd/ww2-rts): an Unreal Engine 5.8 game client and a Rust server, sharing one Rust rules core.

**Status: planning.** No game code exists yet. Step 1 is two measuring tests that decide whether the plan goes ahead:

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
  crates/server/     ww2-server (later)
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
