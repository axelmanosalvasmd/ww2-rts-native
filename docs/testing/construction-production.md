# Construction and production runtime batch

Baseline: the #373 serde hotfix plus current `origin/main`. Prototype source
was inspected at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`; no recorder or
captured behavior fixture was added. Native tests exercise intended outcomes.

| Row | Native intent tests | Scope |
| --- | --- | --- |
| BASE-12 | `base12_depots_snap_to_nearest_free_node_and_release_only_their_claim_on_cancel_or_death`; `base12_node_radius_visibility_terrain_and_other_building_reservations_are_enforced` | Classic paid Depot admission, nearest/free/tied nodes, inclusive radius, visibility, node reservations, claim release on cancel and death, rebuild, every-seat delivery. |
| BASE-26 | `base26_paid_building_sites_assign_routes_or_waiting_assist_without_orphaning_full_queues`; `base26_builder_money_prerequisite_tech_tier_placement_and_sudden_death_admission` | Classic payment, builders, full queues, prerequisites, tier/tech, 25% HP sites, Depot claim, slit direction, Shift Assist, construction completion and every-seat delivery. World admission remains deferred. |
| BASE-22 | `base22_classic_paid_jobs_are_owner_only_and_waiting_cancellation_preserves_the_head` | Actual recorded Classic Buy and CancelProduction, paid job identities/status/charges, owner-only delivery including allied seats, active/foreign/Sudden Death denials, exact original-charge refund and preserved head progress. World room admission remains deferred. |

The `classic_paid_training_and_nodes_survive_every_seat_room_frames_through_completion`
server test attaches a real Classic Match to RoomRuntime and collects every
seat's snapshots for 304 ticks through a ground training completion. It checks
both fault owners on every tick. This does not open public Classic lobby start.

Partial work, with no complete-row promotion:

- BASE-18: existing instant purchases gain typed mode/maker and standalone
  aircraft witnesses. Shared Horde aircraft purchase uses its represented actor
  policy; Horde ground rally routing, hosted execution and aviation event
  delivery remain in their streams.
- BASE-21: standalone Classic aircraft training now stages a parked plane and
  its canonical aviation metadata before consuming the head. Tests cover all
  catalog training kinds, completion timing, FIFO, pauses, rally routing and
  rollback. Full logistics completion and World room admission remain held.
- BASE-59: Classic snapshots now publish the prototype's public node list,
  initialized wallets, mode clock, owner queues/jobs, research and rallies.
  `base59_classic_snapshots_publish_exact_nodes_and_private_research_rallies_through_ticks`
  checks exact JSON fields and privacy across every seat. Shared Horde producer
  queues and full logistics delivery remain incomplete.

Hosted aircraft purchases and queued training remain refused before payment
until aviation AA and downed-plane events can be delivered. The bind guard also
refuses a preexisting queued aircraft job. The standalone aircraft witnesses do
not establish hosted sortie safety. The exact wire/privacy mapping is in
[the recipient contract](../contracts/construction-production.md).

Locally acknowledged owner requests: #5 comment 6217590741 and Axel's
6218552386 confirmation permit prototype-identical wire/privacy work while
World remains closed. This batch preserves the reference pin and ABI 2 header,
adds no FFI or public command shape, and leaves exact-head Hermes review,
hosted checks and merge with the orchestrator.
