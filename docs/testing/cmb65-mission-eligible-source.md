# CMB-65 mission-eligible fighter correction, source only

This new private child starts at preserved `ccad825a38d12ab6aa57c988869f78e84297746a`.
Original `e80d369d`, producing `daaa4743`, docs `ccad825a`, the separate `eb4e97b3`
branch and `/tmp/audit-combat-e80-S2-3fa592e8/` raw logs remain intact. The failed
CMB-65 result is not amended or relabeled. No execution, Source capture, formatter,
linter, discovery, registry test, full/release suite or CI is authorized here.

## Concrete input correction

The failed test selected `fighter-continuous-aa`, whose authentic input explicitly
has air.ammo=0. That fixture proves a different bounded fact: continuous AA does
not depend on aircraft weapon ammo. The later new Attack mission must still
satisfy ordinary fuel/ammo admission. Its false result on the failed producing
head is correct for those input values.

Select the unchanged `held-fighter-attack-air-target-loss` trace for this new
fighter transition instead. Replay its complete captured initialization through
the maintained scene factory and compare the full initial snapshot. Its captured
fighter 1000 is held at Base with fuel bits 4049000000000000 (50) and ammo bits
404e000000000000 (60), and its actual enemy bomber 1001 is airborne at Station.
These values originate from the real aircraft initializer and existing captured
scene. The correction does not assign ammo, type, HP, pose, visibility, mission,
state or any RNG value. Existing scene-input replay stays unchanged; no new
patch fields are added to the fixture or native helper.

The existing captured first action sends that same fighter an Attack mission on
1001 and records admitted=true. It supplies direct evidence that this initial
actor is mission-eligible under the original provider, unlike the zero-ammo AA
case. Initial/full captured frames and the ordinary caller remain distinct
claims. The old zero-ammo trace and all 42 maintained aviation traces stay exact.

## Full proposed caller sequence

The existing focused function retains its name:
`match_aviation::cmb65_mg_flak_and_fighter_hold_then_resume_permitted_anti_air`
in package ww2-simtest, consolidated target it. No new registry entry is needed.

1. Reuse original mg/flak continuous-AA traces unchanged. For the fighter, load
   the full ready-fighter trace and require full initial snapshot equality.
2. Parse and dispatch the actual Stance command enabling Hold Fire. For the
   fighter, require Base, positive ammo/fuel and an actually visible target.
3. Dispatch ordinary Move with the visible target's current coordinates. The
   aircraft Move branch prepares a Patrol mission, validates admission and
   publishes Out state. Require the actual Patrol/None-target metadata and Out.
4. Call the existing step_aircraft provider once at .05 s. Require the fighter
   remains actually airborne, held, and strictly within its 30 m AA range of the
   live airborne visible bomber. This prevents a parked/out-of-range no-damage
   interval from standing in for real Hold Fire.
5. Retain all six held .05 s AA calls and their exact unchanged target HP,
   shooter XP and empty event assertions for all three kinds.
6. For the fighter, parse and dispatch ordinary Attack on the visible airborne
   target. Require no denial, actual Attack metadata, target 1001, Out state,
   unchanged positive ammo and retained Hold Fire. This preserves the mission
   admission obligation through its ordinary command caller, with stronger
   committed-state checks than a silent no-denial receipt alone. Ground units
   retain their actual Hold Fire off Stance command.
7. Retain all six permitted .05 s AA calls and require actual target damage,
   shooter XP gain and an actual AA event for every kind. The held fighter can
   attack only its explicit target, through the existing AA rule.

The command path is Command::from_json -> Match::execute_command ->
prepare_aircraft_order -> validate_aircraft_mission -> apply_aircraft_order.
The latter publishes mission and Out only when prepared.send is true. The
physical phase is step_aircraft -> preview_aircraft_step. AA uses
step_aircraft_anti_air -> prepare_aircraft_anti_air and its canonical commit.
No guard is removed, bypassed or relaxed. No result is counted as a PASS until
an explicitly granted native run completes.

## Ordinary rearm availability

An alternative ordinary provider exists, but the correction does not synthesize
a refill or claim to run a new rearm sequence. In the unchanged
`fighter-rearm-denial` capture, the fighter starts in Rearm with ammo 0 and timer
.1. An immediate Patrol request is refused. Four recorded .05 s stepPlane calls
finish at Base with ammo 60 and fuel 50. The existing native consumer calls
step_aircraft for those same frames. Its rearm branch changes health, timer,
fuel and ammo naturally, and mission admission refuses until Rearm finishes.
The existing `fighter-fuel-home-rearm` capture also contains 750 actual provider
calls through return and rearming. These are old captured facts and old main
component qualifications, not new executions here. The selected ready-fighter
initialization makes a new refill sequence unnecessary for this focused check.

## Exact captured and source gaps

Already captured: full initial ready-fighter state; admitted original Attack;
ordinary rearm refusal/completion; airborne held explicit-target AA effects.
The older aircraft command fixture separately records ordinary command
selection/cleanup. These source/compiled records retain their producing heads.

Not captured as one new contiguous Source trace: Stance on, Patrol Move, one
physical aircraft step, six held AA calls, ordinary Attack and six permitted AA
calls over this ready-fighter initial scene. The new code composes existing
captured providers and asserts the complete selected transition natively; it
is not a fresh exact Source replay or whole-match proof. No new recorder result
exists. If Root requires a matched capture of this composition, a bounded
new-only recorder path and explicit Source grant are still needed. The current
full aviation recorder would rerun the complete 42-trace corpus, so it must not
be run automatically to fill that gap.

Native compile/run, formatting, strict Clippy and independent actual-head review
are also pending. Preserve source-exact CMB-39/CMB-69/connected/CMB-41 as unrun.
Their code and inputs are unchanged by this child. CMB-64's previous PASS remains
on daaa4743; no rerun or new-head claim is made. CMB-42 remains on its separate
branch. No actual status cell, private status patch, fixture or runtime file
changes. Root owns the successor scope, review, execution grant and publication.
