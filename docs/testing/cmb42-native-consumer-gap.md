# CMB-42 named sniper consumer and remaining evidence gap

This recovered document records the earlier checkpoints through30f186a4.
The [current bounded source task](audit-combat-seven-source-f7.md) adds an
unrun tagged-packet reader and declared-input manifest. Statements below that
the reader is missing describe those older checkpoints. Source capture,
native compilation/execution and complete alignment qualification remain absent.

## New-only ordinary Source recorder draft

The separate CMB42 branch now authors two recorder scenes in
`harness/cmb42-ordinary-scenes.mjs`. The maintained entry point is
`harness/match-tick-fixture.mjs --cmb42-ordinary-only`. It exits before the old
connected corpus or its loader exports. The only permitted extra flags are
`--check` and one `--cmb42-output=/absolute/private/directory`. No default output
touches a fixture, and no Source branch has run. The two scene names are:

- `ordinary-sniper-12-30-distance-and-recipient`
- `ordinary-at-7-28-researched-built-producers`

The actual caller path is clean pinned e4e579a -> unchanged createGame
(shared/sim.js:682) -> real command (:3408) -> Buy (:3781) ->
productionBuildings (:388) -> produceUnit (:1071) -> full step (:5305).
The AT prerequisite path is real tech command (:3723), stepLab on ordinary
ticks (:5323), real Build (:3689) and naturally completed paid motor pools.
The recorder binds actual new IDs by set difference and checks completed
owned producers before each purchase. Optional `from` is preserved. There is
no fixtureCommand temporary producer, clearFixtureUnits, sandbox spawn, actor
deletion or constructor replacement. Geometry is reached with ordinary Move.

Both scenes retain all ten constructor actors. They declare MP=5000 and one
initial cooldown=1e9 on the actual purchased shooter, as the named original
helper does, with before/after input receipts. No other actor field is set.
Queued commands carry owner/sequence/next tick; the recorder consumes them in
that order immediately before the real step. Every command has its actual
return, actor births, wallet/story debit and random ranges recorded. Every
contiguous tick records all actual actor fields, players, story, points,
environment seed/wind, projectiles, relevant structure state and both existing
recipient views. Raw encoding retains undefined properties, f64 bits, Map/Set
order and typed-array kinds; it does not apply the approved defaults mapping.

The real Source visibility pass and existing recipient consumer supply all
visibility and target rows. A read-only loader export observes actual canShoot
(:3819). A second recorder-only wrapper calls the unchanged private rng body
(:2345) exactly once and records the actual returned bits and seed before/after.
The recorded global provider begins before each constructor and stays continuous
through all commands and ticks. Its raw consumption receipt includes constructor
draws; neither global nor environment state is reset during a scene. Actual
passive steps must consume no global combat draws or add firing effects, and
all retained actor identity/HP checks run at each tick. Source and native RNG
startup alignment remains a separate comparison gap, not an assumed result.

The sniper keeps literal 12/30 m, then moves the same MG to 40 m so the rifle
must win, then to 60 m where the enemy row must be absent from recipient 0.
The AT keeps literal 7/28 m, real stationary setup and a tank front presented
through a single-unit facing Move. These retained ordinary setups differ from
the original test.js scenes that clear armies and set poses. The original
12/30 and 7/28 windows, the old component's 12/28 geometry and native draft
`7aab8df4cbb6fd286290bbb5c8a30b6317366a34` remain explicit, separate evidence.

For a future exact-head grant only, the bounded command shape is:

```sh
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts \
  node harness/match-tick-fixture.mjs --cmb42-ordinary-only \
  --cmb42-output=/tmp/ROOT_GRANTED_NEW_CAPTURE_DIRECTORY
```

Use x64 Node v24.21.0 and a new existing private directory selected in that
grant. The successful file is `scenes.json`, created exclusively after both
scenes complete. On the first genuine scene failure, `first-failure.json`
preserves partial records, actual random reads and any completed earlier scene,
then the command throws. It refuses an existing failure packet or capture;
there is no automatic repair, retry or old-scope fallback. A later `--check`
would execute both scenes again and needs its own authorization.

