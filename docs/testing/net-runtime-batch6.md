# NET-RUNTIME-5: recipient unit visibility

Base: `ead962f6d54fccb766191b09a247992d8c0b68ec`. Prototype: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

NET-52 describes the `units` list for a receiving seat. The existing native predicate already matched prototype `shared/sim.js:5942-5945`, but cache construction rejected every aircraft before applying it. The cache now validates canonical aviation metadata, projects the ordinary 15-column plane row and sets flag 512 for grounded aircraft. Aircraft plans read the canonical mission with the prototype's existing plan kinds (`shared/sim.js:5819-5821`).

Proven row:

- NET-52: `recipient_snapshot::net52_tests::net52_units_include_allies_visible_enemies_airborne_planes_and_end_reveal`. Three seats cover own and allied units without visible membership, hidden and visible ground enemies, all five enemy aircraft states, grounded friendly aircraft, airborne enemies outside vision and end reveal. Row shape and the grounded flag are asserted.
- NET-52 safety: `recipient_snapshot::net52_tests::net52_aircraft_ticks_and_recipient_delivery_preserve_room_admission_guard`. A recorded Move sends an allied rifle to capture a neutral authored 5-VP point. The unchanged Conquest VP goal is earned through ordinary ticks, which continue through victory and three further ticks. Every tick collects all three seats' terrain/fog/recipient projections without a runner fault. The patrol traverses all five aircraft states. The owner's initial mission plan is retained, and the ordinary victory path reveals every unit to every seat, including a previously hidden enemy plane at base. No ownership, score, finish or reveal is installed in this journey. Removing the required aviation metadata still refuses projection.

This is internal recipient projection coverage. `RecipientProjection::missing` explicitly retains `Air`: private aircraft lists are outside this batch. The complete snapshot adapter still refuses aircraft before room attachment, and the test checks that refusal before and after ticking. No Match field, command admission, transport, wire schema, C header, contract or recipient policy changes. No Source recorder or fixture was added. NET-54 and hosted aircraft remain separate work.

Dropped rows:

- NET-04: deflate negotiation and its 1 KiB threshold need a WebSocket extension transport change, outside the wire-change limit.
- NET-36: full start readiness needs public start ingress and Assault room admission beyond main's private two-human Conquest profile.
- NET-37: async load cancellation and Horde rollback need broader startup integration; synchronous trusted preparation alone does not prove the row.
- NET-54: complete owner-private coverage needs aircraft lists, Classic currencies and logistics projection/admission; the ordinary subset does not prove the row.
- NET-94: full Horde records need hosted Horde results, result fields, persistence and shard-to-front record ownership.
- NET-95: sandbox spawning needs sandbox room admission and public `spawn` ingress beyond main.
- NET-96: complete sharding needs a front/worker supervisor, upgrade forwarding, room-list aggregation and record/map IPC ownership.

Differs candidates: none. The task range is exhausted. No work from PRs #335, #340 or #296 was imported.

Validation uses only `ww2-core` tests at `-j4`, with the shared Cargo slot, 6 GiB memory cap and no swap. Before publication, run formatting and strict `ww2-core` all-target Clippy after merging current main. Hosted checks and parity status updates belong to the orchestrator.

After merging main `b6d05874078f183fc95421cd25e13762b253c57c`, the finish-journey repair passed on Vostro: team 0 won by VP at tick 7,497; the run ended at tick 7,500 after collecting 22,500 recipient projections. The two NET-52 tests and all 37 recipient snapshot tests passed with zero failures. Formatting and strict locked `ww2-core` all-target Clippy passed in S2 with 6 GiB memory and no swap; S2 was released. Complete aircraft snapshots remain refused.
