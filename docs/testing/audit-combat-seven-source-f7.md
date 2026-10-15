# AUDIT_COMBAT bounded seven-row source handoff

Base: `f7b0e5889e3b707beb6d18c4ea8d20a7d9f7b578`.
Checkout: `/home/judiazm/.t3/worktrees/ww2-rts-native/t3-3fa592e8`.
Branch: `audit/combat-seven-source-f7-3fa592e8-2158`.
Reference gitlink: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`, unchanged.
This is one source task, with no execution grant or slot held. Stop after this
handoff. Root owns the next worker, exact-head grants, review and publication.

## Implemented source

Recover the complete independent recorder/helper and gap history from
`30f186a40e1f5b0e2609713b3594e7ec5fcc6ab3`, plus the full two-name native
module from `7aab8df4cbb6fd286290bbb5c8a30b6317366a34`. Do not replay either
ancestry wholesale: both descend from combat work owned by frozen PR269.
Only their CMB42-specific files and additive entry/registration changes are
included. The old recorder object remains available through its original ref.

The maintained new-only entry remains
`harness/match-tick-fixture.mjs --cmb42-ordinary-only`, requiring exactly one
explicit private `--cmb42-output=/absolute/existing/directory`, x64 Node
v24.21.0 and a separately granted Source invocation. It returns before the old
corpus and loader. The new packet format is `cmb42-ordinary-tagged-v1`.
The two complete authored scene names remain:

- `ordinary-sniper-12-30-distance-and-recipient`
- `ordinary-at-7-28-researched-built-producers`

The packet now separates declared inputs from expected observations. Its
constructor input names the retained map, both seats/factions/teams, computed
layout input [0,1], options and seed701. An independent Source observer checks
the actual computed layout before allocation, without replacing it. Each
initial funding/cooldown input, immediate purchase, queued owner/sequence/tick
command and actual step has a manifest entry at its observation boundary.
Raw observations retain all old fields plus actual canShoot/visibility probes.
Environment observations retain signed before/after seeds and explicit u32 bits.

The cfg(test)-only reader is
`rust/crates/core/src/aligned_replay_cmb42.rs`. It uses actual native
construction, purchases, recorded queues and ticks. The command observation
hook saves snapshots inside the sole existing queued boundary, before the
next command executes. It does not dispatch a second simulated queue. The
global input constructor checks actual captured capacity, SplitMix64 continuity,
indexes, seed transitions and fraction bits. It never uses a padded fallback.
The passive scenes require only their constructor global seed draw. The private
mode requires that one-value capacity as well as its metadata, so no accepted
16,384-value Replay::new input can activate it, even with an extra marker key.
Its environment branch executes and returns the actual original generator,
recording seed transitions, bits and native consumer/tick context separately.
Every observation compares the consumed prefix. Both computed layouts must
agree before the declared native layout can qualify.

The native receipt retains full raw Source records, corresponding native
state, complete existing native observer output, both recipient projections,
recipient missing capabilities, real canShoot probes and random receipts.
Property absence, typed kinds, native absence markers and unmapped native
representations remain differences. No defaults are applied or unknown fields
suppressed. The first raw/input difference stops the replay; runner errors
retain partial native state and receipts. The output is created exclusively,
so an old receipt is never overwritten. No packet exists yet and no raw or
alignment equality is claimed.

## Exact native names and inputs

The restored ordinary witnesses, unchanged from the recovered native module:

- `match_tick::cmb42::cmb42_named_sniper_prefers_mg_through_real_buy_move_tick_and_owner_view`
- `match_tick::cmb42::cmb42_named_at_prefers_tank_after_real_research_construction_and_purchases`

Both keep all constructor actors, real purchases, MP5000 and the original
one-time shooter cooldown1e9 input. Sniper distances are12/30, then12/40 and
hidden-MG60. AT distances are7/28 with actual tier research, paid motor pools,
builder completion and tank facing through Move. Neither imports expected
pose, HP, visibility, RNG state or a temporary producer.

The new packet instance is a core unit test, not a simtest integration test:

- `aligned_replay::startup_state::cmb42::ordinary_cmb42_packet_matches_actual_native_boundaries`

It is ignored by default. A future grant must name a frozen genuine packet in
`WW2_CMB42_PACKET` and a new private output file in
`WW2_CMB42_NATIVE_RECEIPT`, then select this exact instance with ignored tests
enabled. Do not infer such a grant from the environment variables or this note.

The new independent component names are:

- `match_combat::audit_owned::cmb38_rifle_and_at_hit_actual_pillbox_front_and_rear_sections`
- `aligned_replay::startup_state::cmb42::combat_effects::cmb63_named_veterancy_and_actual_accuracy_damage_suppression_consumers`

CMB38 uses a completed1500HP pillbox with four real registered sections and
four separate contact inputs: rifle front/rear, AT front/rear. The actual
`DamageWorld::local_contact` path must update section health, entity health and
attacker XP. Source expectations are rifle0.15/0.30 and AT120/240, from
shared/sim.js:189-196,164,4264,4273. This is actual contact composition, not a
new Source capture, ordinary Build replay or full firing loop.

CMB63 reads the complete original test.js:854-860 Veterancy predicate. Its
independent matches declare XP0/250 and fixed component positions before
firing, retain all constructor actors, and start with the same preconstructor
default0.75 tape. It never resets a running stream. Actual Attack and firing
produce five flights: the0.7 rifle misses and the0.84 two-star rifle hits.
Actual accuracy/miss-magnitude/miss-sign receipts must distinguish the two.
The original owner row13 star assertion remains. Subsequent actual direct-hit
and suppression consumers must produce damage3/2.52 and suppression4/2.8,
with actual damage XP credit. These are controlled component effects. They do
not establish a continuous earned-XP match, Horde wave starts or Source capture.

All five names above are authored/restored, uncompiled and unrun on this head.
No old native pass, complete Source assertion or registry count is transferred.

## Held rows and protected providers

CMB38/42/63 retain their existing statuses until complete selected checks and
Root's gates qualify. CMB62 still rejects nonempty ordinary ground fire_at;
the original Tank shells check also needs same-house first-wall passage,
physical cell destruction, order clearing and rifle refusal. CMB67 automatic
aircraft ground fire and bomber salvos remain unsupported; the entire selected
first-half Hold Fire check needs the actual ammo-use consumer. CMB68 missing
or dead carriers still return UnsupportedEjection; actual30% halftrack wreck
loss, rider-hit/blast branches and offshore drowning are not filled by helper
unload. CMB66 still needs original B09 hit/miss/empty funded1-to0 histories,
without assigning ammo or resetting streams. No new runtime actuator or guard
relaxation is attempted in these protected files.

Root and ALIGNED explicitly acknowledged the optional private replay and
command-observer scope. Preserve old Replay::new,16384 validation, draw/receipt,
shot tags, layout/startup observers, default environment substitution and
Arc/Mutex sharing. Shared file changes are additive registration in
aligned_replay_startup.rs, an optional environment branch and command delegate
in aligned_replay.rs, one cfg(test) call in match_tick.rs, the early harness
branch and additive test registrations. Existing recorder bodies, corpora,
old tests, reference gitlink, dependencies, contracts, wire, ABI and all parity
tables stay intact. CHANGELOG.md history is untouched; this task uses a fragment.

The full issue5 read included owner decision6201127263, Hermes exact-head
readback6201618030 and latest merge/queue readbacks6202564980/6202580526.
Acknowledge their receipt locally, separately from this
task's internal scope acknowledgment. The approved comparison-default rule
does not initialize runtime state or qualify this unrun reader. No public reply
is authorized. Startup-table content acceptance from PR195 remains distinct
from transport and Unreal adoption.

PR269 at `2f130c3238f5b0b5db01d26b76af2d60e3475955` and
`/tmp/audit-combat-pr269-true-rebase/` are untouched. No rebase against
intermediate main, push, PR update, hosted run or parity promotion occurred.
Root must provide literal post268 main before that separate publication task.

## Qualification still missing

Execution counts are zero: Source/Node, Cargo/native, format/lint, test
discovery, CI and release. No build output was created. No slot was acquired.
Text/Git inspection is the only verification in this task. In particular the
strict reader's existing native absence/unmapped fields remain an explicit
comparison gap; a future difference must be retained, not repaired by loading
Source state. Ordinary CMB42 correspondence still has the original composed
versus contiguous limits, including old AT12/28 versus original7/28 geometry.
Root owns a fresh exact-head/selection/slot grant, independent review, hosted
checks and Hermes readback before any runtime merge.
