# BASE native intent test batch


The owner approved native behavior tests plus prototype review at [issue 5](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267). The tests below require no new Source recording. Existing fixtures and failures remain unchanged. Exact comparisons for data, pure functions and single components remain in place.

The native batch at `db3ab341300d6ba882dc4db3fb1ed2b5258aa011` passed all 27 tests. The earlier `309f02bc` batch ran all 27: 25 passed and BASE-29/30 failed during actual Classic completion with `finish/story supports represented Conquest, Assault and World only`. The runtime fix preserves the earlier result and uses the existing prepared story update. No row is promoted. Each partial check retains its remaining scope below. The test module is `rust/crates/core/src/base_native_intent_tests.rs`; every literal name below has the prefix `match_construction::classic_depot::tests::native_intent::`. Prototype references are `shared/sim.js` at `e4e579a`. Reviewer confirmation is pending.

| Row | Native test | Prototype lines | Exercised intent | Remaining scope |
| --- | --- | --- | --- | --- |
| BASE-20 | `base20_paid_shortest_owned_five_slot_queues` | `3806-3811` | Owned shortest queues, five-slot refusal, MP/Fuel debit, explicit foreign maker and deadline refusal. | Component commands; complete caller mode matrix awaits review. |
| BASE-21 | `base21_training_pauses_and_emits_each_catalog_kind_at_its_duration` | `381,1065-1079,5352` | All 29 training durations, unfinished/deadline pause, completed requests. | Physical aircraft/naval placement and birth remain open. |
| BASE-22 | `base22_waiting_identity_refund_preserves_progress_and_owner_rows` | `1034-1050,3737-3747,6024` | Real paid identities, owner-only cancellation/refund, kept head progress, waiter promotion and owner filtering. | Reconnect/public delivery remains open. |
| BASE-23 | `base23_building_rally_overrides_personal_rally_for_real_buy` | `3766-3780,1071-1079,3389-3393` | Real paid rifle follows building rally over personal rally; rival refusal and invalid personal destination. | Assault/Horde access and full Classic no-personal matrix remain open. |
| BASE-24 | `base24_horde_allies_share_finished_producers_and_labs_only` | `387,3696,3713,3751,3773` | Horde allied finished producer and lab access, enemy exclusion and ordinary ownership. | Horde Buy/payment/birth, rally, assist and site cancellation remain open. |
| BASE-26 | `base26_paid_site_binds_node_and_assigns_actual_builder` | `3689-3709` | Real Depot upfront charge, 25 percent health, claimed node and Engineer route assignment. | Pillbox direction, broader prerequisite matrix and full order-queue controls remain open. |
| BASE-27 | `base27_blocked_unseen_and_node_placement_refuse_without_debit` | `3277-3313` | Real blocked/visibility refusal without debit, valid node footprint. | World territory/rail and coastal preview comparison remain open; placement stays in the exact pure-function category. |
| BASE-29 | `base29_actual_engineer_completes_site_before_it_becomes_prerequisite` | `5375-5380,5587-5603,5352` | Actual Engineer walk and Barracks completion; unfinished training refused and finished maker queues MG. | Multiple-crew exponent, unfinished shooting and deadline halt remain open. |
| BASE-30 | `base30_supported_section_repair_restores_health_without_wallet_charge` | `3710-3722,5591-5596,2629-2645` | Real section contact and repair increase health without MP debit. | Crew/recovery rates, logistics provisions, failed-section restoration and deadline cap remain open. |
| BASE-31 | `base31_cancel_paid_site_refunds_three_quarters_and_removes_sections` | `3748-3759` | Paid site owner cancellation, 75 percent refund, open footprint, removed sections and released node. | Repeated cancellation bounded terrain history and falling debris remain open. |
| BASE-32 | `base32_scuttle_marks_only_owned_living_targets_without_refund` | `3760-3765,5697` | Own target scuttled, rival unchanged, actual retirement and no refund/bounty. | PASS and prototype review required. |
| BASE-33 | `base33_destroyed_owned_building_retires_to_rubble` | `1100-1106,5678,5762` | Real scuttle followed by canonical retirement and rubble footprint. | Collapse event, Depot release and rebuild on settled rubble remain open. |
| BASE-37 | `base37_motorpool_requires_finished_barracks_before_tier` | `353` | Unfinished Barracks prerequisite refused before tier; finished prerequisite reaches tier denial. | Full Motor Pool construction/training remains open. |
| BASE-38 | `base38_airfield_trains_aircraft_only_after_tier_gate` | `354,548` | Actual component aircraft queues and catalog maker/prerequisite. | World nearby-Airfield paratrooper requirement remains open. |
| BASE-39 | `base39_shipyard_naval_map_gate_precedes_payment` | `355,3303` | Naval-map gate refuses before payment, then actual LCVP queue succeeds. | Coastal placement and physical water launch remain open. |
| BASE-44 | `base44_wall_and_gate_static_dimensions_and_producer_exclusion` | `367-370` | Exact Wall/Gate numbers and gate distance; excludes ordinary Build list and producer list. | Fortification-created finished cells remain open; existing full-catalog exact test retained. |
| BASE-45 | `base45_supplycache_is_an_unfinished_nonproducer` | `371,3277` | Exact Cache numbers/logistics flag and exclusion from producers. | Logistics admission and truck right-click refusal on unfinished Cache remain open. |
| BASE-46 | `base46_regional_base_is_never_buyable_and_has_no_recruit_list` | `357,1183-1187,1300-1316,3781` | Worldbase purchase blocked without debit; free static base and absent recruit list. | World local ownership and live region capture lock remain open. |
| BASE-47 | `base47_tech_off_blocks_research_and_removes_purchase_tier_gate` | `799-800` | Real no-tech research refusal; T1 component can queue mortar when tech is off. | Horde starting tiers and complete startup mode matrix remain open. |
| BASE-48 | `base48_tier_gating_refuses_mortar_before_wallet_debit` | `441-442,453,3639,3664,3698,3785` | Every named T2/T3 value and actual T1 mortar denial without debit. | Wall/Gate dig and entrench gates remain open. |
| BASE-52 | `base52_classic_and_conquest_start_with_their_own_forces` | `1575-1600,1679-1686,1176-1179,783-784` | Independent Classic and Conquest constructors: correct force, finished HQ and nine footprint cells. | Fallback/trapped units, World, Horde, Tutorial and other modes remain open. |
| BASE-53 | `base53_assault_defender_gets_trenches_bunker_and_four_flak` | `975-1002,793,838,846` | Real Assault defender startup: trench/sandbag cells, bunker and four finished Flak; attacker has none. | Horde/Annihilation startup and complete exact cell log remain open. |
| BASE-54 | `base54_guard_fort_types_are_finished_owned_structural_cells` | `1197-1225,1324` | Real finished Wall structure primitive registers cell, health and owner. | World farthest-region ranking, tower/wall placement and capture transfer remain open. |
| BASE-55 | `base55_recovery_engineer_queue_obeys_price_pop_and_capacity` | `1358-1378` | Engineer queue charges 60 MP and obeys capacity/pop checks; exact HQ recovery price. | World recover command, territory/blocked/needs/MP refusals and both recovery operations remain open. |
| BASE-56 | `base56_real_classic_deadline_clears_paid_queues_and_starts_decay` | `5756-5762,3689,3727,3739,3807` | Natural 1500-second Classic deadline, paid queue clear/no refund, invalid cancel precedence and 1 percent decay. | World deadline crossing and Build halt remain open. |
| BASE-57 | `base57_ordinary_retreat_selects_living_hq_then_spawn` | `1725-1735` | Real ordinary retreat selects live own HQ, then spawn after HQ death. | Classic nearest producer, World territory/fallback and convoy source remain open. |
| BASE-59 | `base59_live_queue_snapshot_keeps_ids_progress_rally_and_owner` | `6018-6028` | Real paid queue snapshot contains progress, rally, type and owner-only identities. | Full Classic tech/nodes/personal rally snapshot and public/reconnect delivery remain open. |

