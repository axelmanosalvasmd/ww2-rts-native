# Agent instructions

This repo rebuilds the ww2-rts game natively: a Rust rules core and server, and an Unreal Engine 5.8 client. Read `README.md` and `DESIGN.md` first. The plan and its open work are issue #1 and its sub-issues.

## Agent coordination

At task start, task boundaries, before interface changes and before integration-ready reports, read `docs/agent-coordination.md`, issue #5 and relevant contract PR replies. Acknowledge addressed requests explicitly. Consult `docs/contracts/README.md` before depending on an interface; drafts are not agreed contracts.

## Always update the changelog

Every change must be recorded in `CHANGELOG.md` in the same commit as the change.

- Add a bullet under **Unreleased** at the top: what changed, in plain words.
- Include numbers when you measured something (for example "2,000 units: 9.1 ms mean, 14.3 ms p95 per tick on the Vostro").
- Note bugs you fixed and anything you found but left for later.

## The rules core must match the JavaScript game

- `reference/ww2-rts` is the specification. From the same map, seed and orders, the Rust core must produce the same match as the pinned JavaScript. A difference is a bug in the port, unless `DESIGN.md` records it as a deliberate change.
- Never edit `reference/ww2-rts` here. Gameplay changes land in ww2-rts first. Moving the pin is its own commit, with the port of the changed rules and a passing parity run.
- Keep the core deterministic:
  - no wall-clock time and no unseeded randomness;
  - no iteration over Rust's default `HashMap` or `HashSet` where order affects results (use `BTreeMap` or sorted keys);
  - work split across threads merges in a fixed order;
  - match V8's `Math` results where the JavaScript uses them (for example `Math.hypot`), or record the difference.
- `rust/crates/core` has no engine, networking or file-system code. Unreal reaches it only through the C interface.

## Measure, don't guess

- Performance claims come with numbers: a fixed seed, the unit count, mean and p95, and the machine named.
- Time benchmarks on an idle machine: the Vostro (`ssh vostro`), or the Mac when idle.

## Unreal

- The project lives in `unreal/`. Binary assets go through Git LFS (`.gitattributes`). Editor build output (`Binaries/`, `Intermediate/`, `Saved/`, `DerivedDataCache/`) is ignored.
- Generate meshes and assets from code or from committed Python scripts where possible, so `.uasset` files are build outputs and the source of truth stays text.
- Agents drive the editor through MCP: Epic's Unreal MCP plugin (experimental in 5.8), plus a community server where the official tools fall short.
- Windows builds are made on a Windows PC.

## Tests and CI serve the goal

- When a deliberate change replaces a behavior, rewrite or delete the tests that pinned the old behavior in the same commit, and say which in `CHANGELOG.md`.
- Add tests only when they protect an improvement or a goal: roughly one focused test per stated behavior. No versioned duplicates and no one-off proof scripts in the suite.
- A flaky test (unseeded randomness, wall-clock timing) gets made deterministic or removed. Never just retried.

## Other rules

- Never use em dashes, in code comments, docs, commit messages or chat. Use a period, comma, colon or parentheses.
- `DESIGN.md` holds the decisions and the measurements. Keep it in sync.
- More than one agent session may work in this folder. Stage specific files (not whole folders) and check `git status` for changes that aren't yours before committing.
