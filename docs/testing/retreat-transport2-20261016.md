# Retreat and transport follow-up

This stream stacks on #397; merges after #373. The starting head is
`c644ffa1f3563732df8f2aca9a5166df01ff364d`, with prototype
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

## Each row literal test mapping

| Row | Tests | Scope |
| --- | --- | --- |
| CMB-146 | `match_carriers::transport_intent::cmb146_board_exits_garrison_before_routing_and_delivers_board_and_unload`; `cmb146_board_stops_construction_without_removing_or_refunding_the_site`; `cmb146_board_cancels_paid_dig_without_refund_and_survives_ticks`; `cmb146_board_detaches_only_assigned_mass_entrenchment_workers` | Actual commands, work owners, boarding, unload and complete every-seat snapshots through ordinary ticks. Rail component dispatch remains covered by #397's MOV-81 tests. World room admission is excluded. |
| MOV-48 | `match_reactions::retreat_home_tests::mov48_ordinary_retreat_traverses_multiple_legs_and_delivers_every_seat`; `mov48_classic_retreat_traverses_multiple_legs_with_all_seat_delivery`; existing `mov48_retreat_actual_ground_displacement_is_one_and_a_half_normal_speed`; `mov48_manual_retreat_exits_house_moves_home_and_delivers_every_seat`; `mov48_aircraft_retreat_flies_home_and_rearms_through_ticks_and_all_seats` | Default, Conquest and Classic traverse successive legs and deliver every seat. Existing witnesses cover house exit, speed and aircraft return. World return through territory legs and admitted World room delivery remain unproved. |

CMB-146 follows `shared/sim.js:3520-3570`: assign seats first, exit the
building, cancel the assigned squad's work and clear its prior commands.
Construction leaves the paid unfinished site present. Paid Dig cancellation
removes its descriptor without a refund. Mass entrenchment removes only the
assigned worker's membership and paid segment; an unassigned worker keeps both.
The old tests that refused all house/build/project boarding are replaced by
canonical activity-consistency guards and these behavior tests.

MOV-48 follows `shared/sim.js:1725-1734,3091-3099,3335-3343,5349-5350,5484,5536`
and `shared/navigation.js:347,362,381,385`. The long-route map exceeds the
small-map shortcut, requires at least three route endpoints, observes the
first leg being traversed and retains the final home until arrival. Three
post-arrival ticks check that the match stays healthy. The Classic test replaces
the prior short-route witness. Home selection, 1.5x movement and aircraft
home/rearm retain the tests inherited from #397.

No new runtime state, recipient field, protocol shape, FFI or C header is added.
The existing contracts in `docs/contracts/retreat-transport.md` are unchanged.
The Classic retreat witness now requires complete per-seat snapshots on every tick. World home/rail core components
inherited from #397 do not prove World return through legs or open World rooms.
Those World-only parts remain held on World admission.