The normal first-batch log is `/tmp/native-feature-batches-0323/impl_bases/native.log`.
Its launcher and native job are terminal, with the inherited S1 lock released.
The row-named tests for BASE-20/21/22/23/24/26/27/31/32/33/37/38/39/44/45/46/47/48/52/53/54/55/56/57/59 passed there.
BASE-29 and BASE-30 failed before their completion/repair assertions and remain held.
The successor retains every assertion and adds a completed-building story count.
Its runtime fix calls the already-preflighted story transaction at completion.
The complete 27-test successor suite passed at `db3ab341`; prototype-intent
confirmation and Hermes final-head runtime ACK remain pending.


The passing job is `/tmp/native-feature-batches-0345/impl_bases/native.log`.
All 27 row-named tests passed once with retries disabled and no fail-fast.
The measured native suite took 1.733 seconds on the Vostro under CPUs 8-11,
6 GiB memory, zero cgroup swap, 400 percent CPU quota and one test thread.
The Rust tree was `d6934f6584b631f23b0fc42a751ebc34a4f5ac85`.
The native process and systemd launcher are terminal and the inherited S3 lock
is released. Earlier parser, compilation and semantic failures retain their
producing heads and original logs. No new Source capture or fixture was made.

The test PR leaves docs/parity/bases.md unchanged. Root owns later promotion
and the shared proof ledger. A passing partial test does not close the remaining
scope listed above; each full-row candidate still needs prototype-intent review.