Remaining proof gaps are concrete: this recorder has not executed or produced
a packet; the two native names have never compiled or run; no Rust reader yet
compares the proposed tagged packet with native 7aab fields or reconciles every
constructor/random input. The authored native assertions remain ordinary-rule
witnesses, not compiled results or a Source replay. No CMB42 status changes.

## Remaining command binding and provider assessment

Both authored witnesses now send purchases through parsed JSON, rather than
calling recorded_buy directly. The exact path is Command::from_json
(match_commands.rs:551) -> execute_command Buy branch (:903) -> recorded_buy
(match_recruitment.rs:232) -> recruit stage -> prepare_recruit
(production.rs:564) -> ordinary paid placement/publication. The helper checks
the parsed kind and optional producer, the dispatch result and exactly one
observed new actor, without predicting or setting its ID. It requires every
previous actor remain and the observed recruit's producer existed before the
command, is still present/living/completed/owned, and has real production state.
Explicit AT/tank producer IDs must survive this path. Exact MP debit and story
spend are checked before any intervening tick. This matches the original named
check's immediate command style while avoiding its temporary fixture producer.

The native constructor provides real HQ/barracks. Sniper/MG tier-1 admission
uses them directly; AT/tank use the already-authored real tier-2 research and
completed motor pools. The parsed command path does not bypass winner/away/out,
faction, tier, money, population, cap or producer checks. No missing provider
was identified for these bounded purchases by source inspection. Build uses
execute_build (match_construction.rs:764), and the current ordinary tick invokes
Research before unit turns and the actual construction finish after Firing.

The unrun passive helper previously required Vision on every tick. Current
match_tick.rs:472-482 refreshes it only when tick % 4 == 1. The helper now
asserts exact phase presence or absence on that cadence and requires Research
each tick. It retains its original command receipts, Firing, no firing events,
no retirement and unchanged actor/Combat assertions. No runtime schedule or
guard changed, and no native failure or retry is claimed for this source fix.

The existing firing turn (match_firing.rs:1398 onward) admits the represented
ground sniper/AT inputs and rejects air, rail, operative, grenade/terrain and
beam cases outside this witness. The actual target selector (:1803) uses live
visible actors, can_shoot_target and role weighting. Ordinary Hold Fire clears
selection, so the original named one-time cooldown input remains necessary for
passive automatic selection. Move/stance/actual ticks and SnapshotCache recipient
rows supply the remaining consumer path, with no target/visibility import.

These are inspected provider paths, not compiled results. Both named witnesses,
their full retained command sequence and the helper corrections still require
separate exact-head native qualification. Neither scene has a newly recorded
contiguous Source trace. The existing pure helper/AT component and literal
original named inputs remain qualified separately; none of this moves CMB42.

## Continued AT consumer after restart

The recovered separate branch now also authors
`match_tick::cmb42::cmb42_named_at_prefers_tank_after_real_research_construction_and_purchases`.
This is unrun code. It does not enter PR269 or any earlier execution grant.
The new prerequisites use these actual existing callers:

1. Owner enqueue_command with parsed `tech/tier` -> execute_command ->
   request_research -> prepare_research -> canonical early Research phase.
   Both actual HQ jobs must finish at tier 2 within 1600 real ticks, with
   exact research MP spent and no remaining jobs. No tier/timer setter.
2. Owner Build commands for motorpool on each owner's visible ground ->
   execute_build -> paid site publication -> ordinary builder movement ->
   step_construction_worker and finish_construction_tick. Both actual site
   IDs must finish within 2400 real ticks, with exact site MP spent. No
   temporary producer or built/HP assignment in the test.
3. recorded_buy AT/tank from those actual completed producer IDs, plus a real
   rifle. Require distinct native births, ownership/kind, recruitment metadata,
   exact payment and the actual producer IDs on AT/tank metadata.
