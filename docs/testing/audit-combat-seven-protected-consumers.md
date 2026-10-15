# AUDIT_COMBAT protected-actuator source readback

Checkout: `/home/judiazm/.t3/worktrees/ww2-rts-native/t3-3fa592e8`.
Branch: `audit/combat-seven-consumers-f7-3fa592e8-remaining`.
Fetched main: `f7b0e5889e3b707beb6d18c4ea8d20a7d9f7b578`.
Exact retained packet: `dff2b3530b322a6abc0b695ff3cc5fff74780d63`.
The new branch starts at fetched main and carries that packet as `6e2939c9`.
The original branch and every frozen PR269 ref remain separate.

This task authors six complete consumer bodies. It does not complete CMB66 or
promote any row: the ordinary logistics tick and three protected actuator
families still reject their required inputs. Every new test is explicitly
ignored, uncompiled and unrun. A future grant must select its exact name and
enable ignored tests. No execution permission follows from this document.

## Ownership and preservation

AUDIT_COMBAT owns this new private test module and its additive cfg(test)
registration, fragment and DESIGN entry. Julio / Rust integration owns the
shipping command, firing, support, aircraft, carrier and logistics providers.
Their files and guards are unchanged. No runtime actuator is authorized here.
The only change in `aligned_replay.rs` is the child-module declaration. Its
Replay constructor, 16,384-entry validation, default environment substitution,
draw/receipt/shot/layout/startup bodies and Arc/Mutex storage remain exact.

All dff CMB38/63 components, the two CMB42 native witnesses, the new-only Source
recorder and two scenes, tagged reader, optional global/environment mode,
startup observer and old corpora remain byte-exact. In particular, no Source
NaN/undefined representation is treated as native alignment without capture.
The authored CMB63 XP0/250 and default 0.75 input remain separate from ordinary
CMB42. No Source packet or new observation exists.

The full `docs/testing/audit-combat-seven-source-f7.md` is retained unchanged.
Read it together with `/tmp/audit-combat-2058-handoff-3fa592e8/HANDOFF.md` for
the exact original packet names, input boundaries, preserved evidence and
PR269 constraints. This successor changes none of those receipts.

## Exact new core unit test names

All names have the prefix `aligned_replay::combat_remaining::`:

- `cmb66_b09_funded_last_round_hit_miss_and_empty_after_real_reload_history`
- `cmb62_tank_shells_single_house_same_house_first_wall_and_rifle_refusal`
- `cmb67_original_hold_fire_patrol_then_explicit_ground_attack_uses_ammo`
- `cmb68_halftrack_capacity_riding_views_unload_reboard_and_actual_wreck_exit`
- `cmb68_tank_rider_enemy_hit_rider_blast_allied_controls_and_enclosed_blast`
- `cmb67_automatic_patrol_weapon_and_bomber_area_salvos_use_actual_consumers`

The module is `rust/crates/core/src/audit_combat_remaining.rs`. It invokes
actual Match construction, command parsing/dispatch, funded research, paid
Build, actual builder completion, purchases and ordinary ticks. It retains
both starting armies. There is no private scheduler, temporary producer,
expected-state import, ammo assignment, stream reset or relaxed runtime guard.
Initial MP10000 is declared funding. Explicit aircraft component inputs and
the original halftrack death input are identified below.

## CMB66: complete authored history, current execution gap

Pinned `test-audit-combat.mjs:115-128` supplies three controls: empty/0,
one-round/0, one-round/0.99. Its assertions are attempt counts0/1/1,
shot-down false/true/false and final ammo0 in all cases.

The new consumer starts each independent match with the unchanged default
16,384-value adapter containing its fixed0 or0.99 input before construction.
The adapter is never reset. This is a controlled authored component input,
not the CMB42 ordinary global/environment observation mode or an aligned
captured Source history. Source B09's direct ammo setter is not used.

An enemy tank comes from actual tier research, a paid motor pool, builder
completion and Buy. A real purchased flak gun starts with its genuine full
reserve. Attack on the tank consumes every earlier round through ordinary
weapon reload intervals. Pinned sim.js:212-217 gives flak vehicle damage0,
interval0.3, speed2.5 and chance0.35. The consumer checks every one-round
debit, real elapsed reload interval, unchanged tank health, exact full-to1/0
stock and the total launch count. It then uses Stop, moves the tank beyond
range, checks stock again and releases Hold Fire through a real stance command.
The final paid dive arrives30 m from the gun, inside AA range but away from
the gun's blast position. Attempt/shot-down counts, final ammo0 and story
credit must match the three original predicates. The actual support chance
call is read back by its default adapter consumer file/line and fixed value;
funded controls consume one such draw and empty consumes none. Other support
payload draws retain their own raw records.

This extended ordinary history differs from Source B09's cleared fixture and
river/pose setup. It is authored native preparation, not captured input
correspondence. It must also survive real provisions and supply behavior;
any refill or forced retreat fails its exact stock assertions. No supply or
reserve state is patched to force success.

The first current blocker is `match_tick.rs:300-301`: enabled logistics or
convoys reject before the tick starts. The first research wait therefore
cannot run on this source. Support interception itself already calls
consume_ammo before the chance roll (`match_support.rs:728-739`). The runtime
owner must connect the actual reserve/convoy phases or explicitly authorize a
bounded alternative before this can become a completed funded-history proof.
This task does neither. CMB66 remains incomplete.

## CMB62: original Tank shells predicates

