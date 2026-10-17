# Final parity integration, 2026-10-17

Base: `61947c5b8a8438eb6ccbefa02e7d269c4e032472`.
All merges retain exact parent commits with no-ff ancestry. No squash, rebase or
force-push. World admission remains closed and the reference pin is unchanged.
Final combined validation and three-platform main CI are pending.

## Chain 1: AI

Merged exact `bc30b71012ba3c4ce25f4237128d337e03849e24` automatically. The handoff
records 1,994 passing tests plus formatting and strict Clippy for this chain.
That historical chain result does not qualify the final combined tree.

## Chain 2: rooms

Merged exact `02c048f1be4ee7ae48b93369625063e5a732051a`, whose qualified Rust tree
has 1,918 passing enabled tests. Manual resolutions:

- Global lists retain the broader represented-ordinary-mode guard and use the
  shared Classic-node validator/number conversion, removing only the redundant
  older inline node construction. Wreck identity validation remains.
- Snapshot validation retains Tutorial, Assault and elimination-specific checks,
  and keeps exactly one Classic-only wallet insertion. Other ordinary modes do
  not acquire undefined prototype wallets.
- Save-string diagnostics keep the incoming escaped/debug formatting.
- Poll connectivity uses AI-aware Person::online and still appends the internal
  Tutorial enemy's connected slot. Public start retains its own roster/mode
  guards rather than accidentally inheriting a displaced trusted-start block.
- The async trusted-start owner now carries the AI branch's human-token checks:
  AI seats are allowed, human tokens remain required and distinct, teams remain
  opposed, and host/readiness/restart guards remain. Sandbox admission from the
  rooms chain is not reverted to the older AI branch's restriction.

No test or registration is intentionally removed. The later save merge carries
the separately qualified trusted addAi two-seat bound. Final combined tests are
still pending.

## Chain 3: logistics

Merged exact `5e6966ae4cdad75ced8d1aba9ff745d2c2825186`, whose qualified Rust tree
passed 1,945 enabled tests. Composition preserves both complete test modules
where AI and logistics independently added files with the same path:

- The AI chain's terrain-aware CommanderViewMemory lives in commander_view.rs;
  the hosted Horde chain's detached CommanderMemory remains in commander.rs.
- Ordinary rooms retain their delivered-snapshot AI owner, including saved
  planner memory. Horde retains its hosted owner and internal enemy schedule.
  Explicit runtime guards prevent configuring both owners on one runtime.
  Both test suites remain registered. Seat handover resets the active owner.
- The shared tick batch generation retains the Horde alert-cursor reset, while
  ordinary AI still observes the actual selected start/frame packets.
- Async trusted startup now carries ordinary mode, Assault defender and loaded
  feature flags into the canonical constructor. Its host, readiness, token,
  restart and cancellation guards remain; Horde keeps its dedicated reread path.
- Tutorial and Horde internal slots are appended exactly once. Away remains
  socket absence and non-AI; auto-pause readiness is unchanged.
- Recipient validation composes Tutorial, elimination, Assault and Horde guards.
  Classic nodes have one validated producer, with existing zero normalization.
  The malformed synthetic Horde supply witness retains UnsupportedMode from
  the common actor/finish admission guard, with all atomicity assertions intact.
- Queued aircraft retain the transactional aircraft publication helper; ground
  births retain the incoming logistics reserves and producer accounting. The
  redundant unreachable inline aircraft path is not duplicated.

These are source-composition results, not a qualification of the combined tree.

## Chain 4: save (composition in progress)

Exact parent: `82f25be388cbbebfd5a773a13e5825906c539924`, qualified at 1,959 enabled
tests. The additive C ABI revisions, authored scenarios, editor persistence and
Horde records are retained. Final combined qualification is pending.

The save chain added a third ordinary AI scheduler at the same path as the
already-integrated hosted Horde adapter. Running both would submit duplicate
orders. The merged ordinary room therefore keeps the AI chain's actual delivered
frame owner and codec. The save chain's inactive human helper state remains a
separate room helper owner, reset on seated takeover and restart. Its core tagged
AI codec remains available and tested independently. The ordinary room does not
silently serialize a second inactive planner as if it were the active AI.

The reset/restore tests are adapted to inspect the active runtime AI commander
and its full delivered-view memory rather than the replaced third scheduler;
human helper identity, order preservation, atomic failed load, spectator starts,
strategic memory, fresh attention and restart assertions remain. The existing
AI-chain codec roundtrip and whole-save clone-failure witnesses remain.

Additional composition:
- Preserve both independently added engineering suites under separate paths.
- Combine field-work and retreat snapshot checks with complete-frame arrays and
  every incoming own-unit, hidden-unit and work-row assertion. The deliberately
  superseded partial-snapshot missing-capability lists become complete delivery.
- Resource nodes retain richer position/rate/fuel/visibility rows; the incoming
  explicit node-worth compatibility field and its finite validation remain.
  Aircraft readiness uses one common air_at_base field in both implementations.
- Preserve typed authored-scenario events and reject the decode-only legacy
  authored-mission event rather than emitting it. Tutorial and Horde event
  validation remain present.
- Authored editor maps are captured for async generation; public and trusted
  startup bind to the same private map source. World admission stays closed.

The four-chain diagnostic `cargo check --locked --workspace --all-targets -j1`
passed after source composition. This compiles tests but does not run them.
Final tests, Clippy, FFI linking and three-platform CI remain pending.

Pinned-reference zero-form check: unmodified `shared/sim.js:5249` creates the
plane-arrival object with `dir: s.dir`; `server.js:195` and the seat snapshot path
at line 246 use JSON.stringify. Executing that exact emission statement and send
helper on Node v24.19.0 with controlled +0 and -0 both emits `"dir":0`.
This is a bounded serialization component check, not a full support simulation.
