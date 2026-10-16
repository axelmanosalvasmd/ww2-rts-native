# IF-RUNTIME-3 scope and qualification

This batch starts from main `7ed527b0` on `runtime/if-batch4-20261016`. It changes only internal Rust providers, tests, pure-query fixtures and their generators. No FFI, C header, wire format, contract, privacy, prototype pin or parity status changes are included. Client adoption and ABI exports remain outside the proof.

Candidate server/core rows, pending native qualification:

- IF-22: six `if22_` tests cover live World training births, fire insertion/deferred spread, soak insertion/deduplication, contact FIFO eviction, navigation edge/cache order, and hospital order through transactions/recovery/ordinary ticks/recipient snapshots.
- IF-08: four `interface_map_helpers` tests compare the listed map helpers across all 33 received authored maps and focused house, material, composition, rail and region boundary cases.
- IF-03: seven `recipient_availability` tests cover recipient-only enable states and denial messages. The pure-function fixture compares 1,000 synthetic snapshot/action inputs and 116 denial-code inputs against the pinned browser function. These are query inputs, not captured match behavior.

Both pure-query fixtures were generated and byte-checked with Node v26.11.1/x64 at prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. The map helpers use integer/string operations; availability uses static prices and selection logic. Existing weather and numeric component fixtures retain their original provenance.

The independent source review accepted IF-22 and IF-08 pending native execution. IF-03 initially lacked denial-code coverage and broad query checks; its successor adds the mapping and exact matrix. No review result substitutes for native qualification.

## Runtime survival

`OrderedAid` replaces the existing hospital owner map with insertion order plus keyed membership. Explicit insert/remove/clear methods preserve order; its read-only dereference cannot mutate membership. Recovery and logistics consume its ordered iterator, and transaction cloning preserves both parts. No unit flag, command, event, link or admission capability is added. The hospital test runs ordinary ticks and collects every recipient's complete snapshot, asserting no fault. The birth test exercises existing World production and the actual live-id unit loop; it does not claim complete World recipient delivery.

Map and availability queries retain no state and read only caller-supplied recipient data. Convoy sorting uses the existing V8 sorter on temporary indices and adds no persistent state. Convoy-enabled ordinary ticks remain guarded on main; component tests do not bypass that guard. Existing tick, recipient snapshot, command and elimination guards were inspected for the changed state.

## Rows withheld from this batch

- IF-26: grouping, retry identity, quarter-metre handoff cache and recipient cell equality have focused consumer tests. The native Horde movement planner lacks the prototype `ownPlan` JSON guard (`shared/ai.js:187-194`).
- IF-25: actual convoy priorities/distances now preserve numeric subtraction ties; AI tests exercise current minimum scans and nonfinite admission. Most prototype AI sort consumers are absent. Admitted raw reserves and finite distance overflow can produce NaN comparisons, so a zero-NaN claim is not established.
- IF-04: shared core fort placement exists, including supported bridge ordering and sight masking, but there is no complete recipient-only building/fort preview adapter. Both `wall` and `gate` return `UnsupportedKind`. Existing field-work and bridge-preview checks provide bounded evidence, not row completion.
- IF-12: authoritative map validation and path helpers exist, but complete authored-scenario support and the editor fairness worker path remain incomplete. The prototype fairness worker runs whole AI matches through its host schedule.
- IF-30: no complete seeded six-AI Classic/Conquest outcome harness exists on main. Current AI policy and mode admission do not support that complete run. Whole-match outcome/statistic criteria also need owner agreement under PARITY.md; an independent-stream exact hash is not a substitute.

CLIENT ROWS: none. These assigned rows all have server/core work. No differs candidate is proposed.

## Validation

Native qualification pending. All three shared Cargo slots were occupied during initial attempts; those attempts launched no Cargo process. An acquired S1 slot ran the formatter, which stopped before compilation because the new ceiling helper preceded a module attribute. The helper was moved below the attribute; the slot was released. No native test ran in that failed formatting attempt. Later source checks also corrected missing-seat spectator/default-action behavior and numeric identity comparisons before native qualification.

The next acquired S3 slot completed formatting, then native compilation stopped on a missing `DamageContext` import in the collection tests (E0433). No test executed. The import is corrected and qualification is pending; the slot was released after Cargo exited.