Pinned `test.js:961-980` requires single-house destruction, complete failed
and settled structural metadata, durable rubble objects within6 m, order
clearing, same-house first-wall passage into the3x3 block, and rifle refusal.
The consumer uses the original shot coordinates on the retained80x80 arena,
real tank prerequisites and movement,20/40-second firing windows and a later
settlement window. It records the first wall and center destruction ticks and
requires the wall to fail earlier. It checks the actual section state,
settledAt, every rubble object and the complete rubble-distance predicate.

Current blockers are earlier than contact: `Command::from_json` has no
FireAt arm, so the actual `fireat` becomes Unsupported. The ground launcher
also rejects nonempty `fire_at` at `match_firing.rs:1441-1447`. Neither is
bypassed by assigning fire_at, invoking a terrain blast or dropping a guard.
The command and firing provider owner must implement the actuator.

## CMB67: explicit, automatic and area consumers

Pinned `test.js:1249-1261` first half requires held patrol ammo unchanged
for0.3 seconds, followed by admitted explicit ground Attack and actual ammo
loss over1 second. The consumer retains both assertions and Hold Fire after
Attack. It buys its attacker from a completed paid airfield after research.
The original declared component positions, Station/Patrol state and retarget99
are explicit inputs before the first observation tick. Source attackPick has
no established native counterpart in AircraftRuntime; this task does not
invent one or claim complete raw correspondence.

The additional consumer calls the real automatic-stance and Area mission
admission paths. It requires actual automatic ammo use and an actual bomber
Salvo event at the ordered point with ammo use. It does not directly install
an Area mission or manufacture an ammo decrement. These are source-authored
counterfactuals from sim.js:2226-2234, separate from the selected first-half
Hold Fire predicate.

`match_aircraft_commands.rs:73-98`, `match_commands.rs:1119-1125` and
`match_aviation.rs:394-418,545-565` still reject the required automatic,
ground-attack or Area paths. Quiet patrol, aircraft AA and helper launching
cannot qualify these positive consumers. The aircraft provider owner must
supply the missing weapon actuators. The ground targeting helper also rejects
an airborne shooter at `match_firing.rs:2041-2042`; the additional aircraft
consumer checks actual visibility and nearby geometry before calling its real
stance/mission admission, so that helper does not mask the intended boundary.

## CMB68: selected wreck and separate rider components

Pinned `test.js:7783-7806` includes nearest boarding/capacity, riding movement,
owner riding flag262144, enemy carrier visibility with hidden passenger,
manual unload, reboarding and a declared carrier HP0 input followed by three
actual ticks. The authored selected consumer retains these checks and expects
actual ejection, cleared riding relation and rifle HP70, the exact30% full
squad loss. It uses actual paid carrier prerequisites and commands. The only
health setter is the original declared death input, after real reboarding.
It does not use manual unload as the wreck exit.

The separate rider component uses real tank boarding, actual hostile direct
contact, rider-specific hurt and full blast membership. It distinguishes a
hit on the rider from a blast that first hits the tank. Allied contacts are
explicit component controls. Nearby enemy visibility and shootable carrier
controls make carried-target refusal decisive. Every expelled rider must
leave cargo and receive suppression at least60. Deck riders lose blast HP;
enclosed halftrack passengers retain HP. These invoke actual DamageWorld
consumers and their emitted native events, not hand-authored Disembark events.
They do not qualify wreck ejection or offshore drowning.

Current dead/missing-carrier branches still return UnsupportedEjection in
`match_carriers.rs:427-433`, and ordinary preflight rejects a dead carrier in
`match_carrier_tick.rs:44-65`. Offshore drowning in `test.js:7815-7859` also
needs the missing thrown placement/distance decision (`sim.js:2423-2432`),
plus the separate naval/shore caller. That complete naval history is not
implemented here and remains a named gap. No tank-rider component result can
fill it. The carrier provider owner retains that actuator.

## Coordination, remaining work and execution receipt

Locally acknowledge issue5 owner decision6201127263, Hermes runtime readback
6201618030 and merge/queue readbacks6202564980/6202580526. The latest issue5
read is separate from scope authorization. Relevant PR195 comments retain
content/schema agreement only; transport and Unreal adoption remain separate.
ABI2 and reference e4e579a remain unchanged. No public reply is authorized.

Root owns the next exact recorder read and any later Source-first grant.
All six new native names, both old CMB42 names, the CMB42 packet instance and
the existing CMB38/63 components remain unqualified on this head. CMB38/42/63
keep their dff limits; CMB62/66/67/68 keep the gaps above. No five-row or
seven-row completion quota is claimed.

Execution counts are zero for Source/Node, Cargo/native, format/lint,
discovery, CI and release. No slot, install, build output, push, PR operation,
parity promotion, reset, rebase or new worktree was used. Only read-only
Git/GitHub/text inspection and source/document edits occurred. A separate
read-only child audited the actual selected predicates and the new consumers;
its findings prompted arrival-tolerance, targetability and temporal-wall
corrections. This is source inspection, not independent compiled proof.

PR269 remains frozen at `2f130c3238f5b0b5db01d26b76af2d60e3475955`.
`/tmp/audit-combat-pr269-true-rebase/` remains untouched. No literal post268
main has been supplied for that separate task. Every parity table and
Acceptance cell is retained exactly. Root remains the sole ledger writer,
execution grantor and publication coordinator.

Git/text preservation receipt: `/tmp/audit-combat-seven-protected-3fa592e8/`.
Its complete protected-path SHA256 list covers1,051 existing blobs unchanged
from dff. Only DESIGN and the additive test registration differ among old
files. Three new files contain the test module, this readback and fragment.
`aligned_replay.rs` restores byte-for-byte after removing only the new child
declaration; DESIGN retains its complete previous body. Frozen refs were
read back directly. This is preservation evidence, not a compiled result.
