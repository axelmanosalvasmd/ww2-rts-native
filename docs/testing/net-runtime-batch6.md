# NET-RUNTIME-5: recipient unit visibility

Base: `ead962f6d54fccb766191b09a247992d8c0b68ec`. Prototype: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

NET-52 describes the `units` list for a receiving seat. The existing native predicate already matched prototype `shared/sim.js:5942-5945`, but cache construction rejected every aircraft before applying it. The cache now validates canonical aviation metadata, projects the ordinary 15-column plane row and sets flag 512 for grounded aircraft. Aircraft plans read the canonical mission with the prototype's existing plan kinds (`shared/sim.js:5819-5821`).

Proven row:

- NET-52: `recipient_snapshot::net52_tests::net52_units_include_allies_visible_enemies_airborne_planes_and_end_reveal`. Three seats cover own and allied units without visible membership, hidden and visible ground enemies, all five enemy aircraft states, grounded friendly aircraft, airborne enemies outside vision and end reveal. Row shape and the grounded flag are asserted.
- NET-52 safety: `recipient_snapshot::net52_tests::net52_aircraft_ticks_and_recipient_delivery_preserve_room_admission_guard`. A recorded Move sends an allied rifle to capture a neutral authored 5-VP point. The unchanged Conquest VP goal is earned through ordinary ticks, which continue through victory and three further ticks. Every tick collects all three seats' terrain/fog/recipient projections without a runner fault. The patrol traverses all five aircraft states. The owner's initial mission plan is retained, and the ordinary victory path reveals every unit to every seat, including a previously hidden enemy plane at base. No ownership, score, finish or reveal is installed in this journey. Removing the required aviation metadata still refuses projection.

This is internal recipient projection coverage. After merging main `edeb4ca218e23b25daea33b43e2809d9fe17cf41` (#331), the shared aircraft adapter supplies owner-private `air` rows and admits canonical aircraft without pending aviation events. NET-52 checks successful preflight before and after ticking, the completed Air capability and exact owner-only aircraft membership on every projection. Invalid aviation metadata and uncontracted aviation events remain refused. The merge uses main's validator and mission-plan adapter. `recipient_effects` retains #331's selected-row and numeric-bit comparisons, including the parked aircraft frame, unchanged. No Match field, command admission, transport, wire schema, C header, contract or recipient policy changes. No Source recorder or fixture was added. NET-54's broader scope remains separate work.

Dropped rows:

- NET-04: deflate negotiation and its 1 KiB threshold need a WebSocket extension transport change, outside the wire-change limit.
- NET-36: full start readiness needs public start ingress and Assault room admission beyond main's private two-human Conquest profile.
- NET-37: async load cancellation and Horde rollback need broader startup integration; synchronous trusted preparation alone does not prove the row.
- NET-54: complete owner-private coverage needs aircraft lists, Classic currencies and logistics projection/admission; the ordinary subset does not prove the row.
- NET-94: full Horde records need hosted Horde results, result fields, persistence and shard-to-front record ownership.
- NET-95: sandbox spawning needs sandbox room admission and public `spawn` ingress beyond main.
- NET-96: complete sharding needs a front/worker supervisor, upgrade forwarding, room-list aggregation and record/map IPC ownership.

Differs candidates: none. The task range is exhausted. No work from PRs #335, #340 or #296 was imported.

Repair validation runs the whole `ww2-simtest` crate and the `ww2-core` recipient snapshot suite at `-j4`, with a shared Cargo slot, a 6 GiB memory cap and no swap. A temporary nextest scheduling override assigns four thread slots to each test, keeping one test process active within the memory cap. Formatting and strict workspace all-target Clippy run from `rust/` in a slot. Hosted checks, owner re-ack and parity status updates belong to the orchestrator.

After merging main `b6d05874078f183fc95421cd25e13762b253c57c`, the finish-journey repair passed on Vostro: team 0 won by VP at tick 7,497; the run ended at tick 7,500 after collecting 22,500 recipient projections. The two NET-52 tests and all 37 recipient snapshot tests passed with zero failures. Formatting and strict locked `ww2-core` all-target Clippy passed in S2 with 6 GiB memory and no swap; S2 was released. At that historical head, complete aircraft snapshots remained refused.


Repair validation after merging `edeb4ca218e23b25daea33b43e2809d9fe17cf41`:

- Whole simtest crate: 264 passed. Core recipient snapshot suite: 40 passed, including both NET-52 tests. Total: 304 passed, zero failed, 780 unrelated core tests filtered out.
- `recipient_effects::original_ordinary_batches_match_selected_shot_rows_and_numeric_bits` passed with #331's rewrite unchanged. Its 158 captured frames and 790 recipient results retain selected-seat and exact numeric-bit comparisons; the live-flight refusal retains its exact capability and unchanged guard state.
- `cargo fmt --all --check` and `cargo clippy --locked --workspace --all-targets -j4 -- -D warnings` passed from `rust/`.
- Vostro, 6 GiB memory cap, zero swap. Final tests used S1 and the temporary scheduling override, with build jobs and nextest thread budget both four. All slots were released.

The initial merge-only test run stopped at the stale NET-52 aircraft-refusal assertion (15 passed, one failed). After reconciling that test and the merged capability/plan code with main, a four-process run hit the scope's memory limit (75 passed, four processes terminated, 225 unrun). Systemd recorded `oom-kill`. The final run above ran each test in its own process, one at a time, and passed. No repository test configuration, fixture, visibility predicate or aviation-event refusal changed. Hosted CI and the fresh exact-head owner ack remain pending.