4. Owner Move stages AT (60,80), rifle (67,80), tank (88,80), preserving the
   named 7/28 m distances. A separate single-tank facing Move uses core atan2
   for pi, so it does not rearrange a multi-unit formation. Verify the actual
   front-facing relation and weapon setup after ordinary stationary ticks.
5. Actual Stance off enables automatic AT search for 20 ticks with both
   candidates visible/shootable, no ordered target, selected tank, recipient
   row target equal to that tank, surviving rifle and no firing/HP/RNG change.

Both constructor armies remain. Setup preserves their actual identities and
HP bits on every tick; new sites acquire their HP naturally through construction.
After actual purchases the complete current actor set must keep its identity,
HP, count and Combat-stream state at every tick. The existing sniper helper
retains its strict original invariant. A single initial AT cooldown=1e9 is the
original named Source helper input, not a later reset. Denial, path failure,
unmet construction/research bound, unwanted damage or wrong selection remains
an actual future failure, with no fallback or automatic input repair.

The native compile/run/format/lint gap remains for both named witnesses. No
new contiguous Source capture exists for either retained command-staged scene.
The original 7/28 m and 12/30 m inputs and old component captures stay distinct.
Root must separately review/grant any future two-name CMB42 qualification.
All older checkpoints, PR269's six completed proofs and its eight-commit rebase
packet remain separate. No older Source/native scope is rerun here.

The original private source-only child starts at
`e80d369d495c893436740db58155571a9484594a` and is preserved as
`eb4e97b35c954cc85f4dc2ddda5f1d696c8da512`. The new
`audit/combat-cmb42-invariants-3fa592e8` branch continues only that child.
The original six-instance checkout now preserves the separately reported
`daaa4743` partial run and `ccad825a` failure report. This child has one
additional native instance:
`match_tick::cmb42::cmb42_named_sniper_prefers_mg_through_real_buy_move_tick_and_owner_view`.
It is the sixth new behavior test in the restored branch, and would be the
seventh affected instance if Root later grants the combined child. It is not
silently added to e80d's queued scope.

## Inspected original and actual callers

At reference e4e579a, test.js:2596 defines its local holdFire helper as a one-time
cooldown=1e9 input. It does not set the real Hold Fire stance. The complete
Targets by job block at 2603-2609 purchases a sniper, rifle and MG, holds them
passive, then runs 20 actual ticks and requires a 30 m MG preference over a 12 m
rifle plus surviving rifle HP. The second named AT check is 2620-2628 and is
represented by the older actual AT choice-role component replay. Its authored
rifle/tank distances are 12/28 m, whereas the named AT scene uses 7/28 m.
Do not label the old component as an exact replay of that full named scene.
Complete CMB-42
needs both, not just the AT half or a translated role formula.

The original named purchases use test-fixtures.js fixtureCommand, imported as
command by test.js. That helper temporarily inserts a finished producer if the
real producer is absent, calls the real command and then removes the producer.
Its clearFixtureUnits also removes the original starting footprints. These are
original combat-fixture inputs, not ordinary retained native startup. Do not
silently transfer that temporary producer technique into the genuine native
purchase witness.

The compiled TIER table requires 1 for sniper/MG/Conscripts, but 2 for AT/tank.
The compiled maker table assigns sniper/MG to barracks and AT/tank to motorpool.
The native constructor places HQ and barracks, not a motor pool.
recorded_buy -> recruit stage -> ProductionState::prepare_recruit checks actual
faction, tier, currencies, population, caps and completed owned producer before
publishing a paid birth. A naive extension of the retained sniper test with
recorded_buy("at")/recorded_buy("tank") would therefore lack the authored
prerequisites. Complete named AT coverage needs an ordinary tier-research and
motor-pool construction setup, or a separately approved authentic existing
complete captured setup. No new runtime provider defect is demonstrated by
those admission requirements. No fake producer, tier rewrite, ammo/HP/type/pose
mutation or guard relaxation is proposed.

