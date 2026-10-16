# Combat runtime batch 5

Stream: COMBAT-RUNTIME-4. Reference: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

This batch claims four combat rows: CMB-151, CMB-112, CMB-127 and CMB-149. CMB-130 keeps its component tests but is dropped from the claim. The parity ledger is unchanged.

- CMB-151: `command_feedback::tests::cmb151_every_prototype_denial_has_the_exact_sentence` compares all 28 feedback sentences with the pinned prototype table, including queueFull's command-dependent wording and the fallback for unknown codes. The fixture copies only this pure table from `client/availability.js:6-23`; its rail and aircraft distances are 30 m and 300 m.
- CMB-112: `cmb5_ability_tests::cmb112_cost_table_and_catalog_overrides_are_exact`, `cmb112_classic_and_world_charge_on_use_and_aimed_arrival`, `cmb112_free_ordinary_abilities_survive_ticks_and_recipient_snapshots`, and `cmb112_paid_world_ability_survives_later_core_ticks` check the exact costs, immediate payment, aimed payment after entering range, arrival-time affordability and free ordinary abilities. Classic gains the ability component only. Classic startup and full ticks remain guarded; Classic and World snapshots remain guarded.
- CMB-127: `vision::cmb5_visibility_tests::cmb127_horde_reveals_only_active_final_ground_enemies_and_bosses_to_defenders` checks inactive waves, remaining budget, the final-three threshold, Kaiju, aircraft and team visibility. `cmb127_bunkers_show_through_fog_and_survive_ordinary_ticks_and_recipient_snapshots` checks bunker visibility, hidden nearby squads and the World exception through ordinary VP finish and eight ending ticks, collecting every seat's recipient snapshot on every tick. The original six-tick assertions remain, and the continuation checks the canonical end-of-match reveal. Horde full ticking and delivery remain guarded.
- CMB-149: `cmb5_base_dispatch_tests::cmb149_core_base_dispatch_survives_ticks_and_recipient_snapshots`, `cmb149_world_waiting_job_cancellation_refunds_and_keeps_core_ticks_live`, and `cmb149_research_dispatch_charges_and_keeps_lab_visible_during_ticks` exercise construction, assistance, cancellation, waiting production refunds, research, purchase and rally dispatch. Ordinary dispatch and research state survive through VP finish and eight ending ticks, with every seat's snapshot collected on every tick. World cancellation/rally survives subsequent core ticks on an authored rural map, with World snapshots still refused.

## Runtime state and room boundaries

Building memory refresh and `ghosts` delivery follow the prototype's Classic/World construction-mode gate (`shared/sim.js:384,4444,6037`). Classic keeps its explicit core refresh helper, and World keeps its existing memory phase and layout through the shared implementation. Default/Conquest never populate memory or deliver `ghosts`. `ordinary_default_and_conquest_never_remember_or_deliver_building_ghosts_through_finish` checks independent and allied teams seeing a barracks, retreating, hidden changes, retirement, VP finish, ending hold and revisit. `ordinary_recipient_omits_ghosts_even_with_stale_component_memory` also checks delivery with pre-existing component memory. Tutorial creates none of this memory. Classic ticks and non-ordinary recipient snapshots remain guarded. Horde reveal changes existing visible vectors only and remains outside ordinary ticking and delivery.

Cancel and cancelProduction use canonical construction and production owners. Rally uses main's canonical `set_rally` implementation from PR #316. Cancellation cleans the existing actor metadata; retirement and World elimination already remove producer rallies. These commands do not add waiting unit-order kinds. Public R1 still admits only move, attack-move, stop and stance; the server's unavailable list still contains all three new core command families. Main's merged snapshot adapters retain queued Build-to-Assist, Assist, Cover and Dig projection. This batch claims core dispatch and does not expand public command admission.

World continuation checks use rural regions and retain their constructor guards; resource/city automatic salvos remain guarded. The ability changes reuse existing cooldown, buff, AP and aimed-point fields. Mode identity and firing components accept matching Classic tags without changing tick or snapshot admission. Ordinary ability and base-order tests collect every seat's recipient snapshot on every tick through VP finish and the ending hold, and assert the runner remains unfaulted.

No FFI, C header, wire format, contract, recipient privacy rule or public ingress changes.

## Source detail and deferred rows

The pinned Horde final-wave override also reveals carried ground enemies, despite CMB-127's blanket statement that carried squads are never visible. The core helper preserves that Source override; ordinary vision still hides carried squads. This is a row-description discrepancy, not a proposed gameplay change.

CMB-130 is dropped: building memory runs in Classic/World only; waits on #301 Classic runner and World admission. Its retained component tests are `vision::cmb5_visibility_tests::cmb130_classic_remembers_last_seen_buildings_until_an_airborne_observer_revisits` and `cmb130_world_and_classic_share_building_memory_and_ground_invalidation_rules`. They check remembered identity, type, owner, position and construction progress; shared team sight; hidden changes; unseen destruction; and ground/airborne invalidation. These tests do not establish recipient integration in a running Classic/World match.

CMB-158 is deferred: the current runner still lists UnitReserves, Rail, GroundEffects and SupplyNetwork as unavailable. This batch does not claim complete combat scheduling from its connected subset. CMB-142 stays with unmerged PR #302. CMB-150 and CMB-146 are next work. CMB-141, CMB-84, the rows in #299/#319, mode-owned Horde/Kaiju work, FireAt contracts, CMB-159 and client rows remain outside this stream.

## Verification

The candidate includes main `b6d05874078f183fc95421cd25e13762b253c57c`, already merged without rebasing. The earlier Tutorial merge retained both Classic and Tutorial component admission in abilities, actor identity and firing. Tick, snapshot and public-ingress guards remain intact.

Round 2 checks run from `rust/` on the Vostro, using a shared slot, a 6 GiB memory limit, zero swap and four build/test jobs. Nextest uses the `ci-test` profile and only `ww2-core`, the package changed in this round. Historical results at `74ded6df` are separate from this repair. No JavaScript execution, new behavior recorder or behavior provenance bundle is used.

Checks passed on the repaired Rust tree `64444e5bbef9e1a97f165794de5e1933c2032fed`:

- Combat: 14 passed, 789 skipped (`test(cmb5_) | test(cmb151_)`).
- Recipient snapshots: 41 passed, 762 skipped (`test(recipient_snapshot)`).
- Memory: 23 passed, 780 skipped (`test(memory)`).
- `cargo fmt --all --check`: passed.
- `cargo clippy --locked -p ww2-core --all-targets -j4 -- -D warnings`: passed.

Each nextest command used `--locked -p ww2-core --cargo-profile ci-test -j4`, with `CARGO_BUILD_JOBS=4`. Combat and memory ran in S2; recipient checks ran in S3; fmt and Clippy ran in S2. Every slot was released after its command. The initial combat command duplicated nextest's thread option and exited before compilation or test execution; the corrected command produced the 14 passing results above. Raw logs remain in `/tmp/fix-341b-validation/`.
