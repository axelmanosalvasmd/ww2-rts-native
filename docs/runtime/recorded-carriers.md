# Recorded ground carrier commands

Board and Unload use the existing internal `Command` parser, dispatcher and `OrderKey` queue. This adds no external protocol or C interface. `Unit.board` owns the current boarding target; `CombatState` owns ordered cargo and carrier identities. Accepted boarding replaces ordinary intent and cancels paid field work without refund. A full carrier leaves the paid job and wallet intact.

The unit-loop caller begins the current shared path tick, then calls `apply_unit_boarding_with_routes(id)` once. `Boarded` skips the rest of the unit turn. `NeedsPath` submits the real owner-bound Board request; `Idle` and `Cleared` continue the source loop. Deferred eligibility uses the carrier's current pose, without adding seat or ownership checks to the source queue predicate. Invalid callback facts reject before recipient observation or FIFO mutation. Valid missing, dead, nearby and moved goals retain the source discard behavior; retry-limit failure clears the canonical board target.

The pinned original JavaScript recorder delegates observed requests to the unchanged scheduler and path search. Its 19 traces and 97 actions preserve the previous 14 traces, adding immediate searches, moving deferred goals, nearby obstruction, failure drops and explicit-target semantics. Native replay parses and dispatches every command, checks represented unit/cargo/wallet/grid state after every action, and compares scheduler usage, queue counts, overflow and drops. The late-invalid guard checks the complete scheduler debug state and represented recipient memory/Fog/path state; it does not snapshot unavailable routing internals. An arrival-permutation regression uses the same recorded command queue and actual source Dig-success/Board-full receipts.

Compiled head: `45e06075b6216e7bd65078ecfa0849e22613e02c`. Vostro Linux x86_64 passed twelve focused debug tests (five carriers, three commands, two engineering and two primitive field-work tests), all 211 release workspace tests, strict workspace/all-target Clippy, formatting and release FFI. Both profiles were cleaned for all four workspace packages before compilation; the raw log confirms fresh core compilation from the archived current source. The first attempt at `a1de286` stopped on a private-field test read before any tests ran. The test-only owner accessor fixed that compile error without changing runtime visibility.

```text
archive SHA-256: 01e39e1781e589866372d01be87b91230baac90965f1c21d75773efaca861d93
raw log SHA-256: a0e614285d7068116ee352543b130a235bc6edf155f2fe17d663195797deb281
ALL_GATES_PASS HEAD=45e06075b6216e7bd65078ecfa0849e22613e02c
```

The native cycle ran `cargo test --locked --offline -p ww2-core --lib match_carriers`, the command and engineering library filters, the primitive `field_works` integration test, the full release workspace suite, strict workspace Clippy, `cargo fmt --all --check`, and the release FFI build. Source regeneration uses x64 Node 24.21.0 with `WW2_REFERENCE` pointing to the unchanged `e4e579a` checkout, then `node harness/carriers-fixture.mjs --check`.

The published carrier component ancestry is merged normally, with only documentation changes relative to the compiled head. Full Match tick attachment, boats/shore landing, rail, garrison exits, unrepresented building/entrench jobs and wrecked-carrier ejection remain separate. Isolated mass-project, Cover and aircraft command proposals require later coordinated unions. No reference pin, wire, C ABI or privacy policy changes. Fresh Ubuntu, macOS and Windows hosted checks remain required before merge; local Vostro evidence does not replace them.
