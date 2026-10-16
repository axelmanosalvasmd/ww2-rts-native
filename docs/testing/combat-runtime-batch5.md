# Combat runtime batch 5

Stream: COMBAT-RUNTIME-4. Reference: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

This batch adds core behavior and native intent tests for five combat rows. It does not change the parity ledger. Runtime verification is pending.

- CMB-151: `command_feedback::tests::cmb151_every_prototype_denial_has_the_exact_sentence` compares all 28 feedback sentences with the pinned prototype table, including queueFull's command-dependent wording and the fallback for unknown codes. The fixture copies only this pure table from `client/availability.js:6-23`; its rail and aircraft distances are 30 m and 300 m.
- CMB-112: `cmb5_ability_tests::cmb112_cost_table_and_catalog_overrides_are_exact`, `cmb112_classic_and_world_charge_on_use_and_aimed_arrival`, `cmb112_free_ordinary_abilities_survive_ticks_and_recipient_snapshots`, and `cmb112_paid_world_ability_survives_later_core_ticks` check the exact costs, immediate payment, aimed payment after entering range, arrival-time affordability and free ordinary abilities. Classic gains the ability component only. Classic startup and full ticks remain guarded; Classic and World snapshots remain guarded.
- CMB-130: `vision::cmb5_visibility_tests::cmb130_classic_remembers_last_seen_buildings_until_an_airborne_observer_revisits` and `cmb130_world_and_classic_share_building_memory_and_ground_invalidation_rules` check remembered enemy building identity, type, owner, position and construction progress; shared team sight; hidden changes; unseen destruction; and ground/airborne invalidation. Classic has an explicit core refresh helper. World keeps its existing refresh path and memory layout through a shared implementation.
- CMB-127: `vision::cmb5_visibility_tests::cmb127_horde_reveals_only_active_final_ground_enemies_and_bosses_to_defenders` checks inactive waves, remaining budget, the final-three threshold, Kaiju, aircraft and team visibility. `cmb127_bunkers_show_through_fog_and_survive_ordinary_ticks_and_recipient_snapshots` checks bunker visibility, hidden nearby squads and the World exception through six ordinary ticks and recipient snapshots. Horde full ticking and delivery remain guarded.
- CMB-149: `cmb5_base_dispatch_tests::cmb149_core_base_dispatch_survives_ticks_and_recipient_snapshots`, `cmb149_world_waiting_job_cancellation_refunds_and_keeps_core_ticks_live`, and `cmb149_research_dispatch_charges_and_keeps_lab_visible_during_ticks` exercise construction, assistance, cancellation, waiting production refunds, research, purchase and rally dispatch. Ordinary state survives subsequent ticks and both seats' snapshots. World cancellation/rally survives subsequent core ticks, with World snapshots still refused.

## Runtime state and room boundaries

The Classic memory field is populated only by the explicit Classic refresh helper. Classic cannot enter the ordinary runner or recipient snapshot adapter. Horde reveal changes existing visible vectors only and also remains outside ordinary ticking and delivery. The extracted World memory helper preserves its existing retention predicates and `KnownBuilding` fields.

Cancel, cancelProduction and rally reuse canonical construction, production and rally owners. Cancellation cleans the existing actor metadata; retirement and World elimination already remove producer rallies. These commands do not add waiting unit-order kinds. Public R1 still admits only move, attack-move, stop and stance; the server's unavailable list still contains all three new core command families. Existing queued Build/Assist projection remains pending in SNAPSHOT-ADAPTERS-2. This batch claims core dispatch only and adds no route for these orders into snapshot-carrying rooms.

The ability changes reuse existing cooldown, buff, AP and aimed-point fields. Mode identity and firing components accept matching Classic tags without changing tick or snapshot admission. Ordinary ability and base-order tests collect recipient snapshots after later ticks and assert the runner remains unfaulted.

No FFI, C header, wire format, contract, recipient privacy rule or public ingress changes.

## Source detail and deferred rows

The pinned Horde final-wave override also reveals carried ground enemies, despite CMB-127's blanket statement that carried squads are never visible. The core helper preserves that Source override; ordinary vision still hides carried squads. This is a row-description discrepancy, not a proposed gameplay change.

CMB-158 is deferred: the current runner still lists UnitReserves, Rail, GroundEffects and SupplyNetwork as unavailable. This batch does not claim complete combat scheduling from its connected subset. CMB-142 stays with unmerged PR #302. CMB-150 and CMB-146 are next work. CMB-141, CMB-84, the rows in #299/#319, mode-owned Horde/Kaiju work, FireAt contracts, CMB-159 and client rows remain outside this stream.
