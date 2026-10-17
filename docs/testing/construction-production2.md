# Construction and production continuation

This stream starts from #390 at `32dd5931785f598472b79350d4d1e77f1a013176`
and merges #375 at `9ecc71a5509dccac70276088df990a968c083fce` for its ordinary
logistics ticks. It stacks on #390 and #375, and merges after #373.
The reference remains `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

## Row and intent-test mapping

| Row | Tests | Claim boundary |
| --- | --- | --- |
| BASE-18 | `base18_base59_horde_allied_producer_recruitment_and_exact_private_queue_rows`; `base18_base59_horde_ground_ticks_wave_bunker_defeat_and_every_seat_delivery` plus #390's maker and aircraft tests | Shared Horde facilities charge the buyer, publish buyer-owned ground troops, route to rallies and keep queues empty. Hosted aviation remains held on #369, so the full row is incomplete. |
| BASE-21 | `base21_logistics_training_stages_full_vehicle_reserves_before_fifo_publication`; `base21_base59_logistics_training_publishes_reserves_private_jobs_and_terminal_room_frames` plus #390's all-catalog training tests | Classic training publishes ammunition, provisions and vehicle fuel with the birth, consumes jobs in FIFO order and survives ordinary room ticks and terminal delivery. Hosted aircraft training remains held on #369. World room admission remains deferred. |
| BASE-59 | The two Horde tests above; `base21_base59_logistics_training_publishes_reserves_private_jobs_and_terminal_room_frames`; #390's `base59_classic_snapshots_publish_exact_nodes_and_private_research_rallies_through_ticks` | Exact shared Horde queue rows, owner-only paid identities, Classic queues/jobs/research/nodes/personal rallies and private logistics projection. This is recipient snapshot proof, not complete hosted Horde admission. |

The Horde tests, including
`horde_ground_breath_ticks_publish_exact_public_rows_and_redact_retired_identities`
and `horde_boss_surface_event_matches_prototype_for_every_seat_and_saved_batch`,
exercise the ordinary runner, wave-one spawning, bunker death
and eight terminal ticks, collecting every seat's complete recipient view.
Horde never queues paid training in the prototype, so its canonical queue and
paid-job rows are empty while still carrying each producer's identity and rally.
The snapshot test checks the complete set of shared defender producers and
separately checks the owner's paid-job rows; the enemy sees neither.

The logistics room test starts Classic with logistics enabled, records two
paid rifle jobs, advances through both births and publishes every seat on every
tick. It verifies the prototype's queue/job shape, lab progress, personal and
building rallies, public nodes and owner-only reserve rows, then eliminates the
opponent and continues five terminal ticks. The vehicle test additionally
checks full initial ammunition, provisions and fuel at the publication boundary.
The old Horde mode-refusal assertions and Classic logistics refusal test are
replaced by the supported lifecycle and remaining hosted-air checks;
Annihilation's separate logistics startup refusal remains covered.
Commander, visibility, ability and point-supply tests now distinguish supported
Classic/Horde recipient fields from missing mode state, missing aircraft sortie
metadata and deferred World delivery.

## Remaining aviation dependency

The Horde runner automatically introduces aircraft from wave ten. Its ground
runner and snapshot producers are now connected, but `RoomRuntime` attachment
still refuses Horde through the existing snapshot capability boundary until
#369 provides aviation event delivery. Suppressing aircraft would change the
prototype's rules. Classic hosted aircraft purchases and prequeued aircraft
remain guarded for the same dependency. Standalone aircraft behavior retains
#390's existing witnesses.

## Merge record and review boundaries

The #375 merge had one conflict, in `docs/contracts/save-format.md`. Keep #390's
more detailed elimination rows, which document the same serialized state as
#375's combined row. All Rust paths merged automatically; no Rust conflict hunk
was hand edited. The merge commit is `d6e9de21`.

Owner requests acknowledged locally: issue #5 comment 6217590741 (H, K, N, O,
P), confirmed by Axel in 6218552386. No issue comment is posted by this stream.
The ABI 2 header, reference pin and parity status tables stay unchanged.
Hermes' exact-head acknowledgment, hosted CI, review and merge belong to the
orchestrator. Validation results belong to the producing head reported in the
PR body, not to either parent PR's earlier receipts.

Separate source finding left for the recovery stream: Horde reinforcement's
`recovery_home` still selects an owned HQ, whereas the prototype permits an
allied accessible HQ. This does not block ground recruitment or snapshot
production and is not changed here.
