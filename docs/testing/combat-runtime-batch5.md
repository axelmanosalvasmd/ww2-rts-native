# Combat runtime batch 5

Stream: COMBAT-RUNTIME-4. Reference: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

This batch adds core behavior and native intent tests for five combat rows. It does not change the parity ledger. All 72 selected native checks, formatting and strict workspace Clippy passed on the Vostro in S2, with 6 GiB memory, zero swap and four Cargo jobs.

- CMB-151: `command_feedback::tests::cmb151_every_prototype_denial_has_the_exact_sentence` compares all 28 feedback sentences with the pinned prototype table, including queueFull's command-dependent wording and the fallback for unknown codes. The fixture copies only this pure table from `client/availability.js:6-23`; its rail and aircraft distances are 30 m and 300 m.
- CMB-112: `cmb5_ability_tests::cmb112_cost_table_and_catalog_overrides_are_exact`, `cmb112_classic_and_world_charge_on_use_and_aimed_arrival`, `cmb112_free_ordinary_abilities_survive_ticks_and_recipient_snapshots`, and `cmb112_paid_world_ability_survives_later_core_ticks` check the exact costs, immediate payment, aimed payment after entering range, arrival-time affordability and free ordinary abilities. Classic gains the ability component only. Classic startup and full ticks remain guarded; Classic and World snapshots remain guarded.
- CMB-130: `vision::cmb5_visibility_tests::cmb130_classic_remembers_last_seen_buildings_until_an_airborne_observer_revisits` and `cmb130_world_and_classic_share_building_memory_and_ground_invalidation_rules` check remembered enemy building identity, type, owner, position and construction progress; shared team sight; hidden changes; unseen destruction; and ground/airborne invalidation. `cmb130_ordinary_building_memories_are_private_last_observed_facts_through_match_finish` adds Default and Conquest matches with independent teams and allied sharing. It places canonical buildings, checks private last-observed rows, retires a building under fog, reaches VP finish and revisits the empty location. Classic keeps its explicit core refresh helper. World keeps its existing refresh path and memory layout through a shared implementation.
- CMB-127: `vision::cmb5_visibility_tests::cmb127_horde_reveals_only_active_final_ground_enemies_and_bosses_to_defenders` checks inactive waves, remaining budget, the final-three threshold, Kaiju, aircraft and team visibility. `cmb127_bunkers_show_through_fog_and_survive_ordinary_ticks_and_recipient_snapshots` checks bunker visibility, hidden nearby squads and the World exception through ordinary VP finish and eight ending ticks, collecting every seat's recipient snapshot on every tick. The original six-tick assertions remain, and the continuation checks the canonical end-of-match reveal. Horde full ticking and delivery remain guarded.
- CMB-149: `cmb5_base_dispatch_tests::cmb149_core_base_dispatch_survives_ticks_and_recipient_snapshots`, `cmb149_world_waiting_job_cancellation_refunds_and_keeps_core_ticks_live`, and `cmb149_research_dispatch_charges_and_keeps_lab_visible_during_ticks` exercise construction, assistance, cancellation, waiting production refunds, research, purchase and rally dispatch. Ordinary dispatch and research state survive through VP finish and eight ending ticks, with every seat's snapshot collected on every tick. World cancellation/rally survives subsequent core ticks on an authored rural map, with World snapshots still refused.

## Runtime state and room boundaries

The building memory field is populated by the Default/Conquest vision phase and by the explicit Classic component refresh helper. The recipient adapter emits only the recipient team's hidden last-observed facts using the existing six-field `ghosts` encoding. Hidden live changes do not replace those facts. Classic cannot enter the ordinary runner or recipient snapshot adapter. Tutorial creates none of this memory; World keeps its own memory owner. Horde reveal changes existing visible vectors only and also remains outside ordinary ticking and delivery. The extracted World memory helper preserves its existing retention predicates and `KnownBuilding` fields.

Cancel and cancelProduction use canonical construction and production owners. Rally uses main's canonical `set_rally` implementation from PR #316. Cancellation cleans the existing actor metadata; retirement and World elimination already remove producer rallies. These commands do not add waiting unit-order kinds. Public R1 still admits only move, attack-move, stop and stance; the server's unavailable list still contains all three new core command families. Main's merged snapshot adapters retain queued Build-to-Assist, Assist, Cover and Dig projection. This batch claims core dispatch and does not expand public command admission.

World continuation checks use rural regions and retain their constructor guards; resource/city automatic salvos remain guarded. The ability changes reuse existing cooldown, buff, AP and aimed-point fields. Mode identity and firing components accept matching Classic tags without changing tick or snapshot admission. Ordinary ability and base-order tests collect every seat's recipient snapshot on every tick through VP finish and the ending hold, and assert the runner remains unfaulted.

No FFI, C header, wire format, contract, recipient privacy rule or public ingress changes.

## Source detail and deferred rows

The pinned Horde final-wave override also reveals carried ground enemies, despite CMB-127's blanket statement that carried squads are never visible. The core helper preserves that Source override; ordinary vision still hides carried squads. This is a row-description discrepancy, not a proposed gameplay change.

CMB-158 is deferred: the current runner still lists UnitReserves, Rail, GroundEffects and SupplyNetwork as unavailable. This batch does not claim complete combat scheduling from its connected subset. CMB-142 stays with unmerged PR #302. CMB-150 and CMB-146 are next work. CMB-141, CMB-84, the rows in #299/#319, mode-owned Horde/Kaiju work, FireAt contracts, CMB-159 and client rows remain outside this stream.

## Verification

The candidate includes main `b6d05874078f183fc95421cd25e13762b253c57c`. The earlier Tutorial merge retained both Classic and Tutorial component admission in abilities, actor identity and firing. The final snapshot-adapter merge was automatic. Existing tick, snapshot and public-ingress guards remain intact.

Checks produced at runtime commit `74ded6dfd8bf280861cf3b8958eaccb85e09d937`:

- Core combat nextest: 13 passed, 550 skipped.
- Recipient, AI and Tutorial nextest: 58 passed, 505 skipped.
- Simtest `world_memory::original_world_memory_producers`: 1 passed, 259 skipped. This checks the World visibility and memory producer against its existing pinned fixture.
- `cargo fmt --all --check`: passed.
- `cargo clippy --locked --workspace --all-targets -j4 -- -D warnings`: passed.

All three nextest runs used the repository's `ci-test` Cargo profile, four build jobs and four test processes. All Cargo commands ran from `rust/`. No JavaScript execution, new behavior recorder or behavior provenance bundle was used. The only new fixture is the exact denial sentence table. The documentation follow-up changes no Rust source.