Current match_firing.rs consumes actual visible actors and the pure role
multiplier in pick_firing_target. A real Hold Fire stance clears target_id
instead of searching. This is existing supported behavior, not a defect to
remove. Ground sniper, rifle and MG pass the inspected firing-kind boundary;
recorded_buy, recorded commands, actual advance_tick and SnapshotCache's
recipient projection are existing consumers. No missing runtime provider is
identified for these three kinds. That is a source finding, not execution proof.

## Authored native witness

Use the existing authentic retained arena map and ordinary canonical constructor
with both enemy seats and all ten starting actors. Fund the wallets as the named
Source check does. Buy one actual sniper for seat 0 and actual rifle/MG for seat
1 through recorded_buy, asserting distinct published IDs, kinds, ownership,
recruitment metadata and exact payment. Never fabricate an actor or write its
HP/type/pose or any RNG state.

The sniper gets the original named Source helper's one-time cooldown=1e9 input
immediately after purchase, before the first tick. It is not pulsed/reset later.
All stances, autocast changes and movement use the existing recorded queue with
actual owner keys. A bounded actual-tick staging pass must move the actors to
(60,80), (72,80), (90,80); there is no pose assignment or arrival fallback.
This translates the original 12/30 m geometry without removing the constructor.
Fail if the ordinary moves do not arrive within 1200 ticks. Require 20 genuine
stationary ticks for setup/vision. Every queued command must receive a real
non-denied receipt at the expected next boundary.

Enable the sniper's automatic search through its ordinary Stance command.
For 20 actual ticks require no ordered attack, selected MG, surviving rifle,
actual visible/shootable candidates and owner recipient row target_id=MG.
Move the same MG to 40 m with its owner's ordinary Move, clear the sniper's
previous target through a real held tick, then repeat 20 search ticks requiring
the nearer rifle instead. Both candidates are still visible/shootable; this
control rejects a selector that simply always chooses the MG. Finally move the
MG to 60 m through ordinary owner commands and require actual visibility loss,
can_shoot_target=false, absent MG row in seat 0's recipient projection and zero
selected-target reference during real Hold Fire. No visibility set, opponent
view, shot, health, type, target choice or random value is imported or forged.
All constructor actors and all actual HP bits must remain intact, and the
Combat random stream must stay at its observed pre-run state. The successor
checks actor count, identity, HP bits, Combat stream state and absence of firing
events after every actual tick, including movement and held intervals. It also
rechecks both candidates' real visibility and shootability throughout both
20-tick selection intervals. These are invariants over observed native actors,
not expected-state imports or state restoration.

SnapshotCache is the existing bounded Rust recipient projection. The test does
not claim a complete transport/private-view implementation or change its
policy. Own commands use declared map coordinates and owned unit IDs. No new
wire, protocol, ABI, terrain or gameplay interface is proposed.

## Exact remaining qualification

This is authored native code, not a PASS. No Cargo, Node, rustc, native, formatter,
linter, discovery, full/release or CI command has run. No new Source capture is
present for this retained-constructor command-staged variant. The original
named Source assertion and inputs were read directly, and the old complete
role helper/AT fixture remain unchanged; those do not fabricate a new matched
capture. If Root requires a new exact Source trace for this variant, that is
the genuine capture gap and requires a separate bounded Source grant. Do not
run an old recorder or reset a seed to supply it without that grant.

CMB-42 stays todo. The first-five status patch is unchanged. The failed six-test
batch stopped after CMB-64 PASS and CMB-65 FAIL at `daaa4743`, leaving the other
four selected instances unrun. No retry is authorized. This separate sniper
successor needs its own exact-head review/grant or a precise requested capture.
No runtime/guard repair or row promotion is justified by this preparation.

The exact original AT inputs are shooter (5,21), rifle (12,21), tank (33,21),
with the tank's original front-facing rotation pi, one initial cooldown=1e9 on
the AT and passive enemy timers, followed by 20 full Source ticks. A new bounded
capture must retain that 7/28 m distinction and disclose those original fixture
helpers. The existing choice-role 12/28 m capture stays exact and separately
qualified. The sniper variant remains the retained genuine-purchase consumer
described above; neither its missing contiguous capture nor the AT setup gap
can be filled by a formula translation or a registry-only run.
